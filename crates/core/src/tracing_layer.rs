use crate::proto::{TLRecordVariant, TLValue, TracingRecordItem};
use crate::{FLAGS_AUTO_EXPAND, FLAGS_FORK};
use alloc::borrow::Cow;
use alloc::string::String;
use chrono::{DateTime, Utc};
use core::fmt::{Debug, Display};
use derive_more::{Constructor, Deref, DerefMut, From};
use hashbrown::HashMap;
use portable_atomic::AtomicU64;
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr, format_smolstr};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::{Id, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::{LookupSpan, SpanRef};
use uuid::Uuid;

pub const FIELD_DATA_STABLE_SPAN_ID: &'static str = "sid";
// pub const FIELD_DATA_PARENT_SPAN_ID: &'static str = "__data.parent_span_id";
pub const FIELD_DATA_SPAN_T_ID: &'static str = "__data.span_t_id";
pub const FIELD_DATA_FLAGS: &'static str = "__data.flags";
pub const FIELD_DATA_EMPTY_CHILDREN: &'static str = "__data.empty_children";
pub const FIELD_DATA_FIRST_EVENT: &'static str = "__data.first_event";
pub const FIELD_DATA_IS_CONTAINS_RELATED: &'static str = "__data.is_contains_related";
pub const FIELD_DATA_RELATED_NAME: &'static str = "__data.related_name";
pub const FIELD_DATA_REPEATED_COUNT: &'static str = "__data.repeated_count";
pub const FIELD_DATA_LAST_REPEATED_TIME: &'static str = "__data.last_repeated_time";
pub const CONVENTION_FLAGS_FIELD: &'static str = FLAGS_AUTO_EXPAND;
pub const CONVENTION_FLAGS_FORK: &'static str = FLAGS_FORK;

bitflags::bitflags! {
    #[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Hash)]
    pub struct TracingRecordFlags: u64 {
        const RESERVE = 1;
        const AUTO_EXPAND = 1 << 1;
        const FORK = 1 << 2;
    }
}

#[derive(Default, Clone, Debug, Deref, DerefMut, Constructor, Serialize, Deserialize)]
pub struct TracingFields(HashMap<SmolStr, TLValue>);

impl Visit for TracingFields {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().into(), TLValue::F64(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), TLValue::I64(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), TLValue::U64(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), TLValue::Bool(value));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(
            field.name().into(),
            TLValue::String(if value.bytes().len() > MAX_RECORD_LEN {
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
                field.name().into(),
                TLValue::String(format_smolstr!("<value too long. {}>", str.bytes().len())),
            );
        }
        if str.contains("\x00") {
            self.0.insert(
                field.name().into(),
                TLValue::String(str.replace("\x00", "").to_smolstr()),
            );
        } else {
            self.0.insert(field.name().into(), TLValue::String(str));
        }
    }
}

#[cfg(feature = "serde_json")]
impl From<serde_json::Value> for TracingFields {
    fn from(value: serde_json::Value) -> Self {
        let serde_json::Value::Object(value) = value else {
            unreachable!()
        };
        let fields: HashMap<SmolStr, TLValue> =
            value.into_iter().map(|n| (n.0.into(), n.1.try_into().unwrap())).collect();
        fields.into()
    }
}

impl From<HashMap<SmolStr, TLValue>> for TracingFields {
    fn from(value: HashMap<SmolStr, TLValue>) -> Self {
        let mut fields = Self(value);
        let mut flags = fields.get_flags().unwrap_or(TracingRecordFlags::empty());
        if let Some(_) = fields.remove(FLAGS_AUTO_EXPAND) {
            flags |= TracingRecordFlags::AUTO_EXPAND;
        }
        if let Some(_) = fields.remove(FLAGS_FORK) {
            flags |= TracingRecordFlags::FORK;
        }
        fields.insert_flags(flags);
        fields
    }
}

