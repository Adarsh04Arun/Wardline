//! In-process token-bucket rate limiter.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use wardline_core::{Context, Guard, GuardError, Verdict};

/// Default cap on distinct keys remembered at once.
///
/// A unique-key flood must not grow the map without bound — the same reason
/// the pipeline trace is capped. Oldest-idle buckets are evicted first.
pub const DEFAULT_MAX_KEYS: usize = 16_384;

/// One caller's remaining budget.
#[derive(Debug, Clone)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// In-memory token-bucket limiter, keyed by a string field on [`Context`].
///
/// This is a single-process limiter. It is not a distributed quota store —
/// that would be a network hop on the request path, which is a different
/// `Guard` a caller can write. See `docs/RESEARCH.md`.
///
/// The context value must be a [`wardline_core::Value::Str`]. A missing or
/// differently-typed key is a [`Verdict::Block`], not an error: the request
/// is not attributable, so it does not get a free pass.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), wardline_core::GuardError> {
/// use wardline_core::{Context, Guard, Verdict};
/// use wardline_guards::RateLimitGuard;
///
/// let guard = RateLimitGuard::new("tenant", 2, 0.0)?;
/// let ctx = Context::new().with("tenant", "acme");
///
/// assert_eq!(guard.check("ping", &ctx), Ok(Verdict::Allow));
/// assert_eq!(guard.check("ping", &ctx), Ok(Verdict::Allow));
/// assert!(guard.check("ping", &ctx).is_ok_and(|v| v.is_block()));
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct RateLimitGuard {
    context_key: String,
    capacity: f64,
    refill_per_second: f64,
    max_keys: usize,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimitGuard {
    /// Builds a limiter that starts each key with `capacity` tokens and
    /// refills at `refill_per_second`.
    ///
    /// `capacity == 0` refuses every attributable request. A refill of `0.0`
    /// is a one-shot burst with no recovery.
    pub fn new(
        context_key: impl Into<String>,
        capacity: u32,
        refill_per_second: f64,
    ) -> Result<Self, GuardError> {
        if !refill_per_second.is_finite() || refill_per_second < 0.0 {
            return Err(GuardError::internal(
                "refill_per_second must be a finite non-negative number",
            ));
        }
        Ok(RateLimitGuard {
            context_key: context_key.into(),
            capacity: f64::from(capacity),
            refill_per_second,
            max_keys: DEFAULT_MAX_KEYS,
            buckets: Mutex::new(HashMap::new()),
        })
    }

    /// Caps how many distinct keys are retained. Builder-style.
    #[must_use]
    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    /// The [`Context`] metadata key this guard reads.
    pub fn context_key(&self) -> &str {
        &self.context_key
    }

    /// Tokens granted to a new key, and the ceiling after refill.
    pub fn capacity(&self) -> f64 {
        self.capacity
    }

    fn refill(bucket: &mut Bucket, now: Instant, rate: f64, capacity: f64) {
        let elapsed = now.saturating_duration_since(bucket.last_refill);
        let added = elapsed.as_secs_f64() * rate;
        bucket.tokens = (bucket.tokens + added).min(capacity);
        bucket.last_refill = now;
    }

    fn evict_one(buckets: &mut HashMap<String, Bucket>) {
        let victim = buckets
            .iter()
            .max_by(|left, right| {
                left.1
                    .tokens
                    .partial_cmp(&right.1.tokens)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(key, _)| key.clone());
        if let Some(key) = victim {
            buckets.remove(&key);
        }
    }
}

impl Guard for RateLimitGuard {
    type Input = str;
    type Output = String;

    fn check(&self, _input: &str, ctx: &Context) -> Result<Verdict<String>, GuardError> {
        let Some(id) = ctx.get_str(&self.context_key) else {
            return Ok(Verdict::block(format!(
                "rate limit key '{}' missing from context",
                self.context_key
            )));
        };

        let now = Instant::now();
        let mut buckets = self
            .buckets
            .lock()
            .map_err(|_| GuardError::internal("rate limiter lock poisoned"))?;

        if !buckets.contains_key(id) {
            if self.max_keys == 0 {
                return Ok(Verdict::block("rate limiter has no key capacity"));
            }
            while buckets.len() >= self.max_keys {
                Self::evict_one(&mut buckets);
            }
            buckets.insert(
                id.to_owned(),
                Bucket {
                    tokens: self.capacity,
                    last_refill: now,
                },
            );
        }

        let Some(bucket) = buckets.get_mut(id) else {
            return Err(GuardError::internal(
                "rate limiter lost the bucket it just inserted",
            ));
        };
        Self::refill(bucket, now, self.refill_per_second, self.capacity);

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(Verdict::Allow)
        } else {
            Ok(Verdict::block(format!(
                "rate limit exceeded for {}",
                self.context_key
            )))
        }
    }

    fn name(&self) -> &'static str {
        "rate_limit"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wardline_core::FailPolicy;

    fn ctx(tenant: &str) -> Context {
        Context::new().with("tenant", tenant)
    }

    fn limiter(capacity: u32, refill: f64) -> RateLimitGuard {
        match RateLimitGuard::new("tenant", capacity, refill) {
            Ok(guard) => guard,
            Err(error) => panic!("valid limiter rejected: {error}"),
        }
    }

    #[test]
    fn allows_until_the_bucket_is_empty_then_blocks() {
        let guard = limiter(2, 0.0);
        assert_eq!(guard.check("a", &ctx("acme")), Ok(Verdict::Allow));
        assert_eq!(guard.check("a", &ctx("acme")), Ok(Verdict::Allow));
        assert!(guard.check("a", &ctx("acme")).is_ok_and(|v| v.is_block()));
        assert_eq!(guard.fail_policy(), FailPolicy::FailClosed);
        assert_eq!(guard.name(), "rate_limit");
    }

    #[test]
    fn keys_are_isolated() {
        let guard = limiter(1, 0.0);
        assert_eq!(guard.check("a", &ctx("acme")), Ok(Verdict::Allow));
        assert_eq!(guard.check("a", &ctx("globex")), Ok(Verdict::Allow));
        assert!(guard.check("a", &ctx("acme")).is_ok_and(|v| v.is_block()));
    }

    #[test]
    fn a_missing_key_is_a_block_not_a_free_pass() {
        let guard = limiter(8, 1.0);
        let verdict = guard.check("a", &Context::new());
        assert!(verdict.is_ok_and(|v| {
            v.block_reason()
                .is_some_and(|reason| reason.contains("missing"))
        }));
    }

    #[test]
    fn tokens_refill_over_time() {
        let guard = limiter(1, 50.0);
        assert_eq!(guard.check("a", &ctx("acme")), Ok(Verdict::Allow));
        assert!(guard.check("a", &ctx("acme")).is_ok_and(|v| v.is_block()));
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(guard.check("a", &ctx("acme")), Ok(Verdict::Allow));
    }

    #[test]
    fn rejects_a_nonsensical_refill_rate() {
        let error = match RateLimitGuard::new("tenant", 1, f64::NAN) {
            Err(error) => error,
            Ok(_) => panic!("NaN refill must be rejected"),
        };
        assert_eq!(error.kind(), "internal");
    }

    #[test]
    fn the_key_map_does_not_grow_past_max_keys() {
        let guard = limiter(1, 0.0).with_max_keys(2);
        assert_eq!(guard.check("a", &ctx("one")), Ok(Verdict::Allow));
        assert_eq!(guard.check("a", &ctx("two")), Ok(Verdict::Allow));
        // A third key evicts an idle (empty) bucket rather than growing.
        assert_eq!(guard.check("a", &ctx("three")), Ok(Verdict::Allow));
        let locked = match guard.buckets.lock() {
            Ok(map) => map.len(),
            Err(_) => panic!("lock poisoned"),
        };
        assert!(locked <= 2, "remembered {locked} keys");
    }
}
