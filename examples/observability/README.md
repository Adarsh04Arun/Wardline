# observability

Phase 5 demo: the same pipeline as the other examples, but with the
`wardline-core/tracing` feature on and an in-process [`Metrics`] sink.

The run is three guards, in order:

1. **`always_allow`** — a normal allow (debug span `guard decided`).
2. **`explodes`** — panics on purpose. The pipeline catches it (Phase 4.5)
   and emits `guard panicked`. It is `FailOpen` so the next guard still
   runs.
3. **`always_block`** — a normal block that ends the run.

The process does not abort. After the run, the example prints the trace
and the per-guard counters (`allow` / `block` / `panic`).

## Run

```sh
cargo run -p observability
```

Look for a `wardline.evaluate` span, a `wardline.guard` span per check,
and the `guard panicked` error event from `explodes`.
