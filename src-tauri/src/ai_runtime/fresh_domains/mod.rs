//! Normalized contracts and deterministic validation for current-fact domains.

#![allow(
    dead_code,
    reason = "Task 4 provider/service wiring consumes these DTOs; current tests exercise the contract"
)]

pub(crate) mod contracts;
#[cfg(test)]
mod tests;
pub(crate) mod validation;
