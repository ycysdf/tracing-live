use crate::grpc_service::{RunningSpan, SpanInfo, TracingFields};
use crate::record::AppRunInfo;
use smol_str::{format_smolstr, SmolStr};
use std::sync::Arc;
use tracing_lv_core::proto::field_value::Variant;

pub trait SpanRelatedEvent {
    fn is_related_and_handle(
        &self,
        _app_info: &Arc<AppRunInfo>,
        _message: &mut SmolStr,
        _span_info: &Arc<SpanInfo>,
        _running_span: &Option<RunningSpan>,
        _fields: &mut TracingFields,
        _target: &SmolStr,
        _module_path: &Option<SmolStr>,
    ) -> Option<&'static str>;
}

pub struct ReturnSpanRelatedEvent {}

impl SpanRelatedEvent for ReturnSpanRelatedEvent {
    fn is_related_and_handle(
        &self,
        _app_info: &Arc<AppRunInfo>,
        _message: &mut SmolStr,
        _span_info: &Arc<SpanInfo>,
        _running_span: &Option<RunningSpan>,
        _fields: &mut TracingFields,
        _target: &SmolStr,
        _module_path: &Option<SmolStr>,
    ) -> Option<&'static str> {
        let field = "return";
        let r = _message.is_empty() && _fields.contains_key(field);
        if r {
            let value = _fields.remove(field).unwrap();
            *_message = format_smolstr!("{}", value.variant.unwrap());
        }
        r.then(|| "ret")
    }
}

pub struct ErrSpanRelatedEvent {}

impl SpanRelatedEvent for ErrSpanRelatedEvent {
    fn is_related_and_handle(
        &self,
        _app_info: &Arc<AppRunInfo>,
        _message: &mut SmolStr,
        _span_info: &Arc<SpanInfo>,
        _running_span: &Option<RunningSpan>,
        _fields: &mut TracingFields,
        _target: &SmolStr,
        _module_path: &Option<SmolStr>,
    ) -> Option<&'static str> {
        let field = "error";
        let r = _message.is_empty() && _fields.contains_key(field);
        if r {
            let value = _fields.remove(field).unwrap();
            *_message = format_smolstr!("{}", value.variant.unwrap());
        }
        r.then(|| "err")
    }
}

pub struct TowerHttpSpanRelatedEvent {}

impl SpanRelatedEvent for TowerHttpSpanRelatedEvent {
    fn is_related_and_handle(
        &self,
        _app_info: &Arc<AppRunInfo>,
        _message: &mut SmolStr,
        _span_info: &Arc<SpanInfo>,
        _running_span: &Option<RunningSpan>,
        _fields: &mut TracingFields,
        _target: &SmolStr,
        _module_path: &Option<SmolStr>,
    ) -> Option<&'static str> {
        let r = _message == "finished processing request" && _span_info.name == "request";
        r.then(|| "response")
    }
}
