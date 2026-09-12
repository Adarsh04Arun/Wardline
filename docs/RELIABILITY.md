# Reliability contract

This document states, in plain language, exactly what Wardline guarantees
and exactly what it does not. It exists because "reliable enough for
critical applications" is a claim people will test, not just read — every
line under **Guaranteed** below is backed by a specific, named test, and
this file should not be edited to add a new guarantee without also adding
or pointing to the test that proves it (see `AGENTS.md`).

## Guaranteed

- **A panicking guard cannot crash or unwind past the caller.** Every
  `Guard::check` call runs inside `std::panic::catch_unwind`. A panic is
  converted to `GuardError::Panicked` and handled through that guard's
  ordinary `fail_policy()`, exactly like any other error.
  *Proven by:* `crates/wardline-core/tests/panic_isolation.rs`
  (`a_guard_that_panics_mid_check_does_not_unwind_past_evaluate`,
  `a_guard_that_panics_on_every_call_does_not_abort_the_process`),
  and `Pipeline::a_panicking_guard_is_caught_and_does_not_unwind`.

- **Fail-closed is the default and must be explicitly opted out of.**
  Every built-in guard defaults to `FailPolicy::FailClosed`. A guard using
  `FailOpen` does so with a documented reason in its own source.
  *Proven by:* per-guard unit tests asserting `fail_policy()`, plus
  `crates/wardline-core/tests/fail_policy.rs`
  (`fail_closed_is_what_a_guard_gets_without_asking`).

- **The audit trail has bounded memory use regardless of pipeline length
  or guard count.** The trace buffer has a fixed capacity; older entries
  are dropped, not accumulated without bound.
  *Proven by:* `Trace::memory_stays_flat_however_many_guards_run` and
  `Pipeline::the_trace_is_bounded_however_long_the_pipeline_is`.

- **`strict` guards fail loudly, not silently, when they exceed their
  deadline.** A `strict()` guard that violates its declared timeout
  produces a hard `GuardError::DeadlineViolated` rather than continuing in
  a detached background thread.
  *Proven by:* `Pipeline::a_strict_guard_that_overruns_reports_a_contract_violation`
  and `crates/wardline-core/tests/timeout_behavior.rs`.

- **A panicking guard's fail policy is honored, and the trace still
  records neighbours.** A fail-open panic lets later guards run; a
  fail-closed panic stops the pipeline. Both leave a trace entry.
  *Proven by:* `crates/wardline-core/tests/panic_isolation.rs`
  (`a_panicking_guard_sandwiched_between_two_normal_ones_is_traced`,
  `a_fail_closed_panic_stops_later_guards`) and the proptest suite
  `crates/wardline-core/tests/pipeline_proptest.rs`
  (`short_circuit_and_fail_policy_hold_for_any_script`).

- **Every dependency is scanned in CI.** `cargo-deny` (license and
  advisory checks) and `cargo-audit` run on every PR and on a weekly
  schedule.
  *Proven by:* `.github/workflows/audit.yml`, required as a passing check.

- **No `unsafe` code without a documented, scoped exception.** Every crate
  carries `#![forbid(unsafe_code)]` by default.
  *Proven by:* crate-root lint attributes, checked in CI
  (`RUSTFLAGS: "-D warnings"` in `.github/workflows/ci.yml`).

## Not guaranteed

- **This is not formally certified for safety-critical systems.**
  Certifications such as DO-178C (aviation), IEC 62304 (medical device
  software), or ISO 26262 (automotive) require an audited process and a
  certification body — a solo open-source project does not provide this,
  regardless of code quality. Do not describe Wardline as "certified" or
  "safety-critical-ready" anywhere in this repo's docs or marketing.

- **Non-strict timeouts are best-effort wall-clock bounds, not true
  cancellation.** For a guard that doesn't set `strict()`, the pipeline
  waits up to the declared timeout and then treats it as a
  `GuardError::Timeout` — but the guard's underlying thread may continue
  running in the background after that point, consuming CPU/memory until
  it finishes naturally. This is a genuine constraint of building
  cancellation on top of blocking Rust threads without an async runtime,
  not a bug to be fixed later.

- **`strict` mode requires guard-author cooperation.** Wardline can detect
  and hard-error when a strict guard exceeds its deadline, but it cannot
  force a guard's internal logic to actually check the deadline partway
  through its own computation. Strict mode is a contract with guard
  authors, enforced at the boundary — it is not a sandbox.

- **Built-in guards are reference implementations, not compliance-grade.**
  In particular, the bundled PII detector is a starting point, not a
  HIPAA/GDPR-sufficient control on its own.

## Versioning policy

- No breaking changes to any public API without a major version bump.
- Any change to the guarantees listed above gets its own heading in
  `CHANGELOG.md` — never folded into a general bullet list — so anyone
  pinning this crate in a critical path can see reliability-relevant
  changes without reading every line of every release.
