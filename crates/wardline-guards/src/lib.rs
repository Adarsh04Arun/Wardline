//! Built-in reference guards for Wardline.
//!
//! Deliberately few and dependency-light: enough to prove the
//! `wardline_core::Guard` trait works for both general application actions
//! and LLM calls, not a comprehensive guard library. Nothing here pulls in an
//! async runtime or a network client.
//!
//! Guards land here in Phase 3 of `docs/IMPLEMENTATION_PLAN.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
