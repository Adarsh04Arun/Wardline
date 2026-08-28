//! The ordered, bounded record of what every guard in a pipeline did.

use crate::{FailPolicy, GuardError, Verdict};
use core::fmt;
use std::collections::VecDeque;
use std::time::Duration;

/// The longest block reason a trace entry will store.
///
/// A guard may return a reason of any length; the trace may not, or one
/// pathological guard makes the audit record unbounded.
const MAX_REASON_BYTES: usize = 256;

/// Truncates on a UTF-8 boundary, marking that it happened.
fn clamp(reason: &str) -> String {
    if reason.len() <= MAX_REASON_BYTES {
        return reason.to_owned();
    }
    let mut end = MAX_REASON_BYTES;
    while end > 0 && !reason.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &reason[..end])
}

/// What one guard did, as recorded for the audit trail.
///
/// `#[non_exhaustive]`: match with a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TraceOutcome {
    /// The guard allowed the action unchanged.
    Allowed,
    /// The guard refused. The pipeline stopped here.
    Blocked {
        /// The guard's reason, truncated if it was unreasonably long.
        reason: String,
    },
    /// The guard returned a replacement payload.
    ///
    /// The payload itself is not stored: it is `Out`, which the trace is
    /// deliberately not generic over, and it can be arbitrarily large.
    Modified,
    /// The guard reached no decision, and the pipeline resolved that.
    Failed {
        /// What went wrong.
        error: GuardError,
        /// The failing guard's own policy — never a pipeline-wide default.
        policy: FailPolicy,
        /// Whether that policy stopped the pipeline. Usually implied by
        /// `policy`, but recorded because it can differ: a
        /// [`FailPolicy::FailClosedWithFallback`] guard in a pipeline with no
        /// fallback registered halts as plain fail-closed.
        halted: bool,
    },
}

impl TraceOutcome {
    /// The outcome corresponding to a verdict, whatever its payload type.
    pub fn from_verdict<O>(verdict: &Verdict<O>) -> Self {
        match verdict {
            Verdict::Allow => TraceOutcome::Allowed,
            Verdict::Block { reason } => TraceOutcome::Blocked {
                reason: clamp(reason),
            },
            Verdict::Modify(_) => TraceOutcome::Modified,
        }
    }

    /// A short, stable label for metrics and log fields, free of any payload.
    pub fn kind(&self) -> &'static str {
        match self {
            TraceOutcome::Allowed => "allowed",
            TraceOutcome::Blocked { .. } => "blocked",
            TraceOutcome::Modified => "modified",
            TraceOutcome::Failed { .. } => "failed",
        }
    }

    /// Returns `true` if the pipeline stopped at this entry.
    pub fn halted(&self) -> bool {
        match self {
            TraceOutcome::Blocked { .. } => true,
            TraceOutcome::Failed { halted, .. } => *halted,
            _ => false,
        }
    }
}

impl fmt::Display for TraceOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraceOutcome::Allowed => f.write_str("allowed"),
            TraceOutcome::Blocked { reason } => write!(f, "blocked: {reason}"),
            TraceOutcome::Modified => f.write_str("modified"),
            TraceOutcome::Failed { error, policy, .. } => write!(f, "failed ({policy}): {error}"),
        }
    }
}

/// One guard's line in the audit trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    name: &'static str,
    outcome: TraceOutcome,
    elapsed: Duration,
}

impl TraceEntry {
    /// Records what a guard did and how long it took.
    pub fn new(name: &'static str, outcome: TraceOutcome, elapsed: Duration) -> Self {
        TraceEntry {
            name,
            outcome,
            elapsed,
        }
    }

    /// The guard's [`Guard::name`].
    ///
    /// [`Guard::name`]: crate::Guard::name
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// What the guard did.
    pub fn outcome(&self) -> &TraceOutcome {
        &self.outcome
    }

