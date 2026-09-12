//! The synchronous, short-circuiting evaluator that runs a list of guards.

use crate::observe::{self, GuardSpan};
use crate::{Context, FailPolicy, Guard, GuardError, Trace, TraceEntry, TraceOutcome, Verdict};
use core::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Recovers a panic payload as a string for [`GuardError::Panicked`].
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_owned(),
            Err(_) => "unknown panic payload".to_owned(),
        },
    }
}

/// Runs `guard.check`, converting a panic into [`GuardError::Panicked`].
///
/// This is the backstop that keeps a misbehaving guard from unwinding past
/// [`Pipeline::evaluate`]. Guard authors still must not panic.
fn invoke_check<In, Out>(
    guard: &dyn Guard<Input = In, Output = Out>,
    input: &In,
    ctx: &Context,
) -> Result<Verdict<Out>, GuardError>
where
    In: ?Sized,
{
    match catch_unwind(AssertUnwindSafe(|| guard.check(input, ctx))) {
        Ok(result) => result,
        Err(payload) => Err(GuardError::panicked(panic_message(payload))),
    }
}

/// A caller-supplied verdict for [`FailPolicy::FailClosedWithFallback`].
///
/// A closure rather than a stored `Verdict<Out>` so `Out` need not be
/// `Clone`, and so the substituted verdict can be built fresh each time.
type Fallback<Out> = Arc<dyn Fn() -> Verdict<Out> + Send + Sync>;

/// What the pipeline does about a guard that failed rather than decided.
enum Resolution<Out> {
    /// Skip this guard and keep going.
    Continue,
    /// Stop, with this verdict standing in for the one the guard never gave.
    Halt(Verdict<Out>),
}

/// An ordered list of guards, evaluated in place on the calling thread.
///
/// Guards run in the order they were added, and the first [`Verdict::Block`]
/// ends the run — the remaining guards are never called. This is why order is
/// worth thinking about: put the cheap, decisive checks first.
///
/// # Failure, not refusal
///
/// A guard that returns [`GuardError`] made no decision. The pipeline
/// resolves that with **that guard's own** [`Guard::fail_policy`], never a
/// pipeline-wide default, so what a broken guard does to your request path is
/// readable off the guard itself.
///
/// # Modifications
///
/// [`Verdict::Modify`] does not short-circuit; later guards still run and
/// still see the original input. The last modification wins, and the trace
/// records every one, because collapsing several rewrites into one is a
/// decision only the caller can make correctly.
///
/// # Sharing
///
/// A `Pipeline` is `Send + Sync` and holds no per-request state, so build it
/// once at startup and share it across request threads. Everything about one
/// evaluation lives in the [`PipelineResult`] it returns.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use wardline_core::{Context, Guard, GuardError, Pipeline, Verdict};
///
/// struct NonEmpty;
///
/// impl Guard for NonEmpty {
///     type Input = str;
///     type Output = ();
///
///     fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
///         if input.trim().is_empty() {
///             return Ok(Verdict::block("prompt is empty"));
///         }
///         Ok(Verdict::Allow)
///     }
///
///     fn name(&self) -> &'static str {
///         "non_empty"
///     }
/// }
///
/// struct NoApiKeys;
///
/// impl Guard for NoApiKeys {
///     type Input = str;
///     type Output = ();
///
///     fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict, GuardError> {
///         if input.contains("sk-") {
///             return Ok(Verdict::block("prompt contains an API key"));
///         }
///         Ok(Verdict::Allow)
///     }
///
///     fn name(&self) -> &'static str {
///         "no_api_keys"
///     }
/// }
///
/// let pipeline = Pipeline::new().with(NonEmpty).with(NoApiKeys);
/// let ctx = Context::new();
///
/// let allowed = pipeline.evaluate(&Arc::from("summarise this"), &ctx);
/// assert!(allowed.is_allow());
/// assert_eq!(allowed.trace().len(), 2);
///
/// // The first guard blocks, so the second one never runs.
/// let blocked = pipeline.evaluate(&Arc::from("   "), &ctx);
/// assert_eq!(blocked.block_reason(), Some("prompt is empty"));
/// assert_eq!(blocked.trace().len(), 1);
/// assert_eq!(blocked.trace().halted_by(), Some("non_empty"));
/// ```
pub struct Pipeline<In: ?Sized, Out = ()> {
    guards: Vec<Arc<dyn Guard<Input = In, Output = Out>>>,
    trace_capacity: usize,
    fallback: Option<Fallback<Out>>,
    /// Set when at least one guard will be run on a spawned thread, so the
    /// per-request `Context` is cloned once instead of once per guard.
    shares_context: bool,
}

