use crate::event_service::EventService;
use crate::grpc_service::{
    SpanFullInfo, SpanFullInfoBase, SpanInfo, TracingFields, TracingRecordFlags,
    TracingServiceImpl, FIELD_DATA_EMPTY_CHILDREN, FIELD_DATA_FIRST_EVENT, FIELD_DATA_FLAGS,
    FIELD_DATA_REPEATED_COUNT,
};
use crate::record::{AppRunInfo, SpanId, TracingRecordVariant};
use crate::tracing_service::{
    AppRunDto, BigInt, TracingRecordBatchInserter, TracingRecordDto, TracingRecordFilter,
    TracingSpanEnterDto, TracingSpanRunDto, TracingTreeRecordDto, TracingTreeRecordVariantDto,
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
use std::env;
use std::fmt::Write;
use std::panic::Location;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tonic::Status;
use tracing::{error, info, warn};
use tracing_lv_proto::proto::FieldValue;
use uuid::Uuid;

const UN_SET_RECORD_ID: BigInt = -1;
struct EnteredSpan {
    id: Uuid,
    record_time: DateTime<Utc>,
    record_id: BigInt,
}

#[derive(Deref)]
struct CreatedSpan {
    id: Uuid,
    #[deref]
    span_base_info: Arc<SpanFullInfoBase>,
    parent_span_t_id: Option<u64>,
    total_enter_duration: chrono::Duration,
    enter_span: Option<EnteredSpan>,
    record_id: BigInt,
    last_record_filed_id: Option<(BigInt, TracingFields)>,
    last_repeated_event: Option<(BigInt, SmolStr, usize)>,
    sub_span_t_ids: SmallVec<[(u64, Uuid, bool); 8]>,
}

struct RunningApp {
    app_info: Arc<AppRunInfo>,
    created_spans: hashbrown::HashMap<u64, CreatedSpan>,
    last_repeated_event: Option<(BigInt, SmolStr, usize)>,
}

impl RunningApp {
    pub fn new(app_info: Arc<AppRunInfo>) -> Self {
        Self {
            app_info,
            created_spans: Default::default(),
            last_repeated_event: None,
        }
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

pub enum RunMsg {
    Record(TracingRecordVariant),
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

pub struct RunningApps {
    tracing_service: crate::tracing_service::TracingService,
    running_apps: hashbrown::HashMap<Uuid, RunningApp>,
    event_service: EventService,

    max_delay: Duration,
    buf_records: VecDeque<TracingRecordVariant>,
    dto_buf: Vec<(
        TracingRecordVariant,
        BigInt,
        Option<TracingTreeRecordVariantDto>,
    )>,
    cur_record_field_count: usize,
    max_buf_count: usize,

    batch_inserter: SqlBatchExecutor,
    record_batch_inserter: TracingRecordBatchInserter,
}

impl RunningApps {
    #[inline(always)]
    pub fn scoped_batch_inserter<U>(
        &mut self,
        f: impl FnOnce(&mut Self, &mut SqlBatchExecutor) -> U,
    ) -> U {
        let mut batch_inserter = core::mem::take(&mut self.batch_inserter);
        let r = f(self, &mut batch_inserter);
        core::mem::swap(&mut batch_inserter, &mut self.batch_inserter);
        r
    }
}

enum PreHandleResult {
    BufFilled,
    Continue,
    Pushed,
}
const POSTGRESQL_MAX_BIND_PARAM_COUNT: usize = 32767;
impl RunningApps {
    pub fn new(
        tracing_service: crate::tracing_service::TracingService,
        id: BigInt,
        max_buf_count: Option<usize>,
    ) -> Self {
        let max_buf_count_max_value =
            POSTGRESQL_MAX_BIND_PARAM_COUNT / TracingRecordBatchInserter::BIND_COL_COUNT;
        let max_buf_count = max_buf_count
            .unwrap_or(max_buf_count_max_value)
            .min(max_buf_count_max_value);
        Self {
            tracing_service,
            running_apps: hashbrown::HashMap::new(),
            event_service: Default::default(),
            max_delay: Duration::from_millis(
                env::var("RECORD_MAX_DELAY")
                    .ok()
                    .map(|n| n.parse::<u64>().ok())
                    .flatten()
                    .unwrap_or(200),
            ),
            buf_records: VecDeque::with_capacity(max_buf_count),
            dto_buf: vec![],
            cur_record_field_count: 0,
            max_buf_count,
            batch_inserter: SqlBatchExecutor::default(),
            record_batch_inserter: TracingRecordBatchInserter::new(id),
        }
    }

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
    }

    #[inline(always)]
    fn pre_handle_msg(&mut self, msg: RunMsg) -> PreHandleResult {
        match msg {
            RunMsg::Record(record) => {
                if let TracingRecordVariant::SpanCreate { info, .. } = &record {
                    let Some(running_app) = self.get_running_app_mut(&info.app_info) else {
                        return PreHandleResult::Continue;
                    };

                    if let Some(_previous) = running_app.created_spans.insert(
                        info.span_info.t_id,
                        CreatedSpan {
                            id: Uuid::new_v4(),
                            parent_span_t_id: info.running_span.parent_span_t_id,
                            total_enter_duration: Default::default(),
                            enter_span: None,
                            record_id: UN_SET_RECORD_ID,
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
                        return PreHandleResult::Continue;
                    }
                } else if let TracingRecordVariant::SpanEnter { info } = &record {
                    let Some(created_span) = self.get_created_span_mut(info) else {
                        return PreHandleResult::Continue;
                    };
                    let id = Uuid::new_v4();
                    if let Some(_enter_span) = created_span.enter_span.replace(EnteredSpan {
                        id,
                        record_time: info.record_time,
                        record_id: UN_SET_RECORD_ID,
                    }) {
                        warn!(
                            "app {} span {} already enter! will replace old",
                            info.app_info.id, info.running_span.id
                        );
                        return PreHandleResult::Continue;
                    }
                } else if let TracingRecordVariant::AppStart { app_info, .. } = &record {
                    if let Some(app) = self
                        .running_apps
                        .insert(app_info.run_id, RunningApp::new(app_info.clone()))
                    {
                        warn!("app {} already running!", app.app_info.id);
                        return PreHandleResult::Continue;
                    }
                }
                self.buf_records.push_back(record);
                if self.buf_records.len() >= self.max_buf_count + 1 {
                    PreHandleResult::BufFilled
                } else {
                    PreHandleResult::Pushed
                }
            }
            RunMsg::AddRecordWatcher { sender, filter } => {
                self.event_service.add_record_event_sender(sender, filter);
                PreHandleResult::Continue
            }
        }
    }

    pub async fn handle_records(
        &mut self,
        record_receiver: flume::Receiver<RunMsg>,
    ) -> anyhow::Result<()> {
        let mut json_buf = Vec::new();
        loop {
            let mut delay_future = std::pin::pin!(tokio::time::sleep(self.max_delay));
            match self.pre_handle_msg(record_receiver.recv_async().await?) {
                PreHandleResult::BufFilled => {}
                PreHandleResult::Continue => continue,
                PreHandleResult::Pushed => {}
            }
            let mut instant = Instant::now();
            loop {
                tokio::select! {
                    _ = &mut delay_future => {
                        break;
                    }
                    msg = record_receiver.recv_async() => {
                        match self.pre_handle_msg(msg?) {
                            PreHandleResult::BufFilled => {
                                break;
                            }
                            PreHandleResult::Continue => {},
                            PreHandleResult::Pushed => {}
                        }
                    }
                }
            }
            let buf_prepare_duration = instant.elapsed();
            instant = Instant::now();

            let mut records_iter = self.buf_records.iter_mut();
            let mut cur_record_id = self.record_batch_inserter.id;
            while let Some(record) = records_iter.next() {
                cur_record_id += 1;
                if let TracingRecordVariant::Event {
                    span_info,
                    message,
                    app_info,
                    is_repeated_event,
                    ..
                } = record
                {
                    if let Some(running_app) = self.running_apps.get_mut(&app_info.run_id) {
                        let last_repeated_event = match span_info {
                            None => Some(&mut running_app.last_repeated_event),
                            Some(span_info) => running_app
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
                                writeln!(
                                    self.batch_inserter,
                                    r#"update tracing_record set fields = fields||'{{"{}":{}}}'::jsonb where id = '{}';"#,
                                    FIELD_DATA_REPEATED_COUNT, *repeated_count, repeated_record_id,
                                )?;
                            } else {
                                *last_repeated_event = Some((cur_record_id, message.clone(), 1));
                            }
                            if first_event {
                                writeln!(
                                    self.batch_inserter,
                                    r#"update tracing_record set fields = fields||'{{"{}":true}}'::jsonb where id = '{}';"#,
                                    FIELD_DATA_FIRST_EVENT, cur_record_id,
                                )?;
                                if let Some(span_info) = span_info {
                                    if let Some(created_span) =
                                        running_app.get_created_span_mut(span_info.t_id)
                                    {
                                        let dto = Self::update_to_no_empty_children(
                                            &mut self.batch_inserter,
                                            created_span,
                                        )?;
                                        self.dto_buf.push(dto);
                                    }
                                }
                            }
                        }
                    }
                } else if let TracingRecordVariant::SpanCreate { info, .. } = record {
                    let Some(running_app) = self.running_apps.get_mut(&info.app_info.run_id) else {
                        continue;
                    };
                    let Some(created_span) = running_app.get_created_span_mut(info.t_id) else {
                        continue;
                    };
                    created_span.record_id = cur_record_id;
                }

                let record_time = record.record_time();
                let record_time = record_time.fixed_offset();
                let kind = record.kind();
                let level = record.level();
                let span_id = record.span_id();
                let parent_id = record.parent_id();
                let parent_span_t_id = record.parent_span_t_id();

                record
                    .scoped_json_fields(|record, json_fields| {
                        let target = record.target();
                        let module_path = record.module_path();
                        let file_line = record.file_line();
                        let app_info = record.app_info();
                        self.record_batch_inserter.append_insert_record(
                            record.name(),
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
                debug_assert_eq!(cur_record_id, self.record_batch_inserter.id);

                if let TracingRecordVariant::SpanRecord { info } = record {
                    self.cur_record_field_count += 1;
                    let Some(running_app) = self.running_apps.get_mut(&info.app_info.run_id) else {
                        continue;
                    };
                    let Some(created_span) = running_app.get_created_span_mut(info.t_id) else {
                        continue;
                    };
                    match &mut created_span.last_record_filed_id {
                        None => {
                            created_span.last_record_filed_id =
                                Some((cur_record_id, core::mem::take(&mut info.fields)));
                        }
                        Some((last_record_filed_id, fields)) => {
                            *last_record_filed_id = cur_record_id;
                            fields.insert_other(&mut info.fields);
                        }
                    }
                }
            }
            let records_insert_prepare_duration = instant.elapsed();
            instant = Instant::now();

            let ids = self
                .record_batch_inserter
                .execute(&self.tracing_service.dc)
                .await?;

            let records_insert_execute_duration = instant.elapsed();
            instant = Instant::now();

            let buffed_record_count = ids.end - ids.start;
            debug_assert_eq!(buffed_record_count, self.buf_records.len() as i64);
            self.dto_buf.reserve_exact(self.buf_records.len());
            for record_id in ids {
                let mut record = self.buf_records.pop_front().unwrap();
                let app_info = record.app_info().clone();
                let r = match &mut record {
                    TracingRecordVariant::SpanCreate { info, .. } => {
                        let Some(running_app) = self.running_apps.get_mut(&info.app_info.run_id)
                        else {
                            warn!("app {} not running", app_info.id);
                            continue;
                        };
                        let Some(created_span) = running_app.get_created_span_mut(info.t_id) else {
                            continue;
                        };
                        // created_span.record_id = record_id;
                        let created_span_id = created_span.id;
                        if let Some(parent_span_t_id) = created_span.parent_span_t_id {
                            if let Some(created_span) =
                                running_app.get_created_span_mut(parent_span_t_id)
                            {
                                if created_span.sub_span_t_ids.is_empty()
                                    && created_span.last_repeated_event.is_none()
                                {
                                    self.dto_buf.push(Self::update_to_no_empty_children(
                                        &mut self.batch_inserter,
                                        created_span,
                                    )?);
                                }
                                created_span.sub_span_t_ids.push((
                                    info.t_id,
                                    created_span_id,
                                    false,
                                ));
                            }
                        }

                        if let Some(parent_span_id) = info.running_span.parent {
                            write!(
                                self.batch_inserter,
                                r#"insert into tracing_span_parent(span_id, span_parent_id) values ('{}','{}') on conflict do nothing;"#,
                                info.running_span.id, parent_span_id
                            )?;
                        }

                        let run_time = info.record_time.fixed_offset();
                        let span_id = info.running_span.id;
                        writeln!(
                            self.batch_inserter,
                            r#"insert into tracing_span_run(id,app_run_id,span_id,run_time,record_id,fields) values ('{}','{}','{}','{}','{}','{{}}'::jsonb);"#,
                            created_span_id, app_info.run_id, span_id, run_time, record_id
                        )?;

                        Some(TracingTreeRecordVariantDto::SpanRun(TracingSpanRunDto {
                            id: created_span_id,
                            app_run_id: app_info.run_id,
                            span_id,
                            run_time: info.record_time.fixed_offset(),
                            busy_duration: None,
                            idle_duration: None,
                            record_id,
                            close_record_id: None,
                            exception_end: Default::default(),
                            run_elapsed: Some(0.),
                            fields: Arc::new(info.fields.clone().into_json_map()),
                        }))
                    }
                    TracingRecordVariant::SpanClose { info } => {
                        self.scoped_batch_inserter(|me, batch_inserter| {
                            let running_app = me.get_running_app_mut(&info.app_info).unwrap();
                            set_exception_end_for_sub_spans(
                                info.t_id,
                                batch_inserter,
                                running_app,
                                info.record_time,
                            )
                        })?;
                        let Some(running_app) = self.get_running_app_mut(&info.app_info) else {
                            continue;
                        };
                        let Some(created_span) = running_app.remove_created_span(info.t_id) else {
                            continue;
                        };
                        if let Some(parent_span_t_id) = created_span.parent_span_t_id {
                            if let Some(created_span) =
                                running_app.get_created_span_mut(parent_span_t_id)
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
                            created_span.total_enter_duration.num_milliseconds() as f64 / 1000.;
                        writeln!(
                            self.batch_inserter,
                            r#"update tracing_span_run set busy_duration = '{}',idle_duration='{}',close_record_id='{}' where id = '{}';"#,
                            busy_duration, idle_duration, record_id, created_span.id,
                        )?;
                        Some(TracingTreeRecordVariantDto::SpanRun(TracingSpanRunDto {
                            id: created_span.id,
                            app_run_id: app_info.run_id,
                            span_id,
                            run_time: created_span.record_time.fixed_offset(),
                            busy_duration: Some(busy_duration),
                            idle_duration: Some(idle_duration),
                            record_id: created_span.record_id,
                            close_record_id: Some(record_id),
                            exception_end: Default::default(),
                            run_elapsed: Some(0.),
                            fields: Arc::new(info.fields.clone().into_json_map()),
                        }))
                    }
                    TracingRecordVariant::SpanRecord { info } => {
                        let Some(created_span) = self.get_created_span_mut(info) else {
                            continue;
                        };

                        if !matches!(created_span.last_record_filed_id,Some((i,_))if i == record_id)
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
                                anyhow::Ok(writeln!(
                                    self.batch_inserter,
                                    r#"update tracing_span_run set fields = fields||'{}'::jsonb where id = '{}';"#,
                                    String::from_utf8_lossy(json_buf.as_slice()),
                                    created_span_id,
                                )?)
                            },
                        );
                        r?;
                        info.fields = fields.unwrap();
                        None
                    }
                    TracingRecordVariant::SpanEnter { info } => {
                        let Some(created_span) = self.get_created_span_mut(info) else {
                            continue;
                        };
                        let Some(enter_span) = created_span.enter_span.as_mut() else {
                            warn!(
                                "app {} span {} not enter",
                                app_info.id, info.running_span.id
                            );
                            continue;
                        };
                        let created_span_id = created_span.id;
                        let enter_span_id = enter_span.id;
                        enter_span.record_id = record_id;
                        writeln!(
                            self.batch_inserter,
                            r#"insert into tracing_span_enter(id,span_run_id,enter_time,record_id) values ('{}','{}','{}','{}');"#,
                            enter_span_id,
                            created_span_id,
                            info.record_time.fixed_offset(),
                            record_id,
                        )?;

                        Some(TracingTreeRecordVariantDto::SpanEnter(
                            TracingSpanEnterDto {
                                id: enter_span_id,
                                span_run_id: created_span_id,
                                enter_time: info.record_time.fixed_offset(),
                                enter_elapsed: Some(0.),
                                already_run: None,
                                duration: None,
                                record_id,
                                leave_record_id: None,
                            },
                        ))
                    }
                    TracingRecordVariant::SpanLeave { info } => {
                        let Some(created_span) = self.get_created_span_mut(info) else {
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
                        if let Some(r) = created_span.total_enter_duration.checked_add(&duration) {
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
                            self.batch_inserter,
                            r#"update tracing_span_enter set duration = '{}',leave_record_id='{}' where id = '{}';"#,
                            duration_secs, record_id, enter_span_id,
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
                                leave_record_id: Some(record_id),
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
                                record_id.clone(),
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
                        let Some(mut running_app) = self.remove_running_app_mut(&app_info) else {
                            continue;
                        };
                        let created_spans = core::mem::take(&mut running_app.created_spans);
                        for (t_id, created_span) in created_spans {
                            set_exception_end_for_span_run(
                                &created_span.id,
                                &mut self.batch_inserter,
                                record_time.clone(),
                            )?;
                            set_exception_end_for_sub_spans(
                                t_id,
                                &mut self.batch_inserter,
                                &mut running_app,
                                record_time.clone(),
                            )?;
                        }
                        let app_run_dto = self
                            .tracing_service
                            .update_app_run_stop_time(
                                app_info.run_id,
                                record_time.fixed_offset(),
                                record_id,
                                (*exception_end).then(|| true),
                            )
                            .await?;
                        Some(TracingTreeRecordVariantDto::AppRun(app_run_dto))
                    }
                };
                self.dto_buf.push((record, record_id, r));
            }

            let other_update_prepare_duration = instant.elapsed();
            instant = Instant::now();

            self.batch_inserter
                .execute(&self.tracing_service.dc)
                .await?;

            let other_update_execute_duration = instant.elapsed();
            instant = Instant::now();

            self.event_service.clear();
            let buffed_field_record_count = self.cur_record_field_count;
            self.cur_record_field_count = 0;
            let notify_count = self.dto_buf.len();
            // let mut string = "".to_string();
            for (record, record_id, dto) in self.dto_buf.drain(..) {
                // writeln!(&mut string,"notify: {record:#?}, record_id: {record_id:?}");
                self.event_service.notify(record, record_id, dto).await;
            }
            // println!("{}",string);
            let notify_duration = instant.elapsed();

            info!(
                buffed_record_count,
                buffed_field_record_count,
                notify_count,
                ?buf_prepare_duration,
                ?records_insert_prepare_duration,
                ?records_insert_execute_duration,
                ?other_update_prepare_duration,
                ?other_update_execute_duration,
                ?notify_duration,
                "buf apply and notified"
            );
        }
    }

    fn update_to_no_empty_children(
        batch_inserter: &mut SqlBatchExecutor,
        created_span: &mut CreatedSpan,
    ) -> Result<
        (
            TracingRecordVariant,
            BigInt,
            Option<TracingTreeRecordVariantDto>,
        ),
        std::fmt::Error,
    > {
        let info = SpanFullInfo {
            base: created_span.span_base_info.clone(),
            fields: Default::default(),
        };
        writeln!(
            batch_inserter,
            r#"update tracing_record set fields = fields||'{{"{}": {}}}'::jsonb where id = '{}';"#,
            FIELD_DATA_EMPTY_CHILDREN, false, created_span.record_id,
        )?;
        Ok((
            TracingRecordVariant::SpanRecord { info },
            created_span.record_id,
            None,
        ))
    }
}

#[track_caller]
#[inline(always)]
fn set_exception_end_for_sub_spans(
    span_t_id: u64,
    batch_inserter: &mut SqlBatchExecutor,
    running_app: &mut RunningApp,
    record_time: DateTime<Utc>,
) -> Result<(), std::fmt::Error> {
    let Some(created_span) = running_app.get_created_span_mut(span_t_id) else {
        return Ok(());
    };

    let sub_span_t_ids = core::mem::take(&mut created_span.sub_span_t_ids);
    for (sub_span_t_id, id, _) in sub_span_t_ids.iter().filter(|n| !n.2) {
        set_exception_end_for_span_run(id, batch_inserter, record_time)?;
        set_exception_end_for_sub_spans(*sub_span_t_id, batch_inserter, running_app, record_time)?;
    }
    Ok(())
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
