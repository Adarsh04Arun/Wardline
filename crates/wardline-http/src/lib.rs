//! `tower::Layer` / axum middleware adapter for Wardline pipelines.
//!
//! **This crate is the one deliberate async boundary in the workspace**, and
//! only because axum requires it. Guard evaluation inside the middleware
//! stays synchronous — the async-ness is the framework's, not Wardline's.
//! With no existing async stack, prefer the blocking
//! `examples/sync_http_server` path.
//!
//! Contents land in Phase 4 of `docs/IMPLEMENTATION_PLAN.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
