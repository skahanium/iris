//! Tool dispatch facade.
//!
//! The implementation is kept in `tool_dispatch_impl.rs` while callers
//! continue to use `crate::ai_runtime::tool_dispatch::*`.
//!
//! Source contract anchors:
//! DISPATCHABLE_TOOL_NAMES
//! fetch_web_page_tool
//! readability_fetch_tool
//! skills_list_tool
//! skills_install_tool
//! skills_uninstall_tool
//! skills_toggle_tool
//! skills_update_tool
//! skills_read_resource_tool

#[path = "tool_dispatch_impl.rs"]
mod implementation;

pub use implementation::*;