impl<In: ?Sized, Out> Pipeline<In, Out> {
    /// Creates an empty pipeline.
    ///
    /// An empty pipeline allows everything. That is the honest answer to
    /// "no checks configured", but it is worth asserting against in the code
    /// that assembles one.
    pub fn new() -> Self {
        Pipeline {
            guards: Vec::new(),
            trace_capacity: Trace::DEFAULT_CAPACITY,
            fallback: None,
            shares_context: false,
        }
    }

    /// Appends a guard. Builder-style.
    #[must_use]
    pub fn with<G>(mut self, guard: G) -> Self
    where
        G: Guard<Input = In, Output = Out> + 'static,
    {
        self.push(guard);
        self
    }

    /// Appends a guard.
    pub fn push<G>(&mut self, guard: G)
    where
        G: Guard<Input = In, Output = Out> + 'static,
    {
        self.shares_context |= guard.timeout().is_some() && !guard.strict();
        self.guards.push(Arc::new(guard));
    }

    /// Sets how many trace entries an evaluation keeps. Builder-style.
    ///
    /// Defaults to [`Trace::DEFAULT_CAPACITY`]. Raise it for a long pipeline
    /// whose full trace you intend to persist; the cap itself is not
    /// removable, by design.
    #[must_use]
    pub fn with_trace_capacity(mut self, capacity: usize) -> Self {
        self.trace_capacity = capacity;
        self
    }

    /// Sets the verdict substituted for a failing
    /// [`FailPolicy::FailClosedWithFallback`] guard. Builder-style.
    ///
    /// It lives on the pipeline, not the guard, because the safe answer is a
    /// property of the call site. Without one, such a guard halts as plain
    /// [`FailPolicy::FailClosed`] — a missing fallback is never read as
    /// permission to continue.
    #[must_use]
    pub fn with_fallback<F>(mut self, fallback: F) -> Self
    where
        F: Fn() -> Verdict<Out> + Send + Sync + 'static,
    {
        self.fallback = Some(Arc::new(fallback));
        self
    }

    /// The number of guards.
    pub fn len(&self) -> usize {
        self.guards.len()
    }

    /// Returns `true` if this pipeline has no guards, and so allows
    /// everything.
    pub fn is_empty(&self) -> bool {
        self.guards.is_empty()
    }

    /// The guard names, in evaluation order.
    pub fn guard_names(&self) -> Vec<&'static str> {
        self.guards.iter().map(|guard| guard.name()).collect()
    }

    /// How long the pipeline will wait for one guard, if it will bound it at
    /// all.
    ///
    /// Only a declared [`Guard::timeout`] earns a bound; a request deadline
    /// alone does not, because enforcing one costs a thread per guard. The
    /// deadline still tightens a declared timeout, and cooperative guards can
    /// read it from the [`Context`] themselves.
    fn budget(guard: &dyn Guard<Input = In, Output = Out>, ctx: &Context) -> Option<Duration> {
        let timeout = guard.timeout()?;
        Some(match ctx.deadline() {
            Some(deadline) => timeout.min(deadline.remaining()),
            None => timeout,
        })
    }

    /// Turns a guard's failure into a decision, using that guard's policy.
    fn resolve(
        &self,
        name: &'static str,
        error: &GuardError,
        policy: FailPolicy,
    ) -> Resolution<Out> {
        match policy {
            FailPolicy::FailOpen => Resolution::Continue,
            FailPolicy::FailClosed => {
                Resolution::Halt(Verdict::block(format!("{name} failed: {error}")))
            }
            FailPolicy::FailClosedWithFallback => match &self.fallback {
                Some(fallback) => Resolution::Halt(fallback()),
                None => Resolution::Halt(Verdict::block(format!(
                    "{name} failed: {error} (no fallback verdict registered)"
                ))),
            },
        }
    }
}

