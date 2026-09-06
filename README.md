# Wardline

**A synchronous, embeddable, trait-based guardrail evaluation library for Rust.**

[![CI](https://github.com/adarsh4arun/wardline/actions/workflows/ci.yml/badge.svg)](https://github.com/adarsh4arun/wardline/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

> **Status: pre-alpha, Phase 4.** Core types, the pipeline, built-in
> guards, and the HTTP/LLM adapters are in place. Reliability hardening is
> next. Not published to crates.io. Follow `docs/IMPLEMENTATION_PLAN.md`
> for what lands when.

Guards run **inline in your request path** — before an action is taken or an
LLM response is released — and return a blocking verdict: allow, block, or
modify. No separate service, no network hop, no async runtime.

```text
request ──▶ Pipeline::evaluate() ──▶ Verdict::Allow  ──▶ action proceeds
                    │
                    └──────────────▶ Verdict::Block  ──▶ action never happens
```

## What it is

- **Embeddable.** A library you call in-process, not a proxy you deploy.
- **Trait-based.** Guards are ordinary Rust code implementing one trait, not
  entries in a YAML DSL.
- **Domain-agnostic.** The same pipeline guards a REST action and an LLM call.
- **Fully synchronous.** `wardline-core` and `wardline-guards` depend on
  nothing but `std`. The single sanctioned async boundary is the optional
  `wardline-http` adapter, and only because axum requires it.

## What it isn't

- Not a replacement for out-of-band, platform-scale trust & safety systems
  (see `docs/RESEARCH.md` for the honest positioning against prior art).
- Not certified for safety-critical systems, and it will never claim to be.
- Not a model host — model-based classifiers plug in through an adapter trait.

## Workspace layout

| Crate | Purpose |
|---|---|
| `crates/wardline-core` | `Guard` trait, `Verdict`, `FailPolicy`, `Deadline`, the pipeline executor, panic isolation, bounded audit trace. |
| `crates/wardline-guards` | Built-in reference guards (regex, rate limit, PII, LLM heuristics). |
| `crates/wardline-http` | Optional tower/axum middleware — the one async boundary. |
| `crates/wardline-llm` | Wraps a blocking LLM client call with input/output pipelines. |

## Documentation

- `docs/RESEARCH.md` — positioning, prior art, honest gap analysis
- `docs/IMPLEMENTATION_PLAN.md` — the phased build plan
- `AGENTS.md` — conventions and architectural invariants for contributors
  (human or AI)
- `docs/ARCHITECTURE.md` and `docs/RELIABILITY.md` — land in later phases

## Quickstart

The primary example is a blocking HTTP server. No async runtime:

```sh
cargo run -p sync_http_server
```

That prints one allow and one block (`PromptInjectionGuard`), then exits.
`--listen 127.0.0.1:3000` binds a real port. See each example's README for
what guard fired and why.

Already on axum? `cargo run -p axum_middleware`. Wrapping an LLM call?
`cargo run -p llm_chat_guard`.

## Building

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Minimum supported Rust version: **1.85**.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Issues and PRs are welcome —
please reference the implementation-plan phase you're working in.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in this work by you, as defined in the
Apache-2.0 license, shall be dual-licensed as above, without any additional
terms or conditions.
