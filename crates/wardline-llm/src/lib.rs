//! Wraps an LLM call with Wardline input and output pipelines.
//!
//! The reference path stays synchronous end to end: guard the prompt, make a
//! **blocking** model call (`reqwest::blocking`, `ureq`, a local function),
//! guard the response. This crate does not start an async runtime and does
//! not host a model.
//!
//! ```text
//! prompt ──▶ input pipeline ──▶ llm_call ──▶ output pipeline ──▶ reply
//!                 │                                │
//!                 └── Block: never call the model  └── Block: never release the reply
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::Arc;
use wardline_core::{Context, Pipeline, Trace, Verdict};

/// Which side of the model call refused the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The prompt was blocked. The model was never called.
    Input,
    /// The model ran, but its reply was blocked.
    Output,
}

impl Stage {
    /// A stable label for logs and metrics.
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Input => "input",
            Stage::Output => "output",
        }
    }
}

impl core::fmt::Display for Stage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A pipeline refused the prompt or the reply.
#[derive(Debug, Clone)]
pub struct Blocked {
    stage: Stage,
    reason: String,
    trace: Trace,
}

impl Blocked {
    /// Whether the prompt or the reply was refused.
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// Why it was refused.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The pipeline trace for the side that halted.
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// The guard that stopped the pipeline, if the trace still has it.
    pub fn halted_by(&self) -> Option<&'static str> {
        self.trace.halted_by()
    }
}

impl core::fmt::Display for Blocked {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} blocked by {}: {}",
            self.stage,
            self.halted_by().unwrap_or("unknown"),
            self.reason
        )
    }
}

impl std::error::Error for Blocked {}

/// Failure of [`guarded_prompt`]: a guard said no, or the model call failed.
#[derive(Debug)]
pub enum GuardedError<E> {
    /// A pipeline halted. See [`Blocked::stage`] for which side.
    Blocked(Blocked),
    /// The blocking model function returned an error.
    Model(E),
}

impl<E: core::fmt::Display> core::fmt::Display for GuardedError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GuardedError::Blocked(blocked) => write!(f, "{blocked}"),
            GuardedError::Model(error) => write!(f, "model call failed: {error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for GuardedError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GuardedError::Blocked(blocked) => Some(blocked),
            GuardedError::Model(error) => Some(error),
        }
    }
}

