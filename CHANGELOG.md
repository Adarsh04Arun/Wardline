# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Reliability-relevant changes always get their own heading in a release entry —
never folded into a general bullet list — so anyone pinning Wardline in a
critical path can find them without reading every line.

## [Unreleased]

### Added

- `wardline-core`: the core vocabulary — `Guard` (the trait to implement),
  `Verdict` (allow / block / modify), `GuardError`, `FailPolicy`, `Context`,
  `Deadline`, and `Value`. Types only; the pipeline executor is Phase 2. The
  crate has no dependencies outside `std` (implementation plan Phase 1).

### Reliability

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

[Unreleased]: https://github.com/adarsh4arun/wardline/compare/HEAD...HEAD