    /// Wall-clock time spent on this guard.
    ///
    /// For a guard the pipeline gave up waiting on, this is how long the
    /// pipeline waited — not how long the guard ultimately ran, which it has
    /// no way to know.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

impl fmt::Display for TraceEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} ({:?})", self.name, self.outcome, self.elapsed)
    }
}

/// Every guard the pipeline ran, in order, with a hard cap on entries.
///
/// The cap is the point: a pipeline is caller-assembled and can be any
/// length, so an unbounded trace is a memory leak with a config file for a
/// trigger. Once full, the oldest entry is dropped and [`Trace::dropped`]
/// counts it, so a truncated trace is always visibly truncated.
///
/// The tail is kept rather than the head because the last entries are the
/// ones that decided the outcome.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use wardline_core::{Trace, TraceEntry, TraceOutcome};
///
/// let mut trace = Trace::with_capacity(2);
/// for name in ["first", "second", "third"] {
///     trace.record(TraceEntry::new(name, TraceOutcome::Allowed, Duration::ZERO));
/// }
///
/// let names: Vec<_> = trace.iter().map(TraceEntry::name).collect();
/// assert_eq!(names, ["second", "third"]);
/// assert_eq!(trace.dropped(), 1);
/// assert!(!trace.is_complete());
/// ```
#[derive(Debug, Clone)]
pub struct Trace {
    entries: VecDeque<TraceEntry>,
    capacity: usize,
    dropped: usize,
}

impl Trace {
    /// Entries kept when no capacity is chosen.
    ///
    /// Comfortably more than any hand-assembled pipeline, small enough that
    /// one per in-flight request is not worth thinking about.
    pub const DEFAULT_CAPACITY: usize = 64;

    /// Creates an empty trace holding [`Trace::DEFAULT_CAPACITY`] entries.
    pub fn new() -> Self {
        Trace::with_capacity(Trace::DEFAULT_CAPACITY)
    }

    /// Creates an empty trace holding at most `capacity` entries.
    ///
    /// A capacity of zero is legal, and records nothing but the count of what
    /// it discarded.
    pub fn with_capacity(capacity: usize) -> Self {
        Trace {
            entries: VecDeque::with_capacity(capacity.min(Trace::DEFAULT_CAPACITY)),
            capacity,
            dropped: 0,
        }
    }

    /// Appends an entry, evicting the oldest if the trace is full.
    pub fn record(&mut self, entry: TraceEntry) {
        if self.capacity == 0 {
            self.dropped += 1;
            return;
        }
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
            self.dropped += 1;
        }
        self.entries.push_back(entry);
    }

    /// The retained entries, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &TraceEntry> {
        self.entries.iter()
    }

    /// The number of retained entries — never more than the capacity.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if nothing is retained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry cap this trace was built with.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// How many entries were evicted to stay inside the cap.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// Returns `true` if every guard that ran is still recorded.
    ///
    /// Worth asserting before treating a trace as a complete audit record.
    pub fn is_complete(&self) -> bool {
        self.dropped == 0
    }

    /// The last guard to run — for a halted pipeline, the one that decided
    /// the outcome.
    pub fn last(&self) -> Option<&TraceEntry> {
        self.entries.back()
    }

    /// The name of the guard that stopped the pipeline, if one did.
    pub fn halted_by(&self) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|entry| entry.outcome().halted())
            .map(TraceEntry::name)
    }
}

impl Default for Trace {
    fn default() -> Self {
        Trace::new()
    }
}

