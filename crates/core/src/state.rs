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

/// Wall clock is injected by the host (`at_ms` in commands) so the reducer stays pure.
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

/// Everything that changes at 60 Hz of human speed: small, cheap to broadcast to every client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StateView {
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
    /// "Modo culto": locks accidental edits in the UI.
    pub service_mode: bool,
}

/// Full authoritative state, including the expanded show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresentationState {
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
            outputs,
            service_mode: false,
        }
    }
}

impl PresentationState {
    pub fn view(&self) -> StateView {
        StateView {
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
