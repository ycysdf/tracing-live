//! `SeaORM` Entity, @generated by sea-orm-codegen 1.0.1

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "tracing_span")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub position_info: String,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    pub app_id: Uuid,
    pub app_version: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::tracing_span_run::Entity")]
    TracingSpanRun,
}

impl Related<super::tracing_span_run::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TracingSpanRun.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
