use crate::event_service::EventService;
use crate::grpc_service::{
    SpanFullInfo, SpanFullInfoBase, SpanInfo, TracingFields, TracingRecordFlags,
    TracingServiceImpl, FIELD_DATA_EMPTY_CHILDREN, FIELD_DATA_FIRST_EVENT, FIELD_DATA_FLAGS,
    FIELD_DATA_IS_CONTAINS_RELATED, FIELD_DATA_REPEATED_COUNT,
};
use crate::record::{AppRunInfo, SpanId, TracingRecordVariant};
use crate::tracing_service::{
    AppRunDto, BigInt, TracingRecordBatchInserter, TracingRecordDto, TracingRecordFilter,
    TracingService, TracingSpanEnterDto, TracingSpanRunDto, TracingTreeRecordDto,
    TracingTreeRecordVariantDto,
};
use anyhow::{anyhow, Context};
use chrono::{DateTime, Local, Utc};
use derive_more::Deref;
use futures_util::{StreamExt, TryStreamExt};
use http_body_util::BodyExt;
use sea_orm::sqlx::error::BoxDynError;
use sea_orm::sqlx::postgres::{PgArguments, PgQueryResult, PgRow};
use sea_orm::sqlx::{Arguments, Encode, Error, Postgres, Row, Type};
use sea_orm::{
    sqlx, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, ExecResult, Statement,
};
use smallvec::SmallVec;
use smol_str::{SmolStr, ToSmolStr};
use std::collections::{HashMap, VecDeque};
use std::fmt::Write;
use std::future::Future;
use std::panic::Location;
use std::sync::Arc;
use std::time::Duration;
use std::{env, mem};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio::{join, try_join};
use tonic::Status;
use tracing::{error, info, warn};
use tracing_lv_core::proto::FieldValue;
use uuid::Uuid;

#[derive(Debug,Clone)]
struct EnteredSpan {
    id: Uuid,
    record_time: DateTime<Utc>,
    record_id: i64,
}

#[derive(Clone,Deref)]
struct CreatedSpan {
    id: Uuid,
    #[deref]
    span_base_info: Arc<SpanFullInfoBase>,
    parent_span_t_id: Option<u64>,
    total_enter_duration: chrono::Duration,
    enter_span: Option<EnteredSpan>,
    record_id: i64,
    record_index: i64,
    // 存储字段，合并通知字段更新
    last_record_filed_id: Option<(i64, TracingFields)>,
    last_repeated_event: Option<(i64, SmolStr, usize)>,
    sub_span_t_ids: SmallVec<[(u64, Uuid, bool); 8]>,
}

struct RecordWatcher {
    sender: flume::Sender<Arc<TracingTreeRecordDto>>,
    filter: TracingRecordFilter,
}

#[derive(Clone, Debug)]
pub struct AppRunRecord {
    pub id: i64,
    pub record_index: i64,
    pub variant: TracingRecordVariant,
}

impl Into<TracingRecordDto> for AppRunRecord {
    fn into(mut self) -> TracingRecordDto {
        TracingRecordDto {
            id: self.id,
            record_index: self.record_index,
            app_id: self.variant.app_info().id,
            app_version: self.variant.app_info().version.clone(),
            app_run_id: self.variant.app_info().run_id,
            node_id: self.variant.app_info().node_id.clone(),
            name: self.variant.name().clone(),
            kind: self.variant.kind(),
            level: self.variant.level(),
            span_id: self.variant.span_id(),
            fields: Arc::new(
                self.variant
                    .fields_mut()
                    .map(|n| core::mem::take(n).into_json_map())
                    .unwrap_or_default(),
            ),
            span_id_is_stable: self
                .variant
                .span_full_info()
                .map(|n| n.fields.stable_span_id().is_some()),
            record_time: self.variant.record_time().fixed_offset(),
            target: self.variant.target().cloned(),
            module_path: self.variant.module_path().cloned(),
            position_info: self.variant.file_line().cloned(),
            creation_time: Utc::now().fixed_offset(),
            parent_id: self.variant.parent_id(),
            span_t_id: self.variant.span_t_id().map(|n| n.to_smolstr()),
            parent_span_t_id: self.variant.parent_span_t_id().map(|n| n.to_smolstr()),
            repeated_count: None,
        }
    }
}

