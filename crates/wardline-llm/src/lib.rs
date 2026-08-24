//! Wraps an LLM call with Wardline input and output pipelines.
//!
//! Built against a blocking HTTP client (`reqwest`'s `blocking` feature) so
//! the reference path stays synchronous end to end: guard the prompt, make
//! the call, guard the response.
//!
//! Contents land in Phase 4 of `docs/IMPLEMENTATION_PLAN.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
