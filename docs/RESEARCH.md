# Research — positioning, prior art, and honest gap analysis

This document exists so that anyone evaluating this project — including
future contributors, and you six months from now — can see exactly what
already exists, what this project actually adds, and where the honest
limitations are. It is meant to be updated as the landscape changes, not
written once and frozen.

## 1. The problem this addresses

Guardrail/safety systems for LLMs and application actions generally fall
into one of two shapes:

- **Post-hoc / out-of-band detection.** The action happens, gets published
  as an event, and is evaluated asynchronously — by the time a rule fires,
  the original action has already completed. Discord's Osprey is the
  clearest public example: it's an event stream decisions engine where a
  Rust coordinator manages asynchronous event streams from message queues,
  with rule evaluation happening on separate stateless workers. This is the
  right shape when you're operating at platform scale (Osprey evaluates
  millions of rules per second across hundreds of millions of daily
  actions) and can tolerate a short window between action and response.

- **Inline / synchronous prevention.** The check must complete *before*
  the action is allowed to proceed at all — the caller blocks on the
  verdict. This is the shape LLM guardrails inherently need (you can't
  un-send a response that already reached the user), and it's the shape
  this project targets.

Both are legitimate, solving different problems. This project is not a
faster or better Osprey — it's a different point in the pipeline.

## 2. Landscape scan (as of this writing)

| Tool | Language | Shape | Scope | Notes |
|---|---|---|---|---|
| NVIDIA NeMo Guardrails | Python | Inline, config/DSL-driven | LLM-only | Widely adopted; "rails" define input/output/dialog control |
| Guardrails AI | Python/JS | Inline, validator-driven | LLM-only | Validators from a hub; structured-output focus |
| LMQL, Rebuff | Python | Inline | LLM-only | Narrower, prompt-injection and query-constraint focused |
| Llama Guard | Model weights | Inline (as a classifier) | LLM-only | A model you call, not a framework — composable as a `Guard` impl, not a competitor |
| **guardrail-rs** | Rust | Inline, **reverse proxy** | LLM-only | Sits between app and LLM provider as a network hop; you point your SDK's base URL at it |
| **Fortified LLM Client** | Rust | Inline, CLI + library | LLM-only | Regex, Llama Guard, and other layered checks bundled into an LLM client |
| **Osprey** (Discord/ROOST) | Rust + Python | Async, out-of-band | General (platform trust & safety) | The async reference point this project deliberately diverges from |

## 3. The actual gap

Every existing inline tool is either:
1. Python and LLM-only (NeMo, Guardrails AI, LMQL, Rebuff), or
2. Rust but shaped as a **network proxy** you deploy and route traffic
   through (guardrail-rs), rather than a library you embed directly, or
3. Rust and LLM-specific by design (Fortified LLM Client).

None combine: **embeddable (in-process, no separate service to deploy),
trait-based (checks are code, not config), and domain-agnostic (the same
pipeline abstraction guards a REST action or an LLM call)**.

That combination — not "guardrails" as a category, which is crowded — is
the honest scope of what this project adds. It is a narrower and more
specific claim than "there's no guardrail library for Rust," which is
false, and worth stating plainly so the project doesn't oversell itself
in its own README.

## 4. Why this shape is useful (not just different)

- **No extra network hop.** A reverse-proxy design (guardrail-rs) adds a
  hop between app and provider — fine for many deployments, but it means
  every request pays proxy latency and the proxy becomes another service
  to deploy, scale, and secure. An embedded library runs in the same
  process as the caller.
- **One abstraction for two problems.** Teams building both a REST API and
  an LLM feature (which describes a lot of "AI-native product" teams,
  including healthcare/edu contexts where both patient-facing actions and
  model outputs need gating) currently reach for two different tools. One
  `Guard` trait covering both means one mental model, one audit trail
  format, one place to reason about fail-open/fail-closed policy.
