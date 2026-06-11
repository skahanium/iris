//! Skill runtime facade.
//!
//! The implementation is kept in `skills_impl.rs` while callers continue to
//! use `crate::ai_runtime::skills::*`.

#[path = "skills_impl.rs"]
mod implementation;

pub use implementation::*;
