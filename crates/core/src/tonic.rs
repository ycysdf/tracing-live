use crate::proto::{
    field_value, record_param, AppStart, Event, FieldValue, PosInfo, RecordParam, SpanClose,
    SpanCreate, SpanEnter, SpanLeave, SpanRecordField,
};
use crate::tracing_layer::{SpanInfo, TLMsg};
use crate::{proto, SpanAttrs, TLAppInfo, TLValue, TracingLiveMsgSubscriber, VxMetadata};
use chrono::Utc;
use core::fmt::{Display, Formatter};
use hashbrown::HashMap;
use std::prelude::rust_2015::String;
use std::time::Duration;
use uuid::Uuid;

impl From<f32> for field_value::Variant {
    fn from(value: f32) -> Self {
        field_value::Variant::F64(value.into())
    }
}

impl From<f64> for field_value::Variant {
    fn from(value: f64) -> Self {
        field_value::Variant::F64(value.into())
    }
}

impl From<u32> for field_value::Variant {
    fn from(value: u32) -> Self {
        field_value::Variant::U64(value.into())
    }
}

impl From<u64> for field_value::Variant {
    fn from(value: u64) -> Self {
        field_value::Variant::U64(value.into())
    }
}

impl From<bool> for field_value::Variant {
    fn from(value: bool) -> Self {
        field_value::Variant::Bool(value)
    }
}

impl From<String> for field_value::Variant {
    fn from(value: String) -> Self {
        field_value::Variant::String(value)
    }
}

impl From<&'static str> for field_value::Variant {
    fn from(value: &'static str) -> Self {
        field_value::Variant::String(value.into())
    }
}

impl From<FieldValue> for serde_json::Value {
    fn from(value: FieldValue) -> Self {
        value.variant.unwrap().into()
    }
}

impl From<field_value::Variant> for serde_json::Value {
    fn from(value: field_value::Variant) -> Self {
        match value {
            field_value::Variant::F64(n) => {
                serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap())
            }
            field_value::Variant::I64(n) => serde_json::Value::Number(n.into()),
            field_value::Variant::U64(n) => serde_json::Value::Number(n.into()),
            field_value::Variant::Bool(n) => serde_json::Value::Bool(n),
            field_value::Variant::String(n) => serde_json::Value::String(n),
            field_value::Variant::Null(_) => serde_json::Value::Null,
        }
    }
}

impl From<serde_json::Value> for field_value::Variant {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Number(n) if n.is_f64() => {
                field_value::Variant::F64(n.as_f64().unwrap())
            }
            serde_json::Value::Number(n) if n.is_u64() => {
                field_value::Variant::U64(n.as_u64().unwrap())
            }
            serde_json::Value::Number(n) if n.is_i64() => {
                field_value::Variant::I64(n.as_i64().unwrap())
            }
            serde_json::Value::Bool(n) => field_value::Variant::Bool(n),
            serde_json::Value::String(n) => field_value::Variant::String(n),
            serde_json::Value::Null => field_value::Variant::Null(true),
            _ => field_value::Variant::Null(false),
        }
    }
}

impl From<serde_json::Value> for FieldValue {
    fn from(value: serde_json::Value) -> Self {
        let variant = value.into();
        FieldValue {
            variant: Some(variant),
        }
    }
}

impl From<TLValue> for field_value::Variant {
    fn from(value: TLValue) -> Self {
        match value {
            TLValue::F64(n) => field_value::Variant::F64(n),
            TLValue::I64(n) => field_value::Variant::I64(n),
            TLValue::U64(n) => field_value::Variant::U64(n),
            TLValue::Bool(n) => field_value::Variant::Bool(n),
            TLValue::Str(n) => field_value::Variant::String(n.into()),
        }
    }
}

impl From<TLValue> for FieldValue {
    fn from(value: TLValue) -> Self {
        FieldValue {
            variant: Some(value.into()),
        }
    }
}

impl<'a> From<&'a VxMetadata> for PosInfo {
    fn from(value: &'a VxMetadata) -> Self {
        Self {
            module_path: value.module_path.unwrap_or("unknown").into(),
            file_line: alloc::format!(
                "{}:{}",
                value.file.unwrap_or("unknown"),
                value.line.unwrap_or(0)
            ),
        }
    }
}

impl<'a> From<&'a SpanInfo> for proto::SpanInfo {
    fn from(value: &'a SpanInfo) -> Self {
        Self {
            t_id: value.id,
            name: value.metadata.name.into(),
            file_line: alloc::format!(
                "{}:{}",
                value.metadata.file.unwrap_or("unknown"),
                value.metadata.line.unwrap_or(0)
            ),
        }
    }
}

impl From<SpanAttrs> for HashMap<String, FieldValue> {
    fn from(value: SpanAttrs) -> Self {
        value
            .0
            .into_iter()
            .map(|n| (n.0.into(), n.1.into()))
            .collect()
    }
}

