#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;
mod flags;
pub use flags::*;
#[cfg(feature = "std")]
#[path = "tracing.rs"]
pub mod proto;
#[cfg(feature = "std")]
mod tonic;
#[cfg(feature = "std")]
pub use tonic::*;

mod tracing_layer;
#[cfg(feature = "std")]
pub mod catch_panic;

pub use tracing_layer::*;
#[cfg(all(feature = "build-proto", not(feature = "build-proto-dev")))]
::tonic::include_proto!("tracing");
