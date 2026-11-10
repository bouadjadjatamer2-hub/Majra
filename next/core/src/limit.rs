//! One transfer budget shared by every connection of every running download.
//!
//! IDM's speed limiter is global: it caps the whole list, not each file. A token bucket
//! does that with one atomic load on the unlimited path, so the default setup pays
//! nothing for a feature it does not use.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// Bytes per second, `0` meaning "no limit".
pub struct SpeedLimiter {
    limit: AtomicU64,
    bucket: Mutex<Bucket>,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl SpeedLimiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            limit: AtomicU64::new(bytes_per_sec),
            bucket: Mutex::new(Bucket {
                tokens: 0.0,
                last: Instant::now(),
            }),
        }
    }

    pub fn unlimited() -> Self {
        Self::new(0)
    }

    pub fn set_limit(&self, bytes_per_sec: u64) {
        self.limit.store(bytes_per_sec, Ordering::Relaxed);
        let mut bucket = self.bucket.lock().unwrap();
        bucket.tokens = 0.0;
        bucket.last = Instant::now();
    }

    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    /// Wait until `bytes` may leave the network. Returns `false` when the download is
    /// cancelled while waiting, so a pause never sits in the throttle.
    pub async fn acquire(&self, bytes: usize, cancel: &CancellationToken) -> bool {
        let limit = self.limit.load(Ordering::Relaxed);
        if limit == 0 {
            return !cancel.is_cancelled();
        }
        let mut need = bytes as f64;
        loop {
            let wait = {
                let mut bucket = self.bucket.lock().unwrap();
                let now = Instant::now();
                let elapsed = now.duration_since(bucket.last).as_secs_f64();
                bucket.last = now;
                // A quarter-second burst keeps fast servers from stalling on the first
                // chunk while still averaging out to the configured rate.
                let capacity = (limit as f64 * 0.25).max(64.0 * 1024.0);
                bucket.tokens = (bucket.tokens + elapsed * limit as f64).min(capacity);
                if bucket.tokens >= need {
                    bucket.tokens -= need;
                    return !cancel.is_cancelled();
                }
                // Take what is there; other connections wait for their own share.
                need -= bucket.tokens;
                bucket.tokens = 0.0;
                Duration::from_secs_f64(need / limit as f64)
            };
            tokio::select! {
                _ = cancel.cancelled() => return false,
                _ = tokio::time::sleep(
                    wait.max(Duration::from_millis(1)).min(Duration::from_millis(100)),
                ) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_never_waits() {
        let limiter = SpeedLimiter::unlimited();
        let cancel = CancellationToken::new();
        let start = Instant::now();
        assert!(limiter.acquire(4 * 1024 * 1024, &cancel).await);
        assert!(start.elapsed() < Duration::from_millis(5));
    }

    #[tokio::test]
    async fn a_limited_budget_takes_time() {
        let limiter = SpeedLimiter::new(1_000_000);
        let cancel = CancellationToken::new();
        let start = Instant::now();
        assert!(limiter.acquire(250_000, &cancel).await);
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(200), "elapsed {elapsed:?}");
        assert!(elapsed < Duration::from_millis(700), "elapsed {elapsed:?}");
    }

    #[tokio::test]
    async fn cancelling_while_waiting_returns_false() {
        let limiter = SpeedLimiter::new(1_000_000);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(!limiter.acquire(250_000, &cancel).await);
    }
}
