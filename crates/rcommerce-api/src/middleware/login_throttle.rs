//! Login throttling: per-source-IP attempt budgets and per-(IP, e-mail)
//! failure budgets, owned by `AppState`.
//!
//! Two layers, both in memory per process:
//! - Every login attempt counts against the source IP's attempt budget
//!   (`login_ip_max_attempts` per window). This bounds total brute-force
//!   volume regardless of which e-mails are probed.
//! - Every failed password counts against the (IP, canonical e-mail) pair's
//!   failure budget (`login_pair_max_failures` per window). A successful
//!   login clears only that pair.
//!
//! There is deliberately no account-only lock: an attacker can only lock
//! their own IP out of a given account, never the account's owner out of
//! their own (a pure per-account counter would be a trivial lockout vector).
//!
//! Check-and-increment is a single atomic call per request so concurrent
//! attempts cannot race past the budget. Both maps are hard-bounded with
//! expiry/eviction; the server schedules periodic [`LoginThrottle::cleanup`].

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rcommerce_core::config::RateLimitConfig;

/// Canonical e-mail for throttle keys: trimmed and lowercased so different
/// spellings of the same address share one budget.
pub fn canonical_email(email: &str) -> String {
    email.trim().to_lowercase()
}

struct WindowCounter {
    count: u32,
    window_start: Instant,
}

impl WindowCounter {
    fn bump(&mut self, window: Duration) {
        if self.window_start.elapsed() >= window {
            self.count = 0;
            self.window_start = Instant::now();
        }
        self.count += 1;
    }

    fn is_over(&self, max: u32, window: Duration) -> bool {
        self.window_start.elapsed() < window && self.count >= max
    }

    fn retry_after(&self, window: Duration) -> u64 {
        window
            .saturating_sub(self.window_start.elapsed())
            .as_secs()
            .max(1)
    }
}

/// Insert into `map`, evicting expired entries and then the oldest window
/// when the hard capacity is reached.
fn insert_bounded<K: Eq + std::hash::Hash + Clone>(
    map: &mut HashMap<K, WindowCounter>,
    key: K,
    window: Duration,
    max_entries: usize,
) {
    if map.len() >= max_entries && !map.contains_key(&key) {
        map.retain(|_, c| c.window_start.elapsed() < window);
        if map.len() >= max_entries {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, c)| c.window_start)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
        }
    }
    map.entry(key).or_insert(WindowCounter {
        count: 0,
        window_start: Instant::now(),
    });
}

/// AppState-owned login throttle. Clone shares the same underlying maps.
#[derive(Clone)]
pub struct LoginThrottle {
    window: Duration,
    ip_max_attempts: u32,
    pair_max_failures: u32,
    max_tracked: usize,
    ip_attempts: std::sync::Arc<Mutex<HashMap<IpAddr, WindowCounter>>>,
    pair_failures: std::sync::Arc<Mutex<HashMap<(IpAddr, String), WindowCounter>>>,
}

impl LoginThrottle {
    /// Build from the `[rate_limiting]` configuration.
    pub fn new(config: &RateLimitConfig) -> Self {
        Self::with_limits(
            config.login_ip_max_attempts,
            config.login_pair_max_failures,
            Duration::from_secs(config.login_window_secs.max(1)),
            config.max_tracked_ips.max(1),
        )
    }

