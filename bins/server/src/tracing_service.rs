use crate::dyn_query::{TLBinOp, TableColumnInfo, TableInfo};
use crate::global_data::GLOBAL_DATA;
use crate::grpc_service::{
    TracingServiceImpl, FIELD_DATA_LAST_REPEATED_TIME, FIELD_DATA_REPEATED_COUNT,
    FIELD_DATA_SPAN_T_ID, FIELD_DATA_STABLE_SPAN_ID,
};
use crate::record::{AppRunInfo, SpanCacheId, SpanId, TracingKind, TracingRecordVariant};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Utc};
use entity::app_build::{ActiveModel, Column};
use entity::tracing_span::Entity;
use entity::*;
use sea_orm::prelude::{BigDecimal, Decimal, Expr, Json, RcOrArc, StringLen};
use sea_orm::sea_query::extension::postgres::{IntoTypeRef, PgExpr};
use sea_orm::sea_query::{
    Alias, ArrayType, BinOper, ColumnRef, Cond, ConditionExpression, ConditionType, FunctionCall,
    IntoColumnRef, IntoCondition, IntoTableRef, PgInterval, SimpleExpr, Table, UnOper,
};
use sea_orm::sqlx::error::BoxDynError;
use sea_orm::sqlx::postgres::{PgArguments, PgRow};
use sea_orm::sqlx::{Arguments, Encode, Error, Postgres, Row, Type};
use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::{
    sqlx, ColumnTrait, ColumnType, Condition, ConnectionTrait, DbBackend, DynIden, IdenStatic,
    Iterable, LoaderTrait, PaginatorTrait, QueryOrder, QuerySelect, QueryTrait, SelectColumns,
    Statement, Value,
};
use sea_orm::{DatabaseConnection, DbErr, EntityTrait, InsertResult, QueryFilter, TryInsertResult};
use sea_orm_migration::SchemaManager;
use sea_query_binder::SqlxValues;
use serde::{Deserialize, Deserializer, Serialize};
use smallvec::SmallVec;
use smol_str::{SmolStr, ToSmolStr};
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::{Add, Range};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::time::FutureExt as _;
use tracing::{error, info, instrument, warn};
use tracing_lv_proto::proto::{FieldValue, Level};
use tracing_subscriber::filter::FilterExt;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

pub type BigInt = i64;

const INSERT_STR: &'static str = r#"
insert into tracing_record(id,app_id, app_version, app_run_id, node_id, name, record_time, kind, level, span_id, parent_span_t_id, parent_id, fields, target, module_path, position_info)
values
"#;

pub struct TracingRecordBatchInserter {
    sql: String,
    args: Option<PgArguments>,
    placeholder_num: usize,
    pub id: i64,
}

impl TracingRecordBatchInserter {
    pub fn new(id: i64) -> Self {
        Self {
            sql: INSERT_STR.to_string(),
            args: Some(PgArguments::default()),
            placeholder_num: 0,
            id,
        }
    }
}

impl TracingRecordBatchInserter {
    pub const BIND_COL_COUNT: usize = 8;
    pub async fn execute(&mut self, dc: &DatabaseConnection) -> Result<Range<BigInt>, Error> {
        if self.placeholder_num == 0 {
            return Ok(0..0);
        }
        let count = self.placeholder_num / Self::BIND_COL_COUNT;
        self.sql.pop();
        sqlx::query_with(self.sql.as_str(), self.args.take().unwrap())
            .execute(dc.get_postgres_connection_pool())
            .await
            .inspect_err(|err| {
                error!(
                    sql = self.sql,
                    placeholder_num = self.placeholder_num,
                    "Failed to execute query: {}",
                    err
                );
            })
            .map(|_n| {
                self.sql.clear();
                self.sql.push_str(INSERT_STR);
                self.placeholder_num = 0;
                self.args = Some(Default::default());
            })?;
        Ok(((self.id + 1) - (count as i64))..(self.id + 1))
    }

    #[inline(always)]
    pub fn append_insert_record(
        &mut self,
        name: &str,
        record_time: &DateTime<FixedOffset>,
        kind: &str,
        level: Option<Level>,
        span_id: Option<SpanId>,
        parent_span_t_id: Option<BigInt>,
        parent: Option<SpanId>,
        fields: Option<&serde_json::Value>,
        target: Option<&str>,
        module_path: Option<&str>,
        position_info: Option<&str>,
        app_info: &AppRunInfo,
    ) -> Result<(), BoxDynError> {
        use std::fmt::Write;
        self.sql.push_str("(");
        // for _ in 0..Self::COL_COUNT {
        //     self.placeholder_num += 1;
        //     write!(&mut self.sql, "${},", self.placeholder_num)?;
        // }
        self.id += 1;
        {
            write!(&mut self.sql, "'{}',", self.id)?;
            write!(&mut self.sql, "'{}',", app_info.id)?;
            self.add_arg(app_info.version.as_str())?;
            write!(&mut self.sql, "'{}',", app_info.run_id)?;
            self.add_arg(app_info.node_id.as_str())?;
            self.add_arg(name)?;
            write!(&mut self.sql, "'{}',", record_time)?;
            self.add_arg(kind)?;
            match level.map(|n| n as i32) {
                None => {
                    write!(&mut self.sql, "NULL,")?;
                }
                Some(level) => {
                    write!(&mut self.sql, "'{}',", level)?;
                }
            }
            match span_id {
                None => {
                    write!(&mut self.sql, "NULL,")?;
                }
                Some(value) => {
                    write!(&mut self.sql, "'{}',", value)?;
                }
            }
            match parent_span_t_id {
                None => {
                    write!(&mut self.sql, "NULL,")?;
                }
                Some(value) => {
                    write!(&mut self.sql, "{},", value)?;
                }
            }
            match parent {
                None => {
                    write!(&mut self.sql, "NULL,")?;
                }
                Some(value) => {
                    write!(&mut self.sql, "'{}',", value)?;
                }
            }
            self.add_arg(fields)?;
            self.add_arg(target)?;
            self.add_arg(module_path)?;
            self.add_arg(position_info)?;
        }
        self.sql.pop();
        self.sql.push_str("),");

        Ok(())
    }

    #[inline(always)]
    fn add_arg<'q, T>(&mut self, value: T) -> Result<(), BoxDynError>
    where
        T: Encode<'q, Postgres> + Type<Postgres> + 'q,
    {
        self.placeholder_num += 1;
        write!(&mut self.sql, "${},", self.placeholder_num)?;
        let args = self.args.as_mut().unwrap();
        args.add(value)?;
        Ok(())
    }
    //
    // #[inline(always)]
    // fn add_arg_without_bind<'q, T>(&mut self,value: impl std::fmt::Write)-> Result<(), BoxDynError>
    // {
    //     self.placeholder_num += 1;
    //     write!(&mut self.sql, "{},", value)?;
    //     let args = self.args.as_mut().unwrap();
    //     args.add(value)?;
    //     Ok(())
    // }
}

