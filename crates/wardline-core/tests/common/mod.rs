//! A configurable guard shared by the integration tests.
//!
//! Each test file is its own crate, so items only one of them uses look dead
//! to the others.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;
use wardline_core::{Context, FailPolicy, Guard, GuardError, Verdict};

/// Names of the guards that were actually entered, in order.
///
/// Trace length says a guard produced no entry; this says it was never
/// called at all, which is the stronger claim short-circuiting makes.
pub type Log = Arc<Mutex<Vec<&'static str>>>;

/// Creates an empty call log.
pub fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

/// Reads the call log. A poisoned lock reads as empty, which fails the
/// assertions that matter rather than panicking inside them.
pub fn ran(log: &Log) -> Vec<&'static str> {
    log.lock().map(|names| names.clone()).unwrap_or_default()
}

/// A guard whose behaviour is chosen at construction.
pub struct Probe {
    name: &'static str,
    log: Log,
    outcome: fn() -> Result<Verdict<String>, GuardError>,
    sleep: Option<Duration>,
    policy: FailPolicy,
    timeout: Option<Duration>,
    strict: bool,
}

impl Probe {
    pub fn new(
        name: &'static str,
        log: &Log,
        outcome: fn() -> Result<Verdict<String>, GuardError>,
    ) -> Self {
        Probe {
            name,
            log: Arc::clone(log),
            outcome,
            sleep: None,
            policy: FailPolicy::FailClosed,
            timeout: None,
            strict: false,
        }
    }

    /// Makes the guard take `duration` to answer.
    pub fn sleeping(mut self, duration: Duration) -> Self {
        self.sleep = Some(duration);
        self
    }

    pub fn policy(mut self, policy: FailPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }
}

impl Guard for Probe {
    type Input = str;
    type Output = String;

    fn check(&self, _input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
        if let Ok(mut names) = self.log.lock() {
            names.push(self.name);
        }
        if let Some(duration) = self.sleep {
            std::thread::sleep(duration);
        }
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

pub fn allow() -> Result<Verdict<String>, GuardError> {
    Ok(Verdict::Allow)
}

pub fn block() -> Result<Verdict<String>, GuardError> {
    Ok(Verdict::block("refused by policy"))
}

pub fn modify() -> Result<Verdict<String>, GuardError> {
    Ok(Verdict::Modify("[redacted]".to_owned()))
}

pub fn fail() -> Result<Verdict<String>, GuardError> {
    Err(GuardError::dependency("classifier unreachable"))
}
