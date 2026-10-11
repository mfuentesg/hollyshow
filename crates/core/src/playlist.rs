//! Services (playlists): ordered items that reference library content by id, never by copy.

use crate::bible::{ParallelLayout, Passage, VersionCode};
use crate::ids::{ArrangementId, ItemId, MediaId, PlaylistId, SongId, ThemeId};
use serde::{Deserialize, Serialize};
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

impl Playlist {
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
        self.items.iter().filter_map(|i| i.duration_secs).sum()
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
}
