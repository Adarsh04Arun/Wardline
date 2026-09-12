# Pipeline fuzz target

Adversarial testing for `Pipeline::evaluate` (implementation plan Phase 4.5).
Not part of the main workspace: libFuzzer needs nightly and does not build
cleanly on every host, so a normal `cargo test --workspace` never sees this
crate.

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run pipeline_fuzz
```

Run from this directory (`fuzz/`) or via `cargo fuzz` from the repo root if
you have cargo-fuzz configured. Budget this separately from PR-gating CI.
