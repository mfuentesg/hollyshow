//! Song library domain model (PLAN §6: songs, song_sections, song_arrangements).

use crate::ids::{ArrangementId, SectionId, SongId, ThemeId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SectionKind {
    Verse,
    Chorus,
    PreChorus,
    Bridge,
    Intro,
    Outro,
    Tag,
    Interlude,
    Other,
}

/// A named block of lyrics. Lyrics may carry inline ChordPro chords such as `[G]Sublime [D]gracia`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Section {
    pub id: SectionId,
    pub kind: SectionKind,
    /// Display label: "Verso 1", "Coro".
    pub label: String,
    pub lyrics: String,
    /// Optional second-language line for each lyric line (bilingual songs).
    #[serde(default)]
    pub translation: Option<String>,
}

/// Play order, independent of the order sections were written (e.g. V1 C V2 C P C C).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Arrangement {
    pub id: ArrangementId,
    pub name: String,
    pub sequence: Vec<SectionId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Song {
    pub id: SongId,
    pub title: String,
    pub alt_title: Option<String>,
    pub author: Option<String>,
    pub copyright: Option<String>,
    pub ccli: Option<String>,
    /// Musical key, e.g. `G`, `F#m`.
    pub key: Option<String>,
    pub bpm: Option<u16>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub theme_id: Option<ThemeId>,
    pub sections: Vec<Section>,
    pub arrangements: Vec<Arrangement>,
    /// Unix epoch milliseconds.
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl Song {
    pub fn section(&self, id: &SectionId) -> Option<&Section> {
        self.sections.iter().find(|s| &s.id == id)
    }

    /// Sections in play order for `arrangement` (default = written order when `None`/unknown).
    pub fn ordered_sections(&self, arrangement: Option<&ArrangementId>) -> Vec<&Section> {
        let chosen = arrangement
            .and_then(|id| self.arrangements.iter().find(|a| &a.id == id))
            .or_else(|| self.arrangements.first());
        match chosen {
            Some(a) => a
                .sequence
                .iter()
                .filter_map(|id| self.section(id))
                .collect(),
            None => self.sections.iter().collect(),
        }
    }
}

/// Lightweight row for lists and search results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SongSummary {
    pub id: SongId,
    pub title: String,
    pub author: Option<String>,
    pub key: Option<String>,
    /// Matching lyric fragment for search results.
    pub snippet: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Song {
        let sec = |id: &str, kind| Section {
            id: SectionId::new(id),
            kind,
            label: id.into(),
            lyrics: String::new(),
            translation: None,
        };
        Song {
            id: SongId::new("s1"),
            title: "T".into(),
            alt_title: None,
            author: None,
            copyright: None,
            ccli: None,
            key: None,
            bpm: None,
            tags: vec![],
            theme_id: None,
            sections: vec![sec("v1", SectionKind::Verse), sec("c", SectionKind::Chorus)],
            arrangements: vec![Arrangement {
                id: ArrangementId::new("a1"),
                name: "Default".into(),
                sequence: ["v1", "c", "v1", "c"]
                    .into_iter()
                    .map(SectionId::new)
                    .collect(),
            }],
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn arrangement_repeats_sections() {
        let s = song();
        let ids: Vec<_> = s
            .ordered_sections(None)
            .iter()
            .map(|x| x.id.as_str().to_owned())
            .collect();
        assert_eq!(ids, ["v1", "c", "v1", "c"]);
    }

    #[test]
    fn falls_back_to_written_order() {
        let mut s = song();
        s.arrangements.clear();
        assert_eq!(s.ordered_sections(None).len(), 2);
    }
}
