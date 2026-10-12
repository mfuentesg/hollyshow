//! The single source of truth shown by every window and remote (PLAN §3.1).

use crate::bible::{ParallelLayout, VersionCode};
use crate::ids::{ItemId, MediaId, OutputId, SlideId, ThemeId, TimerId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SlideKind {
    Lyrics,
    Bible,
    Media,
    Text,
    Countdown,
    Blank,
}

/// One column of a (possibly parallel) Bible slide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BibleColumn {
    pub version: VersionCode,
    /// Human reference in the version language: "Juan 3:16-18".
    pub reference: String,
    pub text: String,
    /// Copyright / abbreviation shown when the theme asks for it.
    pub copyright: Option<String>,
}

/// A fully resolved, renderable slide. Outputs only need this plus a theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Slide {
    pub id: SlideId,
    pub kind: SlideKind,
    /// Section label for lyrics ("Coro") or reference for Bible.
    pub label: Option<String>,
    pub lines: Vec<String>,
    /// Second-language lines (bilingual songs), same length as `lines` when present.
    #[serde(default)]
    pub translation: Vec<String>,
    /// Lines with inline ChordPro chords for the musician / stage view.
    #[serde(default)]
    pub chart: Vec<String>,
    #[serde(default)]
    pub bible: Vec<BibleColumn>,
    #[serde(default)]
    pub layout: ParallelLayout,
    pub media: Option<MediaId>,
    pub footer: Option<String>,
    pub theme_id: Option<ThemeId>,
    /// Slides sharing a group render at one common font size (the smallest that fits them all),
    /// so a song does not jump between sizes. `None` lets every slide auto-fit on its own.
    #[serde(default)]
    pub size_group: Option<String>,
}

impl Slide {
    pub fn blank(id: impl Into<String>) -> Self {
        Self {
            id: SlideId::new(id),
            kind: SlideKind::Blank,
            label: None,
            lines: vec![],
            translation: vec![],
            chart: vec![],
            bible: vec![],
            layout: ParallelLayout::default(),
            media: None,
            footer: None,
            theme_id: None,
            size_group: None,
        }
    }

    pub fn lyrics(id: impl Into<String>, label: Option<&str>, lines: &[&str]) -> Self {
        Self {
            kind: SlideKind::Lyrics,
            label: label.map(str::to_owned),
            lines: lines.iter().map(|l| (*l).to_owned()).collect(),
            ..Self::blank(id)
        }
    }
}

/// A service item already expanded to slides by the host (songs crate / bible crate).
/// Keeping expansion outside the reducer keeps `core` pure and trivially testable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShowEntry {
    pub item_id: ItemId,
    pub title: String,
    pub slides: Vec<Slide>,
    pub duration_secs: Option<u32>,
}

