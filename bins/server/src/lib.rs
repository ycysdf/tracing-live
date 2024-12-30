#![feature(duration_millis_float)]
#![allow(unused_imports)]

use shadow_rs::shadow;

mod dyn_query;
pub mod event_service;
mod global_data;
pub mod grpc_service;
pub mod record;
pub mod running_app;
mod setting_service;
pub mod tracing_service;
mod web_error;
pub mod web_service;

shadow!(build);
