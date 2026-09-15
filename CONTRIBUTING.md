# Contributing to Wardline

Thanks for your interest. Wardline is a small, opinionated library, and the
constraints below exist to keep it that way — please read them before opening
a large PR, so your time isn't wasted on something that has to be turned down
for architectural reasons.

## Before you start

- Read [`AGENTS.md`](AGENTS.md). It lists the **non-negotiable architectural
  invariants** (no async runtime in the core, panics isolated at the pipeline
  boundary, fail-closed by default, always produce a trace). These apply to
  human and AI contributors equally.
- Read [`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md) and check
  which phase is active (Phase 8 is the remaining release work). Work that
  belongs to a later phase is usually better filed as an issue than sent as
  a PR.
- Read [`docs/RESEARCH.md`](docs/RESEARCH.md) for what this project is
  deliberately *not* trying to be.

## Filing issues

Good issues include:

- **Bugs:** what you expected, what happened, a minimal reproducing pipeline,
  and your `rustc --version` / OS.
- **Guard proposals:** the use case first, then the implementation sketch. The
  bar for a new built-in guard is high — the built-ins are reference
  implementations, not a catalogue. A guard that needs a heavy dependency
  almost certainly belongs in your own crate implementing the `Guard` trait.
- **Reliability reports:** anything that causes a panic to escape the
  pipeline, a trace to grow unboundedly, or a fail policy to be ignored is
  treated as a priority bug, not a nice-to-have.

Security-sensitive reports: please don't open a public issue. Use GitHub's
private vulnerability reporting on this repository instead.

## Pull request checklist

Before marking a PR ready for review:

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace --all-features` passes
- [ ] `cargo doc --workspace --no-deps` builds with no warnings
- [ ] `cargo deny check` and `cargo audit` pass
- [ ] New public items have doc comments (`#![deny(missing_docs)]` enforces this)
- [ ] New `Guard` impls have unit tests for allow, block/modify, and their
      declared `fail_policy()`
- [ ] Changes to `pipeline.rs` have an integration test under
      `crates/wardline-core/tests/`
- [ ] `CHANGELOG.md` updated under `Unreleased` for user-facing changes
- [ ] The PR description names the implementation-plan phase it belongs to

## Things that need a discussion first

Open an issue before writing code for any of these:

- Adding an external dependency to `wardline-core` or `wardline-guards`
- Changing a built-in guard's default `fail_policy()`
- Any `unsafe` code (every crate carries `#![forbid(unsafe_code)]`)
- Weakening or removing panic isolation, timeout handling, or the bounded trace
- Adding a "Guaranteed" claim to `docs/RELIABILITY.md` — every such line must
  cite a specific test that proves it

## Commit conventions

Conventional-commit prefixes: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`,
`chore:`. Prefer small commits that map to one acceptance criterion in the
implementation plan over one large commit per phase.

## Licensing of contributions

Wardline is dual-licensed under MIT and Apache-2.0. By submitting a
contribution you agree it is licensed under both, per the Apache-2.0
definition of "Contribution". There is no CLA and no DCO sign-off requirement.
