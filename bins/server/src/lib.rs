#![feature(duration_millis_float)]
#![feature(unboxed_closures)]
#![allow(unused_imports)]

use crate::tracing_service::BigInt;
use shadow_rs::shadow;
use std::sync::atomic::{AtomicI64, AtomicU64};
use std::{sync, u64};

mod dyn_query;
pub mod event_service;
mod global_data;
pub mod grpc_service;
pub mod record;
pub mod related_event;
pub mod running_app;
mod setting_service;
pub mod tracing_service;
mod web_error;
pub mod web_service;

shadow!(build);

// pub fn u64_to_i64(value: u64) -> i64 {
//     i64::from_le_bytes(value.to_le_bytes())
// }
//
// pub fn i64_to_u64(value: i64) -> u64 {
//     u64::from_le_bytes(value.to_le_bytes())
// }

pub struct RecordIdGenerator(AtomicI64);

impl RecordIdGenerator {
    pub fn reset(&self, value: i64) {
        self.0.swap(value, sync::atomic::Ordering::SeqCst);
    }
    pub fn next(&self) -> i64 {
        self.0.fetch_add(1, sync::atomic::Ordering::SeqCst)
    }
}

pub static RECORD_ID_GENERATOR: RecordIdGenerator = RecordIdGenerator(AtomicI64::new(1));
