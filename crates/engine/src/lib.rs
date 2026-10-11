//! `hollyshow-engine`: thread-safe owner of the [`PresentationState`].
//!
//! It serialises commands, stamps the host clock, de-duplicates client retries per session and
//! broadcasts [`Update`]s. It owns the only concurrency primitive (tokio's broadcast channel) so
//! `hollyshow-core` stays a pure reducer.
//!
//! Threading: `dispatch` takes a short `std::sync::Mutex`; reducer work is microseconds and never
//! awaits, so it is safe to call from async tasks and from Tauri command threads alike. The
//! reducer works on a clone and the result is assigned only on success, so a panic inside it can
//! never leave a half-written state; a poisoned lock is therefore safe to recover from (the
//! release profile unwinds instead of aborting, see the workspace `Cargo.toml`).

use hollyshow_core::{
    apply, CommandId, CoreError, Ctx, EngineCommand, Event, PresentationState, SessionId,
    StateView, Update,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::broadcast;

/// Remembered command ids per session.
const DEDUP_WINDOW_PER_SESSION: usize = 128;
/// Sessions with a dedup window kept at once (least recently used is evicted).
const MAX_SESSIONS: usize = 256;
const CHANNEL_CAPACITY: usize = 256;

/// Source of wall-clock time. Injected so tests are deterministic.
pub trait Clock: Send + Sync {
    /// Unix epoch milliseconds.
    fn now_ms(&self) -> i64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }
}

/// Manually driven clock for tests.
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }
    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }
    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// What an accepted command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub version: u64,
    pub events: Vec<Event>,
}

/// Result of [`Engine::dispatch`]. Failures are cached too, so retrying a rejected command
/// returns the same error instead of re-evaluating against newer state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatched {
    pub result: Result<Outcome, CoreError>,
    /// The `(session, id)` pair was already processed and the stored result was replayed.
    pub duplicate: bool,
}

type Cached = Result<Outcome, CoreError>;

#[derive(Default)]
struct Window {
    last_used: u64,
    recent: VecDeque<(CommandId, Cached)>,
}

struct Inner {
    state: PresentationState,
    sessions: HashMap<SessionId, Window>,
    tick: u64,
}

pub struct Engine {
    inner: Mutex<Inner>,
    tx: broadcast::Sender<Update>,
    clock: Arc<dyn Clock>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(PresentationState::default(), Arc::new(SystemClock))
    }
}

impl Engine {
    pub fn new(state: PresentationState, clock: Arc<dyn Clock>) -> Self {
        Self::with_capacity(state, clock, CHANNEL_CAPACITY)
    }

