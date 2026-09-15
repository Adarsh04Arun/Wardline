# Architecture

Why the pipeline looks the way it does. Usage and the five-minute
quickstart live in the [README](../README.md). The reliability *contract*
lives in [`RELIABILITY.md`](RELIABILITY.md); this file is the design
rationale behind those promises, plus the measured cost of keeping them.

The timeout section below is the one people misread: a non-strict
timeout bounds the *caller's wait*. It does not cancel the guard.

## The shape

```text
evaluate(input, ctx)
  for guard in order:
      catch_unwind(guard.check)
          ├── Allow / Modify  → record, continue
          ├── Block           → record, stop
          └── Err / panic     → apply *that* guard's FailPolicy
  return (verdict, bounded Trace)
```

Guards run on the caller's thread. There is no queue and no worker pool.
A `Block` means the action never happens, not that it will be reversed
shortly afterwards.

## Fail policy is per-guard

A `GuardError` is "this check reached no decision." That is not the same
thing as `Verdict::Block`. Conflating them is how a broken classifier
becomes a hole.

The pipeline never applies a workspace-wide default. It reads
`guard.fail_policy()`:

- **FailClosed** (the trait default) — halt, with a block reason that
  names the guard and the error. A missing check must not look like
  permission.
- **FailOpen** — skip this guard and continue. Requires a doc comment on
  the impl saying why this check is safe to drop.
- **FailClosedWithFallback** — halt with a caller-supplied verdict
  registered on the `Pipeline`. No fallback registered means plain
  fail-closed. A missing fallback is never read as "continue."

`FailClosedWithFallback` lives on the pipeline, not the guard, because
the safe substitute is a property of the call site.

## Timeouts are two different things

`Guard::timeout()` is optional. `None` means the pipeline will wait as
long as the guard runs — fine for a regex over a string in memory,
dangerous for anything that talks to the network.

When a timeout is set, the request `Deadline` can only *tighten* it. A
deadline alone does not spawn a watchdog thread; that would cost a
thread per unbounded guard.

**Non-strict (the default).** The pipeline runs the check on a detached
thread and `recv_timeout`s. When the budget expires it reports
`GuardError::Timeout` and stops *waiting*. The guard keeps running until
it finishes. That is a wall-clock bound for the caller, not cancellation.
Abandoned work can still burn CPU and hold locks. This is a constraint of
blocking Rust threads without an async runtime, not a bug to paper over.

**Strict.** `strict() == true` means the guard promised to consult
`ctx.deadline()` and return on its own. The pipeline runs it inline and
never abandons it. If it overruns anyway, the result is
`GuardError::DeadlineViolated` — a contract violation, surfaced in
tests, not masked by a background thread. Wardline can detect a
non-cooperative strict guard. It cannot interrupt one.

## Panic isolation

Guard authors must return `Result` and must not panic. Independently,
every `check` is wrapped in `catch_unwind`. A panic becomes
`GuardError::Panicked` and is resolved through that guard's fail policy,
exactly like any other error. Both layers are required; neither is
optional because the other exists.

This is why `Guard` is `RefUnwindSafe` and why the crate's release
profile sets `panic = "unwind"`. `panic = "abort"` would make isolation
impossible.

A panic inside a non-strict timed guard is caught on the worker thread
and sent back as `Panicked`. It does not degrade to `Timeout` or
"thread disconnected."

## The trace is bounded

Every evaluation produces a `Trace`, including the allow path and
including panics. That is the audit log, not optional instrumentation.

The buffer has a hard cap (`Trace::DEFAULT_CAPACITY` is 64). Once full,
the oldest entry is dropped and `dropped()` counts it. A truncated trace
is always visibly truncated. Block reasons are clamped so one
pathological string cannot make the record unbounded.

`Modify` payloads are not stored in the trace. They can be arbitrarily
large, and the trace is deliberately not generic over `Out`.

## Observability

The `tracing` feature (off by default) emits a `wardline.evaluate` span
and a `wardline.guard` span per check, plus an error event when a guard
panics. Without that feature, `wardline-core` still depends on nothing
outside `std`.

`Metrics` / `Trace::emit_metrics` is a replay of the finished trace.
Wardline does not pick Prometheus or StatsD for you.

## Measured overhead (Phase 6)

Figures below come from `cargo bench -p wardline-core --bench pipeline`
on the machine that landed this section. They are a baseline, not a
promise. Re-run the bench after changing `pipeline.rs` or
`catch_unwind` and replace the numbers.

**Host:** Windows 10 (build 26200), `rustc` 1.94.1 (e408947bf), criterion
0.5.1, `--release`. No-op `Guard` that only returns `Verdict::Allow`.
Input is a short `&str`. Criterion middle estimate; the bracket is the
95% CI.

| Measurement | Typical | Per guard | Notes |
|---|---|---|---|
| `pipeline_evaluate/noop_guards/1` | 221 ns [208, 238] | 221 ns | one isolated no-op through `evaluate` |
| `pipeline_evaluate/noop_guards/4` | 1.35 µs [1.27, 1.44] | 338 ns | |
| `pipeline_evaluate/noop_guards/16` | 4.85 µs [4.56, 5.21] | 303 ns | |
| `pipeline_evaluate/noop_guards/64` | 4.05 µs [3.97, 4.14] | 63 ns | |
| `check_isolation/raw_check` | 1.5 ns [1.0, 2.0] | — | `Guard::check` only |
| `check_isolation/catch_unwind` | 7.8 ns [7.4, 8.3] | — | same check inside `catch_unwind` |
| panic-isolation overhead | **~6 ns** | — | `catch_unwind − raw_check` |

`evaluate` includes `catch_unwind`, the trace record, and fail-policy
resolution. The isolation pair is the incremental cost of not trusting
the guard: about six nanoseconds on this host for a no-op. A regex or a
network classifier will dominate that.

The no-op series sits on the noise floor. The 16-vs-64 crossing is not a
claim that longer pipelines get cheaper — treat the floor as "tens to
a few hundred nanoseconds per no-op guard," not as a scaling law.
Re-run `cargo bench -p wardline-core --bench pipeline` after changing
the executor.

## What this file is not

It is not a certification argument. See [`RELIABILITY.md`](RELIABILITY.md)
for what is guaranteed and what is explicitly not.