#[cfg(feature = "std")]
impl From<SpanAttrs> for std::collections::HashMap<String, FieldValue> {
    fn from(value: SpanAttrs) -> Self {
        value
            .0
            .into_iter()
            .map(|n| (n.0.into(), n.1.into()))
            .collect()
    }
}

impl From<TLMsg> for RecordParam {
    fn from(value: TLMsg) -> Self {
        Self {
            send_time: 0,
            record_index: value.record_index(),
            variant: Some(value.into()),
        }
    }
}

impl From<TLMsg> for record_param::Variant {
    fn from(value: TLMsg) -> Self {
        match value {
            TLMsg::SpanCreate {
                date,
                span,
                attributes,
                parent_span,
                ..
            } => record_param::Variant::SpanCreate(SpanCreate {
                record_time: date.timestamp_nanos_opt().unwrap(),
                span_info: Some((&span).into()),
                pos_info: Some((&span.metadata).into()),
                parent_span_info: parent_span.map(|n| (&n).into()),
                fields: attributes.into(),
                target: span.metadata.target.into(),
                level: proto::Level::from_str_name(span.metadata.level)
                    .unwrap()
                    .into(),
            }),
            TLMsg::SpanRecordAttr {
                span,
                date,
                attributes,
                ..
            } => record_param::Variant::SpanRecordField(SpanRecordField {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some((&span.metadata).into()),
                span_info: Some((&span).into()),
                fields: attributes.into(),
            }),
            TLMsg::SpanEnter { span, date, .. } => record_param::Variant::SpanEnter(SpanEnter {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some((&span.metadata).into()),
                span_info: Some((&span).into()),
            }),
            TLMsg::SpanLeave { span, date, .. } => record_param::Variant::SpanLeave(SpanLeave {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some((&span.metadata).into()),
                span_info: Some((&span).into()),
            }),
            TLMsg::SpanClose { span, date, .. } => record_param::Variant::SpanClose(SpanClose {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some((&span.metadata).into()),
                span_info: Some((&span).into()),
            }),
            TLMsg::Event {
                message,
                metadata,
                attributes,
                span,
                date,
                ..
            } => record_param::Variant::Event(Event {
                record_time: date.timestamp_nanos_opt().unwrap(),
                message: message.into(),
                pos_info: Some((&metadata).into()),
                fields: attributes.into(),
                target: metadata.target.into(),
                span_info: span.map(|n| (&n).into()),
                level: proto::Level::from_str_name(metadata.level).unwrap().into(),
            }),
        }
    }
}

pub type FieldValueVariant = field_value::Variant;

pub struct MsgReceiverSubscriber {
    sender: flume::Sender<RecordParam>,
}

impl MsgReceiverSubscriber {
    pub fn new(sender: flume::Sender<RecordParam>) -> Self {
        Self { sender }
    }
}

impl TracingLiveMsgSubscriber for MsgReceiverSubscriber {
    fn on_msg(&self, msg: TLMsg) {
        if self.sender.is_disconnected() {
            return;
        }
        if let Err(err) = self.sender.send(msg.into()) {
            if let Some(variant) = &err.0.variant {
                if let record_param::Variant::Event(e) = variant {
                    if e.target == "tower::buffer::worker" {
                        return;
                    }
                }
            }
            std::eprintln!("failed to send msg. {:?}", err.0.variant);
        }
    }
}

impl Display for field_value::Variant {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            field_value::Variant::F64(n) => {
                write!(f, "{}", n)
            }
            field_value::Variant::I64(n) => {
                write!(f, "{}", n)
            }
            field_value::Variant::U64(n) => {
                write!(f, "{}", n)
            }
            field_value::Variant::Bool(n) => {
                write!(f, "{}", n)
            }
            field_value::Variant::String(n) => {
                write!(f, "{}", n)
            }
            field_value::Variant::Null(_n) => {
                write!(f, "NULL")
            }
        }
    }
}

pub trait TLAppInfoExt {
    fn into_app_start(self, run_id: Uuid, rtt: Duration) -> AppStart;
}

impl TLAppInfoExt for TLAppInfo {
    fn into_app_start(self, run_id: Uuid, rtt: Duration) -> AppStart {
        let record_time = Utc::now().timestamp_nanos_opt().unwrap();
        AppStart {
            record_time,
            id: self.app_id.into(),
            run_id: run_id.into(),
            node_id: self.node_id.to_string(),
            name: self.app_name.to_string(),
            version: self.app_version.to_string(),
            data: self
                .data
                .into_iter()
                .map(|n| {
                    (
                        n.0.to_string(),
                        FieldValue {
                            variant: Some(n.1.into()),
                        },
                    )
                })
                .collect(),
            rtt: rtt.as_secs_f64(),
            reconnect: false,
        }
    }
}
