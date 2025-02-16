use std::sync::Arc;
use tracing::error;

use crate::record::TracingRecordVariant;
use crate::running_app::AppRunRecord;
use crate::tracing_service::{
   TracingRecordFilter, TracingSpanRunDto, TracingTreeRecordVariantDto,
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
    pub fn clear_disconnected(&mut self) {
        self.record_event_senders.retain(|n| !n.0.is_disconnected());
    }
    #[inline]
    pub async fn notify(
       &mut self,
       item: AppRunRecord,
       variant_dto: Option<TracingTreeRecordVariantDto>,
    ) {
        for x in self.record_event_senders.iter_mut() {
            x.2 = item.variant.filter(&x.1)
        }
        let dto = item.into();
        let tree_record_dto = Arc::new(TracingTreeRecordDto::new(dto, variant_dto));
        for sender in self.record_event_senders.iter().filter(|n| n.2) {
            // println!("notify: {tree_record_dto:#?}");
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
