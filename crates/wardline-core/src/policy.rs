//! What the pipeline does when a guard fails rather than decides.

/// How the pipeline resolves a guard that errored, timed out, or panicked.
///
/// Every guard declares its own policy via [`Guard::fail_policy`]; the
/// pipeline never applies a workspace-wide default silently, so the blast
/// radius of a broken guard is readable off the guard itself.
///
/// [`Guard::fail_policy`]: crate::Guard::fail_policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailPolicy {
    /// Treat the failure as allow and continue.
    ///
    /// For advisory guards only — a guard that fails open is a guard that,
    /// when broken, silently stops guarding.
    FailOpen,
    /// Treat the failure as a block and stop. The default.
    ///
    /// The block reason names the guard and the error, so an outage stays
    /// distinguishable from a real refusal in the trace.
    FailClosed,
    /// Stop and substitute a caller-provided fallback verdict.
    ///
    /// The fallback goes to the pipeline, not the guard, because the safe
    /// answer is a property of the call site.
    FailClosedWithFallback,
}

impl FailPolicy {
    /// Returns `true` if a failure under this policy lets the pipeline keep
    /// running.
    pub fn continues_on_failure(&self) -> bool {
        matches!(self, FailPolicy::FailOpen)
    }

    /// Returns `true` if a failure under this policy stops the pipeline.
    pub fn halts_on_failure(&self) -> bool {
        !self.continues_on_failure()
    }
}

impl Default for FailPolicy {
    /// Fail closed.
    fn default() -> Self {
        FailPolicy::FailClosed
    }
}

impl core::fmt::Display for FailPolicy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            FailPolicy::FailOpen => "fail-open",
            FailPolicy::FailClosed => "fail-closed",
            FailPolicy::FailClosedWithFallback => "fail-closed-with-fallback",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant, once. A new variant fails to compile here until it is
    /// considered — which is the point.
    const ALL: [FailPolicy; 3] = [
        FailPolicy::FailOpen,
        FailPolicy::FailClosed,
        FailPolicy::FailClosedWithFallback,
    ];

    #[test]
    fn every_variant_is_covered_and_halting_is_the_majority() {
        for policy in ALL {
            // Exhaustive match, no wildcard arm.
            let halts = match policy {
                FailPolicy::FailOpen => false,
                FailPolicy::FailClosed => true,
                FailPolicy::FailClosedWithFallback => true,
            };
            assert_eq!(policy.halts_on_failure(), halts, "{policy}");
            assert_eq!(policy.continues_on_failure(), !halts, "{policy}");
        }
    }

    #[test]
    fn only_fail_open_continues() {
        let continuing: Vec<_> = ALL
            .into_iter()
            .filter(FailPolicy::continues_on_failure)
            .collect();
        assert_eq!(continuing, vec![FailPolicy::FailOpen]);
    }

    #[test]
    fn default_is_fail_closed() {
        assert_eq!(FailPolicy::default(), FailPolicy::FailClosed);
    }

    #[test]
    fn display_is_stable_for_logs() {
        assert_eq!(FailPolicy::FailOpen.to_string(), "fail-open");
        assert_eq!(FailPolicy::FailClosed.to_string(), "fail-closed");
        assert_eq!(
            FailPolicy::FailClosedWithFallback.to_string(),
            "fail-closed-with-fallback"
        );
    }
}