    /// Like [`Engine::new`] with an explicit broadcast capacity (tests use a tiny one to
    /// exercise lag handling).
    pub fn with_capacity(state: PresentationState, clock: Arc<dyn Clock>, capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            inner: Mutex::new(Inner {
                state,
                sessions: HashMap::new(),
                tick: 0,
            }),
            tx,
            clock,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    /// Applies a command on behalf of `session`. When `(session, id)` was seen before, the
    /// original result is replayed and nothing is re-applied, which makes client retries after a
    /// dropped ack safe. Ids are scoped per session, so two phones that both count `c-1` never
    /// collide.
    pub fn dispatch(
        &self,
        session: &SessionId,
        id: Option<&CommandId>,
        cmd: EngineCommand,
    ) -> Dispatched {
        let mut inner = self.lock();
        inner.tick += 1;
        let tick = inner.tick;

        if let Some(id) = id {
            if let Some(w) = inner.sessions.get_mut(session) {
                w.last_used = tick;
                if let Some((_, cached)) = w.recent.iter().find(|(k, _)| k == id) {
                    return Dispatched {
                        result: cached.clone(),
                        duplicate: true,
                    };
                }
            }
        }

        let ctx = Ctx {
            now_ms: self.clock.now_ms(),
        };
        let result: Cached = apply(&inner.state, ctx, cmd).map(|(state, events)| {
            let changed = state.version != inner.state.version;
            inner.state = state;
            let outcome = Outcome {
                version: inner.state.version,
                events,
            };
            if changed {
                // No receivers is fine (e.g. before any window opens).
                let _ = self.tx.send(Update {
                    version: outcome.version,
                    view: inner.state.view(),
                    events: outcome.events.clone(),
                });
            }
            outcome
        });

        if let Some(id) = id {
            if !inner.sessions.contains_key(session) && inner.sessions.len() >= MAX_SESSIONS {
                if let Some(oldest) = inner
                    .sessions
                    .iter()
                    .min_by_key(|(_, w)| w.last_used)
                    .map(|(k, _)| k.clone())
                {
                    inner.sessions.remove(&oldest);
                }
            }
            let w = inner.sessions.entry(session.clone()).or_default();
            w.last_used = tick;
            if w.recent.len() == DEDUP_WINDOW_PER_SESSION {
                w.recent.pop_front();
            }
            w.recent.push_back((id.clone(), result.clone()));
        }
        Dispatched {
            result,
            duplicate: false,
        }
    }

    /// Forgets a session's dedup window (call when its socket closes for good).
    pub fn end_session(&self, session: &SessionId) {
        self.lock().sessions.remove(session);
    }

    pub fn snapshot(&self) -> PresentationState {
        self.lock().state.clone()
    }

    /// Reads the state without cloning it (use for projections and lookups).
    pub fn with_state<R>(&self, f: impl FnOnce(&PresentationState) -> R) -> R {
        f(&self.lock().state)
    }

    pub fn view(&self) -> StateView {
        self.lock().state.view()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.tx.subscribe()
    }

    /// Publishes host-originated events (library edits, playlist saves) that do not touch the
    /// presentation state, so clients can refresh their queries.
    pub fn publish_host_event(&self, event: Event) {
        let inner = self.lock();
        let _ = self.tx.send(Update {
            version: inner.state.version,
            view: inner.state.view(),
            events: vec![event],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{Command, ShowEntry, Slide, Timer, TimerKind};

    fn sid(s: &str) -> SessionId {
        SessionId::new(s)
    }

    fn engine() -> Engine {
        let e = Engine::default();
        e.dispatch(
            &sid("host"),
            None,
            EngineCommand::LoadShow {
                entries: vec![ShowEntry {
                    item_id: "a".into(),
                    title: "a".into(),
                    duration_secs: None,
                    slides: vec![Slide::blank("a:0"), Slide::blank("a:1")],
                }],
            },
        )
        .result
        .unwrap();
        e
    }

    fn next() -> EngineCommand {
        Command::Next {
            expected_version: None,
        }
        .into()
    }

    #[test]
    fn retried_command_id_is_not_applied_twice() {
        let e = engine();
        let (s, id) = (sid("phone"), CommandId::new("c-1"));
        let first = e.dispatch(&s, Some(&id), next());
        let again = e.dispatch(&s, Some(&id), next());
        assert!(!first.duplicate && again.duplicate);
        assert_eq!(first.result, again.result);
        assert_eq!(e.snapshot().live.unwrap().slide_index, 0);
    }

    #[test]
    fn same_command_id_from_two_sessions_does_not_collide() {
        let e = engine();
        let id = CommandId::new("c-1");
        let a = e.dispatch(&sid("A"), Some(&id), next());
        let b = e.dispatch(&sid("B"), Some(&id), next());
        assert!(!a.duplicate && !b.duplicate);
        assert_eq!(e.snapshot().live.unwrap().slide_index, 1);
    }

    #[test]
    fn rejected_commands_are_cached_and_stable_on_retry() {
        let e = engine();
        let (s, id) = (sid("A"), CommandId::new("c-9"));
        let stale: EngineCommand = Command::Next {
            expected_version: Some(999),
        }
        .into();
        let first = e.dispatch(&s, Some(&id), stale.clone());
        assert!(matches!(
            first.result,
            Err(CoreError::VersionMismatch { .. })
        ));
        let again = e.dispatch(&s, Some(&id), stale);
        assert!(again.duplicate);
        assert_eq!(first.result, again.result);
    }

    #[test]
    fn dedup_window_is_per_session_and_bounded() {
        let e = engine();
        let chatty = sid("chatty");
        for i in 0..(DEDUP_WINDOW_PER_SESSION + 10) {
            e.dispatch(
                &chatty,
                Some(&CommandId::new(format!("x{i}"))),
                Command::Blackout { on: i % 2 == 0 }.into(),
            );
        }
        let quiet = sid("quiet");
        let qid = CommandId::new("q");
        e.dispatch(&quiet, Some(&qid), Command::Logo { on: true }.into());
        for i in 0..(DEDUP_WINDOW_PER_SESSION * 2) {
            e.dispatch(
                &chatty,
                Some(&CommandId::new(format!("y{i}"))),
                Command::Clear { on: i % 2 == 0 }.into(),
            );
        }
        // A chatty client evicts only its own history.
        assert!(
            e.dispatch(&quiet, Some(&qid), Command::Logo { on: true }.into())
                .duplicate
        );
        assert!(
            !e.dispatch(&chatty, Some(&CommandId::new("x0")), next())
                .duplicate
        );
    }

    #[test]
    fn session_table_is_bounded() {
        let e = engine();
        for i in 0..(MAX_SESSIONS + 20) {
            e.dispatch(
                &sid(&format!("s{i}")),
                Some(&CommandId::new("c")),
                Command::Logo { on: i % 2 == 0 }.into(),
            );
        }
        assert!(e.lock().sessions.len() <= MAX_SESSIONS);
    }

    #[test]
    fn broadcasts_only_on_change() {
        let e = engine();
        let mut rx = e.subscribe();
        let s = sid("A");
        e.dispatch(&s, None, Command::Blackout { on: true }.into());
        e.dispatch(&s, None, Command::Blackout { on: true }.into());
        let u = rx.try_recv().unwrap();
        assert!(u.view.blackout);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn timers_use_the_host_clock_not_the_client() {
        let clock = Arc::new(ManualClock::new(10_000));
        let e = Engine::new(PresentationState::default(), clock.clone());
        let s = sid("phone");
        let t = Timer::new("svc", "Culto", TimerKind::Countdown, 60);
        e.dispatch(&s, None, Command::SetTimer { timer: t }.into())
            .result
            .unwrap();
        e.dispatch(&s, None, Command::StartTimer { id: "svc".into() }.into())
            .result
            .unwrap();
        clock.advance(5_000);
        let st = e.snapshot();
        assert_eq!(st.timers[0].started_at_ms, Some(10_000));
        assert_eq!(st.timers[0].remaining_at(clock.now_ms()), Some(55_000));
    }

    #[test]
    fn slow_subscriber_sees_lag_not_a_hang() {
        let e = Engine::with_capacity(PresentationState::default(), Arc::new(SystemClock), 2);
        let mut rx = e.subscribe();
        for i in 0..6 {
            e.dispatch(&sid("A"), None, Command::Blackout { on: i % 2 == 0 }.into());
        }
        assert!(matches!(
            rx.try_recv(),
            Err(broadcast::error::TryRecvError::Lagged(_))
        ));
    }
}
