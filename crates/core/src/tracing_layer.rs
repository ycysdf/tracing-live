use alloc::borrow::Cow;
use chrono::{DateTime, Utc};
use core::fmt::{Debug, Display, Formatter};
use core::sync::atomic::AtomicU64;
use derive_more::From;
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use smol_str::{format_smolstr, SmolStr, ToSmolStr};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::{Id, Metadata, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SpanAttrs(pub HashMap<Cow<'static, str>, TLValue>);

impl Visit for SpanAttrs {
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
                field.name().into(),
                TLValue::Str(format_smolstr!("<value too long. {}>", str.bytes().len())),
            );
        }
        if str.contains("\x00") {
            self.0.insert(
                field.name().into(),
                TLValue::Str(str.replace("\x00", "").to_smolstr()),
            );
        } else {
            self.0.insert(field.name().into(), TLValue::Str(str));
        }
    }
}

#[derive(Debug, From, Clone, PartialEq, Serialize, Deserialize)]
pub enum TLValue {
    F64(f64),
    I64(i64),
    U64(u64),
    Bool(bool),
    Str(SmolStr),
}

impl Display for TLValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
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

impl From<&'static str> for TLValue {
    fn from(value: &'static str) -> Self {
        TLValue::Str(value.into())
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
            module_path: value.module_path().map(|n|n.into()),
            file: value.file().map(|n|n.into()),
            line: value.line(),
        }
    }
}

pub const MAX_RECORD_LEN: usize = 1024 * 1024;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SpanInfo {
    pub id: u64,
    pub metadata: VxMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TLMsg {
    SpanCreate {
        record_index: u64,
        date: DateTime<Utc>,
        span: SpanInfo,
        attributes: SpanAttrs,
        parent_span: Option<SpanInfo>,
    },
    SpanRecordAttr {
        record_index: u64,
        date: DateTime<Utc>,
        span: SpanInfo,
        attributes: SpanAttrs,
    },
    SpanEnter {
        record_index: u64,
        date: DateTime<Utc>,
        span: SpanInfo,
    },
    SpanLeave {
        record_index: u64,
        date: DateTime<Utc>,
        span: SpanInfo,
    },
    SpanClose {
        record_index: u64,
        date: DateTime<Utc>,
        span: SpanInfo,
    },
    Event {
        record_index: u64,
        date: DateTime<Utc>,
        message: SmolStr,
        metadata: VxMetadata,
        attributes: SpanAttrs,
        span: Option<SpanInfo>,
    },
}

impl TLMsg {
    pub fn record_index(&self)->u64 {
         match self {
               TLMsg::SpanCreate { record_index, .. } => *record_index,
               TLMsg::SpanRecordAttr { record_index, .. } => *record_index,
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
        let mut attributes = SpanAttrs::default();
        attrs.record(&mut attributes);
        let parent_id = attrs
            .parent()
            .and_then(|id| _ctx.span(id))
            .or_else(|| _ctx.lookup_current())
            .map(|n| n.id().into_u64());
        let parent = parent_id.map(|n| _ctx.span(&Id::from_u64(n))).flatten();
        let msg = TLMsg::SpanCreate {
            record_index: self.record_index.fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            date: now(),
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
        let date = now();
        let _span = _ctx.span(_span).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let mut attributes = SpanAttrs::default();
        _values.record(&mut attributes);

        let msg = TLMsg::SpanRecordAttr {
            record_index: self.record_index.fetch_add(1, core::sync::atomic::Ordering::SeqCst),
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
        let date = now();
        let mut attributes = SpanAttrs::default();
        _event.record(&mut attributes);
        let message = attributes.0.remove("message");
        let msg = TLMsg::Event {
            record_index: self.record_index.fetch_add(1, core::sync::atomic::Ordering::SeqCst),
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
        let date = now();
        let _span = _ctx.span(_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let msg = TLMsg::SpanEnter {
            record_index: self.record_index.fetch_add(1, core::sync::atomic::Ordering::SeqCst),
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
        let date = now();
        let _span = _ctx.span(_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let msg = TLMsg::SpanLeave {
            record_index: self.record_index.fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            date,
            span: SpanInfo {
                id: _span.id().into_u64(),
                metadata: _span.metadata().into(),
            },
        };

        self.subscriber.on_msg(msg);
    }
    fn on_close(&self, _id: Id, _ctx: Context<'_, S>) {
        let date = now();
        let _span = _ctx.span(&_id).unwrap();
        if _span.metadata().target().starts_with("h2::proto") {
            return;
        }
        let msg = TLMsg::SpanClose {
            record_index: self.record_index.fetch_add(1, core::sync::atomic::Ordering::SeqCst),
            date,
            span: SpanInfo {
                id: _span.id().into_u64(),
                metadata: _span.metadata().into(),
            },
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
