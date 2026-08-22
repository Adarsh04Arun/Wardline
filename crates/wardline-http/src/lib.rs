//! `tower::Layer` / axum middleware adapter for Wardline pipelines.
//!
//! **This crate is the one deliberate async boundary in the workspace**, and
//! only because axum requires it. Guard evaluation inside the middleware stays
//! fully synchronous — the async-ness belongs to the surrounding framework,
//! not to Wardline. Teams with no existing async stack should prefer the
//! blocking `examples/sync_http_server` path instead.
//!
//! Contents land in Phase 4 of `docs/IMPLEMENTATION_PLAN.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
