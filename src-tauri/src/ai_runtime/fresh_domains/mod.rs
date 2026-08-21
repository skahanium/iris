//! Normalized contracts and deterministic validation for current-fact domains.

#![allow(
    dead_code,
    reason = "The domain DTO set deliberately models all supported operations; some variants are consumed through mapped provider output rather than directly constructed in every production module"
)]

pub(crate) mod contracts;
pub(crate) mod host_renderer;
pub(crate) mod location;
pub(crate) mod provider;
pub(crate) mod service;
#[cfg(test)]
mod tests;
pub(crate) mod validation;
