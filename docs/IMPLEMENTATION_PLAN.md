# Implementation Plan — Wardline

**Status:** Phases 0–8 are in the tree. Tag `v0.1.0` is the crates.io cut.

A synchronous, embeddable, trait-based guardrail evaluation library in Rust.
Guards run inline in the caller's request path — before an action or LLM
response is released — and return a blocking verdict (allow / block / modify).

This document is written for coding agents (Claude Code, Cursor, etc.) to
execute against. Work through phases in order; do not start a phase until
the previous phase's acceptance criteria are met. Each phase lists exactly
which files it touches, so progress is traceable file-by-file.

See `AGENTS.md` at the repo root for standing conventions (style, testing,
commit format) that apply to every phase below — read that file first.

The project is deliberately, fully synchronous end to end. No phase should
introduce an async runtime dependency into `wardline-core` or `wardline-guards`.
Where an async framework is unavoidable for an integration (e.g. an
existing async web app), that boundary is isolated to a single optional
adapter crate — see Phase 4.

---

## Repo layout (target state)

```
wardline/
├── Cargo.toml                     # workspace root
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── AGENTS.md
├── CHANGELOG.md
├── .github/
│   └── workflows/
│       ├── ci.yml                 # fmt, clippy, test, on every PR
│       ├── audit.yml              # cargo-deny + cargo-audit, scheduled + on every PR
│       └── release.yml            # publish to crates.io on tag
├── docs/
│   ├── RESEARCH.md                # market research / positioning
│   ├── ARCHITECTURE.md            # design rationale, diagrams
│   ├── RELIABILITY.md             # explicit reliability contract — what's guaranteed, what isn't
│   └── IMPLEMENTATION_PLAN.md     # this file
├── crates/
│   ├── wardline-core/             # zero-dependency-ish core: trait + pipeline
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs             # public re-exports only
│   │       ├── guard.rs           # Guard trait
│   │       ├── verdict.rs         # Verdict, ModifiedOutput
│   │       ├── policy.rs          # FailPolicy
│   │       ├── deadline.rs        # Deadline, cooperative timeout support
│   │       ├── context.rs         # Context (request metadata, deadline)
│   │       ├── error.rs           # GuardError (includes Panicked variant)
│   │       ├── trace.rs           # bounded audit-trail ring buffer
│   │       └── pipeline.rs        # Pipeline executor (the sync core)
│   ├── wardline-guards/           # built-in guard implementations
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── regex_filter.rs    # pattern-based block/redact
│   │       ├── rate_limit.rs      # token-bucket guard
│   │       ├── pii.rs             # PII pattern detection
│   │       └── llm/
│   │           ├── mod.rs
│   │           ├── prompt_injection.rs
│   │           └── output_classifier.rs  # trait hook for external classifiers
│   ├── wardline-http/             # tower::Layer / axum middleware adapter (optional, isolates the only async boundary)
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── wardline-llm/              # wraps an LLM client call with a Pipeline
│       ├── Cargo.toml
│       └── src/lib.rs
├── examples/
│   ├── sync_http_server/          # tiny_http/rouille — zero-async reference example
│   │   └── src/main.rs
│   ├── axum_middleware/           # optional: for teams already on an async framework
│   │   └── src/main.rs
│   └── llm_chat_guard/            # runnable example: guard prompt + response, reqwest::blocking
│       └── src/main.rs
├── fuzz/
│   └── fuzz_targets/
│       └── pipeline_fuzz.rs       # cargo-fuzz target for the pipeline executor
└── tests/
    └── integration/
        ├── pipeline_short_circuit.rs
        ├── fail_policy.rs
        ├── timeout_behavior.rs
        └── panic_isolation.rs
```

---

## Phase 0 — Repo scaffolding & OSS hygiene

**Goal:** a repo that builds, lints, and is legally/structurally ready to be
public, before any guard logic exists.

