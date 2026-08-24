//! Cooperative wall-clock deadlines.

use core::fmt;
use std::time::{Duration, Instant};

/// A point in time by which work is expected to be finished.
///
/// Cooperative: holding one obliges the holder to check it and give up when
/// it passes. Wardline can detect a guard that ignores its deadline (see
/// [`Guard::strict`]) but cannot interrupt one.
///
/// Absolute rather than relative on purpose — a [`Duration`] passed down a
/// call stack silently restarts at every hop.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use wardline_core::Deadline;
///
/// let deadline = Deadline::after(Duration::from_secs(30));
/// assert!(!deadline.is_expired());
///
/// let past = Deadline::after(Duration::ZERO);
/// assert!(past.is_expired());
/// assert_eq!(past.remaining(), Duration::ZERO);
/// ```
///
/// [`Guard::strict`]: crate::Guard::strict
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Deadline(Instant);

impl Deadline {
    /// Creates a deadline at an exact instant.
    pub fn at(instant: Instant) -> Self {
        Deadline(instant)
    }

    /// Creates a deadline `duration` from now.
    ///
    /// Zero yields an already-expired deadline; for "no deadline" use
    /// `Option<Deadline>` and pass `None`.
    pub fn after(duration: Duration) -> Self {
        Deadline(Instant::now() + duration)
    }

    /// The instant this deadline falls at.
    pub fn instant(&self) -> Instant {
        self.0
    }

    /// Time left, saturating at [`Duration::ZERO`] rather than going
    /// negative.
    ///
    /// It deliberately does not report how late you are — no correct caller
    /// behaves differently based on that.
    pub fn remaining(&self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    /// Returns `true` once the deadline has passed.
    ///
    /// This reads the clock, so two calls can differ. Read it once per
    /// decision.
    pub fn is_expired(&self) -> bool {
        self.remaining() == Duration::ZERO
    }

    /// The earlier of two deadlines — the one that actually binds.
    pub fn min(self, other: Deadline) -> Deadline {
        Deadline(self.0.min(other.0))
    }

    /// A deadline this much earlier, saturating at now.
    ///
    /// Useful for reserving time to handle the timeout itself.
    pub fn less(self, duration: Duration) -> Deadline {
        Deadline(self.0.checked_sub(duration).unwrap_or_else(Instant::now))
    }
}

impl From<Instant> for Deadline {
    fn from(instant: Instant) -> Self {
        Deadline(instant)
    }
}

impl fmt::Display for Deadline {
    /// Renders time remaining — the underlying [`Instant`] has no meaningful
    /// absolute representation to print.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let remaining = self.remaining();
        if remaining == Duration::ZERO {
            f.write_str("deadline expired")
        } else {
            write!(f, "deadline in {remaining:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_future_deadline_is_not_expired() {
        let deadline = Deadline::after(Duration::from_secs(60));
        assert!(!deadline.is_expired());
        assert!(deadline.remaining() > Duration::from_secs(30));
        assert!(deadline.remaining() <= Duration::from_secs(60));
    }

    #[test]
    fn a_zero_deadline_is_immediately_expired() {
        let deadline = Deadline::after(Duration::ZERO);
        assert!(deadline.is_expired());
        assert_eq!(deadline.remaining(), Duration::ZERO);
    }

    #[test]
    fn remaining_saturates_instead_of_underflowing() {
        // `Instant` has no fixed epoch, so a long subtraction may not be
        // representable. Skip rather than panic when it isn't.
        let Some(long_past) = Instant::now().checked_sub(Duration::from_secs(3600)) else {
            return;
        };
        let deadline = Deadline::at(long_past);
        assert!(deadline.is_expired());
        assert_eq!(deadline.remaining(), Duration::ZERO);
    }

    #[test]
    fn a_deadline_expires_once_its_time_passes() {
        let deadline = Deadline::after(Duration::from_millis(20));
        assert!(!deadline.is_expired());
        std::thread::sleep(Duration::from_millis(40));
        assert!(deadline.is_expired());
    }

    #[test]
    fn min_picks_the_binding_deadline() {
        let soon = Deadline::after(Duration::from_secs(1));
        let later = Deadline::after(Duration::from_secs(600));
        assert_eq!(soon.min(later), soon);
        assert_eq!(later.min(soon), soon);
    }

    #[test]
    fn less_saturates_at_now_rather_than_panicking() {
        let deadline = Deadline::after(Duration::from_millis(1));
        let impossible = deadline.less(Duration::from_secs(86_400));
        assert!(impossible.is_expired());
    }

    #[test]
    fn ordering_follows_time() {
        let soon = Deadline::after(Duration::from_secs(1));
        let later = Deadline::after(Duration::from_secs(2));
        assert!(soon < later);
    }
}
