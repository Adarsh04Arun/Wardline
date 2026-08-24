//! Failures that stop a guard from reaching a decision.

use core::fmt;

/// Something went wrong while evaluating a guard.
///
/// A `GuardError` means the guard did *not* decide — it is not a decision to
/// block. Conflating "the check failed" with "the check says no" is how a
/// broken dependency becomes a silent hole. Blocking is [`Verdict::Block`];
/// failing is this, resolved by the guard's own [`FailPolicy`].
///
/// `#[non_exhaustive]`: match with a wildcard arm, since new failure modes
/// may arrive in a minor release.
///
/// [`Verdict::Block`]: crate::Verdict::Block
/// [`FailPolicy`]: crate::FailPolicy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GuardError {
    /// The guard did not finish within its declared [`Guard::timeout`].
    ///
    /// For a non-strict guard this is best-effort: the pipeline stops
    /// *waiting*, but the work may still be running. See
    /// `docs/RELIABILITY.md`.
    ///
    /// [`Guard::timeout`]: crate::Guard::timeout
    Timeout,

    /// A strict guard blew through its deadline.
    ///
    /// Unlike [`GuardError::Timeout`], this reports a contract violation: a
    /// [`Guard::strict`] guard promised to consult the deadline and return on
    /// its own. Produced by the pipeline, not by guard authors.
    ///
    /// [`Guard::strict`]: crate::Guard::strict
    DeadlineViolated,

    /// The guard panicked and the pipeline caught it. Carries the panic
    /// message where one could be recovered.
    ///
    /// Produced by the pipeline, not by guard authors — deliberate failures
    /// should be [`GuardError::Internal`].
    Panicked(String),

    /// The guard's own logic failed: a malformed rule, an impossible state, a
    /// config error found at check time.
    Internal(String),

    /// Something the guard relies on failed: a datastore, a model endpoint.
    ///
    /// Separate from [`GuardError::Internal`] because the operational
    /// response differs — a bug in the guard versus an outage elsewhere — and
    /// because this is the variant worth retrying.
    Dependency(String),
}

impl GuardError {
    /// Builds a [`GuardError::Internal`] from anything string-like.
    pub fn internal(message: impl Into<String>) -> Self {
        GuardError::Internal(message.into())
    }

    /// Builds a [`GuardError::Dependency`] from anything string-like.
    pub fn dependency(message: impl Into<String>) -> Self {
        GuardError::Dependency(message.into())
    }

    /// Returns `true` for both the soft ([`GuardError::Timeout`]) and hard
    /// ([`GuardError::DeadlineViolated`]) time failures.
    pub fn is_timeout(&self) -> bool {
        matches!(self, GuardError::Timeout | GuardError::DeadlineViolated)
    }

    /// Returns `true` if this failure came from a caught panic.
    pub fn is_panic(&self) -> bool {
        matches!(self, GuardError::Panicked(_))
    }

    /// A short, stable label for metrics and log fields.
    ///
    /// Unlike `Display`, it never includes the message payload, so it is safe
    /// as a bounded-cardinality key.
    pub fn kind(&self) -> &'static str {
        match self {
            GuardError::Timeout => "timeout",
            GuardError::DeadlineViolated => "deadline_violated",
            GuardError::Panicked(_) => "panicked",
            GuardError::Internal(_) => "internal",
            GuardError::Dependency(_) => "dependency",
        }
    }

    /// The message carried by this error, if it has one.
    pub fn message(&self) -> Option<&str> {
        match self {
            GuardError::Panicked(message)
            | GuardError::Internal(message)
            | GuardError::Dependency(message) => Some(message),
            GuardError::Timeout | GuardError::DeadlineViolated => None,
        }
    }
}

impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardError::Timeout => f.write_str("guard timed out"),
            GuardError::DeadlineViolated => {
                f.write_str("strict guard exceeded its deadline without yielding")
            }
            GuardError::Panicked(message) => write!(f, "guard panicked: {message}"),
            GuardError::Internal(message) => write!(f, "guard failed internally: {message}"),
            GuardError::Dependency(message) => write!(f, "guard dependency failed: {message}"),
        }
    }
}

impl std::error::Error for GuardError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<GuardError> {
        vec![
            GuardError::Timeout,
            GuardError::DeadlineViolated,
            GuardError::Panicked("boom".to_owned()),
            GuardError::Internal("bad rule".to_owned()),
            GuardError::Dependency("classifier unreachable".to_owned()),
        ]
    }

    #[test]
    fn kinds_are_unique_and_payload_free() {
        let mut kinds: Vec<_> = all().iter().map(GuardError::kind).collect();
        let count = kinds.len();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), count, "two variants share a kind label");
        for error in all() {
            assert!(
                !error.kind().contains(' '),
                "kind labels must be usable as metric dimensions"
            );
        }
    }

    #[test]
    fn timeout_predicates_cover_both_time_variants() {
        assert!(GuardError::Timeout.is_timeout());
        assert!(GuardError::DeadlineViolated.is_timeout());
        assert!(!GuardError::internal("x").is_timeout());
        assert!(!GuardError::dependency("x").is_timeout());
    }

    #[test]
    fn only_panicked_is_a_panic() {
        assert!(GuardError::Panicked("boom".to_owned()).is_panic());
        assert!(!GuardError::internal("boom").is_panic());
    }

    #[test]
    fn messages_are_carried_by_the_variants_that_have_them() {
        assert_eq!(GuardError::internal("bad rule").message(), Some("bad rule"));
        assert_eq!(GuardError::dependency("down").message(), Some("down"));
        assert_eq!(
            GuardError::Panicked("boom".to_owned()).message(),
            Some("boom")
        );
        assert_eq!(GuardError::Timeout.message(), None);
        assert_eq!(GuardError::DeadlineViolated.message(), None);
    }

    #[test]
    fn display_includes_the_message_for_diagnostics() {
        assert_eq!(
            GuardError::Panicked("index out of bounds".to_owned()).to_string(),
            "guard panicked: index out of bounds"
        );
        assert_eq!(GuardError::Timeout.to_string(), "guard timed out");
    }

    #[test]
    fn is_a_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        for error in all() {
            assert_error(&error);
        }
    }
}
