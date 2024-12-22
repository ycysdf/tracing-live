#![allow(async_fn_in_trait)]
pub mod client;
mod futures;
mod tokio;
pub use tracing_lv_proto::*;

use chrono::{DateTime, Local};
pub use futures::*;
use serde::{Deserialize, Serialize};
use smol_str::{format_smolstr, SmolStr, ToSmolStr};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
pub use tokio::*;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Metadata, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

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
        let parent = parent_id
            .map(|n| _ctx.span(&tracing::Id::from_u64(n)))
            .flatten();
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
    fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
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