- **Rules as code, not config.** DSL/config-driven tools trade flexibility
  for lower barrier to entry. A trait-based design is a deliberate
  trade-off the other way: harder to hand to a non-engineer, but every
  check gets full language support (types, testing, IDE navigation) with
  no DSL to learn or debug.
- **Fully synchronous by design, not by omission.** The library never
  depends on an async runtime. This is what makes it embeddable in
  contexts an async-only library structurally can't reach without a bridge
  — see section 5.

## 5. Who this is actually for

**Primary target: teams building LLM-backed features and/or general app
actions who want inline enforcement without standing up a separate proxy
service**, and who are comfortable writing guard logic in Rust rather than
a config file. This is the use case the project was scoped around from the
start — it's a smaller, more specific audience than "anyone doing LLM
safety," and that's fine; it doesn't need to be everything to be useful.

**Secondary fits, not the headline use case:** the sync-only, runtime-free
design happens to also suit a number of other contexts well, because they
share the same underlying need — no async executor already running, or a
need for FFI-friendly, low-overhead calls:
- CLI tools and agentic dev tools running as blocking scripts
- Embedded/IoT/robotics control loops gating an actuator command before it
  fires
- FFI-exposed libraries (Python via PyO3, Node via napi-rs), where bridging
  a caller into an async Rust runtime is real friction this design avoids
- Thread-per-request sync web servers (`tiny_http`, `rouille`)
- Desktop/native GUI apps (Tauri, egui) gating an action in an event handler
- Batch/pipeline and ETL jobs validating records inline
- WASM plugin/sandbox hosts, many of which don't carry an async executor at all

It is important not to conflate this list with a claim that the project is
*built for* real-time-critical or safety-certified systems (aviation,
medical device firmware, automotive). Embedded/IoT is a good-fit secondary
audience because of the shared architectural property (no runtime, bounded
overhead) — it is not evidence of, or a substitute for, formal safety
certification. See `docs/RELIABILITY.md` for the explicit, tested
guarantees this project does and does not make, which is the honest basis
for any "suitable for critical applications" claim, not the target list
above on its own.

**Not the target:** platforms operating at Discord's scale who need async,
high-throughput detection across a queue — Osprey (or something like it)
remains the right tool there.

## 6. Honest limitations to state up front (in the README, not buried)

- **Not a replacement for async detection systems.** If you need to catch
  slow-forming abuse patterns across millions of events, this tool doesn't
  do that — it only ever sees one request at a time, synchronously.
- **Not formally certified for safety-critical systems.** Reliability
  hardening (panic isolation, bounded resource use, supply-chain scanning
  — see `docs/RELIABILITY.md`) raises the bar for production trust; it is
  not equivalent to DO-178C, IEC 62304, ISO 26262, or similar certification,
  which requires an audited process this project does not provide.
- **Synchronous timeouts are best-effort in non-strict mode, not true
  cancellation.** A blocking guard that ignores its deadline can still run
  to completion in the background even after the pipeline has moved on
  (see `docs/ARCHITECTURE.md`). `strict` mode converts this into a hard,
  detectable error instead of a silent leak, but still requires the guard
  author to cooperate with the deadline — it can't force cooperation, only
  refuse to mask its absence.
- **Built-in guards are reference implementations, not compliance-grade.**
  The PII detector, in particular, should never be marketed as sufficient
  for HIPAA/GDPR compliance on its own — say so explicitly wherever it's
  mentioned.
- **At extreme throughput, synchronous blocking will eventually lose to
  async** — this is a property of I/O, not something better engineering
  fixes. The pitch is simplicity, embeddability, and auditability for
  teams who aren't at that scale, not raw throughput superiority.

## 7. Naming

**Wardline** — confirmed available on crates.io at time of writing (a
close alternative, `bulwark`, is already taken by an unrelated
security/hardening project and was deliberately avoided to prevent
confusion). Re-verify availability immediately before the Phase 8 release
step, since registries change.
