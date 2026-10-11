//! "Modo culto" keeps the machine awake: no screensaver, display sleep or idle suspend while a
//! service is running. Uses the platform APIs through the `keepawake` crate (IOKit on macOS,
//! `SetThreadExecutionState` on Windows, the D-Bus inhibit interfaces on Linux). When the
//! platform refuses (no D-Bus session, a minimal VM) the failure is logged and the service
//! carries on: this is a convenience, never a precondition.

use std::sync::Mutex;

#[derive(Default)]
pub struct Power {
    guard: Mutex<Option<keepawake::KeepAwake>>,
}

impl Power {
    /// Starts or stops inhibiting sleep. Idempotent. Returns whether sleep is inhibited now.
    pub fn set_awake(&self, on: bool) -> bool {
        let mut g = self.guard.lock().unwrap_or_else(|p| p.into_inner());
        match (on, g.is_some()) {
            (true, false) => {
                match keepawake::Builder::default()
                    .display(true)
                    .idle(true)
                    .sleep(true)
                    .reason("Modo culto de HollyShow")
                    .app_name("HollyShow")
                    .app_reverse_domain("com.hollyshow.app")
                    .create()
                {
                    Ok(k) => *g = Some(k),
                    Err(e) => log::warn!("could not prevent sleep: {e}"),
                }
            }
            (false, true) => *g = None,
            _ => {}
        }
        g.is_some()
    }

    #[cfg(test)]
    pub fn is_awake(&self) -> bool {
        self.guard
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some()
    }
}

/// Edge detector so the event bridge only touches the OS when service mode actually changes.
#[derive(Debug, Default)]
pub struct ModeEdge(Option<bool>);

impl ModeEdge {
    /// Returns `Some(new)` when `on` differs from the previous observation.
    pub fn changed(&mut self, on: bool) -> Option<bool> {
        if self.0 == Some(on) {
            None
        } else {
            self.0 = Some(on);
            Some(on)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_fires_only_on_change() {
        let mut e = ModeEdge::default();
        assert_eq!(e.changed(false), Some(false));
        assert_eq!(e.changed(false), None);
        assert_eq!(e.changed(true), Some(true));
        assert_eq!(e.changed(true), None);
        assert_eq!(e.changed(false), Some(false));
    }

    #[test]
    fn turning_off_without_a_guard_is_a_noop() {
        let p = Power::default();
        assert!(!p.set_awake(false));
        assert!(!p.is_awake());
    }
}
