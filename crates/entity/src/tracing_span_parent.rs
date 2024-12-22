//! `SeaORM` Entity, @generated by sea-orm-codegen 1.0.1

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "tracing_span_parent")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub span_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub span_parent_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::tracing_span::Entity",
        from = "Column::SpanId",
        to = "super::tracing_span::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    TracingSpan2,
    #[sea_orm(
        belongs_to = "super::tracing_span::Entity",
        from = "Column::SpanParentId",
        to = "super::tracing_span::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    TracingSpan1,
}

impl ActiveModelBehavior for ActiveModel {}