#[derive(Serialize, Deserialize, PartialEq, Copy, Clone, Debug, ToSchema)]
pub enum TracingLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<Level> for TracingLevel {
    fn from(value: Level) -> Self {
        match value {
            Level::Trace => TracingLevel::Trace,
            Level::Debug => TracingLevel::Debug,
            Level::Info => TracingLevel::Info,
            Level::Warn => TracingLevel::Warn,
            Level::Error => TracingLevel::Error,
        }
    }
}
impl From<TracingLevel> for Level {
    fn from(value: TracingLevel) -> Self {
        match value {
            TracingLevel::Trace => Level::Trace,
            TracingLevel::Debug => Level::Debug,
            TracingLevel::Info => Level::Info,
            TracingLevel::Warn => Level::Warn,
            TracingLevel::Error => Level::Error,
        }
    }
}

#[derive(Serialize, Debug, ToSchema)]
pub struct AppLatestRunInfoDto {
    pub id: Uuid,
    pub run_time: DateTime<FixedOffset>,
    #[schema(value_type = Object)]
    pub data: Option<Json>,
}

#[derive(Serialize, Debug, ToSchema)]
pub struct AppLatestInfoDto {
    pub id: Uuid,
    #[schema(value_type = String)]
    pub name: SmolStr,
    pub build_time: DateTime<FixedOffset>,
    // pub run_info: Option<AppLatestRunInfoDto>,
    pub node_count: u64,
    pub online_node_count: u64,
}
#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct TracingRecordDto {
    pub id: BigInt,
    pub app_id: Uuid,
    #[schema(value_type = String)]
    pub app_version: SmolStr,
    pub app_run_id: Uuid,
    #[schema(value_type = String)]
    pub node_id: SmolStr,
    #[schema(value_type = String)]
    pub name: SmolStr,
    pub kind: TracingKind,
    pub level: Option<TracingLevel>,
    pub span_id: Option<Uuid>,
    #[schema(value_type = Object)]
    pub fields: Arc<serde_json::Map<String, serde_json::Value>>,
    #[schema(value_type = Option<String>)]
    pub target: Option<SmolStr>,
    #[schema(value_type = Option<String>)]
    pub module_path: Option<SmolStr>,
    #[schema(value_type = Option<String>)]
    pub position_info: Option<SmolStr>,
    pub record_time: DateTime<FixedOffset>,
    pub creation_time: DateTime<FixedOffset>,
    pub span_id_is_stable: Option<bool>,
    pub parent_id: Option<Uuid>,
    // JSON does not support u64
    #[schema(value_type = Option<String>)]
    pub span_t_id: Option<SmolStr>,
    // JSON does not support u64
    #[schema(value_type = Option<String>)]
    pub parent_span_t_id: Option<SmolStr>,
    pub repeated_count: Option<u32>,
}

#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct TracingSpanRunDto {
    pub id: Uuid,
    pub app_run_id: Uuid,
    pub span_id: Uuid,
    pub run_time: DateTime<FixedOffset>,
    pub busy_duration: Option<f64>,
    pub idle_duration: Option<f64>,
    pub record_id: i64,
    pub close_record_id: Option<i64>,
    pub exception_end: Option<DateTime<FixedOffset>>,
    pub run_elapsed: Option<f64>,
    #[schema(value_type = Object)]
    pub fields: Arc<serde_json::Map<String, serde_json::Value>>,
}

impl From<tracing_span_run::Model> for TracingSpanRunDto {
    fn from(n: tracing_span_run::Model) -> Self {
        TracingSpanRunDto {
            id: n.id,
            app_run_id: n.app_run_id,
            span_id: n.span_id,
            run_elapsed: GLOBAL_DATA
                .get_node_now_timestamp_nanos(n.app_run_id)
                .map(|node_now| {
                    Duration::from_nanos(
                        (node_now - n.run_time.timestamp_nanos_opt().unwrap()) as u64,
                    )
                    .as_millis_f64()
                }),
            run_time: n.run_time,
            busy_duration: n.busy_duration,
            idle_duration: n.idle_duration,
            record_id: n.record_id,
            close_record_id: n.close_record_id,
            exception_end: n.exception_end,
            fields: Arc::new(
                n.fields
                    .map(|n| {
                        let serde_json::Value::Object(map) = n else {
                            unreachable!()
                        };
                        map
                    })
                    .unwrap_or_default(),
            ),
        }
    }
}
#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct TracingSpanEnterDto {
    pub id: Uuid,
    pub span_run_id: Uuid,
    pub enter_time: DateTime<FixedOffset>,
    pub enter_elapsed: Option<f64>,
    pub already_run: Option<Duration>,
    pub duration: Option<f64>,
    pub record_id: i64,
    pub leave_record_id: Option<i64>,
}

impl From<tracing_span_enter::Model> for TracingSpanEnterDto {
    fn from(n: tracing_span_enter::Model) -> Self {
        TracingSpanEnterDto {
            id: n.id,
            span_run_id: n.span_run_id,
            enter_time: n.enter_time,
            enter_elapsed: None,
            already_run: None,
            duration: n.duration,
            record_id: n.record_id,
            leave_record_id: n.leave_record_id,
        }
    }
}

#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct TracingTreeEndDto {
    end_date: DateTime<FixedOffset>,
    exception_end: Option<DateTime<FixedOffset>>,
}

impl From<&TracingSpanRunDto> for Option<TracingTreeEndDto> {
    fn from(value: &TracingSpanRunDto) -> Self {
        if let Some(end_date) = value.exception_end {
            return Some(TracingTreeEndDto {
                end_date,
                exception_end: value.exception_end,
            });
        }
        let busy_duration = value.busy_duration?;
        let idle_duration = value.idle_duration?;
        Some(TracingTreeEndDto {
            end_date: value.run_time.add(
                TimeDelta::try_milliseconds(
                    (busy_duration * 1_000.0 + idle_duration * 1_000.0) as i64,
                )
                .unwrap_or_else(|| {
                    warn!(
                        "TimeDelta::try_milliseconds error. busy_duration: {}. idle_duration: {}",
                        busy_duration, idle_duration
                    );
                    Default::default()
                }),
            ),
            exception_end: value.exception_end,
        })
    }
}
impl From<&TracingSpanEnterDto> for Option<TracingTreeEndDto> {
    fn from(value: &TracingSpanEnterDto) -> Self {
        let duration = value.duration?;
        Some(TracingTreeEndDto {
            end_date: value.enter_time.add(
                TimeDelta::try_milliseconds((duration * 1_000.0) as i64).unwrap_or_else(|| {
                    warn!("TimeDelta::try_milliseconds error. duration: {}", duration);
                    Default::default()
                }),
            ),
            exception_end: None,
        })
    }
}