/// What an output currently displays: a copy of the slide, so outputs never need the show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Cue {
    /// `None` for ad-hoc content such as a Bible passage requested from a remote.
    pub item_id: Option<ItemId>,
    pub slide_index: u32,
    pub slide: Slide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Target {
    Live,
    Preview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StageMessage {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TimerKind {
    Countdown,
    Stopwatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TimerRun {
    Stopped,
    Running,
    Paused,
}

/// Timestamps come from the host clock (`Ctx::now_ms`), never from clients, so every viewer
/// computes the same remaining time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Timer {
    pub id: TimerId,
    pub label: String,
    pub kind: TimerKind,
    /// Countdown length (ignored by stopwatches).
    pub duration_secs: u32,
    pub run: TimerRun,
    /// Epoch ms at which the current run segment started.
    #[ts(type = "number | null")]
    pub started_at_ms: Option<i64>,
    /// Accumulated elapsed time of previous run segments.
    #[ts(type = "number")]
    pub elapsed_ms: i64,
}

impl Timer {
    pub fn new(id: &str, label: &str, kind: TimerKind, duration_secs: u32) -> Self {
        Self {
            id: TimerId::new(id),
            label: label.to_owned(),
            kind,
            duration_secs,
            run: TimerRun::Stopped,
            started_at_ms: None,
            elapsed_ms: 0,
        }
    }

    pub fn elapsed_at(&self, now_ms: i64) -> i64 {
        match (self.run, self.started_at_ms) {
            (TimerRun::Running, Some(s)) => self.elapsed_ms + (now_ms - s).max(0),
            _ => self.elapsed_ms,
        }
    }

    /// Milliseconds past the planned duration (0 while on time); `None` for stopwatches.
    pub fn overtime_ms_at(&self, now_ms: i64) -> Option<i64> {
        self.remaining_at(now_ms).map(|r| (-r).max(0))
    }

    /// Remaining milliseconds for countdowns (negative once overrun); `None` for stopwatches.
    pub fn remaining_at(&self, now_ms: i64) -> Option<i64> {
        (self.kind == TimerKind::Countdown)
            .then(|| i64::from(self.duration_secs) * 1000 - self.elapsed_at(now_ms))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OutputKind {
    /// Main projector: lyrics, Bible, media.
    Main,
    /// Confidence monitor with next slide, clock, timers, messages.
    Stage,
    /// Transparent lower third (OBS / vMix browser source).
    LowerThird,
    /// Lobby / side screens: announcements loop or mirror of main.
    Secondary,
}

/// Per-output "look" over the same presentation state (multiple outputs, PLAN §2.5 C).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutputLook {
    pub kind: OutputKind,
    pub theme_id: Option<ThemeId>,
    pub enabled: bool,
    /// When false this output ignores blackout (e.g. stage display stays visible).
    pub follows_blackout: bool,
}

/// Layer state of one output, independent of the others (PLAN §2.5 C). The global
/// `blackout`/`clear`/`logo` flags still apply on top (see [`StateView::frame_for`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutputLayers {
    /// Background override for this output (replaces the theme background).
    pub background: Option<MediaId>,
    /// Foreground media (image/video) drawn above the background and below the text.
    pub media: Option<MediaId>,
    /// Whether the slide text is drawn on this output.
    pub text: bool,
    /// Free text drawn above everything (lobby notice, lower-third alert).
    pub overlay: Option<String>,
    /// Blackout of this output only; the global blackout is separate.
    pub blackout: bool,
}

impl Default for OutputLayers {
    fn default() -> Self {
        Self {
            background: None,
            media: None,
            text: true,
            overlay: None,
            blackout: false,
        }
    }
}

/// What one output must draw right now: the result of combining the global flags, the output's
/// own look and its layers. Renderers use this instead of re-deriving the rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutputFrame {
    pub output: OutputId,
    pub kind: OutputKind,
    pub theme_id: Option<ThemeId>,
    pub blackout: bool,
    pub logo: bool,
    pub background: Option<MediaId>,
    pub media: Option<MediaId>,
    /// Slide text visible (false when cleared, hidden by the layer, or blacked out).
    pub show_text: bool,
    pub overlay: Option<String>,
    pub cue: Option<Cue>,
}

/// Clock of the item currently live, started when the operator reaches the item (per-item
/// duration). The host clock stamps `started_at_ms`; viewers derive elapsed and overtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ItemClock {
    pub item_id: ItemId,
    pub planned_secs: u32,
    #[ts(type = "number")]
    pub started_at_ms: i64,
}

impl ItemClock {
    pub fn elapsed_ms_at(&self, now_ms: i64) -> i64 {
        (now_ms - self.started_at_ms).max(0)
    }

    /// Milliseconds left of the planned time (negative once overrun).
    pub fn remaining_ms_at(&self, now_ms: i64) -> i64 {
        i64::from(self.planned_secs) * 1000 - self.elapsed_ms_at(now_ms)
    }

    pub fn overtime_ms_at(&self, now_ms: i64) -> i64 {
        (-self.remaining_ms_at(now_ms)).max(0)
    }
}

/// Everything that changes at 60 Hz of human speed: small, cheap to broadcast to every client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StateView {
    /// Boot epoch of the host: random per run, constant while it lives. `version` restarts at 0
    /// on every run, so `(epoch, version)` is what identifies a state; clients resync on change.
    #[ts(type = "number")]
    pub epoch: u64,
    #[ts(type = "number")]
    pub version: u64,
    /// Bumped whenever the show (slide lists) changes; clients resync the snapshot on change.
    #[ts(type = "number")]
    pub show_revision: u64,
    pub live: Option<Cue>,
    pub preview: Option<Cue>,
    pub blackout: bool,
    /// Hide text but keep background.
    pub clear: bool,
    pub logo: bool,
    pub stage_message: Option<StageMessage>,
    pub timers: Vec<Timer>,
    pub outputs: BTreeMap<OutputId, OutputLook>,
    /// Per-output layer state; every key of `outputs` has an entry.
    #[serde(default)]
    pub output_layers: BTreeMap<OutputId, OutputLayers>,
    /// Clock of the live item (only when it has a planned duration).
    #[serde(default)]
    pub item_clock: Option<ItemClock>,
    /// "Modo culto": locks accidental edits (live edits, outputs, timers) in the UI and reducer.
    pub service_mode: bool,
}

impl StateView {
    /// Combines global flags, the output's look and its layers into what it must draw.
    /// Returns `None` for unknown or disabled outputs.
    ///
    /// * Blackout: the global flag (unless the output ignores it) or the output's own flag.
    /// * Text: hidden by `clear`, by the output's text layer, or by any blackout.
    /// * The live cue is always carried so outputs can fade back without a round trip.
    pub fn frame_for(&self, output: &OutputId) -> Option<OutputFrame> {
        let look = self.outputs.get(output).filter(|l| l.enabled)?;
        let layers = self.output_layers.get(output).cloned().unwrap_or_default();
        let blackout = (self.blackout && look.follows_blackout) || layers.blackout;
        Some(OutputFrame {
            output: output.clone(),
            kind: look.kind,
            theme_id: look.theme_id.clone(),
            blackout,
            logo: self.logo && !blackout,
            background: layers.background,
            media: layers.media,
            show_text: layers.text && !self.clear && !blackout,
            overlay: layers.overlay.filter(|_| !blackout),
            cue: self.live.clone(),
        })
    }
}

