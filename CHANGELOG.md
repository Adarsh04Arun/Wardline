# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Reliability-relevant changes always get their own heading in a release entry —
never folded into a general bullet list — so anyone pinning Wardline in a
critical path can find them without reading every line.

## [Unreleased]

### Added

- `wardline-http`: `tower::Layer` that buffers a request body and runs a
  Wardline pipeline synchronously inside axum. This crate is the one
  deliberate async boundary; evaluation itself is still blocking
  (implementation plan Phase 4).
- `wardline-llm`: `guarded_prompt` — input pipeline, blocking model call,
  output pipeline. An input block never calls the model; an output block
  never returns the reply.
- Examples: `sync_http_server` (tiny_http, zero async), `axum_middleware`,
  and `llm_chat_guard`. `cargo run -p <example>` prints an allow and a
  block without binding a port.

- `wardline-guards`: reference implementations — `RegexBlockGuard`,
  `RegexRedactGuard`, an in-process `RateLimitGuard`, a baseline `PiiGuard`
  (not compliance-grade), a heuristic `PromptInjectionGuard`, and
  `ClassifierAdapter` so an external model can be plugged in as a `Guard`
  without this crate hosting one (implementation plan Phase 3).

- `wardline-core`: `Pipeline`, the synchronous executor. It runs guards in the
  order they were added, stops at the first `Verdict::Block`, and returns a
  `PipelineResult` carrying the decision alongside the `Trace` of every guard
  that ran — so a caller can log why a request was refused, not just that it
  was (implementation plan Phase 2).
- `wardline-core`: `Trace`, `TraceEntry`, and `TraceOutcome` — the per-guard
  audit record, with a hard cap on retained entries.

- `wardline-core`: the core vocabulary — `Guard` (the trait to implement),
  `Verdict` (allow / block / modify), `GuardError`, `FailPolicy`, `Context`,
  `Deadline`, and `Value`. Types only; the pipeline executor is Phase 2. The
  crate has no dependencies outside `std` (implementation plan Phase 1).

### Reliability

- A guard that declares a `timeout()` is bounded by it: the pipeline stops
  waiting and reports `GuardError::Timeout`. This bounds the *caller's* wait,
  not the guard — abandoned work keeps running on its own thread. A `strict()`
  guard is never abandoned; it runs inline and reports
  `GuardError::DeadlineViolated` if it overruns.
- `FailPolicy::FailClosedWithFallback` with no fallback registered on the
  pipeline halts as plain fail-closed. A missing fallback is never read as
  permission to continue.
- The trace has a hard entry cap (`Trace::DEFAULT_CAPACITY`, 64) and clamps
  block reasons, so memory use is flat regardless of pipeline length. A
  truncated trace reports how many entries it dropped.
- `FailPolicy::FailClosed` is the default for every guard, and a guard's
  failure is resolved by its own policy — the pipeline will never apply a
  workspace-wide default silently.
- `Guard` carries a `RefUnwindSafe` bound from its first release, so adding
  `catch_unwind` panic isolation in Phase 4.5 is not a breaking change.
- `GuardError` is `#[non_exhaustive]`, so new failure modes can be added
  without a major bump. Match with a wildcard arm.

- Workspace scaffolding: `wardline-core`, `wardline-guards`, `wardline-http`,
  and `wardline-llm` crate skeletons, dual MIT/Apache-2.0 licensing, contributor
  docs, and a CI workflow running fmt, clippy, test, MSRV, and rustdoc
  (implementation plan Phase 0).

[Unreleased]: https://github.com/Adarsh04Arun/Wardline/compare/HEAD...HEAD
