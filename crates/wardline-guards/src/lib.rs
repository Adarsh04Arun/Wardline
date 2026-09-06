//! Built-in reference guards for Wardline.
//!
//! Deliberately few and dependency-light: enough to prove the
//! [`wardline_core::Guard`] trait works for both general application actions
//! and LLM calls, not a comprehensive guard library. Nothing here pulls in an
//! async runtime or a network client.
//!
//! All text guards use `Input = str` and `Output = String`, so they compose
//! in one [`wardline_core::Pipeline`].
//!
//! | Type | Role |
//! |---|---|
//! | [`RegexBlockGuard`] | Block on a pattern match. |
//! | [`RegexRedactGuard`] | Rewrite matches and return [`Verdict::Modify`]. |
//! | [`RateLimitGuard`] | In-process token bucket, keyed from [`Context`]. |
//! | [`PiiGuard`] | Baseline email / phone / SSN detector — not compliance-grade. |
//!
//! [`Verdict::Modify`]: wardline_core::Verdict::Modify
//! [`Context`]: wardline_core::Context

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod pii;
mod rate_limit;
mod regex_filter;

pub use pii::{PiiAction, PiiGuard};
pub use rate_limit::{DEFAULT_MAX_KEYS, RateLimitGuard};
pub use regex_filter::{RegexBlockGuard, RegexRedactGuard};
