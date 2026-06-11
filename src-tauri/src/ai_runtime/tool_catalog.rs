//! Tool catalog facade.
//!
//! The implementation is kept in `tool_catalog_impl.rs` while callers continue
//! to use `crate::ai_runtime::tool_catalog::*`.
//!
//! Source contract anchors:
//! name: "fetch_web_page"
//! requires_confirmation: true
//! name: "readability_fetch"
//! name: "skills_list"
//! name: "skills_install"
//! name: "skills_uninstall"
//! name: "skills_toggle"
//! name: "skills_update"
//! name: "skills_read_resource"

#[path = "tool_catalog_impl.rs"]
mod implementation;

pub use implementation::*;
