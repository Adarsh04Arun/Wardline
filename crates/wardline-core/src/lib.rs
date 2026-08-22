//! Core vocabulary and execution engine for Wardline.
//!
//! This crate defines the `Guard` trait, the verdict/fail-policy types every
//! guard speaks in, and the synchronous pipeline executor that runs guards in
//! order, short-circuits on the first block, and isolates panics.
//!
//! It depends on nothing outside `std` by design: the core must stay
//! embeddable in any process without dragging in an async runtime or an HTTP
//! client. See `docs/IMPLEMENTATION_PLAN.md` — the contents below land in
//! Phase 1 (types) and Phase 2 (pipeline).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
