//! Core vocabulary and execution engine for Wardline.
//!
//! Wardline evaluates guardrails inline and synchronously, in the caller's
//! request path: a check returns a blocking verdict *before* an action is
//! taken or an LLM response is released. No queue, no separate service, no
//! async runtime — a blocked action never happens rather than being detected
//! shortly after it did.
//!
//! | Type | Role |
//! |---|---|
//! | [`Guard`] | One check. The trait you implement. |
//! | [`Verdict`] | What a guard decided: allow, block, or modify. |
//! | [`GuardError`] | Why a guard reached no decision at all. |
//! | [`FailPolicy`] | What the pipeline does about that failure. |
//! | [`Context`] | Per-request metadata and the deadline. |
//! | [`Deadline`] | A cooperative wall-clock bound. |
//! | [`Pipeline`] | An ordered list of guards, run in place. |
//! | [`Trace`] | The bounded record of what each guard did. |
//!
//! The key distinction: [`Verdict::Block`] means the guard said *no*;
//! [`GuardError`] means it said *nothing*. Failures are resolved by the
//! failing guard's own [`FailPolicy`], which defaults to
//! [`FailPolicy::FailClosed`], so a broken guard doesn't silently stop
//! guarding.
//!
//! [`Pipeline::evaluate`] runs guards in order, stops at the first block, and
//! returns the decision together with its [`Trace`] — so a caller can log
//! *why* something was refused, not just that it was. This crate depends on
//! nothing outside `std`; see `AGENTS.md` for the architectural invariants.
//!
//! # Example
//!
//! ```
//! use wardline_core::{Context, Guard, GuardError, Verdict};
//!
//! struct NoProfanity;
//!
//! impl Guard for NoProfanity {
//!     type Input = str;
//!     type Output = ();
//!
//!     fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
//!         if input.contains("darn") {
//!             return Ok(Verdict::block("profanity detected"));
//!         }
//!         Ok(Verdict::Allow)
//!     }
//!
//!     fn name(&self) -> &'static str {
//!         "no_profanity"
//!     }
//! }
//!
//! let ctx = Context::new();
//! assert_eq!(NoProfanity.check("hello there", &ctx), Ok(Verdict::Allow));
//! assert!(NoProfanity.check("well darn", &ctx).is_ok_and(|v| v.is_block()));
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod context;
mod deadline;
mod error;
mod guard;
mod pipeline;
mod policy;
mod trace;
mod verdict;

pub use context::{Context, Value};
pub use deadline::Deadline;
pub use error::GuardError;
pub use guard::Guard;
pub use pipeline::{Pipeline, PipelineResult};
pub use policy::FailPolicy;
pub use trace::{Trace, TraceEntry, TraceOutcome};
pub use verdict::Verdict;

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::RefUnwindSafe;

    /// The pipeline borrows a `Context` across a `catch_unwind` boundary
    /// (Phase 4.5). Asserting the bound here means a future field breaks this
    /// test rather than the pipeline.
    #[test]
    fn context_survives_the_catch_unwind_boundary() {
        fn assert_unwind_safe<T: RefUnwindSafe>() {}
        assert_unwind_safe::<Context>();
        assert_unwind_safe::<Deadline>();
        assert_unwind_safe::<Value>();
    }

    /// A pipeline is shared across request threads, so what travels with it
    /// must be `Send + Sync`.
    #[test]
    fn public_types_are_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Context>();
        assert_send_sync::<Deadline>();
        assert_send_sync::<FailPolicy>();
        assert_send_sync::<GuardError>();
        assert_send_sync::<Trace>();
        assert_send_sync::<TraceEntry>();
        assert_send_sync::<TraceOutcome>();
        assert_send_sync::<Value>();
        assert_send_sync::<Verdict<String>>();
        assert_send_sync::<PipelineResult<String>>();
    }
}