pub enum AppRunMsg {
    Record(AppRunRecord),
    AddRecordWatcher(RecordWatcher),
}

pub enum RunMsg {
    AppRun {
        record_sender: flume::Sender<AppRunMsg>,
        run_info: Arc<AppRunInfo>,
        record_receiver: flume::Receiver<AppRunMsg>,
    },
    AddRecordWatcher {
        sender: flume::Sender<Arc<TracingTreeRecordDto>>,
        filter: TracingRecordFilter,
    },
}

pub struct SqlBatchExecutor {
    pub sql: String,
    // args: Vec<sea_orm::Value>,
}

impl Write for SqlBatchExecutor {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.sql.write_str(s)
    }
}

impl Default for SqlBatchExecutor {
    fn default() -> Self {
        Self {
            sql: String::with_capacity(32),
            // args: vec![],
        }
    }
}

impl SqlBatchExecutor {
    pub fn append(
        &mut self,
        f: impl FnOnce(&mut String /*, &mut SqlBatchArgs*/) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        f(&mut self.sql)?;
        // let mut args = core::mem::take(&mut self.args);
        // let mut batch_args = SqlBatchArgs { args: &mut args };
        // f(&mut self.sql, &mut batch_args)?;
        // core::mem::swap(&mut self.args, &mut args);
        Ok(())
    }
    pub async fn execute(&mut self, dc: &DatabaseConnection) -> Result<Option<()>, Error> {
        if self.sql.is_empty() {
            return Ok(None);
        }
        {
            let mut stream =
                sqlx::raw_sql(self.sql.as_str()).execute_many(dc.get_postgres_connection_pool());
            while let Some(r) = stream.next().await {
                if let Err(err) = r {
                    error!(sql = self.sql, "Failed to execute sql: {}", err);
                }
            }
        }
        self.sql.clear();
        Ok(Some(()))
    }
}

pub struct TLConfig {
    pub record_max_delay: Duration,
    pub record_max_buf_count: usize,
}

pub struct RunningAppInfo {
    run_info: Arc<AppRunInfo>,
    record_sender: flume::Sender<AppRunMsg>,
    task: JoinHandle<()>,
}

impl Drop for RunningAppInfo {
    fn drop(&mut self) {
        self.task.abort()
    }
}

pub struct RunningApps {
    tracing_service: TracingService,
    running_apps: HashMap<Uuid, RunningAppInfo>,
    config: Arc<TLConfig>,
}

impl RunningApps {
    pub fn new(tracing_service: TracingService, config: TLConfig) -> Self {
        Self {
            tracing_service,
            running_apps: Default::default(),
            config: Arc::new(config),
        }
    }

    pub async fn handle_records(&mut self, msg_receiver: flume::Receiver<RunMsg>) {
        while let Ok(msg) = msg_receiver.recv_async().await {
            match msg {
                RunMsg::AppRun {
                    run_info,
                    record_receiver,
                    record_sender,
                } => {
                    let mut running_app = RunningApp::new(
                        self.tracing_service.clone(),
                        run_info.clone(),
                        self.config.clone(),
                    );
                    let task = tokio::spawn(async move {
                        running_app.handle_records(record_receiver).await.unwrap();
                    });
                    if let Some(_app) = self.running_apps.insert(
                        run_info.run_id,
                        RunningAppInfo {
                            run_info: run_info.clone(),
                            record_sender,
                            task,
                        },
                    ) {
                        warn!("app {} already running!", run_info.id);
                        continue;
                    }
                }
                RunMsg::AddRecordWatcher { sender, filter } => {
                    for (_, app) in self.running_apps.iter_mut() {
                        if let Some(app_run_ids) = filter.app_run_ids.as_ref() {
                            if !app_run_ids.is_empty()
                                && !app_run_ids.contains(&app.run_info.run_id)
                            {
                                break;
                            }
                        }
                        let _r = app.record_sender
                           .send(AppRunMsg::AddRecordWatcher(RecordWatcher {
                               sender: sender.clone(),
                               filter: filter.clone(),
                           }));
                    }
                }
            }
        }
    }
}

pub struct RunningApp {
    tracing_service: TracingService,

