use crate::global_data::GLOBAL_DATA;
use crate::record::{AppRunInfo, SpanCacheId, SpanId, TracingKind, TracingRecordVariant};
use crate::running_app::RunMsg;
use crate::tracing_service::{BigSerialId, TracingLevel, TracingRecordBatchInserter};
use bitflags::bitflags;
use bon::bon;
use chrono::{DateTime, FixedOffset, Local, Utc};
use derive_more::{Constructor, Deref, DerefMut, From};
use futures_util::StreamExt;
use serde_json::json;
use smallvec::SmallVec;
use smol_str::{SmolStr, ToSmolStr};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, info_span, warn, Instrument};
use tracing_lv_proto::tracing_service_server::TracingService;
use tracing_lv_proto::{
    field_value, record_param, AppStart, FieldValue, Ping, PingResult, PosInfo, RecordParam,
    SpanClose, SpanCreate, SpanLeave, SpanRecordField, TracingRecordResult, FLAGS_AUTO_EXPAND,
    FLAGS_FORK,
};
use uuid::Uuid;

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

#[derive(Clone, Debug)]
pub struct SpanInfo {
    pub module_path: SmolStr,
    pub t_id: u64,
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

#[derive(Default, Clone, Debug, Deref, DerefMut, Constructor)]
pub struct TracingFields(HashMap<String, FieldValue>);

impl From<HashMap<String, FieldValue>> for TracingFields {
    fn from(value: HashMap<String, FieldValue>) -> Self {
        let mut fields = Self(value);
        let mut flags = fields.get_flags().unwrap_or(TracingRecordFlags::empty());
        if let Some(_) = fields.remove(FLAGS_AUTO_EXPAND) {
            flags |= TracingRecordFlags::AUTO_EXPAND;
        }
        if let Some(_) = fields.remove(FLAGS_FORK) {
            flags |= TracingRecordFlags::FORK;
        }
        fields.insert_flags(flags);
        fields
    }
}

impl TracingFields {
    pub fn stable_span_id(&self) -> Option<&str> {
        let stable_span_id = self
            .0
            .get(FIELD_DATA_STABLE_SPAN_ID)
            .map(|n| n.variant.as_ref())
            .flatten();
        stable_span_id.map(|n| {
            let field_value::Variant::String(sid) = n else {
                unreachable!()
            };
            sid.as_str()
        })
    }
    pub fn insert_other(&mut self, other: &mut Self) {
        for (key, value) in other.drain() {
            self.0.insert(key, value);
        }
    }
    pub fn insert_flags(&mut self, value: TracingRecordFlags) -> Option<FieldValue> {
        self.insert_field(
            FIELD_DATA_FLAGS,
            serde_json::Value::Number(serde_json::Number::from(value.bits())),
        )
    }
    pub fn get_flags(&mut self) -> Option<TracingRecordFlags> {
        let value = self.0.get(FIELD_DATA_FLAGS)?;
        let variant = value.variant.as_ref().unwrap();
        let field_value::Variant::U64(flags) = variant else {
            return None;
        };
        TracingRecordFlags::from_bits(*flags)
    }
    pub fn insert_empty_children(&mut self) -> Option<FieldValue> {
        self.insert_field(FIELD_DATA_EMPTY_CHILDREN, serde_json::Value::Bool(true))
    }
    pub fn update_to_no_empty_children(&mut self) -> Option<FieldValue> {
        self.insert_field(FIELD_DATA_EMPTY_CHILDREN, serde_json::Value::Bool(false))
    }
    pub fn insert_span_t_id(&mut self, value: Option<u64>) -> Option<FieldValue> {
        if value.is_none() {
            return None;
        }
        self.insert_field(
            FIELD_DATA_SPAN_T_ID,
            value
                .map(|t_id| serde_json::Value::Number(t_id.into()))
                .unwrap_or(serde_json::Value::Null),
        )
    }

    pub fn insert_field(
        &mut self,
        name: impl Into<String>,
        value: impl Into<FieldValue>,
    ) -> Option<FieldValue> {
        self.0.insert(name.into(), value.into())
    }

