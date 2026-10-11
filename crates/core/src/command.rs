//! Everything a client (operator window, remote, API) may ask the core to do.
//!
//! Two layers, deliberately separate:
//! * [`Command`] is the **wire** type: it is what crosses the Tauri IPC or the WebSocket and is
//!   mirrored to TypeScript. It carries no host-only data (no resolved slides) and no clock values.
//! * [`EngineCommand`] is what the reducer consumes. It is **never** deserialised from the wire:
//!   only the host (`hollyshow-app`) builds it, after authorising and enriching a wire command.
//!
//! Wire shape: `{"kind":"next","expectedVersion":418}` (PLAN §3.4).

use crate::bible::{ParallelLayout, Passage, VersionCode};
use crate::ids::{ItemId, OutputId, PlaylistId, TimerId};
use crate::state::{Cue, OutputLayers, OutputLook, ShowEntry, Slide, Target, Timer};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Command {
    // ---- navigation (relative commands carry the version they were issued against) ----
    /// Advance live to the next slide of the show. No-op at the end of the show.
    Next {
        #[ts(type = "number | null")]
        expected_version: Option<u64>,
    },
    Prev {
        #[ts(type = "number | null")]
        expected_version: Option<u64>,
    },
    // ---- absolute commands: safe to repeat ----
    GoTo {
        item_id: ItemId,
        slide_index: u32,
        target: Target,
    },
    ShowItem {
        item_id: ItemId,
        target: Target,
    },
    /// Promote preview to live.
    TakePreview,
    Blackout {
        on: bool,
    },
    Clear {
        on: bool,
    },
    Logo {
        on: bool,
    },
    /// Request a Bible passage in the chosen versions. The host looks the text up; clients can
    /// only name the passage, never supply the slide that ends up on the projector.
    ShowBible {
        passage: Passage,
        versions: Vec<VersionCode>,
        layout: ParallelLayout,
        target: Target,
    },
    StageMessage {
        text: Option<String>,
    },
    // ---- show management ----
    /// Make `id` the active service: the host expands it to slides and loads the show.
    /// Operator only.
    SetActivePlaylist {
        id: PlaylistId,
    },
    /// Rebuild the show from the active playlist (after editing songs, themes or the playlist).
    /// Operator only. Live edits are re-applied on top (see `hollyshow-app`).
    RefreshShow,
    /// Live edit ("reflow"): replace the text of a slide, updating live/preview if they show it.
    /// Operator only. Ephemeral: it overlays the show for this service and never writes back to
    /// the song library (the host re-applies it after a `RefreshShow`).
    UpdateSlide {
        item_id: ItemId,
        slide_index: u32,
        slide: Slide,
    },
    // ---- timers (the host stamps the clock; clients never send times) ----
    /// Create a timer or change its label/kind/duration. Run state is owned by the host: a new
    /// timer starts stopped and an existing one keeps running as it was.
    SetTimer {
        timer: Timer,
    },
    StartTimer {
        id: TimerId,
    },
    PauseTimer {
        id: TimerId,
    },
    ResetTimer {
        id: TimerId,
    },
    RemoveTimer {
        id: TimerId,
    },
    // ---- outputs / modes ----
    SetOutputLook {
        output: OutputId,
        look: OutputLook,
    },
    /// Replace the layer state (background/media/text/overlay/blackout) of an existing output.
    SetOutputLayers {
        output: OutputId,
        layers: OutputLayers,
    },
    /// Forget an output (its look and layers). `main` cannot be removed.
    RemoveOutput {
        output: OutputId,
    },
    /// Restart the per-item clock of the live item (e.g. after a long pause).
    ResetItemClock,
    /// "Modo culto": lock accidental edits. Always allowed, so the operator can unlock.
    ServiceMode {
        on: bool,
    },
    /// A command from a newer protocol revision. Deserialisation falls back to this instead of
    /// failing the whole frame; every role is refused and the sender gets `unsupported`.
    #[serde(other)]
    Unknown,
}

impl Command {
    /// Relative commands are not idempotent by nature and must carry `expected_version`
    /// when issued by a remote.
    pub fn is_relative(&self) -> bool {
        matches!(self, Self::Next { .. } | Self::Prev { .. })
    }
}

/// Host-supplied inputs the reducer must not read from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ctx {
    /// Wall clock in Unix epoch milliseconds, stamped by the engine from its `Clock`.
    pub now_ms: i64,
}

/// Session state restored after a restart (see `hollyshow-app`).
#[derive(Debug, Clone, PartialEq)]
pub struct Restore {
    pub live: Option<Cue>,
    pub timers: Vec<Timer>,
    pub service_mode: bool,
}

/// Input of the reducer. Not serialisable on purpose: it can only be built by host code.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    /// A wire command that needs nothing from the host. `ShowBible` (needs resolved text),
    /// `SetActivePlaylist`, `RefreshShow` and `Unknown` are rejected here.
    Client(Command),
    /// A Bible slide resolved by the host from the library.
    ShowBible {
        target: Target,
        layout: ParallelLayout,
        slide: Slide,
    },
    /// Replace the expanded show (built by the host from the active playlist).
    LoadShow { entries: Vec<ShowEntry> },
    /// Boot-time restoration of the previous session.
    Restore(Restore),
}

impl From<Command> for EngineCommand {
    fn from(c: Command) -> Self {
        Self::Client(c)
    }
}
