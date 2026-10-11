//! Facts emitted after a command changed the state. Consumers (Tauri windows, WebSocket
//! clients, loggers, usage statistics) subscribe to these instead of diffing state.

use crate::ids::{ItemId, OutputId, PlaylistId, TimerId};
use crate::protocol::Role;
use crate::state::{StateView, Target};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LibraryScope {
    Songs,
    Bible,
    Themes,
    Media,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Event {
    ShowLoaded {
        entries: u32,
    },
    SlideChanged {
        target: Target,
        item_id: Option<ItemId>,
        slide_index: u32,
    },
    BlackoutChanged {
        on: bool,
    },
    ClearChanged {
        on: bool,
    },
    LogoChanged {
        on: bool,
    },
    StageMessageChanged {
        text: Option<String>,
    },
    LiveEdited {
        item_id: ItemId,
        slide_index: u32,
    },
    TimerChanged {
        id: TimerId,
    },
    TimerRemoved {
        id: TimerId,
    },
    OutputLookChanged {
        output: OutputId,
    },
    OutputLayersChanged {
        output: OutputId,
    },
    OutputRemoved {
        output: OutputId,
    },
    /// The live item changed or its clock was reset.
    ItemClockChanged {
        item_id: Option<ItemId>,
    },
    ServiceModeChanged {
        on: bool,
    },
    // ---- emitted by the host, not by the reducer ----
    PlaylistChanged {
        id: PlaylistId,
    },
    LibraryChanged {
        scope: LibraryScope,
    },
    /// An event from a newer protocol revision; old peers ignore it instead of dropping the frame.
    #[serde(other)]
    Unknown,
}

impl Event {
    /// Whether a client with `role` may be told about this event. Projectors in the audience and
    /// stage displays only hear about what is on screen, never about editing activity.
    pub fn visible_to(&self, role: Role) -> bool {
        match role {
            Role::Operator | Role::Musician | Role::Pastor => true,
            Role::Stage => matches!(
                self,
                Event::SlideChanged { .. }
                    | Event::BlackoutChanged { .. }
                    | Event::ClearChanged { .. }
                    | Event::LogoChanged { .. }
                    | Event::StageMessageChanged { .. }
                    | Event::TimerChanged { .. }
                    | Event::TimerRemoved { .. }
                    | Event::ItemClockChanged { .. }
            ),
            Role::Congregation => matches!(
                self,
                Event::SlideChanged {
                    target: Target::Live,
                    ..
                } | Event::BlackoutChanged { .. }
                    | Event::ClearChanged { .. }
                    | Event::LogoChanged { .. }
            ),
        }
    }
}

/// What the engine broadcasts after every accepted command.
///
/// This is the **unfiltered** host-side update (Tauri windows are trusted). Network clients must
/// only ever receive [`Update::for_role`] output, via `ServerMsg::from_update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Update {
    #[ts(type = "number")]
    pub version: u64,
    pub view: StateView,
    pub events: Vec<Event>,
}

impl Update {
    /// The role-filtered projection: view fields the role may not see are blanked and invisible
    /// events are dropped.
    pub fn for_role(&self, role: Role) -> Update {
        Update {
            version: self.version,
            view: crate::protocol::project_view(&self.view, role),
            events: self
                .events
                .iter()
                .filter(|e| e.visible_to(role))
                .cloned()
                .collect(),
        }
    }
}
