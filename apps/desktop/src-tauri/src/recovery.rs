//! Crash recovery ("retomar donde estabas"): the session (open service, what is on screen,
//! timers, modo culto) is saved after every accepted command by `hollyshow-app`; this module adds
//! the safety nets around it:
//!
//! * an autosave tick (running timers change without any command),
//! * a flush from the panic hook, on a helper thread so a panic that happens while the engine
//!   lock is held cannot deadlock the hook,
//! * the hourly database backup for sessions that stay open for days.

use crate::state::{now_ms, BACKUPS_KEPT};
use hollyshow_app::AppService;
use hollyshow_storage_sqlite::Store;
use std::sync::Arc;
use std::time::Duration;

/// Seconds between autosaves.
pub const AUTOSAVE_SECS: u64 = 5;
/// How long the panic hook waits for the flush before letting the panic continue.
const PANIC_FLUSH_BUDGET: Duration = Duration::from_millis(1500);

/// Runs `flush` on a helper thread and waits at most `budget`. Returns whether it finished.
pub fn flush_with_budget(flush: impl FnOnce() + Send + 'static, budget: Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("panic-flush".into())
        .spawn(move || {
            flush();
            let _ = tx.send(());
        });
    spawned.is_ok() && rx.recv_timeout(budget).is_ok()
}

/// Chains a hook that saves the session after any panic, then runs the previous hook.
pub fn install_panic_flush(app: Arc<AppService>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let a = app.clone();
        if !flush_with_budget(move || a.flush_session(), PANIC_FLUSH_BUDGET) {
            log::error!("could not save the session from the panic hook in time");
        }
        previous(info);
    }));
}

/// Periodic autosave and hourly backup. Runs for the life of the app.
pub fn spawn_autosave(app: Arc<AppService>, store: Arc<Store>, backups: std::path::PathBuf) {
    tauri::async_runtime::spawn(async move {
        let mut ticks: u64 = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(AUTOSAVE_SECS)).await;
            ticks += 1;
            let (a, s, dir) = (app.clone(), store.clone(), backups.clone());
            let hourly = ticks % (3600 / AUTOSAVE_SECS) == 0;
            let _ = tauri::async_runtime::spawn_blocking(move || {
                a.flush_session();
                if hourly {
                    if let Err(e) = s.daily_backup(&dir, BACKUPS_KEPT, now_ms()) {
                        log::warn!("scheduled backup failed: {e}");
                    }
                }
            })
            .await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn flush_runs_within_budget() {
        let done = Arc::new(AtomicBool::new(false));
        let d = done.clone();
        assert!(flush_with_budget(
            move || d.store(true, Ordering::SeqCst),
            Duration::from_secs(2)
        ));
        assert!(done.load(Ordering::SeqCst));
    }

    #[test]
    fn a_stuck_flush_never_blocks_the_panic_hook_for_long() {
        let start = std::time::Instant::now();
        let finished = flush_with_budget(
            || std::thread::sleep(Duration::from_millis(600)),
            Duration::from_millis(50),
        );
        assert!(!finished);
        assert!(start.elapsed() < Duration::from_millis(400));
    }
}
