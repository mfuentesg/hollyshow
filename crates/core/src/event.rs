//! Facts emitted after a command changed the state. Consumers (Tauri windows, WebSocket
//! clients, loggers, usage statistics) subscribe to these instead of diffing state.

use crate::ids::{ItemId, OutputId, PlaylistId, TimerId};
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
}

/// What the engine broadcasts after every accepted command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Update {
    #[ts(type = "number")]
    pub version: u64,
    pub view: StateView,
    pub events: Vec<Event>,
}
