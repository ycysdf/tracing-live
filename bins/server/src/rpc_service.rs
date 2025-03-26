use crate::RECORD_ID_GENERATOR;
use crate::global_data::GLOBAL_DATA;
use crate::record::{
    AppRunInfo, EventRecordItem, SmolStrExt, SpanCacheId, SpanId, SpanRecordItem, TLSpanInfo,
    TraceId, TracingRecordVariant, TracingSpanInfo,
};
use crate::related_event::{
    ErrSpanRelatedEvent, ReturnSpanRelatedEvent, SpanRelatedEvent, TowerHttpSpanRelatedEvent,
};
use crate::running_app::{AppRunMsg, AppRunRecord, CreatedSpan, EnteredSpan, RunMsg};
use crate::tracing_service::TracingLevel;
use anyhow::Context;
use bitflags::bitflags;
use bon::bon;
use chrono::{DateTime, Utc};
use derive_more::{Constructor, Deref, DerefMut};
use futures::Stream;
use futures_util::StreamExt;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait};
use serde_json::Value;
use smallvec::SmallVec;
use smol_str::{SmolStr, format_smolstr};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use tracing::field::debug;
use tracing::{Instrument, Span, error, info, info_span, warn};
use tracing_lv_core::proto::{
    AppStartInfo, TLRecordVariant, TLValue, TracingRecordItem, TracingReplyVariant, TracingService,
};
use tracing_lv_core::{
    FIELD_DATA_EMPTY_CHILDREN, FIELD_DATA_FLAGS, FIELD_DATA_IS_CONTAINS_RELATED,
    FIELD_DATA_RELATED_NAME, FIELD_DATA_SPAN_T_ID, FIELD_DATA_STABLE_SPAN_ID, FLAGS_AUTO_EXPAND,
    FLAGS_FORK, SpanRawInfo, TLMsg, TracingFields,
};
use uuid::Uuid;
use xy_rpc::maybe_send::MaybeSend;
use xy_rpc::{RpcError, stream_with_sender};

pub struct TracingServiceImpl {
    pub tracing_service: crate::tracing_service::TracingService,
    pub record_sender: flume::Sender<RunMsg>,
    pub span: Span,
}

impl TracingServiceImpl {
    pub fn new() {}
}