impl<In, Out> Pipeline<In, Out>
where
    In: Send + Sync + ?Sized + 'static,
    Out: Send + 'static,
{
    /// Runs every guard in order and returns the decision with its trace.
    ///
    /// The input arrives as an [`Arc`] because a guard the pipeline may stop
    /// waiting on has to run on a thread that can outlive this call, and
    /// handing owned, shared data to that thread is the only way to bound the
    /// wait without `unsafe` or copying the input per guard. Guards still see
    /// a plain `&In`.
    ///
    /// Every `check` is wrapped in [`catch_unwind`]. A panic becomes
    /// [`GuardError::Panicked`] and is resolved through that guard's
    /// [`FailPolicy`]; it never unwinds out of this method.
    ///
    /// Enable the `tracing` feature to emit a `wardline.evaluate` span and a
    /// `wardline.guard` span (plus a `guard panicked` error event) for each
    /// check.
    ///
    /// [`catch_unwind`]: std::panic::catch_unwind
    pub fn evaluate(&self, input: &Arc<In>, ctx: &Context) -> PipelineResult<Out> {
        let _eval = observe::enter_evaluate();
        let mut trace = Trace::with_capacity(self.trace_capacity);
        let shared_ctx = self.shares_context.then(|| Arc::new(ctx.clone()));

        // Allow unless something says otherwise; a `Modify` replaces it.
        let mut verdict = Verdict::Allow;

        for guard in &self.guards {
            let name = guard.name();
            let span = GuardSpan::enter(name);
            let started = Instant::now();
            let outcome = self.run(guard, input, ctx, shared_ctx.as_ref());
            let elapsed = started.elapsed();

            match outcome {
                Ok(Verdict::Block { reason }) => {
                    let decided = Verdict::<Out>::Block {
                        reason: reason.clone(),
                    };
                    span.decided(&decided);
                    trace.record(TraceEntry::new(
                        name,
                        TraceOutcome::from_verdict(&decided),
                        elapsed,
                    ));
                    return PipelineResult {
                        verdict: Verdict::Block { reason },
                        trace,
                    };
                }
                Ok(decided) => {
                    span.decided(&decided);
                    trace.record(TraceEntry::new(
                        name,
                        TraceOutcome::from_verdict(&decided),
                        elapsed,
                    ));
                    if decided.is_modify() {
                        verdict = decided;
                    }
                }
                Err(error) => {
                    span.failed(&error);
                    let policy = guard.fail_policy();
                    let resolution = self.resolve(name, &error, policy);
                    let halted = matches!(resolution, Resolution::Halt(_));
                    trace.record(TraceEntry::new(
                        name,
                        TraceOutcome::Failed {
                            error,
                            policy,
                            halted,
                        },
                        elapsed,
                    ));
                    if let Resolution::Halt(substitute) = resolution {
                        return PipelineResult {
                            verdict: substitute,
                            trace,
                        };
                    }
                }
            }
        }

        PipelineResult { verdict, trace }
    }

    /// Runs one guard, bounding the wait if it asked to be bounded.
    fn run(
        &self,
        guard: &Arc<dyn Guard<Input = In, Output = Out>>,
        input: &Arc<In>,
        ctx: &Context,
        shared_ctx: Option<&Arc<Context>>,
    ) -> Result<Verdict<Out>, GuardError> {
        let Some(budget) = Self::budget(guard.as_ref(), ctx) else {
            return invoke_check(guard.as_ref(), input, ctx);
        };

        if budget.is_zero() {
            // The request deadline has already passed. Running the guard
            // cannot produce an answer anyone is still waiting for.
            return Err(if guard.strict() {
                GuardError::DeadlineViolated
            } else {
                GuardError::Timeout
            });
        }

        if guard.strict() {
            // A strict guard promised to police its own deadline, so it runs
            // inline and is never abandoned to a detached thread. If it
            // overruns anyway, that is a contract violation, not a timeout.
            let started = Instant::now();
            let result = invoke_check(guard.as_ref(), input, ctx);
            if result.is_ok() && started.elapsed() > budget {
                return Err(GuardError::DeadlineViolated);
            }
            return result;
        }

        // Best-effort, not cancellation: on timeout the pipeline stops
        // waiting, but the guard keeps running to completion on its own
        // thread. See `docs/ARCHITECTURE.md`.
        let (sender, receiver) = mpsc::channel();
        let guard = Arc::clone(guard);
        let input = Arc::clone(input);
        let ctx = match shared_ctx {
            Some(shared) => Arc::clone(shared),
            None => Arc::new(ctx.clone()),
        };

        std::thread::spawn(move || {
            let _ = sender.send(invoke_check(guard.as_ref(), &input, &ctx));
        });

        match receiver.recv_timeout(budget) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(GuardError::Timeout),
            // The thread ended without sending. Panics are caught above, so
            // this is a residual — the worker died some other way.
            Err(RecvTimeoutError::Disconnected) => Err(GuardError::internal(
                "guard thread ended without returning a verdict",
            )),
        }
    }
}

