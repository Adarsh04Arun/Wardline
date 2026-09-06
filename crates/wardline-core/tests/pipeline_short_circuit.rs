//! A block stops the pipeline, and the trace says where.

mod common;

use common::{Probe, allow, block, log, modify, ran};
use std::sync::Arc;
use wardline_core::{Context, Pipeline, TraceEntry, TraceOutcome};

fn input() -> Arc<str> {
    Arc::from("a request payload")
}

#[test]
fn a_block_from_the_second_of_three_guards_stops_the_third_running() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("first", &log, allow))
        .with(Probe::new("second", &log, block))
        .with(Probe::new("third", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_block());
    assert_eq!(result.block_reason(), Some("refused by policy"));
    assert_eq!(
        ran(&log),
        ["first", "second"],
        "the third guard must never be entered"
    );
}

#[test]
fn every_guard_runs_when_none_of_them_block() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("first", &log, allow))
        .with(Probe::new("second", &log, allow))
        .with(Probe::new("third", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert!(result.is_allow());
    assert_eq!(ran(&log), ["first", "second", "third"]);
}

#[test]
fn the_trace_is_ordered_and_complete_for_an_allowed_request() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("first", &log, allow))
        .with(Probe::new("second", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());
    let trace = result.trace();

    assert!(trace.is_complete());
    let names: Vec<_> = trace.iter().map(TraceEntry::name).collect();
    assert_eq!(names, ["first", "second"]);
    for entry in trace {
        assert_eq!(entry.outcome(), &TraceOutcome::Allowed);
    }
    assert_eq!(trace.halted_by(), None);
}

#[test]
fn the_trace_ends_at_the_blocking_guard() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("first", &log, allow))
        .with(Probe::new("second", &log, block))
        .with(Probe::new("third", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());
    let trace = result.trace();

    assert_eq!(trace.len(), 2);
    assert_eq!(trace.halted_by(), Some("second"));
    assert_eq!(
        trace.last().map(TraceEntry::outcome),
        Some(&TraceOutcome::Blocked {
            reason: "refused by policy".to_owned()
        })
    );
    assert!(
        trace.to_string().contains("refused by policy"),
        "the rendered trace has to say why"
    );
}

#[test]
fn a_modification_lets_the_remaining_guards_run() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("first", &log, modify))
        .with(Probe::new("second", &log, allow));

    let result = pipeline.evaluate(&input(), &Context::new());

    assert_eq!(ran(&log), ["first", "second"]);
    assert!(result.is_modify());
    assert_eq!(result.modified(), Some(&"[redacted]".to_owned()));
}

#[test]
fn the_same_pipeline_gives_the_same_answer_twice() {
    let log = log();
    let pipeline = Pipeline::new()
        .with(Probe::new("first", &log, allow))
        .with(Probe::new("second", &log, block));
    let ctx = Context::new();

    let first = pipeline.evaluate(&input(), &ctx);
    let second = pipeline.evaluate(&input(), &ctx);

    assert_eq!(first.block_reason(), second.block_reason());
    assert_eq!(first.trace().len(), second.trace().len());
    assert_eq!(ran(&log), ["first", "second", "first", "second"]);
}
