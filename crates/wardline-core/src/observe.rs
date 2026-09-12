//! Feature-gated span/event hooks. Compiles to nothing without `tracing`.

use crate::{GuardError, Verdict};

#[cfg(feature = "tracing")]
use crate::TraceOutcome;

/// Process-wide span covering one [`Pipeline::evaluate`] call.
///
/// [`Pipeline::evaluate`]: crate::Pipeline::evaluate
pub(crate) fn enter_evaluate() -> impl Drop {
    #[cfg(feature = "tracing")]
    {
        tracing::info_span!("wardline.evaluate").entered()
    }
    #[cfg(not(feature = "tracing"))]
    {
        Noop
    }
}

#[cfg(not(feature = "tracing"))]
struct Noop;

#[cfg(not(feature = "tracing"))]
impl Drop for Noop {
    fn drop(&mut self) {}
}

/// Per-guard span. Records the decision or the failure, including panics.
pub(crate) struct GuardSpan {
    #[cfg_attr(not(feature = "tracing"), allow(dead_code))]
    name: &'static str,
    #[cfg(feature = "tracing")]
    _span: tracing::span::EnteredSpan,
}

impl GuardSpan {
    pub(crate) fn enter(name: &'static str) -> Self {
        GuardSpan {
            name,
            #[cfg(feature = "tracing")]
            _span: tracing::info_span!("wardline.guard", guard.name = name).entered(),
        }
    }

    pub(crate) fn decided<O>(&self, verdict: &Verdict<O>) {
        #[cfg(feature = "tracing")]
        {
            tracing::debug!(
                guard.name = self.name,
                outcome = TraceOutcome::from_verdict(verdict).kind(),
                "guard decided"
            );
        }
        #[cfg(not(feature = "tracing"))]
        {
            let _ = (self, verdict);
        }
    }

    pub(crate) fn failed(&self, error: &GuardError) {
        #[cfg(feature = "tracing")]
        {
            if error.is_panic() {
                tracing::error!(
                    guard.name = self.name,
                    outcome = "panicked",
                    panic.message = error.message().unwrap_or("unknown"),
                    "guard panicked"
                );
            } else {
                tracing::warn!(
                    guard.name = self.name,
                    outcome = "failed",
                    error.kind = error.kind(),
                    error = %error,
                    "guard failed"
                );
            }
        }
        #[cfg(not(feature = "tracing"))]
        {
            let _ = (self, error);
        }
    }
}
