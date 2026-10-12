//! CCLI usage logging: dedupe rules plus a background writer.
//!
//! Going live must not wait for the database, so the song is queued here and a thread appends it
//! to the usage log. [`UsageDedupe`] decides *whether* a cue counts, as a pure function of the
//! song, the clock and the open service, so the rules are unit tested without threads.

use hollyshow_core::SongId;
use hollyshow_storage_api::Repos;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A song that was already counted is not counted again for this long, even if the operator goes
/// back to it. A different service (playlist) resets the window.
pub(crate) const REPEAT_WINDOW_MS: i64 = 30 * 60 * 1000;

/// Decides which "song went live" observations are a new CCLI use.
///
/// Rules, applied to every accepted command with the song now on the live output:
/// - moving between slides of the song that is already live never counts;
/// - a song counts once per presentation session: after being counted it is ignored for
///   [`REPEAT_WINDOW_MS`] (coming back to it after another item, or a double tap, is the same use);
/// - opening a different service starts a new day: the history is forgotten.
#[derive(Debug, Default)]
pub(crate) struct UsageDedupe {
    current: Option<SongId>,
    counted_at: HashMap<SongId, i64>,
}

impl UsageDedupe {
    /// Observes the song now live (`None` when live shows something that is not a song).
    /// Returns true when it must be logged.
    pub(crate) fn observe(&mut self, song: Option<&SongId>, now_ms: i64) -> bool {
        let Some(song) = song else {
            self.current = None;
            return false;
        };
        if self.current.as_ref() == Some(song) {
            return false;
        }
        self.current = Some(song.clone());
        let repeat = self
            .counted_at
            .get(song)
            .is_some_and(|&at| now_ms.saturating_sub(at) < REPEAT_WINDOW_MS);
        if repeat {
            return false;
        }
        self.counted_at.insert(song.clone(), now_ms);
        true
    }

    /// A new service was opened.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

pub(crate) struct UsageLogger {
    tx: Mutex<Sender<(SongId, i64)>>,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl UsageLogger {
    pub(crate) fn spawn(repos: Repos) -> Self {
        let (tx, rx) = channel::<(SongId, i64)>();
        let spawned = std::thread::Builder::new()
            .name("usage-logger".into())
            .spawn(move || {
                while let Ok((song, at)) = rx.recv() {
                    if let Err(e) = repos.songs.log_usage(&song, at) {
                        log::warn!("usage log: {e}");
                    }
                }
            });
        if let Err(e) = spawned {
            log::error!("could not start the usage logger: {e}");
        }
        Self { tx: Mutex::new(tx) }
    }

    pub(crate) fn submit(&self, song: SongId, at_ms: i64) {
        let tx = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        let _ = tx.send((song, at_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str) -> SongId {
        SongId::new(id)
    }

    #[test]
    fn first_time_counts_and_slide_moves_do_not() {
        let mut d = UsageDedupe::default();
        assert!(d.observe(Some(&s("a")), 0));
        assert!(!d.observe(Some(&s("a")), 1_000));
        // Still the same song long after the window: moving slides is not a new use.
        assert!(!d.observe(Some(&s("a")), REPEAT_WINDOW_MS * 3));
    }

    #[test]
    fn returning_within_the_window_is_the_same_use() {
        let mut d = UsageDedupe::default();
        assert!(d.observe(Some(&s("a")), 0));
        assert!(d.observe(Some(&s("b")), 10));
        assert!(!d.observe(Some(&s("a")), 20));
        // A non-song in between does not change that.
        assert!(!d.observe(None, 30));
        assert!(!d.observe(Some(&s("a")), 40));
    }

    #[test]
    fn counts_again_after_the_window() {
        let mut d = UsageDedupe::default();
        assert!(d.observe(Some(&s("a")), 0));
        assert!(!d.observe(None, 1));
        assert!(!d.observe(Some(&s("a")), REPEAT_WINDOW_MS - 1));
        assert!(!d.observe(None, REPEAT_WINDOW_MS));
        assert!(d.observe(Some(&s("a")), REPEAT_WINDOW_MS));
    }

    #[test]
    fn a_new_service_resets_the_history() {
        let mut d = UsageDedupe::default();
        assert!(d.observe(Some(&s("a")), 0));
        d.reset();
        assert!(d.observe(Some(&s("a")), 5));
    }

    #[test]
    fn non_songs_never_count() {
        let mut d = UsageDedupe::default();
        assert!(!d.observe(None, 0));
    }
}
