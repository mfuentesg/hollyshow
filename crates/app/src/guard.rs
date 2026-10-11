//! Panic containment. The release profile unwinds (no `panic = "abort"`), so a bug in one
//! command is logged and answered with an `internal` error instead of blanking the projector.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Runs `f`, returning `None` (after logging) if it panicked.
pub(crate) fn contain<T>(what: &str, f: impl FnOnce() -> T) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => Some(v),
        Err(_) => {
            log::error!("panic contained while handling {what}; the presentation engine is intact");
            None
        }
    }
}

/// Logs every panic with its location through `log`, then runs the previous hook. Call once at
/// start-up.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        previous(info);
    }));
}