    fn with_limits(
        ip_max_attempts: u32,
        pair_max_failures: u32,
        window: Duration,
        max_tracked: usize,
    ) -> Self {
        Self {
            window,
            ip_max_attempts,
            pair_max_failures,
            max_tracked,
            ip_attempts: std::sync::Arc::new(Mutex::new(HashMap::new())),
            pair_failures: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Atomically reserve capacity in both budgets for one login attempt.
    /// Pair attempts that fail remain in the map; successful authentication
    /// clears the pair. Reserving before password verification prevents a
    /// concurrent burst from passing the pair check before any failure is
    /// recorded.
    pub fn check_attempt(&self, ip: IpAddr, email: &str) -> Result<(), u64> {
        let mut attempts = self.ip_attempts.lock().unwrap_or_else(|e| e.into_inner());
        insert_bounded(&mut attempts, ip, self.window, self.max_tracked);
        let counter = attempts.get_mut(&ip).expect("just inserted");
        if counter.is_over(self.ip_max_attempts, self.window) {
            return Err(counter.retry_after(self.window));
        }
        counter.bump(self.window);
        drop(attempts);

        let key = (ip, canonical_email(email));
        let mut failures = self.pair_failures.lock().unwrap_or_else(|e| e.into_inner());
        insert_bounded(&mut failures, key.clone(), self.window, self.max_tracked);
        let counter = failures.get_mut(&key).expect("just inserted");
        if counter.is_over(self.pair_max_failures, self.window) {
            return Err(counter.retry_after(self.window));
        }
        counter.bump(self.window);
        Ok(())
    }

    /// Forget the failures of this (IP, e-mail) pair after a successful
    /// login. Only this pair is cleared: the IP attempt budget stays (it
    /// measures volume, not correctness) and other pairs are untouched.
    pub fn clear_pair(&self, ip: IpAddr, email: &str) {
        self.pair_failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(ip, canonical_email(email)));
    }

    /// Drop expired entries from both maps.
    pub fn cleanup(&self) {
        let window = self.window;
        self.ip_attempts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, c| c.window_start.elapsed() < window);
        self.pair_failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, c| c.window_start.elapsed() < window);
    }

    /// Number of IPs currently tracked (for tests/metrics).
    #[cfg(test)]
    fn tracked_ips(&self) -> usize {
        self.ip_attempts.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throttle(ip_max: u32, pair_max: u32) -> LoginThrottle {
        LoginThrottle::with_limits(ip_max, pair_max, Duration::from_secs(60), 1000)
    }

    const IP: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(198, 51, 100, 7));

    #[test]
    fn ip_attempt_budget_counts_every_attempt() {
        let t = throttle(3, 100);
        for _ in 0..3 {
            assert!(t.check_attempt(IP, "a@example.com").is_ok());
        }
        let retry = t.check_attempt(IP, "a@example.com").unwrap_err();
        assert!(retry > 0 && retry <= 60);
    }

    #[test]
    fn pair_failure_budget_locks_only_that_pair() {
        let t = throttle(1000, 2);
        t.check_attempt(IP, "victim@example.com").unwrap();
        t.check_attempt(IP, "victim@example.com").unwrap();
        // Same IP, other e-mail: unaffected (no account-only lock).
        assert!(t.check_attempt(IP, "other@example.com").is_ok());
        // The pair is now over budget.
        let retry = t.check_attempt(IP, "victim@example.com").unwrap_err();
        assert!(retry > 0);
        // Same e-mail from another IP: unaffected.
        let other_ip: IpAddr = "198.51.100.8".parse().unwrap();
        assert!(t.check_attempt(other_ip, "victim@example.com").is_ok());
    }

    #[test]
    fn success_clears_only_the_matching_pair() {
        let t = throttle(1000, 1);
        t.check_attempt(IP, "User@Example.com").unwrap();
        t.check_attempt(IP, "other@example.com").unwrap();
        // Over budget for the pair (case-insensitive key).
        assert!(t.check_attempt(IP, " user@example.com ").is_err());
        t.clear_pair(IP, "user@example.com");
        assert!(t.check_attempt(IP, "user@example.com").is_ok());
        // The other pair's failures remain.
        assert!(t.check_attempt(IP, "other@example.com").is_err());
    }

    #[test]
    fn state_is_hard_bounded() {
        let t = LoginThrottle::with_limits(1_000_000, 1_000_000, Duration::from_secs(60), 5);
        for i in 0..20 {
            let ip: IpAddr = format!("10.0.0.{}", i + 1).parse().unwrap();
            t.check_attempt(ip, "a@example.com").unwrap();
        }
        assert_eq!(t.tracked_ips(), 5);
        assert_eq!(t.pair_failures.lock().unwrap().len(), 5);
    }

    #[test]
    fn cleanup_drops_expired_entries() {
        let t = throttle(1, 100);
        let _ = t.check_attempt(IP, "a@example.com"); // budget exhausted (1 attempt)
        assert!(t.check_attempt(IP, "a@example.com").is_err());
        // Backdate the window so cleanup sees it as expired.
        {
            let mut attempts = t.ip_attempts.lock().unwrap();
            let c = attempts.get_mut(&IP).unwrap();
            c.window_start = Instant::now() - Duration::from_secs(120);
            c.count = 0;
        }
        t.cleanup();
        assert_eq!(t.tracked_ips(), 0);
        assert!(t.check_attempt(IP, "a@example.com").is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_attempts_cannot_exceed_the_budget() {
        let t = std::sync::Arc::new(throttle(50, 1000));
        let mut handles = Vec::new();
        for _ in 0..20 {
            let t = t.clone();
            handles.push(tokio::spawn(async move {
                let mut ok = 0;
                for _ in 0..10 {
                    if t.check_attempt(IP, "brute@example.com").is_ok() {
                        ok += 1;
                    }
                }
                ok
            }));
        }
        let total: usize = futures_sum(handles).await;
        // 20 tasks x 10 attempts race for a budget of 50; exactly 50 may pass.
        assert_eq!(total, 50);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_attempts_cannot_exceed_the_pair_budget() {
        let t = std::sync::Arc::new(throttle(1000, 5));
        let mut handles = Vec::new();
        for _ in 0..20 {
            let t = t.clone();
            handles.push(tokio::spawn(async move {
                let mut ok = 0;
                for _ in 0..10 {
                    if t.check_attempt(IP, "same@example.com").is_ok() {
                        ok += 1;
                    }
                }
                ok
            }));
        }
        assert_eq!(futures_sum(handles).await, 5);
    }

    async fn futures_sum(handles: Vec<tokio::task::JoinHandle<usize>>) -> usize {
        let mut total = 0;
        for h in handles {
            total += h.await.unwrap();
        }
        total
    }
}
