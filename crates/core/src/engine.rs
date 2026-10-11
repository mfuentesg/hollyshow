//! Thread-safe owner of the [`PresentationState`]: serialises commands, de-duplicates retries and
//! broadcasts [`Update`]s. Both the Tauri shell and the LAN server hold an `Arc<Engine>`.
//!
//! Threading: `dispatch` takes a short `std::sync::Mutex`; reducer work is microseconds and never
//! awaits, so it is safe to call from async tasks and from Tauri command threads alike.

use crate::command::Command;
use crate::error::CoreError;
use crate::event::{Event, Update};
use crate::ids::CommandId;
use crate::reducer::apply;
use crate::state::{PresentationState, StateView};
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::broadcast;

const DEDUP_WINDOW: usize = 512;
const CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub version: u64,
    pub events: Vec<Event>,
    /// True when `id` was already processed and the stored result was replayed.
    pub duplicate: bool,
}

struct Inner {
    state: PresentationState,
    recent: VecDeque<(CommandId, Outcome)>,
}

pub struct Engine {
    inner: Mutex<Inner>,
    tx: broadcast::Sender<Update>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(PresentationState::default())
    }
}

impl Engine {
    pub fn new(state: PresentationState) -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            inner: Mutex::new(Inner {
                state,
                recent: VecDeque::new(),
            }),
            tx,
        }
    }

    /// Applies a command. When `id` was seen before, the original outcome is returned and nothing
    /// is re-applied, which makes client retries after a dropped ack safe.
    pub fn dispatch(&self, id: Option<&CommandId>, cmd: Command) -> Result<Outcome, CoreError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(id) = id {
            if let Some((_, o)) = inner.recent.iter().find(|(k, _)| k == id) {
                return Ok(Outcome {
                    duplicate: true,
                    ..o.clone()
                });
            }
        }
        let (state, events) = apply(&inner.state, cmd)?;
        let changed = state.version != inner.state.version;
        inner.state = state;
        let outcome = Outcome {
            version: inner.state.version,
            events,
            duplicate: false,
        };
        if let Some(id) = id {
            if inner.recent.len() == DEDUP_WINDOW {
                inner.recent.pop_front();
            }
            inner.recent.push_back((id.clone(), outcome.clone()));
        }
        if changed {
            // No receivers is fine (e.g. before any window opens).
            let _ = self.tx.send(Update {
                version: outcome.version,
                view: inner.state.view(),
                events: outcome.events.clone(),
            });
        }
        Ok(outcome)
    }

    pub fn snapshot(&self) -> PresentationState {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .state
            .clone()
    }

    pub fn view(&self) -> StateView {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .state
            .view()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.tx.subscribe()
    }

    /// Publishes host-originated events (library edits, playlist saves) that do not touch the
    /// presentation state, so clients can refresh their queries.
    pub fn publish_host_event(&self, event: Event) {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
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
    use crate::state::{ShowEntry, Slide};

    fn engine() -> Engine {
        let e = Engine::default();
        e.dispatch(
            None,
            Command::LoadShow {
                entries: vec![ShowEntry {
                    item_id: "a".into(),
                    title: "a".into(),
                    duration_secs: None,
                    slides: vec![Slide::blank("a:0"), Slide::blank("a:1")],
                }],
            },
        )
        .unwrap();
        e
    }

    #[test]
    fn retried_command_id_is_not_applied_twice() {
        let e = engine();
        let id = CommandId::new("c-1");
        let first = e
            .dispatch(
                Some(&id),
                Command::Next {
                    expected_version: None,
                },
            )
            .unwrap();
        let again = e
            .dispatch(
                Some(&id),
                Command::Next {
                    expected_version: None,
                },
            )
            .unwrap();
        assert!(!first.duplicate && again.duplicate);
        assert_eq!(first.version, again.version);
        assert_eq!(e.snapshot().live.unwrap().slide_index, 0);
    }

    #[test]
    fn broadcasts_only_on_change() {
        let e = engine();
        let mut rx = e.subscribe();
        e.dispatch(None, Command::Blackout { on: true }).unwrap();
        e.dispatch(None, Command::Blackout { on: true }).unwrap();
        let u = rx.try_recv().unwrap();
        assert!(u.view.blackout);
        assert!(rx.try_recv().is_err());
    }
}
