//! The pure presentation reducer: `apply(state, command) -> (new state, events)`.
//!
//! Rules (PLAN §3.1, §3.4):
//! * Rejected commands never change state and never bump the version.
//! * Absolute commands (`GoTo`, `Blackout`, ...) are idempotent: repeating one that is already
//!   satisfied is a no-op (same version, no events).
//! * Relative commands (`Next`, `Prev`) are rejected with [`CoreError::VersionMismatch`] when
//!   `expected_version` is set and stale, so retries and double taps cannot skip slides.
//! * Every accepted, state-changing command increments `version` by exactly one.
//! * Time never comes from the system clock; the host passes it in [`Ctx`].
//! * The reducer only sees [`EngineCommand`]s, which are built by host code and never come from
//!   the wire, so nothing a remote sends can inject a slide or a timestamp.

use crate::command::{Command, Ctx, EngineCommand, Restore};
use crate::error::CoreError;
use crate::event::Event;
use crate::state::{
    Cue, ItemClock, OutputLayers, PresentationState, StageMessage, Target, Timer, TimerKind,
    TimerRun,
};

/// Longest stage message / overlay accepted (it must stay readable from the stage).
pub const MAX_MESSAGE_CHARS: usize = 280;
/// Longest countdown accepted: 24 h.
pub const MAX_TIMER_SECS: u32 = 24 * 3600;
const MAX_LABEL_CHARS: usize = 80;
const MAX_ID_CHARS: usize = 64;

pub type Applied = (PresentationState, Vec<Event>);

/// Applies `cmd` to `state`, returning the successor state and the events it produced.
pub fn apply(
    state: &PresentationState,
    ctx: Ctx,
    cmd: EngineCommand,
) -> Result<Applied, CoreError> {
    let mut next = state.clone();
    let mut events = Vec::new();

    let cmd = match cmd {
        EngineCommand::Client(c) => c,
        EngineCommand::ShowBible {
            target,
            layout,
            mut slide,
        } => {
            slide.layout = layout;
            let cue = Cue {
                item_id: None,
                slide_index: 0,
                slide,
            };
            match target {
                Target::Live => set_live(&mut next, &mut events, cue),
                Target::Preview => set_preview(&mut next, &mut events, Some(cue)),
            }
            sync_item_clock(state, &mut next, ctx, &mut events);
            return finish(state, next, events);
        }
        EngineCommand::LoadShow { entries } => {
            if next.show != entries {
                events.push(Event::ShowLoaded {
                    entries: entries.len() as u32,
                });
                next.show = entries;
                next.show_revision += 1;
                // Preview pointed into the old show; live keeps its own copy and stays on screen.
                next.preview = None;
            }
            return finish(state, next, events);
        }
        EngineCommand::Restore(Restore {
            live,
            timers,
            service_mode,
        }) => {
            next.live = live;
            next.timers = timers;
            next.service_mode = service_mode;
            sync_item_clock(state, &mut next, ctx, &mut events);
            return finish(state, next, events);
        }
    };

    match cmd {
        Command::Next {
            expected_version,
            epoch,
        } => {
            check_version(state, expected_version, epoch)?;
            if let Some(pos) = next_target(state) {
                go_live(&mut next, &mut events, pos);
            }
        }
        Command::Prev {
            expected_version,
            epoch,
        } => {
            check_version(state, expected_version, epoch)?;
            if let Some(pos) = state.position_before(state.live_position()) {
                go_live(&mut next, &mut events, pos);
            }
        }
        Command::GoTo {
            item_id,
            slide_index,
            target,
        } => {
            let ei = state
                .show
                .iter()
                .position(|e| e.item_id == item_id)
                .ok_or_else(|| CoreError::UnknownItem(item_id.clone()))?;
            let count = state.show[ei].slides.len();
            if slide_index as usize >= count {
                return Err(CoreError::SlideOutOfRange {
                    item_id,
                    slide_index,
                    slide_count: count as u32,
                });
            }
            let pos = (ei, slide_index as usize);
            match target {
                Target::Live => go_live(&mut next, &mut events, pos),
                Target::Preview => set_preview(&mut next, &mut events, Some(state.cue_at(pos))),
            }
        }
        Command::ShowItem { item_id, target } => {
            return apply(
                state,
                ctx,
                EngineCommand::Client(Command::GoTo {
                    item_id,
                    slide_index: 0,
                    target,
                }),
            );
        }
        Command::TakePreview => {
            let cue = state.preview.clone().ok_or(CoreError::NothingInPreview)?;
            match locate(state, &cue) {
                Some(pos) => go_live(&mut next, &mut events, pos),
                None => set_live(&mut next, &mut events, cue),
            }
        }
        Command::Blackout { on } => {
            if next.blackout != on {
                next.blackout = on;
                events.push(Event::BlackoutChanged { on });
            }
        }
        Command::Clear { on } => {
            if next.clear != on {
                next.clear = on;
                events.push(Event::ClearChanged { on });
            }
        }
        Command::Logo { on } => {
            if next.logo != on {
                next.logo = on;
                events.push(Event::LogoChanged { on });
            }
        }
        // Needs host-resolved text: the app layer turns it into `EngineCommand::ShowBible`.
        Command::ShowBible { .. } => return Err(CoreError::BibleNotResolved),
        Command::SetActivePlaylist { .. } | Command::RefreshShow | Command::Unknown => {
            return Err(CoreError::Unsupported(
                "not a presentation command".to_owned(),
            ))
        }
        Command::StageMessage { text } => {
            let msg = text
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .map(|text| StageMessage { text });
            if let Some(m) = &msg {
                check_len("stage message", &m.text, MAX_MESSAGE_CHARS)?;
            }
            if next.stage_message != msg {
                events.push(Event::StageMessageChanged {
                    text: msg.as_ref().map(|m| m.text.clone()),
                });
                next.stage_message = msg;
            }
        }
        Command::UpdateSlide {
            item_id,
            slide_index,
            slide,
        } => {
            ensure_unlocked(state)?;
            let ei = state
                .show
                .iter()
                .position(|e| e.item_id == item_id)
                .ok_or_else(|| CoreError::UnknownItem(item_id.clone()))?;
            let count = state.show[ei].slides.len();
            let si = slide_index as usize;
            if si >= count {
                return Err(CoreError::SlideOutOfRange {
                    item_id,
                    slide_index,
                    slide_count: count as u32,
                });
            }
            if state.show[ei].slides[si] != slide {
                next.show[ei].slides[si] = slide.clone();
                next.show_revision += 1;
                for cue in [&mut next.live, &mut next.preview].into_iter().flatten() {
                    if cue.item_id.as_ref() == Some(&item_id) && cue.slide_index == slide_index {
                        cue.slide = slide.clone();
                    }
                }
                if state.live.as_ref().is_some_and(|c| {
                    c.item_id.as_ref() == Some(&item_id) && c.slide_index == slide_index
                }) {
                    events.push(Event::LiveEdited {
                        item_id,
                        slide_index,
                    });
                }
            }
        }
        Command::SetTimer { timer } => {
            validate_timer(&timer)?;
            let id = timer.id.clone();
            match next.timers.iter_mut().find(|t| t.id == id) {
                Some(existing) => {
                    // Run state belongs to the host; only the configuration is client-editable.
                    let mut updated = timer;
                    updated.run = existing.run;
                    updated.started_at_ms = existing.started_at_ms;
                    updated.elapsed_ms = existing.elapsed_ms;
                    if *existing != updated {
                        *existing = updated;
                        events.push(Event::TimerChanged { id });
                    }
                }
                None => {
                    let mut timer = timer;
                    timer.run = TimerRun::Stopped;
                    timer.started_at_ms = None;
                    timer.elapsed_ms = 0;
                    next.timers.push(timer);
                    events.push(Event::TimerChanged { id });
                }
            }
        }
        Command::StartTimer { id } => {
            let t = timer_mut(&mut next, &id)?;
            if t.run != TimerRun::Running {
                t.run = TimerRun::Running;
                t.started_at_ms = Some(ctx.now_ms);
                events.push(Event::TimerChanged { id });
            }
        }
        Command::PauseTimer { id } => {
            let t = timer_mut(&mut next, &id)?;
            if t.run == TimerRun::Running {
                t.elapsed_ms = t.elapsed_at(ctx.now_ms);
                t.run = TimerRun::Paused;
                t.started_at_ms = None;
                events.push(Event::TimerChanged { id });
            }
        }
        Command::ResetTimer { id } => {
            let t = timer_mut(&mut next, &id)?;
            if t.run != TimerRun::Stopped || t.elapsed_ms != 0 {
                t.run = TimerRun::Stopped;
                t.started_at_ms = None;
                t.elapsed_ms = 0;
                events.push(Event::TimerChanged { id });
            }
        }
        Command::RemoveTimer { id } => {
            let before = next.timers.len();
            next.timers.retain(|t| t.id != id);
            if next.timers.len() == before {
                return Err(CoreError::UnknownTimer(id.to_string()));
            }
            events.push(Event::TimerRemoved { id });
        }
        Command::SetOutputLook { output, look } => {
            ensure_unlocked(state)?;
            check_id("output id", output.as_str())?;
            if next.outputs.get(&output) != Some(&look) {
                next.outputs.insert(output.clone(), look);
                next.output_layers.entry(output.clone()).or_default();
                events.push(Event::OutputLookChanged { output });
            }
        }
        Command::SetOutputLayers { output, layers } => {
            if !state.outputs.contains_key(&output) {
                return Err(CoreError::UnknownOutput(output.to_string()));
            }
            let layers = normalise_layers(layers)?;
            if state.output_layers.get(&output) != Some(&layers) {
                next.output_layers.insert(output.clone(), layers);
                events.push(Event::OutputLayersChanged { output });
            }
        }
        Command::RemoveOutput { output } => {
            ensure_unlocked(state)?;
            if output.as_str() == MAIN_OUTPUT {
                return Err(CoreError::Invalid(
                    "the main output cannot be removed".into(),
                ));
            }
            if next.outputs.remove(&output).is_none() {
                return Err(CoreError::UnknownOutput(output.to_string()));
            }
            next.output_layers.remove(&output);
            events.push(Event::OutputRemoved { output });
        }
        Command::ResetItemClock => {
            if let Some(clock) = next.item_clock.as_mut() {
                clock.started_at_ms = ctx.now_ms;
                events.push(Event::ItemClockChanged {
                    item_id: Some(clock.item_id.clone()),
                });
            }
        }
        Command::ServiceMode { on } => {
            if next.service_mode != on {
                next.service_mode = on;
                events.push(Event::ServiceModeChanged { on });
            }
        }
    }

    sync_item_clock(state, &mut next, ctx, &mut events);
    finish(state, next, events)
}

