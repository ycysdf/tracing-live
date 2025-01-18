use std::sync::Arc;
use tracing::error;

use crate::record::TracingRecordVariant;
use crate::tracing_service::{
   BigInt, TracingRecordFilter, TracingSpanRunDto, TracingTreeRecordVariantDto,
};
use crate::tracing_service::{TracingRecordDto, TracingTreeRecordDto};

#[derive(Default)]
pub struct EventService {
    pub record_event_senders: Vec<(
        flume::Sender<Arc<TracingTreeRecordDto>>,
        TracingRecordFilter,
        bool,
    )>,
}

impl EventService {
    pub fn need_notify(&self) -> bool {
        !self.record_event_senders.is_empty()
    }
    pub fn clear(&mut self) {
        self.record_event_senders.retain(|n| !n.0.is_disconnected());
    }
    #[inline]
    pub async fn notify(
       &mut self,
       item: TracingRecordVariant,
       record_id: u64,
       variant_dto: Option<TracingTreeRecordVariantDto>,
    ) {
        for x in self.record_event_senders.iter_mut() {
            x.2 = item.filter(&x.1)
        }
        let dto = item.into_dto(record_id);
        let tree_record_dto = Arc::new(TracingTreeRecordDto::new(dto, variant_dto));
        for sender in self.record_event_senders.iter().filter(|n| n.2) {
            let _ = sender.0.send_async(tree_record_dto.clone()).await.inspect_err(|err| {
                error!("Failed to notify tracing of events: {}", err);
            });
        }
    }
    pub fn add_record_watcher(
        &mut self,
        sender: flume::Sender<Arc<TracingTreeRecordDto>>,
        filter: TracingRecordFilter,
    ) {
        self.record_event_senders.push((sender, filter, true));
    }
}
