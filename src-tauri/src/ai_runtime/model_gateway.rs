//! Model gateway facade.
//!
//! The implementation is kept in `model_gateway_impl.rs` while callers
//! continue to use `crate::ai_runtime::model_gateway::*`.

#[path = "model_gateway_impl.rs"]
mod implementation;

pub use implementation::*;