impl From<&AppRunDto> for Option<TracingTreeEndDto> {
    fn from(value: &AppRunDto) -> Self {
        Some(TracingTreeEndDto {
            end_date: value.stop_time?,
            exception_end: value
                .exception_end
                .then(|| value.stop_time.unwrap_or(value.creation_time)),
        })
    }
}

#[derive(Serialize, Clone, Debug, ToSchema)]
pub struct TracingTreeRecordDto {
    pub record: TracingRecordDto,
    pub variant: Option<TracingTreeRecordVariantDto>,
    pub end: Option<TracingTreeEndDto>,
}

impl TracingTreeRecordDto {
    pub fn new(record: TracingRecordDto, variant: Option<TracingTreeRecordVariantDto>) -> Self {
        let end = variant
            .as_ref()
            .map(|n| {
                let r: Option<TracingTreeEndDto> = n.into();
                r
            })
            .flatten();
        Self {
            variant,
            record,
            end,
        }
    }
}

#[derive(Serialize, Clone, Debug, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TracingTreeRecordVariantDto {
    SpanRun(TracingSpanRunDto),
    SpanEnter(TracingSpanEnterDto),
    AppRun(AppRunDto),
}
impl From<&TracingTreeRecordVariantDto> for Option<TracingTreeEndDto> {
    fn from(value: &TracingTreeRecordVariantDto) -> Self {
        match value {
            TracingTreeRecordVariantDto::SpanRun(n) => n.into(),
            TracingTreeRecordVariantDto::AppRun(n) => n.into(),
            TracingTreeRecordVariantDto::SpanEnter(n) => n.into(),
        }
    }
}

#[derive(Default, Clone, Debug, Deserialize, IntoParams, ToSchema)]
pub struct CursorInfo {
    pub id: BigInt,
    pub is_before: bool,
}
#[derive(Clone, Debug, Deserialize, ToSchema)]
pub enum TracingRecordScene {
    Tree,
    SpanField,
    SpanEnter,
}