/// Full authoritative state, including the expanded show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresentationState {
    /// Boot epoch, see [`StateView::epoch`]. `0` means "not stamped yet": the engine assigns a
    /// random one when it takes ownership of the state.
    #[ts(type = "number")]
    pub epoch: u64,
    #[ts(type = "number")]
    pub version: u64,
    #[ts(type = "number")]
    pub show_revision: u64,
    pub show: Vec<ShowEntry>,
    pub live: Option<Cue>,
    pub preview: Option<Cue>,
    pub blackout: bool,
    pub clear: bool,
    pub logo: bool,
    pub stage_message: Option<StageMessage>,
    pub timers: Vec<Timer>,
    pub outputs: BTreeMap<OutputId, OutputLook>,
    #[serde(default)]
    pub output_layers: BTreeMap<OutputId, OutputLayers>,
    #[serde(default)]
    pub item_clock: Option<ItemClock>,
    pub service_mode: bool,
}

impl Default for PresentationState {
    fn default() -> Self {
        let mut outputs = BTreeMap::new();
        outputs.insert(
            OutputId::new("main"),
            OutputLook {
                kind: OutputKind::Main,
                theme_id: None,
                enabled: true,
                follows_blackout: true,
            },
        );
        outputs.insert(
            OutputId::new("stage"),
            OutputLook {
                kind: OutputKind::Stage,
                theme_id: None,
                enabled: false,
                follows_blackout: false,
            },
        );
        Self {
            epoch: 0,
            version: 0,
            show_revision: 0,
            show: vec![],
            live: None,
            preview: None,
            blackout: false,
            clear: false,
            logo: false,
            stage_message: None,
            timers: vec![],
            output_layers: outputs
                .keys()
                .map(|k| (k.clone(), OutputLayers::default()))
                .collect(),
            outputs,
            item_clock: None,
            service_mode: false,
        }
    }
}

impl PresentationState {
    pub fn view(&self) -> StateView {
        StateView {
            epoch: self.epoch,
            version: self.version,
            show_revision: self.show_revision,
            live: self.live.clone(),
            preview: self.preview.clone(),
            blackout: self.blackout,
            clear: self.clear,
            logo: self.logo,
            stage_message: self.stage_message.clone(),
            timers: self.timers.clone(),
            outputs: self.outputs.clone(),
            output_layers: self.output_layers.clone(),
            item_clock: self.item_clock.clone(),
            service_mode: self.service_mode,
        }
    }

    pub fn entry(&self, item_id: &ItemId) -> Option<&ShowEntry> {
        self.show.iter().find(|e| &e.item_id == item_id)
    }

    /// Flat (entry index, slide index) position of the live cue, if it still belongs to the show.
    pub fn live_position(&self) -> Option<(usize, usize)> {
        let live = self.live.as_ref()?;
        let item = live.item_id.as_ref()?;
        let ei = self.show.iter().position(|e| &e.item_id == item)?;
        let si = live.slide_index as usize;
        (si < self.show[ei].slides.len()).then_some((ei, si))
    }

    /// Next position in flat show order, skipping entries without slides.
    pub fn position_after(&self, pos: Option<(usize, usize)>) -> Option<(usize, usize)> {
        match pos {
            None => self.first_position(),
            Some((ei, si)) => {
                if si + 1 < self.show[ei].slides.len() {
                    return Some((ei, si + 1));
                }
                (ei + 1..self.show.len())
                    .find(|i| !self.show[*i].slides.is_empty())
                    .map(|i| (i, 0))
            }
        }
    }

    pub fn position_before(&self, pos: Option<(usize, usize)>) -> Option<(usize, usize)> {
        let (ei, si) = pos?;
        if si > 0 {
            return Some((ei, si - 1));
        }
        (0..ei)
            .rev()
            .find(|i| !self.show[*i].slides.is_empty())
            .map(|i| (i, self.show[i].slides.len() - 1))
    }

    fn first_position(&self) -> Option<(usize, usize)> {
        self.show
            .iter()
            .position(|e| !e.slides.is_empty())
            .map(|i| (i, 0))
    }

    pub fn cue_at(&self, (ei, si): (usize, usize)) -> Cue {
        let e = &self.show[ei];
        Cue {
            item_id: Some(e.item_id.clone()),
            slide_index: si as u32,
            slide: e.slides[si].clone(),
        }
    }
}