const MAIN_OUTPUT: &str = "main";

/// Starts the per-item clock when the live item changes: the clock exists only for items with a
/// planned duration and is dropped for ad-hoc content.
fn sync_item_clock(
    state: &PresentationState,
    next: &mut PresentationState,
    ctx: Ctx,
    events: &mut Vec<Event>,
) {
    let item_of = |s: &PresentationState| s.live.as_ref().and_then(|c| c.item_id.clone());
    let live_item = item_of(next);
    if live_item == item_of(state) {
        return;
    }
    let planned = live_item
        .as_ref()
        .and_then(|id| next.entry(id))
        .and_then(|e| e.duration_secs)
        .filter(|d| *d > 0);
    next.item_clock = match (live_item.clone(), planned) {
        (Some(item_id), Some(planned_secs)) => Some(ItemClock {
            item_id,
            planned_secs,
            started_at_ms: ctx.now_ms,
        }),
        _ => None,
    };
    if next.item_clock != state.item_clock {
        events.push(Event::ItemClockChanged { item_id: live_item });
    }
}

fn ensure_unlocked(state: &PresentationState) -> Result<(), CoreError> {
    if state.service_mode {
        Err(CoreError::Locked)
    } else {
        Ok(())
    }
}

fn check_len(what: &str, text: &str, max: usize) -> Result<(), CoreError> {
    if text.chars().count() > max {
        return Err(CoreError::Invalid(format!(
            "{what} exceeds {max} characters"
        )));
    }
    Ok(())
}

fn check_id(what: &str, id: &str) -> Result<(), CoreError> {
    if id.trim().is_empty() || id.chars().count() > MAX_ID_CHARS {
        return Err(CoreError::Invalid(format!(
            "{what} must have 1..={MAX_ID_CHARS} characters"
        )));
    }
    Ok(())
}