    pub fn into_json_iter(self) -> impl Iterator<Item = (String, serde_json::Value)> {
        self.0.into_iter().map(|(k, v)| (k, v.into()))
    }
    pub fn into_json_map(self) -> serde_json::Map<String, serde_json::Value> {
        self.into_json_iter().collect()
    }
    pub fn into_json_value(self) -> serde_json::Value {
        serde_json::Value::Object(self.into_json_iter().collect())
    }
}

pub struct AppRunLifetime {
    tracing_service: crate::tracing_service::TracingService,
    #[allow(dead_code)]
    record_sender: flume::Sender<RunMsg>,
    pub app_run_info: Arc<AppRunInfo>,
    pub app_start: AppStart,
    pub span_id_cache: lru::LruCache<SpanCacheId, SpanId>,
    pub running_spans: hashbrown::HashMap<u64, RunningSpan>,
}

#[bon]
impl AppRunLifetime {
    #[builder]
    async fn get_span_full_info(
        &mut self,
        #[builder(start_fn)] record_time: i64,
        #[builder(start_fn)] span_info: tracing_lv_proto::SpanInfo,
        #[builder(start_fn)] pos_info: Option<PosInfo>,
        #[builder(into)] fields: Option<TracingFields>,
        running_span: Option<(Option<Arc<SpanInfo>>, SmolStr, tracing_lv_proto::Level)>,
    ) -> Result<SpanFullInfo, Status> {
        let span_info = self.new_span_info(span_info, pos_info);

        let record_time = DateTime::from_timestamp_nanos(record_time);

        let mut fields = fields.unwrap_or_default();
        let running_span = match running_span {
            None => self.get_running_span(span_info.t_id)?,
            Some((parent_span_info, target, level)) => {
                let parent_span_t_id = parent_span_info.as_ref().map(|n| n.t_id);
                let parent_span_id = self.get_span_id_optional(parent_span_info).await?;
                let span_id = match fields.stable_span_id() {
                    None => self.get_span_id(&span_info.cache_id).await?,
                    Some(n) => {
                        Uuid::from_str(n).map_err(|_| Status::invalid_argument("invalid sid"))?
                    }
                };
                RunningSpan {
                    parent_span_t_id,
                    info: span_info.clone(),
                    id: span_id,
                    parent: parent_span_id,
                    target: Some(target.clone()),
                    level: Some(level)
                        .clone()
                        .map(|n: tracing_lv_proto::Level| n.into()),
                }
            }
        };
        fields.insert_span_t_id(Some(span_info.t_id));
        Ok(SpanFullInfo {
            base: Arc::new(SpanFullInfoBase {
                record_time,
                app_info: self.app_run_info.clone(),
                span_info,
                running_span,
            }),
            fields,
        })
    }
}
impl AppRunLifetime {
    async fn app_start(app_start: AppStart) -> Result<TracingRecordVariant, Status> {
        let app_info = Arc::new(AppRunInfo {
            id: Uuid::from_bytes(
                app_start
                    .id
                    .try_into()
                    .map_err(|_| Status::invalid_argument("invalid app id"))?,
            ),
            version: app_start.version.into(),
            run_id: Uuid::from_bytes(
                app_start
                    .run_id
                    .try_into()
                    .map_err(|_| Status::invalid_argument("invalid app run id"))?,
            ),
            node_id: app_start.node_id.into(),
        });
        let record_time = DateTime::from_timestamp_nanos(app_start.record_time);

        Ok(TracingRecordVariant::AppStart {
            record_time,
            app_info: app_info.clone(),
            name: app_start.name.into(),
            fields: app_start.data.into(),
        })
    }
    pub async fn new(
        app_start: AppStart,
        tracing_service: crate::tracing_service::TracingService,
        record_sender: flume::Sender<RunMsg>,
    ) -> Result<Self, Status> {
        let record = Self::app_start(app_start.clone()).await?;
        let app_run_info = record.app_info().clone();
        let _ = record_sender.send(RunMsg::Record(record));
        Ok(Self {
            app_start,
            tracing_service,
            record_sender,
            app_run_info,
            span_id_cache: lru::LruCache::new(NonZeroUsize::new(1024).unwrap()),
            running_spans: Default::default(),
        })
    }

