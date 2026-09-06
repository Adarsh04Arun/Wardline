//! A slow guard costs the caller its timeout, not its real runtime.

mod common;

use common::{Probe, allow, log, ran};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wardline_core::{Context, Deadline, FailPolicy, GuardError, Pipeline, TraceOutcome};

fn input() -> Arc<str> {
    Arc::from("a request payload")
}

/// Long enough that a machine under load cannot reach it by accident.
const FOREVER: Duration = Duration::from_secs(30);

fn error_of(outcome: &TraceOutcome) -> &GuardError {
    let TraceOutcome::Failed { error, .. } = outcome else {
        panic!("expected a failure, got {outcome:?}");
    };
    error
}

#[test]
fn a_guard_that_overruns_its_timeout_produces_a_timeout_error() {
    let log = log();
    let pipeline = Pipeline::new().with(
        Probe::new("slow", &log, allow)
            .sleeping(FOREVER)
            .timeout(Duration::from_millis(50)),
    );

    let result = pipeline.evaluate(&input(), &Context::new());
    let Some(entry) = result.trace().last() else {
        panic!("the slow guard should have been traced");
    };

    assert_eq!(error_of(entry.outcome()), &GuardError::Timeout);
    assert!(
        result.is_block(),
        "a timeout is a failure, and fails closed"
    );
}

#[test]
fn the_pipeline_is_bounded_by_the_timeout_not_by_the_guard() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(
            Probe::new("slow", &log, allow)
                .sleeping(FOREVER)
                .timeout(Duration::from_millis(50))
                .policy(FailPolicy::FailOpen),
        )
        .with(Probe::new("after", &log, allow));

    let started = Instant::now();
    let result = pipeline.evaluate(&input(), &Context::new());
    let elapsed = started.elapsed();

    assert!(
        elapsed < FOREVER / 4,
        "the pipeline waited {elapsed:?}; it must give up at the timeout"
    );
    assert!(result.is_allow());
    assert_eq!(ran(&log), ["slow", "after"]);
}

#[test]
fn several_slow_guards_cost_their_own_timeouts_and_no_more() {
    let log = log();
    let budget = Duration::from_millis(40);
    let pipeline = Pipeline::new()
        .with(
            Probe::new("first", &log, allow)
                .sleeping(FOREVER)
                .timeout(budget)
                .policy(FailPolicy::FailOpen),
        )
        .with(
            Probe::new("second", &log, allow)
                .sleeping(FOREVER)
                .timeout(budget)
                .policy(FailPolicy::FailOpen),
        );

    let started = Instant::now();
    let result = pipeline.evaluate(&input(), &Context::new());
    let elapsed = started.elapsed();

    assert!(result.is_allow());
    assert!(
        elapsed < FOREVER / 4,
        "two abandoned guards took {elapsed:?}"
    );
    assert_eq!(result.trace().len(), 2);
}

#[test]
fn a_guard_that_finishes_in_time_is_untouched() {
    let log = log();
    let pipeline =
        Pipeline::new().with(Probe::new("prompt", &log, allow).timeout(Duration::from_secs(5)));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_allow());
    assert_eq!(ran(&log), ["prompt"]);
    assert_eq!(
        result.trace().last().map(|entry| entry.outcome().kind()),
        Some("allowed")
    );
}

#[test]
fn a_strict_guard_that_overruns_is_a_contract_violation_not_a_timeout() {
    // A strict guard promised to watch the clock itself, so the pipeline runs
    // it inline and never abandons it. Overrunning is reported as the
    // distinct, louder error.
    let log = log();
    let pipeline = Pipeline::new().with(
        Probe::new("stubborn", &log, allow)
            .sleeping(Duration::from_millis(80))
            .timeout(Duration::from_millis(10))
            .strict(),
    );

    let result = pipeline.evaluate(&input(), &Context::new());
    let Some(entry) = result.trace().last() else {
        panic!("the strict guard should have been traced");
    };

    assert_eq!(error_of(entry.outcome()), &GuardError::DeadlineViolated);
    assert!(entry.elapsed() >= Duration::from_millis(80));
}

#[test]
fn a_strict_guard_inside_its_budget_is_left_alone() {
    let log = log();
    let pipeline =
        Pipeline::new().with(Probe::new("polite", &log, allow).timeout(FOREVER).strict());

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_allow());
    assert_eq!(ran(&log), ["polite"]);
}

#[test]
fn an_expired_request_deadline_skips_a_bounded_guard_entirely() {
    let log = log();
    let pipeline =
        Pipeline::new().with(Probe::new("bounded", &log, allow).timeout(Duration::from_secs(5)));
    let expired = Context::new().with_deadline(Deadline::after(Duration::ZERO));

    let result = pipeline.evaluate(&input(), &expired);

    assert!(result.is_block());
    assert!(
        ran(&log).is_empty(),
        "there is no point starting work nobody is waiting for"
    );
}

#[test]
fn the_request_deadline_tightens_a_longer_guard_timeout() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(
            Probe::new("slow", &log, allow)
                .sleeping(FOREVER)
                .timeout(Duration::from_secs(20))
                .policy(FailPolicy::FailOpen),
        )
        .with(Probe::new("after", &log, allow));
    let ctx = Context::new().with_deadline(Deadline::after(Duration::from_millis(60)));

    let started = Instant::now();
    let result = pipeline.evaluate(&input(), &ctx);
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "the deadline should have cut the guard's own timeout short, but the \
         pipeline waited {elapsed:?}"
    );
    assert!(result.is_allow());
    assert_eq!(ran(&log), ["slow", "after"]);
}

#[test]
fn an_untimed_guard_is_never_moved_off_the_calling_thread() {
    // No declared timeout means no thread and no abandonment: the caller gets
    // exactly the synchronous, in-line evaluation the library promises.
    let log = log();
    // The test harness names the thread it runs each test on; the threads the
    // pipeline spawns for bounded guards are unnamed.
    let pipeline = Pipeline::new().with(Probe::new("inline", &log, || {
        assert!(
            std::thread::current().name().is_some(),
            "an untimed guard must run on the caller's thread"
        );
        allow()
    }));

    let result = pipeline.evaluate(&input(), &Context::new());
    assert!(result.is_allow());
}
