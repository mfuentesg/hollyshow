//! Services (playlists): ordered items that reference library content by id, never by copy.

use crate::bible::{ParallelLayout, Passage, VersionCode};
use crate::ids::{ArrangementId, ItemId, MediaId, PlaylistId, SongId, ThemeId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ItemContent {
    Song {
        song_id: SongId,
        arrangement_id: Option<ArrangementId>,
        /// Semitones to transpose chords for this service only.
        transpose: i8,
    },
    /// Stored as canonical reference; text is resolved from the chosen versions at show time.
    Bible {
        passage: Passage,
        versions: Vec<VersionCode>,
        layout: ParallelLayout,
    },
    Media {
        media_id: MediaId,
        loop_playback: bool,
    },
    /// Free text slide: announcements, welcome, offering...
    Text { body: String },
    /// Pre-service countdown to a wall-clock or fixed duration.
    Countdown { seconds: u32 },
    /// Pure organisational marker, projects nothing ("Alabanza", "Prédica").
    Header,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ServiceItem {
    pub id: ItemId,
    pub title: String,
    pub content: ItemContent,
    /// Planned duration; drives the service timer / overrun warnings.
    pub duration_secs: Option<u32>,
    pub theme_id: Option<ThemeId>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    /// ISO date `YYYY-MM-DD` when scheduled.
    pub date: Option<String>,
    pub items: Vec<ServiceItem>,
    /// Templates are cloned to create a new service in seconds (PLAN §2.5 D).
    pub is_template: bool,
}

/// Why a pure playlist edit was refused. Edits never leave the playlist half changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlaylistError {
    #[error("unknown item {0}")]
    UnknownItem(ItemId),
    #[error("an item with id {0} already exists")]
    DuplicateId(ItemId),
    #[error("position {index} is out of range for a playlist of {len} items")]
    OutOfRange { index: usize, len: usize },
}

/// Planned start of one item relative to the start of the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlannedItem {
    pub item_id: ItemId,
    pub start_secs: u32,
    /// `None` for items without a planned duration (they add nothing to the schedule).
    pub duration_secs: Option<u32>,
}

impl Playlist {
    pub fn index_of(&self, id: &ItemId) -> Option<usize> {
        self.items.iter().position(|i| &i.id == id)
    }

    /// Inserts `item` at `at` (`None` appends) and returns its final position.
    pub fn add_item(
        &mut self,
        item: ServiceItem,
        at: Option<usize>,
    ) -> Result<usize, PlaylistError> {
        if self.index_of(&item.id).is_some() {
            return Err(PlaylistError::DuplicateId(item.id));
        }
        let len = self.items.len();
        let at = at.unwrap_or(len);
        if at > len {
            return Err(PlaylistError::OutOfRange { index: at, len });
        }
        self.items.insert(at, item);
        Ok(at)
    }

    /// Moves an item so that it ends at index `to` of the resulting list.
    pub fn move_item(&mut self, id: &ItemId, to: usize) -> Result<(), PlaylistError> {
        let from = self
            .index_of(id)
            .ok_or_else(|| PlaylistError::UnknownItem(id.clone()))?;
        let len = self.items.len();
        if to >= len {
            return Err(PlaylistError::OutOfRange { index: to, len });
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);
        Ok(())
    }

    pub fn remove_item(&mut self, id: &ItemId) -> Result<ServiceItem, PlaylistError> {
        let i = self
            .index_of(id)
            .ok_or_else(|| PlaylistError::UnknownItem(id.clone()))?;
        Ok(self.items.remove(i))
    }

    /// Copies an item right after the original under `new_id`; returns the copy's position.
    pub fn duplicate_item(&mut self, id: &ItemId, new_id: ItemId) -> Result<usize, PlaylistError> {
        if self.index_of(&new_id).is_some() {
            return Err(PlaylistError::DuplicateId(new_id));
        }
        let i = self
            .index_of(id)
            .ok_or_else(|| PlaylistError::UnknownItem(id.clone()))?;
        let mut copy = self.items[i].clone();
        copy.id = new_id;
        self.items.insert(i + 1, copy);
        Ok(i + 1)
    }

    /// Turns a finished service into a reusable template (fresh id, no date).
    pub fn to_template(&self, id: PlaylistId, name: String) -> Playlist {
        Playlist {
            id,
            name,
            date: None,
            is_template: true,
            items: self.items.clone(),
        }
    }

    /// Cumulative planned schedule; items without a duration take no time.
    pub fn schedule(&self) -> Vec<PlannedItem> {
        let mut t = 0u32;
        self.items
            .iter()
            .map(|i| {
                let p = PlannedItem {
                    item_id: i.id.clone(),
                    start_secs: t,
                    duration_secs: i.duration_secs,
                };
                t = t.saturating_add(i.duration_secs.unwrap_or(0));
                p
            })
            .collect()
    }

    /// Clones a template into a dated service, assigning fresh item ids via `new_id`.
    pub fn instantiate(
        &self,
        id: PlaylistId,
        name: String,
        date: Option<String>,
        mut new_id: impl FnMut() -> ItemId,
    ) -> Playlist {
        Playlist {
            id,
            name,
            date,
            is_template: false,
            items: self
                .items
                .iter()
                .cloned()
                .map(|mut i| {
                    i.id = new_id();
                    i
                })
                .collect(),
        }
    }

