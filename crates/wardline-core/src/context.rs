//! Per-request state handed to every guard in a pipeline.

use crate::Deadline;
use std::collections::HashMap;

/// A typed metadata value carried in a [`Context`].
///
/// A tiny closed set rather than a general JSON value, because
/// `wardline-core` depends on nothing outside `std`. For request facts a
/// guard keys off — tenant, tier, retry count — not whole request bodies;
/// those are the guard's `Input`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Text: identifiers, paths, model names.
    Str(String),
    /// A signed integer: counts, sizes, epoch seconds.
    Int(i64),
    /// A floating-point number: scores, ratios.
    Float(f64),
    /// A flag.
    Bool(bool),
}

impl Value {
    /// The text, or `None` if this is not a [`Value::Str`].
    ///
    /// Accessors never coerce: a guard that asked for a string and found an
    /// integer has a bug upstream, and formatting it would hide that.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The integer, or `None` if this is not a [`Value::Int`].
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(number) => Some(*number),
            _ => None,
        }
    }

    /// The float, or `None` if this is not a [`Value::Float`].
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(number) => Some(*number),
            _ => None,
        }
    }

    /// The boolean, or `None` if this is not a [`Value::Bool`].
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(flag) => Some(*flag),
            _ => None,
        }
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::Str(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::Str(value.to_owned())
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Int(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

/// Everything a guard knows about the request beyond the input itself.
///
/// Built once per request, borrowed immutably by every guard, dropped when
/// the request ends. Guards cannot mutate it — a guard communicates through
/// its [`Verdict`], which lands in the audit trace, not by leaving state
/// behind for the next guard.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use wardline_core::{Context, Deadline};
///
/// let ctx = Context::new()
///     .with_deadline(Deadline::after(Duration::from_millis(50)))
///     .with("tenant", "acme-corp")
///     .with("retry_count", 2i64);
///
/// assert_eq!(ctx.get_str("tenant"), Some("acme-corp"));
/// assert_eq!(ctx.get_int("retry_count"), Some(2));
/// assert!(!ctx.is_expired());
/// ```
///
/// [`Verdict`]: crate::Verdict
#[derive(Debug, Clone, Default)]
pub struct Context {
    metadata: HashMap<String, Value>,
    deadline: Option<Deadline>,
}

impl Context {
    /// Creates an empty context with no deadline.
    pub fn new() -> Self {
        Context::default()
    }

    /// Sets the request deadline. Builder-style.
    #[must_use]
    pub fn with_deadline(mut self, deadline: Deadline) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Adds a metadata entry, replacing any existing one. Builder-style.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.insert(key, value);
        self
    }

    /// Adds a metadata entry, returning the value it replaced.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        self.metadata.insert(key.into(), value.into())
    }

    /// Sets or clears the deadline after construction.
    pub fn set_deadline(&mut self, deadline: Option<Deadline>) {
        self.deadline = deadline;
    }

    /// Looks up a metadata value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    /// Looks up text. `None` if absent *or* a different type — see
    /// [`Value::as_str`].
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// Looks up an integer.
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(Value::as_int)
    }

    /// Looks up a float.
    pub fn get_float(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(Value::as_float)
    }

    /// Looks up a boolean.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(Value::as_bool)
    }

    /// Returns `true` if `key` has a value of any type.
    pub fn contains(&self, key: &str) -> bool {
        self.metadata.contains_key(key)
    }

    /// Iterates over all metadata entries in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.metadata
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    /// The number of metadata entries.
    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    /// Returns `true` if there is no metadata. Says nothing about the
    /// deadline.
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    /// The request deadline, if one was set.
    ///
    /// `None` means no time bound was declared, not that time is unlimited —
    /// a guard's own [`Guard::timeout`] still applies.
    ///
    /// [`Guard::timeout`]: crate::Guard::timeout
    pub fn deadline(&self) -> Option<Deadline> {
        self.deadline
    }

    /// Returns `true` if a deadline was set and has passed. A context with no
    /// deadline never expires.
    pub fn is_expired(&self) -> bool {
        self.deadline.is_some_and(|deadline| deadline.is_expired())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn builder_round_trips_every_value_type() {
        let ctx = Context::new()
            .with("tenant", "acme")
            .with("retries", 3i64)
            .with("score", 0.5f64)
            .with("trusted", true);

        assert_eq!(ctx.get_str("tenant"), Some("acme"));
        assert_eq!(ctx.get_int("retries"), Some(3));
        assert_eq!(ctx.get_float("score"), Some(0.5));
        assert_eq!(ctx.get_bool("trusted"), Some(true));
        assert_eq!(ctx.len(), 4);
        assert!(!ctx.is_empty());
    }

    #[test]
    fn typed_lookups_do_not_coerce_across_types() {
        let ctx = Context::new().with("retries", 3i64);
        assert_eq!(ctx.get_int("retries"), Some(3));
        assert_eq!(ctx.get_str("retries"), None);
        assert_eq!(ctx.get_bool("retries"), None);
        assert!(ctx.contains("retries"));
    }

    #[test]
    fn a_missing_key_is_none_everywhere() {
        let ctx = Context::new();
        assert!(ctx.is_empty());
        assert_eq!(ctx.get("absent"), None);
        assert_eq!(ctx.get_str("absent"), None);
        assert_eq!(ctx.get_int("absent"), None);
        assert!(!ctx.contains("absent"));
    }

    #[test]
    fn insert_replaces_and_returns_the_old_value() {
        let mut ctx = Context::new().with("tier", "free");
        let previous = ctx.insert("tier", "paid");
        assert_eq!(previous, Some(Value::Str("free".to_owned())));
        assert_eq!(ctx.get_str("tier"), Some("paid"));
        assert_eq!(ctx.len(), 1);
    }

    #[test]
    fn a_context_without_a_deadline_never_expires() {
        let ctx = Context::new();
        assert_eq!(ctx.deadline(), None);
        assert!(!ctx.is_expired());
    }

    #[test]
    fn expiry_tracks_the_deadline() {
        let live = Context::new().with_deadline(Deadline::after(Duration::from_secs(60)));
        assert!(!live.is_expired());

        let dead = Context::new().with_deadline(Deadline::after(Duration::ZERO));
        assert!(dead.is_expired());
    }

    #[test]
    fn set_deadline_can_clear_an_existing_one() {
        let mut ctx = Context::new().with_deadline(Deadline::after(Duration::ZERO));
        assert!(ctx.is_expired());
        ctx.set_deadline(None);
        assert!(!ctx.is_expired());
        assert_eq!(ctx.deadline(), None);
    }

    #[test]
    fn iter_yields_every_entry() {
        let ctx = Context::new().with("a", 1i64).with("b", 2i64);
        let mut entries: Vec<_> = ctx
            .iter()
            .map(|(key, value)| (key, value.clone()))
            .collect();
        entries.sort_by_key(|(key, _)| *key);
        assert_eq!(entries, vec![("a", Value::Int(1)), ("b", Value::Int(2))]);
    }
}
