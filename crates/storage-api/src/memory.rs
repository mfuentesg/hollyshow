//! In-memory repositories for tests and UI development. Not durable.
//!
//! Limitation: Bible passages are resolved by `BibleRef` ordering, which is only canonical
//! inside one book, so cross-book ranges return verses sorted alphabetically by book id.

use crate::error::{StorageError, StorageResult};
use crate::repo::*;
use hollyshow_core::text::fold;
use hollyshow_core::{
    BibleHit, BibleRef, BibleVersion, Passage, Playlist, PlaylistId, PlaylistSummary, Song, SongId,
    SongSummary, Theme, ThemeId, Verse, VersionCode,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Default)]
struct Data {
    songs: HashMap<SongId, Song>,
    usage: Vec<(SongId, i64)>,
    versions: Vec<BibleVersion>,
    verses: BTreeMap<(VersionCode, BibleRef), Verse>,
    themes: BTreeMap<ThemeId, Theme>,
    playlists: BTreeMap<PlaylistId, Playlist>,
    settings: BTreeMap<String, String>,
}

#[derive(Default)]
pub struct MemoryStore {
    data: Mutex<Data>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the repository bundle backed by one shared store.
    pub fn repos() -> Repos {
        Self::bundle(Arc::new(Self::new()))
    }

