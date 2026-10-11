//! Per-client update feed: role projection and slow-consumer handling in one place, so every
//! transport (WebSocket today, others later) gets the same guarantees.

use hollyshow_core::{ResyncReason, Role, ServerMsg, Update};
use hollyshow_engine::Engine;
use std::sync::Arc;
use tokio::sync::broadcast::{self, error::RecvError};

/// Turns the engine's broadcast into the exact messages one client should receive.
///
/// Create it **before** sending the initial snapshot (`AppService::feed` then
/// `AppService::snapshot_msg`) so no update can fall in the gap; the client discards events whose
/// `version` it already has.
pub struct RoleFeed {
    rx: broadcast::Receiver<Update>,
    role: Role,
    engine: Arc<Engine>,
}

impl RoleFeed {
    pub(crate) fn new(engine: Arc<Engine>, role: Role) -> Self {
        Self {
            rx: engine.subscribe(),
            role,
            engine,
        }
    }

    /// Next batch to send, or `None` when the engine is gone.
    ///
    /// On `Lagged` (the client was too slow and updates were dropped) the missed events are not
    /// replayed: the client gets a fresh role-filtered `State` followed by `Resync`, and answers
    /// by requesting a snapshot. A slow phone can therefore never stall the engine or other
    /// clients.
    pub async fn next(&mut self) -> Option<Vec<ServerMsg>> {
        match self.rx.recv().await {
            Ok(update) => Some(ServerMsg::from_update(
                &update,
                self.role,
                self.engine.now_ms(),
            )),
            Err(RecvError::Lagged(missed)) => {
                log::warn!("client feed lagged by {missed} updates; resyncing");
                Some(vec![
                    self.engine.with_state(|s| ServerMsg::State {
                        view: hollyshow_core::project_view(&s.view(), self.role),
                        now_ms: self.engine.now_ms(),
                    }),
                    ServerMsg::Resync {
                        reason: ResyncReason::Lagged,
                    },
                ])
            }
            Err(RecvError::Closed) => None,
        }
    }
}
