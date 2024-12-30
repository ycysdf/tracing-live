mod flags;
pub use flags::*;
#[path = "generated_proto.rs"]
pub mod proto;
mod tracing_layer;
pub use tracing_layer::*;

// tonic::include_proto!("tracing");
