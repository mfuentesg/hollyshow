//! Pairing: a 6-digit PIN and a QR token, both redeemed for a session token.
//!
//! The host creates an *offer* for a role. An offer is **one-time use**: the first successful
//! redemption, by PIN or by QR token, consumes the whole offer (the other credential stops
//! working too), so a code that leaked over someone's shoulder or in a photo of the screen is
//! worthless once the intended phone has paired. Offers also expire (2 minutes by default).
//! Guessing is throttled three ways: a per-address lockout (5 wrong credentials lock that
//! address for 5 minutes), a global lockout that stops a botnet-style sweep of the 10^6 PIN
//! space, and the short life of offers.

use crate::util::{ct_eq, random_below, random_hex};
use hollyshow_core::Role;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const GLOBAL_WINDOW: Duration = Duration::from_secs(60);
const GLOBAL_MAX_FAILURES: u32 = 30;
const GLOBAL_LOCK: Duration = Duration::from_secs(60);
/// Wrong credentials from one address only count together inside this window.
const IP_WINDOW: Duration = Duration::from_secs(5 * 60);
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
    /// Milliseconds; adjustable while running (the offers already made keep their deadline).
    ttl_ms: AtomicU64,
    max_failures_per_ip: u32,
    lockout: Duration,
}

impl Pairing {
    pub fn new(ttl: Duration, max_failures_per_ip: u32, lockout: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            ttl_ms: AtomicU64::new(millis(ttl)),
            max_failures_per_ip: max_failures_per_ip.max(1),
            lockout,
        }
    }

    /// Lifetime of offers created from now on.
    pub fn set_ttl(&self, ttl: Duration) {
        self.ttl_ms.store(millis(ttl), Ordering::Relaxed);
    }

    pub fn ttl(&self) -> Duration {
        Duration::from_millis(self.ttl_ms.load(Ordering::Relaxed))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn create_offer(&self, role: Role) -> Result<Offer, PairError> {
        self.create_offer_at(role, Instant::now())
    }

    pub fn create_offer_at(&self, role: Role, now: Instant) -> Result<Offer, PairError> {
        let ttl = self.ttl();
        let mut inner = self.lock();
        inner.offers.retain(|o| o.expires > now);
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
            expires: now + ttl,
        });
        Ok(Offer {
            pin,
            token,
            role,
            expires_in: ttl,
        })
    }

    /// Invalidates every open offer (the operator pressed "cancel", or the app is locked down).
    pub fn clear_offers(&self) {
        self.lock().offers.clear();
    }

    /// Whether the offer with this QR token can still be redeemed (not spent, not expired).
    /// The host uses it to replace the code on screen as soon as a phone has used it.
    pub fn is_live(&self, token: &str) -> bool {
        self.is_live_at(token, Instant::now())
    }

    pub fn is_live_at(&self, token: &str, now: Instant) -> bool {
        self.lock()
            .offers
            .iter()
            .any(|o| o.expires > now && ct_eq(&o.token, token))
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

        let hit = inner.offers.iter().position(|o| match cred {
            Credential::Pin(p) => ct_eq(&o.pin, p.trim()),
            Credential::Token(t) => ct_eq(&o.token, t.trim()),
        });
        if let Some(i) = hit {
            // One-time use: the offer is gone for both the PIN and the QR token.
            let role = inner.offers.remove(i).role;
            inner.per_ip.remove(&ip);
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

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
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
        Pairing::new(Duration::from_secs(120), 5, Duration::from_secs(300))
    }
    fn ip(n: u8) -> IpAddr {
        IpAddr::from([192, 168, 1, n])
    }
    fn wrong_pin(o: &Offer) -> &'static str {
        if o.pin == "000000" {
            "000001"
        } else {
            "000000"
        }
    }

    #[test]
    fn pin_is_six_digits_and_works_exactly_once() {
        let p = pairing();
        let o = p.create_offer(Role::Musician).unwrap();
        assert_eq!(o.pin.len(), 6);
        assert!(o.pin.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(
            p.redeem(Credential::Pin(&format!(" {} ", o.pin)), ip(1)),
            Ok(Role::Musician)
        );
        assert_eq!(
            p.redeem(Credential::Pin(&o.pin), ip(2)),
            Err(PairError::Invalid)
        );
    }

    #[test]
    fn qr_token_is_one_time_and_spends_the_pin_too() {
        let p = pairing();
        let o = p.create_offer(Role::Pastor).unwrap();
        assert!(p.is_live(&o.token));
        assert_eq!(
            p.redeem(Credential::Token(&o.token), ip(1)),
            Ok(Role::Pastor)
        );
        assert!(!p.is_live(&o.token));
        assert_eq!(
            p.redeem(Credential::Token(&o.token), ip(1)),
            Err(PairError::Invalid)
        );
        assert_eq!(
            p.redeem(Credential::Pin(&o.pin), ip(1)),
            Err(PairError::Invalid)
        );
    }

    #[test]
    fn redeeming_one_offer_leaves_the_others_alone() {
        let p = pairing();
        let a = p.create_offer(Role::Stage).unwrap();
        let b = p.create_offer(Role::Operator).unwrap();
        assert_eq!(p.redeem(Credential::Pin(&a.pin), ip(1)), Ok(Role::Stage));
        assert_eq!(p.redeem(Credential::Pin(&b.pin), ip(1)), Ok(Role::Operator));
    }

    #[test]
    fn the_lifetime_can_change_for_new_offers() {
        let p = pairing();
        let old = p.create_offer(Role::Stage).unwrap();
        p.set_ttl(Duration::from_secs(600));
        let new = p.create_offer(Role::Stage).unwrap();
        assert_eq!(old.expires_in, Duration::from_secs(120));
        assert_eq!(new.expires_in, Duration::from_secs(600));
    }

    #[test]
    fn offers_expire() {
        let p = pairing();
        let t0 = Instant::now();
        let o = p.create_offer_at(Role::Operator, t0).unwrap();
        assert_eq!(o.expires_in, Duration::from_secs(120));
        assert!(p.is_live_at(&o.token, t0 + Duration::from_secs(119)));
        let late = t0 + Duration::from_secs(121);
        assert!(!p.is_live_at(&o.token, late));
        assert_eq!(
            p.redeem_at(Credential::Pin(&o.pin), ip(1), late),
            Err(PairError::Invalid)
        );
        assert_eq!(
            p.redeem_at(Credential::Token(&o.token), ip(1), late),
            Err(PairError::Invalid)
        );
    }

    #[test]
    fn an_address_is_locked_out_after_five_wrong_pins_for_five_minutes() {
        let p = pairing();
        let t0 = Instant::now();
        let o = p.create_offer_at(Role::Operator, t0).unwrap();
        for i in 0..5 {
            assert_eq!(
                p.redeem_at(
                    Credential::Pin(wrong_pin(&o)),
                    ip(9),
                    t0 + Duration::from_secs(i)
                ),
                Err(PairError::Invalid)
            );
        }
        // Even the right PIN is refused while locked, and the offer is not spent by it...
        let still = t0 + Duration::from_secs(60);
        assert!(matches!(
            p.redeem_at(Credential::Pin(&o.pin), ip(9), still),
            Err(PairError::Locked { retry_secs }) if (200..=300).contains(&retry_secs)
        ));
        // ...other addresses are unaffected...
        let o2 = p.create_offer_at(Role::Operator, still).unwrap();
        assert_eq!(
            p.redeem_at(Credential::Pin(&o2.pin), ip(10), still),
            Ok(Role::Operator)
        );
        // ...and the lock ends after 5 minutes.
        let after = t0 + Duration::from_secs(4 + 301);
        let fresh = p.create_offer_at(Role::Operator, after).unwrap();
        assert_eq!(
            p.redeem_at(Credential::Pin(&fresh.pin), ip(9), after),
            Ok(Role::Operator)
        );
    }

    #[test]
    fn four_wrong_pins_do_not_lock() {
        let p = pairing();
        let t0 = Instant::now();
        let o = p.create_offer_at(Role::Operator, t0).unwrap();
        for _ in 0..4 {
            let _ = p.redeem_at(Credential::Pin(wrong_pin(&o)), ip(9), t0);
        }
        assert_eq!(
            p.redeem_at(Credential::Pin(&o.pin), ip(9), t0),
            Ok(Role::Operator)
        );
    }

    #[test]
    fn a_distributed_sweep_trips_the_global_lock() {
        let p = Pairing::new(Duration::from_secs(120), 1000, Duration::from_secs(300));
        let t0 = Instant::now();
        let o = p.create_offer_at(Role::Operator, t0).unwrap();
        for n in 0..GLOBAL_MAX_FAILURES {
            let _ = p.redeem_at(
                Credential::Pin(wrong_pin(&o)),
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
