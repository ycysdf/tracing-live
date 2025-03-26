use crate::record::{AppRunInfo, EventRecordItem, TracingSpanInfo};
use crate::rpc_service::RunningSpan;
use smol_str::{SmolStr, format_smolstr};
use std::sync::Arc;
use tracing_lv_core::SpanRawInfo;

pub trait SpanRelatedEvent {
    fn is_related_and_handle(
        &self,
        _app_info: &Arc<AppRunInfo>,
        _event_item: &mut EventRecordItem,
    ) -> Option<&'static str>;
}

pub struct ReturnSpanRelatedEvent {}

impl SpanRelatedEvent for ReturnSpanRelatedEvent {
    fn is_related_and_handle(&self, _app_info: &Arc<AppRunInfo>, _event_item: &mut EventRecordItem) -> Option<&'static str> {
        let field = "return";
        let r = _event_item.message.is_empty() && _event_item.fields.contains_key(field);
        if r {
            let value = _event_item.fields.remove(field).unwrap();
            _event_item.message = format_smolstr!("{}", value);
        }
        r.then(|| "ret")
    }
}

pub struct ErrSpanRelatedEvent {}

impl SpanRelatedEvent for ErrSpanRelatedEvent {
    fn is_related_and_handle(&self, _app_info: &Arc<AppRunInfo>, _event_item: &mut EventRecordItem) -> Option<&'static str> {

        let field = "error";
        let r = _event_item.message.is_empty() && _event_item.fields.contains_key(field);
        if r {
            let value = _event_item.fields.remove(field).unwrap();
            _event_item.message = format_smolstr!("{}", value);
        }
        r.then(|| "err")
    }
}

pub struct TowerHttpSpanRelatedEvent {}

impl SpanRelatedEvent for TowerHttpSpanRelatedEvent {
    fn is_related_and_handle(&self, _app_info: &Arc<AppRunInfo>, _event_item: &mut EventRecordItem) -> Option<&'static str> {

        let r = _event_item.message == "finished processing request" && _event_item.span.as_ref().is_some_and(|n| n.name == "request");
        r.then(|| "response")
    }
}
