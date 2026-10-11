//! Coalescing background writer for the persisted session.
//!
//! `AppService::dispatch` must not wait for the database (a backup, a Bible install or a slow
//! disk would stall every client's ack), so the JSON is handed to one thread that always writes
//! only the newest pending value.

use hollyshow_storage_api::SettingsRepo;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const RETRY_DELAY: Duration = Duration::from_millis(1000);

#[derive(Default)]
struct Inner {
    pending: Option<String>,
    writing: bool,
    closed: bool,
}

struct Shared {
    inner: Mutex<Inner>,
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub(crate) struct SessionWriter {
    shared: Arc<Shared>,
}

impl SessionWriter {
    pub(crate) fn spawn(settings: Arc<dyn SettingsRepo>, key: &'static str) -> Self {
        let shared = Arc::new(Shared {
            inner: Mutex::default(),
            cv: Condvar::new(),
        });
        let s = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("session-writer".into())
            .spawn(move || run(&s, &*settings, key));
        if let Err(e) = spawned {
            log::error!("could not start the session writer: {e}");
        }
        Self { shared }
    }

    /// Queues `json`, replacing any value not written yet. Never blocks on I/O.
    pub(crate) fn submit(&self, json: String) {
        let mut g = self.shared.lock();
        g.pending = Some(json);
        self.shared.cv.notify_all();
    }

    /// Waits until everything submitted so far is on disk (or `budget` runs out).
    pub(crate) fn drain(&self, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        let mut g = self.shared.lock();
        while g.pending.is_some() || g.writing {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            g = self
                .shared
                .cv
                .wait_timeout(g, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        true
    }
}

impl Drop for SessionWriter {
    fn drop(&mut self) {
        let mut g = self.shared.lock();
        g.closed = true;
        self.shared.cv.notify_all();
    }
}

fn run(shared: &Shared, settings: &dyn SettingsRepo, key: &str) {
    loop {
        let json = {
            let mut g = shared.lock();
            loop {
                if let Some(j) = g.pending.take() {
                    g.writing = true;
                    break j;
                }
                if g.closed {
                    return;
                }
                g = shared.cv.wait(g).unwrap_or_else(|p| p.into_inner());
            }
        };
        let result = settings.set(key, &json);
        let mut g = shared.lock();
        g.writing = false;
        if let Err(e) = result {
            log::warn!("could not persist the session: {e}");
            if g.pending.is_none() {
                g.pending = Some(json);
            }
            let (guard, _) = shared
                .cv
                .wait_timeout(g, RETRY_DELAY)
                .unwrap_or_else(|p| p.into_inner());
            g = guard;
            if g.closed {
                return;
            }
        }
        shared.cv.notify_all();
        drop(g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_storage_api::{memory::MemoryStore, StorageResult};

    struct Slow(MemoryStore);
    impl SettingsRepo for Slow {
        fn get(&self, k: &str) -> StorageResult<Option<String>> {
            self.0.get(k)
        }
        fn set(&self, k: &str, v: &str) -> StorageResult<()> {
            std::thread::sleep(Duration::from_millis(100));
            self.0.set(k, v)
        }
        fn remove(&self, k: &str) -> StorageResult<()> {
            self.0.remove(k)
        }
    }

    #[test]
    fn submit_does_not_block_and_the_newest_value_wins() {
        let repo = Arc::new(Slow(MemoryStore::default()));
        let w = SessionWriter::spawn(repo.clone(), "k");
        let t = Instant::now();
        for i in 0..20 {
            w.submit(format!("v{i}"));
        }
        assert!(t.elapsed() < Duration::from_millis(50), "submit blocked");
        assert!(w.drain(Duration::from_secs(3)));
        assert_eq!(repo.get("k").unwrap().as_deref(), Some("v19"));
    }
}
