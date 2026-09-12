//! Per-guard counters a caller can feed into Prometheus, StatsD, or similar.

use crate::{PipelineResult, Trace, TraceOutcome};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Counts of allow / block / modify / error / panic, keyed by guard name.
///
/// Implement this against whatever you already emit metrics to. Wardline
/// does not pull in a metrics backend; it only defines the events.
///
/// Replay a finished evaluation with [`Trace::emit_metrics`] (or
/// [`PipelineResult::emit_metrics`]) rather than hooking the pipeline
/// itself — the trace is the source of truth, including panic outcomes.
pub trait Metrics: Send + Sync {
    /// A guard returned [`Verdict::Allow`].
    ///
    /// [`Verdict::Allow`]: crate::Verdict::Allow
    fn record_allow(&self, guard: &'static str);

    /// A guard returned [`Verdict::Block`].
    ///
    /// [`Verdict::Block`]: crate::Verdict::Block
    fn record_block(&self, guard: &'static str);

    /// A guard returned [`Verdict::Modify`].
    ///
    /// [`Verdict::Modify`]: crate::Verdict::Modify
    fn record_modify(&self, guard: &'static str);

    /// A guard returned a non-panic [`GuardError`].
    ///
    /// `kind` is [`GuardError::kind`] — a bounded-cardinality label
    /// (`timeout`, `dependency`, …), never the free-text message.
    ///
    /// [`GuardError`]: crate::GuardError
    /// [`GuardError::kind`]: crate::GuardError::kind
    fn record_error(&self, guard: &'static str, kind: &'static str);

    /// The pipeline caught a panic from this guard.
    fn record_panic(&self, guard: &'static str);
}

/// Snapshot of one guard's counters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GuardCounters {
    /// [`Verdict::Allow`] decisions.
    ///
    /// [`Verdict::Allow`]: crate::Verdict::Allow
    pub allow: u64,
    /// [`Verdict::Block`] decisions.
    ///
    /// [`Verdict::Block`]: crate::Verdict::Block
    pub block: u64,
    /// [`Verdict::Modify`] decisions.
    ///
    /// [`Verdict::Modify`]: crate::Verdict::Modify
    pub modify: u64,
    /// Non-panic [`GuardError`]s.
    ///
    /// [`GuardError`]: crate::GuardError
    pub error: u64,
    /// Caught panics.
    pub panic: u64,
}

/// A process-local [`Metrics`] sink, useful in tests and examples.
///
/// Not a production backend — there is no export format and no reset
/// across process lifetime unless you drop it.
#[derive(Debug, Default)]
pub struct InMemoryMetrics {
    inner: Mutex<BTreeMap<&'static str, GuardCounters>>,
}

impl InMemoryMetrics {
    /// Creates an empty sink.
    pub fn new() -> Self {
        InMemoryMetrics::default()
    }

    /// A copy of every guard that has been recorded so far.
    ///
    /// A poisoned lock reads as empty, which fails the assertions that
    /// matter rather than panicking inside them.
    pub fn snapshot(&self) -> BTreeMap<&'static str, GuardCounters> {
        self.inner.lock().map(|map| map.clone()).unwrap_or_default()
    }

    /// Counters for one guard, or zeros if it has never been recorded.
    pub fn counters(&self, guard: &'static str) -> GuardCounters {
        self.snapshot().get(guard).copied().unwrap_or_default()
    }
}

impl InMemoryMetrics {
    fn bump(&self, guard: &'static str, update: impl FnOnce(&mut GuardCounters)) {
        if let Ok(mut map) = self.inner.lock() {
            update(map.entry(guard).or_default());
        }
    }
}

impl Metrics for InMemoryMetrics {
    fn record_allow(&self, guard: &'static str) {
        self.bump(guard, |c| c.allow += 1);
    }

    fn record_block(&self, guard: &'static str) {
        self.bump(guard, |c| c.block += 1);
    }

    fn record_modify(&self, guard: &'static str) {
        self.bump(guard, |c| c.modify += 1);
    }

    fn record_error(&self, guard: &'static str, _kind: &'static str) {
        self.bump(guard, |c| c.error += 1);
    }

    fn record_panic(&self, guard: &'static str) {
        self.bump(guard, |c| c.panic += 1);
    }
}

impl Trace {
    /// Increments one counter per retained entry on `metrics`.
    ///
    /// Dropped entries are not replayed — raise the trace capacity if you
    /// need every guard counted.
    pub fn emit_metrics(&self, metrics: &dyn Metrics) {
        for entry in self.iter() {
            match entry.outcome() {
                TraceOutcome::Allowed => metrics.record_allow(entry.name()),
                TraceOutcome::Blocked { .. } => metrics.record_block(entry.name()),
                TraceOutcome::Modified => metrics.record_modify(entry.name()),
                TraceOutcome::Failed { error, .. } if error.is_panic() => {
                    metrics.record_panic(entry.name());
                }
                TraceOutcome::Failed { error, .. } => {
                    metrics.record_error(entry.name(), error.kind());
                }
            }
        }
    }
}

impl<Out> PipelineResult<Out> {
    /// Forwards the trace to `metrics`. See [`Trace::emit_metrics`].
    pub fn emit_metrics(&self, metrics: &dyn Metrics) {
        self.trace().emit_metrics(metrics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FailPolicy, GuardError, TraceEntry};
    use std::time::Duration;

    fn entry(name: &'static str, outcome: TraceOutcome) -> TraceEntry {
        TraceEntry::new(name, outcome, Duration::ZERO)
    }

    #[test]
    fn a_trace_increments_the_matching_counter_for_each_outcome() {
        let mut trace = Trace::new();
        trace.record(entry("a", TraceOutcome::Allowed));
        trace.record(entry(
            "b",
            TraceOutcome::Blocked {
                reason: "no".to_owned(),
            },
        ));
        trace.record(entry("c", TraceOutcome::Modified));
        trace.record(entry(
            "d",
            TraceOutcome::Failed {
                error: GuardError::Timeout,
                policy: FailPolicy::FailClosed,
                halted: true,
            },
        ));
        trace.record(entry(
            "e",
            TraceOutcome::Failed {
                error: GuardError::Panicked("boom".to_owned()),
                policy: FailPolicy::FailOpen,
                halted: false,
            },
        ));

        let metrics = InMemoryMetrics::new();
        trace.emit_metrics(&metrics);

        assert_eq!(metrics.counters("a").allow, 1);
        assert_eq!(metrics.counters("b").block, 1);
        assert_eq!(metrics.counters("c").modify, 1);
        assert_eq!(metrics.counters("d").error, 1);
        assert_eq!(metrics.counters("e").panic, 1);
        assert_eq!(metrics.counters("e").error, 0, "a panic is not an error");
    }
}
