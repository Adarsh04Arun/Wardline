//! A guard that fails is resolved by its own policy, never a shared default.

mod common;

use common::{Probe, allow, fail, log, ran};
use std::sync::Arc;
use wardline_core::{Context, FailPolicy, Pipeline, TraceEntry, TraceOutcome, Verdict};

fn input() -> Arc<str> {
    Arc::from("a request payload")
}

#[test]
fn a_fail_open_guard_that_errors_lets_the_pipeline_continue() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("advisory", &log, fail).policy(FailPolicy::FailOpen))
        .with(Probe::new("after", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_allow());
    assert_eq!(ran(&log), ["advisory", "after"]);
    assert_eq!(result.trace().len(), 2);
}

#[test]
fn a_fail_closed_guard_that_errors_stops_the_pipeline() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("critical", &log, fail))
        .with(Probe::new("after", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_block());
    assert_eq!(ran(&log), ["critical"], "the later guard must not run");
    assert_eq!(result.trace().halted_by(), Some("critical"));
}

#[test]
fn fail_closed_is_what_a_guard_gets_without_asking() {
    // The default is the whole reliability posture, so assert it from
    // outside the crate rather than trusting the trait definition.
    let log = log();
    let pipeline = Pipeline::new().with(Probe::new("silent", &log, fail));

    let result = pipeline.evaluate(&input(), &Context::new());
    let Some(entry) = result.trace().last() else {
        panic!("the failing guard should have been traced");
    };
    let TraceOutcome::Failed { policy, .. } = entry.outcome() else {
        panic!("expected a failure, got {:?}", entry.outcome());
    };
    assert_eq!(*policy, FailPolicy::FailClosed);
    assert!(result.is_block());
}

#[test]
fn each_guard_answers_for_itself_in_a_mixed_pipeline() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("advisory", &log, fail).policy(FailPolicy::FailOpen))
        .with(Probe::new("also_advisory", &log, fail).policy(FailPolicy::FailOpen))
        .with(Probe::new("critical", &log, fail))
        .with(Probe::new("never", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert_eq!(ran(&log), ["advisory", "also_advisory", "critical"]);
    assert!(result.is_block());
    assert_eq!(result.trace().halted_by(), Some("critical"));

    let halted: Vec<_> = result
        .trace()
        .iter()
        .map(|entry| (entry.name(), entry.outcome().halted()))
        .collect();
    assert_eq!(
        halted,
        [
            ("advisory", false),
            ("also_advisory", false),
            ("critical", true)
        ]
    );
}

#[test]
fn a_failure_never_reads_as_a_refusal_in_the_trace() {
    let log = log();
    let pipeline = Pipeline::new().with(Probe::new("critical", &log, fail));

    let result = pipeline.evaluate(&input(), &Context::new());
    let Some(entry) = result.trace().last() else {
        panic!("the failing guard should have been traced");
    };

    assert_eq!(entry.outcome().kind(), "failed");
    let TraceOutcome::Failed { error, .. } = entry.outcome() else {
        panic!("expected a failure, got {:?}", entry.outcome());
    };
    assert_eq!(error.kind(), "dependency");
    // The caller still gets a block, but one that says what really happened.
    assert!(
        result
            .block_reason()
            .is_some_and(|reason| reason.contains("critical") && reason.contains("unreachable"))
    );
}

#[test]
fn a_fallback_verdict_stands_in_for_the_decision_that_was_never_reached() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("classifier", &log, fail).policy(FailPolicy::FailClosedWithFallback))
        .with(Probe::new("after", &log, allow))
        .with_fallback(|| Verdict::Modify("[unverified]".to_owned()));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert_eq!(result.modified(), Some(&"[unverified]".to_owned()));
    assert_eq!(ran(&log), ["classifier"], "the fallback still halts");
}

#[test]
fn a_fallback_policy_without_a_fallback_falls_closed_not_open() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("classifier", &log, fail).policy(FailPolicy::FailClosedWithFallback))
        .with(Probe::new("after", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(
        result.is_block(),
        "a missing fallback must never be read as permission to continue"
    );
    assert_eq!(ran(&log), ["classifier"]);
    assert_eq!(result.trace().halted_by(), Some("classifier"));
}

#[test]
fn a_failing_guard_is_still_traced_when_it_fails_open() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("advisory", &log, fail).policy(FailPolicy::FailOpen))
        .with(Probe::new("after", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());
    let kinds: Vec<_> = result
        .trace()
        .iter()
        .map(|entry| (entry.name(), entry.outcome().kind()))
        .collect();

    assert_eq!(kinds, [("advisory", "failed"), ("after", "allowed")]);
    assert_eq!(
        result.trace().iter().map(TraceEntry::name).count(),
        2,
        "failing open must not mean failing silently"
    );
}
