//! Hybrid retrieval facade.
//!
//! The implementation is kept in `retrieval_broker_impl.rs` while callers
//! continue to use `crate::ai_runtime::retrieval_broker::*`.

#[path = "retrieval_broker_impl.rs"]
mod implementation;

pub use implementation::*;