    app_info: Arc<AppRunInfo>,
    created_spans: hashbrown::HashMap<u64, CreatedSpan>,
    last_repeated_event: Option<(u64, SmolStr, usize)>,

    // running_apps: hashbrown::HashMap<Uuid, RunningApp>,

    // buf_records: VecDeque<(TracingRecordVariant, u64)>,
    dto_buf: Vec<(AppRunRecord, Option<TracingTreeRecordVariantDto>)>,
    record_batch_inserter: TracingRecordBatchInserter,
    pub config: Arc<TLConfig>,
}

impl RunningApp {
    #[track_caller]
    #[inline(always)]
    fn set_exception_end_for_sub_spans(
        &mut self,
        span_t_id: u64,
        batch_inserter: &mut SqlBatchExecutor,
        record_time: DateTime<Utc>,
    ) -> Result<(), std::fmt::Error> {
        let Some(created_span) = self.get_created_span_mut(span_t_id) else {
            return Ok(());
        };

        let sub_span_t_ids = core::mem::take(&mut created_span.sub_span_t_ids);
        for (sub_span_t_id, id, _) in sub_span_t_ids.iter().filter(|n| !n.2) {
            set_exception_end_for_span_run(id, batch_inserter, record_time)?;
            self.set_exception_end_for_sub_spans(*sub_span_t_id, batch_inserter, record_time)?;
        }
        Ok(())
    }

    #[track_caller]
    fn get_created_span_mut(&mut self, span_t_id: u64) -> Option<&mut CreatedSpan> {
        let Some(created_span) = self.created_spans.get_mut(&span_t_id) else {
            warn!(
                "app {} span_t_id {} not created. {:?}",
                self.app_info.id,
                span_t_id,
                Location::caller(),
            );
            return None;
        };
        Some(created_span)
    }
    fn remove_created_span(&mut self, span_t_id: u64) -> Option<CreatedSpan> {
        let Some(created_span) = self.created_spans.remove(&span_t_id) else {
            warn!(
                "app {} span_t_id {} not created",
                self.app_info.id, span_t_id
            );
            return None;
        };
        Some(created_span)
    }
}

enum PreHandleResult {
    BufFilled,
    Continue,
    Pushed,
}
const POSTGRESQL_MAX_BIND_PARAM_COUNT: usize = 32767;

impl RunningApp {
    pub fn new(
        tracing_service: TracingService,
        app_info: Arc<AppRunInfo>,
        config: Arc<TLConfig>,
    ) -> Self {
        Self {
            config,
            tracing_service,
            app_info,
            created_spans: Default::default(),
            last_repeated_event: None,
            dto_buf: vec![],
            record_batch_inserter: TracingRecordBatchInserter::new(),
        }
    }
    /*
        #[inline(always)]
        fn get_running_app_mut(&mut self, app_info: &AppRunInfo) -> Option<&mut RunningApp> {
            let Some(running_app) = self.running_apps.get_mut(&app_info.run_id) else {
                warn!("app {} not running", app_info.id);
                return None;
            };
            Some(running_app)
        }
        #[inline(always)]
        fn remove_running_app_mut(&mut self, app_info: &AppRunInfo) -> Option<RunningApp> {
            let Some(running_app) = self.running_apps.remove(&app_info.run_id) else {
                warn!("app {} not running", app_info.id);
                return None;
            };
            Some(running_app)
        }

    #[track_caller]
    #[inline(always)]
    fn get_created_span_mut(&mut self, info: &SpanFullInfo) -> Option<&mut CreatedSpan> {
        let running_app = self.get_running_app_mut(&info.app_info)?;
        running_app.get_created_span_mut(info.t_id)
    }*/

    #[inline(always)]
    fn pre_handle_msg(
        msg: AppRunMsg,
        max_buf_count: usize,
        buf: &mut VecDeque<AppRunRecord>,
        record_watch_buf: &mut Vec<RecordWatcher>,
    ) -> PreHandleResult {
        match msg {
            AppRunMsg::Record(record) => {
                buf.push_back(record);
                if buf.len() >= max_buf_count + 1 {
                    PreHandleResult::BufFilled
                } else {
                    PreHandleResult::Pushed
                }
            }
            AppRunMsg::AddRecordWatcher(record_watcher) => {
                record_watch_buf.push(record_watcher);
                PreHandleResult::Continue
            }
        }
    }