**Files:**
- `Cargo.toml` (workspace, members = crates/*)
- `LICENSE-MIT`, `LICENSE-APACHE` (dual-license is the Rust ecosystem norm)
- `README.md` (placeholder: name, one-line pitch, status badge, license)
- `.github/workflows/ci.yml` — run `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` on every push/PR
- `.gitignore` (standard Rust)
- `CONTRIBUTING.md` — how to file issues, PR checklist, DCO/CLA if desired
- `CODE_OF_CONDUCT.md` — standard Contributor Covenant

**Acceptance criteria:**
- `cargo build` succeeds on an empty workspace with the crate skeletons below (empty `lib.rs` files are fine at this stage)
- CI workflow runs and passes on a trivial commit

---

## Phase 1 — Core types (`wardline-core`)

**Goal:** define the vocabulary every other crate builds on. No I/O, no
concurrency yet — just types and the trait.

**Files:** `crates/wardline-core/src/{guard,verdict,policy,deadline,context,error}.rs`, `lib.rs`

**Tasks:**
1. `verdict.rs` — define:
   ```rust
   pub enum Verdict<O = ()> {
       Allow,
       Block { reason: String },
       Modify(O),
   }
   ```
2. `policy.rs` — define:
   ```rust
   pub enum FailPolicy {
       FailOpen,
       FailClosed,
       FailClosedWithFallback, // pipeline substitutes a caller-provided default Verdict
   }
   ```
3. `deadline.rs` — define `Deadline(Instant)` with `remaining()`, `is_expired()`. This is what cooperative guards check.
4. `context.rs` — define `Context` carrying request metadata (free-form `HashMap<String, Value>` or similar) and the current `Deadline`.
5. `error.rs` — define `GuardError` as an error enum, at minimum `Timeout`, `Panicked(String)`, `Internal(String)`, `Dependency(String)`. `Panicked` is populated by the pipeline (Phase 4.5), not by guard authors.
6. `guard.rs` — define the core trait:
   ```rust
   pub trait Guard: Send + Sync + std::panic::RefUnwindSafe {
       type Input;
       type Output;
       fn check(&self, input: &Self::Input, ctx: &Context) -> Result<Verdict<Self::Output>, GuardError>;
       fn fail_policy(&self) -> FailPolicy { FailPolicy::FailClosed }
       fn timeout(&self) -> Option<Duration> { None } // None = no enforced timeout
       fn strict(&self) -> bool { false } // true = cooperative-only, no detached-thread fallback (see Phase 4.5)
       fn name(&self) -> &'static str;
   }
   ```
   Note the `RefUnwindSafe` bound — this is required for `catch_unwind` in Phase 4.5 and should be added now so it doesn't become a breaking change later.

**Acceptance criteria:**
- `wardline-core` compiles standalone with no external dependencies beyond `std` (this is a deliberate constraint — the core must stay embeddable without pulling in tokio, reqwest, etc.)
- Unit tests for `Deadline::is_expired` and `FailPolicy` exhaustiveness
- Every public item has a doc comment (`#![deny(missing_docs)]` on the crate)
- `#![forbid(unsafe_code)]` at the crate root

---

## Phase 2 — Pipeline executor

**Goal:** the actual synchronous, blocking, short-circuiting evaluator. This
is the piece that doesn't exist elsewhere — build it carefully and test it
harder than anything else in the repo.

**Files:** `crates/wardline-core/src/{pipeline,trace}.rs`, `tests/integration/{pipeline_short_circuit,fail_policy,timeout_behavior}.rs`

**Tasks:**
1. `Pipeline<In, Out>` holds an ordered `Vec<Box<dyn Guard<Input = In, Output = Out>>>`.
2. `evaluate(&self, input: &In, ctx: &Context) -> PipelineResult<Out>`:
   - Runs guards **in order**, short-circuits on first `Block`.
   - For each guard, if `guard.timeout()` is `Some(d)`, run the check via the thread + `mpsc::channel` + `recv_timeout(d)` pattern (see `docs/ARCHITECTURE.md` for why this is best-effort, not true cancellation) — unless `guard.strict()` is `true`, in which case see Phase 4.5.
   - On `GuardError` (including a timeout), apply *that guard's* `fail_policy()` — do not fall back to a pipeline-wide default silently.
   - Record a per-guard trace entry (name, verdict/error, elapsed time) into a **bounded** trace buffer (`trace.rs`) regardless of outcome — this is required for the audit-log story, not optional instrumentation. Bounded so a pathological pipeline can't grow the trace unboundedly.
3. `PipelineResult` should carry the final `Verdict` plus the full trace, so callers can log *why* something was blocked, not just that it was.

**Acceptance criteria:**
- Test: a `Block` from guard 2 of 3 prevents guard 3 from running at all
- Test: a guard configured `FailOpen` that errors still lets the pipeline continue; one configured `FailClosed` stops it
- Test: a guard that sleeps longer than its `timeout()` produces a `GuardError::Timeout`, and the pipeline's total wall-clock time is bounded by the timeout, not by the guard's actual runtime
- Test: trace output is present and ordered for both success and block cases, and the trace buffer's memory use is bounded regardless of pipeline length

---

## Phase 3 — Built-in guards (`wardline-guards`)

**Goal:** a small number of genuinely useful, dependency-light reference
implementations — enough to prove the trait works for both "general action"
and "LLM" use cases, not a comprehensive guard library.

**Files:** `crates/wardline-guards/src/{lib,regex_filter,rate_limit,pii}.rs`, `src/llm/{mod,prompt_injection,output_classifier}.rs`

**Tasks:**
1. `regex_filter.rs` — `RegexBlockGuard` (block on pattern match) and `RegexRedactGuard` (returns `Modify` with matches replaced). Uses the `regex` crate.
2. `rate_limit.rs` — simple token-bucket `RateLimitGuard`, in-memory, keyed by a field the caller extracts from `Context`.
3. `pii.rs` — a handful of common patterns (email, phone, SSN-shaped) as a starter `PiiGuard` — document clearly in-code that this is a baseline, not a compliance-grade PII detector.
4. `llm/output_classifier.rs` — **do not implement a classifier**. Define a trait/adapter so an external model (Llama Guard, a hosted API, a local heuristic) can be plugged in as a `Guard`. This keeps `wardline-guards` dependency-light and avoids scope creep into model hosting.
5. `llm/prompt_injection.rs` — a heuristic (keyword/pattern-based) guard as a cheap first line of defense, with docs explicitly noting it's not a substitute for a model-based classifier plugged in via the adapter above.

**Acceptance criteria:**
- Each guard has unit tests covering: normal allow, normal block/modify, and its declared `fail_policy()`
- None of these guards pull in async runtimes or network clients directly (the classifier *adapter* trait can be implemented by something that does, in a downstream crate/example)
- `#![forbid(unsafe_code)]` unless a specific dependency requires otherwise, documented inline if so

---

## Phase 4 — Integration adapters

**Goal:** prove the library is actually embeddable in the two contexts from
your scoping decision: general app actions and LLM calls — with a zero-async
reference path as the primary example, and an async adapter clearly marked
as the one deliberate exception.

**Files:** `crates/wardline-http/src/lib.rs`, `crates/wardline-llm/src/lib.rs`, `examples/sync_http_server/src/main.rs`, `examples/axum_middleware/src/main.rs`, `examples/llm_chat_guard/src/main.rs`

**Tasks:**
1. **`examples/sync_http_server`** (primary example) — use `tiny_http` or `rouille` (both blocking, thread-per-request) to demonstrate the pipeline with zero async anywhere in the call stack. This is what the README quickstart should point to first.
2. **`wardline-http`** (optional, secondary) — a `tower::Layer`/`Service` wrapper for teams already on axum. Document explicitly in the crate's top-level doc comment that this crate is the one deliberate async boundary in the project, and that the guard evaluation inside it is still synchronous — the async-ness is axum's, not Wardline's.
3. **`wardline-llm`** — a thin wrapper: `guarded_prompt(pipeline_in, pipeline_out, llm_call_fn, input) -> Result<Output, Blocked>`, built against `reqwest`'s `blocking` feature so the reference example stays fully synchronous end to end.
4. Three runnable examples matching the crates above, each guarding a trivial toy endpoint/prompt so a reader can `cargo run --example ...` and see a block happen in under a minute.

**Acceptance criteria:**
- All three examples run standalone with `cargo run -p <example>` and demonstrate at least one allow and one block
- `sync_http_server` has zero async dependencies in its own `Cargo.toml`
- README in each example explains what guard triggered and why

---

## Phase 4.5 — Hardening for critical-application reliability

**Goal:** move from "correct in the happy path" to "safe to embed in a
process where a misbehaving guard cannot be allowed to take the whole
process down with it." This phase is what makes the reliability claims in
`docs/RELIABILITY.md` true, not aspirational — do not write that document
until this phase's acceptance criteria pass.

**Files:** `crates/wardline-core/src/{pipeline,error,deadline}.rs`, `tests/integration/panic_isolation.rs`, `fuzz/fuzz_targets/pipeline_fuzz.rs`, `.github/workflows/audit.yml`, `docs/RELIABILITY.md`

**Tasks:**
1. **Panic isolation.** Wrap every `guard.check(...)` call in `std::panic::catch_unwind`. On a caught panic, produce `GuardError::Panicked(<message>)` and apply that guard's `fail_policy()` exactly as any other `GuardError` would be handled — a panicking guard must never unwind past the pipeline boundary. Requires the `RefUnwindSafe` bound added in Phase 1.
2. **Strict (cooperative-only) timeout mode.** For guards where `strict()` returns `true`, do not use the thread+channel fallback from Phase 2. Instead, only allow the guard to run if it demonstrably checks `ctx.deadline` — enforce this by having strict guards receive a `Deadline` they must consult, and treat any strict guard that exceeds its declared timeout as a **hard error** (`GuardError::DeadlineViolated`), not a soft timeout — this surfaces misbehaving guards during testing rather than masking them with a detached background thread in production.
3. **Bounded resource use.** Confirm the trace buffer from Phase 2 has a hard cap (e.g. last N entries, oldest dropped) and add a test proving memory use is flat regardless of pipeline length or guard count.
4. **Supply-chain hygiene.** Add `cargo-deny` (license + advisory + duplicate-dependency checks) and `cargo-audit` to `.github/workflows/audit.yml`, run on every PR and on a weekly schedule. Pin an MSRV in every crate's `Cargo.toml` and check it in CI.
5. **Adversarial testing.**
   - `tests/integration/panic_isolation.rs`: a guard that panics mid-check, one that panics on the first call and succeeds on retry, and a pipeline with a panicking guard sandwiched between two normal ones — verify the trace still records all three and the panicking guard's `fail_policy()` was honored.
   - Add `proptest`-based property tests for the pipeline: for any random sequence of guard outcomes (allow/block/error/panic in any order), the short-circuit and fail-policy invariants hold.
   - Add a `cargo-fuzz` target (`fuzz/fuzz_targets/pipeline_fuzz.rs`) fuzzing pipeline construction and evaluation against arbitrary guard behavior, run manually/periodically rather than in every CI run (fuzzing budgets are usually separate from PR-gating CI).
6. **Write `docs/RELIABILITY.md`** once 1–5 above are done and tested — see structure below. This is the artifact critical-adjacent adopters will actually read before your source code.

**`docs/RELIABILITY.md` required sections:**
- **Guaranteed:** a panicking guard cannot crash or unwind past the caller; fail-closed is the default and must be explicitly opted out of per-guard; the audit trail has bounded memory use; every dependency is scanned by `cargo-deny`/`cargo-audit` in CI.
- **Not guaranteed:** this is not formally certified for safety-critical systems (DO-178C, IEC 62304, ISO 26262, etc.) — that requires a certification body and audited process this project does not provide; non-strict (thread-based) timeouts are best-effort wall-clock bounds, not true cancellation, and a misbehaving non-strict guard can still consume background resources after its deadline; `strict` mode requires guard authors to cooperate with the deadline, and Wardline cannot force cooperation, only detect and hard-error on its absence.
- **SemVer and versioning policy:** no breaking changes without a major bump; reliability-relevant changes always noted in `CHANGELOG.md` under their own heading, not buried in general changes.

**Acceptance criteria:**
- `cargo test --workspace` includes and passes `panic_isolation.rs` and the proptest suite
- A guard that panics on every call does not cause `cargo test` (or any example) to abort — the process survives, the pipeline correctly reports the failure
- `cargo deny check` and `cargo audit` both pass clean in CI
- `docs/RELIABILITY.md` exists and every claim in its "Guaranteed" section is backed by a specific test named in this phase

---

## Phase 5 — Observability

**Goal:** the audit-log requirement from the pipeline trace (Phase 2) needs
a real sink, not just an in-memory buffer.

**Files:** `crates/wardline-core/src/pipeline.rs` (extend), new `crates/wardline-tracing/` if it grows large enough to warrant its own crate

**Tasks:**
1. Emit `tracing` spans/events per guard evaluation (feature-gated, so `wardline-core` doesn't force a `tracing` dependency on users who don't want it).
2. Provide a simple `Metrics` trait (counts of allow/block/error/panic per guard) that users can implement against Prometheus, StatsD, whatever they have.

**Acceptance criteria:**
- With the `tracing` feature off, zero additional dependencies compile in
- With it on, running an example produces readable structured spans, including panic events from Phase 4.5

---

## Phase 6 — Testing & benchmarks

**Files:** `tests/integration/*`, new `benches/pipeline.rs` (criterion)

**Tasks:**
1. Expand integration tests to cover multi-guard pipelines mixing block/modify/allow/panic across the built-in guards.
2. Add a criterion benchmark measuring pipeline overhead per guard, and separately the overhead added by `catch_unwind` (target: document the actual number, don't promise one in advance).
3. Add a concurrency test: multiple threads calling `pipeline.evaluate()` concurrently against a shared `Pipeline` (guards are `Send + Sync`, so this should be safe by construction — prove it).

**Acceptance criteria:**
- `cargo test --workspace` green
- `cargo bench` produces a checked-in baseline number referenced in `docs/ARCHITECTURE.md`, including the panic-isolation overhead figure

---

## Phase 7 — Docs & examples polish

**Files:** `README.md`, `docs/ARCHITECTURE.md`, doc comments across all public APIs

**Tasks:**
1. `README.md`: what it is, what it isn't (see `docs/RESEARCH.md` positioning and `docs/RELIABILITY.md` guarantees), quickstart (the `sync_http_server` example, abbreviated), link to full docs.
2. `docs/ARCHITECTURE.md`: write up the fail-policy, timeout, and panic-isolation design decisions from this conversation, including the honest caveat about non-strict thread-based timeouts not being true cancellation.
3. Ensure `cargo doc` builds cleanly with no warnings and every public item is documented (`#![deny(missing_docs)]` should already be forcing this from Phase 1 — verify it's holding).

**Status (this tree):** README quickstart, `docs/ARCHITECTURE.md`, and
`#![deny(missing_docs)]` are in place. Verify with
`cargo doc --workspace --no-deps --all-features` (RUSTDOCFLAGS=`-D warnings`).

**Acceptance criteria:**
- A newcomer can go from `git clone` to a running example in under 5 minutes following only the README

---

## Phase 8 — Open-source release readiness

**Files:** `.github/workflows/release.yml`, `CHANGELOG.md`, version fields across `Cargo.toml` files

**Tasks:**
1. Confirm crate names are available on crates.io (`wardline` confirmed available as of this plan's writing — re-verify immediately before publishing, since registries change).
2. Set up `cargo-release` or a manual tag-triggered publish workflow.
3. Write `CHANGELOG.md` starting at `0.1.0`.
4. Final pass: `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, `cargo test --workspace`, `cargo doc --workspace --no-deps`, `cargo deny check`, `cargo audit`.
5. Tag `v0.1.0`, publish.

**Status (this tree):** crate names re-checked unused on crates.io (2026-09-15).
`release.yml` publishes `wardline-core`, then `wardline-guards`,
`wardline-http`, and `wardline-llm` on `v*` tags when
`CARGO_REGISTRY_TOKEN` is set. `CHANGELOG.md` starts at `0.1.0`.

**Acceptance criteria:**
- Fresh clone + `cargo build --workspace` + `cargo test --workspace` succeed with no local state assumptions
- Public crates.io listing (or a clear "not yet published, use git dependency" note in README if you choose to hold off)

---

## Notes for coding agents

- Do not add `tokio` (or any async runtime) as a dependency of `wardline-core` or `wardline-guards`. If a phase seems to need it, stop and flag it — that's a signal the sync boundary is being violated, not a reason to add the dependency. The only sanctioned async boundary is `wardline-http`, and only because axum requires it.
- Every `Guard` implementation must return `Result`, never panic and never `.unwrap()` on anything derived from external input — but the pipeline must also defend against a guard panicking anyway (Phase 4.5). Both layers matter; neither substitutes for the other.
- Default to `FailPolicy::FailClosed` in every built-in guard unless there's a documented reason not to.
- Keep phases in separate commits/PRs where practical — the acceptance criteria per phase are meant to be independently verifiable.
- Do not write or edit `docs/RELIABILITY.md` claims ahead of the tests that back them — that document is a contract, not marketing copy.
