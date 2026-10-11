//! Everything a client (operator window, remote, API) may ask the core to do.
//!
//! Wire shape: `{"kind":"next","expectedVersion":418}` (PLAN §3.4).

use crate::bible::{ParallelLayout, Passage, VersionCode};
use crate::ids::{ItemId, OutputId, TimerId};
use crate::state::{OutputLook, ShowEntry, Slide, Target, Timer};
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
    /// Show a Bible passage in the chosen versions. The host resolves the text into `resolved`
    /// (clients leave it `None`); the reducer rejects the command if it is still missing.
    ShowBible {
        passage: Passage,
        versions: Vec<VersionCode>,
        layout: ParallelLayout,
        target: Target,
        resolved: Option<Slide>,
    },
    StageMessage {
        text: Option<String>,
    },
    // ---- show management ----
    /// Replace the expanded show (host-generated from the playlist).
    LoadShow {
        entries: Vec<ShowEntry>,
    },
    /// Live edit ("reflow"): replace the text of a slide, updating live/preview if they show it.
    UpdateSlide {
        item_id: ItemId,
        slide_index: u32,
        slide: Slide,
    },
    // ---- timers (wall clock injected by the host) ----
    SetTimer {
        timer: Timer,
    },
    StartTimer {
        id: TimerId,
        #[ts(type = "number")]
        at_ms: i64,
    },
    PauseTimer {
        id: TimerId,
        #[ts(type = "number")]
        at_ms: i64,
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
    ServiceMode {
        on: bool,
    },
}

impl Command {
    /// Relative commands are not idempotent by nature and must carry `expected_version`
    /// when issued by a remote.
    pub fn is_relative(&self) -> bool {
        matches!(self, Self::Next { .. } | Self::Prev { .. })
    }
}
