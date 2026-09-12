//! Pipeline overhead per guard, and the cost of `catch_unwind` isolation.
//!
//! Numbers are measured, not promised. Record a fresh run in
//! `docs/ARCHITECTURE.md` when the executor changes.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::Duration;
use wardline_core::{Context, Guard, GuardError, Pipeline, Verdict};

struct Noop;

impl Guard for Noop {
    type Input = str;
    type Output = ();

    fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
        Ok(Verdict::Allow)
    }

    fn name(&self) -> &'static str {
        "noop"
    }
}

fn pipeline_of(n: usize) -> Pipeline<str> {
    let mut pipeline = Pipeline::new();
    for _ in 0..n {
        pipeline.push(Noop);
    }
    pipeline
}

fn pipeline_per_guard(c: &mut Criterion) {
    let input = Arc::<str>::from("hello from the bench");
    let ctx = Context::new();
    let mut group = c.benchmark_group("pipeline_evaluate");
    group.measurement_time(Duration::from_secs(8));
    group.warm_up_time(Duration::from_secs(2));

    for n in [1usize, 4, 16, 64] {
        let pipeline = pipeline_of(n);
        group.bench_with_input(BenchmarkId::new("noop_guards", n), &n, |b, _| {
            b.iter(|| pipeline.evaluate(black_box(&input), black_box(&ctx)));
        });
    }
    group.finish();
}

fn catch_unwind_overhead(c: &mut Criterion) {
    let guard = Noop;
    let ctx = Context::new();
    let mut group = c.benchmark_group("check_isolation");
    group.measurement_time(Duration::from_secs(8));
    group.warm_up_time(Duration::from_secs(2));

    group.bench_function("raw_check", |b| {
        b.iter(|| guard.check(black_box("hello"), black_box(&ctx)));
    });
    group.bench_function("catch_unwind", |b| {
        b.iter(|| {
            catch_unwind(AssertUnwindSafe(|| {
                guard.check(black_box("hello"), black_box(&ctx))
            }))
        });
    });
    group.finish();
}

criterion_group!(benches, pipeline_per_guard, catch_unwind_overhead);
criterion_main!(benches);
