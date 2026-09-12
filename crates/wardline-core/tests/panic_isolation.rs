//! A panicking guard must not unwind past the pipeline, and its fail policy
//! is honored exactly as any other [`GuardError`] would be.

mod common;

use common::{Probe, allow, boom, log, ran};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use wardline_core::{Context, FailPolicy, Guard, GuardError, Pipeline, TraceOutcome, Verdict};

fn input() -> Arc<str> {
    Arc::from("a request payload")
}

/// Panics on the first `check`, then allows. Shared across two evaluations
/// of the same pipeline so a retry can succeed.
struct OnceThenAllow {
    calls: AtomicUsize,
}

impl Guard for OnceThenAllow {
    type Input = str;
    type Output = String;

    fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("first-call boom");
        }
        Ok(Verdict::Allow)
    }

    fn name(&self) -> &'static str {
        "flaky"
    }
}

#[test]
fn a_guard_that_panics_mid_check_does_not_unwind_past_evaluate() {
    let log = log();
    let pipeline = Pipeline::new().with(Probe::new("volatile", &log, boom));

    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pipeline.evaluate(&input(), &Context::new())
    }));
    let result = match caught {
        Ok(result) => result,
        Err(_) => panic!("Pipeline::evaluate must not unwind"),
    };

    assert!(result.is_block());
    assert_eq!(ran(&log), ["volatile"]);
    let Some(entry) = result.trace().last() else {
        panic!("the panicking guard should have been traced");
    };
    let TraceOutcome::Failed {
        error,
        policy,
        halted,
    } = entry.outcome()
    else {
        panic!("expected a panic failure, got {:?}", entry.outcome());
    };
    assert!(error.is_panic());
    assert_eq!(error.message(), Some("deliberate test panic"));
    assert_eq!(*policy, FailPolicy::FailClosed);
    assert!(*halted);
}

#[test]
fn a_guard_that_panics_on_the_first_call_can_succeed_on_retry() {
    let pipeline = Pipeline::new().with(OnceThenAllow {
        calls: AtomicUsize::new(0),
    });
    let ctx = Context::new();

    let first = pipeline.evaluate(&input(), &ctx);
    assert!(first.is_block());
    let Some(entry) = first.trace().last() else {
        panic!("first call should have been traced");
    };
    let TraceOutcome::Failed { error, .. } = entry.outcome() else {
        panic!("first call should be recorded as a panic");
    };
    assert_eq!(error.message(), Some("first-call boom"));

    let second = pipeline.evaluate(&input(), &ctx);
    assert!(second.is_allow());
    let Some(entry) = second.trace().last() else {
        panic!("retry should have been traced");
    };
    assert_eq!(entry.outcome().kind(), "allowed");
}

#[test]
fn a_panicking_guard_sandwiched_between_two_normal_ones_is_traced() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("before", &log, allow))
        .with(Probe::new("volatile", &log, boom).policy(FailPolicy::FailOpen))
        .with(Probe::new("after", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_allow());
    assert_eq!(ran(&log), ["before", "volatile", "after"]);
    assert_eq!(result.trace().len(), 3);

    let kinds: Vec<_> = result
        .trace()
        .iter()
        .map(|entry| {
            (
                entry.name(),
                entry.outcome().kind(),
                entry.outcome().halted(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("before", "allowed", false),
            ("volatile", "failed", false),
            ("after", "allowed", false),
        ]
    );

    let Some(middle) = result.trace().iter().nth(1) else {
        panic!("the sandwiched guard should have been traced");
    };
    let TraceOutcome::Failed { error, policy, .. } = middle.outcome() else {
        panic!("expected the middle guard to be a failure");
    };
    assert!(error.is_panic());
    assert_eq!(*policy, FailPolicy::FailOpen);
}

#[test]
fn a_fail_closed_panic_stops_later_guards() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("before", &log, allow))
        .with(Probe::new("volatile", &log, boom))
        .with(Probe::new("after", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_block());
    assert_eq!(ran(&log), ["before", "volatile"]);
    assert_eq!(result.trace().len(), 2);
    assert_eq!(result.trace().halted_by(), Some("volatile"));
}

#[test]
fn a_panic_inside_a_timed_guard_is_still_reported_as_panicked() {
    let log = log();
    let pipeline =
        Pipeline::new().with(Probe::new("timed_boom", &log, boom).timeout(Duration::from_secs(5)));

    let result = pipeline.evaluate(&input(), &Context::new());
    let Some(entry) = result.trace().last() else {
        panic!("the timed panicking guard should have been traced");
    };
    let TraceOutcome::Failed { error, .. } = entry.outcome() else {
        panic!("expected a panic failure, got {:?}", entry.outcome());
    };
    assert!(
        error.is_panic(),
        "a panic on the timeout thread must not degrade to Timeout or Internal"
    );
}

#[test]
fn a_guard_that_panics_on_every_call_does_not_abort_the_process() {
    let pipeline = Pipeline::new().with(Probe::new("always_boom", &log(), boom));
    for _ in 0..8 {
        let result = pipeline.evaluate(&input(), &Context::new());
        assert!(result.is_block());
        assert!(result.trace().last().is_some_and(|entry| matches!(
            entry.outcome(),
            TraceOutcome::Failed { error, .. } if error.is_panic()
        )));
    }
}
