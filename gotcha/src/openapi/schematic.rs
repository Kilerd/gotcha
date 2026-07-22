//! The schema layer now lives in the lightweight `gotcha_core` crate.
//!
//! Everything is re-exported here so existing paths such as
//! `gotcha::openapi::schematic::Schematic` keep resolving unchanged. `ParameterProvider`
//! and the axum-facing impls come from `gotcha_core`'s `axum` feature, which the `gotcha`
//! `openapi` feature enables.
pub use gotcha_core::{EnhancedSchema, ParameterProvider, Schematic};