    pub async fn handle_records(
        &mut self,
        record_receiver: flume::Receiver<AppRunMsg>,
    ) -> anyhow::Result<()> {
        let mut json_buf = Vec::new();

        let mut cur_record_field_count = 0;
        let config = self.config.clone();
        let mut buf_records_1: VecDeque<AppRunRecord> =
            VecDeque::with_capacity(config.record_max_buf_count);
        let mut buf_records_2: VecDeque<AppRunRecord> =
            VecDeque::with_capacity(config.record_max_buf_count);
        let mut buf_event_senders = Vec::new();
        let mut batch_inserter = SqlBatchExecutor::default();

        let mut event_service = EventService::default();
        let mut instant;

        loop {
            instant = Instant::now();
            {
                let future_1 = async {
                    let mut delay_future =
                        std::pin::pin!(tokio::time::sleep(config.record_max_delay));
                    match Self::pre_handle_msg(
                        record_receiver.recv_async().await?,
                        config.record_max_buf_count,
                        &mut buf_records_1,
                        &mut buf_event_senders,
                    ) {
                        PreHandleResult::BufFilled => {}
                        PreHandleResult::Continue => return Ok(()),
                        PreHandleResult::Pushed => {}
                    }
                    loop {
                        tokio::select! {
                            _ = &mut delay_future => {
                                break;
                            }
                            msg = record_receiver.recv_async() => {
                                match Self::pre_handle_msg(msg?,config.record_max_buf_count,&mut buf_records_1,&mut buf_event_senders) {
                                    PreHandleResult::BufFilled => {
                                        break;
                                    }
                                    PreHandleResult::Continue => {},
                                    PreHandleResult::Pushed => {}
                                }
                            }
                        }
                    }
                    anyhow::Ok(())
                };
                let future_2 = async {
                    let batch_inserter = &mut batch_inserter;
                    let mut records_iter = buf_records_2.iter_mut();
                    while let Some(record) = records_iter.next() {
                        /*                  if let TracingRecordVariant::Event {
                           span_info,
                           message,
                           app_info,
                           is_repeated_event,
                           is_related_event,
                           ..
                        } = record
                        {
                           let last_repeated_event = match span_info {
                              None => Some(&mut self.last_repeated_event),
                              Some(span_info) => self
                                 .get_created_span_mut(span_info.t_id)
                                 .map(|n| &mut n.last_repeated_event),
                           };
                           if let Some(last_repeated_event) = last_repeated_event {
                              let first_event = last_repeated_event.is_none();
                              *is_repeated_event = last_repeated_event.as_ref().map(|n| n.1.as_str())
                                 == Some(message.as_str());
                              if *is_repeated_event {
                                 #[allow(unused_mut)]
                                 let (mut repeated_record_id, _, repeated_count) =
                                    last_repeated_event.as_mut().unwrap();
                                 *repeated_count += 1;
                                 // TODO:
                                 // writeln!(
                                 //     batch_inserter,
                                 //     r#"update tracing_record set fields = fields||'{{"{}":{}}}'::jsonb where id = '{}';"#,
                                 //     FIELD_DATA_REPEATED_COUNT,
                                 //     *repeated_count,
                                 //     u64_to_i64(repeated_record_id),
                                 // )?;
                              } else {
                                 *last_repeated_event = Some((*record_index, message.clone(), 1));
                              }
                              if first_event {
                                 writeln!(
                                    batch_inserter,
                                    r#"update tracing_record set fields = fields||'{{"{}":true}}'::jsonb where id = '{}' and app_run_id = '{}';"#,
                                    FIELD_DATA_FIRST_EVENT, *record_index, app_info.run_id,
                                 )?;
                                 if let Some(span_info) = span_info {
                                    if let Some(created_span) =
                                       self.get_created_span_mut(span_info.t_id)
                                    {
                                       let dto = Self::update_to_no_empty_children(
                                          batch_inserter,
                                          created_span,
                                       )?;
                                       self.dto_buf.push(dto);
                                    }
                                 }
                              }
                              if *is_related_event {
                                 if let Some(span_info) = span_info {
                                    if let Some(created_span) =
                                       self.get_created_span_mut(span_info.t_id)
                                    {
                                       // TODO:
                                       writeln!(
                                          batch_inserter,
                                          r#"update tracing_record set fields = fields||'{{"{}":true}}'::jsonb where id = '{}';"#,
                                          FIELD_DATA_IS_CONTAINS_RELATED,
                                          created_span.record_index,
                                       )?;
                                       let mut info = SpanFullInfo {
                                          base: created_span.span_base_info.clone(),
                                          fields: Default::default(),
                                       };
                                       info.fields.update_to_contains_related();
                                       let record_index = created_span.record_index;
                                       self.dto_buf.push((
                                          TracingRecordVariant::SpanRecord { info },
                                          record_index,
                                          None,
                                       ));
                                    }
                                 }
                              }
                           }
                        }*/

                        let record_time = record.variant.record_time();
                        let record_time = record_time.fixed_offset();
                        let kind = record.variant.kind();
                        let level = record.variant.level();
                        let span_id = record.variant.span_id();
                        let parent_id = record.variant.parent_id();
                        let parent_span_t_id = record.variant.parent_span_t_id();

                        // debug_assert_eq!(*record_index, self.record_batch_inserter.id);
                        record
                            .variant
                            .scoped_json_fields(|n, json_fields| {
                                let target = n.target();
                                let module_path = n.module_path();
                                let file_line = n.file_line();
                                let app_info = n.app_info();
                                self.record_batch_inserter.append_insert_record(
                                    record.id,
                                    record.record_index,
                                    n.name(),
                                    &record_time,
                                    kind.as_str(),
                                    level.map(|n| n.into()),
                                    span_id,
                                    parent_span_t_id.map(|n| i64::from_le_bytes(n.to_le_bytes())),
                                    parent_id,
                                    json_fields.as_ref(),
                                    target.map(|n| n.as_str()),
                                    module_path.map(|n| n.as_str()),
                                    file_line.map(|n| n.as_str()),
                                    app_info.as_ref(),
                                )
                            })
                            .map_err(|n| anyhow!("{n:?}"))?;
                    }
                    let records_insert_prepare_duration = instant.elapsed();
                    instant = Instant::now();

                    let records_insert_execute_duration = instant.elapsed();
                    instant = Instant::now();

                    self.dto_buf.reserve_exact(buf_records_2.len());
                    while let Some(mut record) = buf_records_2.pop_front() {
                        let app_info = record.variant.app_info().clone();
                        let r = match &mut record.variant {
                            TracingRecordVariant::SpanCreate { info, .. } => {
                                println!("SpanCrate: {}. record_index: {}",info.span_info.t_id,record.record_index);
                                if let Some(_previous) = self.created_spans.insert(
                                    info.span_info.t_id,
                                    CreatedSpan {
                                        id: Uuid::new_v4(),
                                        parent_span_t_id: info.running_span.parent_span_t_id,
                                        total_enter_duration: Default::default(),
                                        enter_span: None,
                                        record_id: record.record_index,
                                        record_index: record.record_index,
                                        last_record_filed_id: None,
                                        last_repeated_event: None,
                                        span_base_info: info.base.clone(),
                                        sub_span_t_ids: Default::default(),
                                    },
                                ) {
                                    warn!(
                                        "app {} span_t_id {} already created!",
                                        info.app_info.run_id, info.span_info.t_id
                                    );
                                }
                                let Some(created_span) = self.get_created_span_mut(info.t_id)
                                else {
                                    continue;
                                };
                                let created_span_id = created_span.id;
                                if let Some(parent_span_t_id) = created_span.parent_span_t_id {
                                    if let Some(created_span) =
                                        self.get_created_span_mut(parent_span_t_id)
                                    {
                                        created_span.sub_span_t_ids.push((
                                            info.t_id,
                                            created_span_id,
                                            false,
                                        ));
                                        if created_span.sub_span_t_ids.is_empty()
                                            && created_span.last_repeated_event.is_none()
                                        {
                                            let r = Self::update_to_no_empty_children(
                                                batch_inserter,
                                                created_span,
                                            )?;
                                            self.dto_buf.push(r);
                                        }
                                    }
                                }

                                if let Some(parent_span_id) = info.running_span.parent {
                                    write!(
                                        batch_inserter,
                                        r#"insert into tracing_span_parent(span_id, span_parent_id) values ('{}','{}') on conflict do nothing;"#,
                                        info.running_span.id, parent_span_id
                                    )?;
                                }

                                let run_time = info.record_time.fixed_offset();
                                let span_id = info.running_span.id;
                                writeln!(
                                    batch_inserter,
                                    r#"insert into tracing_span_run(id,app_run_id,span_id,run_time,record_id,fields) values ('{}','{}','{}','{}','{}','{{}}'::jsonb);"#,
                                    created_span_id, app_info.run_id, span_id, run_time, record.id
                                )?;

                                Some(TracingTreeRecordVariantDto::SpanRun(TracingSpanRunDto {
                                    id: created_span_id,
                                    app_run_id: app_info.run_id,
                                    span_id,
                                    run_time: info.record_time.fixed_offset(),
                                    busy_duration: None,
                                    idle_duration: None,
                                    record_id: record.id,
                                    close_record_id: None,
                                    exception_end: Default::default(),
                                    run_elapsed: Some(0.),
                                    fields: Arc::new(info.fields.clone().into_json_map()),
                                    related_events: Default::default(),
                                }))
                            }
                            TracingRecordVariant::SpanClose { info } => {
                                self.set_exception_end_for_sub_spans(
                                    info.t_id,
                                    batch_inserter,
                                    info.record_time,
                                )?;
                                let Some(created_span) = self.remove_created_span(info.t_id) else {
                                    continue;
                                };
                                if let Some(parent_span_t_id) = created_span.parent_span_t_id {
                                    if let Some(created_span) =
                                        self.get_created_span_mut(parent_span_t_id)
                                    {
                                        if let Some(find) = created_span
                                            .sub_span_t_ids
                                            .iter_mut()
                                            .find(|n| n.0 == info.t_id)
                                        {
                                            find.2 = true;
                                        }
                                    }
                                }

                                let span_id = info.running_span.id;
                                let duration = info.record_time - created_span.record_time;
                                let idle_duration = duration - created_span.total_enter_duration;
                                let idle_duration = idle_duration.num_milliseconds() as f64 / 1000.;
                                let busy_duration =
                                    created_span.total_enter_duration.num_milliseconds() as f64
                                        / 1000.;
                                writeln!(
                                    batch_inserter,
                                    r#"update tracing_span_run set busy_duration = '{}',idle_duration='{}',close_record_id='{}' where id = '{}';"#,
                                    busy_duration, idle_duration, record.id, created_span.id,
                                )?;
                                Some(TracingTreeRecordVariantDto::SpanRun(TracingSpanRunDto {
                                    id: created_span.id,
                                    app_run_id: app_info.run_id,
                                    span_id,
                                    run_time: created_span.record_time.fixed_offset(),
                                    busy_duration: Some(busy_duration),
                                    idle_duration: Some(idle_duration),
                                    record_id: created_span.record_id,
                                    close_record_id: Some(record.id),
                                    exception_end: Default::default(),
                                    run_elapsed: Some(0.),
                                    fields: Arc::new(info.fields.clone().into_json_map()),
                                    related_events: Default::default(),
                                }))
                            }
                            TracingRecordVariant::SpanRecord { info } => {
                                let Some(created_span) = self.get_created_span_mut(info.t_id)
                                else {
                                    continue;
                                };
                                {
                                    cur_record_field_count += 1;
                                    match &mut created_span.last_record_filed_id {
                                        None => {
                                            created_span.last_record_filed_id = Some((
                                                record.id,
                                                core::mem::take(&mut info.fields),
                                            ));
                                        }
                                        Some((last_record_filed_id, fields)) => {
                                            *last_record_filed_id = record.id;
                                            fields.insert_other(&mut info.fields);
                                        }
                                    }
                                }

                                if !matches!(created_span.last_record_filed_id,Some((i,_))if i == record.id)
                                {
                                    continue;
                                }
                                let (_, fields) = created_span.last_record_filed_id.take().unwrap();
                                let created_span_id = created_span.id;

                                let (r, fields) = TracingRecordVariant::scoped_json_fields_by(
                                    Some(fields),
                                    |json_fields| {
                                        json_buf.clear();
                                        serde_json::to_writer(&mut json_buf, json_fields)?;
                                        let json = String::from_utf8_lossy(json_buf.as_slice());
                                        let quote = "$$";
                                        let quote  = if json.contains(quote) {
                                            "$UU21c99cb0-e891-4764-a6c6-8b83a468bff7UU$"
                                        }else {
                                            quote
                                        };
                                        anyhow::Ok(writeln!(
                                            batch_inserter,
                                            r#"update tracing_span_run set fields = fields||{quote}{}{quote}::jsonb where id = '{}';"#,
                                            json,
                                            created_span_id,
                                        )?)
                                    },
                                );
                                r?;
                                info.fields = fields.unwrap();
                                None
                            }
                            TracingRecordVariant::SpanEnter { info } => {
                                let Some(created_span) = self.get_created_span_mut(info.t_id)
                                else {
                                    continue;
                                };
                                let id = Uuid::new_v4();
                                if let Some(_enter_span) =
                                    created_span.enter_span.replace(EnteredSpan {
                                        id,
                                        record_time: info.record_time,
                                        record_id: record.id,
                                    })
                                {
                                    warn!(
                                        "app {} span {} already enter! will replace old",
                                        info.app_info.id, info.running_span.id
                                    );
                                }
                                let enter_span = created_span.enter_span.as_mut().unwrap();
                                let created_span_id = created_span.id;
                                let enter_span_id = enter_span.id;
                                enter_span.record_id = record.id;
                                writeln!(
                                    batch_inserter,
                                    r#"insert into tracing_span_enter(id,span_run_id,enter_time,record_id) values ('{}','{}','{}','{}');"#,
                                    enter_span_id,
                                    created_span_id,
                                    info.record_time.fixed_offset(),
                                    record.id,
                                )?;

                                Some(TracingTreeRecordVariantDto::SpanEnter(
                                    TracingSpanEnterDto {
                                        id: enter_span_id,
                                        span_run_id: created_span_id,
                                        enter_time: info.record_time.fixed_offset(),
                                        enter_elapsed: Some(0.),
                                        already_run: None,
                                        duration: None,
                                        record_id: record.id,
                                        leave_record_id: None,
                                    },
                                ))
                            }
                            TracingRecordVariant::SpanLeave { info } => {
                                let Some(created_span) = self.get_created_span_mut(info.t_id)
                                else {
                                    continue;
                                };

                                let Some(enter_span) = created_span.enter_span.take() else {
                                    warn!(
                                        "app {} span {} not enter!",
                                        app_info.id, info.running_span.id
                                    );
                                    continue;
                                };
                                let duration = info.record_time - enter_span.record_time;
                                if let Some(r) =
                                    created_span.total_enter_duration.checked_add(&duration)
                                {
                                    created_span.total_enter_duration = r;
                                } else {
                                    warn!(
                                        "app {} span {} total_enter_duration overflow",
                                        app_info.id, info.running_span.id
                                    );
                                }
                                let created_span_id = created_span.id;
                                let enter_span_id = enter_span.id;

                                let duration_secs = (duration.num_milliseconds() as f64) / 1000.;
                                writeln!(
                                    batch_inserter,
                                    r#"update tracing_span_enter set duration = '{}',leave_record_id='{}' where id = '{}';"#,
                                    duration_secs, record.id, enter_span_id,
                                )?;
                                Some(TracingTreeRecordVariantDto::SpanEnter(
                                    TracingSpanEnterDto {
                                        id: enter_span_id,
                                        span_run_id: created_span_id,
                                        enter_time: enter_span.record_time.fixed_offset(),
                                        enter_elapsed: Some(0.),
                                        already_run: None,
                                        duration: Some(duration_secs),
                                        record_id: enter_span.record_id,
                                        leave_record_id: Some(record.id),
                                    },
                                ))
                            }
                            TracingRecordVariant::Event { .. } => None,
                            TracingRecordVariant::AppStart {
                                app_info,
                                record_time,
                                name,
                                fields,
                            } => {
                                let _r = self.tracing_service.try_insert_app(app_info.id).await?;
                                let _r = self
                                    .tracing_service
                                    .try_insert_app_build(app_info.clone(), name.to_string())
                                    .await?;
                                let app_run_dto = self
                                    .tracing_service
                                    .insert_app_run(
                                        app_info.clone(),
                                        fields.clone().into_json_value(),
                                        record_time.fixed_offset(),
                                        record.id as _,
                                    )
                                    .await?;
                                Some(TracingTreeRecordVariantDto::AppRun(app_run_dto))
                            }
                            TracingRecordVariant::AppStop {
                                app_info,
                                record_time,
                                exception_end,
                                ..
                            } => {
                                let created_spans = self.created_spans.clone();
                                for (t_id, created_span) in created_spans {
                                    set_exception_end_for_span_run(
                                        &created_span.id,
                                        batch_inserter,
                                        record_time.clone(),
                                    )?;
                                    self.set_exception_end_for_sub_spans(
                                        t_id,
                                        batch_inserter,
                                        record_time.clone(),
                                    )?;
                                }
                                let app_run_dto = self
                                    .tracing_service
                                    .update_app_run_stop_time(
                                        app_info.run_id,
                                        record_time.fixed_offset(),
                                        record.id as _,
                                        exception_end.then(|| true),
                                    )
                                    .await?;
                                Some(TracingTreeRecordVariantDto::AppRun(app_run_dto))
                            }
                        };
                        self.dto_buf.push((record, r));
                    }

                    let other_update_prepare_duration = instant.elapsed();
                    instant = Instant::now();

                    let other_update_execute_duration = instant.elapsed();
                    instant = Instant::now();

                    event_service.clear();
                    let buffed_field_record_count = cur_record_field_count;
                    cur_record_field_count = 0;
                    let notify_count = self.dto_buf.len();

                    let mut dto_buf = core::mem::take(&mut self.dto_buf);
                    try_join!(
                        self.record_batch_inserter.execute(&self.tracing_service.dc),
                        batch_inserter.execute(&self.tracing_service.dc),
                        async {
                            for (record, dto) in dto_buf.drain(..) {
                                // writeln!(&mut string,"notify: {record:#?}, record_id: {record_id:?}");
                                event_service.notify(record, dto).await;
                            }
                            Ok(())
                        }
                    )?;

                    core::mem::swap(&mut self.dto_buf, &mut dto_buf);
                    anyhow::Ok(())
                };

                try_join!(future_1, future_2)?;
            }
            core::mem::swap(&mut buf_records_1, &mut buf_records_2);
            assert_eq!(buf_records_1.len(),0);

            for x in buf_event_senders.drain(..) {
                event_service.add_record_watcher(x.sender, x.filter);
            }

            // let buf_prepare_duration = instant.elapsed();
            // instant = Instant::now();

            // println!("{}",string);
            let notify_duration = instant.elapsed();

            // info!(
            //     buffed_record_count,
            //     buffed_field_record_count,
            //     notify_count,
            //     ?buf_prepare_duration,
            //     ?records_insert_prepare_duration,
            //     ?records_insert_execute_duration,
            //     ?other_update_prepare_duration,
            //     ?other_update_execute_duration,
            //     ?notify_duration,
            //     "buf apply and notified"
            // );
        }
    }

    fn update_to_no_empty_children(
        batch_inserter: &mut SqlBatchExecutor,
        created_span: &mut CreatedSpan,
    ) -> Result<(AppRunRecord, Option<TracingTreeRecordVariantDto>), std::fmt::Error> {
        let mut info = SpanFullInfo {
            base: created_span.span_base_info.clone(),
            fields: Default::default(),
        };
        info.fields.update_to_no_empty_children();
        writeln!(
            batch_inserter,
            r#"update tracing_record set fields = fields||'{{"{}": {}}}'::jsonb where id = '{}';"#,
            FIELD_DATA_EMPTY_CHILDREN,
            false,
            created_span.record_id,
        )?;
        Ok((
            AppRunRecord {
                id: created_span.record_id,
                record_index: created_span.record_index,
                variant: TracingRecordVariant::SpanRecord { info },
            },
            None,
        ))
    }
}

#[inline(always)]
fn set_exception_end_for_span_run(
    id: &Uuid,
    batch_inserter: &mut SqlBatchExecutor,
    record_time: DateTime<Utc>,
) -> Result<(), std::fmt::Error> {
    writeln!(
        batch_inserter,
        r#"update tracing_span_run set exception_end='{}' where id = '{}';"#,
        record_time, id
    )
}
