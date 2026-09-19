//! A token-bucket rate limiter for the endpoints that cost real work.
//!
//! `/auth/register` and `/auth/login` each derive an Argon2id hash, which
//! is deliberately expensive: roughly 19 MiB and tens of milliseconds, and
//! only a bounded number may run at once. Without a limiter an
//! unauthenticated client can keep that bound saturated from a single
//! machine, and every legitimate login is then shed as `503`. The hashing
//! semaphore caps the damage; it does not stop the attempt, and it does
//! nothing at all against password guessing.
//!
//! In-process and in-memory on purpose. A shared store would be a second
//! network round trip in front of the cheapest requests, and a second
//! thing to be down. The cost is that each instance limits independently,
//! so a fleet of N instances admits N times the configured rate -- stated
//! here rather than discovered later, and the right answer when that
//! matters is a limiter at the edge, not a database call in this path.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// How many requests a key may make back to back before the sustained
/// rate applies.
const DEFAULT_BURST: u32 = 10;
/// How long one token takes to come back, so the sustained rate is one
/// request per this interval.
const DEFAULT_REFILL_INTERVAL: Duration = Duration::from_secs(6);
/// Upper bound on tracked keys.
///
/// The table is itself an attack surface: without a ceiling, one request
/// per forged key would grow it without limit. When it is full, idle
/// buckets are dropped first; if every bucket is active the least
/// recently used one goes, which costs that key its history rather than
/// costing the service its memory.
const MAX_TRACKED_KEYS: usize = 100_000;

#[derive(Debug, Clone, Copy)]
pub struct RateLimitConfig {
    pub burst: u32,
    pub refill_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            burst: DEFAULT_BURST,
            refill_interval: DEFAULT_REFILL_INTERVAL,
        }
    }
}

/// The identity a bucket is kept for.
///
/// Peer address and login are separate namespaces on purpose: limiting by
/// address alone lets one address spread guesses across many accounts, and
/// limiting by login alone lets one attacker with many addresses hammer
/// one account. Both are checked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RateLimitKey {
    Peer(IpAddr),
    /// Lower-cased, so `Alice` and `alice` share one budget -- the `users`
    /// index is case-insensitive, so treating them separately would be a
    /// free doubling.
    Login(String),
    /// Used when the peer address is unavailable, which happens in tests
    /// driving the router directly. One shared bucket still bounds total
    /// work; it simply cannot tell callers apart.
    Anonymous,
}

impl RateLimitKey {
    pub fn login(value: &str) -> Self {
        Self::Login(value.to_lowercase())
    }
}

/// When this key's next request would be "on schedule".
///
/// The limiter is a virtual-scheduling one (GCRA) rather than a token
/// counter, because it has to be exact. A counter that adds a fractional
/// refill on every call accumulates floating-point error: five sixths plus
/// one sixth is not reliably one, so a bucket can sit permanently a hair
/// below the threshold and refuse a caller who is owed a request. This
/// keeps a single instant per key and compares instants, which is integer
/// arithmetic all the way down and cannot drift.
#[derive(Debug, Clone, Copy)]
struct Bucket {
    /// The theoretical arrival time: the moment at which the requests
    /// admitted so far would have been evenly spaced. Debt, expressed as a
    /// point in the future.
    tat: Instant,
}

/// Cheaply cloneable; every clone shares one table.
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<Mutex<HashMap<RateLimitKey, Bucket>>>,
}

/// How long the caller should wait before the next attempt can succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryAfter(pub Duration);

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Admits one request for `key`, or reports how long until one can be
    /// admitted.
    pub fn check(&self, key: &RateLimitKey) -> Result<(), RetryAfter> {
        self.check_at(key, Instant::now())
    }

    /// `check`, with the clock supplied, so refill can be tested without
    /// sleeping. A test that sleeps for its assertions is a test that
    /// eventually fails on a loaded machine for reasons unrelated to the
    /// behaviour it claims to check.
    fn check_at(&self, key: &RateLimitKey, now: Instant) -> Result<(), RetryAfter> {
        let emission = self.config.refill_interval;
        // How far ahead of `now` the schedule may run before a request is
        // refused. One emission interval per burst slot beyond the first.
        let tolerance = emission.saturating_mul(self.config.burst.saturating_sub(1));

        // A poisoned lock means another thread panicked while holding it.
        // This table is plain data with no invariant to corrupt, so
        // recovering beats turning one panic into a permanently
        // unavailable service.
        let mut buckets = self
            .buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if buckets.len() >= MAX_TRACKED_KEYS && !buckets.contains_key(key) {
            evict(&mut buckets, now);
        }

        let bucket = buckets.entry(key.clone()).or_insert(Bucket { tat: now });
        // An idle key's schedule catches up to the present rather than
        // banking credit: an hour of silence must not buy an hour of
        // requests.
        let scheduled = bucket.tat.max(now);
        let debt = scheduled.saturating_duration_since(now);

        if debt > tolerance {
            // `max(1s)` because `Retry-After` is expressed in whole
            // seconds: rounding down would invite an immediate retry that
            // is refused again.
            return Err(RetryAfter((debt - tolerance).max(Duration::from_secs(1))));
        }

        bucket.tat = scheduled + emission;
        Ok(())
    }
}