fn validate_timer(t: &Timer) -> Result<(), CoreError> {
    check_id("timer id", t.id.as_str())?;
    check_len("timer label", &t.label, MAX_LABEL_CHARS)?;
    if t.kind == TimerKind::Countdown && t.duration_secs > MAX_TIMER_SECS {
        return Err(CoreError::Invalid(format!(
            "countdown longer than {MAX_TIMER_SECS} s"
        )));
    }
    Ok(())
}

fn normalise_layers(mut l: OutputLayers) -> Result<OutputLayers, CoreError> {
    l.overlay = l
        .overlay
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty());
    if let Some(o) = &l.overlay {
        check_len("overlay", o, MAX_MESSAGE_CHARS)?;
    }
    Ok(l)
}

/// Bumps the version when something observable changed.
fn finish(
    state: &PresentationState,
    mut next: PresentationState,
    events: Vec<Event>,
) -> Result<Applied, CoreError> {
    if next == *state {
        // Idempotent repeat: nothing observable changed.
        return Ok((next, Vec::new()));
    }
    next.version = state.version + 1;
    Ok((next, events))
}

/// The epoch is checked first: a version from another boot is meaningless, so it must never be
/// compared with this boot's counter.
fn check_version(
    state: &PresentationState,
    expected: Option<u64>,
    epoch: Option<u64>,
) -> Result<(), CoreError> {
    if let Some(e) = epoch.filter(|e| *e != state.epoch) {
        return Err(CoreError::EpochMismatch {
            expected: e,
            actual: state.epoch,
        });
    }
    match expected {
        Some(e) if e != state.version => Err(CoreError::VersionMismatch {
            expected: e,
            actual: state.version,
        }),
        _ => Ok(()),
    }
}

fn timer_mut<'a>(
    s: &'a mut PresentationState,
    id: &crate::ids::TimerId,
) -> Result<&'a mut crate::state::Timer, CoreError> {
    s.timers
        .iter_mut()
        .find(|t| &t.id == id)
        .ok_or_else(|| CoreError::UnknownTimer(id.to_string()))
}

/// Where `Next` should land. When live is ad-hoc content (a Bible passage) and the operator has
/// a preview from the show, `Next` takes that preview; otherwise it starts the show.
fn next_target(state: &PresentationState) -> Option<(usize, usize)> {
    match state.live_position() {
        Some(pos) => state.position_after(Some(pos)),
        None => state
            .preview
            .as_ref()
            .and_then(|c| locate(state, c))
            .or_else(|| state.position_after(None)),
    }
}

fn locate(state: &PresentationState, cue: &Cue) -> Option<(usize, usize)> {
    let item = cue.item_id.as_ref()?;
    let ei = state.show.iter().position(|e| &e.item_id == item)?;
    ((cue.slide_index as usize) < state.show[ei].slides.len())
        .then_some((ei, cue.slide_index as usize))
}

/// Puts a show slide live, makes content visible again, and queues the following slide as preview.
fn go_live(next: &mut PresentationState, events: &mut Vec<Event>, pos: (usize, usize)) {
    let cue = next.cue_at(pos);
    set_live(next, events, cue);
    let following = next.position_after(Some(pos)).map(|p| next.cue_at(p));
    set_preview(next, events, following);
}

fn set_live(next: &mut PresentationState, events: &mut Vec<Event>, cue: Cue) {
    // Showing content implies the operator wants to see it: lift "clear" and "logo".
    // Blackout is an explicit safety state and is only lifted by `Blackout { on: false }`.
    if next.clear {
        next.clear = false;
        events.push(Event::ClearChanged { on: false });
    }
    if next.logo {
        next.logo = false;
        events.push(Event::LogoChanged { on: false });
    }
    if next.live.as_ref() != Some(&cue) {
        events.push(Event::SlideChanged {
            target: Target::Live,
            item_id: cue.item_id.clone(),
            slide_index: cue.slide_index,
        });
        next.live = Some(cue);
    }
}

