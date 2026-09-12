//! Property tests: for any sequence of guard outcomes, short-circuit and
//! fail-policy invariants hold, and a panic never escapes `evaluate`.

use proptest::prelude::*;
use std::sync::Arc;
use wardline_core::{Context, FailPolicy, Guard, GuardError, Pipeline, TraceOutcome, Verdict};

#[derive(Clone, Debug)]
enum OutcomeKind {
    Allow,
    Block,
    Error,
    Panic,
}

#[derive(Clone, Debug)]
enum PolicyKind {
    Open,
    Closed,
}

#[derive(Clone, Debug)]
struct Scripted {
    name: &'static str,
    kind: OutcomeKind,
    policy: FailPolicy,
}

const NAMES: [&str; 8] = ["g0", "g1", "g2", "g3", "g4", "g5", "g6", "g7"];

impl Guard for Scripted {
    type Input = str;
    type Output = String;

    fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
        match self.kind {
            OutcomeKind::Allow => Ok(Verdict::Allow),
            OutcomeKind::Block => Ok(Verdict::block(self.name)),
            OutcomeKind::Error => Err(GuardError::internal("scripted")),
            OutcomeKind::Panic => panic!("scripted panic"),
        }
    }

    fn fail_policy(&self) -> FailPolicy {
        self.policy
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

fn outcome_strat() -> impl Strategy<Value = OutcomeKind> {
    prop_oneof![
        Just(OutcomeKind::Allow),
        Just(OutcomeKind::Block),
        Just(OutcomeKind::Error),
        Just(OutcomeKind::Panic),
    ]
}

fn policy_strat() -> impl Strategy<Value = PolicyKind> {
    prop_oneof![Just(PolicyKind::Open), Just(PolicyKind::Closed)]
}

fn expected_halt(steps: &[(OutcomeKind, PolicyKind)]) -> Option<usize> {
    for (index, (kind, policy)) in steps.iter().enumerate() {
        match kind {
            OutcomeKind::Allow => {}
            OutcomeKind::Block => return Some(index),
            OutcomeKind::Error | OutcomeKind::Panic => {
                if matches!(policy, PolicyKind::Closed) {
                    return Some(index);
                }
            }
        }
    }
    None
}

proptest! {
    #[test]
    fn short_circuit_and_fail_policy_hold_for_any_script(
        steps in prop::collection::vec((outcome_strat(), policy_strat()), 0..8)
    ) {
        let mut pipeline: Pipeline<str, String> = Pipeline::new();
        for (index, (kind, policy)) in steps.iter().enumerate() {
            pipeline.push(Scripted {
                name: NAMES[index],
                kind: kind.clone(),
                policy: match policy {
                    PolicyKind::Open => FailPolicy::FailOpen,
                    PolicyKind::Closed => FailPolicy::FailClosed,
                },
            });
        }

        let halt_at = expected_halt(&steps);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline.evaluate(&Arc::from("payload"), &Context::new())
        }));
        let result = match caught {
            Ok(result) => result,
            Err(_) => {
                return Err(TestCaseError::fail("evaluate must never unwind"));
            }
        };

        let expected_len = halt_at.map(|index| index + 1).unwrap_or(steps.len());
        prop_assert_eq!(result.trace().len(), expected_len);

        if let Some(index) = halt_at {
            prop_assert!(result.is_block());
            prop_assert_eq!(result.trace().halted_by(), Some(NAMES[index]));
        } else {
            prop_assert!(result.is_allow());
            prop_assert_eq!(result.trace().halted_by(), None);
        }

        for (offset, entry) in result.trace().iter().enumerate() {
            match &steps[offset].0 {
                OutcomeKind::Allow => {
                    prop_assert_eq!(entry.outcome().kind(), "allowed");
                }
                OutcomeKind::Block => {
                    prop_assert_eq!(entry.outcome().kind(), "blocked");
                }
                OutcomeKind::Error => match entry.outcome() {
                    TraceOutcome::Failed { error, .. } => {
                        prop_assert!(!error.is_panic());
                        prop_assert_eq!(error.kind(), "internal");
                    }
                    other => {
                        return Err(TestCaseError::fail(format!(
                            "expected a failure, got {other:?}"
                        )));
                    }
                },
                OutcomeKind::Panic => match entry.outcome() {
                    TraceOutcome::Failed { error, .. } => {
                        prop_assert!(error.is_panic());
                    }
                    other => {
                        return Err(TestCaseError::fail(format!(
                            "expected a panic failure, got {other:?}"
                        )));
                    }
                },
            }
        }
    }
}
