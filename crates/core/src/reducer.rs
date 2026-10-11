//! The pure presentation reducer: `apply(state, command) -> (new state, events)`.
//!
//! Rules (PLAN §3.1, §3.4):
//! * Rejected commands never change state and never bump the version.
//! * Absolute commands (`GoTo`, `Blackout`, ...) are idempotent: repeating one that is already
//!   satisfied is a no-op (same version, no events).
//! * Relative commands (`Next`, `Prev`) are rejected with [`CoreError::VersionMismatch`] when
//!   `expected_version` is set and stale, so retries and double taps cannot skip slides.
//! * Every accepted, state-changing command increments `version` by exactly one.
//! * Time never comes from the system clock; commands carry `at_ms`.

use crate::command::Command;
use crate::error::CoreError;
use crate::event::Event;
use crate::state::{Cue, PresentationState, StageMessage, Target, TimerRun};

pub type Applied = (PresentationState, Vec<Event>);

/// Applies `cmd` to `state`, returning the successor state and the events it produced.
pub fn apply(state: &PresentationState, cmd: Command) -> Result<Applied, CoreError> {
    let mut next = state.clone();
    let mut events = Vec::new();

    match cmd {
        Command::Next { expected_version } => {
            check_version(state, expected_version)?;
            if let Some(pos) = next_target(state) {
                go_live(&mut next, &mut events, pos);
            }
        }
        Command::Prev { expected_version } => {
            check_version(state, expected_version)?;
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
                Command::GoTo {
                    item_id,
                    slide_index: 0,
                    target,
                },
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
        Command::ShowBible {
            passage: _,
            versions,
            layout,
            target,
            resolved,
        } => {
            if versions.is_empty() {
                return Err(CoreError::Invalid(
                    "ShowBible needs at least one version".into(),
                ));
            }
            let mut slide = resolved.ok_or(CoreError::BibleNotResolved)?;
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
        }
        Command::StageMessage { text } => {
            let msg = text
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .map(|text| StageMessage { text });
            if next.stage_message != msg {
                events.push(Event::StageMessageChanged {
                    text: msg.as_ref().map(|m| m.text.clone()),
                });
                next.stage_message = msg;
            }
        }
        Command::LoadShow { entries } => {
            if next.show != entries {
                events.push(Event::ShowLoaded {
                    entries: entries.len() as u32,
                });
                next.show = entries;
                next.show_revision += 1;
                // Preview pointed into the old show; live keeps its own copy and stays on screen.
                next.preview = None;
            }
        }
        Command::UpdateSlide {
            item_id,
            slide_index,
            slide,
        } => {
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
            let id = timer.id.clone();
            match next.timers.iter_mut().find(|t| t.id == id) {
                Some(existing) if *existing == timer => {}
                Some(existing) => {
                    *existing = timer;
                    events.push(Event::TimerChanged { id });
                }
                None => {
                    next.timers.push(timer);
                    events.push(Event::TimerChanged { id });
                }
            }
        }
        Command::StartTimer { id, at_ms } => {
            let t = timer_mut(&mut next, &id)?;
            if t.run != TimerRun::Running {
                t.run = TimerRun::Running;
                t.started_at_ms = Some(at_ms);
                events.push(Event::TimerChanged { id });
            }
        }
        Command::PauseTimer { id, at_ms } => {
            let t = timer_mut(&mut next, &id)?;
            if t.run == TimerRun::Running {
                t.elapsed_ms = t.elapsed_at(at_ms);
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
            if next.outputs.get(&output) != Some(&look) {
                next.outputs.insert(output.clone(), look);
                events.push(Event::OutputLookChanged { output });
            }
        }
        Command::ServiceMode { on } => {
            if next.service_mode != on {
                next.service_mode = on;
                events.push(Event::ServiceModeChanged { on });
            }
        }
    }

    if next == *state {
        // Idempotent repeat: nothing observable changed.
        return Ok((next, Vec::new()));
    }
    next.version = state.version + 1;
    Ok((next, events))
}

fn check_version(state: &PresentationState, expected: Option<u64>) -> Result<(), CoreError> {
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
    next.clear = false;
    next.logo = false;
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
            Command::LoadShow {
                entries: vec![entry("a", 2), entry("empty", 0), entry("b", 2)],
            },
        )
        .unwrap()
        .0
    }

    fn run(s: &PresentationState, c: Command) -> PresentationState {
        apply(s, c).unwrap().0
    }

    fn next(v: Option<u64>) -> Command {
        Command::Next {
            expected_version: v,
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
            },
        );
        assert_eq!(s.live.as_ref().unwrap().slide.id.as_str(), "a:1");
        s = run(
            &s,
            Command::Prev {
                expected_version: None,
            },
        );
        s = run(
            &s,
            Command::Prev {
                expected_version: None,
            },
        );
        assert_eq!(s.live.as_ref().unwrap().slide.id.as_str(), "a:0");
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

    fn bible_cmd(resolved: Option<Slide>) -> Command {
        Command::ShowBible {
            passage: Passage::parse_osis("John.3.16").unwrap(),
            versions: vec![VersionCode("RVR1909".into())],
            layout: ParallelLayout::Columns,
            target: Target::Live,
            resolved,
        }
    }

    #[test]
    fn show_bible_requires_host_resolution() {
        let s = loaded();
        assert_eq!(
            apply(&s, bible_cmd(None)).unwrap_err(),
            CoreError::BibleNotResolved
        );
        let mut slide = Slide::blank("bible");
        slide.kind = SlideKind::Bible;
        let s = run(&s, bible_cmd(Some(slide)));
        let live = s.live.unwrap();
        assert!(live.item_id.is_none());
        assert_eq!(live.slide.kind, SlideKind::Bible);
    }

    #[test]
    fn next_after_adhoc_bible_resumes_show() {
        let mut s = loaded();
        s = run(&s, bible_cmd(Some(Slide::blank("bible"))));
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
            Command::LoadShow {
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
        let t = Timer::new("svc", "Culto", TimerKind::Countdown, 60);
        s = run(&s, Command::SetTimer { timer: t });
        s = run(
            &s,
            Command::StartTimer {
                id: TimerId::new("svc"),
                at_ms: 1_000,
            },
        );
        assert_eq!(s.timers[0].remaining_at(11_000), Some(50_000));
        s = run(
            &s,
            Command::PauseTimer {
                id: TimerId::new("svc"),
                at_ms: 11_000,
            },
        );
        assert_eq!(s.timers[0].remaining_at(99_000), Some(50_000));
        s = run(
            &s,
            Command::StartTimer {
                id: TimerId::new("svc"),
                at_ms: 20_000,
            },
        );
        assert_eq!(s.timers[0].remaining_at(25_000), Some(45_000));
        s = run(
            &s,
            Command::ResetTimer {
                id: TimerId::new("svc"),
            },
        );
        assert_eq!(s.timers[0].remaining_at(0), Some(60_000));
        assert!(matches!(
            apply(
                &s,
                Command::StartTimer {
                    id: TimerId::new("nope"),
                    at_ms: 0
                }
            ),
            Err(CoreError::UnknownTimer(_))
        ));
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
}
