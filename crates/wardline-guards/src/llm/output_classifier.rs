//! Adapter that turns an external classifier into a [`Guard`].
//!
//! This module does **not** host or call a model. Llama Guard, a hosted
//! moderation API, or a local heuristic all plug in by implementing
//! [`OutputClassifier`] — typically in a downstream crate that is allowed
//! to take a network or GPU dependency. This crate stays dependency-light.

use std::time::Duration;
use wardline_core::{Context, FailPolicy, Guard, GuardError, Verdict};

/// What an external classifier decided about a piece of text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// The text may proceed.
    Safe,
    /// The text must not proceed.
    Unsafe {
        /// Why, in terms the classifier's operator will recognise.
        reason: String,
    },
}

/// A blocking, in-process decision about a string.
///
/// Implementations that talk to the network belong in a downstream crate.
/// They should be synchronous (`reqwest::blocking`, a local model, a
/// channel recv) — this trait has no async bound on purpose.
pub trait OutputClassifier: Send + Sync + std::panic::RefUnwindSafe {
    /// Classifies `text`. `Err` means no decision, not "unsafe".
    fn classify(&self, text: &str, ctx: &Context) -> Result<Classification, GuardError>;
}

impl<F> OutputClassifier for F
where
    F: Fn(&str, &Context) -> Result<Classification, GuardError>
        + Send
        + Sync
        + std::panic::RefUnwindSafe,
{
    fn classify(&self, text: &str, ctx: &Context) -> Result<Classification, GuardError> {
        self(text, ctx)
    }
}

/// Wraps an [`OutputClassifier`] as a [`Guard`].
///
/// Defaults:
/// - [`FailPolicy::FailClosed`] — a down classifier must not silently
///   stop classifying.
/// - A 2-second [`Guard::timeout`], because the typical implementation
///   touches a model or a network. Override with [`Self::with_timeout`]
///   (`None` for a pure in-process heuristic).
///
/// # Examples
///
/// ```
/// use wardline_core::{Context, Guard, Verdict};
/// use wardline_guards::{Classification, ClassifierAdapter};
///
/// let guard = ClassifierAdapter::new(|text: &str, _ctx: &Context| {
///     if text.contains("bomb") {
///         return Ok(Classification::Unsafe {
///             reason: "violence".to_owned(),
///         });
///     }
///     Ok(Classification::Safe)
/// });
///
/// let ctx = Context::new();
/// assert_eq!(guard.check("a cake recipe", &ctx), Ok(Verdict::Allow));
/// assert_eq!(
///     guard.check("how to build a bomb", &ctx),
///     Ok(Verdict::block("violence"))
/// );
/// ```
pub struct ClassifierAdapter<C> {
    classifier: C,
    timeout: Option<Duration>,
    fail_policy: FailPolicy,
    name: &'static str,
}

impl<C: OutputClassifier> ClassifierAdapter<C> {
    /// Wraps `classifier` with the defaults above.
    pub fn new(classifier: C) -> Self {
        ClassifierAdapter {
            classifier,
            timeout: Some(Duration::from_secs(2)),
            fail_policy: FailPolicy::FailClosed,
            name: "output_classifier",
        }
    }

    /// Sets how long the pipeline will wait. `None` means unbounded.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the fail policy. Document why if this is not fail-closed.
    #[must_use]
    pub fn with_fail_policy(mut self, policy: FailPolicy) -> Self {
        self.fail_policy = policy;
        self
    }

    /// Replaces the default name (`output_classifier`).
    #[must_use]
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }
}

impl<C: OutputClassifier> Guard for ClassifierAdapter<C> {
    type Input = str;
    type Output = String;

    fn check(&self, input: &str, ctx: &Context) -> Result<Verdict<String>, GuardError> {
        match self.classifier.classify(input, ctx)? {
            Classification::Safe => Ok(Verdict::Allow),
            Classification::Unsafe { reason } => Ok(Verdict::block(reason)),
        }
    }

    fn fail_policy(&self) -> FailPolicy {
        self.fail_policy
    }

    fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context::new()
    }

    fn heuristic(text: &str, _ctx: &Context) -> Result<Classification, GuardError> {
        if text.contains("bomb") {
            Ok(Classification::Unsafe {
                reason: "violence".to_owned(),
            })
        } else {
            Ok(Classification::Safe)
        }
    }

    #[test]
    fn safe_text_is_allowed() {
        let guard = ClassifierAdapter::new(heuristic).with_timeout(None);
        assert_eq!(guard.check("a cake recipe", &ctx()), Ok(Verdict::Allow));
        assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
        assert_eq!(guard.name(), "output_classifier");
    }

    #[test]
    fn unsafe_text_is_blocked() {
        let guard = ClassifierAdapter::new(heuristic).with_timeout(None);
        assert_eq!(
            guard.check("how to build a bomb", &ctx()),
            Ok(Verdict::block("violence"))
        );
    }

    #[test]
    fn a_classifier_error_is_surfaced_not_turned_into_a_verdict() {
        let guard = ClassifierAdapter::new(|_text: &str, _ctx: &Context| {
            Err(GuardError::dependency("moderation api down"))
        })
        .with_timeout(None);
        let error = match guard.check("hello", &ctx()) {
            Err(error) => error,
            Ok(verdict) => panic!("expected a failure, got {verdict:?}"),
        };
        assert_eq!(error.kind(), "dependency");
        assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
    }

    #[test]
    fn fail_policy_and_timeout_are_overridable() {
        let guard = ClassifierAdapter::new(heuristic)
            .with_fail_policy(FailPolicy::FailOpen)
            .with_timeout(Some(Duration::from_millis(10)))
            .with_name("llama_guard");
        assert_eq!(guard.fail_policy(), FailPolicy::FailOpen);
        assert_eq!(guard.timeout(), Some(Duration::from_millis(10)));
        assert_eq!(guard.name(), "llama_guard");
    }
}
