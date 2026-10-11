//! Pairing: a 6-digit PIN and a one-time QR token, both redeemed for a session token.
//!
//! The host creates an *offer* for a role. The PIN can be typed on a phone and works for a few
//! devices before it is spent; the QR token works once. Both expire. Guessing is throttled three
//! ways: a per-address lockout, a global lockout that stops a botnet-style sweep of the 10^6 PIN
//! space, and the short life of offers.

use crate::util::{ct_eq, random_below, random_hex};
use hollyshow_core::Role;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const GLOBAL_WINDOW: Duration = Duration::from_secs(60);
const GLOBAL_MAX_FAILURES: u32 = 30;
const GLOBAL_LOCK: Duration = Duration::from_secs(60);
const IP_WINDOW: Duration = Duration::from_secs(60);
const MAX_OFFERS: usize = 16;

#[derive(Debug, Clone, Copy)]
pub enum Credential<'a> {
    Pin(&'a str),
    Token(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairError {
    /// Wrong, spent or expired credential.
    Invalid,
    /// Too many failures: try again in this many seconds.
    Locked { retry_secs: u64 },
    /// The OS could not provide randomness; refusing is safer than minting weak secrets.
    NoEntropy,
}

/// What the operator shows on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub pin: String,
    pub token: String,
    pub role: Role,
    pub expires_in: Duration,
}

struct Stored {
    role: Role,
    pin: String,
    token: String,
    expires: Instant,
    pin_uses_left: u32,
    token_used: bool,
}

#[derive(Default)]
struct Failures {
    count: u32,
    window_start: Option<Instant>,
    locked_until: Option<Instant>,
}

#[derive(Default)]
struct Inner {
    offers: Vec<Stored>,
    per_ip: HashMap<IpAddr, Failures>,
    global: Failures,
}

pub struct Pairing {
    inner: Mutex<Inner>,
    ttl: Duration,
    pin_max_uses: u32,
    max_failures_per_ip: u32,
    lockout: Duration,
}

impl Pairing {
    pub fn new(
        ttl: Duration,
        pin_max_uses: u32,
        max_failures_per_ip: u32,
        lockout: Duration,
    ) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            ttl,
            pin_max_uses: pin_max_uses.max(1),
            max_failures_per_ip: max_failures_per_ip.max(1),
            lockout,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn create_offer(&self, role: Role) -> Result<Offer, PairError> {
        self.create_offer_at(role, Instant::now())
    }

    pub fn create_offer_at(&self, role: Role, now: Instant) -> Result<Offer, PairError> {
        let mut inner = self.lock();
        inner
            .offers
            .retain(|o| o.expires > now && (o.pin_uses_left > 0 || !o.token_used));
        // Bounded: the oldest offers go first, so a stuck "new code" button cannot grow memory.
        while inner.offers.len() >= MAX_OFFERS {
            inner.offers.remove(0);
        }
        let mut pin = String::new();
        for _ in 0..16 {
            let candidate = format!(
                "{:06}",
                random_below(1_000_000).ok_or(PairError::NoEntropy)?
            );
            if !inner.offers.iter().any(|o| o.pin == candidate) {
                pin = candidate;
                break;
            }
        }
        if pin.is_empty() {
            return Err(PairError::NoEntropy);
        }
        let token = random_hex(16).ok_or(PairError::NoEntropy)?;
        inner.offers.push(Stored {
            role,
            pin: pin.clone(),
            token: token.clone(),
            expires: now + self.ttl,
            pin_uses_left: self.pin_max_uses,
            token_used: false,
        });
        Ok(Offer {
            pin,
            token,
            role,
            expires_in: self.ttl,
        })
    }

    /// Invalidates every open offer (the operator pressed "cancel", or the app is locked down).
    pub fn clear_offers(&self) {
        self.lock().offers.clear();
    }

    pub fn redeem(&self, cred: Credential<'_>, ip: IpAddr) -> Result<Role, PairError> {
        self.redeem_at(cred, ip, Instant::now())
    }

    pub fn redeem_at(
        &self,
        cred: Credential<'_>,
        ip: IpAddr,
        now: Instant,
    ) -> Result<Role, PairError> {
        let mut inner = self.lock();
        if let Some(retry) =
            locked(inner.per_ip.get(&ip), now).or_else(|| locked(Some(&inner.global), now))
        {
            return Err(PairError::Locked {
                retry_secs: retry.as_secs().max(1),
            });
        }
        inner.offers.retain(|o| o.expires > now);

        let hit = inner.offers.iter_mut().find(|o| match cred {
            Credential::Pin(p) => o.pin_uses_left > 0 && ct_eq(&o.pin, p.trim()),
            Credential::Token(t) => !o.token_used && ct_eq(&o.token, t.trim()),
        });
        if let Some(offer) = hit {
            match cred {
                Credential::Pin(_) => offer.pin_uses_left -= 1,
                Credential::Token(_) => offer.token_used = true,
            }
            let role = offer.role;
            inner.per_ip.remove(&ip);
            inner
                .offers
                .retain(|o| o.pin_uses_left > 0 || !o.token_used);
            return Ok(role);
        }

        let (per_ip_max, lockout) = (self.max_failures_per_ip, self.lockout);
        record_failure(
            inner.per_ip.entry(ip).or_default(),
            now,
            IP_WINDOW,
            per_ip_max,
            lockout,
        );
        record_failure(
            &mut inner.global,
            now,
            GLOBAL_WINDOW,
            GLOBAL_MAX_FAILURES,
            GLOBAL_LOCK,
        );
        if inner.per_ip.len() > 1024 {
            inner.per_ip.retain(|_, f| {
                f.locked_until.is_some_and(|u| u > now)
                    || f.window_start
                        .is_some_and(|w| now.duration_since(w) < IP_WINDOW)
            });
        }
        Err(PairError::Invalid)
    }
}

fn locked(f: Option<&Failures>, now: Instant) -> Option<Duration> {
    f.and_then(|f| f.locked_until)
        .filter(|u| *u > now)
        .map(|u| u - now)
}

fn record_failure(f: &mut Failures, now: Instant, window: Duration, max: u32, lock: Duration) {
    if f.window_start
        .is_none_or(|w| now.duration_since(w) > window)
    {
        f.window_start = Some(now);
        f.count = 0;
    }
    f.count += 1;
    if f.count >= max {
        f.locked_until = Some(now + lock);
        f.count = 0;
        f.window_start = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairing() -> Pairing {
        Pairing::new(Duration::from_secs(600), 2, 3, Duration::from_secs(300))
    }
    fn ip(n: u8) -> IpAddr {
        IpAddr::from([192, 168, 1, n])
    }

    #[test]
    fn pin_is_six_digits_and_works_a_limited_number_of_times() {
        let p = pairing();
        let o = p.create_offer(Role::Musician).unwrap();
        assert_eq!(o.pin.len(), 6);
        assert!(o.pin.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(p.redeem(Credential::Pin(&o.pin), ip(1)), Ok(Role::Musician));
        assert_eq!(
            p.redeem(Credential::Pin(&format!(" {} ", o.pin)), ip(2)),
            Ok(Role::Musician)
        );
        assert_eq!(
            p.redeem(Credential::Pin(&o.pin), ip(3)),
            Err(PairError::Invalid)
        );
    }

    #[test]
    fn qr_token_is_one_time_and_independent_of_the_pin() {
        let p = pairing();
        let o = p.create_offer(Role::Pastor).unwrap();
        assert_eq!(
            p.redeem(Credential::Token(&o.token), ip(1)),
            Ok(Role::Pastor)
        );
        assert_eq!(
            p.redeem(Credential::Token(&o.token), ip(1)),
            Err(PairError::Invalid)
        );
        assert_eq!(p.redeem(Credential::Pin(&o.pin), ip(1)), Ok(Role::Pastor));
    }

    #[test]
    fn offers_expire() {
        let p = pairing();
        let t0 = Instant::now();
        let o = p.create_offer_at(Role::Operator, t0).unwrap();
        let late = t0 + Duration::from_secs(601);
        assert_eq!(
            p.redeem_at(Credential::Pin(&o.pin), ip(1), late),
            Err(PairError::Invalid)
        );
    }

    #[test]
    fn an_address_is_locked_out_after_repeated_failures() {
        let p = pairing();
        let t0 = Instant::now();
        let o = p.create_offer_at(Role::Operator, t0).unwrap();
        let wrong = if o.pin == "000000" {
            "000001"
        } else {
            "000000"
        };
        for _ in 0..3 {
            assert_eq!(
                p.redeem_at(Credential::Pin(wrong), ip(9), t0),
                Err(PairError::Invalid)
            );
        }
        // Even the right PIN is refused while locked...
        assert!(matches!(
            p.redeem_at(Credential::Pin(&o.pin), ip(9), t0),
            Err(PairError::Locked { .. })
        ));
        // ...other addresses are unaffected, and the lock ends.
        assert_eq!(
            p.redeem_at(Credential::Pin(&o.pin), ip(10), t0),
            Ok(Role::Operator)
        );
        let o2 = p.create_offer_at(Role::Operator, t0).unwrap();
        let after = t0 + Duration::from_secs(301);
        assert_eq!(
            p.redeem_at(Credential::Pin(&o2.pin), ip(9), after),
            Ok(Role::Operator)
        );
    }

    #[test]
    fn a_distributed_sweep_trips_the_global_lock() {
        let p = Pairing::new(Duration::from_secs(600), 2, 1000, Duration::from_secs(300));
        let t0 = Instant::now();
        let o = p.create_offer_at(Role::Operator, t0).unwrap();
        let wrong = if o.pin == "000000" {
            "000001"
        } else {
            "000000"
        };
        for n in 0..GLOBAL_MAX_FAILURES {
            let _ = p.redeem_at(
                Credential::Pin(wrong),
                IpAddr::from([10, 0, 0, n as u8]),
                t0,
            );
        }
        assert!(matches!(
            p.redeem_at(Credential::Pin(&o.pin), ip(200), t0),
            Err(PairError::Locked { .. })
        ));
    }

    #[test]
    fn offers_are_bounded_and_pins_unique() {
        let p = pairing();
        let mut pins = std::collections::HashSet::new();
        for _ in 0..MAX_OFFERS {
            assert!(pins.insert(p.create_offer(Role::Stage).unwrap().pin));
        }
        for _ in 0..40 {
            p.create_offer(Role::Stage).unwrap();
        }
        assert!(p.lock().offers.len() <= MAX_OFFERS);
    }
}
