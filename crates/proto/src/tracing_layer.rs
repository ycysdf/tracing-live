use crate::proto::{
    field_value, record_param, AppStart, Event, FieldValue, PosInfo, RecordParam, SpanClose,
    SpanCreate, SpanEnter, SpanLeave, SpanRecordField,
};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use smol_str::{format_smolstr, SmolStr, ToSmolStr};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::time::Duration;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::{Id, Metadata, Subscriber, Value};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(bound(deserialize = "'de: 'static"))]
pub struct SpanAttrs(pub HashMap<&'static str, TLValue>);

pub const MAX_RECORD_LEN: usize = 1024 * 1024;

impl Visit for SpanAttrs {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name(), TLValue::F64(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name(), TLValue::I64(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name(), TLValue::U64(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name(), TLValue::Bool(value));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(
            field.name(),
            TLValue::Str(if value.bytes().len() > MAX_RECORD_LEN {
                format_smolstr!("<string too long. {}>", value.bytes().len())
            } else {
                value.replace("\x00", "").to_smolstr()
            }),
        );
    }
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        let str = format_smolstr!("{:?}", value);
        if str.bytes().len() > MAX_RECORD_LEN {
            self.0.insert(
                field.name(),
                TLValue::Str(format_smolstr!("<value too long. {}>", str.bytes().len())),
            );
        }
        if str.contains("\x00") {
            self.0.insert(
                field.name(),
                TLValue::Str(str.replace("\x00", "").to_smolstr()),
            );
        } else {
            self.0.insert(field.name(), TLValue::Str(str));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TLValue {
    F64(f64),
    I64(i64),
    U64(u64),
    Bool(bool),
    Str(SmolStr),
}

impl Display for TLValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TLValue::F64(n) => write!(f, "{n}"),
            TLValue::I64(n) => write!(f, "{n}"),
            TLValue::U64(n) => write!(f, "{n}"),
            TLValue::Bool(n) => write!(f, "{n}"),
            TLValue::Str(n) => write!(f, "{n}"),
        }?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VxMetadata {
    name: &'static str,
    target: &'static str,
    level: &'static str,
    module_path: Option<&'static str>,
    file: Option<&'static str>,
    line: Option<u32>,
}

impl From<&'static Metadata<'static>> for VxMetadata {
    fn from(value: &'static Metadata<'static>) -> Self {
        Self {
            name: value.name(),
            target: value.target(),
            level: value.level().as_str(),
            module_path: value.module_path(),
            file: value.file(),
            line: value.line(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound(deserialize = "'de: 'static"))]
pub struct SpanInfo {
    id: u64,
    metadata: VxMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound(deserialize = "'de: 'static"))]
pub enum TLMsg {
    SpanCreate {
        date: DateTime<Local>,
        span: SpanInfo,
        attributes: SpanAttrs,
        parent_span: Option<SpanInfo>,
    },
    SpanRecordAttr {
        date: DateTime<Local>,
        span: SpanInfo,
        attributes: SpanAttrs,
    },
    SpanEnter {
        date: DateTime<Local>,
        span: SpanInfo,
    },
    SpanLeave {
        date: DateTime<Local>,
        span: SpanInfo,
    },
    SpanClose {
        date: DateTime<Local>,
        span: SpanInfo,
    },
    Event {
        date: DateTime<Local>,
        message: SmolStr,
        metadata: VxMetadata,
        attributes: SpanAttrs,
        span: Option<SpanInfo>,
    },
}

pub trait TracingLiveMsgSubscriber: Send + Sync + 'static {
    fn on_msg(&self, msg: TLMsg);
}

impl<T, F> TracingLiveMsgSubscriber for (T, F)
where
    T: TracingLiveMsgSubscriber,
    F: Fn(&TLMsg) + Send + Sync + 'static,
{
    fn on_msg(&self, msg: TLMsg) {
        (self.1)(&msg);
        self.0.on_msg(msg);
    }
}
pub struct TLLayer<F> {
    pub subscriber: F,
    pub enable_enter: bool,
}

impl<S, F> Layer<S> for TLLayer<F>
where
    for<'a> S: Subscriber + LookupSpan<'a>,
    F: TracingLiveMsgSubscriber,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, _ctx: Context<'_, S>) {
        if attrs.metadata().target().starts_with("h2::proto") {
            return;
        }
        let mut attributes = SpanAttrs::default();
        attrs.record(&mut attributes);
        let parent_id = attrs
            .parent()
            .and_then(|id| _ctx.span(id))
            .or_else(|| _ctx.lookup_current())
            .map(|n| n.id().into_u64());
        let parent = parent_id.map(|n| _ctx.span(&Id::from_u64(n))).flatten();
        let msg = TLMsg::SpanCreate {
            date: Local::now(),
            span: SpanInfo {
                id: id.into_u64(),
                metadata: attrs.metadata().into(),
            },
            attributes,
            parent_span: parent.map(|n| SpanInfo {
                id: n.id().into_u64(),
                metadata: n.metadata().into(),
            }),
        };
        self.subscriber.on_msg(msg);
    }

    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        let date = Local::now();
        let _span = _ctx.span(_span).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let mut attributes = SpanAttrs::default();
        _values.record(&mut attributes);

        let msg = TLMsg::SpanRecordAttr {
            date,
            span: SpanInfo {
                id: _span.id().into_u64(),
                metadata: _span.metadata().into(),
            },
            attributes,
        };

        self.subscriber.on_msg(msg);
    }
    fn on_event(&self, _event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if _event.metadata().target().starts_with("h2::proto") {
            return;
        }
        let date = Local::now();
        let mut attributes = SpanAttrs::default();
        _event.record(&mut attributes);
        let message = attributes.0.remove("message");
        let msg = TLMsg::Event {
            date,
            message: message.map(|n| n.to_smolstr()).unwrap_or("".into()),
            metadata: _event.metadata().into(),
            attributes,
            span: _ctx.event_span(_event).map(|n| SpanInfo {
                id: n.id().into_u64(),
                metadata: n.metadata().into(),
            }),
        };

        self.subscriber.on_msg(msg);
    }
    fn on_enter(&self, _id: &Id, _ctx: Context<'_, S>) {
        if !self.enable_enter {
            return;
        }
        let date = Local::now();
        let _span = _ctx.span(_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let msg = TLMsg::SpanEnter {
            date,
            span: SpanInfo {
                id: _span.id().into_u64(),
                metadata: _span.metadata().into(),
            },
        };

        self.subscriber.on_msg(msg);
    }
    fn on_exit(&self, _id: &Id, _ctx: Context<'_, S>) {
        if !self.enable_enter {
            return;
        }
        let date = Local::now();
        let _span = _ctx.span(_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let msg = TLMsg::SpanLeave {
            date,
            span: SpanInfo {
                id: _span.id().into_u64(),
                metadata: _span.metadata().into(),
            },
        };

        self.subscriber.on_msg(msg);
    }
    fn on_close(&self, _id: Id, _ctx: Context<'_, S>) {
        let date = Local::now();
        let _span = _ctx.span(&_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let msg = TLMsg::SpanClose {
            date,
            span: SpanInfo {
                id: _span.id().into_u64(),
                metadata: _span.metadata().into(),
            },
        };

        self.subscriber.on_msg(msg);
    }
}

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
            file_line: format!(
                "{}:{}",
                value.file.unwrap_or("unknown"),
                value.line.unwrap_or(0)
            ),
        }
    }
}
impl<'a> From<&'a SpanInfo> for crate::proto::SpanInfo {
    fn from(value: &'a SpanInfo) -> Self {
        Self {
            t_id: value.id,
            name: value.metadata.name.into(),
            file_line: format!(
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

impl From<TLMsg> for RecordParam {
    fn from(value: TLMsg) -> Self {
        Self {
            send_time: 0,
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
            } => record_param::Variant::SpanCreate(SpanCreate {
                record_time: date.timestamp_nanos_opt().unwrap(),
                span_info: Some((&span).into()),
                pos_info: Some((&span.metadata).into()),
                parent_span_info: parent_span.map(|n| (&n).into()),
                fields: attributes.into(),
                target: span.metadata.target.into(),
                level: crate::proto::Level::from_str_name(span.metadata.level)
                    .unwrap()
                    .into(),
            }),
            TLMsg::SpanRecordAttr {
                span,
                date,
                attributes,
            } => record_param::Variant::SpanRecordField(SpanRecordField {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some((&span.metadata).into()),
                span_info: Some((&span).into()),
                fields: attributes.into(),
            }),
            TLMsg::SpanEnter { span, date } => record_param::Variant::SpanEnter(SpanEnter {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some((&span.metadata).into()),
                span_info: Some((&span).into()),
            }),
            TLMsg::SpanLeave { span, date } => record_param::Variant::SpanLeave(SpanLeave {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some((&span.metadata).into()),
                span_info: Some((&span).into()),
            }),
            TLMsg::SpanClose { span, date } => record_param::Variant::SpanClose(SpanClose {
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
            } => record_param::Variant::Event(Event {
                record_time: date.timestamp_nanos_opt().unwrap(),
                message: message.into(),
                pos_info: Some((&metadata).into()),
                fields: attributes.into(),
                target: metadata.target.into(),
                span_info: span.map(|n| (&n).into()),
                level: crate::proto::Level::from_str_name(metadata.level)
                    .unwrap()
                    .into(),
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TLAppInfo {
    pub app_id: Uuid,
    pub app_name: String,
    pub app_version: String,
    pub node_id: String,
    pub static_data: HashMap<String, FieldValueVariant>,
    pub data: HashMap<String, FieldValueVariant>,
}

impl TLAppInfo {
    pub fn new(
        app_id: impl Into<Uuid>,
        app_name: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            app_name: app_name.into(),
            app_version: app_version.into(),
            node_id: "default_node".to_string(),
            static_data: Default::default(),
            data: Default::default(),
        }
    }

    pub fn into_app_start(self, run_id: Uuid, rtt: Duration) -> AppStart {
        let record_time = Utc::now().timestamp_nanos_opt().unwrap();
        AppStart {
            record_time,
            id: self.app_id.into(),
            run_id: run_id.into(),
            node_id: self.node_id,
            name: self.app_name,
            version: self.app_version,
            data: self
                .data
                .into_iter()
                .map(|n| (n.0, FieldValue { variant: Some(n.1) }))
                .collect(),
            rtt: rtt.as_secs_f64(),
        }
    }

    pub fn node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = node_id.into();
        self
    }

    pub fn node_name(self, node_name: impl Into<String>) -> Self {
        self.with_data("node_name", node_name.into())
    }

    pub fn brief(self, brief: impl Into<String>) -> Self {
        self.with_data("brief", brief.into())
    }

    pub fn second_name(self, second_name: impl Into<String>) -> Self {
        self.with_data("second_name", second_name.into())
    }

    pub fn with_data(
        mut self,
        name: impl Into<String>,
        value: impl Into<FieldValueVariant>,
    ) -> Self {
        self.data.insert(name.into(), value.into());
        self
    }

    pub fn with_static_data(
        mut self,
        name: impl Into<String>,
        value: impl Into<FieldValueVariant>,
    ) -> Self {
        self.static_data.insert(name.into(), value.into());
        self
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
            eprintln!("failed to send msg. {:?}", err.0.variant);
        }
    }
}
