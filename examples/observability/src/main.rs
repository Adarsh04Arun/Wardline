//! Structured spans and per-guard counters, including a caught panic.

use std::sync::Arc;
use tracing_subscriber::fmt;
use wardline_core::{Context, FailPolicy, Guard, GuardError, InMemoryMetrics, Pipeline, Verdict};

struct AlwaysAllow;

impl Guard for AlwaysAllow {
    type Input = str;
    type Output = ();

    fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
        Ok(Verdict::Allow)
    }

    fn name(&self) -> &'static str {
        "always_allow"
    }
}

struct Explodes;

impl Guard for Explodes {
    type Input = str;
    type Output = ();

    fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
        panic!("demo panic from Phase 4.5");
    }

    fn fail_policy(&self) -> FailPolicy {
        // Documented: this demo wants later guards to still run so the
        // panic event and the following block both appear in the trace.
        FailPolicy::FailOpen
    }

    fn name(&self) -> &'static str {
        "explodes"
    }
}

struct AlwaysBlock;

impl Guard for AlwaysBlock {
    type Input = str;
    type Output = ();

    fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
        Ok(Verdict::block("demo block"))
    }

    fn name(&self) -> &'static str {
        "always_block"
    }
}

fn pipeline() -> Pipeline<str> {
    Pipeline::new()
        .with(AlwaysAllow)
        .with(Explodes)
        .with(AlwaysBlock)
}

fn main() {
    // The pipeline already converts the panic into `GuardError::Panicked`
    // and a `guard panicked` event. The default hook would reprint the
    // same panic as if the process were dying.
    std::panic::set_hook(Box::new(|_| {}));

    fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();

    let pipeline = pipeline();
    let metrics = InMemoryMetrics::new();
    let result = pipeline.evaluate(&Arc::from("hello"), &Context::new());
    result.emit_metrics(&metrics);

    println!("verdict: {:?}", result.verdict());
    println!("trace:\n{}", result.trace());
    println!("metrics:");
    for (name, counters) in metrics.snapshot() {
        println!(
            "  {name}: allow={} block={} modify={} error={} panic={}",
            counters.allow, counters.block, counters.modify, counters.error, counters.panic
        );
    }
    println!("The panic event is `guard panicked` from `explodes` (FailOpen).");
    println!("The block is from `always_block`; the process did not abort.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_records_allow_panic_and_block() {
        let pipeline = pipeline();
        let metrics = InMemoryMetrics::new();
        let result = pipeline.evaluate(&Arc::from("hello"), &Context::new());
        result.emit_metrics(&metrics);

        assert!(result.is_block());
        assert_eq!(result.block_reason(), Some("demo block"));
        assert_eq!(result.trace().len(), 3);
        assert_eq!(metrics.counters("always_allow").allow, 1);
        assert_eq!(metrics.counters("explodes").panic, 1);
        assert_eq!(metrics.counters("always_block").block, 1);
    }
}
