//! `SeaORM` Entity, @generated by sea-orm-codegen 1.0.1

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "app_run")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    pub app_version: String,
    pub node_id: String,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub data: Option<Json>,
    pub creation_time: DateTimeWithTimeZone,
    pub record_id: i64,
    pub stop_record_id: Option<i64>,
    pub start_time: DateTimeWithTimeZone,
    pub stop_time: Option<DateTimeWithTimeZone>,
    pub exception_end: Option<bool>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::app_build::Entity",
        from = "(Column::AppId, Column::AppVersion)",
        to = "(super::app_build::Column::AppId, super::app_build::Column::AppVersion)",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    AppBuild,
    #[sea_orm(has_many = "super::tracing_span_run::Entity")]
    TracingSpanRun,
}

impl Related<super::app_build::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AppBuild.def()
    }
}

impl Related<super::tracing_span_run::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TracingSpanRun.def()
    }
}

impl Related<super::app::Entity> for Entity {
    fn to() -> RelationDef {
        super::app_build::Relation::App.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::app_build::Relation::AppRun.def().rev())
    }
}

impl ActiveModelBehavior for ActiveModel {}
