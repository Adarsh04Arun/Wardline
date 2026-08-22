//! Built-in reference guards for Wardline.
//!
//! These are deliberately few and dependency-light: enough to prove the
//! `wardline_core::Guard` trait works for both general application actions
//! and LLM calls, not a comprehensive guard library. Guards land here in
//! Phase 3 of `docs/IMPLEMENTATION_PLAN.md`.
//!
//! Like `wardline-core`, nothing in this crate pulls in an async runtime or a
//! network client.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
