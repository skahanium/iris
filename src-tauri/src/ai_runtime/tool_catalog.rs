//! Tool catalog facade.
//!
//! The implementation is kept in `tool_catalog_impl.rs` while callers continue
//! to use `crate::ai_runtime::tool_catalog::*`.
//!
//! Source contract anchors:
//! name: "web_search"
//! name: "skills_list"

#[path = "tool_catalog_impl.rs"]
mod implementation;

pub use implementation::*;