impl TracingFields {
    pub fn stable_span_id(&self) -> Option<&str> {
        let stable_span_id = self.0.get(FIELD_DATA_STABLE_SPAN_ID);
        stable_span_id.map(|n| {
            let TLValue::String(sid) = n else {
                unreachable!()
            };
            sid.as_str()
        })
    }
    pub fn extend(&mut self, other: &mut Self) {
        for (key, value) in other.drain() {
            self.0.insert(key, value);
        }
    }
    pub fn insert_flags(&mut self, value: TracingRecordFlags) -> Option<TLValue> {
        self.0
            .insert(FIELD_DATA_FLAGS.into(), TLValue::U64(value.bits()))
    }
    pub fn get_flags(&mut self) -> Option<TracingRecordFlags> {
        let value = self.0.get(FIELD_DATA_FLAGS)?;
        let TLValue::U64(flags) = value else {
            return None;
        };
        TracingRecordFlags::from_bits(*flags)
    }
    pub fn insert_empty_children(&mut self) -> Option<TLValue> {
        self.insert_field(FIELD_DATA_EMPTY_CHILDREN, true)
    }

    pub fn insert_field(
        &mut self,
        name: impl Into<SmolStr>,
        value: impl Into<TLValue>,
    ) -> Option<TLValue> {
        self.0.insert(name.into(), value.into())
    }

    pub fn update_to_no_empty_children(&mut self) -> Option<TLValue> {
        self.insert_field(FIELD_DATA_EMPTY_CHILDREN, false)
    }
    pub fn update_to_contains_related(&mut self) -> Option<TLValue> {
        self.insert_field(FIELD_DATA_IS_CONTAINS_RELATED, false)
    }
    pub fn insert_span_trace_id(&mut self, value: Option<u64>) -> Option<TLValue> {
        if value.is_none() {
            return None;
        }
        self.insert_field(
            FIELD_DATA_SPAN_T_ID,
            value
                .map(|t_id| TLValue::U64(t_id))
                .unwrap_or(TLValue::Null),
        )
    }

    pub fn get_span_t_id(&self) -> Option<u64> {
        let field_value = self.get(FIELD_DATA_SPAN_T_ID)?;
        let TLValue::U64(t_id) = field_value else {
            unreachable!()
        };
        Some(*t_id)
    }

    #[cfg(feature = "serde_json")]
    pub fn into_json_iter(self) -> impl Iterator<Item = (SmolStr, serde_json::Value)> {
        self.0.into_iter().map(|(k, v)| (k, v.into()))
    }

    #[cfg(feature = "serde_json")]
    pub fn into_json_map(self) -> serde_json::Map<String, serde_json::Value> {
        self.into_json_iter().map(|n| (n.0.to_string(),n.1)).collect()
    }

