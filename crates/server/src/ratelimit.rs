//! Token buckets: per client address for HTTP / handshakes, per connection for WebSocket frames.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Classic token bucket. `capacity` is the burst, `per_sec` the sustained rate.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    per_sec: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(capacity: u32, per_sec: f64) -> Self {
        Self::new_at(capacity, per_sec, Instant::now())
    }

    pub fn new_at(capacity: u32, per_sec: f64, now: Instant) -> Self {
        Self {
            capacity: f64::from(capacity),
            per_sec,
            tokens: f64::from(capacity),
            last: now,
        }
    }

    pub fn take(&mut self) -> bool {
        self.take_at(Instant::now())
    }

    pub fn take_at(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.per_sec).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn is_idle(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last) > Duration::from_secs(60)
    }
}

/// One bucket per client address. Entries that have been quiet for a minute are dropped when the
/// map grows, so a scan from many addresses cannot grow memory without bound.
pub struct IpLimiter {
    capacity: u32,
    per_sec: f64,
    buckets: Mutex<HashMap<IpAddr, TokenBucket>>,
}

const PRUNE_AT: usize = 2048;

impl IpLimiter {
    pub fn new(capacity: u32, per_sec: f64) -> Self {
        Self {
            capacity,
            per_sec,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn check(&self, ip: IpAddr) -> bool {
        self.check_at(ip, Instant::now())
    }

    pub fn check_at(&self, ip: IpAddr, now: Instant) -> bool {
        let mut map = self.buckets.lock().unwrap_or_else(|p| p.into_inner());
        if map.len() >= PRUNE_AT {
            map.retain(|_, b| !b.is_idle(now));
        }
        map.entry(ip)
            .or_insert_with(|| TokenBucket::new_at(self.capacity, self.per_sec, now))
            .take_at(now)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.buckets.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_burst_then_refills() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new_at(3, 1.0, t0);
        assert!(b.take_at(t0) && b.take_at(t0) && b.take_at(t0));
        assert!(!b.take_at(t0));
        assert!(!b.take_at(t0 + Duration::from_millis(500)));
        assert!(b.take_at(t0 + Duration::from_millis(1100)));
        // Never exceeds the burst however long it idles.
        let later = t0 + Duration::from_secs(3600);
        assert!(b.take_at(later) && b.take_at(later) && b.take_at(later));
        assert!(!b.take_at(later));
    }

    #[test]
    fn limiter_is_per_address_and_prunes() {
        let t0 = Instant::now();
        let lim = IpLimiter::new(1, 0.0);
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        assert!(lim.check_at(a, t0));
        assert!(!lim.check_at(a, t0));
        assert!(lim.check_at(b, t0));
        for i in 0..PRUNE_AT as u32 {
            lim.check_at(IpAddr::from(std::net::Ipv4Addr::from(0x0a01_0000 + i)), t0);
        }
        lim.check_at(a, t0 + Duration::from_secs(120));
        assert!(lim.len() < PRUNE_AT);
    }
}