impl TracingService for TracingServiceImpl {
    fn app_run(
        &self,
        info: AppStartInfo,
        mut streaming: xy_rpc::TransStream<TracingRecordItem, impl xy_rpc::formats::SerdeFormat>,
    ) -> impl Future<Output = impl Stream<Item = Result<TracingReplyVariant, RpcError>> + 'static>
    + MaybeSend {
        let tracing_service = self.tracing_service.clone();
        let record_sender = self.record_sender.clone();
        let span = self.span.clone();
        async move {
            stream_with_sender(move |reply_sender| {
                let span = info_span!(parent: span.id(),"app lifetime",?info);
                let delta_date_nanos = {
                    let half_rtt = info.rtt / 2;
                    let now = Utc::now();
                    now.timestamp_nanos_opt().unwrap()
                        - (half_rtt.num_milliseconds() * 1_000_000.0 as i64)
                        - info.send_time.timestamp_nanos_opt().unwrap()
                };
                let span_id = span.id();
                let fut = async move {
                    let (app_run_record_sender, app_run_record_receiver) = flume::unbounded();
                    let (mut app_run_lifetime, app_run_record) = AppRunLifetime::new(
                        info,
                        tracing_service.clone(),
                        app_run_record_sender,
                    )
                    .instrument(info_span!(parent:span_id,"start",app_run_info="",record=""))
                    .await?;

                    {
                        GLOBAL_DATA.add_running_app(
                            app_run_lifetime.app_run_info.run_id,
                            delta_date_nanos,
                        );
                    }

                    // TODO:
                    record_sender
                        .send(RunMsg::AppRun {
                            record_sender: app_run_lifetime.record_sender.clone(),
                            app_run_record,
                            record_receiver: app_run_record_receiver,
                        })
                        .context("send app run message failed")?;

                    if app_run_lifetime.app_start.reconnect {
                        let last_record_index = tracing_service
                            .query_app_run_last_record_index(app_run_lifetime.app_run_info.run_id)
                            .await
                            .context("query app run last record index error. err: {err:?}")?
                            .context("not found app run record")?;
                        reply_sender
                            .send(Ok(TracingReplyVariant::AppReconnectReply {
                                last_record_index: last_record_index as u64,
                            }))
                            .context("send reconnect reply message failed")?;
                    }

                    let mut last_record_index = 0;
                    let result = async {
                        while let Some(result) = streaming.next().await {
                            let TracingRecordItem { send_time, variant } = result?;

                            let record_index = variant.record_index() as i64;
                            last_record_index = record_index;
                            let variant = match variant {
                                TLRecordVariant::TLMsg(msg) => app_run_lifetime.record(msg).await?,
                                TLRecordVariant::AppStop { exception_end, .. } => {
                                    return Ok(Some((send_time, exception_end)));
                                }
                            };
                            // TracingRecordVariant::AppStop {}
                            // println!(
                            //     "{:?} .record_index: {record_index}",
                            //     app_run_lifetime.app_run_info.run_id
                            // );
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
                        info!("app stop");
                        return anyhow::Ok(None);
                    }
                    .await;
                    let exception_stop = match result {
                        Err(err) => {
                            warn!(?err,run_info=?app_run_lifetime.app_run_info,"app exception_end");
                            Some(Utc::now())
                        }
                        Ok(Some((date, exception_end))) => exception_end.then_some(date),
                        Ok(None) => None,
                    };
                    let app_run_record = app_run_lifetime
                        .app_stop(exception_stop, last_record_index + 1)
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
                    anyhow::Ok(())
                }
                .instrument(span);
                async move { fut.await.unwrap() }
            })
        }
    }

    fn ping(&self) -> impl Future<Output = ()> + MaybeSend {
        async move {}
    }
}

#[derive(Clone, Debug, Deref)]
pub struct SpanFullInfoBase {
    pub record_time: DateTime<Utc>,
    pub app_info: Arc<AppRunInfo>,
    #[deref]
    pub span_info: Arc<SpanInfo>,
    pub running_span: RunningSpan,
}
#[derive(Clone, Debug, Deref)]
pub struct SpanFullInfo {
    #[deref]
    pub base: Arc<SpanFullInfoBase>,
    pub fields: TracingFields,
}

#[derive(Clone, Debug, Deref)]
pub struct SpanInfo {
    pub module_path: SmolStr,
    pub t_id: u64,
    #[deref]
    pub cache_id: SpanCacheId,
}

#[derive(Clone, Debug)]
pub struct RunningSpan {
    pub info: Arc<SpanInfo>,
    pub parent_span_t_id: Option<u64>,
    pub id: SpanId,
    pub parent: Option<SpanId>,
    pub target: Option<SmolStr>,
    pub level: Option<TracingLevel>,
}

bitflags! {
    #[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Hash)]
    pub struct TracingRecordFlags: u64 {
        const RESERVE = 1;
        const AUTO_EXPAND = 1 << 1;
        const FORK = 1 << 2;
    }
}

pub struct AppRunLifetime {
    tracing_service: crate::tracing_service::TracingService,
    #[allow(dead_code)]
    pub record_sender: flume::Sender<AppRunMsg>,
    pub app_run_info: Arc<AppRunInfo>,
    pub app_start: AppStartInfo,
    pub span_id_cache: lru::LruCache<SpanCacheId, SpanId>,
    // pub running_spans: hashbrown::HashMap<u64, RunningSpan>,
}

impl AppRunLifetime {
    pub async fn get_span_info(
        &mut self,
        span_info: SpanRawInfo,
        fields: Option<&TracingFields>,
        parent_span: Option<&SpanRawInfo>,
    ) -> anyhow::Result<TracingSpanInfo> {
        let trace_id = span_info.id;
        let tl_base = Arc::new(span_info.into());
        let parent_tl_base: Option<TLSpanInfo> = parent_span.map(|n| n.clone().into());
        Ok(TracingSpanInfo {
            trace_id,
            parent_trace_id: parent_span.as_ref().map(|n| n.id),
            parent_id: match parent_tl_base {
                None => None,
                Some(parent_span) => Some(self.get_span_id(&parent_span, None).await?),
            },
            id: self.get_span_id(&tl_base, fields).await?,
            tl_base,
        })
    }
    pub async fn get_span_id(
        &mut self,
        span_info: &TLSpanInfo,
        fields: Option<&TracingFields>,
    ) -> anyhow::Result<SpanId> {
        let cache_id = SpanCacheId {
            app_id: self.app_start.id,
            app_version: self.app_start.version.clone(),
            name: span_info.name.clone(),
            file_line: span_info.file_line(),
        };
        Ok(match fields.and_then(|n| n.stable_span_id()) {
            None => self.get_span_id_by_cahce_id(&cache_id).await?,
            Some(n) => Uuid::from_str(n).context("invalid sid")?,
        })
    }
    pub async fn record(&mut self, msg: TLMsg) -> anyhow::Result<TracingRecordVariant> {
        Ok(match msg {
            TLMsg::SpanCreate {
                record_index,
                record_date,
                mut fields,
                parent_span,
                span,
            } => {
                let span = self
                    .get_span_info(span, Some(&fields), parent_span.as_ref())
                    .await?;
                fields.insert_empty_children();
                fields.insert_span_trace_id(Some(span.trace_id));
                let mut span_item = SpanRecordItem {
                    record_index,
                    span,
                    fields,
                    record_date,
                    span_parent: match parent_span {
                        None => None,
                        Some(span) => Some(self.get_span_info(span, None, None).await?),
                    },
                };

                // {
                //     self.running_spans.insert(
                //         full_info.t_id,
                //         RunningSpan {
                //             info: full_info.span_info.clone(),
                //             parent_span_t_id: full_info.running_span.parent_span_t_id,
                //             id: full_info.running_span.id,
                //             parent: full_info.running_span.parent,
                //             target: Some(target),
                //             level: Some(level.into()),
                //         },
                //     );
                // }

                TracingRecordVariant::SpanCreate { span_item }
            }
            TLMsg::SpanEnter {
                record_index,
                record_date,
                span,
                parent_span,
            } => {
                // pub parent_span_t_id: Option<u64>,
                // pub id: SpanId,
                // pub parent: Option<SpanId>,
                let span = self.get_span_info(span, None, parent_span.as_ref()).await?;
                TracingRecordVariant::SpanEnter {
                    span,
                    record_date,
                    record_index,
                }
            }
            TLMsg::SpanLeave {
                record_index,
                record_date,
                span,
                parent_span,
            } => {
                let span = self.get_span_info(span, None, parent_span.as_ref()).await?;
                TracingRecordVariant::SpanLeave {
                    span,
                    record_date,
                    record_index,
                }
            }
            TLMsg::SpanClose {
                record_index,
                record_date,
                span,
                parent_span,
            } => {
                let span = self.get_span_info(span, None, parent_span.as_ref()).await?;
                TracingRecordVariant::SpanClose {
                    span,
                    record_index,
                    record_date,
                }
            }
            TLMsg::SpanRecordField {
                record_index,
                record_date,
                span,
                fields,
                parent_span,
            } => {
                let span = self.get_span_info(span, None, parent_span.as_ref()).await?;
                TracingRecordVariant::SpanRecord {
                    span,
                    fields,
                    record_index,
                    record_date,
                }
            }
            TLMsg::Event {
                record_index,
                record_date,
                mut message,
                metadata,
                mut fields,
                span,
                parent_span,
            } => {
                let mut event_item = EventRecordItem {
                    fields,
                    record_date,
                    message,
                    span: match span {
                        None => None,
                        Some(span) => {
                            Some(self.get_span_info(span, None, parent_span.as_ref()).await?)
                        }
                    },
                    target: metadata.target.into_smol_str(),
                    level: metadata
                        .level
                        .as_ref()
                        .try_into()
                        .ok()
                        .context("invalid level")?,
                    module_path: metadata
                        .module_path
                        .map(|n| n.into_smol_str())
                        .unwrap_or(SmolStr::new_static("<UNKNOWN>")),
                    file: metadata.file.map(|n| n.into_smol_str()),
                    line: metadata.line,
                    record_index,
                    is_related_event: false,
                    is_repeated_event: false,
                };

                let is_related_event = {
                    let span_related_event_objs: &[&dyn SpanRelatedEvent] = &[
                        &ReturnSpanRelatedEvent {},
                        &ErrSpanRelatedEvent {},
                        &TowerHttpSpanRelatedEvent {},
                    ];
                    if event_item.span.is_some() {
                        span_related_event_objs.iter().any(|n| {
                            match n.is_related_and_handle(&self.app_run_info, &mut event_item) {
                                None => false,
                                Some(name) => {
                                    event_item
                                        .fields
                                        .insert_field(FIELD_DATA_RELATED_NAME, name);
                                    true
                                }
                            }
                        })
                    } else {
                        false
                    }
                };
                event_item.is_related_event = is_related_event;

                TracingRecordVariant::Event { event_item }
                /*
                let (span_info, running_span) = if let Some(span_info) = span {
                    let span_info = self.new_span_info(&span_info);
                    match self.running_spans.get(&span_info.t_id).cloned() {
                        None => {
                            warn!(?span_info, "not found running span");
                            (None, None)
                        }
                        Some(running_span) => (Some(span_info), Some(running_span)),
                    }
                } else {
                    (None, None)
                };
                let target: SmolStr = metadata.target.into();
                let level: TracingLevel = metadata.level.try_into().unwrap();
                fields.insert_span_t_id(span_info.as_ref().map(|n| n.t_id));

                if is_related_event {}

                TracingRecordVariant::Event {
                    message,
                    module_path,
                    file_line,
                    record_time,
                    span_info,
                    running_span,
                    fields,
                    target,
                    level,
                    app_info: self.app_run_info.clone(),
                    is_repeated_event: false,
                    is_related_event,
                }*/
            }
        })
    }
    async fn app_start(app_start: AppStartInfo) -> anyhow::Result<TracingRecordVariant> {
        let app_info = Arc::new(AppRunInfo {
            id: app_start.id,
            version: app_start.version.into(),
            run_id: app_start.run_id,
            node_id: app_start.node_id.into(),
        });

        Ok(TracingRecordVariant::AppStart {
            record_date: app_start.record_time,
            app_info: app_info.clone(),
            name: app_start.name.into(),
            fields: app_start.data.into(),
            reconnect: app_start.reconnect,
            created_spans: Default::default(),
        })
    }

