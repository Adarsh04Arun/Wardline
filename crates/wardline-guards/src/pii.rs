//! Baseline pattern detector for a few common PII shapes.
//!
//! This is a starter, not a compliance-grade detector. It will miss real PII
//! and will flag things that are not PII. Do not describe it as sufficient
//! for HIPAA, GDPR, or any other regime on its own.

use regex::Regex;
use wardline_core::{Context, Guard, GuardError, Verdict};

const EMAIL: &str = r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}";
const PHONE: &str = r"\b(?:\+?1[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?)\d{3}[-.\s]?\d{4}\b";
const SSN: &str = r"\b\d{3}-\d{2}-\d{4}\b";

fn compiled(pattern: &'static str) -> Result<Regex, GuardError> {
    Regex::new(pattern).map_err(|error| {
        GuardError::internal(format!("built-in PII pattern {pattern:?} failed: {error}"))
    })
}

/// What [`PiiGuard`] does when it finds a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PiiAction {
    /// Refuse the action. The default — leaking a match is worse than a
    /// false block on a reference detector.
    Block,
    /// Rewrite matches (`[EMAIL]`, `[PHONE]`, `[SSN]`) and continue.
    Redact,
}

/// Scans for email, US-ish phone, and SSN-shaped tokens.
///
/// Patterns are deliberately simple. International numbers, national IDs
/// outside the US SSN shape, and obfuscated emails (`name [at] host`) are
/// out of scope.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), wardline_core::GuardError> {
/// use wardline_core::{Context, Guard, Verdict};
/// use wardline_guards::{PiiAction, PiiGuard};
///
/// let blocker = PiiGuard::new()?;
/// let ctx = Context::new();
/// assert!(blocker
///     .check("mail me at ada@example.com", &ctx)
///     .is_ok_and(|v| v.is_block()));
///
/// let redactor = PiiGuard::new()?.with_action(PiiAction::Redact);
/// assert_eq!(
///     redactor.check("mail me at ada@example.com", &ctx),
///     Ok(Verdict::Modify("mail me at [EMAIL]".to_owned()))
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct PiiGuard {
    action: PiiAction,
    email: Regex,
    phone: Regex,
    ssn: Regex,
}

impl PiiGuard {
    /// Builds the starter detector in [`PiiAction::Block`] mode.
    pub fn new() -> Result<Self, GuardError> {
        Ok(PiiGuard {
            action: PiiAction::Block,
            email: compiled(EMAIL)?,
            phone: compiled(PHONE)?,
            ssn: compiled(SSN)?,
        })
    }

    /// Sets whether a match blocks or is rewritten. Builder-style.
    #[must_use]
    pub fn with_action(mut self, action: PiiAction) -> Self {
        self.action = action;
        self
    }

    /// The configured action.
    pub fn action(&self) -> PiiAction {
        self.action
    }

    fn first_kind(&self, input: &str) -> Option<&'static str> {
        if self.email.is_match(input) {
            Some("email")
        } else if self.ssn.is_match(input) {
            Some("ssn")
        } else if self.phone.is_match(input) {
            Some("phone")
        } else {
            None
        }
    }

    fn redact(&self, input: &str) -> String {
        let without_email = self.email.replace_all(input, "[EMAIL]");
        let without_ssn = self.ssn.replace_all(&without_email, "[SSN]");
        self.phone.replace_all(&without_ssn, "[PHONE]").into_owned()
    }
}

impl Guard for PiiGuard {
    type Input = str;
    type Output = String;

    fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
        match self.action {
            PiiAction::Block => match self.first_kind(input) {
                Some(kind) => Ok(Verdict::block(format!("{kind} detected"))),
                None => Ok(Verdict::Allow),
            },
            PiiAction::Redact => {
                let redacted = self.redact(input);
                if redacted == input {
                    Ok(Verdict::Allow)
                } else {
                    Ok(Verdict::Modify(redacted))
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "pii"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wardline_core::FailPolicy;

    fn guard() -> PiiGuard {
        match PiiGuard::new() {
            Ok(guard) => guard,
            Err(error) => panic!("built-in PII patterns must compile: {error}"),
        }
    }

    fn ctx() -> Context {
        Context::new()
    }

    #[test]
    fn clean_text_is_allowed() {
        let guard = guard();
        assert_eq!(
            guard.check("no identifiers here", &ctx()),
            Ok(Verdict::Allow)
        );
        assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
        assert_eq!(guard.name(), "pii");
        assert_eq!(guard.action(), PiiAction::Block);
    }

    #[test]
    fn email_phone_and_ssn_shapes_are_blocked() {
        let guard = guard();
        assert_eq!(
            guard.check("write to ada@example.com please", &ctx()),
            Ok(Verdict::block("email detected"))
        );
        assert_eq!(
            guard.check("call 415-555-0100", &ctx()),
            Ok(Verdict::block("phone detected"))
        );
        assert_eq!(
            guard.check("ssn 123-45-6789", &ctx()),
            Ok(Verdict::block("ssn detected"))
        );
    }

    #[test]
    fn redact_mode_rewrites_every_kind() {
        let guard = guard().with_action(PiiAction::Redact);
        assert_eq!(
            guard.check("ada@example.com / 123-45-6789 / 415-555-0100", &ctx()),
            Ok(Verdict::Modify("[EMAIL] / [SSN] / [PHONE]".to_owned()))
        );
        assert_eq!(guard.check("still clean", &ctx()), Ok(Verdict::Allow));
        assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
    }
}
