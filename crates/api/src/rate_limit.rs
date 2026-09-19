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

use sha2::{Digest, Sha256};
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
/// per forged key would grow it without limit.
const MAX_TRACKED_KEYS: usize = 100_000;
/// How many entries one eviction pass may look at.
///
/// Eviction runs under the lock that every `/auth/*` request contends on,
/// so its cost has to be constant, not proportional to the table. Scanning
/// all 100_000 entries on every request with an untracked key -- which is
/// exactly what a forged-key flood produces -- would turn the component
/// built to stop amplification into the amplifier: each cheap request
/// buying a full scan plus up to that many key clones, with every other
/// auth request blocked behind it.
///
/// A sample rather than a true LRU. Choosing the best of a handful of
/// candidates keeps the table bounded, which is the actual requirement;
/// evicting the theoretically ideal entry is not worth an intrusive list
/// and its invariants.
const EVICTION_SAMPLE: usize = 16;

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
    /// A login's budget, keyed on the SHA-256 of its lower-cased form.
    ///
    /// Lower-cased because the `users` index is case-insensitive, so
    /// `Alice` and `alice` are one account and must share one budget.
    /// Hashed because the key is attacker-chosen and is retained: the
    /// limiter is consulted before the login's length is validated, so
    /// storing the string itself would let a caller park up to a body's
    /// worth of bytes per bucket. `MAX_TRACKED_KEYS` bounds the number of
    /// entries, not their size, so without this the table's ceiling is
    /// counted in gigabytes rather than entries.
    Login([u8; 32]),
    /// One invitation's probe budget, keyed on the token's SHA-256 and
    /// never on the token itself.
    ///
    /// Registration answers "that login is taken" with `409` and anything
    /// else with `403`, which a holder of one valid invitation can use as
    /// an unlimited login-existence oracle: a failed probe does not
    /// consume the invitation. Collapsing the two responses would hide the
    /// one thing an honest user needs to be told, so the probe is bounded
    /// instead. The address budget alone does not close this -- an
    /// attacker with many addresses and one invitation would still get
    /// unlimited attempts.
    Invitation([u8; 32]),
    /// Used when the peer address is unavailable, which happens in tests
    /// driving the router directly. One shared bucket still bounds total
    /// work; it simply cannot tell callers apart.
    Anonymous,
}

impl RateLimitKey {
    pub fn login(value: &str) -> Self {
        Self::Login(Sha256::digest(value.to_lowercase().as_bytes()).into())
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

/// Drops one bucket, looking at a bounded number of candidates.
///
/// A settled bucket -- one whose schedule has caught up with the present
/// -- carries no information and is dropped on sight. Otherwise the
/// candidate closest to settling goes, since it is the one whose history
/// is worth least. Both decisions are made over at most
/// `EVICTION_SAMPLE` entries, so the work per call does not grow with the
/// table.
fn evict(buckets: &mut HashMap<RateLimitKey, Bucket>, now: Instant) {
    let mut best: Option<(RateLimitKey, Instant)> = None;

    for (key, bucket) in buckets.iter().take(EVICTION_SAMPLE) {
        if bucket.tat <= now {
            // Settled: nothing to weigh, take it.
            best = Some((key.clone(), bucket.tat));
            break;
        }
        if best.as_ref().is_none_or(|(_, tat)| bucket.tat < *tat) {
            best = Some((key.clone(), bucket.tat));
        }
    }

    if let Some((key, _)) = best {
        buckets.remove(&key);
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

    /// The key must be fixed-width whatever the caller sends, because the
    /// limiter runs before the login's length is validated and the key is
    /// retained afterwards.
    #[test]
    fn a_login_key_is_fixed_width_however_long_the_login() {
        let RateLimitKey::Login(short) = RateLimitKey::login("alice") else {
            panic!("a login key")
        };
        let RateLimitKey::Login(enormous) = RateLimitKey::login(&"a".repeat(250_000)) else {
            panic!("a login key")
        };
        assert_eq!(short.len(), enormous.len());
        assert_ne!(short, enormous, "different logins must not collide");
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

    /// Eviction runs under the lock every auth request contends on, so
    /// its cost must not grow with the table. A full scan per request was
    /// amplification inside the anti-amplification component.
    #[test]
    fn eviction_looks_at_a_bounded_number_of_entries() {
        let mut buckets = HashMap::new();
        let now = Instant::now();
        for index in 0..10_000 {
            buckets.insert(
                RateLimitKey::login(&format!("key-{index}")),
                Bucket {
                    // All unsettled, so the cheap "drop a settled one"
                    // path cannot be what keeps this bounded.
                    tat: now + Duration::from_secs(60 + index as u64 % 97),
                },
            );
        }

        let before = buckets.len();
        evict(&mut buckets, now);
        assert_eq!(
            buckets.len(),
            before - 1,
            "one eviction removes exactly one entry"
        );
    }

    #[test]
    fn the_table_stays_bounded_under_forged_keys() {
        let limiter = limiter(1, 60);
        let start = Instant::now();
        // Each key is used once and immediately carries debt, so no
        // bucket is settled -- this exercises the sampled least-settled
        // path rather than the cheap one.
        for index in 0..(MAX_TRACKED_KEYS + 500) {
            let key = RateLimitKey::login(&format!("forged-{index}"));
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
