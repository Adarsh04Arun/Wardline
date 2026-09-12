# AGENTS.md

Instructions for any AI coding agent (Claude Code, Cursor, Copilot Workspace,
etc.) working in this repository. Read this before making changes. If an
instruction here conflicts with a prompt you were given for a specific task,
this file wins unless the task explicitly says to override it.

## What this project is

A synchronous, embeddable, trait-based guardrail evaluation library in Rust,
hardened for use in reliability-sensitive applications. Read
`docs/RESEARCH.md` for positioning, `docs/ARCHITECTURE.md` for design
rationale, and `docs/RELIABILITY.md` for the explicit reliability contract
before making architectural changes — don't re-derive decisions that are
already written down.

## Non-negotiable architectural invariants

These are not style preferences — violating them breaks the reason this
project exists. Do not "fix" them without a discussion in an issue first.

1. **`wardline-core` and `wardline-guards` never depend on an async
   runtime.** No `tokio`, no `async-std`, no `futures` executor. The only
   sanctioned async boundary in the whole workspace is `wardline-http`,
   and only because axum requires it. If a task seems to require async
   anywhere else, stop and flag it rather than adding the dependency.
2. **Guards must not panic — and the pipeline must not trust that they
   won't.** Guard authors write `Result`-returning code with no
   `.unwrap()`/`.expect()` on external input. Independently, the pipeline
   wraps every `guard.check()` call in `catch_unwind` and converts a panic
   into `GuardError::Panicked`. Both layers are required; neither is
   optional because the other exists.
3. **Default fail policy is `FailClosed`.** Any built-in guard that departs
   from this must say why in a doc comment on the impl.
4. **Every guard declares its own timeout, strictness, and fail policy** —
   never rely on a pipeline-wide default silently applying to all guards.
5. **`strict` guards get hard errors, not silent leaks.** A guard marked
   `strict()` that exceeds its deadline produces `GuardError::DeadlineViolated`
   and must not fall back to a detached background thread. Non-strict
   guards may use the thread+channel timeout pattern, but this must always
   be documented as best-effort wall-clock bounding, not true cancellation.
6. **The pipeline always produces a trace**, even on the allow path and
   even when a guard panics. Don't optimize this away; it's required for
   the audit-log use case, not optional instrumentation. The trace buffer
   is bounded (fixed capacity) — never let it grow unboundedly with
   pipeline length.
7. **No new `unsafe` code without a documented, reviewed reason.** Every
   crate carries `#![forbid(unsafe_code)]` unless a specific, named
   dependency requires an exception — and that exception must be scoped to
   the smallest possible module, not applied crate-wide.
8. **Claims in `docs/RELIABILITY.md` must be backed by a named test.**
   Do not add or edit a "Guaranteed" line in that file without also adding
   or pointing to the specific test that proves it.

## Where things live (read this instead of guessing)

| Path | Purpose |
|---|---|
| `crates/wardline-core/` | Trait, Verdict, FailPolicy, Deadline, Pipeline, panic isolation, bounded trace, optional `tracing` feature, `Metrics`. Zero *required* dependencies; `tracing` is feature-gated. |
| `crates/wardline-guards/` | Built-in reference guards (regex, rate limit, PII, LLM heuristics). |
| `crates/wardline-http/` | tower/axum integration — the one place an async boundary is expected. |
| `crates/wardline-llm/` | Wraps an LLM call (via `reqwest::blocking`) with input/output pipelines. |
| `examples/sync_http_server/` | Primary reference example — zero async anywhere in the stack. |
| `examples/axum_middleware/` | Secondary example for teams already on an async framework. |
| `examples/llm_chat_guard/` | Guard a prompt and reply around a blocking model call. |
| `examples/observability/` | `tracing` spans and `Metrics` counters, including a caught panic. |
| `fuzz/` | `cargo-fuzz` targets for the pipeline executor. Run manually/periodically, not gating every PR. |
| `tests/integration/` | Cross-crate behavior tests, especially fail-policy, timeout, and panic-isolation edge cases. |
| `docs/RESEARCH.md` | Why this project exists, prior art, honest limitations. Update if positioning changes. |
| `docs/ARCHITECTURE.md` | Design rationale for fail-policy/timeout/panic-isolation decisions. |
| `docs/RELIABILITY.md` | The explicit reliability contract — what's guaranteed, what isn't, and the test backing each guarantee. |
| `docs/IMPLEMENTATION_PLAN.md` | Phased build plan — check which phase is active before adding scope. |
| `docs/PAPER_TITLES.md` | Curated taxonomy of IEEE paper titles and venue recommendations. |
| `docs/IEEE_RESEARCH_PAPER.md` | Primary manuscript (Systems & Guardrail Trilemma perspective). |
| `docs/IEEE_RESEARCH_PAPER_V2.md` | Secondary manuscript (Formal Methods, Pre-Action Invariants & Safety Shields perspective). |

When you touch a file, keep this table accurate. If you add a new crate or
move something, update this file in the same change.

## Style and tooling

- Format with `cargo fmt` before committing. CI will reject unformatted code.
- `cargo clippy --workspace -- -D warnings` must pass — treat clippy
  warnings as errors, not suggestions.
- `cargo deny check` and `cargo audit` must pass — treat a new advisory or
  license violation as a blocking issue, not a warning to note and move on.
- Every public item needs a doc comment. `wardline-core` has
  `#![deny(missing_docs)]` — do not remove or weaken this.
- Prefer small, focused commits that map to one acceptance criterion in
  `docs/IMPLEMENTATION_PLAN.md` rather than one commit per phase.

## Testing expectations

- New `Guard` implementations require unit tests covering: a normal allow,
  a normal block/modify, and behavior under its declared `fail_policy()`.
- Changes to `pipeline.rs` require an integration test in
  `tests/integration/`, not just a unit test — pipeline behavior is only
  meaningful across multiple guards.
- Any change touching panic handling, timeouts, or the trace buffer
  requires a corresponding case in `tests/integration/panic_isolation.rs`
  or the proptest suite — this is the code path `docs/RELIABILITY.md`
  makes promises about.
- Do not mark a phase from `docs/IMPLEMENTATION_PLAN.md` complete until its
  listed acceptance criteria pass, not just "code compiles."

## Commit / PR conventions

- Conventional commit prefixes: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`.
- Reference the implementation-plan phase in the PR description (e.g.
  "Phase 4.5 — hardening") so reviewers can check it against the
  acceptance criteria directly.
- Update `CHANGELOG.md` under `Unreleased` for any user-facing change.
  Reliability-relevant changes get their own heading in the changelog
  entry, not folded into general bullet points.

## Things to never do without asking a human first

- Adding a new external dependency to `wardline-core` or `wardline-guards`.
- Changing a built-in guard's default `fail_policy()`.
- Removing or loosening the timeout, panic-isolation, or bounded-trace
  mechanisms described in `docs/ARCHITECTURE.md` and `docs/RELIABILITY.md`.
- Weakening a "Guaranteed" claim in `docs/RELIABILITY.md`, or adding a new
  one without the test that proves it.
- Publishing to crates.io (Phase 8 is a deliberate, human-triggered step).
