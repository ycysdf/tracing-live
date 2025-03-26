use crate::RECORD_ID_GENERATOR;
use crate::global_data::GLOBAL_DATA;
use crate::record::{AppRunInfo, SpanCacheId, SpanId, TracingKind, TracingRecordVariant};
use crate::related_event::{
    ErrSpanRelatedEvent, ReturnSpanRelatedEvent, SpanRelatedEvent, TowerHttpSpanRelatedEvent,
};
use crate::running_app::{AppRunMsg, AppRunRecord, CreatedSpan, EnteredSpan, RunMsg};
use crate::tracing_service::{TracingLevel, TracingRecordBatchInserter};
use anyhow::Context;
use bitflags::bitflags;
use bon::bon;
use chrono::{DateTime, FixedOffset, Local, Utc};
use derive_more::{Constructor, Deref, DerefMut, From};
use entity::app::Entity;
use flume::r#async::RecvStream;
use futures_util::StreamExt;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use smallvec::SmallVec;
use smol_str::{SmolStr, ToSmolStr, format_smolstr};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};
use tracing::field::debug;
use tracing::{Instrument, Span, error, info, info_span, instrument, warn};
use tracing_lv_core::proto::{AppStartInfo, TLValue};
use tracing_lv_core::{FLAGS_AUTO_EXPAND, FLAGS_FORK, TLMsg, proto};
use uuid::Uuid;

#[tonic::async_trait]
impl TracingService for TracingServiceImpl {
    type AppRunStream = RecvStream<'static, Result<AppRunReplay, Status>>;

    async fn app_run(
        &self,
        request: Request<Streaming<RecordParam>>,
    ) -> Result<Response<Self::AppRunStream>, Status> {
        let mut streaming = request.into_inner();
        let record = streaming
            .next()
            .await
            .ok_or_else(|| Status::unavailable("empty message"))??;
        let record_param::Variant::AppStart(app_start) = record.variant.unwrap() else {
            panic!("must first app start")
        };
        let half_rtt = app_start.rtt / 2.0;
        let span = info_span!(parent: self.span.id(),"app lifetime",?app_start);
        let (app_run_record_sender, app_run_record_receiver) = flume::unbounded();
        let (mut app_run_lifetime, app_run_record) = AppRunLifetime::new(
            app_start,
            self.tracing_service.clone(),
            app_run_record_sender,
        )
        .instrument(info_span!(parent:span.id(),"start",app_run_info="",record=""))
        .await?;

        {
            let now = Utc::now();
            let delta_date_nanos = now.timestamp_nanos_opt().unwrap()
                - (half_rtt * 1_000_000_000.0) as i64
                - record.send_time;

            GLOBAL_DATA.add_running_app(app_run_lifetime.app_run_info.run_id, delta_date_nanos);
        }

        // TODO:
        self.record_sender
            .send(RunMsg::AppRun {
                record_sender: app_run_lifetime.record_sender.clone(),
                app_run_record,
                record_receiver: app_run_record_receiver,
            })
            .map_err(|_| Status::unavailable("send app run message failed"))?;

        let record_sender = self.record_sender.clone();
        // Tonic Bug: ?. Future is sometimes canceled
        let (reply_sender, reply_receiver) = flume::unbounded();
        if app_run_lifetime.app_start.reconnect {
            let last_record_index = self
                .tracing_service
                .query_app_run_last_record_index(app_run_lifetime.app_run_info.run_id)
                .await
                .map_err(|err| {
                    Status::internal(format!(
                        "query app run last record index error. err: {err:?}"
                    ))
                })?
                .ok_or_else(|| Status::internal("not found app run record"))?;
            reply_sender
                .send(Ok(AppRunReplay {
                    send_time: Utc::now().timestamp_nanos_opt().unwrap(),
                    variant: Some(proto::app_run_replay::Variant::ReconnectReply(
                        AppReconnectReply {
                            last_record_index: last_record_index as u64,
                        },
                    )),
                }))
                .map_err(|_| Status::unavailable("send reconnect reply message failed"))?;
        }
        let _ = tokio::spawn(
            async move {
                let mut last_record_index = 0;
                let result = async {
                    let mut error_count = 0;
                    while let Some(result) = streaming.next().await {
                        match result {
                            Ok(RecordParam {
                                send_time,
                                variant,
                                record_index,
                            }) => {
                                let record_index = record_index as i64;
                                // println!(
                                //     "{:?} .record_index: {record_index}",
                                //     app_run_lifetime.app_run_info.run_id
                                // );
                                error_count = 0;
                                let variant = variant.unwrap();
                                let variant =
                                    if let record_param::Variant::AppStop(app_stop) = variant {
                                        info!("app stop");
                                        return Ok(Some((app_stop, send_time)));
                                    } else {
                                        last_record_index = record_index;
                                        app_run_lifetime.record(variant).await?
                                    };
                                if let Err(err) = app_run_lifetime.record_sender.send(
                                    AppRunMsg::Record(AppRunRecord {
                                        id: RECORD_ID_GENERATOR.next(),
                                        record_index,
                                        variant,
                                    }),
                                ) {
                                    info!(?err, "record_sender send failed. exit!");
                                    break;
                                }
                            }
                            Err(err) => {
                                error_count += 1;
                                warn!(error_count, "{err:?}");
                                if error_count > 3 {
                                    return Err(err);
                                }
                            }
                        }
                    }
                    Ok::<_, Status>(None)
                }
                .await;
                let app_stop = result.unwrap_or_else(|err| {
                    error!("{err}");
                    None
                });
                let normal_stop = match app_stop {
                    None => {
                        warn!(run_info=?app_run_lifetime.app_run_info,"app exception_end");
                        None
                    }
                    Some((_app_stop, send_time)) => Some(DateTime::from_timestamp_nanos(send_time)),
                };
                let app_run_record = app_run_lifetime
                    .app_stop(normal_stop, last_record_index + 1)
                    .await
                    .inspect_err(|err| {
                        error!("app_stop error: {err}");
                    })
                    .unwrap();
                record_sender
                    .send(RunMsg::AppStop { app_run_record })
                    .inspect_err(|err| error!("send app stop message failed. {err:?}"))
                    .unwrap();

                info!("app lifetime end");
                drop(reply_sender);
            }
            .instrument(span),
        );
        Ok(Response::new(reply_receiver.into_stream()))
    }

    async fn ping(&self, _request: Request<PingParam>) -> Result<Response<PingResult>, Status> {
        Ok(Response::new(PingResult {}))
    }
}
