//! Expands a playlist (items that reference library content by id) into the renderable show.
//! Pure orchestration over the repositories plus the `songs` and `bible` crates.

use hollyshow_bible::{build_bible_slide, VersionText};
use hollyshow_core::{
    BibleVersion, ItemContent, Playlist, ServiceItem, ShowEntry, Slide, SlideKind, Theme, Verse,
};
use hollyshow_songs::{SlideSplitter, SplitOptions};
use hollyshow_storage_api::Repos;

pub fn build_show(
    repos: &Repos,
    splitter: &dyn SlideSplitter,
    playlist: &Playlist,
) -> Vec<ShowEntry> {
    playlist
        .items
        .iter()
        .map(|item| ShowEntry {
            item_id: item.id.clone(),
            title: item.title.clone(),
            duration_secs: item.duration_secs,
            slides: slides_for(repos, splitter, item),
        })
        .collect()
}

fn slides_for(repos: &Repos, splitter: &dyn SlideSplitter, item: &ServiceItem) -> Vec<Slide> {
    let mut slides = match &item.content {
        ItemContent::Song {
            song_id,
            arrangement_id,
            transpose,
        } => match repos.songs.get(song_id) {
            Ok(Some(song)) => {
                let theme = theme_for(repos, item, song.theme_id.as_ref());
                let mut opts = SplitOptions::from_theme(&theme);
                opts.transpose = *transpose;
                splitter.split(&song, arrangement_id.as_ref(), opts)
            }
            Ok(None) => {
                log::warn!("service item {} references missing song {song_id}", item.id);
                vec![]
            }
            Err(e) => {
                log::error!("could not load song {song_id}: {e}");
                vec![]
            }
        },
        ItemContent::Bible {
            passage,
            versions,
            layout,
        } => {
            let mut loaded: Vec<(BibleVersion, Vec<Verse>)> = Vec::new();
            for code in versions {
                match repos.bibles.get_version(code) {
                    Ok(Some(meta)) => {
                        let verses = repos.bibles.passage(code, passage).unwrap_or_else(|e| {
                            log::error!("could not read {code:?} passage: {e}");
                            vec![]
                        });
                        loaded.push((meta, verses));
                    }
                    _ => log::warn!("service item {} uses unavailable Bible version", item.id),
                }
            }
            if loaded.is_empty() {
                vec![]
            } else {
                let cols: Vec<VersionText<'_>> = loaded
                    .iter()
                    .map(|(version, verses)| VersionText { version, verses })
                    .collect();
                vec![build_bible_slide(passage, *layout, &cols)]
            }
        }
        ItemContent::Media { media_id, .. } => {
            let mut s = Slide::blank(format!("media:{}", item.id));
            s.kind = SlideKind::Media;
            s.media = Some(media_id.clone());
            vec![s]
        }
        ItemContent::Text { body } => text_slides(&item.id.to_string(), body),
        ItemContent::Countdown { seconds } => {
            let mut s = Slide::blank(format!("countdown:{}", item.id));
            s.kind = SlideKind::Countdown;
            s.label = Some(item.title.clone());
            s.lines = vec![format!("{:02}:{:02}", seconds / 60, seconds % 60)];
            vec![s]
        }
        // Organisational marker: projects nothing and is skipped by navigation.
        ItemContent::Header => vec![],
    };
    if item.theme_id.is_some() {
        for s in &mut slides {
            s.theme_id = item.theme_id.clone();
        }
    }
    slides
}

/// Paragraphs (blank-line separated) become one slide each.
fn text_slides(item_id: &str, body: &str) -> Vec<Slide> {
    let mut slides = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let flush = |current: &mut Vec<&str>, slides: &mut Vec<Slide>| {
        if !current.is_empty() {
            let mut s = Slide::blank(format!("text:{item_id}:{}", slides.len()));
            s.kind = SlideKind::Text;
            s.lines = current.drain(..).map(str::to_owned).collect();
            slides.push(s);
        }
    };
    for line in body.lines() {
        if line.trim().is_empty() {
            flush(&mut current, &mut slides);
        } else {
            current.push(line.trim_end());
        }
    }
    flush(&mut current, &mut slides);
    slides
}