    pub async fn get_span_id(&mut self, info: &SpanCacheId) -> Result<SpanId, Status> {
        Ok(
            if let Some(span_id) = { self.span_id_cache.get(info).copied() } {
                span_id
            } else {
                let span_id = self
                    .tracing_service
                    .find_or_insert_tracing_span(info.clone())
                    .await
                    .map_err(|err| Status::internal(format!("query_span_id error. {err}")))?;

                self.span_id_cache.put(info.clone(), span_id);
                span_id
            },
        )
    }
    pub async fn get_span_id_optional(
        &mut self,
        info: Option<impl AsRef<SpanInfo>>,
    ) -> Result<Option<SpanId>, Status> {
        Ok(match info {
            None => None,
            Some(id) => Some(self.get_span_id(&id.as_ref().cache_id).await?),
        })
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

    fn new_span_cache_id(&self, span_info: tracing_lv_proto::SpanInfo) -> SpanCacheId {
        SpanCacheId::new(self.app_run_info.as_ref(), span_info)
    }

    fn new_span_info(
        &self,
        span_info: tracing_lv_proto::SpanInfo,
        pos_info: Option<PosInfo>,
    ) -> Arc<SpanInfo> {
        Arc::new(SpanInfo {
            module_path: pos_info.map(|n| n.module_path.into()).unwrap_or_default(),
            t_id: span_info.t_id,
            cache_id: self.new_span_cache_id(span_info),
        })
    }

    async fn span_create(&mut self, param: SpanCreate) -> Result<RunMsg, Status> {
        let parent_span_info = param.parent_span_info.map(|n| self.new_span_info(n, None));

        let target: SmolStr = param.target.into();
        let level: tracing_lv_proto::Level = param.level.try_into().unwrap();

        let mut full_info = self
            .get_span_full_info(param.record_time, param.span_info.unwrap(), param.pos_info)
            .fields(param.fields)
            .running_span((parent_span_info.clone(), target.clone(), level))
            .call()
            .await?;
        full_info.fields.insert_empty_children();

        {
            self.running_spans.insert(
                full_info.t_id,
                RunningSpan {
                    info: full_info.span_info.clone(),
                    parent_span_t_id: full_info.running_span.parent_span_t_id,
                    id: full_info.running_span.id,
                    parent: full_info.running_span.parent,
                    target: Some(target),
                    level: Some(level.into()),
                },
            );
        }

        Ok(RunMsg::Record(TracingRecordVariant::SpanCreate {
            name: full_info.cache_id.name.clone(),
            info: full_info,
            parent_span_info,
        }))
    }

    fn get_running_span(&self, t_id: u64) -> Result<RunningSpan, Status> {
        Ok(self
            .running_spans
            .get(&t_id)
            .ok_or_else(|| Status::invalid_argument("span no created"))?
            .clone())
    }

    async fn span_enter(&mut self, param: tracing_lv_proto::SpanEnter) -> Result<RunMsg, Status> {
        Ok(RunMsg::Record(TracingRecordVariant::SpanEnter {
            info: self
                .get_span_full_info(param.record_time, param.span_info.unwrap(), param.pos_info)
                .call()
                .await?,
        }))
    }

    async fn span_leave(&mut self, param: SpanLeave) -> Result<RunMsg, Status> {
        Ok(RunMsg::Record(TracingRecordVariant::SpanLeave {
            info: self
                .get_span_full_info(param.record_time, param.span_info.unwrap(), param.pos_info)
                .call()
                .await?,
        }))
    }

    async fn span_close(&mut self, param: SpanClose) -> Result<RunMsg, Status> {
        Ok(RunMsg::Record(TracingRecordVariant::SpanClose {
            info: self
                .get_span_full_info(param.record_time, param.span_info.unwrap(), param.pos_info)
                .call()
                .await?,
        }))
    }

    async fn span_record_field(&mut self, param: SpanRecordField) -> Result<RunMsg, Status> {
        Ok(RunMsg::Record(TracingRecordVariant::SpanRecord {
            info: self
                .get_span_full_info(param.record_time, param.span_info.unwrap(), param.pos_info)
                .fields(param.fields)
                .call()
                .await?,
        }))
    }

    async fn event(&mut self, param: tracing_lv_proto::Event) -> Result<RunMsg, Status> {
        let record_time = DateTime::from_timestamp_nanos(param.record_time);
        let (span_info, running_span) = if let Some(span_info) = param.span_info {
            let span_info = self.new_span_info(span_info, None);
            let running_span = self.running_spans.get(&span_info.t_id).unwrap().clone();

            (Some(span_info), Some(running_span))
        } else {
            (None, None)
        };
        // let (repeated_record_id, first_event, span) = if let Some(span_info) = param.span_info {
        //     let span_info = self.new_span_info(span_info, None);
        //     let running_span = self.running_spans.get(&span_info.t_id).unwrap().clone();
        //     (
        //         if running_span
        //             .last_repeated_event
        //             .as_ref()
        //             .map(|n| n.1.as_str())
        //             == Some(param.message.as_str())
        //         {
        //             Some(running_span.last_repeated_event.as_ref().unwrap().0)
        //         } else {
        //             None
        //         },
        //         running_span.last_repeated_event.is_none(),
        //         Some((span_info, running_span)),
        //     )
        // } else {
        //     (
        //         if self.last_repeated_event.as_ref().map(|n| n.1.as_str())
        //             == Some(param.message.as_str())
        //         {
        //             Some(self.last_repeated_event.as_ref().unwrap().0)
        //         } else {
        //             None
        //         },
        //         self.last_repeated_event.is_none(),
        //         None,
        //     )
        // };
        // let (span_info, running_span) = span.unzip();
        // let is_repeated = repeated_record_id.is_some();
        let mut fields: TracingFields = param.fields.into();
        // fields.insert_span_t_id(span_info.t_id);
        // let span_info = self.new_span_info(span_info, pos_info);
        let target: SmolStr = param.target.into();
        let level: tracing_lv_proto::Level = param.level.try_into().unwrap();

        // if first_event {
        //     fields.insert_field(FIELD_DATA_FIRST_EVENT, serde_json::Value::Null);
        // }
        fields.insert_span_t_id(span_info.as_ref().map(|n| n.t_id));

        // if let Some(repeated_record_id) = repeated_record_id {
        //     self.tracing_service.incremental_repeated_event(repeated_record_id, record_time.fixed_offset())
        //     .await
        //     .map_err(|err| {
        //        Status::internal(format!("repeated_record_id {repeated_record_id:?} incremental_repeated_event error: {err:?}"))
        //     })?;
        // } else {
        //     // TODO:
        //     if let Some(span_info) = span_info.as_ref() {
        //         let running_span = self.running_spans.get_mut(&span_info.t_id).unwrap();
        //         running_span.last_repeated_event = Some((record_id, param.message.to_smolstr()));
        //     } else {
        //         self.last_repeated_event = Some((record_id, param.message.to_smolstr()));
        //     }
        // }
        let (module_path, file_line) = param
            .pos_info
            .map(|n| (n.module_path.into(), n.file_line.into()))
            .unzip();

        Ok(RunMsg::Record(TracingRecordVariant::Event {
            message: param.message.into(),
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
        }))
    }

    async fn app_stop(mut self, exception_end: bool) -> Result<(), Status> {
        let record_time = Utc::now();

        let running_spans = self.running_spans.clone();

        let _ = async {
            for (t_id, running_span) in running_spans {
                let span_cache_id = self
                    .span_id_cache
                    .iter()
                    .find(|n| n.1 == &running_span.id)
                    .map(|n| n.0)
                    .cloned()
                    .unwrap();
                if let Err(err) = self
                    .span_leave(SpanLeave {
                        record_time: record_time.timestamp_nanos_opt().unwrap(),
                        span_info: Some(tracing_lv_proto::SpanInfo {
                            t_id,
                            name: span_cache_id.name.to_string(),
                            file_line: span_cache_id.file_line.to_string(),
                        }),
                        pos_info: Some(PosInfo {
                            module_path: "".to_string(),
                            file_line: span_cache_id.file_line.to_string(),
                        }),
                    })
                    .await
                {
                    error!("failed to leave span {err:?}");
                }
                if let Err(err) = self
                    .span_close(SpanClose {
                        record_time: record_time.timestamp_nanos_opt().unwrap(),
                        span_info: Some(tracing_lv_proto::SpanInfo {
                            t_id,
                            name: span_cache_id.name.to_string(),
                            file_line: span_cache_id.file_line.to_string(),
                        }),
                        pos_info: Some(PosInfo {
                            module_path: "".to_string(),
                            file_line: span_cache_id.file_line.to_string(),
                        }),
                    })
                    .await
                {
                    error!("failed to close span {err:?}");
                }
            }
            Ok::<(),Status>(())
        }
        .await
        .inspect_err(|err| {
            error!("error: {err}");
        });
        let _ = self
            .record_sender
            .send(RunMsg::Record(TracingRecordVariant::AppStop {
                record_time,
                app_info: self.app_run_info.clone(),
                name: self.app_start.name.into(),
                exception_end,
            }))
            .inspect_err(|err| {
                error!(?err, "send app stop msg failed");
            });
        Ok(())
    }
}

pub struct TracingServiceImpl {
    tracing_service: crate::tracing_service::TracingService,
    record_sender: flume::Sender<RunMsg>,
}

impl TracingServiceImpl {
    pub fn new(
        tracing_service: crate::tracing_service::TracingService,
        record_sender: flume::Sender<RunMsg>,
    ) -> Self {
        Self {
            record_sender,
            tracing_service,
        }
    }
}

pub const FIELD_DATA_STABLE_SPAN_ID: &'static str = "sid";
// pub const FIELD_DATA_PARENT_SPAN_ID: &'static str = "__data.parent_span_id";
pub const FIELD_DATA_SPAN_T_ID: &'static str = "__data.span_t_id";
pub const FIELD_DATA_FLAGS: &'static str = "__data.flags";
pub const FIELD_DATA_EMPTY_CHILDREN: &'static str = "__data.empty_children";
pub const FIELD_DATA_FIRST_EVENT: &'static str = "__data.first_event";
pub const FIELD_DATA_REPEATED_COUNT: &'static str = "__data.repeated_count";
pub const FIELD_DATA_LAST_REPEATED_TIME: &'static str = "__data.last_repeated_time";
pub const CONVENTION_FLAGS_FIELD: &'static str = FLAGS_AUTO_EXPAND;
pub const CONVENTION_FLAGS_FORK: &'static str = FLAGS_FORK;

#[tonic::async_trait]
impl TracingService for TracingServiceImpl {
    async fn app_run(
        &self,
        request: Request<Streaming<RecordParam>>,
    ) -> Result<Response<TracingRecordResult>, Status> {
        let mut streaming = request.into_inner();
        let record = streaming
            .next()
            .await
            .ok_or_else(|| Status::unavailable("empty message"))??;
        let record_param::Variant::AppStart(app_start) = record.variant.unwrap() else {
            panic!("must first app start")
        };
        let half_rtt = app_start.rtt / 2.0;
        let mut app_run_lifetime = AppRunLifetime::new(
            app_start,
            self.tracing_service.clone(),
            self.record_sender.clone(),
        )
        .await?;

        {
            let now = Utc::now();
            let delta_date_nanos = now.timestamp_nanos_opt().unwrap()
                - (half_rtt * 1_000_000_000.0) as i64
                - record.send_time;

            GLOBAL_DATA.add_running_app(app_run_lifetime.app_run_info.run_id, delta_date_nanos);
        }

        let app_run_info = app_run_lifetime.app_run_info.clone();

        // Tonic Bug: ?. Future is sometimes canceled
        tokio::spawn(
            async move {
                let mut exception_end = true;
                let result = async {
                    while let Some(result) = streaming.next().await {
                        match result {
                            Ok(RecordParam {
                                send_time: _, // TODO: Check if time is too long
                                variant,
                            }) => {
                                let variant = variant.unwrap();
                                // if !matches!(variant, record_param::Variant::Event(_)) {
                                //     app_run_lifetime.last_repeated_event = None;
                                // }
                                let record = match variant {
                                    record_param::Variant::AppStart(_) => {
                                        unreachable!("AppStart should not be call")
                                    }
                                    record_param::Variant::SpanCreate(param) => {
                                        app_run_lifetime.span_create(param).await?
                                    }
                                    record_param::Variant::SpanEnter(param) => {
                                        app_run_lifetime.span_enter(param).await?
                                    }
                                    record_param::Variant::SpanLeave(param) => {
                                        app_run_lifetime.span_leave(param).await?
                                    }
                                    record_param::Variant::SpanClose(param) => {
                                        app_run_lifetime.span_close(param).await?
                                    }
                                    record_param::Variant::SpanRecordField(param) => {
                                        app_run_lifetime.span_record_field(param).await?
                                    }
                                    record_param::Variant::Event(param) => {
                                        app_run_lifetime.event(param).await?
                                    }
                                    record_param::Variant::AppStop(_) => {
                                        info!("app stop");
                                        exception_end = false;
                                        break;
                                    }
                                };
                                if let Err(err) = app_run_lifetime.record_sender.send(record) {
                                    info!(?err, "record_sender send failed. exit!");
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!("{err:?}");
                            }
                        }
                    }
                    Ok::<(), Status>(())
                }
                .await;
                GLOBAL_DATA.remove_running_app(app_run_lifetime.app_run_info.run_id);
                if let Err(err) = result {
                    error!("{err}");
                }
                if exception_end {
                    warn!(run_info=?app_run_lifetime.app_run_info,"app exception_end");
                }
                app_run_lifetime
                    .app_stop(exception_end)
                    .await
                    .inspect_err(|err| {
                        error!("app_stop error: {err}");
                    })?;

                info!("app lifetime end");
                Ok(Response::new(TracingRecordResult {}))
            }
            .instrument(info_span!("app lifetime", ?app_run_info)),
        )
        .await
        .map_err(|n| Status::internal(format!("app lifetime panic: {n}")))?
    }

    async fn ping(&self, _request: Request<Ping>) -> Result<Response<PingResult>, Status> {
        Ok(Response::new(PingResult {}))
    }
}
