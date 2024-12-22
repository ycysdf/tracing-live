// use sea_orm::ActiveValue::{Set, Unchanged};
// use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
//
// pub struct SettingService {
//     dc: DatabaseConnection,
// }
//
// impl SettingService {
//     pub fn new(dc: DatabaseConnection) -> Self {
//         Self { dc }
//     }
//
//     pub async fn get_setting(&self, name: &str) -> Result<Option<serde_json::Value>, DbErr> {
//         use entity::setting::*;
//         let model = Entity::find()
//             .filter(Column::Name.eq(name))
//             .one(&self.dc)
//             .await?;
//         Ok(model.map(|n| n.value))
//     }
//
//     pub async fn set_setting_value(
//         &self,
//         name: impl Into<String>,
//         value: serde_json::Value,
//     ) -> Result<(), DbErr> {
//         use entity::setting::*;
//         Entity::update(ActiveModel {
//             name: Unchanged(name.into()),
//             value: Set(value),
//             ..Default::default()
//         })
//         .exec(&self.dc)
//         .await?;
//         Ok(())
//     }
// }