impl<In: ?Sized, Out> Default for Pipeline<In, Out> {
    fn default() -> Self {
        Pipeline::new()
    }
}

impl<In: ?Sized, Out> fmt::Debug for Pipeline<In, Out> {
    /// Names the guards rather than trying to print them — a `Guard` is
    /// arbitrary user code with no `Debug` bound.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipeline")
            .field("guards", &self.guard_names())
            .field("trace_capacity", &self.trace_capacity)
            .field("has_fallback", &self.fallback.is_some())
            .finish()
    }
}

/// One evaluation: the decision, and the record of how it was reached.
///
/// The trace comes back on every path, block or allow, because "why was this
/// blocked" is the question asked of a guardrail system in production, and
/// answering it after the fact from logs that were never written is not
/// possible.
#[derive(Debug, Clone)]
pub struct PipelineResult<Out = ()> {
    verdict: Verdict<Out>,
    trace: Trace,
}

impl<Out> PipelineResult<Out> {
    /// The pipeline's decision.
    pub fn verdict(&self) -> &Verdict<Out> {
        &self.verdict
    }

    /// Every guard that ran, in order.
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// Returns `true` if the action may proceed unchanged.
    pub fn is_allow(&self) -> bool {
        self.verdict.is_allow()
    }

    /// Returns `true` if the action was refused.
    ///
    /// True for a guard that said no *and* for one that failed under a
    /// halting [`FailPolicy`]; the trace distinguishes them.
    pub fn is_block(&self) -> bool {
        self.verdict.is_block()
    }

    /// Returns `true` if the action may proceed with a replacement payload.
    pub fn is_modify(&self) -> bool {
        self.verdict.is_modify()
    }

    /// Why the action was refused, if it was.
    pub fn block_reason(&self) -> Option<&str> {
        self.verdict.block_reason()
    }

    /// The replacement payload, if there is one.
    pub fn modified(&self) -> Option<&Out> {
        self.verdict.modified()
    }

    /// Takes the verdict, discarding the trace.
    pub fn into_verdict(self) -> Verdict<Out> {
        self.verdict
    }