impl<'a> IntoIterator for &'a Trace {
    type Item = &'a TraceEntry;
    type IntoIter = std::collections::vec_deque::Iter<'a, TraceEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl fmt::Display for Trace {
    /// One guard per line, oldest first, prefixed by a note when the trace
    /// was truncated — a log line that silently omits guards is worse than no
    /// log line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.dropped > 0 {
            writeln!(f, "[{} earlier entries dropped]", self.dropped)?;
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{entry}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &'static str) -> TraceEntry {
        TraceEntry::new(name, TraceOutcome::Allowed, Duration::from_millis(1))
    }

    #[test]
    fn entries_are_kept_in_order() {
        let mut trace = Trace::new();
        for name in ["a", "b", "c"] {
            trace.record(entry(name));
        }
        let names: Vec<_> = trace.iter().map(TraceEntry::name).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(trace.len(), 3);
        assert!(trace.is_complete());
    }

    #[test]
    fn memory_stays_flat_however_many_guards_run() {
        let mut trace = Trace::with_capacity(8);
        for _ in 0..100_000 {
            trace.record(entry("noisy"));
        }
        assert_eq!(trace.len(), 8);
        assert_eq!(trace.capacity(), 8);
        assert_eq!(trace.dropped(), 100_000 - 8);
        assert!(!trace.is_complete());
    }

    #[test]
    fn a_full_trace_keeps_the_deciding_tail() {
        let mut trace = Trace::with_capacity(2);
        trace.record(entry("first"));
        trace.record(entry("second"));
        trace.record(TraceEntry::new(
            "third",
            TraceOutcome::Blocked {
                reason: "no".to_owned(),
            },
            Duration::ZERO,
        ));
        assert_eq!(trace.last().map(TraceEntry::name), Some("third"));
        assert_eq!(trace.halted_by(), Some("third"));
    }

    #[test]
    fn zero_capacity_records_nothing_but_still_counts() {
        let mut trace = Trace::with_capacity(0);
        trace.record(entry("a"));
        trace.record(entry("b"));
        assert!(trace.is_empty());
        assert_eq!(trace.dropped(), 2);
    }

    #[test]
    fn a_pathological_block_reason_is_clamped_on_a_char_boundary() {
        let long = "é".repeat(1_000);
        let outcome = TraceOutcome::from_verdict(&Verdict::<()>::block(long));
        let TraceOutcome::Blocked { reason } = outcome else {
            panic!("expected a blocked outcome");
        };
        assert!(reason.len() <= MAX_REASON_BYTES + '…'.len_utf8());
        assert!(reason.ends_with('…'));
    }

    #[test]
    fn a_short_reason_is_kept_verbatim() {
        let outcome = TraceOutcome::from_verdict(&Verdict::<()>::block("banned phrase"));
        assert_eq!(
            outcome,
            TraceOutcome::Blocked {
                reason: "banned phrase".to_owned()
            }
        );
    }

    #[test]
    fn outcome_kinds_are_unique_and_payload_free() {
        let outcomes = [
            TraceOutcome::Allowed,
            TraceOutcome::Blocked {
                reason: "spaces and such".to_owned(),
            },
            TraceOutcome::Modified,
            TraceOutcome::Failed {
                error: GuardError::Timeout,
                policy: FailPolicy::FailClosed,
                halted: true,
            },
        ];
        let mut kinds: Vec<_> = outcomes.iter().map(TraceOutcome::kind).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), outcomes.len());
        for outcome in &outcomes {
            assert!(!outcome.kind().contains(' '));
        }
    }

    #[test]
    fn only_blocks_and_halting_failures_stop_the_pipeline() {
        assert!(!TraceOutcome::Allowed.halted());
        assert!(!TraceOutcome::Modified.halted());
        assert!(
            TraceOutcome::Blocked {
                reason: "no".to_owned()
            }
            .halted()
        );
        assert!(
            !TraceOutcome::Failed {
                error: GuardError::Timeout,
                policy: FailPolicy::FailOpen,
                halted: false,
            }
            .halted()
        );
    }

    #[test]
    fn display_announces_truncation() {
        let mut trace = Trace::with_capacity(1);
        trace.record(entry("a"));
        trace.record(entry("b"));
        let rendered = trace.to_string();
        assert!(rendered.starts_with("[1 earlier entries dropped]"));
        assert!(rendered.contains("b allowed"));
        assert!(!rendered.contains("a allowed"));
    }
}
