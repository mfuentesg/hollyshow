//! Host pipeline shared by the Tauri shell and the LAN server, so both behave identically:
//! `client command -> enrich_command (resolve Bible text) -> Engine::dispatch`, and
//! `QueryRequest -> execute_query -> QueryResponse`.

use crate::error::{StorageError, StorageResult};
use crate::repo::Repos;
use hollyshow_core::{
    Ack, BibleColumn, Command, CommandId, CoreError, Engine, ParallelLayout, Passage, QueryRequest,
    QueryResponse, Slide, SlideId, SlideKind, VersionCode, WireError,
};

pub fn execute_query(repos: &Repos, q: &QueryRequest) -> StorageResult<QueryResponse> {
    Ok(match q {
        QueryRequest::SearchSongs { text, limit } => QueryResponse::Songs {
            items: repos.songs.search(text, *limit)?,
        },
        QueryRequest::GetSong { id } => QueryResponse::Song {
            song: repos.songs.get(id)?,
        },
        QueryRequest::ListPlaylists => QueryResponse::Playlists {
            items: repos.playlists.list()?,
        },
        QueryRequest::GetPlaylist { id } => QueryResponse::Playlist {
            playlist: repos.playlists.get(id)?,
        },
        QueryRequest::ListBibleVersions => QueryResponse::BibleVersions {
            items: repos.bibles.list_versions()?,
        },
        QueryRequest::GetPassage { version, passage } => QueryResponse::Passage {
            verses: repos.bibles.passage(version, passage)?,
        },
        QueryRequest::SearchBible {
            text,
            version,
            limit,
        } => QueryResponse::BibleHits {
            items: repos.bibles.search(text, version.as_ref(), *limit)?,
        },
        QueryRequest::ListThemes => QueryResponse::Themes {
            items: repos.themes.list()?,
        },
    })
}

/// Builds the renderable Bible slide for `passage` in each of `versions`.
///
/// A verse missing from a version (different canon or versification) yields an explicit marker
/// instead of an error, so parallel display never breaks.
pub fn resolve_bible_slide(
    repos: &Repos,
    passage: &Passage,
    versions: &[VersionCode],
    layout: ParallelLayout,
) -> StorageResult<Slide> {
    let mut columns = Vec::with_capacity(versions.len());
    for code in versions {
        let meta = repos
            .bibles
            .get_version(code)?
            .ok_or_else(|| StorageError::NotFound(format!("Bible version {}", code.0)))?;
        let verses = repos.bibles.passage(code, passage)?;
        let text = if verses.is_empty() {
            "\u{2014}".to_owned()
        } else {
            verses
                .iter()
                .map(|v| match v.reference.verse {
                    Some(n) if verses.len() > 1 => format!("{n} {}", v.text),
                    _ => v.text.clone(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        columns.push(BibleColumn {
            version: code.clone(),
            // The `bible` crate swaps this for the localized book name ("Juan 3:16").
            reference: passage.to_osis(),
            text,
            copyright: Some(meta.copyright_text).filter(|c| !c.is_empty()),
        });
    }
    let mut slide = Slide::blank(format!("bible:{}", passage.to_osis()));
    slide.id = SlideId::new(format!("bible:{}", passage.to_osis()));
    slide.kind = SlideKind::Bible;
    slide.label = Some(passage.to_osis());
    slide.layout = layout;
    slide.bible = columns;
    Ok(slide)
}

/// Fills in what the pure reducer cannot know. Currently: Bible text for `ShowBible`.
pub fn enrich_command(repos: &Repos, cmd: Command) -> Result<Command, CoreError> {
    match cmd {
        Command::ShowBible {
            passage,
            versions,
            layout,
            target,
            resolved: None,
        } => {
            let slide = resolve_bible_slide(repos, &passage, &versions, layout)
                .map_err(|e| CoreError::Invalid(e.to_string()))?;
            Ok(Command::ShowBible {
                passage,
                versions,
                layout,
                target,
                resolved: Some(slide),
            })
        }
        other => Ok(other),
    }
}

/// Full command path used by every transport.
pub fn dispatch(engine: &Engine, repos: &Repos, id: Option<&CommandId>, cmd: Command) -> Ack {
    let result = enrich_command(repos, cmd).and_then(|c| engine.dispatch(id, c));
    match result {
        Ok(o) => Ack {
            ok: true,
            version: o.version,
            error: None,
            duplicate: o.duplicate,
        },
        Err(e) => Ack {
            ok: false,
            version: engine.view().version,
            error: Some(WireError::from(&e)),
            duplicate: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;
    use hollyshow_core::{BibleRef, BibleVersion, Target, Verse};
    use std::sync::Arc;

    fn repos_with_bible() -> Repos {
        let store = Arc::new(MemoryStore::new());
        store.install_version(
            BibleVersion {
                code: VersionCode("TEST".into()),
                name: "Test".into(),
                language: "es".into(),
                license: "PD".into(),
                copyright_text: "Dominio publico".into(),
                is_public_domain: true,
                versification: "kjv".into(),
                sort_order: 0,
                is_enabled: true,
                is_default: true,
                is_secondary: false,
            },
            (16..=17)
                .map(|n| Verse {
                    reference: BibleRef::verse("John", 3, n),
                    text: format!("texto {n}"),
                    marks: vec![],
                })
                .collect(),
        );
        Repos {
            songs: store.clone(),
            bibles: store.clone(),
            themes: store.clone(),
            playlists: store.clone(),
            settings: store,
        }
    }

    #[test]
    fn show_bible_is_resolved_then_applied() {
        let repos = repos_with_bible();
        let engine = Engine::default();
        let ack = dispatch(
            &engine,
            &repos,
            None,
            Command::ShowBible {
                passage: Passage::parse_osis("John.3.16-17").unwrap(),
                versions: vec![VersionCode("TEST".into())],
                layout: ParallelLayout::Columns,
                target: Target::Live,
                resolved: None,
            },
        );
        assert!(ack.ok, "{ack:?}");
        let live = engine.snapshot().live.unwrap();
        assert_eq!(live.slide.bible[0].text, "16 texto 16 17 texto 17");
        assert_eq!(
            live.slide.bible[0].copyright.as_deref(),
            Some("Dominio publico")
        );
    }

    #[test]
    fn unknown_version_is_a_clean_error() {
        let repos = repos_with_bible();
        let engine = Engine::default();
        let ack = dispatch(
            &engine,
            &repos,
            None,
            Command::ShowBible {
                passage: Passage::parse_osis("John.3.16").unwrap(),
                versions: vec![VersionCode("NOPE".into())],
                layout: ParallelLayout::Columns,
                target: Target::Live,
                resolved: None,
            },
        );
        assert!(!ack.ok);
        assert_eq!(ack.error.unwrap().code, "invalid");
        assert_eq!(engine.view().version, 0);
    }

    #[test]
    fn queries_route_to_repos() {
        let repos = repos_with_bible();
        match execute_query(&repos, &QueryRequest::ListBibleVersions).unwrap() {
            QueryResponse::BibleVersions { items } => assert_eq!(items.len(), 1),
            other => panic!("{other:?}"),
        }
    }
}