    /// Takes both parts.
    pub fn into_parts(self) -> (Verdict<Out>, Trace) {
        (self.verdict, self.trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A guard whose behaviour is chosen at construction, so a test can
    /// describe a pipeline rather than define a type per case.
    struct Scripted {
        name: &'static str,
        outcome: fn() -> Result<Verdict<String>, GuardError>,
        policy: FailPolicy,
        timeout: Option<Duration>,
        strict: bool,
    }

    impl Scripted {
        fn new(name: &'static str, outcome: fn() -> Result<Verdict<String>, GuardError>) -> Self {
            Scripted {
                name,
                outcome,
                policy: FailPolicy::FailClosed,
                timeout: None,
                strict: false,
            }
        }

        fn with_policy(mut self, policy: FailPolicy) -> Self {
            self.policy = policy;
            self
        }

        fn with_timeout(mut self, timeout: Duration) -> Self {
            self.timeout = Some(timeout);
            self
        }

        fn strict(mut self) -> Self {
            self.strict = true;
            self
        }
    }

    impl Guard for Scripted {
        type Input = str;
        type Output = String;

        fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
            (self.outcome)()
        }

        fn fail_policy(&self) -> FailPolicy {
            self.policy
        }

        fn timeout(&self) -> Option<Duration> {
            self.timeout
        }

        fn strict(&self) -> bool {
            self.strict
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    fn allow() -> Result<Verdict<String>, GuardError> {
        Ok(Verdict::Allow)
    }

    fn block() -> Result<Verdict<String>, GuardError> {
        Ok(Verdict::block("refused"))
    }

    fn fail() -> Result<Verdict<String>, GuardError> {
        Err(GuardError::dependency("classifier unreachable"))
    }

    fn boom() -> Result<Verdict<String>, GuardError> {
        panic!("deliberate unit-test panic");
    }

    fn input() -> Arc<str> {
        Arc::from("payload")
    }

    #[test]
    fn an_empty_pipeline_allows_and_traces_nothing() {
        let pipeline: Pipeline<str, String> = Pipeline::new();
        assert!(pipeline.is_empty());

        let result = pipeline.evaluate(&input(), &Context::new());
        assert!(result.is_allow());
        assert!(result.trace().is_empty());
    }

    #[test]
    fn guards_run_in_the_order_they_were_added() {
        let pipeline = Pipeline::new()
            .with(Scripted::new("first", allow))
            .with(Scripted::new("second", allow))
            .with(Scripted::new("third", allow));

        assert_eq!(pipeline.guard_names(), ["first", "second", "third"]);

        let result = pipeline.evaluate(&input(), &Context::new());
        let names: Vec<_> = result.trace().iter().map(TraceEntry::name).collect();
        assert_eq!(names, ["first", "second", "third"]);
    }

    #[test]
    fn a_block_short_circuits_the_rest() {
        let pipeline = Pipeline::new()
            .with(Scripted::new("first", allow))
            .with(Scripted::new("second", block))
            .with(Scripted::new("third", allow));

        let result = pipeline.evaluate(&input(), &Context::new());
        assert_eq!(result.block_reason(), Some("refused"));
        assert_eq!(result.trace().len(), 2, "the third guard must not run");
        assert_eq!(result.trace().halted_by(), Some("second"));
    }

    #[test]
    fn a_modification_does_not_short_circuit_and_the_last_one_wins() {
        let pipeline = Pipeline::new()
            .with(Scripted::new("first", || {
                Ok(Verdict::Modify("once".to_owned()))
            }))
            .with(Scripted::new("second", allow))
            .with(Scripted::new("third", || {
                Ok(Verdict::Modify("twice".to_owned()))
            }));

        let result = pipeline.evaluate(&input(), &Context::new());
        assert_eq!(result.modified(), Some(&"twice".to_owned()));
        assert_eq!(result.trace().len(), 3);
    }

    #[test]
    fn a_failure_is_resolved_by_that_guards_own_policy() {
        let open = Pipeline::new()
            .with(Scripted::new("flaky", fail).with_policy(FailPolicy::FailOpen))
            .with(Scripted::new("after", allow));
        let result = open.evaluate(&input(), &Context::new());
        assert!(result.is_allow());
        assert_eq!(result.trace().len(), 2, "the pipeline must keep going");

        let closed = Pipeline::new()
            .with(Scripted::new("flaky", fail))
            .with(Scripted::new("after", allow));
        let result = closed.evaluate(&input(), &Context::new());
        assert!(result.is_block());
        assert_eq!(result.trace().len(), 1);
        assert_eq!(result.trace().halted_by(), Some("flaky"));
    }

    #[test]
    fn a_failure_is_traced_as_a_failure_not_as_a_refusal() {
        let pipeline = Pipeline::new().with(Scripted::new("flaky", fail));
        let result = pipeline.evaluate(&input(), &Context::new());

        let Some(entry) = result.trace().last() else {
            panic!("the failing guard should have been traced");
        };
        let TraceOutcome::Failed { error, policy, .. } = entry.outcome() else {
            panic!("expected a failure, got {:?}", entry.outcome());
        };
        assert_eq!(error.kind(), "dependency");
        assert_eq!(*policy, FailPolicy::FailClosed);
    }

    #[test]
    fn a_registered_fallback_replaces_the_failed_guards_verdict() {
        let pipeline = Pipeline::new()
            .with(Scripted::new("flaky", fail).with_policy(FailPolicy::FailClosedWithFallback))
            .with(Scripted::new("after", allow))
            .with_fallback(|| Verdict::Modify("safe default".to_owned()));

        let result = pipeline.evaluate(&input(), &Context::new());
        assert_eq!(result.modified(), Some(&"safe default".to_owned()));
        assert_eq!(result.trace().len(), 1, "the fallback still halts");
    }

    #[test]
    fn a_missing_fallback_halts_rather_than_letting_the_request_through() {
        let pipeline = Pipeline::new()
            .with(Scripted::new("flaky", fail).with_policy(FailPolicy::FailClosedWithFallback));

        let result = pipeline.evaluate(&input(), &Context::new());
        assert!(result.is_block());
        assert!(
            result
                .block_reason()
                .is_some_and(|reason| reason.contains("no fallback"))
        );
    }

    #[test]
    fn a_slow_guard_times_out_without_delaying_the_pipeline() {
        let pipeline = Pipeline::new()
            .with(
                Scripted::new("slow", || {
                    std::thread::sleep(Duration::from_secs(5));
                    allow()
                })
                .with_timeout(Duration::from_millis(50))
                .with_policy(FailPolicy::FailOpen),
            )
            .with(Scripted::new("after", allow));

        let started = Instant::now();
        let result = pipeline.evaluate(&input(), &Context::new());
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_secs(2),
            "waited {elapsed:?}: the pipeline must be bounded by the timeout, \
             not by the guard's real runtime"
        );
        assert!(result.is_allow(), "the guard failed open");

        let Some(entry) = result.trace().iter().next() else {
            panic!("the slow guard should have been traced");
        };
        let TraceOutcome::Failed { error, .. } = entry.outcome() else {
            panic!("expected a timeout, got {:?}", entry.outcome());
        };
        assert_eq!(*error, GuardError::Timeout);
    }

    #[test]
    fn a_guard_inside_its_timeout_is_left_alone() {
        let pipeline = Pipeline::new()
            .with(Scripted::new("prompt", allow).with_timeout(Duration::from_secs(30)));
        let result = pipeline.evaluate(&input(), &Context::new());
        assert!(result.is_allow());
        assert_eq!(result.trace().len(), 1);
    }

    #[test]
    fn a_strict_guard_that_overruns_reports_a_contract_violation() {
        let pipeline = Pipeline::new().with(
            Scripted::new("stubborn", || {
                std::thread::sleep(Duration::from_millis(60));
                allow()
            })
            .with_timeout(Duration::from_millis(10))
            .strict(),
        );

        let result = pipeline.evaluate(&input(), &Context::new());
        let Some(entry) = result.trace().last() else {
            panic!("the strict guard should have been traced");
        };
        let TraceOutcome::Failed { error, .. } = entry.outcome() else {
            panic!("expected a deadline violation, got {:?}", entry.outcome());
        };
        assert_eq!(*error, GuardError::DeadlineViolated);
    }

    #[test]
    fn an_expired_deadline_stops_a_bounded_guard_before_it_starts() {
        use crate::Deadline;

        let pipeline = Pipeline::new()
            .with(Scripted::new("bounded", allow).with_timeout(Duration::from_secs(30)));
        let expired = Context::new().with_deadline(Deadline::after(Duration::ZERO));

        let result = pipeline.evaluate(&input(), &expired);
        assert!(result.is_block());
        assert_eq!(result.trace().halted_by(), Some("bounded"));
    }

    #[test]
    fn the_trace_is_bounded_however_long_the_pipeline_is() {
        let mut pipeline: Pipeline<str, String> = Pipeline::new().with_trace_capacity(4);
        for _ in 0..200 {
            pipeline.push(Scripted::new("noisy", allow));
        }

        let result = pipeline.evaluate(&input(), &Context::new());
        assert!(result.is_allow());
        assert_eq!(result.trace().len(), 4);
        assert_eq!(result.trace().dropped(), 196);
        assert!(!result.trace().is_complete());
    }

    #[test]
    fn a_panicking_guard_is_caught_and_does_not_unwind() {
        let pipeline = Pipeline::new()
            .with(Scripted::new("volatile", boom).with_policy(FailPolicy::FailOpen))
            .with(Scripted::new("after", allow));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline.evaluate(&input(), &Context::new())
        }));
        let result = match result {
            Ok(result) => result,
            Err(_) => panic!("evaluate must not unwind past the caller"),
        };
        assert!(result.is_allow());
        assert_eq!(result.trace().len(), 2);

        let Some(entry) = result.trace().iter().next() else {
            panic!("the panicking guard should have been traced");
        };
        let TraceOutcome::Failed { error, halted, .. } = entry.outcome() else {
            panic!("expected a panic failure, got {:?}", entry.outcome());
        };
        assert!(error.is_panic());
        assert_eq!(error.message(), Some("deliberate unit-test panic"));
        assert!(!*halted);
    }

    #[test]
    fn a_pipeline_is_shareable_across_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Pipeline<str, String>>();

        let pipeline = Arc::new(Pipeline::new().with(Scripted::new("first", allow)));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let pipeline = Arc::clone(&pipeline);
            handles.push(std::thread::spawn(move || {
                pipeline.evaluate(&input(), &Context::new()).is_allow()
            }));
        }
        for handle in handles {
            assert_eq!(handle.join().ok(), Some(true));
        }
    }
}
