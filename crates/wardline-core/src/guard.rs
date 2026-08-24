//! The one trait everything else in Wardline is built around.

use crate::{Context, FailPolicy, GuardError, Verdict};
use std::time::Duration;

/// A single check that runs before an action is allowed to happen.
///
/// This is the whole extension mechanism: checks are ordinary Rust code, not
/// entries in a config DSL, so they are typed, unit-testable, and reviewable
/// like anything else in your service.
///
/// # Contract
///
/// - **Don't panic.** Return [`GuardError`] instead. The pipeline catches
///   panics too, but that is a backstop for bugs, not a control-flow path.
/// - **Don't block indefinitely.** Declare a [`Guard::timeout`], and consult
///   `ctx.deadline()` if you declare [`Guard::strict`].
/// - **Don't mutate global state.** `&self` is shared across request threads.
///
/// # Bounds
///
/// `Send + Sync` let one pipeline serve many threads. [`RefUnwindSafe`] is
/// required by the pipeline's [`catch_unwind`]; it is here from the start so
/// panic isolation isn't a breaking change later. Interior mutability that
/// fails the bound can be wrapped in [`AssertUnwindSafe`] — once you're
/// satisfied a mid-check panic can't leave it observably broken.
///
/// # Examples
///
/// ```
/// use wardline_core::{Context, FailPolicy, Guard, GuardError, Verdict};
///
/// struct MaxLength {
///     limit: usize,
/// }
///
/// impl Guard for MaxLength {
///     type Input = str;
///     type Output = ();
///
///     fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
///         if input.len() > self.limit {
///             return Ok(Verdict::block(format!(
///                 "input is {} bytes, limit is {}",
///                 input.len(),
///                 self.limit
///             )));
///         }
///         Ok(Verdict::Allow)
///     }
///
///     fn name(&self) -> &'static str {
///         "max_length"
///     }
/// }
///
/// let guard = MaxLength { limit: 8 };
/// let ctx = Context::new();
///
/// assert_eq!(guard.check("short", &ctx), Ok(Verdict::Allow));
/// assert_eq!(
///     guard.check("far too long to pass", &ctx),
///     Ok(Verdict::block("input is 20 bytes, limit is 8"))
/// );
/// assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
/// ```
///
/// [`RefUnwindSafe`]: std::panic::RefUnwindSafe
/// [`AssertUnwindSafe`]: std::panic::AssertUnwindSafe
/// [`catch_unwind`]: std::panic::catch_unwind
pub trait Guard: Send + Sync + std::panic::RefUnwindSafe {
    /// What this guard inspects — a prompt, a request body, a domain action.
    ///
    /// Borrowed, never consumed: several guards examine the same input in
    /// turn, and rewrites go through [`Verdict::Modify`].
    type Input: ?Sized;

    /// The replacement payload for [`Verdict::Modify`]. `()` for a guard that
    /// only allows or blocks.
    type Output;

    /// Decides whether the action may proceed.
    ///
    /// `Ok` for a decision, `Err` when no decision could be reached — the two
    /// are handled very differently (see [`GuardError`]).
    fn check(
        &self,
        input: &Self::Input,
        ctx: &Context,
    ) -> Result<Verdict<Self::Output>, GuardError>;

    /// How the pipeline resolves a failure of *this* guard.
    ///
    /// Failing open deserves a doc comment on the impl saying why this check
    /// is safe to skip when it breaks.
    fn fail_policy(&self) -> FailPolicy {
        FailPolicy::FailClosed
    }

    /// How long this guard may take before the pipeline gives up on it.
    ///
    /// `None` means no enforced bound — fine for pure computation over data
    /// in memory, dangerous for anything touching a network.
    fn timeout(&self) -> Option<Duration> {
        None
    }

    /// Whether this guard enforces its own deadline.
    ///
    /// `false` lets the pipeline bound it from another thread and abandon the
    /// wait on timeout — best-effort, since abandoned work keeps running.
    /// `true` means the guard consults `ctx.deadline()` and returns on its
    /// own; overrunning anyway is reported as
    /// [`GuardError::DeadlineViolated`].
    ///
    /// Setting `true` without actually checking the deadline is worse than
    /// leaving it `false` — it removes the backstop.
    fn strict(&self) -> bool {
        false
    }

    /// A short, stable identifier, `snake_case`, no spaces.
    ///
    /// It appears in traces, block reasons, and metrics, so treat it as
    /// public interface. Name the check ("`prompt_injection`"), not the
    /// implementation ("`regex_v2`").
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Deadline;
    use std::panic::RefUnwindSafe;

    /// Exercises nothing but the defaults.
    struct AlwaysAllow;

    impl Guard for AlwaysAllow {
        type Input = str;
        type Output = ();

        fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
            Ok(Verdict::Allow)
        }

        fn name(&self) -> &'static str {
            "always_allow"
        }
    }

    /// Overrides every default, to prove they are all overridable.
    struct Fussy;

    impl Guard for Fussy {
        type Input = String;
        type Output = String;

        fn check(&self, input: &String, ctx: &Context) -> Result<Verdict<String>, GuardError> {
            if ctx.is_expired() {
                return Err(GuardError::DeadlineViolated);
            }
            if input.contains("secret") {
                return Ok(Verdict::Modify(input.replace("secret", "[redacted]")));
            }
            Ok(Verdict::Allow)
        }

        fn fail_policy(&self) -> FailPolicy {
            FailPolicy::FailOpen
        }

        fn timeout(&self) -> Option<Duration> {
            Some(Duration::from_millis(5))
        }

        fn strict(&self) -> bool {
            true
        }

        fn name(&self) -> &'static str {
            "fussy"
        }
    }

    #[test]
    fn defaults_are_fail_closed_untimed_and_non_strict() {
        let guard = AlwaysAllow;
        assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
        assert_eq!(guard.timeout(), None);
        assert!(!guard.strict());
        assert_eq!(guard.name(), "always_allow");
    }

    #[test]
    fn every_default_can_be_overridden() {
        let guard = Fussy;
        assert_eq!(guard.fail_policy(), FailPolicy::FailOpen);
        assert_eq!(guard.timeout(), Some(Duration::from_millis(5)));
        assert!(guard.strict());
    }

    #[test]
    fn check_returns_decisions_and_failures_distinctly() {
        let guard = Fussy;
        let ctx = Context::new();

        let allowed = guard.check(&"nothing to see".to_owned(), &ctx);
        assert_eq!(allowed, Ok(Verdict::Allow));

        let modified = guard.check(&"a secret value".to_owned(), &ctx);
        assert_eq!(
            modified,
            Ok(Verdict::Modify("a [redacted] value".to_owned()))
        );

        let expired = Context::new().with_deadline(Deadline::after(Duration::ZERO));
        let failed = guard.check(&"anything".to_owned(), &expired);
        assert_eq!(failed, Err(GuardError::DeadlineViolated));
    }

    #[test]
    fn guards_are_object_safe_and_shareable_across_threads() {
        // The pipeline stores `Box<dyn Guard<…>>`, so object safety is part
        // of the contract, as are the bounds that let one pipeline serve many
        // threads and survive `catch_unwind`.
        fn assert_bounds<G: Send + Sync + RefUnwindSafe + ?Sized>() {}
        assert_bounds::<AlwaysAllow>();
        assert_bounds::<dyn Guard<Input = str, Output = ()>>();

        let guards: Vec<Box<dyn Guard<Input = str, Output = ()>>> = vec![Box::new(AlwaysAllow)];
        assert_eq!(guards[0].name(), "always_allow");
    }
}
