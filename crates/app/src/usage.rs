//! Background logger for the CCLI usage report.
//!
//! Going live must not wait for the database, so the (playlist, item) pair is queued here and a
//! thread resolves it to a song and appends to the usage log. Items that are not songs are ignored.

use hollyshow_core::{ItemContent, ItemId, PlaylistId};
use hollyshow_storage_api::Repos;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct UsageLogger {
    tx: Mutex<Sender<(PlaylistId, ItemId)>>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn record(repos: &Repos, playlist: &PlaylistId, item: &ItemId) {
    let pl = match repos.playlists.get(playlist) {
        Ok(Some(pl)) => pl,
        Ok(None) => return,
        Err(e) => return log::warn!("usage log: could not read the service: {e}"),
    };
    let song = pl
        .items
        .iter()
        .find(|i| &i.id == item)
        .and_then(|i| match &i.content {
            ItemContent::Song { song_id, .. } => Some(song_id.clone()),
            _ => None,
        });
    if let Some(song) = song {
        if let Err(e) = repos.songs.log_usage(&song, now_ms()) {
            log::warn!("usage log: {e}");
        }
    }
}

impl UsageLogger {
    pub(crate) fn spawn(repos: Repos) -> Self {
        let (tx, rx) = channel::<(PlaylistId, ItemId)>();
        let spawned = std::thread::Builder::new()
            .name("usage-logger".into())
            .spawn(move || {
                while let Ok((playlist, item)) = rx.recv() {
                    record(&repos, &playlist, &item);
                }
            });
        if let Err(e) = spawned {
            log::error!("could not start the usage logger: {e}");
        }
        Self { tx: Mutex::new(tx) }
    }

    pub(crate) fn submit(&self, playlist: PlaylistId, item: ItemId) {
        let tx = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        let _ = tx.send((playlist, item));
    }
}
