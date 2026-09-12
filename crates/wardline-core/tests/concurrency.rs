//! A shared [`Pipeline`] is safe for concurrent `evaluate` calls.
//!
//! Guards are `Send + Sync` by construction. This file proves the executor
//! does not introduce per-request shared mutable state that would race.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use wardline_core::{Context, FailPolicy, Guard, GuardError, Pipeline, Verdict};

/// Counts how many times `check` ran, so we can tell every thread's work
/// actually entered the guard.
struct Counting {
    name: &'static str,
    hits: Arc<AtomicUsize>,
    block_on: Option<&'static str>,
}

impl Guard for Counting {
    type Input = str;
    type Output = ();

    fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        if let Some(needle) = self.block_on {
            if input.contains(needle) {
                return Ok(Verdict::block("refused"));
            }
        }
        Ok(Verdict::Allow)
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// Panics on a marker string. Fail-open so later guards still run.
struct BoomOn;

impl Guard for BoomOn {
    type Input = str;
    type Output = ();

    fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
        if input.contains("boom") {
            panic!("concurrent boom");
        }
        Ok(Verdict::Allow)
    }

    fn fail_policy(&self) -> FailPolicy {
        FailPolicy::FailOpen
    }

    fn name(&self) -> &'static str {
        "boom_on"
    }
}

fn pipeline(hits: &Arc<AtomicUsize>) -> Pipeline<str> {
    Pipeline::new()
        .with(Counting {
            name: "first",
            hits: Arc::clone(hits),
            block_on: None,
        })
        .with(BoomOn)
        .with(Counting {
            name: "last",
            hits: Arc::clone(hits),
            block_on: Some("block"),
        })
}

#[test]
fn many_threads_share_one_pipeline_and_agree_on_the_verdict() {
    // The pipeline catches these; the default hook would reprint each one.
    std::panic::set_hook(Box::new(|_| {}));

    let hits = Arc::new(AtomicUsize::new(0));
    let pipeline = Arc::new(pipeline(&hits));
    let threads = 8;
    let per_thread = 64;

    let mut handles = Vec::new();
    for id in 0..threads {
        let pipeline = Arc::clone(&pipeline);
        handles.push(thread::spawn(move || {
            let ctx = Context::new();
            let mut allowed = 0usize;
            let mut blocked = 0usize;
            let mut panicked = 0usize;
            for i in 0..per_thread {
                let body = match i % 3 {
                    0 => format!("ok-{id}-{i}"),
                    1 => format!("please block this {id}-{i}"),
                    _ => format!("boom then continue {id}-{i}"),
                };
                let input = Arc::<str>::from(body);
                let result = pipeline.evaluate(&input, &ctx);
                if result.is_allow() {
                    allowed += 1;
                } else if result.is_block() {
                    blocked += 1;
                }
                if result.trace().iter().any(|entry| {
                    matches!(
                        entry.outcome(),
                        wardline_core::TraceOutcome::Failed { error, .. } if error.is_panic()
                    )
                }) {
                    panicked += 1;
                }
            }
            (allowed, blocked, panicked)
        }));
    }

    let mut allowed = 0;
    let mut blocked = 0;
    let mut panicked = 0;
    for handle in handles {
        match handle.join() {
            Ok((a, b, p)) => {
                allowed += a;
                blocked += b;
                panicked += p;
            }
            Err(_) => panic!("a worker thread unwound — the pipeline leaked a panic"),
        }
    }

    let total = threads * per_thread;
    let expected_blocked = threads * (0..per_thread).filter(|i| i % 3 == 1).count();
    let expected_panicked = threads * (0..per_thread).filter(|i| i % 3 == 2).count();
    assert_eq!(allowed + blocked, total);
    assert_eq!(blocked, expected_blocked);
    assert_eq!(allowed, total - blocked);
    // `i % 3 == 2` contains "boom"; fail-open so those still allow.
    assert_eq!(panicked, expected_panicked);
    // first always runs; last always runs (boom is fail-open).
    assert_eq!(hits.load(Ordering::SeqCst), total * 2);
}

#[test]
fn concurrent_evaluations_do_not_corrupt_the_trace() {
    let hits = Arc::new(AtomicUsize::new(0));
    let pipeline = Arc::new(pipeline(&hits));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let pipeline = Arc::clone(&pipeline);
        handles.push(thread::spawn(move || {
            let result = pipeline.evaluate(&Arc::from("clean"), &Context::new());
            assert!(result.is_allow());
            assert_eq!(result.trace().len(), 3);
            assert!(result.trace().is_complete());
            result.trace().len()
        }));
    }
    for handle in handles {
        match handle.join() {
            Ok(len) => assert_eq!(len, 3),
            Err(_) => panic!("a worker thread unwound"),
        }
    }
}