    pub async fn new(
        app_start: AppStartInfo,
        tracing_service: crate::tracing_service::TracingService,
        record_sender: flume::Sender<AppRunMsg>,
    ) -> anyhow::Result<(Self, AppRunRecord)> {
        let mut record = Self::app_start(app_start.clone()).await?;
        let TracingRecordVariant::AppStart { app_info, .. } = &record else {
            unreachable!()
        };
        let app_info = app_info.clone();
        /*let running_spans = if app_start.reconnect {
            let span = get_running_span(&tracing_service.dc, &app_start).await?;
            let mut created_spans: hashbrown::HashMap<u64, CreatedSpan> =
                span.map(|n| (n.t_id, n)).collect();
            let mut sub_t_ids: hashbrown::HashMap<u64, SmallVec<[(u64, Uuid, bool); 8]>> =
                Default::default();
            for (_, item) in created_spans.iter() {
                if let Some(parent_span_t_id) = item.running_span.parent_span_t_id {
                    sub_t_ids.entry(parent_span_t_id).or_default().push((
                        item.t_id,
                        item.running_span.id,
                        false,
                    ));
                }
            }
            for item in created_spans.values_mut() {
                item.sub_span_t_ids = sub_t_ids.remove(&item.t_id).unwrap_or_default();
            }
            let TracingRecordVariant::AppStart {
                created_spans: css, ..
            } = &mut record
            else {
                unreachable!()
            };
            let r = created_spans
                .iter()
                .map(|n| (*n.0, n.1.running_span.clone()))
                .collect();
            css.extend(created_spans);
            r
        } else {
            Default::default()
        };*/
        Span::current().record("record", debug(&record));
        Span::current().record("app_start", debug(&app_start));
        Span::current().record("app_info", debug(&app_info));
        let record = AppRunRecord {
            id: RECORD_ID_GENERATOR.next(),
            record_index: 0,
            variant: record,
        };
        Ok((
            Self {
                app_start,
                tracing_service,
                record_sender,
                app_run_info: app_info.clone(),
                span_id_cache: lru::LruCache::new(NonZeroUsize::new(1024).unwrap()),
                // running_spans,
            },
            record,
        ))
    }

