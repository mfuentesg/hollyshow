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
/// back to it. A new calendar day (local time) resets the window.
pub(crate) const REPEAT_WINDOW_MS: i64 = 30 * 60 * 1000;

/// Decides which "song went live" observations are a new CCLI use.
///
/// Rules, applied to every accepted command with the song now on the live output:
/// - moving between slides of the song that is already live never counts;
/// - a song counts once per presentation session: after being counted it is ignored for
///   [`REPEAT_WINDOW_MS`] (coming back to it after another item, or a double tap, is the same use);
/// - a new local calendar day (a new service day) forgets the history. Switching playlists does
///   not: the same song shown from another playlist within the window is the same use.
///
/// The history is seeded from the persisted usage log on start so a restart does not re-log.
#[derive(Debug, Default)]
pub(crate) struct UsageDedupe {
    current: Option<SongId>,
    counted_at: HashMap<SongId, i64>,
    day: Option<i64>,
}

impl UsageDedupe {
    /// Seeds the history with already persisted uses (`(song, shown_at_ms)`).
    pub(crate) fn seed(&mut self, uses: impl IntoIterator<Item = (SongId, i64)>) {
        for (song, at) in uses {
            let e = self.counted_at.entry(song).or_insert(at);
            *e = (*e).max(at);
        }
    }

    /// Observes the song now live (`None` when live shows something that is not a song).
    /// `day` is the local calendar day number of `now_ms`. Returns true when it must be logged.
    pub(crate) fn observe(&mut self, song: Option<&SongId>, now_ms: i64, day: i64) -> bool {
        if self.day.is_some_and(|d| d != day) {
            self.counted_at.clear();
        }
        self.day = Some(day);
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
}

/// Local calendar day number (days since 1970-01-01 in the machine's time zone).
pub(crate) fn local_day(now_ms: i64) -> i64 {
    use chrono::{Local, TimeZone};
    Local.timestamp_millis_opt(now_ms).single().map_or_else(
        || now_ms.div_euclid(86_400_000),
        |t| i64::from(chrono::Datelike::num_days_from_ce(&t)),
    )
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
        assert!(d.observe(Some(&s("a")), 0, 0));
        assert!(!d.observe(Some(&s("a")), 1_000, 0));
        // Still the same song long after the window: moving slides is not a new use.
        assert!(!d.observe(Some(&s("a")), REPEAT_WINDOW_MS * 3, 0));
    }

    #[test]
    fn returning_within_the_window_is_the_same_use() {
        let mut d = UsageDedupe::default();
        assert!(d.observe(Some(&s("a")), 0, 0));
        assert!(d.observe(Some(&s("b")), 10, 0));
        assert!(!d.observe(Some(&s("a")), 20, 0));
        // A non-song in between does not change that.
        assert!(!d.observe(None, 30, 0));
        assert!(!d.observe(Some(&s("a")), 40, 0));
    }

    #[test]
    fn counts_again_after_the_window() {
        let mut d = UsageDedupe::default();
        assert!(d.observe(Some(&s("a")), 0, 0));
        assert!(!d.observe(None, 1, 0));
        assert!(!d.observe(Some(&s("a")), REPEAT_WINDOW_MS - 1, 0));
        assert!(!d.observe(None, REPEAT_WINDOW_MS, 0));
        assert!(d.observe(Some(&s("a")), REPEAT_WINDOW_MS, 0));
    }

    #[test]
    fn a_new_calendar_day_resets_the_history() {
        let mut d = UsageDedupe::default();
        assert!(d.observe(Some(&s("a")), 0, 10));
        assert!(d.observe(Some(&s("b")), 1, 10));
        // Midnight passes with a different song live: a is a fresh use on day 11.
        assert!(d.observe(Some(&s("a")), 2, 11));
        assert!(d.observe(Some(&s("b")), 3, 11));
    }

    #[test]
    fn seeded_history_prevents_relogging_after_a_restart() {
        let mut d = UsageDedupe::default();
        d.seed([(s("a"), 100), (s("a"), 50)]);
        assert!(!d.observe(Some(&s("a")), 100 + REPEAT_WINDOW_MS - 1, 0));
        assert!(!d.observe(None, 100 + REPEAT_WINDOW_MS, 0));
        assert!(d.observe(Some(&s("a")), 100 + REPEAT_WINDOW_MS, 0));
    }

    #[test]
    fn local_day_changes_across_a_day() {
        let a = local_day(1_700_000_000_000);
        assert_eq!(local_day(1_700_000_000_000 + 2 * 86_400_000), a + 2);
    }

    #[test]
    fn non_songs_never_count() {
        let mut d = UsageDedupe::default();
        assert!(!d.observe(None, 0, 0));
    }
}
