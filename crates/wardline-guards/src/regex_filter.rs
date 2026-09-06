//! Pattern-based block and redact guards.

use regex::Regex;
use wardline_core::{Context, Guard, GuardError, Verdict};

fn compile(pattern: &str) -> Result<Regex, GuardError> {
    Regex::new(pattern)
        .map_err(|error| GuardError::internal(format!("invalid regex {pattern:?}: {error}")))
}

/// Blocks the action when `pattern` matches anywhere in the input.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), wardline_core::GuardError> {
/// use wardline_core::{Context, Guard, Verdict};
/// use wardline_guards::RegexBlockGuard;
///
/// let guard = RegexBlockGuard::new(r"sk-[A-Za-z0-9]+")?;
/// let ctx = Context::new();
///
/// assert_eq!(guard.check("hello", &ctx), Ok(Verdict::Allow));
/// assert!(guard.check("key sk-abc123", &ctx).is_ok_and(|v| v.is_block()));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct RegexBlockGuard {
    pattern: Regex,
    reason: String,
    name: &'static str,
}

impl RegexBlockGuard {
    /// Builds a guard from a Rust regex. Fails if the pattern does not compile.
    pub fn new(pattern: &str) -> Result<Self, GuardError> {
        Ok(RegexBlockGuard {
            pattern: compile(pattern)?,
            reason: format!("matched /{pattern}/"),
            name: "regex_block",
        })
    }

    /// Replaces the default block reason. Builder-style.
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = reason.into();
        self
    }

    /// Replaces the default name (`regex_block`). Builder-style.
    #[must_use]
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    /// The compiled pattern.
    pub fn pattern(&self) -> &Regex {
        &self.pattern
    }
}

impl Guard for RegexBlockGuard {
    type Input = str;
    type Output = String;

    fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
        if self.pattern.is_match(input) {
            Ok(Verdict::block(self.reason.clone()))
        } else {
            Ok(Verdict::Allow)
        }
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// Rewrites every match of `pattern` and returns [`Verdict::Modify`].
///
/// Unchanged input is [`Verdict::Allow`], so a no-op redact does not look
/// like a rewrite in the trace.
///
/// Replacement syntax is [`regex`]'s: `$1`, `$name`, `$$`.
///
/// [`regex`]: https://docs.rs/regex
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), wardline_core::GuardError> {
/// use wardline_core::{Context, Guard, Verdict};
/// use wardline_guards::RegexRedactGuard;
///
/// let guard = RegexRedactGuard::new(r"\d{3}-\d{2}-\d{4}", "[SSN]")?;
/// let ctx = Context::new();
///
/// assert_eq!(
///     guard.check("ssn 123-45-6789 leaked", &ctx),
///     Ok(Verdict::Modify("ssn [SSN] leaked".to_owned()))
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct RegexRedactGuard {
    pattern: Regex,
    replacement: String,
    name: &'static str,
}

impl RegexRedactGuard {
    /// Builds a redact guard. Fails if the pattern does not compile.
    pub fn new(pattern: &str, replacement: impl Into<String>) -> Result<Self, GuardError> {
        Ok(RegexRedactGuard {
            pattern: compile(pattern)?,
            replacement: replacement.into(),
            name: "regex_redact",
        })
    }

    /// Replaces the default name (`regex_redact`). Builder-style.
    #[must_use]
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    /// The compiled pattern.
    pub fn pattern(&self) -> &Regex {
        &self.pattern
    }

    /// The replacement string passed to [`Regex::replace_all`].
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

impl Guard for RegexRedactGuard {
    type Input = str;
    type Output = String;

    fn check(&self, input: &str, _ctx: &Context) -> Result<Verdict<String>, GuardError> {
        let rewritten = self.pattern.replace_all(input, self.replacement.as_str());
        if rewritten.as_ref() == input {
            Ok(Verdict::Allow)
        } else {
            Ok(Verdict::Modify(rewritten.into_owned()))
        }
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wardline_core::FailPolicy;

    fn assert_fail_closed<G: Guard>(guard: &G) {
        assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
        assert_eq!(guard.timeout(), None);
        assert!(!guard.strict());
    }

    fn ctx() -> Context {
        Context::new()
    }

    #[test]
    fn regex_block_allows_when_the_pattern_is_absent() {
        let Ok(guard) = RegexBlockGuard::new(r"banned") else {
            panic!("static pattern must compile");
        };
        assert_eq!(guard.check("perfectly fine", &ctx()), Ok(Verdict::Allow));
        assert_fail_closed(&guard);
        assert_eq!(guard.name(), "regex_block");
    }

    #[test]
    fn regex_block_refuses_on_a_match() {
        let Ok(guard) = RegexBlockGuard::new(r"sk-[A-Za-z0-9]+")
            .map(|g| g.with_reason("api key in prompt").with_name("no_api_keys"))
        else {
            panic!("static pattern must compile");
        };
        let verdict = guard.check("export OPENAI_KEY=sk-abc", &ctx());
        assert_eq!(verdict, Ok(Verdict::block("api key in prompt")));
        assert_eq!(guard.name(), "no_api_keys");
    }

    #[test]
    fn regex_block_rejects_a_bad_pattern() {
        let error = match RegexBlockGuard::new("(") {
            Err(error) => error,
            Ok(_) => panic!("an open paren is not a valid regex"),
        };
        assert_eq!(error.kind(), "internal");
    }

    #[test]
    fn regex_redact_rewrites_matches_and_leaves_clean_input_alone() {
        let Ok(guard) = RegexRedactGuard::new(r"\d+", "[NUM]") else {
            panic!("static pattern must compile");
        };
        assert_eq!(guard.check("no digits", &ctx()), Ok(Verdict::Allow));
        assert_eq!(
            guard.check("room 12 and 3", &ctx()),
            Ok(Verdict::Modify("room [NUM] and [NUM]".to_owned()))
        );
        assert_fail_closed(&guard);
        assert_eq!(guard.name(), "regex_redact");
    }

    #[test]
    fn regex_redact_rejects_a_bad_pattern() {
        let error = match RegexRedactGuard::new("*", "x") {
            Err(error) => error,
            Ok(_) => panic!("a leading star is not a valid regex"),
        };
        assert_eq!(error.kind(), "internal");
    }
}