    pub fn total_planned_secs(&self) -> u32 {
        self.items
            .iter()
            .filter_map(|i| i.duration_secs)
            .fold(0u32, u32::saturating_add)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlaylistSummary {
    pub id: PlaylistId,
    pub name: String,
    pub date: Option<String>,
    pub item_count: u32,
    pub is_template: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instantiate_assigns_new_ids_and_clears_template_flag() {
        let tpl = Playlist {
            id: PlaylistId::new("tpl"),
            name: "Domingo".into(),
            date: None,
            is_template: true,
            items: vec![ServiceItem {
                id: ItemId::new("old"),
                title: "Bienvenida".into(),
                content: ItemContent::Text {
                    body: "Bienvenidos".into(),
                },
                duration_secs: Some(120),
                theme_id: None,
                notes: None,
            }],
        };
        let mut n = 0;
        let svc = tpl.instantiate(
            PlaylistId::new("p1"),
            "12 oct".into(),
            Some("2026-10-12".into()),
            || {
                n += 1;
                ItemId::new(format!("new{n}"))
            },
        );
        assert!(!svc.is_template);
        assert_eq!(svc.items[0].id.as_str(), "new1");
        assert_eq!(svc.total_planned_secs(), 120);
    }

    fn item(id: &str, secs: Option<u32>) -> ServiceItem {
        ServiceItem {
            id: ItemId::new(id),
            title: id.into(),
            content: ItemContent::Header,
            duration_secs: secs,
            theme_id: None,
            notes: None,
        }
    }

    fn list(ids: &[&str]) -> Playlist {
        Playlist {
            id: PlaylistId::new("p"),
            name: "P".into(),
            date: Some("2026-10-12".into()),
            is_template: false,
            items: ids.iter().map(|i| item(i, Some(60))).collect(),
        }
    }

    fn ids(p: &Playlist) -> Vec<&str> {
        p.items.iter().map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn add_inserts_and_appends() {
        let mut p = list(&["a", "b"]);
        assert_eq!(p.add_item(item("c", None), None), Ok(2));
        assert_eq!(p.add_item(item("x", None), Some(0)), Ok(0));
        assert_eq!(ids(&p), ["x", "a", "b", "c"]);
        assert_eq!(
            p.add_item(item("a", None), None),
            Err(PlaylistError::DuplicateId("a".into()))
        );
        assert_eq!(
            p.add_item(item("y", None), Some(9)),
            Err(PlaylistError::OutOfRange { index: 9, len: 4 })
        );
        assert_eq!(p.items.len(), 4, "failed edits change nothing");
    }

    #[test]
    fn move_puts_item_at_final_index() {
        let mut p = list(&["a", "b", "c", "d"]);
        p.move_item(&"a".into(), 2).unwrap();
        assert_eq!(ids(&p), ["b", "c", "a", "d"]);
        p.move_item(&"d".into(), 0).unwrap();
        assert_eq!(ids(&p), ["d", "b", "c", "a"]);
        p.move_item(&"d".into(), 0).unwrap();
        assert_eq!(ids(&p), ["d", "b", "c", "a"]);
        assert!(p.move_item(&"zz".into(), 0).is_err());
        assert!(p.move_item(&"a".into(), 4).is_err());
    }

    #[test]
    fn remove_and_duplicate() {
        let mut p = list(&["a", "b", "c"]);
        assert_eq!(p.duplicate_item(&"b".into(), "b2".into()), Ok(2));
        assert_eq!(ids(&p), ["a", "b", "b2", "c"]);
        assert_eq!(p.items[2].title, "b");
        assert!(p.duplicate_item(&"b".into(), "c".into()).is_err());
        assert!(p.duplicate_item(&"nope".into(), "n".into()).is_err());
        assert_eq!(p.remove_item(&"a".into()).unwrap().id.as_str(), "a");
        assert!(p.remove_item(&"a".into()).is_err());
        assert_eq!(ids(&p), ["b", "b2", "c"]);
    }

    #[test]
    fn template_round_trip() {
        let svc = list(&["a", "b"]);
        let tpl = svc.to_template("tpl".into(), "Domingo".into());
        assert!(tpl.is_template && tpl.date.is_none());
        let mut n = 0;
        let new = tpl.instantiate(
            "p2".into(),
            "19 oct".into(),
            Some("2026-10-19".into()),
            || {
                n += 1;
                ItemId::new(format!("n{n}"))
            },
        );
        assert_eq!(ids(&new), ["n1", "n2"]);
        assert!(!new.is_template);
        assert_eq!(ids(&svc), ["a", "b"], "source untouched");
    }

    #[test]
    fn schedule_accumulates_durations() {
        let mut p = list(&["a", "b", "c"]);
        p.items[1].duration_secs = None;
        let s = p.schedule();
        assert_eq!(
            s.iter().map(|x| x.start_secs).collect::<Vec<_>>(),
            [0, 60, 60]
        );
        assert_eq!(p.total_planned_secs(), 120);
    }
}