/// Guards `prompt`, calls `llm` only if allowed, then guards the reply.
///
/// A [`Verdict::Modify`] on the input pipeline is what the model sees. A
/// modify on the output pipeline is what the caller receives. A block on
/// either side is [`GuardedError::Blocked`] — the model is skipped on an
/// input block, and a blocked reply is not returned.
pub fn guarded_prompt<E>(
    input_pipeline: &Pipeline<str, String>,
    output_pipeline: &Pipeline<str, String>,
    llm: impl FnOnce(&str) -> Result<String, E>,
    prompt: &str,
    ctx: &Context,
) -> Result<String, GuardedError<E>> {
    let incoming = Arc::<str>::from(prompt);
    let inbound = input_pipeline.evaluate(&incoming, ctx);
    let (verdict, trace) = inbound.into_parts();
    let outgoing = match verdict {
        Verdict::Block { reason } => {
            return Err(GuardedError::Blocked(Blocked {
                stage: Stage::Input,
                reason,
                trace,
            }));
        }
        Verdict::Modify(rewritten) => rewritten,
        Verdict::Allow => prompt.to_owned(),
    };

    let raw = llm(&outgoing).map_err(GuardedError::Model)?;
    let outgoing_reply = Arc::<str>::from(raw.as_str());
    let outbound = output_pipeline.evaluate(&outgoing_reply, ctx);
    let (verdict, trace) = outbound.into_parts();
    match verdict {
        Verdict::Block { reason } => Err(GuardedError::Blocked(Blocked {
            stage: Stage::Output,
            reason,
            trace,
        })),
        Verdict::Modify(rewritten) => Ok(rewritten),
        Verdict::Allow => Ok(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wardline_core::{Guard, GuardError, Verdict};

    struct Scripted {
        name: &'static str,
        decide: fn(&str) -> Result<Verdict<String>, GuardError>,
    }

    impl Guard for Scripted {
        type Input = str;
        type Output = String;

        fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
            (self.decide)(input)
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    fn echo(prompt: &str) -> Result<String, String> {
        Ok(format!("echo:{prompt}"))
    }

    fn boom(_prompt: &str) -> Result<String, String> {
        Err("provider down".to_owned())
    }

    fn empty() -> Pipeline<str, String> {
        Pipeline::new()
    }

    fn ctx() -> Context {
        Context::new()
    }

    #[test]
    fn an_allowed_prompt_reaches_the_model_and_comes_back() {
        let reply = match guarded_prompt(&empty(), &empty(), echo, "hello", &ctx()) {
            Ok(reply) => reply,
            Err(error) => panic!("expected a reply, got {error}"),
        };
        assert_eq!(reply, "echo:hello");
    }

    #[test]
    fn an_input_block_never_calls_the_model() {
        let input = Pipeline::new().with(Scripted {
            name: "secrets",
            decide: |text| {
                if text.contains("sk-") {
                    Ok(Verdict::block("api key"))
                } else {
                    Ok(Verdict::Allow)
                }
            },
        });
        let called = std::sync::atomic::AtomicBool::new(false);
        let result = guarded_prompt(
            &input,
            &empty(),
            |prompt| {
                called.store(true, std::sync::atomic::Ordering::SeqCst);
                echo(prompt)
            },
            "key sk-abc",
            &ctx(),
        );
        let error = match result {
            Err(error) => error,
            Ok(reply) => panic!("expected a block, got {reply}"),
        };
        match error {
            GuardedError::Blocked(blocked) => {
                assert_eq!(blocked.stage(), Stage::Input);
                assert_eq!(blocked.reason(), "api key");
                assert_eq!(blocked.halted_by(), Some("secrets"));
            }
            GuardedError::Model(error) => panic!("expected a block, got model error {error}"),
        }
        assert!(
            !called.load(std::sync::atomic::Ordering::SeqCst),
            "the model must not run after an input block"
        );
    }

    #[test]
    fn an_output_block_discards_the_reply() {
        let output = Pipeline::new().with(Scripted {
            name: "toxicity",
            decide: |text| {
                if text.contains("nope") {
                    Ok(Verdict::block("toxic"))
                } else {
                    Ok(Verdict::Allow)
                }
            },
        });
        let result = guarded_prompt(
            &empty(),
            &output,
            |_prompt: &str| -> Result<String, String> { Ok("nope".to_owned()) },
            "hello",
            &ctx(),
        );
        match result {
            Err(GuardedError::Blocked(blocked)) => {
                assert_eq!(blocked.stage(), Stage::Output);
                assert_eq!(blocked.reason(), "toxic");
            }
            other => panic!("expected an output block, got {other:?}"),
        }
    }

    #[test]
    fn input_modify_is_what_the_model_sees() {
        let input = Pipeline::new().with(Scripted {
            name: "redact",
            decide: |text| Ok(Verdict::Modify(text.replace("secret", "[redacted]"))),
        });
        let reply = match guarded_prompt(&input, &empty(), echo, "a secret value", &ctx()) {
            Ok(reply) => reply,
            Err(error) => panic!("expected a reply, got {error}"),
        };
        assert_eq!(reply, "echo:a [redacted] value");
    }

    #[test]
    fn output_modify_is_what_the_caller_receives() {
        let output = Pipeline::new().with(Scripted {
            name: "redact",
            decide: |text| Ok(Verdict::Modify(text.replace("echo:", ""))),
        });
        let reply = match guarded_prompt(&empty(), &output, echo, "hello", &ctx()) {
            Ok(reply) => reply,
            Err(error) => panic!("expected a reply, got {error}"),
        };
        assert_eq!(reply, "hello");
    }

    #[test]
    fn a_model_error_is_not_turned_into_a_block() {
        let result = guarded_prompt(&empty(), &empty(), boom, "hello", &ctx());
        match result {
            Err(GuardedError::Model(error)) => assert_eq!(error, "provider down"),
            other => panic!("expected a model error, got {other:?}"),
        }
    }
}