fn theme_for(
    repos: &Repos,
    item: &ServiceItem,
    song_theme: Option<&hollyshow_core::ThemeId>,
) -> Theme {
    item.theme_id
        .as_ref()
        .or(song_theme)
        .and_then(|id| repos.themes.get(id).ok().flatten())
        .unwrap_or_else(Theme::default_theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{
        Arrangement, ArrangementId, BibleRef, ItemId, ParallelLayout, Passage, PlaylistId, Section,
        SectionId, SectionKind, Song, SongId, VersionCode,
    };
    use hollyshow_songs::DefaultSplitter;
    use hollyshow_storage_api::memory::MemoryStore;
    use hollyshow_storage_api::SongRepo;
    use std::sync::Arc;

    pub(crate) fn song(id: &str, lyrics: &str) -> Song {
        Song {
            id: SongId::new(id),
            title: id.into(),
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
            arrangements: vec![Arrangement {
                id: ArrangementId::new("a"),
                name: "A".into(),
                sequence: vec![SectionId::new("v1")],
            }],
            created_at: 0,
            updated_at: 0,
        }
    }

    fn item(id: &str, content: ItemContent) -> ServiceItem {
        ServiceItem {
            id: ItemId::new(id),
            title: id.into(),
            content,
            duration_secs: Some(60),
            theme_id: None,
            notes: None,
        }
    }

    #[test]
    fn expands_every_item_kind() {
        let store = Arc::new(MemoryStore::new());
        SongRepo::save(&*store, &song("s1", "uno\ndos\ntres\ncuatro\ncinco")).unwrap();
        let repos = MemoryStore::bundle(store.clone());
        store.install_version(
            BibleVersion {
                code: VersionCode("T".into()),
                name: "T".into(),
                language: "es".into(),
                license: "PD".into(),
                copyright_text: "PD".into(),
                is_public_domain: true,
                versification: "kjv".into(),
                sort_order: 0,
                is_enabled: true,
                is_default: true,
                is_secondary: false,
            },
            vec![Verse {
                reference: BibleRef::verse("John", 3, 16),
                text: "porque".into(),
                marks: vec![],
            }],
        );
        let pl = Playlist {
            id: PlaylistId::new("p"),
            name: "p".into(),
            date: None,
            is_template: false,
            items: vec![
                item("h", ItemContent::Header),
                item(
                    "song",
                    ItemContent::Song {
                        song_id: SongId::new("s1"),
                        arrangement_id: None,
                        transpose: 0,
                    },
                ),
                item(
                    "missing",
                    ItemContent::Song {
                        song_id: SongId::new("nope"),
                        arrangement_id: None,
                        transpose: 0,
                    },
                ),
                item(
                    "bible",
                    ItemContent::Bible {
                        passage: Passage::parse_osis("John.3.16").unwrap(),
                        versions: vec![VersionCode("T".into()), VersionCode("ghost".into())],
                        layout: ParallelLayout::Columns,
                    },
                ),
                item(
                    "txt",
                    ItemContent::Text {
                        body: "Hola\nmundo\n\n\nAdios".into(),
                    },
                ),
                item("cd", ItemContent::Countdown { seconds: 305 }),
            ],
        };
        let show = build_show(&repos, &DefaultSplitter, &pl);
        let counts: Vec<usize> = show.iter().map(|e| e.slides.len()).collect();
        // header, song (4 lines/slide default theme -> 2), missing song, bible, text, countdown
        assert_eq!(counts, [0, 2, 0, 1, 2, 1]);
        assert_eq!(
            show[3].slides[0].bible.len(),
            1,
            "unknown version is skipped"
        );
        assert_eq!(show[4].slides[0].lines, ["Hola", "mundo"]);
        assert_eq!(show[5].slides[0].lines, ["05:05"]);
        assert_eq!(show[1].duration_secs, Some(60));
    }
}