    pub async fn get_span_id_by_cahce_id(&mut self, info: &SpanCacheId) -> anyhow::Result<SpanId> {
        Ok(
            if let Some(span_id) = { self.span_id_cache.get(info).copied() } {
                span_id
            } else {
                let span_id = self
                    .tracing_service
                    .find_or_insert_tracing_span(info.clone())
                    .await
                    .context("query_span_id error")?;

                self.span_id_cache.put(info.clone(), span_id);
                span_id
            },
        )
    }
    //
    // pub async fn span_event_record(
    //     &mut self,
    //     kind: TracingKind,
    //     record_time: DateTime<FixedOffset>,
    //     span_info: &SpanInfo,
    //     fields: Option<serde_json::Value>,
    //     running_span: &RunningSpan,
    // ) -> Result<BigSerialId, Status> {
    //     let record_id = self
    //         .tracing_service
    //         .insert_record(
    //             span_info.cache_id.name.to_string(),
    //             record_time.fixed_offset(),
    //             kind.as_str().into(),
    //             running_span.level.map(|n| n.into()),
    //             Some(running_span.id),
    //             running_span.parent,
    //             fields,
    //             running_span.target.as_ref().map(|n| n.to_string()),
    //             Some(span_info.module_path.to_string()),
    //             Some(span_info.cache_id.file_line.to_string()),
    //             self.app_run_info.clone(),
    //         )
    //         .await
    //         .map_err(|err| Status::internal(format!("insert record error. {err}")))?;
    //
    //     Ok(record_id)
    // }

