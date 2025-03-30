use crate::{TLAppInfo, TLMsg};
use alloc::boxed::Box;
use chrono::{DateTime, Duration, Utc};
use core::fmt::{Display, Formatter};
use derive_more::From;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::time::Instant;
use hashbrown::HashMap;
use uuid::{Bytes, Uuid};
use xy_rpc::formats::{MessagePackFormat, SerdeFormat};
use xy_rpc::maybe_send::{MaybeSend, MaybeSync};
use xy_rpc::{RpcError, TransStream, rpc_service};

pub type FORMAT = MessagePackFormat;

#[derive(Debug, From, Clone, PartialEq, Serialize, Deserialize)]
pub enum TLValue {
   F64(f64),
   I64(i64),
   U64(u64),
   Bool(bool),
   String(SmolStr),
   Bytes(Bytes),
   Null,
}
#[cfg(feature = "serde_json")]
impl Into<serde_json::Value> for TLValue {
   fn into(self) -> serde_json::Value {
      match self {
         TLValue::F64(n) => serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap()),
         TLValue::I64(n) => serde_json::Value::Number(serde_json::Number::from_i128(n as _).unwrap()),
         TLValue::U64(n) => serde_json::Value::Number(serde_json::Number::from_u128(n as _).unwrap()),
         TLValue::Bool(n) => serde_json::Value::Bool(n),
         TLValue::String(n) => serde_json::Value::String(n.to_string()),
         TLValue::Bytes(n) => serde_json::Value::String(todo!()), // TODO:
         TLValue::Null => serde_json::Value::Null,
      }
   }
}

#[cfg(feature = "serde_json")]
impl TryFrom<serde_json::Value> for TLValue {
   type Error = serde_json::Value;

   fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
      Ok(match value {
         serde_json::Value::Null => TLValue::Null,
         serde_json::Value::Bool(n) => TLValue::Bool(n),
         serde_json::Value::Number(n) => {
            if let Some(n) = n.as_u64() {
               TLValue::U64(n)
            } else if let Some(n) = n.as_f64() {
               TLValue::F64(n)
            } else {
               return Err(serde_json::Value::Number(n))
            }
         }
         serde_json::Value::String(n) => TLValue::String(n.into()),
         _ => return Err(value)
      })
   }
}

impl Display for TLValue {
   fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
      match self {
         TLValue::F64(n) => write!(f, "{n}"),
         TLValue::I64(n) => write!(f, "{n}"),
         TLValue::U64(n) => write!(f, "{n}"),
         TLValue::Bool(n) => write!(f, "{n}"),
         TLValue::String(n) => write!(f, "{n}"),
         TLValue::Bytes(n) => write!(f, "{n:?}"),
         TLValue::Null => write!(f, "<Null>"),
      }?;
      Ok(())
   }
}

impl From<&'static str> for TLValue {
   fn from(value: &'static str) -> Self {
      TLValue::String(value.into())
   }
}

impl From<alloc::string::String> for TLValue {
   fn from(value: alloc::string::String) -> Self {
      TLValue::String(value.into())
   }
}

#[derive(Debug,Clone, Serialize, Deserialize)]
pub struct AppStartInfo {
   pub record_time: DateTime<Utc>,
   pub id: Uuid,
   pub node_id: SmolStr,
   pub run_id: Uuid,
   pub name: SmolStr,
   pub version: SmolStr,
   pub data: hashbrown::HashMap<SmolStr, TLValue>,
   pub rtt: Duration,
   pub reconnect: bool,
   pub send_time: DateTime<Utc>,
}

impl AppStartInfo {
   pub fn from_app_info(app_info: TLAppInfo, run_id: Uuid) -> Self {
      let instant = Instant::now();
      let rtt: Duration = Duration::from_std(instant.elapsed()).unwrap_or_default();
      let send_time = Utc::now();
      Self {
         record_time: send_time,
         id: app_info.app_id,
         node_id: app_info.node_id,
         run_id,
         name: app_info.app_name,
         version: app_info.app_version,
         data: app_info.data,
         rtt,
         reconnect: false,
         send_time,
      }
   }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TLRecordVariant {
   TLMsg(TLMsg),
   AppStop { exception_end: bool },
}

impl TLRecordVariant {
   pub fn record_index(&self) -> u64 {
      match self {
         TLRecordVariant::TLMsg(m) => m.record_index(),
         TLRecordVariant::AppStop { .. } => 0,
      }
   }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TracingRecordItem {
   pub variant: TLRecordVariant,
   pub send_time: DateTime<Utc>,
}

impl From<TLMsg> for TracingRecordItem {
   fn from(value: TLMsg) -> Self {
      Self {
         variant: TLRecordVariant::TLMsg(value),
         send_time: Utc::now(),
      }
   }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TracingReplyVariant {
   AppReconnectReply { last_record_index: u64 },
}

#[rpc_service]
pub trait TracingService: MaybeSend + MaybeSync {
   async fn app_run(
      &self,
      info: AppStartInfo,
      stream: TransStream<TracingRecordItem, impl SerdeFormat>,
   ) -> impl Stream<Item=Result<TracingReplyVariant, RpcError>> + MaybeSend + 'static;
   async fn ping(&self);
}