/// Drops the bucket that would be missed least: one carrying no debt
/// first, since it holds no information, and otherwise the one furthest
/// from the present.
fn evict(buckets: &mut HashMap<RateLimitKey, Bucket>, now: Instant) {
    let settled: Vec<RateLimitKey> = buckets
        .iter()
        .filter(|(_, bucket)| bucket.tat <= now)
        .map(|(key, _)| key.clone())
        .collect();

    if !settled.is_empty() {
        for key in settled {
            buckets.remove(&key);
        }
        return;
    }

    if let Some(stalest) = buckets
        .iter()
        .min_by_key(|(_, bucket)| bucket.tat)
        .map(|(key, _)| key.clone())
    {
        buckets.remove(&stalest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter(burst: u32, refill_secs: u64) -> RateLimiter {
        RateLimiter::new(RateLimitConfig {
            burst,
            refill_interval: Duration::from_secs(refill_secs),
        })
    }

    fn peer(last_octet: u8) -> RateLimitKey {
        RateLimitKey::Peer(IpAddr::from([10, 0, 0, last_octet]))
    }

    #[test]
    fn a_burst_is_allowed_and_then_refused() {
        let limiter = limiter(3, 6);
        let start = Instant::now();

        for attempt in 1..=3 {
            assert!(
                limiter.check_at(&peer(1), start).is_ok(),
                "attempt {attempt} is within the burst"
            );
        }
        let refused = limiter
            .check_at(&peer(1), start)
            .expect_err("the fourth is over budget");
        assert!(
            refused.0 >= Duration::from_secs(1),
            "a refusal must say when to come back, not just say no"
        );
    }

    #[test]
    fn tokens_come_back_at_the_configured_rate() {
        let limiter = limiter(2, 6);
        let start = Instant::now();
        assert!(limiter.check_at(&peer(2), start).is_ok());
        assert!(limiter.check_at(&peer(2), start).is_ok());
        assert!(limiter.check_at(&peer(2), start).is_err());

        // Not yet: five seconds is less than one refill interval.
        assert!(
            limiter
                .check_at(&peer(2), start + Duration::from_secs(5))
                .is_err()
        );
        // Now: one full interval has passed, so exactly one token exists.
        assert!(
            limiter
                .check_at(&peer(2), start + Duration::from_secs(6))
                .is_ok()
        );
        assert!(
            limiter
                .check_at(&peer(2), start + Duration::from_secs(6))
                .is_err(),
            "one interval buys one request, not a refilled burst"
        );
    }

    #[test]
    fn a_bucket_never_refills_past_its_burst() {
        let limiter = limiter(2, 1);
        let start = Instant::now();
        // An hour of idleness must not bank an hour of requests.
        let much_later = start + Duration::from_secs(3_600);
        assert!(limiter.check_at(&peer(3), much_later).is_ok());
        assert!(limiter.check_at(&peer(3), much_later).is_ok());
        assert!(
            limiter.check_at(&peer(3), much_later).is_err(),
            "idle time is capped at the burst, or a quiet account becomes a free flood"
        );
    }

    #[test]
    fn one_key_exhausting_its_budget_does_not_affect_another() {
        let limiter = limiter(1, 60);
        let start = Instant::now();
        assert!(limiter.check_at(&peer(4), start).is_ok());
        assert!(limiter.check_at(&peer(4), start).is_err());
        assert!(
            limiter.check_at(&peer(5), start).is_ok(),
            "a limiter that lets one caller lock out another is a denial-of-service tool"
        );
    }

    #[test]
    fn peer_and_login_budgets_are_separate_namespaces() {
        let limiter = limiter(1, 60);
        let start = Instant::now();
        assert!(limiter.check_at(&peer(6), start).is_ok());
        assert!(
            limiter
                .check_at(&RateLimitKey::login("alice"), start)
                .is_ok(),
            "spending the address budget must not spend the account's"
        );
        // ...and a login is case-folded, matching the unique index.
        assert!(
            limiter
                .check_at(&RateLimitKey::login("ALICE"), start)
                .is_err(),
            "Alice and alice are one account and must share one budget"
        );
    }

    #[test]
    fn the_table_stays_bounded_under_forged_keys() {
        let limiter = limiter(1, 60);
        let start = Instant::now();
        // Each key is used once and immediately carries debt, so no
        // bucket can be evicted as settled -- this exercises the
        // furthest-from-now path rather than the cheap one.
        for index in 0..(MAX_TRACKED_KEYS + 500) {
            let key = RateLimitKey::Login(format!("forged-{index}"));
            let _ = limiter.check_at(&key, start);
        }
        let tracked = limiter
            .buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        assert!(
            tracked <= MAX_TRACKED_KEYS,
            "the limiter's own table must not be an unbounded allocation: {tracked}"
        );
    }
}