    #[cfg(feature = "serde_json")]
    pub fn into_json_value(self) -> serde_json::Value {
        serde_json::Value::Object(self.into_json_iter().map(|n| (n.0.to_string(),n.1)).collect())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VxMetadata {
    pub name: Cow<'static, str>,
    pub target: Cow<'static, str>,
    pub level: Cow<'static, str>,
    pub module_path: Option<Cow<'static, str>>,
    pub file: Option<Cow<'static, str>>,
    pub line: Option<u32>,
}

impl From<&'static Metadata<'static>> for VxMetadata {
    fn from(value: &'static Metadata<'static>) -> Self {
        Self {
            name: value.name().into(),
            target: value.target().into(),
            level: value.level().as_str().into(),
            module_path: value.module_path().map(|n| n.into()),
            file: value.file().map(|n| n.into()),
            line: value.line(),
        }
    }
}

pub const MAX_RECORD_LEN: usize = 1024 * 1024;

#[derive(Serialize, Deref, DerefMut, Deserialize, Debug, Clone)]
pub struct SpanRawInfo {
    pub id: u64,
    #[deref_mut]
    #[deref]
    pub metadata: VxMetadata,
}
impl<'a, R> From<SpanRef<'a, R>> for SpanRawInfo
where
    R: LookupSpan<'a>,
{
    fn from(value: SpanRef<'a, R>) -> Self {
        Self {
            id: value.id().into_u64(),
            metadata: value.metadata().into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TLMsg {
    SpanCreate {
        record_index: u64,
        record_date: DateTime<Utc>,
        span: SpanRawInfo,
        fields: TracingFields,
        parent_span: Option<SpanRawInfo>,
    },
    SpanRecordField {
        record_index: u64,
        record_date: DateTime<Utc>,
        span: SpanRawInfo,
        fields: TracingFields,
        parent_span: Option<SpanRawInfo>,
    },
    SpanEnter {
        record_index: u64,
        record_date: DateTime<Utc>,
        span: SpanRawInfo,
        parent_span: Option<SpanRawInfo>,
    },
    SpanLeave {
        record_index: u64,
        record_date: DateTime<Utc>,
        span: SpanRawInfo,
        parent_span: Option<SpanRawInfo>,
    },
    SpanClose {
        record_index: u64,
        record_date: DateTime<Utc>,
        span: SpanRawInfo,
        parent_span: Option<SpanRawInfo>,
    },
    Event {
        record_index: u64,
        record_date: DateTime<Utc>,
        message: SmolStr,
        metadata: VxMetadata,
        fields: TracingFields,
        span: Option<SpanRawInfo>,
        parent_span: Option<SpanRawInfo>,
    },
}

impl TLMsg {
    pub fn record_index(&self) -> u64 {
        match self {
            TLMsg::SpanCreate { record_index, .. } => *record_index,
            TLMsg::SpanRecordField { record_index, .. } => *record_index,
            TLMsg::SpanEnter { record_index, .. } => *record_index,
            TLMsg::SpanLeave { record_index, .. } => *record_index,
            TLMsg::SpanClose { record_index, .. } => *record_index,
            TLMsg::Event { record_index, .. } => *record_index,
        }
    }
}

pub trait TracingLiveMsgSubscriber: Send + Sync + 'static {
    fn on_msg(&self, msg: TLMsg);
}

impl TracingLiveMsgSubscriber for alloc::boxed::Box<dyn TracingLiveMsgSubscriber> {
    fn on_msg(&self, msg: TLMsg) {
        (**self).on_msg(msg)
    }
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
impl<T, F> TracingLiveMsgSubscriber for (T, Option<F>)
where
    T: TracingLiveMsgSubscriber,
    F: Fn(&TLMsg) + Send + Sync + 'static,
{
    fn on_msg(&self, msg: TLMsg) {
        if let Some(f) = &self.1 {
            f(&msg);
        }
        self.0.on_msg(msg);
    }
}

pub struct TLLayer<F> {
    pub subscriber: F,
    pub enable_enter: bool,
    pub record_index: AtomicU64,
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
        let mut fields = TracingFields::default();
        attrs.record(&mut fields);
        let parent_id = attrs
            .parent()
            .and_then(|id| _ctx.span(id))
            .or_else(|| _ctx.lookup_current())
            .map(|n| n.id().into_u64());
        let parent = parent_id.map(|n| _ctx.span(&Id::from_u64(n))).flatten();
        let msg = TLMsg::SpanCreate {
            record_index: self
                .record_index
                .fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            record_date: now(),
            span: SpanRawInfo {
                id: id.into_u64(),
                metadata: attrs.metadata().into(),
            },
            fields,
            parent_span: parent.map(|n| n.into()),
        };
        self.subscriber.on_msg(msg);
    }

    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        let date = now();
        let _span = _ctx.span(_span).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let parent_span = _span.parent();
        let mut attributes = TracingFields::default();
        _values.record(&mut attributes);

        let msg = TLMsg::SpanRecordField {
            record_index: self
                .record_index
                .fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            record_date: date,
            span: _span.into(),
            fields: attributes,
            parent_span: parent_span.map(|n| n.into()),
        };

        self.subscriber.on_msg(msg);
    }
    fn on_event(&self, _event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if _event.metadata().target().starts_with("h2::proto") {
            return;
        }
        let span = _ctx.event_span(_event).map(|n| n.into());
        let parent_span = span.as_ref().and_then(|n:&SpanRawInfo| _ctx.span(&Id::from_u64(n.id))).map(|n| n.into());
        let date = now();
        let mut fields = TracingFields::default();
        _event.record(&mut fields);
        let message = fields.0.remove("message");
        let msg = TLMsg::Event {
            record_index: self
                .record_index
                .fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            record_date: date,
            message: message.map(|n| n.to_smolstr()).unwrap_or("".into()),
            metadata: _event.metadata().into(),
            fields,
            span,
            parent_span,
        };

        self.subscriber.on_msg(msg);
    }
    fn on_enter(&self, _id: &Id, _ctx: Context<'_, S>) {
        if !self.enable_enter {
            return;
        }
        let date = now();
        let _span = _ctx.span(_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let parent_span = _span.parent();
        let msg = TLMsg::SpanEnter {
            record_index: self
                .record_index
                .fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            record_date: date,
            span: _span.into(),
            parent_span: parent_span.map(|n| n.into()),
        };

        self.subscriber.on_msg(msg);
    }
    fn on_exit(&self, _id: &Id, _ctx: Context<'_, S>) {
        if !self.enable_enter {
            return;
        }
        let date = now();
        let _span = _ctx.span(_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let parent_span = _span.parent();
        let msg = TLMsg::SpanLeave {
            record_index: self
                .record_index
                .fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            record_date: date,
            span: _span.into(),
            parent_span: parent_span.map(|n| n.into()),
        };

        self.subscriber.on_msg(msg);
    }
    fn on_close(&self, _id: Id, _ctx: Context<'_, S>) {
        let date = now();
        let _span = _ctx.span(&_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let parent_span = _span.parent();
        let msg = TLMsg::SpanClose {
            record_index: self
                .record_index
                .fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            record_date: date,
            span: _span.into(),
            parent_span: parent_span.map(|n| n.into()),
        };

        self.subscriber.on_msg(msg);
    }
}

#[derive(Clone, Debug)]
pub struct TLAppInfo {
    pub app_id: Uuid,
    pub app_name: SmolStr,
    pub app_version: SmolStr,
    pub node_id: SmolStr,
    pub static_data: HashMap<SmolStr, TLValue>,
    pub data: HashMap<SmolStr, TLValue>,
}

impl TLAppInfo {
    pub fn new(
        app_id: impl Into<Uuid>,
        app_name: impl Into<SmolStr>,
        app_version: impl Into<SmolStr>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            app_name: app_name.into(),
            app_version: app_version.into(),
            node_id: "default_node".into(),
            static_data: Default::default(),
            data: Default::default(),
        }
    }

    pub fn node_id(mut self, node_id: impl Into<SmolStr>) -> Self {
        self.node_id = node_id.into();
        self
    }

    pub fn node_name(self, node_name: impl Into<SmolStr>) -> Self {
        self.with_data("node_name", node_name.into())
    }

    pub fn brief(self, brief: impl Into<SmolStr>) -> Self {
        self.with_data("brief", brief.into())
    }

    pub fn second_name(self, second_name: impl Into<SmolStr>) -> Self {
        self.with_data("second_name", second_name.into())
    }

    pub fn with_data(mut self, name: impl Into<SmolStr>, value: impl Into<TLValue>) -> Self {
        self.data.insert(name.into(), value.into());
        self
    }

    pub fn with_static_data(mut self, name: impl Into<SmolStr>, value: impl Into<TLValue>) -> Self {
        self.static_data.insert(name.into(), value.into());
        self
    }
}

#[cfg(feature = "std")]
fn now() -> DateTime<Utc> {
    Utc::now()
}

#[cfg(not(feature = "std"))]
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_nanos(0)
}

pub struct MsgReceiverSubscriber {
    sender: flume::Sender<TracingRecordItem>,
}

impl MsgReceiverSubscriber {
    pub fn new(sender: flume::Sender<TracingRecordItem>) -> Self {
        Self { sender }
    }
}

impl TracingLiveMsgSubscriber for MsgReceiverSubscriber {
    fn on_msg(&self, msg: TLMsg) {
        if self.sender.is_disconnected() {
            return;
        }
        if let Err(err) = self.sender.send(msg.into()) {
            if let TLRecordVariant::TLMsg(TLMsg::Event { metadata, .. }) = &err.0.variant {
                if metadata.target == "tower::buffer::worker" {
                    return;
                }
            }
            std::eprintln!("failed to send msg. {:?}", err.0.variant);
        }
    }
}
