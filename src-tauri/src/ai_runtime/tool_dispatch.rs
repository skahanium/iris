//! Tool dispatch facade.
//!
//! The implementation is kept in `tool_dispatch_impl.rs` while callers
//! continue to use `crate::ai_runtime::tool_dispatch::*`.
//!
//! Source contract anchors:
//! DISPATCHABLE_TOOL_NAMES
//! web_search_tool
//! skills_list_tool

#[path = "tool_dispatch_impl.rs"]
mod implementation;

pub use implementation::*;