fn set_preview(next: &mut PresentationState, events: &mut Vec<Event>, cue: Option<Cue>) {
    if next.preview != cue {
        if let Some(c) = &cue {
            events.push(Event::SlideChanged {
                target: Target::Preview,
                item_id: c.item_id.clone(),
                slide_index: c.slide_index,
            });
        }
        next.preview = cue;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bible::{ParallelLayout, Passage, VersionCode};
    use crate::ids::{ItemId, TimerId};
    use crate::state::{ShowEntry, Slide, SlideKind, Timer, TimerKind};

    fn apply_at(
        s: &PresentationState,
        now_ms: i64,
        c: impl Into<EngineCommand>,
    ) -> Result<Applied, CoreError> {
        super::apply(s, Ctx { now_ms }, c.into())
    }

    fn apply(s: &PresentationState, c: impl Into<EngineCommand>) -> Result<Applied, CoreError> {
        apply_at(s, 0, c)
    }

    fn entry(id: &str, n: usize) -> ShowEntry {
        ShowEntry {
            item_id: ItemId::new(id),
            title: id.into(),
            duration_secs: None,
            slides: (0..n)
                .map(|i| Slide::lyrics(format!("{id}:{i}"), None, &[&format!("{id} line {i}")]))
                .collect(),
        }
    }

    fn loaded() -> PresentationState {
        let s = PresentationState::default();
        apply(
            &s,
            EngineCommand::LoadShow {
                entries: vec![entry("a", 2), entry("empty", 0), entry("b", 2)],
            },
        )
        .unwrap()
        .0
    }

    fn run(s: &PresentationState, c: impl Into<EngineCommand>) -> PresentationState {
        apply(s, c).unwrap().0
    }

    fn next(v: Option<u64>) -> Command {
        Command::Next {
            expected_version: v,
            epoch: None,
        }
    }

    #[test]
    fn next_walks_the_whole_show_skipping_empty_items() {
        let mut s = loaded();
        let mut seen = vec![];
        for _ in 0..4 {
            s = run(&s, next(None));
            seen.push(s.live.as_ref().unwrap().slide.id.to_string());
        }
        assert_eq!(seen, ["a:0", "a:1", "b:0", "b:1"]);
        // End of show: no-op, version untouched.
        let v = s.version;
        let (s2, ev) = apply(&s, next(None)).unwrap();
        assert_eq!(s2.version, v);
        assert!(ev.is_empty());
    }

    #[test]
    fn next_queues_following_slide_as_preview() {
        let s = run(&loaded(), next(None));
        assert_eq!(s.preview.as_ref().unwrap().slide.id.as_str(), "a:1");
        let s = run(&s, next(None));
        assert_eq!(s.preview.as_ref().unwrap().slide.id.as_str(), "b:0");
    }

    #[test]
    fn prev_goes_back_and_stops_at_start() {
        let mut s = loaded();
        s = run(
            &s,
            Command::GoTo {
                item_id: "b".into(),
                slide_index: 0,
                target: Target::Live,
            },
        );
        s = run(
            &s,
            Command::Prev {
                expected_version: None,
                epoch: None,
            },
        );
        assert_eq!(s.live.as_ref().unwrap().slide.id.as_str(), "a:1");
        s = run(
            &s,
            Command::Prev {
                expected_version: None,
                epoch: None,
            },
        );
        s = run(
            &s,
            Command::Prev {
                expected_version: None,
                epoch: None,
            },
        );
        assert_eq!(s.live.as_ref().unwrap().slide.id.as_str(), "a:0");
    }

    #[test]
    fn epoch_mismatch_is_rejected_before_the_version_is_compared() {
        let mut s = run(&loaded(), next(None));
        s.epoch = 42;
        // The version matches by accident (the restart scenario), the epoch does not.
        for forward in [true, false] {
            let cmd = if forward {
                Command::Next {
                    expected_version: Some(s.version),
                    epoch: Some(7),
                }
            } else {
                Command::Prev {
                    expected_version: Some(s.version),
                    epoch: Some(7),
                }
            };
            let err = apply(&s, cmd).unwrap_err();
            assert_eq!(
                err,
                CoreError::EpochMismatch {
                    expected: 7,
                    actual: 42
                }
            );
            assert_eq!(err.code(), "epoch_mismatch");
        }
        let ok = Command::Next {
            expected_version: Some(s.version),
            epoch: Some(42),
        };
        assert!(apply(&s, ok).is_ok());
        // A local caller that sends no epoch is not checked.
        assert!(apply(&s, next(Some(s.version))).is_ok());
    }

    #[test]
    fn stale_expected_version_is_rejected_and_state_untouched() {
        let s = run(&loaded(), next(None));
        let err = apply(&s, next(Some(s.version - 1))).unwrap_err();
        assert_eq!(
            err,
            CoreError::VersionMismatch {
                expected: s.version - 1,
                actual: s.version
            }
        );
        // Double tap with the same expected version: second one fails, no double jump.
        let v = s.version;
        let s2 = run(&s, next(Some(v)));
        assert!(apply(&s2, next(Some(v))).is_err());
    }

    #[test]
    fn version_increments_by_one_per_change() {
        let s = loaded();
        let v = s.version;
        let s = run(&s, next(None));
        assert_eq!(s.version, v + 1);
    }

    #[test]
    fn absolute_commands_are_idempotent() {
        let s = run(&loaded(), Command::Blackout { on: true });
        let (s2, ev) = apply(&s, Command::Blackout { on: true }).unwrap();
        assert_eq!(s2, s);
        assert!(ev.is_empty());
        let goto = Command::GoTo {
            item_id: "a".into(),
            slide_index: 1,
            target: Target::Live,
        };
        let s3 = run(&s, goto.clone());
        let (s4, ev) = apply(&s3, goto).unwrap();
        assert_eq!(s4.version, s3.version);
        assert!(ev.is_empty());
    }

    #[test]
    fn goto_validates_item_and_range() {
        let s = loaded();
        assert!(matches!(
            apply(
                &s,
                Command::GoTo {
                    item_id: "zzz".into(),
                    slide_index: 0,
                    target: Target::Live
                }
            ),
            Err(CoreError::UnknownItem(_))
        ));
        assert!(matches!(
            apply(
                &s,
                Command::GoTo {
                    item_id: "a".into(),
                    slide_index: 9,
                    target: Target::Live
                }
            ),
            Err(CoreError::SlideOutOfRange { slide_count: 2, .. })
        ));
    }

    #[test]
    fn showing_content_lifts_clear_and_logo_but_not_blackout() {
        let mut s = loaded();
        s = run(&s, Command::Clear { on: true });
        s = run(&s, Command::Logo { on: true });
        s = run(&s, Command::Blackout { on: true });
        s = run(&s, next(None));
        assert!(!s.clear && !s.logo && s.blackout);
    }

    #[test]
    fn preview_then_take() {
        let mut s = loaded();
        assert_eq!(
            apply(&s, Command::TakePreview).unwrap_err(),
            CoreError::NothingInPreview
        );
        s = run(
            &s,
            Command::ShowItem {
                item_id: "b".into(),
                target: Target::Preview,
            },
        );
        assert!(s.live.is_none());
        s = run(&s, Command::TakePreview);
        assert_eq!(s.live.as_ref().unwrap().slide.id.as_str(), "b:0");
    }

    fn bible_cmd(slide: Slide) -> EngineCommand {
        EngineCommand::ShowBible {
            layout: ParallelLayout::Columns,
            target: Target::Live,
            slide,
        }
    }

    #[test]
    fn wire_show_bible_is_refused_by_the_reducer() {
        let s = loaded();
        let wire = Command::ShowBible {
            passage: Passage::parse_osis("John.3.16").unwrap(),
            versions: vec![VersionCode("RVR1909".into())],
            layout: ParallelLayout::Columns,
            target: Target::Live,
        };
        assert_eq!(apply(&s, wire).unwrap_err(), CoreError::BibleNotResolved);
    }

    #[test]
    fn host_only_and_unknown_commands_are_unsupported() {
        let s = loaded();
        for c in [
            Command::RefreshShow,
            Command::SetActivePlaylist { id: "p".into() },
            Command::Unknown,
        ] {
            assert!(matches!(apply(&s, c), Err(CoreError::Unsupported(_))));
        }
    }

    #[test]
    fn host_resolved_bible_goes_live() {
        let s = loaded();
        let mut slide = Slide::blank("bible");
        slide.kind = SlideKind::Bible;
        let s = run(&s, bible_cmd(slide));
        let live = s.live.unwrap();
        assert!(live.item_id.is_none());
        assert_eq!(live.slide.kind, SlideKind::Bible);
    }

    #[test]
    fn next_after_adhoc_bible_resumes_show() {
        let mut s = loaded();
        s = run(&s, bible_cmd(Slide::blank("bible")));
        s = run(&s, next(None));
        assert_eq!(s.live.unwrap().slide.id.as_str(), "a:0");
    }

    #[test]
    fn stage_message_set_trim_and_clear() {
        let mut s = loaded();
        s = run(
            &s,
            Command::StageMessage {
                text: Some("  Cierre en 2 min ".into()),
            },
        );
        assert_eq!(s.stage_message.as_ref().unwrap().text, "Cierre en 2 min");
        s = run(
            &s,
            Command::StageMessage {
                text: Some("   ".into()),
            },
        );
        assert!(s.stage_message.is_none());
    }

    #[test]
    fn live_edit_updates_visible_slide_without_moving() {
        let mut s = loaded();
        s = run(
            &s,
            Command::GoTo {
                item_id: "a".into(),
                slide_index: 0,
                target: Target::Live,
            },
        );
        let rev = s.show_revision;
        let fixed = Slide::lyrics("a:0", None, &["corregido"]);
        let (s2, ev) = apply(
            &s,
            Command::UpdateSlide {
                item_id: "a".into(),
                slide_index: 0,
                slide: fixed.clone(),
            },
        )
        .unwrap();
        assert_eq!(s2.live.as_ref().unwrap().slide, fixed);
        assert_eq!(s2.live.as_ref().unwrap().slide_index, 0);
        assert!(s2.show_revision > rev);
        assert!(ev.iter().any(|e| matches!(e, Event::LiveEdited { .. })));
    }

    #[test]
    fn load_show_keeps_live_on_screen() {
        let mut s = loaded();
        s = run(&s, next(None));
        let live = s.live.clone();
        s = run(
            &s,
            EngineCommand::LoadShow {
                entries: vec![entry("zzz", 1)],
            },
        );
        assert_eq!(s.live, live);
        assert!(s.preview.is_none());
        assert!(s.live_position().is_none());
    }

    #[test]
    fn timers_run_pause_reset() {
        let mut s = loaded();
        let id = || TimerId::new("svc");
        let t = Timer::new("svc", "Culto", TimerKind::Countdown, 60);
        s = run(&s, Command::SetTimer { timer: t });
        s = apply_at(&s, 1_000, Command::StartTimer { id: id() })
            .unwrap()
            .0;
        assert_eq!(s.timers[0].remaining_at(11_000), Some(50_000));
        s = apply_at(&s, 11_000, Command::PauseTimer { id: id() })
            .unwrap()
            .0;
        assert_eq!(s.timers[0].remaining_at(99_000), Some(50_000));
        s = apply_at(&s, 20_000, Command::StartTimer { id: id() })
            .unwrap()
            .0;
        assert_eq!(s.timers[0].remaining_at(25_000), Some(45_000));
        s = run(&s, Command::ResetTimer { id: id() });
        assert_eq!(s.timers[0].remaining_at(0), Some(60_000));
        assert!(matches!(
            apply(
                &s,
                Command::StartTimer {
                    id: TimerId::new("nope")
                }
            ),
            Err(CoreError::UnknownTimer(_))
        ));
    }

    #[test]
    fn set_timer_ignores_client_supplied_run_state() {
        let mut forged = Timer::new("svc", "Culto", TimerKind::Countdown, 60);
        forged.run = TimerRun::Running;
        forged.started_at_ms = Some(1);
        forged.elapsed_ms = 59_000;
        let s = run(
            &loaded(),
            Command::SetTimer {
                timer: forged.clone(),
            },
        );
        assert_eq!(s.timers[0].run, TimerRun::Stopped);
        assert_eq!(s.timers[0].elapsed_ms, 0);
        // Renaming a running timer keeps its run state.
        let s = apply_at(&s, 5_000, Command::StartTimer { id: "svc".into() })
            .unwrap()
            .0;
        forged.label = "Nuevo".into();
        let s = run(&s, Command::SetTimer { timer: forged });
        assert_eq!(s.timers[0].label, "Nuevo");
        assert_eq!(s.timers[0].run, TimerRun::Running);
        assert_eq!(s.timers[0].started_at_ms, Some(5_000));
    }

    #[test]
    fn restore_sets_live_and_timers_once() {
        let live = Cue {
            item_id: None,
            slide_index: 0,
            slide: Slide::blank("x"),
        };
        let r = Restore {
            live: Some(live.clone()),
            timers: vec![Timer::new("t", "t", TimerKind::Stopwatch, 0)],
            service_mode: true,
        };
        let s = run(&PresentationState::default(), EngineCommand::Restore(r));
        assert_eq!(s.live, Some(live));
        assert!(s.service_mode);
        assert_eq!(s.version, 1);
    }

    #[test]
    fn countdown_goes_negative_when_overrun() {
        let mut t = Timer::new("t", "t", TimerKind::Countdown, 10);
        t.run = TimerRun::Running;
        t.started_at_ms = Some(0);
        assert_eq!(t.remaining_at(12_000), Some(-2_000));
    }

    #[test]
    fn rejected_commands_do_not_change_state() {
        let s = loaded();
        let before = s.clone();
        let _ = apply(
            &s,
            Command::RemoveTimer {
                id: TimerId::new("x"),
            },
        );
        assert_eq!(s, before);
    }

    // ---- outputs, layers, frames ----

    use crate::ids::{MediaId, OutputId};
    use crate::state::{OutputKind, OutputLayers, OutputLook};

    fn lobby_look() -> OutputLook {
        OutputLook {
            kind: OutputKind::Secondary,
            theme_id: None,
            enabled: true,
            follows_blackout: true,
        }
    }

    fn with_lobby(s: &PresentationState) -> PresentationState {
        run(
            s,
            Command::SetOutputLook {
                output: "lobby".into(),
                look: lobby_look(),
            },
        )
    }

    #[test]
    fn new_output_gets_default_layers_and_look_is_idempotent() {
        let s = with_lobby(&loaded());
        assert_eq!(
            s.output_layers.get(&OutputId::new("lobby")),
            Some(&OutputLayers::default())
        );
        let (s2, ev) = apply(
            &s,
            Command::SetOutputLook {
                output: "lobby".into(),
                look: lobby_look(),
            },
        )
        .unwrap();
        assert_eq!(s2, s);
        assert!(ev.is_empty());
        assert!(matches!(
            apply(
                &s,
                Command::SetOutputLook {
                    output: " ".into(),
                    look: lobby_look()
                }
            ),
            Err(CoreError::Invalid(_))
        ));
    }

    #[test]
    fn layers_require_a_known_output_and_are_independent() {
        let s = with_lobby(&loaded());
        let layers = OutputLayers {
            background: Some(MediaId::new("bg")),
            overlay: Some("  Bienvenidos  ".into()),
            blackout: true,
            ..OutputLayers::default()
        };
        assert!(matches!(
            apply(
                &s,
                Command::SetOutputLayers {
                    output: "nope".into(),
                    layers: layers.clone()
                }
            ),
            Err(CoreError::UnknownOutput(_))
        ));
        let cmd = Command::SetOutputLayers {
            output: "lobby".into(),
            layers,
        };
        let s2 = run(&s, cmd.clone());
        let lobby = &s2.output_layers[&OutputId::new("lobby")];
        assert_eq!(lobby.overlay.as_deref(), Some("Bienvenidos"));
        assert_eq!(
            s2.output_layers[&OutputId::new("main")],
            OutputLayers::default()
        );
        let (s3, ev) = apply(&s2, cmd).unwrap();
        assert_eq!(s3.version, s2.version);
        assert!(ev.is_empty());
        let too_long = Command::SetOutputLayers {
            output: "lobby".into(),
            layers: OutputLayers {
                overlay: Some("x".repeat(MAX_MESSAGE_CHARS + 1)),
                ..OutputLayers::default()
            },
        };
        assert!(matches!(apply(&s2, too_long), Err(CoreError::Invalid(_))));
    }

    #[test]
    fn frame_combines_global_flags_look_and_layers() {
        let mut s = with_lobby(&loaded());
        s = run(&s, next(None));
        let (main, stage, lobby) = (
            OutputId::new("main"),
            OutputId::new("stage"),
            OutputId::new("lobby"),
        );
        // Stage is disabled by default.
        assert!(s.view().frame_for(&stage).is_none());
        assert!(s.view().frame_for(&OutputId::new("ghost")).is_none());
        assert!(s.view().frame_for(&main).unwrap().show_text);

        // Output-only blackout leaves the others alone.
        s = run(
            &s,
            Command::SetOutputLayers {
                output: lobby.clone(),
                layers: OutputLayers {
                    blackout: true,
                    ..OutputLayers::default()
                },
            },
        );
        let v = s.view();
        assert!(v.frame_for(&lobby).unwrap().blackout);
        assert!(!v.frame_for(&main).unwrap().blackout);

        // Global blackout: the stage ignores it when it does not follow blackout.
        s = run(
            &s,
            Command::SetOutputLook {
                output: stage.clone(),
                look: OutputLook {
                    kind: OutputKind::Stage,
                    theme_id: None,
                    enabled: true,
                    follows_blackout: false,
                },
            },
        );
        s = run(&s, Command::Blackout { on: true });
        let v = s.view();
        assert!(v.frame_for(&main).unwrap().blackout);
        assert!(!v.frame_for(&main).unwrap().show_text);
        assert!(!v.frame_for(&stage).unwrap().blackout);
        assert!(v.frame_for(&stage).unwrap().show_text);

        // Clear hides text only; the text layer hides it per output.
        s = run(&s, Command::Blackout { on: false });
        s = run(&s, Command::Clear { on: true });
        assert!(!s.view().frame_for(&main).unwrap().show_text);
        s = run(&s, Command::Clear { on: false });
        s = run(
            &s,
            Command::SetOutputLayers {
                output: main.clone(),
                layers: OutputLayers {
                    text: false,
                    media: Some(MediaId::new("m")),
                    ..OutputLayers::default()
                },
            },
        );
        let f = s.view().frame_for(&main).unwrap();
        assert!(!f.show_text);
        assert_eq!(f.media, Some(MediaId::new("m")));
        assert!(f.cue.is_some());
    }

    #[test]
    fn remove_output_drops_look_and_layers_but_never_main() {
        let s = with_lobby(&loaded());
        assert!(matches!(
            apply(
                &s,
                Command::RemoveOutput {
                    output: "main".into()
                }
            ),
            Err(CoreError::Invalid(_))
        ));
        assert!(matches!(
            apply(
                &s,
                Command::RemoveOutput {
                    output: "ghost".into()
                }
            ),
            Err(CoreError::UnknownOutput(_))
        ));
        let s2 = run(
            &s,
            Command::RemoveOutput {
                output: "lobby".into(),
            },
        );
        assert!(!s2.outputs.contains_key(&OutputId::new("lobby")));
        assert!(!s2.output_layers.contains_key(&OutputId::new("lobby")));
        assert_eq!(s2.outputs.len(), s2.output_layers.len());
    }

    // ---- per-item clock ----

    fn timed_show() -> PresentationState {
        let mut a = entry("a", 2);
        a.duration_secs = Some(60);
        let mut b = entry("b", 1);
        b.duration_secs = Some(10);
        let c = entry("c", 1);
        run(
            &PresentationState::default(),
            EngineCommand::LoadShow {
                entries: vec![a, b, c],
            },
        )
    }

    #[test]
    fn item_clock_starts_on_item_change_not_slide_change() {
        let s = timed_show();
        let s = apply_at(&s, 1_000, next(None)).unwrap().0;
        let clock = s.item_clock.clone().unwrap();
        assert_eq!(
            (
                clock.item_id.as_str(),
                clock.planned_secs,
                clock.started_at_ms
            ),
            ("a", 60, 1_000)
        );
        // Next slide of the same item keeps the clock.
        let s2 = apply_at(&s, 30_000, next(None)).unwrap().0;
        assert_eq!(s2.item_clock, s.item_clock);
        // New item restarts it with the new duration.
        let (s3, ev) = apply_at(&s2, 40_000, next(None)).unwrap();
        let c3 = s3.item_clock.clone().unwrap();
        assert_eq!(
            (c3.item_id.as_str(), c3.planned_secs, c3.started_at_ms),
            ("b", 10, 40_000)
        );
        assert!(ev
            .iter()
            .any(|e| matches!(e, Event::ItemClockChanged { .. })));
        // An item without duration has no clock.
        let s4 = apply_at(&s3, 41_000, next(None)).unwrap().0;
        assert!(s4.item_clock.is_none());
    }

    #[test]
    fn item_clock_overtime_and_reset() {
        let s = apply_at(&timed_show(), 0, next(None)).unwrap().0;
        let c = s.item_clock.clone().unwrap();
        assert_eq!(c.remaining_ms_at(45_000), 15_000);
        assert_eq!(c.overtime_ms_at(45_000), 0);
        assert_eq!(c.overtime_ms_at(75_000), 15_000);
        assert_eq!(c.elapsed_ms_at(-5), 0);
        let s2 = apply_at(&s, 75_000, Command::ResetItemClock).unwrap().0;
        assert_eq!(s2.item_clock.unwrap().started_at_ms, 75_000);
        // No clock: reset is a harmless no-op.
        let none = apply(&loaded(), Command::ResetItemClock).unwrap();
        assert!(none.1.is_empty());
    }

    #[test]
    fn adhoc_bible_drops_the_item_clock() {
        let s = apply_at(&timed_show(), 0, next(None)).unwrap().0;
        let s = run(&s, bible_cmd(Slide::blank("bible")));
        assert!(s.item_clock.is_none());
    }

    #[test]
    fn restore_restarts_the_clock_of_the_restored_item() {
        let s = timed_show();
        let live = s.cue_at((0, 1));
        let r = Restore {
            live: Some(live),
            timers: vec![],
            service_mode: false,
        };
        let s = apply_at(&s, 9_000, EngineCommand::Restore(r)).unwrap().0;
        assert_eq!(s.item_clock.unwrap().started_at_ms, 9_000);
    }

    #[test]
    fn timer_overtime_helper() {
        let mut t = Timer::new("t", "t", TimerKind::Countdown, 10);
        t.run = TimerRun::Running;
        t.started_at_ms = Some(0);
        assert_eq!(t.overtime_ms_at(4_000), Some(0));
        assert_eq!(t.overtime_ms_at(12_500), Some(2_500));
        let sw = Timer::new("s", "s", TimerKind::Stopwatch, 0);
        assert_eq!(sw.overtime_ms_at(1), None);
    }

    // ---- modo culto ----

    #[test]
    fn service_mode_locks_edits_but_not_operation() {
        let mut s = loaded();
        s = run(&s, next(None));
        s = run(&s, Command::ServiceMode { on: true });
        let locked: Vec<Command> = vec![
            Command::UpdateSlide {
                item_id: "a".into(),
                slide_index: 0,
                slide: Slide::lyrics("a:0", None, &["x"]),
            },
            Command::SetOutputLook {
                output: "lobby".into(),
                look: lobby_look(),
            },
            Command::RemoveOutput {
                output: "stage".into(),
            },
        ];
        for c in locked {
            let before = s.clone();
            assert_eq!(
                apply(&s, c.clone()).unwrap_err(),
                CoreError::Locked,
                "{c:?}"
            );
            assert_eq!(s, before);
        }
        // Operating the service stays possible.
        for c in [
            next(None),
            Command::Blackout { on: true },
            Command::Clear { on: true },
            Command::Logo { on: true },
            Command::StageMessage {
                text: Some("hola".into()),
            },
            Command::SetOutputLayers {
                output: "main".into(),
                layers: OutputLayers {
                    text: false,
                    ..OutputLayers::default()
                },
            },
            // Timers stay editable: a prayer timer is added mid-service.
            Command::SetTimer {
                timer: Timer::new("t", "t", TimerKind::Stopwatch, 0),
            },
        ] {
            assert!(apply(&s, c).is_ok());
        }
        // Unlock, then edits work again.
        s = run(&s, Command::ServiceMode { on: false });
        assert!(apply(
            &s,
            Command::UpdateSlide {
                item_id: "a".into(),
                slide_index: 0,
                slide: Slide::lyrics("a:0", None, &["x"]),
            }
        )
        .is_ok());
    }

    #[test]
    fn host_commands_ignore_the_lock() {
        let s = run(&loaded(), Command::ServiceMode { on: true });
        let s = run(
            &s,
            EngineCommand::LoadShow {
                entries: vec![entry("z", 1)],
            },
        );
        assert_eq!(s.show.len(), 1);
    }

    // ---- validation ----

    #[test]
    fn oversized_inputs_are_rejected() {
        let s = loaded();
        assert!(matches!(
            apply(
                &s,
                Command::StageMessage {
                    text: Some("x".repeat(MAX_MESSAGE_CHARS + 1))
                }
            ),
            Err(CoreError::Invalid(_))
        ));
        assert!(apply(
            &s,
            Command::StageMessage {
                text: Some("x".repeat(MAX_MESSAGE_CHARS))
            }
        )
        .is_ok());
        let mut t = Timer::new("t", "t", TimerKind::Countdown, MAX_TIMER_SECS + 1);
        assert!(matches!(
            apply(&s, Command::SetTimer { timer: t.clone() }),
            Err(CoreError::Invalid(_))
        ));
        t.duration_secs = MAX_TIMER_SECS;
        assert!(apply(&s, Command::SetTimer { timer: t.clone() }).is_ok());
        t.label = "l".repeat(200);
        assert!(apply(&s, Command::SetTimer { timer: t }).is_err());
    }

    // ---- idempotency of every absolute command ----

    #[test]
    fn every_absolute_command_is_idempotent_and_rejections_are_pure() {
        let base = with_lobby(&run(&loaded(), next(None)));
        let timer = Timer::new("t", "t", TimerKind::Countdown, 30);
        let cmds: Vec<Command> = vec![
            Command::GoTo {
                item_id: "b".into(),
                slide_index: 1,
                target: Target::Live,
            },
            Command::GoTo {
                item_id: "b".into(),
                slide_index: 0,
                target: Target::Preview,
            },
            Command::ShowItem {
                item_id: "b".into(),
                target: Target::Live,
            },
            Command::Blackout { on: true },
            Command::Clear { on: true },
            Command::Logo { on: true },
            Command::StageMessage {
                text: Some("hola".into()),
            },
            Command::UpdateSlide {
                item_id: "a".into(),
                slide_index: 1,
                slide: Slide::lyrics("a:1", None, &["n"]),
            },
            Command::SetTimer { timer },
            Command::SetOutputLook {
                output: "x".into(),
                look: lobby_look(),
            },
            Command::SetOutputLayers {
                output: "lobby".into(),
                layers: OutputLayers {
                    overlay: Some("o".into()),
                    ..OutputLayers::default()
                },
            },
            Command::ServiceMode { on: true },
            Command::ResetItemClock,
        ];
        for c in cmds {
            let once = apply_at(&base, 5, c.clone()).unwrap().0;
            let (twice, ev) = apply_at(&once, 9, c.clone()).unwrap();
            assert_eq!(twice, once, "{c:?} must be idempotent");
            assert!(ev.is_empty(), "{c:?} repeated must emit nothing");
        }
        for bad in [
            Command::GoTo {
                item_id: "zzz".into(),
                slide_index: 0,
                target: Target::Live,
            },
            Command::StartTimer { id: "ghost".into() },
            Command::PauseTimer { id: "ghost".into() },
            Command::ResetTimer { id: "ghost".into() },
            Command::RemoveTimer { id: "ghost".into() },
            Command::TakePreview.clone(),
        ] {
            let mut s = base.clone();
            s.preview = None;
            let before = s.clone();
            if apply(&s, bad).is_err() {
                assert_eq!(s, before);
            }
        }
    }

    #[test]
    fn timer_commands_are_idempotent_at_the_same_clock() {
        let s = run(
            &loaded(),
            Command::SetTimer {
                timer: Timer::new("t", "t", TimerKind::Countdown, 30),
            },
        );
        let started = apply_at(&s, 100, Command::StartTimer { id: "t".into() })
            .unwrap()
            .0;
        let again = apply_at(&started, 900, Command::StartTimer { id: "t".into() }).unwrap();
        assert_eq!(
            again.0, started,
            "start while running keeps the original start"
        );
        assert!(again.1.is_empty());
        let paused = apply_at(&started, 200, Command::PauseTimer { id: "t".into() })
            .unwrap()
            .0;
        let again = apply_at(&paused, 999, Command::PauseTimer { id: "t".into() }).unwrap();
        assert_eq!(again.0, paused);
        let reset = run(&paused, Command::ResetTimer { id: "t".into() });
        assert_eq!(run(&reset, Command::ResetTimer { id: "t".into() }), reset);
    }

    // ---- property tests ----

    mod props {
        use super::*;
        use proptest::prelude::*;

        fn total_slides(s: &PresentationState) -> usize {
            s.show.iter().map(|e| e.slides.len()).sum()
        }

        fn flat_index(s: &PresentationState) -> Option<usize> {
            let (ei, si) = s.live_position()?;
            Some(s.show[..ei].iter().map(|e| e.slides.len()).sum::<usize>() + si)
        }

        proptest! {
            /// N clients that all read version v and press Next with expectedVersion = v: exactly
            /// one wins, whatever the arrival order, and live moves by exactly one slide.
            #[test]
            fn concurrent_next_advances_exactly_one_slide(
                clients in 2usize..12,
                start in 0usize..3,
                order_seed in any::<u64>(),
            ) {
                let mut s = loaded();
                for _ in 0..start {
                    s = run(&s, next(None));
                }
                let v = s.version;
                let before = flat_index(&s);
                let mut order: Vec<usize> = (0..clients).collect();
                // Deterministic shuffle from the seed.
                let mut x = order_seed | 1;
                for i in (1..order.len()).rev() {
                    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
                    order.swap(i, (x % (i as u64 + 1)) as usize);
                }
                let mut wins = 0;
                for _ in order {
                    match apply(&s, next(Some(v))) {
                        Ok((n, _)) => { s = n; wins += 1; }
                        Err(e) => prop_assert_eq!(e.code(), "version_mismatch"),
                    }
                }
                // `start < 4` keeps live before the last slide, so the winner always moves.
                prop_assert_eq!(wins, 1);
                let moved = match (before, flat_index(&s)) {
                    (Some(a), Some(b)) => b - a,
                    (None, Some(b)) => b + 1,
                    _ => 0,
                };
                prop_assert_eq!(moved, 1);
            }

            /// Same race with a Prev mixed in: still at most one relative command succeeds per
            /// observed version.
            #[test]
            fn mixed_relative_commands_succeed_once_per_version(
                kinds in proptest::collection::vec(any::<bool>(), 2..10),
            ) {
                let mut s = run(&run(&loaded(), next(None)), next(None));
                let v = s.version;
                let mut ok = 0;
                for forward in kinds {
                    let c = if forward {
                        next(Some(v))
                    } else {
                        Command::Prev { expected_version: Some(v), epoch: None }
                    };
                    if let Ok((n, _)) = apply(&s, c) { s = n; ok += 1; }
                }
                prop_assert_eq!(ok, 1);
                prop_assert_eq!(s.version, v + 1);
            }

            /// Arbitrary command streams: version only grows by 0 or 1, rejected commands never
            /// touch the state, and live always points at valid content.
            #[test]
            fn random_streams_keep_the_invariants(ops in proptest::collection::vec(0u8..16, 1..60)) {
                let mut s = timed_show();
                let mut now = 0i64;
                for op in ops {
                    now += 700;
                    let cmd = match op {
                        0..=2 => next(None),
                        3 => Command::Prev { expected_version: None, epoch: None },
                        4 => Command::Blackout { on: op % 2 == 0 },
                        5 => Command::Clear { on: true },
                        6 => Command::Logo { on: true },
                        7 => Command::GoTo { item_id: "b".into(), slide_index: 0, target: Target::Live },
                        8 => Command::GoTo { item_id: "ghost".into(), slide_index: 0, target: Target::Live },
                        9 => Command::TakePreview,
                        10 => Command::SetTimer { timer: Timer::new("t", "t", TimerKind::Countdown, 5) },
                        11 => Command::StartTimer { id: "t".into() },
                        12 => Command::PauseTimer { id: "t".into() },
                        13 => Command::ServiceMode { on: now % 2 == 0 },
                        14 => Command::RemoveTimer { id: "t".into() },
                        _ => Command::ResetItemClock,
                    };
                    if let Ok((n, ev)) = apply_at(&s, now, cmd) {
                        {
                            prop_assert!(n.version == s.version || n.version == s.version + 1);
                            if n.version == s.version {
                                prop_assert!(n == s && ev.is_empty());
                            } else {
                                prop_assert!(!ev.is_empty());
                            }
                            s = n;
                        }
                    }
                    if s.live.is_some() && s.live.as_ref().unwrap().item_id.is_some() {
                        prop_assert!(s.live_position().is_some());
                    }
                    prop_assert_eq!(s.outputs.len(), s.output_layers.len());
                    prop_assert!(flat_index(&s).is_none_or(|i| i < total_slides(&s)));
                    if let Some(c) = &s.item_clock {
                        prop_assert_eq!(
                            s.live.as_ref().and_then(|l| l.item_id.as_ref()),
                            Some(&c.item_id)
                        );
                    }
                }
            }
        }
    }
}