    /// Wraps an existing store (kept by the caller for direct seeding) as a repository bundle.
    pub fn bundle(s: Arc<Self>) -> Repos {
        Repos {
            songs: s.clone(),
            bibles: s.clone(),
            themes: s.clone(),
            playlists: s.clone(),
            settings: s,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Data> {
        self.data.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Test helper: install a version with its verses.
    pub fn install_version(&self, version: BibleVersion, verses: Vec<Verse>) {
        let mut d = self.lock();
        for v in verses {
            d.verses
                .insert((version.code.clone(), v.reference.clone()), v);
        }
        d.versions.retain(|x| x.code != version.code);
        d.versions.push(version);
    }

    pub fn song_usage(&self) -> Vec<(SongId, i64)> {
        self.lock().usage.clone()
    }
}

fn summary(s: &Song, snippet: Option<String>) -> SongSummary {
    SongSummary {
        id: s.id.clone(),
        title: s.title.clone(),
        author: s.author.clone(),
        key: s.key.clone(),
        snippet,
    }
}

impl SongRepo for MemoryStore {
    fn search(&self, text: &str, limit: u32) -> StorageResult<Vec<SongSummary>> {
        let q = fold(text);
        let d = self.lock();
        let mut out: Vec<_> = d
            .songs
            .values()
            .filter_map(|s| {
                if q.is_empty() {
                    return Some(summary(s, None));
                }
                if fold(&s.title).contains(&q)
                    || fold(s.author.as_deref().unwrap_or("")).contains(&q)
                {
                    return Some(summary(s, None));
                }
                s.sections
                    .iter()
                    .find_map(|sec| sec.lyrics.lines().find(|l| fold(l).contains(&q)))
                    .map(|line| summary(s, Some(line.to_owned())))
            })
            .collect();
        out.sort_by_cached_key(|s| fold(&s.title));
        out.truncate(limit as usize);
        Ok(out)
    }

    fn get(&self, id: &SongId) -> StorageResult<Option<Song>> {
        Ok(self.lock().songs.get(id).cloned())
    }

    fn save(&self, song: &Song) -> StorageResult<()> {
        self.lock().songs.insert(song.id.clone(), song.clone());
        Ok(())
    }

    fn delete(&self, id: &SongId) -> StorageResult<()> {
        self.lock()
            .songs
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    fn log_usage(&self, id: &SongId, shown_at_ms: i64) -> StorageResult<()> {
        self.lock().usage.push((id.clone(), shown_at_ms));
        Ok(())
    }
}

impl BibleRepo for MemoryStore {
    fn list_versions(&self) -> StorageResult<Vec<BibleVersion>> {
        let mut v = self.lock().versions.clone();
        v.sort_by_key(|x| x.sort_order);
        Ok(v)
    }

    fn get_version(&self, code: &VersionCode) -> StorageResult<Option<BibleVersion>> {
        Ok(self
            .lock()
            .versions
            .iter()
            .find(|v| &v.code == code)
            .cloned())
    }

    fn set_enabled(&self, code: &VersionCode, enabled: bool) -> StorageResult<()> {
        let mut d = self.lock();
        let v = d
            .versions
            .iter_mut()
            .find(|v| &v.code == code)
            .ok_or_else(|| StorageError::NotFound(code.0.clone()))?;
        v.is_enabled = enabled;
        Ok(())
    }

    fn set_default(&self, code: &VersionCode) -> StorageResult<()> {
        let mut d = self.lock();
        if !d.versions.iter().any(|v| &v.code == code) {
            return Err(StorageError::NotFound(code.0.clone()));
        }
        for v in &mut d.versions {
            v.is_default = &v.code == code;
        }
        Ok(())
    }

    fn set_secondary(&self, code: Option<&VersionCode>) -> StorageResult<()> {
        let mut d = self.lock();
        if let Some(c) = code {
            if !d.versions.iter().any(|v| &v.code == c) {
                return Err(StorageError::NotFound(c.0.clone()));
            }
        }
        for v in &mut d.versions {
            v.is_secondary = code == Some(&v.code);
        }
        Ok(())
    }

    fn remove_version(&self, code: &VersionCode) -> StorageResult<()> {
        let mut d = self.lock();
        d.versions.retain(|v| &v.code != code);
        d.verses.retain(|(c, _), _| c != code);
        Ok(())
    }

    fn passage(&self, version: &VersionCode, passage: &Passage) -> StorageResult<Vec<Verse>> {
        let d = self.lock();
        let in_range = |r: &BibleRef| {
            let lo = (passage.start.chapter, passage.start.verse.unwrap_or(0));
            let hi = (passage.end.chapter, passage.end.verse.unwrap_or(u16::MAX));
            let here = (r.chapter, r.verse.unwrap_or(0));
            r.book == passage.start.book && here >= lo && here <= hi
        };
        Ok(d.verses
            .iter()
            .filter(|((c, r), _)| c == version && in_range(r))
            .map(|(_, v)| v.clone())
            .collect())
    }

    fn search(
        &self,
        text: &str,
        version: Option<&VersionCode>,
        limit: u32,
    ) -> StorageResult<Vec<BibleHit>> {
        let q = fold(text);
        let d = self.lock();
        Ok(d.verses
            .iter()
            .filter(|((c, _), v)| version.is_none_or(|x| x == c) && fold(&v.text).contains(&q))
            .take(limit as usize)
            .map(|((c, r), v)| BibleHit {
                version: c.clone(),
                reference: r.clone(),
                text: v.text.clone(),
            })
            .collect())
    }
}

impl ThemeRepo for MemoryStore {
    fn list(&self) -> StorageResult<Vec<Theme>> {
        Ok(self.lock().themes.values().cloned().collect())
    }
    fn get(&self, id: &ThemeId) -> StorageResult<Option<Theme>> {
        Ok(self.lock().themes.get(id).cloned())
    }
    fn save(&self, theme: &Theme) -> StorageResult<()> {
        self.lock().themes.insert(theme.id.clone(), theme.clone());
        Ok(())
    }
    fn delete(&self, id: &ThemeId) -> StorageResult<()> {
        self.lock().themes.remove(id);
        Ok(())
    }
}

impl PlaylistRepo for MemoryStore {
    fn list(&self) -> StorageResult<Vec<PlaylistSummary>> {
        Ok(self
            .lock()
            .playlists
            .values()
            .map(|p| PlaylistSummary {
                id: p.id.clone(),
                name: p.name.clone(),
                date: p.date.clone(),
                item_count: p.items.len() as u32,
                is_template: p.is_template,
            })
            .collect())
    }
    fn get(&self, id: &PlaylistId) -> StorageResult<Option<Playlist>> {
        Ok(self.lock().playlists.get(id).cloned())
    }
    fn save(&self, playlist: &Playlist) -> StorageResult<()> {
        self.lock()
            .playlists
            .insert(playlist.id.clone(), playlist.clone());
        Ok(())
    }
    fn delete(&self, id: &PlaylistId) -> StorageResult<()> {
        self.lock().playlists.remove(id);
        Ok(())
    }
}

impl SettingsRepo for MemoryStore {
    fn get(&self, key: &str) -> StorageResult<Option<String>> {
        Ok(self.lock().settings.get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> StorageResult<()> {
        self.lock()
            .settings
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    fn remove(&self, key: &str) -> StorageResult<()> {
        self.lock().settings.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{Section, SectionId, SectionKind};

    fn song(id: &str, title: &str, lyrics: &str) -> Song {
        Song {
            id: SongId::new(id),
            title: title.into(),
            alt_title: None,
            author: None,
            copyright: None,
            ccli: None,
            key: None,
            bpm: None,
            tags: vec![],
            theme_id: None,
            sections: vec![Section {
                id: SectionId::new("v1"),
                kind: SectionKind::Verse,
                label: "Verso 1".into(),
                lyrics: lyrics.into(),
                translation: None,
            }],
            arrangements: vec![],
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn song_search_ignores_accents_and_case() {
        let r = MemoryStore::repos();
        r.songs
            .save(&song("1", "Sublime Gracia", "Cuán dulce el sonido"))
            .unwrap();
        r.songs
            .save(&song("2", "Otra", "Santo es el Señor"))
            .unwrap();
        assert_eq!(
            r.songs.search("cuan DULCE", 10).unwrap()[0].id.as_str(),
            "1"
        );
        let hits = r.songs.search("senor", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet.as_deref(), Some("Santo es el Señor"));
    }

    #[test]
    fn bible_default_is_exclusive_and_passage_ranges() {
        let s = MemoryStore::new();
        let ver = |c: &str| BibleVersion {
            code: VersionCode(c.into()),
            name: c.into(),
            language: "es".into(),
            license: "PD".into(),
            copyright_text: String::new(),
            is_public_domain: true,
            versification: "kjv".into(),
            sort_order: 0,
            is_enabled: true,
            is_default: false,
            is_secondary: false,
        };
        let verses = (14..=18)
            .map(|n| Verse {
                reference: BibleRef::verse("John", 3, n),
                text: format!("v{n}"),
                marks: vec![],
            })
            .collect();
        s.install_version(ver("A"), verses);
        s.install_version(ver("B"), vec![]);
        s.set_default(&VersionCode("A".into())).unwrap();
        s.set_default(&VersionCode("B".into())).unwrap();
        let defaults: Vec<_> = s
            .list_versions()
            .unwrap()
            .into_iter()
            .filter(|v| v.is_default)
            .collect();
        assert_eq!(defaults.len(), 1);
        let p = Passage::parse_osis("John.3.16-18").unwrap();
        assert_eq!(s.passage(&VersionCode("A".into()), &p).unwrap().len(), 3);
    }
}