#[derive(Default, Clone, Debug, Deserialize, IntoParams, ToSchema)]
pub struct TracingRecordFilter {
    pub cursor: Option<CursorInfo>,
    pub count: Option<u64>,
    pub search: Option<String>,
    pub scene: Option<TracingRecordScene>,
    pub app_build_ids: Option<SmallVec<[(Uuid, Option<String>); 2]>>,
    pub app_run_ids: Option<SmallVec<[Uuid; 2]>>,
    pub node_ids: Option<SmallVec<[String; 2]>>,
    pub parent_id: Option<Uuid>,
    pub parent_span_t_id: Option<u64>,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
    pub kinds: Option<SmallVec<[TracingKind; 2]>>,
    pub span_ids: Option<SmallVec<[Uuid; 2]>>,
    pub targets: Option<SmallVec<[(TLBinOp, String); 2]>>,
    pub name: Option<(TLBinOp, String)>,
    pub fields: Option<SmallVec<[TracingRecordFieldFilter; 2]>>,
    pub levels: Option<SmallVec<[TracingLevel; 2]>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct TracingRecordFieldFilter {
    pub name: String,
    pub op: TLBinOp,
    pub value: Option<String>,
}

impl TracingRecordFieldFilter {
    pub fn with_prefix(self, prefix: &str) -> Self {
        Self {
            name: format!("{}{}", prefix, self.name),
            op: self.op,
            value: self.value,
        }
    }
}

#[derive(Default, Clone, Debug, Deserialize, IntoParams, ToSchema)]
pub struct AppNodeFilter {
    pub main_app_id: Option<Uuid>,
    pub after_record_id: Option<BigInt>,
    pub app_build_ids: Option<SmallVec<[(Uuid, Option<String>); 2]>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct AppRunDto {
    pub id: Uuid,
    pub app_id: Uuid,
    #[schema(value_type = String)]
    pub app_version: SmolStr,
    #[schema(value_type = Object)]
    pub data: Arc<serde_json::Map<String, serde_json::Value>>,
    pub creation_time: DateTime<FixedOffset>,
    pub record_id: BigInt,
    pub stop_record_id: Option<BigInt>,
    pub start_time: DateTime<FixedOffset>,
    pub stop_time: Option<DateTime<FixedOffset>>,
    pub exception_end: bool,
    pub run_elapsed: Option<f64>,
}

impl From<app_run::Model> for AppRunDto {
    fn from(n: app_run::Model) -> Self {
        AppRunDto {
            id: n.id,
            app_id: n.app_id,
            app_version: n.app_version.into(),
            run_elapsed: GLOBAL_DATA
                .get_node_now_timestamp_nanos(n.id)
                .map(|node_now| {
                    Duration::from_nanos(
                        (node_now - n.start_time.timestamp_nanos_opt().unwrap()) as u64,
                    )
                    .as_millis_f64()
                }),
            data: Arc::new(if let Some(data) = n.data {
                let serde_json::Value::Object(data) = data else {
                    unreachable!("should is object json");
                };
                data
            } else {
                Default::default()
            }),
            creation_time: n.creation_time,
            record_id: n.record_id,
            stop_record_id: n.stop_record_id,
            start_time: n.start_time,
            stop_time: n.stop_time,
            exception_end: n.exception_end.unwrap_or_default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct AppNodeRunDto {
    pub app_run_id: Uuid,
    pub app_build_ids: SmallVec<[(Uuid, String); 2]>,
    #[schema(value_type = String)]
    pub node_id: SmolStr,
    #[schema(value_type = Object)]
    pub data: Arc<serde_json::Map<String, serde_json::Value>>,
    pub creation_time: DateTime<FixedOffset>,
    pub record_id: BigInt,
    pub stop_record_id: Option<BigInt>,
    pub start_time: DateTime<FixedOffset>,
    pub stop_time: Option<DateTime<FixedOffset>>,
    pub exception_end: bool,
}

#[derive(Serialize, Debug, ToSchema)]
pub struct TracingSpanDto {
    pub id: Uuid,
    pub position_info: String,
    pub name: String,
    pub app_id: Uuid,
    pub app_version: String,
}

#[derive(Clone)]
pub struct TracingService {
    pub dc: DatabaseConnection,
}

impl TracingService {
    pub fn new(dc: DatabaseConnection) -> Self {
        Self { dc }
    }

    #[instrument(skip(self))]
    pub async fn init(&self) -> Result<(), DbErr> {
        if std::env::var("AUTO_INIT_DATABASE")
            .map(|n| n == "true")
            .unwrap_or(false)
        {
            let manager = SchemaManager::new(&self.dc);
            if !manager.has_table("tracing_record").await? {
                info!("tracing_record table does not exist. start init database!");
                self.dc
                    .execute_unprepared(include_str!("../sql.sql"))
                    .await?;
                info!("init database successfully!");
            }
        }

        let exception_app_runs = app_run::Entity::find()
            .filter(app_run::Column::StopTime.is_null())
            .all(&self.dc)
            .await?;
        for app_run in exception_app_runs.iter() {
            let last_record = tracing_record::Entity::find()
                .filter(
                    tracing_record::Column::AppRunId.is_in(exception_app_runs.iter().map(|n| n.id)),
                )
                .order_by_desc(tracing_record::Column::CreationTime)
                .one(&self.dc)
                .await?
                .map(|n| n.record_time);
            app_run::Entity::update(app_run::ActiveModel {
                id: Unchanged(app_run.id),
                exception_end: Set(Some(true)),
                stop_time: Set(Some(last_record.unwrap_or(app_run.start_time))),
                ..Default::default()
            })
            .exec(&self.dc)
            .await?;
        }

        let exception_span_runs = tracing_span_run::Entity::find()
            .filter(tracing_span_run::Column::CloseRecordId.is_null())
            .all(&self.dc)
            .await?;
        for item in exception_span_runs.iter() {
            let last_enter = tracing_span_enter::Entity::find()
                .filter(tracing_span_enter::Column::SpanRunId.eq(item.id))
                .order_by_desc(tracing_span_enter::Column::EnterTime)
                .one(&self.dc)
                .await?;
            let stop_time = last_enter
                .map(|n| {
                    n.enter_time
                        + TimeDelta::try_milliseconds(
                            n.duration.map(|n| n * 1000.).unwrap_or_default() as _,
                        )
                        .unwrap_or_else(|| {
                            warn!(
                                "TimeDelta::try_milliseconds error. duration: {}",
                                n.duration.map(|n| n * 1000.).unwrap_or_default()
                            );
                            Default::default()
                        })
                })
                .unwrap_or(item.run_time);
            tracing_span_run::Entity::update(tracing_span_run::ActiveModel {
                id: Unchanged(item.id),
                exception_end: Set(Some(stop_time)),
                ..Default::default()
            })
            .exec(&self.dc)
            .await?;
        }
        Ok(())
    }

    pub async fn list_latest_apps(&self) -> Result<Vec<AppLatestInfoDto>, DbErr> {
        use app_build::*;
        let apps = Entity::find()
            .distinct_on([Column::AppId])
            .order_by_desc(Column::AppId)
            .order_by_desc(Column::CreationTime)
            .all(&self.dc)
            .await?;

        let mut results = Vec::with_capacity(apps.len());

        for n in apps {
            let node_count = app_run::Entity::find()
                .filter(app_run::Column::AppId.eq(n.app_id))
                .distinct_on([app_run::Column::NodeId])
                .order_by_desc(app_run::Column::NodeId)
                .count(&self.dc)
                .await?;
            let online_node_count = app_run::Entity::find()
                .filter(
                    app_run::Column::AppId
                        .eq(n.app_id)
                        .and(app_run::Column::StopTime.is_null()),
                )
                .distinct_on([app_run::Column::NodeId])
                .order_by_desc(app_run::Column::NodeId)
                .count(&self.dc)
                .await?;

            results.push(AppLatestInfoDto {
                id: n.app_id,
                name: n.app_name.into(),
                build_time: n.creation_time,
                // run_info,
                node_count,
                online_node_count,
            });
        }

        Ok(results)
    }

    pub async fn list_records_app_run_infos(
        &self,
        record_ids: impl IntoIterator<Item = BigInt>,
    ) -> Result<impl Iterator<Item = AppRunDto>, DbErr> {
        use app_run::*;
        Ok(Entity::find()
            .filter(Column::RecordId.is_in(record_ids))
            .all(&self.dc)
            .await?
            .into_iter()
            .map(|n| n.into()))
    }
    pub async fn list_records_span_run_infos(
        &self,
        span_start_record_ids: impl IntoIterator<Item = BigInt>,
    ) -> Result<impl Iterator<Item = TracingSpanRunDto>, DbErr> {
        use tracing_span_run::*;
        Ok(Entity::find()
            .filter(Column::RecordId.is_in(span_start_record_ids))
            .all(&self.dc)
            .await?
            .into_iter()
            .map(|n| n.into()))
    }
    pub async fn list_records_span_enter_infos(
        &self,
        span_start_record_ids: impl IntoIterator<Item = BigInt>,
    ) -> Result<impl Iterator<Item = TracingSpanEnterDto>, DbErr> {
        use tracing_span_enter::*;
        Ok(Entity::find()
            .filter(Column::RecordId.is_in(span_start_record_ids))
            .all(&self.dc)
            .await?
            .into_iter()
            .map(|n| n.into()))
    }

    pub async fn list_tree_records(
        &self,
        filter: TracingRecordFilter,
    ) -> Result<impl Iterator<Item = TracingTreeRecordDto>, DbErr> {
        let records: Vec<_> = self.list_records(filter).await?.collect();
        let mut span_enter_infos: HashMap<_, _> = self
            .list_records_span_enter_infos(
                records
                    .iter()
                    .filter(|n| n.kind == TracingKind::SpanEnter)
                    .map(|n| n.id),
            )
            .await?
            .map(|n| (n.record_id, n))
            .collect();
        let mut span_run_infos: HashMap<_, _> = self
            .list_records_span_run_infos(
                records
                    .iter()
                    .filter(|n| n.kind == TracingKind::SpanCreate)
                    .map(|n| n.id),
            )
            .await?
            .map(|n| (n.record_id, n))
            .collect();
        let mut app_run_infos: HashMap<_, _> = self
            .list_records_app_run_infos(
                records
                    .iter()
                    .filter(|n| n.kind == TracingKind::AppStart)
                    .map(|n| n.id),
            )
            .await?
            .map(|n| (n.record_id, n))
            .collect();
        Ok(records.into_iter().map(move |n| {
            let variant = if n.kind == TracingKind::SpanCreate {
                span_run_infos
                    .remove(&n.id)
                    .map(TracingTreeRecordVariantDto::SpanRun)
            } else if n.kind == TracingKind::SpanEnter {
                span_enter_infos
                    .remove(&n.id)
                    .map(|mut dto| {
                        dto.enter_elapsed = GLOBAL_DATA
                            .get_node_now_timestamp_nanos(n.app_run_id)
                            .map(|node_now| {
                                Duration::from_nanos(
                                    (node_now - dto.enter_time.timestamp_nanos_opt().unwrap())
                                        as u64,
                                )
                                .as_millis_f64()
                            });
                        dto
                    })
                    .map(TracingTreeRecordVariantDto::SpanEnter)
            } else if n.kind == TracingKind::AppStart {
                app_run_infos
                    .remove(&n.id)
                    .map(TracingTreeRecordVariantDto::AppRun)
            } else {
                None
            };
            TracingTreeRecordDto::new(n, variant)
        }))
    }

    pub async fn record_introspection(&self) -> Result<TableInfo, DbErr> {
        use tracing_record::*;
        Ok(TableInfo {
            name: "record".into(),
            columns: Column::iter()
                .map(|n| {
                    let name = n.as_str();
                    let def = n.def();
                    let x = def.get_column_type();
                    TableColumnInfo {
                        name: name.to_string().into(),
                        column_type: x.clone().into(),
                    }
                })
                .collect(),
        })
    }
    pub async fn query_last_record_id(&self) -> Result<Option<BigInt>, DbErr> {
        use tracing_record::*;
        let option = Entity::find()
            .order_by_desc(Column::Id)
            .one(&self.dc)
            .await?;
        Ok(option.map(|n| n.id))
    }

    pub async fn list_records(
        &self,
        filter: TracingRecordFilter,
    ) -> Result<impl Iterator<Item = TracingRecordDto>, DbErr> {
        use tracing_record::*;
        let mut select = Entity::find()
            .apply_if(filter.parent_id, |n, parent_id| {
                n.filter(if parent_id.as_u128() != 0 {
                    Column::ParentId.eq(parent_id)
                } else {
                    Column::ParentId
                        .is_null()
                        .and(Column::Kind.ne(TracingKind::AppStart.as_str()))
                })
            })
            .apply_if(filter.parent_span_t_id, |n, span_t_id| {
                n.filter(Column::ParentSpanTId.eq(i64::from_le_bytes(span_t_id.to_le_bytes())))
            })
            .apply_if(filter.start_time, |n, start_time| {
                n.filter(Column::RecordTime.gt(start_time))
            })
            .apply_if(filter.end_time, |n, end_time| {
                n.filter(Column::RecordTime.lt(end_time))
            })
            .apply_if(filter.name, |n, (op, value)| {
                n.filter(Expr::col(Column::Name).binary(op, value))
            })
            .apply_if(filter.scene, |n, scene| match scene {
                TracingRecordScene::Tree => n.filter(
                    Expr::col(Column::Kind).is_in(
                        [
                            TracingKind::Event,
                            TracingKind::AppStart,
                            TracingKind::SpanCreate,
                        ]
                        .map(|n| n.as_str()),
                    ),
                ),
                TracingRecordScene::SpanField => n,
                TracingRecordScene::SpanEnter => n,
            });
        let mut condition = Cond::any();
        for (app_id, version) in filter.app_build_ids.unwrap_or_default() {
            condition = condition.add(match version {
                None => Column::AppId.eq(app_id),
                Some(version) => Column::AppId.eq(app_id).and(Column::AppVersion.eq(version)),
            });
        }
        if !condition.is_empty() {
            select = select.filter(condition);
        }
        if let Some(node_ids) = filter.node_ids {
            select = select.filter(Column::NodeId.is_in(node_ids));
        }
        if let Some(app_run_ids) = filter.app_run_ids {
            select = select.filter(Column::AppRunId.is_in(app_run_ids));
        }
        if let Some(levels) = filter.levels {
            select = select.filter(
                Cond::any()
                    .add(Column::Level.is_in(levels.into_iter().map(|n| {
                        let level: Level = n.into();
                        level as u32
                    })))
                    .add(Column::Level.is_null()),
            );
        }
        if let Some(span_ids) = filter.span_ids {
            select = select.filter(Column::SpanId.is_in(span_ids));
        }
        {
            let targets = filter.targets.unwrap_or_default();
            if !targets.is_empty() {
                let mut expr = Cond::any();
                for (op, value) in targets {
                    expr = expr.add(Expr::col(Column::Target).binary(op, value));
                }
                select = select.filter(expr);
            }
        }
        if let Some(kinds) = filter.kinds.as_ref() {
            select = select.filter(Column::Kind.is_in(kinds.iter().map(|n| n.as_str())));
        }
        for TracingRecordFieldFilter { name, op, value } in filter.fields.unwrap_or_default() {
            let value = SimpleExpr::Value(Value::String(value.map(|n| n.into())));

            select = select.filter(
                Expr::col(Column::Fields)
                    .cast_json_field(name)
                    .binary(op, value),
            );
        }
        if let Some(search) = filter.search.as_ref() {
            select = select.filter(
                Cond::any().add(Column::Name.like(search)).add(
                    Expr::col(Column::Fields)
                        .cast_json_field("name")
                        .like(search),
                ),
            )
        }
        let count = filter.count.unwrap_or(32);
        let records = if let Some(cursor) = filter.cursor {
            if cursor.is_before {
                select
                    .order_by_desc(Column::Id)
                    .cursor_by(Column::Id)
                    .before(cursor.id)
                    .last(count)
                    .all(&self.dc)
                    .await?
            } else {
                select
                    .order_by_desc(Column::Id)
                    .cursor_by(Column::Id)
                    .after(cursor.id)
                    .last(count)
                    .all(&self.dc)
                    .await?
            }
        } else {
            let mut records = select
                .order_by_desc(Column::Id)
                .limit(count)
                .all(&self.dc)
                .await?;

            records.reverse();
            records
        };
        Ok(records.into_iter().map(|n| {
            let (span_id_is_stable, span_t_id, repeated_count, fields) =
                if let Some(value) = n.fields {
                    let serde_json::Value::Object(mut fields) = value else {
                        unreachable!()
                    };

                    (
                        fields.remove(FIELD_DATA_STABLE_SPAN_ID).is_some(),
                        fields
                            .remove(FIELD_DATA_SPAN_T_ID)
                            .map(|n| n.as_u64().map(|n| n.to_smolstr()))
                            .flatten(),
                        fields
                            .remove(FIELD_DATA_REPEATED_COUNT)
                            .map(|n| n.as_u64().map(|n| n as u32))
                            .flatten(),
                        fields,
                    )
                } else {
                    (false, None, None, Default::default())
                };
            TracingRecordDto {
                id: n.id,
                app_id: n.app_id,
                app_version: n.app_version.into(),
                app_run_id: n.app_run_id,
                node_id: n.node_id.into(),
                name: n.name.into(),
                kind: n.kind.parse().unwrap(),
                level: n
                    .level
                    .map(|n| {
                        tracing_lv_proto::proto::Level::try_from(n)
                            .ok()
                            .map(|n| n.into())
                    })
                    .flatten(),
                span_id: n.span_id,
                fields: Arc::new(fields),
                target: n.target.map(|n| n.into()),
                module_path: n.module_path.map(|n| n.into()),
                position_info: n.position_info.map(|n| n.into()),
                record_time: n.record_time,
                creation_time: n.creation_time,
                span_id_is_stable: Some(span_id_is_stable),
                parent_id: n.parent_id,
                span_t_id,
                parent_span_t_id: n
                    .parent_span_t_id
                    .map(|n| u64::from_le_bytes(n.to_le_bytes()).to_smolstr()),
                repeated_count,
            }
        }))
    }

    pub async fn list_tracing_span_field_names(
        &self,
        span_id: SpanId,
    ) -> Result<Vec<String>, DbErr> {
        use tracing_record::*;
        let model = Entity::find()
            .filter(
                Column::SpanId
                    .eq(span_id)
                    .and(Column::Kind.eq(TracingKind::SpanCreate.as_str())),
            )
            .select_only()
            .column(Column::Fields)
            .limit(1)
            .one(&self.dc)
            .await?;
        let Some(model) = model else {
            return Ok(vec![]);
        };
        let Some(fields) = model.fields else {
            return Ok(vec![]);
        };
        let serde_json::Value::Object(fields) = fields else {
            return Ok(vec![]);
        };
        Ok(fields
            .into_iter()
            .map(|n| n.0)
            .filter(|n| !n.starts_with("__"))
            .collect())
    }

    pub async fn list_tracing_span(
        &self,
        app_id: Option<Uuid>,
        app_version: Option<String>,
    ) -> Result<Vec<TracingSpanDto>, DbErr> {
        use tracing_span::*;
        Ok(Entity::find()
            .filter(
                Column::AppId
                    .eq(app_id)
                    .and(Column::AppVersion.eq(app_version)),
            )
            .all(&self.dc)
            .await?
            .into_iter()
            .map(|n| TracingSpanDto {
                id: n.id,
                position_info: n.position_info,
                name: n.name,
                app_id: n.app_id,
                app_version: n.app_version,
            })
            .collect())
    }

    pub async fn app_node_run_count(&self, app_id: Uuid, node_id: &str) -> Result<u64, DbErr> {
        use app_run::*;
        Entity::find()
            .filter(Column::NodeId.eq(node_id).and(Column::AppId.eq(app_id)))
            .count(&self.dc)
            .await
    }

    pub async fn list_node(&self, mut filter: AppNodeFilter) -> Result<Vec<AppNodeRunDto>, DbErr> {
        use app_run::*;
        let mut select = Entity::find();

        let mut condition = Cond::any();
        for (app_id, version) in filter.app_build_ids.unwrap_or_default() {
            condition = condition.add(match version {
                None => Column::AppId.eq(app_id),
                Some(version) => Column::AppId.eq(app_id).and(Column::AppVersion.eq(version)),
            });
        }
        if !condition.is_empty() {
            select = select.filter(condition);
        }
        if let Some(after_record_id) = filter.after_record_id {
            select = select.filter(Column::RecordId.gt(after_record_id));
        }
        let mut app_runs = select
            .order_by_desc(Column::NodeId)
            .order_by_desc(Column::AppId)
            .order_by_desc(Column::CreationTime)
            .distinct_on([Column::NodeId, Column::AppId])
            .all(&self.dc)
            .await?;

        let mut result = Vec::with_capacity(app_runs.len());
        for item in app_runs.chunk_by_mut(|a, b| a.node_id == b.node_id) {
            if filter.main_app_id.is_none() {
                filter.main_app_id = Some(item[0].app_id);
            }
            let count = item.len();
            let model = item
                .iter_mut()
                .enumerate()
                .find(|(i, n)| n.app_id == filter.main_app_id.unwrap() || *i == (count - 1))
                .unwrap()
                .1;
            result.push(AppNodeRunDto {
                app_run_id: model.id,
                node_id: core::mem::take(&mut model.node_id).into(),
                data: Arc::new(if let Some(data) = model.data.take() {
                    let serde_json::Value::Object(data) = data else {
                        unreachable!("should is object json");
                    };
                    data
                } else {
                    Default::default()
                }),
                creation_time: model.creation_time,
                record_id: model.record_id,
                stop_record_id: model.stop_record_id,
                start_time: model.start_time,
                stop_time: model.stop_time,
                exception_end: model.exception_end.unwrap_or_default(),
                app_build_ids: item
                    .iter()
                    .map(|n| (n.app_id, n.app_version.clone()))
                    .collect(),
            })
        }
        result.sort_by_key(|n| (n.stop_time.is_some(), Reverse(n.creation_time)));
        Ok(result)
    }
    /*
        pub async fn list_app_run(&self, filter: AppRunFilter) -> Result<Vec<AppRunDto>, DbErr> {
            use app_run::*;
            let mut select = Entity::find()
                .apply_if(filter.app_ids, |n, value| {
                    n.filter(Column::AppId.is_in(value))
                })
                // .apply_if(filter.app_id, |n, value| n.filter(Column::AppId.eq(value)))
                .apply_if(filter.app_versions, |n, value| {
                    n.filter(Column::AppVersion.is_in(value))
                });
            for TracingRecordFieldFilter { name, op, value } in filter.fields.unwrap_or_default() {
                let value = SimpleExpr::Value(Value::String(value.map(|n| n.into())));

                select = select.filter(
                    Expr::col(Column::Data)
                        .cast_json_field(name.as_str())
                        .binary(op, value.clone()),
                );
            }
            let select = select
                .find_also_related(app_build::Entity)
                .order_by_desc(Column::CreationTime);
            let app_runs = if let Some(cursor) = filter.cursor {
                select
                    .cursor_by(Column::RecordId)
                    .after(cursor)
                    .all(&self.dc)
                    .await?
            } else {
                select.all(&self.dc).await?
            };
            Ok(app_runs
                .into_iter()
                .map(|(n, app_build)| {
                    let dto: AppRunDto = n.into();
                    dto
                })
                .collect())
        }
    */
    pub async fn list_app_version(&self, app_id: Option<Uuid>) -> Result<Vec<String>, DbErr> {
        use app_build::*;
        Ok(Entity::find()
            .filter(Column::AppId.eq(app_id))
            .select_only()
            .column(Column::AppVersion)
            .all(&self.dc)
            .await?
            .into_iter()
            .map(|n| n.app_version)
            .collect())
    }

    pub async fn find_app_run_by_id(&self, run_id: Uuid) -> Result<Option<app_run::Model>, DbErr> {
        use app_run::*;
        Entity::find_by_id(run_id).one(&self.dc).await
    }

    pub async fn tracing_span_create(
        &self,
        run_id: Uuid,
        span_id: Uuid,
        run_time: DateTime<FixedOffset>,
        record_id: BigInt,
    ) -> Result<Uuid, DbErr> {
        use tracing_span_run::*;
        let result = Entity::insert(ActiveModel {
            app_run_id: Set(run_id),
            span_id: Set(span_id),
            run_time: Set(run_time),
            record_id: Set(record_id),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?;
        Ok(result.last_insert_id)
    }

    pub async fn tracing_span_close(
        &self,
        id: Uuid,
        busy_duration: Duration,
        idle_duration: Duration,
        close_record_id: BigInt,
    ) -> Result<TracingSpanRunDto, DbErr> {
        use tracing_span_run::*;
        let model = Entity::update(ActiveModel {
            id: Unchanged(id),
            busy_duration: Set(Some(busy_duration.as_secs_f64())),
            idle_duration: Set(Some(idle_duration.as_secs_f64())),
            close_record_id: Set(Some(close_record_id)),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?;
        Ok(model.into())
    }

    pub async fn update_span_fields(
        &self,
        id: Uuid,
        updated_fields: impl Iterator<Item = (String, serde_json::Value)>,
    ) -> Result<(), DbErr> {
        use tracing_span_run::*;
        let mut fields = Entity::find_by_id(id)
            .one(&self.dc)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound(id.to_string()))?
            .fields;
        if fields.is_none() {
            fields = Some(serde_json::Value::Object(serde_json::Map::new()))
        }
        let Some(value) = fields.as_mut() else {
            unreachable!()
        };
        let serde_json::Value::Object(map) = value else {
            unreachable!()
        };

        for (field, value) in updated_fields {
            map.insert(field, value);
        }
        Entity::update(ActiveModel {
            id: Unchanged(id),
            fields: Set(fields),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?;
        Ok(())
    }

    pub async fn tracing_span_enter(
        &self,
        span_run_id: Uuid,
        enter_time: DateTime<FixedOffset>,
        record_id: BigInt,
    ) -> Result<Uuid, DbErr> {
        use tracing_span_enter::*;
        Ok(Entity::insert(ActiveModel {
            span_run_id: Set(span_run_id),
            enter_time: Set(enter_time),
            record_id: Set(record_id),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?
        .last_insert_id)
    }

    pub async fn tracing_span_leave(
        &self,
        id: Uuid,
        duration: Duration,
        leave_record_id: BigInt,
    ) -> Result<(), DbErr> {
        use tracing_span_enter::*;
        Entity::update(ActiveModel {
            id: Unchanged(id),
            duration: Set(Some(duration.as_secs_f64())),
            leave_record_id: Set(Some(leave_record_id)),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?;
        Ok(())
    }

    pub async fn try_insert_tracing_span_parent(
        &self,
        id: Uuid,
        parent_span_id: Uuid,
    ) -> Result<TryInsertResult<InsertResult<tracing_span_parent::ActiveModel>>, DbErr> {
        use tracing_span_parent::*;
        Entity::insert(ActiveModel {
            span_id: Set(id),
            span_parent_id: Set(parent_span_id),
            ..Default::default()
        })
        .on_conflict_do_nothing()
        .exec(&self.dc)
        .await
    }

    pub async fn try_insert_tracing_span(
        &self,
        id: Uuid,
        span_cache_id: SpanCacheId,
    ) -> Result<TryInsertResult<InsertResult<tracing_span::ActiveModel>>, DbErr> {
        use tracing_span::*;
        Entity::insert(ActiveModel {
            app_id: Set(span_cache_id.app_id),
            app_version: Set(span_cache_id.app_version.to_string()),
            id: Unchanged(id),
            position_info: Set(span_cache_id.file_line.to_string()),
            name: Set(span_cache_id.name.to_string()),
            ..Default::default()
        })
        .on_conflict_do_nothing()
        .exec(&self.dc)
        .await
    }

    pub async fn find_app_by_id(&self, id: Uuid) -> Result<Option<app::Model>, DbErr> {
        use app::*;
        Entity::find_by_id(id).one(&self.dc).await
    }

    pub async fn try_insert_app(
        &self,
        id: Uuid,
    ) -> Result<TryInsertResult<InsertResult<app::ActiveModel>>, DbErr> {
        use app::*;
        Entity::insert(ActiveModel {
            id: Set(id),
            ..Default::default()
        })
        .on_conflict_do_nothing()
        .exec(&self.dc)
        .await
    }

    pub async fn try_insert_app_build(
        &self,
        app_info: Arc<AppRunInfo>,
        app_name: String,
    ) -> Result<TryInsertResult<InsertResult<ActiveModel>>, DbErr> {
        use app_build::*;
        Entity::insert(ActiveModel {
            app_id: Set(app_info.id),
            app_version: Set(app_info.version.to_string()),
            app_name: Set(app_name),
            ..Default::default()
        })
        .on_conflict_do_nothing()
        .exec(&self.dc)
        .await
    }

    pub async fn insert_app_run(
        &self,
        app_info: Arc<AppRunInfo>,
        data: serde_json::Value,
        start_time: DateTime<FixedOffset>,
        record_id: BigInt,
    ) -> Result<AppRunDto, DbErr> {
        use app_run::*;
        let model = Entity::insert(ActiveModel {
            id: Set(app_info.run_id),
            app_id: Set(app_info.id),
            node_id: Set(app_info.node_id.to_string()),
            app_version: Set(app_info.version.to_string()),
            data: Set(Some(data)),
            record_id: Set(record_id),
            start_time: Set(start_time),
            ..Default::default()
        })
        .exec_with_returning(&self.dc)
        .await?;
        Ok(model.into())
    }
    pub async fn update_app_run_stop_time(
        &self,
        id: Uuid,
        stop_time: DateTime<FixedOffset>,
        stop_record_id: BigInt,
        exception_end: Option<bool>,
    ) -> Result<AppRunDto, DbErr> {
        use app_run::*;
        let model = Entity::update(ActiveModel {
            id: Unchanged(id),
            stop_time: Set(Some(stop_time)),
            stop_record_id: Set(Some(stop_record_id)),
            exception_end: Set(exception_end),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?;
        Ok(model.into())
    }

    pub async fn find_or_insert_tracing_span(
        &self,
        span_cache_id: SpanCacheId,
    ) -> Result<SpanId, DbErr> {
        use tracing_span::*;
        let model = Entity::find()
            .filter(
                Column::Name
                    .eq(span_cache_id.name.as_str())
                    .and(Column::PositionInfo.eq(span_cache_id.file_line.as_str()))
                    .and(Column::AppId.eq(span_cache_id.app_id))
                    .and(Column::AppVersion.eq(span_cache_id.app_version.as_str())),
            )
            .one(&self.dc)
            .await?;
        if let Some(model) = model {
            Ok(model.id)
        } else {
            let span_id = Uuid::new_v4();
            Entity::insert(ActiveModel {
                id: Set(span_id),
                name: Set(span_cache_id.name.to_string()),
                position_info: Set(span_cache_id.file_line.to_string()),
                app_version: Set(span_cache_id.app_version.to_string()),
                app_id: Set(span_cache_id.app_id),
                ..Default::default()
            })
            .exec(&self.dc)
            .await
            .map(|n| n.last_insert_id)
        }
    }

    pub async fn incremental_repeated_event(
        &self,
        id: BigInt,
        record_time: DateTime<FixedOffset>,
    ) -> Result<(), DbErr> {
        use tracing_record::*;
        let fields = Entity::find()
            .filter(Column::Id.eq(id))
            .one(&self.dc)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound(id.to_string()))?
            .fields;
        let mut fields = fields.unwrap_or(serde_json::Value::Object(Default::default()));
        let serde_json::Value::Object(fields_map) = &mut fields else {
            unreachable!()
        };
        fields_map.insert(
            FIELD_DATA_LAST_REPEATED_TIME.to_string(),
            serde_json::Value::String(record_time.to_string()),
        );
        let field_value = fields_map
            .entry(FIELD_DATA_REPEATED_COUNT)
            .or_insert(serde_json::Value::Number(1.into()));
        *field_value = serde_json::Value::Number((field_value.as_u64().unwrap_or(1) + 1).into());
        Entity::update(ActiveModel {
            id: Unchanged(id),
            fields: Set(Some(fields)),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?;
        Ok(())
    }

    // pub async fn list_repeated_events(&self, repeated_record_id: BigSerialId) ->Result<(),DbErr> {
    //      use tracing_record::*;
    //     Entity::find()
    //        .filter(Column::Id == repeated_record_id)
    //     Ok(())
    // }

    pub async fn insert_record(
        &self,
        name: String,
        record_time: DateTime<FixedOffset>,
        kind: String,
        level: Option<Level>,
        span_id: Option<SpanId>,
        parent: Option<SpanId>,
        fields: Option<serde_json::Value>,
        target: Option<String>,
        module_path: Option<String>,
        position_info: Option<String>,
        app_info: Arc<AppRunInfo>,
    ) -> Result<BigInt, DbErr> {
        use tracing_record::*;

        Ok(Entity::insert(ActiveModel {
            app_id: Set(app_info.id),
            app_version: Set(app_info.version.to_string()),
            app_run_id: Set(app_info.run_id),
            node_id: Set(app_info.node_id.to_string()),
            name: Set(name),
            record_time: Set(record_time.fixed_offset()),
            kind: Set(kind),
            level: Set(level.map(|n| n as i32)),
            span_id: Set(span_id),
            parent_id: Set(parent),
            fields: Set(fields),
            target: Set(target),
            module_path: Set(module_path),
            position_info: Set(position_info),
            ..Default::default()
        })
        .exec(&self.dc)
        .await?
        .last_insert_id)
    }
}

impl From<&TracingTreeRecordDto> for Option<AppRunDto> {
    fn from(value: &TracingTreeRecordDto) -> Self {
        if value.record.kind == TracingKind::AppStart {
            Some(AppRunDto {
                id: value.record.app_run_id,
                app_id: value.record.app_id,
                app_version: value.record.app_version.clone(),
                data: value.record.fields.clone(),
                creation_time: Default::default(),
                record_id: value.record.id.clone(),
                stop_record_id: None,
                start_time: value.record.record_time,
                stop_time: None,
                exception_end: false,
                run_elapsed: GLOBAL_DATA
                    .get_node_now_timestamp_nanos(value.record.app_run_id)
                    .map(|node_now| {
                        Duration::from_nanos(
                            (node_now - value.record.record_time.timestamp_nanos_opt().unwrap())
                                as u64,
                        )
                        .as_millis_f64()
                    }),
            })
        } else if value.record.kind == TracingKind::AppStop {
            Some(AppRunDto {
                id: value.record.app_run_id,
                app_id: value.record.app_id,
                app_version: value.record.app_version.clone(),
                data: value.record.fields.clone(),
                creation_time: Default::default(),
                record_id: value.record.id.clone(),
                stop_record_id: Some(value.record.id.clone()),
                // TODO:
                start_time: Default::default(),
                stop_time: Some(value.record.record_time),
                exception_end: false,
                run_elapsed: GLOBAL_DATA
                    .get_node_now_timestamp_nanos(value.record.app_run_id)
                    .map(|node_now| {
                        Duration::from_nanos(
                            (node_now - value.record.record_time.timestamp_nanos_opt().unwrap())
                                as u64,
                        )
                        .as_millis_f64()
                    }),
            })
        } else {
            None
        }
    }
}
// impl From<&TracingRecordVariant> for Option<AppRunDto> {
//     fn from(value: &TracingRecordVariant) -> Self {
//         Some(match value {
//             TracingRecordVariant::AppStart {
//                 record_id,
//                 record_time,
//                 app_info,
//                 name,
//                 fields,
//             } => {
//                 let static_data = fields.get("static_data").unwrap().clone();
//                 let data = fields.get("data").unwrap().clone();
//                 AppRunDto {
//                     id: app_info.run_id,
//                     app_id: app_info.id,
//                     app_version: app_info.version.clone(),
//                     data: Some(data),
//                     creation_time: Default::default(),
//                     static_data: Some(static_data),
//                     record_id: record_id.clone(),
//                     stop_record_id: None,
//                     start_time: record_time.fixed_offset(),
//                     stop_time: None,
//                 }
//             }
//             TracingRecordVariant::AppStop {
//                 record_id,
//                 record_time,
//                 app_info,
//                 name,
//             } => AppRunDto {
//                 id: app_info.run_id,
//                 app_id: app_info.id,
//                 app_version: app_info.version.clone(),
//                 data: None,
//                 creation_time: Default::default(),
//                 static_data: None,
//                 record_id: 0,
//                 stop_record_id: Some(record_id.clone()),
//                 stop_time: Some(record_time.fixed_offset()),
//                 start_time: Default::default(),
//             },
//             _ => return None,
//         })
//     }
// }