    // fn new_span_info(&self, span_info: &tracing_lv_core::SpanRawInfo) -> Arc<SpanInfo> {
    //     Arc::new(SpanInfo {
    //         module_path: span_info
    //             .metadata
    //             .module_path
    //             .clone()
    //             .unwrap_or_default()
    //             .into(),
    //         t_id: span_info.id,
    //         cache_id: SpanCacheId {
    //             app_id: self.app_start.id,
    //             app_version: self.app_start.version.clone(),
    //             name: span_info.metadata.name.clone().into(),
    //             file_line: format_smolstr!(
    //                 "{}:{}",
    //                 span_info.metadata.file.clone().unwrap_or_default(),
    //                 span_info.metadata.line.clone().unwrap_or_default()
    //             ),
    //         },
    //     })
    // }

    // fn get_running_span(&self, t_id: u64) -> Result<RunningSpan, Status> {
    //    Ok(self
    //       .running_spans
    //       .get(&t_id)
    //       .ok_or_else(|| Status::invalid_argument("span no created"))?
    //       .clone())
    // }

    async fn app_stop(
        mut self,
        exception_sotp: Option<DateTime<Utc>>,
        record_index: i64,
    ) -> anyhow::Result<AppRunRecord> {
        let exception_end = exception_sotp.is_some();
        let record_time = exception_sotp.clone().unwrap_or_else(|| {
            GLOBAL_DATA
                .get_node_now_timestamp_nanos(self.app_run_info.run_id)
                .map(DateTime::from_timestamp_nanos)
                .unwrap_or_else(|| {
                    warn!("not found global app_running");
                    Utc::now()
                })
        });

        // TODO: migrate to other file2
        /*      let running_spans = self.running_spans.clone();

        let _ = async {
           for (t_id, running_span) in running_spans {
              let span_info = tracing_lv_core::proto::SpanInfo {
                 t_id,
                 name: running_span.info.name.to_string(),
                 file_line: running_span.info.file_line.to_string(),
              };
              let pos_info = PosInfo {
                 module_path: running_span.info.module_path.to_string(),
                 file_line: running_span.info.file_line.to_string(),
              };
              if let Err(err) = self
                 .span_leave(SpanLeave {
                    record_time: record_time.timestamp_nanos_opt().unwrap(),
                    span_info: Some(span_info.clone()),
                    pos_info: Some(pos_info.clone()),
                 })
                 .await
              {
                 error!("failed to leave span {err:?}");
              }
              if let Err(err) = self
                 .span_close(SpanClose {
                    record_time: record_time.timestamp_nanos_opt().unwrap(),
                    span_info: Some(span_info),
                    pos_info: Some(pos_info),
                 })
                 .await
              {
                 error!("failed to close span {err:?}");
              }
           }
           Ok::<(), Status>(())
        }
           .await
           .inspect_err(|err| {
              error!("error: {err}");
           });*/
        Ok(AppRunRecord {
            id: RECORD_ID_GENERATOR.next(),
            record_index,
            variant: TracingRecordVariant::AppStop {
                record_date: record_time,
                app_info: self.app_run_info.clone(),
                name: self.app_start.name.into(),
                exception_end,
            },
        })
    }
}
//
// async fn get_running_span(
//     dc: &DatabaseConnection,
//     app_start: &AppStartInfo,
// ) -> Result<impl Iterator<Item = CreatedSpan> + 'static, DbErr> {
//     use entity::tracing_span_run::*;
//     let models = Entity::find()
//         .filter(
//             Column::AppRunId
//                 .eq(app_start.run_id)
//                 .and(Column::ExceptionEnd.is_null())
//                 .and(Column::CloseRecordId.is_null()),
//         )
//         .all(dc)
//         .await?;
//     let mut span_enter_models: hashbrown::HashMap<Uuid, Vec<entity::tracing_span_enter::Model>> =
//         Default::default();
//     for item in Entity::find()
//         .filter(entity::tracing_span_enter::Column::SpanRunId.is_in(models.iter().map(|n| n.id)))
//         .all(dc)
//         .await?
//         .into_iter()
//     {
//         span_enter_models
//             .entry(item.id)
//             .or_default()
//             .push(item);
//     }
//     let mut records: HashMap<i64, entity::tracing_record::Model> =
//         Entity::find()
//             .filter(entity::tracing_record::Column::Id.is_in(models.iter().map(|n| n.record_id)))
//             .all(dc)
//             .await?
//             .into_iter()
//             .map(|n| (n.id, n))
//             .collect();
//     let app_id = app_start.id;
//     let app_version = app_start.version.clone();
//     Ok(models
//         .into_iter()
//         .filter_map(move |n| {
//             let record = records.remove(&n.record_id);
//             if record.is_none() {
//                 warn!(record_id = n.record_id, "not found span_run record");
//             }
//             record.map(|record| (n, record))
//         })
//         .filter_map(move |(n, record)| {
//             let tracing_fields = TracingFields::from(record.fields.unwrap_or_default());
//             let span_info = Arc::new(SpanInfo {
//                 module_path: record.module_path.map(|n| n.into()).unwrap_or_default(),
//                 t_id: tracing_fields.get_span_t_id()?,
//                 cache_id: SpanCacheId {
//                     app_id: app_id.clone(),
//                     app_version: app_version.clone(),
//                     name: record.name.into(),
//                     file_line: record.position_info.map(|n| n.into()).unwrap_or_default(),
//                 },
//             });
//             let mut span_enters = span_enter_models.remove(&n.id).unwrap_or_default();
//             let running_span = RunningSpan {
//                 info: span_info.clone(),
//                 parent_span_t_id: record
//                     .parent_span_t_id
//                     .map(|n| u64::from_le_bytes(n.to_le_bytes())),
//                 id: n.span_id,
//                 parent: record.parent_id,
//                 target: record.target.map(|n| n.into()),
//                 level: record.level.map(|n| TracingLevel::try_from(n).unwrap()),
//             };
//             let total_enter_duration_secs: f64 =
//                 span_enters.iter().filter_map(|n| n.duration).sum();
//             span_enters.sort_by_key(|n| n.enter_time);
//             Some(CreatedSpan {
//                 id: n.span_id,
//                 span_base_info: Arc::new(SpanFullInfoBase {
//                     record_time: record.record_time.to_utc(),
//                     app_info: app_run_info.clone(),
//                     span_info,
//                     running_span,
//                 }),
//                 total_enter_duration: chrono::Duration::milliseconds(
//                     (total_enter_duration_secs * 1000.) as i64,
//                 )
//                 .into(),
//                 enter_span: span_enters
//                     .last()
//                     .filter(|n| n.duration.is_none())
//                     .map(|n| EnteredSpan {
//                         id: n.id,
//                         record_time: n.enter_time.to_utc(),
//                         record_id: n.record_id,
//                     }),
//                 record_id: n.record_id,
//                 record_index: record.app_run_record_index,
//                 last_record_filed_id: None,
//                 sub_span_t_ids: Default::default(),
//             })
//         }))
// }
//
// pub const FIELD_DATA_STABLE_SPAN_ID: &'static str = "sid";
// // pub const FIELD_DATA_PARENT_SPAN_ID: &'static str = "__data.parent_span_id";
// pub const FIELD_DATA_SPAN_T_ID: &'static str = "__data.span_t_id";
// pub const FIELD_DATA_FLAGS: &'static str = "__data.flags";
// pub const FIELD_DATA_EMPTY_CHILDREN: &'static str = "__data.empty_children";
// pub const FIELD_DATA_FIRST_EVENT: &'static str = "__data.first_event";
// pub const FIELD_DATA_IS_CONTAINS_RELATED: &'static str = "__data.is_contains_related";
// pub const FIELD_DATA_RELATED_NAME: &'static str = "__data.related_name";
// pub const FIELD_DATA_REPEATED_COUNT: &'static str = "__data.repeated_count";
// pub const FIELD_DATA_LAST_REPEATED_TIME: &'static str = "__data.last_repeated_time";
// pub const CONVENTION_FLAGS_FIELD: &'static str = FLAGS_AUTO_EXPAND;
// pub const CONVENTION_FLAGS_FORK: &'static str = FLAGS_FORK;
