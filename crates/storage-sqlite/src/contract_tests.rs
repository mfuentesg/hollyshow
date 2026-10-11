//! Repository contract battery (run against `MemoryStore` and `Store`) plus SQLite-specific tests.

use crate::*;
use hollyshow_core::{
    Arrangement, ArrangementId, BibleRef, BibleVersion, ItemContent, ItemId, Passage, Playlist,
    PlaylistId, Section, SectionId, SectionKind, ServiceItem, Song, SongId, Theme, ThemeId, Verse,
    VersionCode,
};
use hollyshow_storage_api::memory::MemoryStore;
use hollyshow_storage_api::{BibleRepo, Repos, StorageError};
use std::sync::Arc;

fn section(id: &str, kind: SectionKind, label: &str, lyrics: &str) -> Section {
    Section {
        id: SectionId::new(id),
        kind,
        label: label.into(),
        lyrics: lyrics.into(),
        translation: None,
    }
}

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
        sections: vec![section("v1", SectionKind::Verse, "Verso 1", lyrics)],
        arrangements: vec![],
        created_at: 10,
        updated_at: 20,
    }
}

fn full_song() -> Song {
    let mut s = song(
        "full",
        "Sublime Gracia",
        "[G]Cuán dulce el sonido\nque salvó a un infeliz",
    );
    s.alt_title = Some("Amazing Grace".into());
    s.author = Some("John Newton".into());
    s.copyright = Some("Dominio público".into());
    s.ccli = Some("22025".into());
    s.key = Some("G".into());
    s.bpm = Some(72);
    s.tags = vec!["himno".into(), "gracia".into()];
    s.theme_id = Some(ThemeId::new("default"));
    s.sections.push(Section {
        translation: Some("Praise God".into()),
        ..section("c", SectionKind::Chorus, "Coro", "Alabad a Dios")
    });
    s.arrangements = vec![
        Arrangement {
            id: ArrangementId::new("a1"),
            name: "Domingo".into(),
            sequence: ["v1", "c", "v1", "c"].map(SectionId::new).to_vec(),
        },
        Arrangement {
            id: ArrangementId::new("a2"),
            name: "Corto".into(),
            sequence: vec![SectionId::new("c")],
        },
    ];
    s
}

fn playlist(id: &str, date: Option<&str>, template: bool) -> Playlist {
    Playlist {
        id: PlaylistId::new(id),
        name: format!("Culto {id}"),
        date: date.map(Into::into),
        is_template: template,
        items: vec![
            ServiceItem {
                id: ItemId::new("i1"),
                title: "Bienvenida".into(),
                content: ItemContent::Text {
                    body: "Hola".into(),
                },
                duration_secs: Some(60),
                theme_id: None,
                notes: Some("sonreír".into()),
            },
            ServiceItem {
                id: ItemId::new("i2"),
                title: "Alabanza".into(),
                content: ItemContent::Song {
                    song_id: SongId::new("full"),
                    arrangement_id: Some(ArrangementId::new("a1")),
                    transpose: -2,
                },
                duration_secs: None,
                theme_id: None,
                notes: None,
            },
        ],
    }
}

fn version(code: &str, order: i64) -> BibleVersion {
    BibleVersion {
        code: VersionCode(code.into()),
        name: format!("Biblia {code}"),
        language: "es".into(),
        license: "Public Domain".into(),
        copyright_text: String::new(),
        is_public_domain: true,
        versification: "kjv".into(),
        sort_order: order,
        is_enabled: true,
        is_default: false,
        is_secondary: false,
    }
}

fn verse(book: &str, c: u16, v: u16, text: &str) -> Verse {
    Verse {
        reference: BibleRef::verse(book, c, v),
        text: text.into(),
        marks: vec![],
    }
}

/// Behaviour every `Repos` implementation must share.
fn contract(r: &Repos) {
    // Songs: round trip, accent/case-insensitive search, replace, delete.
    let full = full_song();
    r.songs.save(&full).unwrap();
    assert_eq!(r.songs.get(&full.id).unwrap().unwrap(), full);
    assert!(r.songs.get(&SongId::new("nope")).unwrap().is_none());
    r.songs
        .save(&song("2", "Otra canción", "Santo es el Señor"))
        .unwrap();
    assert_eq!(
        r.songs.search("cuan DULCE", 10).unwrap()[0].id.as_str(),
        "full"
    );
    let hits = r.songs.search("senor", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].snippet.as_deref(), Some("Santo es el Señor"));
    assert_eq!(r.songs.search("", 10).unwrap().len(), 2);
    assert_eq!(r.songs.search("", 1).unwrap().len(), 1);
    assert!(r.songs.search("zzzz", 10).unwrap().is_empty());
    let mut changed = full.clone();
    changed.title = "Nuevo título".into();
    changed.sections.truncate(1);
    changed.arrangements.clear();
    r.songs.save(&changed).unwrap();
    assert_eq!(r.songs.get(&full.id).unwrap().unwrap(), changed);
    assert!(r.songs.search("sublime", 10).unwrap().is_empty());
    r.songs.delete(&SongId::new("2")).unwrap();
    assert!(matches!(
        r.songs.delete(&SongId::new("2")),
        Err(StorageError::NotFound(_))
    ));
    assert!(r.songs.search("senor", 10).unwrap().is_empty());
    r.songs.log_usage(&full.id, 1).unwrap();

    // Themes.
    let mut t = Theme::default_theme();
    r.themes.save(&t).unwrap();
    t.name = "Otro".into();
    r.themes.save(&t).unwrap();
    assert_eq!(r.themes.list().unwrap().len(), 1);
    assert_eq!(r.themes.get(&t.id).unwrap().unwrap().name, "Otro");
    r.themes.delete(&t.id).unwrap();
    assert!(r.themes.get(&t.id).unwrap().is_none());

    // Playlists.
    let p = playlist("p1", Some("2026-10-11"), false);
    r.playlists.save(&p).unwrap();
    r.playlists.save(&playlist("tpl", None, true)).unwrap();
    assert_eq!(r.playlists.get(&p.id).unwrap().unwrap(), p);
    let list = r.playlists.list().unwrap();
    assert_eq!(list.len(), 2);
    let s = list.iter().find(|s| s.id == p.id).unwrap();
    assert_eq!((s.item_count, s.is_template), (2, false));
    r.playlists.delete(&p.id).unwrap();
    assert!(r.playlists.get(&p.id).unwrap().is_none());

    // Settings.
    assert_eq!(r.settings.get("k").unwrap(), None);
    r.settings.set("k", "1").unwrap();
    r.settings.set("k", "2").unwrap();
    assert_eq!(r.settings.get("k").unwrap().as_deref(), Some("2"));
    r.settings.remove("k").unwrap();
    assert_eq!(r.settings.get("k").unwrap(), None);
}

#[test]
fn memory_store_satisfies_the_contract() {
    contract(&MemoryStore::repos());
}

#[test]
fn sqlite_store_satisfies_the_contract() {
    contract(&Arc::new(Store::open_in_memory().unwrap()).repos());
}

#[test]
fn search_ranks_titles_above_lyrics_and_matches_prefixes() {
    let s = Store::open_in_memory().unwrap();
    s.save_song_for_test(&song("a", "Gloria", "otro texto"));
    s.save_song_for_test(&song("b", "Otra", "Gloria a Dios en las alturas"));
    let hits = hollyshow_storage_api::SongRepo::search(&s, "glor", 10).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        hits[1].snippet.as_deref(),
        Some("Gloria a Dios en las alturas")
    );
    // ñ folds to n, chords are not indexed, FTS syntax characters are harmless.
    s.save_song_for_test(&song("c", "Niño", "[Am]Es[G]pera"));
    let sp = |q: &str| hollyshow_storage_api::SongRepo::search(&s, q, 10).unwrap();
    assert_eq!(sp("nino").len(), 1);
    assert_eq!(sp("espera").len(), 1);
    assert!(sp("Am").is_empty());
    assert!(sp("\"* OR NEAR(").len() <= 3);
    assert!(sp("!!!").is_empty());
}

impl Store {
    fn save_song_for_test(&self, s: &Song) {
        hollyshow_storage_api::SongRepo::save(self, s).unwrap();
    }
}

#[test]
fn invalid_songs_are_rejected_without_partial_writes() {
    let s = Store::open_in_memory().unwrap();
    let mut dup = song("d", "Dup", "x");
    dup.sections
        .push(section("v1", SectionKind::Chorus, "Coro", "y"));
    assert!(matches!(
        hollyshow_storage_api::SongRepo::save(&s, &dup),
        Err(StorageError::Invalid(_))
    ));
    assert_eq!(s.song_count().unwrap(), 0);
    let mut empty_id = song("", "x", "y");
    empty_id.id = SongId::new("");
    assert!(hollyshow_storage_api::SongRepo::save(&s, &empty_id).is_err());
}

#[test]
fn usage_report_counts_per_song_and_survives_deletion() {
    let s = Store::open_in_memory().unwrap();
    let mut a = song("a", "Alfa", "x");
    a.ccli = Some("111".into());
    hollyshow_storage_api::SongRepo::save(&s, &a).unwrap();
    hollyshow_storage_api::SongRepo::save(&s, &song("b", "Beta", "y")).unwrap();
    for t in [100, 200, 300] {
        hollyshow_storage_api::SongRepo::log_usage(&s, &SongId::new("a"), t).unwrap();
    }
    hollyshow_storage_api::SongRepo::log_usage(&s, &SongId::new("b"), 250).unwrap();
    hollyshow_storage_api::SongRepo::delete(&s, &SongId::new("a")).unwrap();
    let rep = s.usage_report(0, 1000).unwrap();
    assert_eq!(rep[0].song_id.as_str(), "a");
    assert_eq!(rep[0].times_shown, 3);
    assert_eq!(rep[0].title.as_deref(), Some("Alfa"));
    assert_eq!(rep[0].ccli.as_deref(), Some("111"));
    assert_eq!(rep[1].times_shown, 1);
    assert_eq!(s.usage_report(150, 260).unwrap().len(), 2);
    assert_eq!(s.usage_report(500, 600).unwrap().len(), 0);
}

#[test]
fn playlists_list_newest_first_with_undated_last() {
    let s = Store::open_in_memory().unwrap();
    for (id, d) in [
        ("a", Some("2026-01-01")),
        ("b", None),
        ("c", Some("2026-03-01")),
    ] {
        hollyshow_storage_api::PlaylistRepo::save(&s, &playlist(id, d, false)).unwrap();
    }
    let ids: Vec<_> = hollyshow_storage_api::PlaylistRepo::list(&s)
        .unwrap()
        .into_iter()
        .map(|p| p.id.to_string())
        .collect();
    assert_eq!(ids, ["c", "a", "b"]);
}

fn john_store() -> Store {
    let s = Store::open_in_memory().unwrap();
    let verses = vec![
        verse(
            "Gen",
            1,
            1,
            "En el principio creó Dios los cielos y la tierra.",
        ),
        verse("Gen", 1, 2, "Y la tierra estaba desordenada y vacía."),
        verse(
            "John",
            3,
            15,
            "Para que todo aquel que en él creyere no se pierda.",
        ),
        verse("John", 3, 16, "Porque de tal manera amó Dios al mundo."),
        verse("John", 3, 17, "Porque no envió Dios á su Hijo al mundo."),
        verse("John", 4, 1, "Cuando, pues, el Señor entendió."),
    ];
    s.install_bible(&version("RVR1909", 1), &verses).unwrap();
    s.install_bible(
        &version("KJV", 2),
        &[verse("John", 3, 16, "For God so loved the world.")],
    )
    .unwrap();
    s
}

#[test]
fn bible_passages_ranges_and_missing_verses() {
    let s = john_store();
    let es = VersionCode("RVR1909".into());
    let get = |p: &str| s.passage(&es, &Passage::parse_osis(p).unwrap()).unwrap();
    assert_eq!(get("John.3.16").len(), 1);
    assert_eq!(get("John.3.16-18").len(), 2, "verse 18 does not exist");
    assert_eq!(get("John.3.15-John.4.1").len(), 4, "spans chapters");
    assert_eq!(
        get("Gen.1.2-John.3.15").len(),
        2,
        "spans books in canonical order"
    );
    assert_eq!(get("John.3").len(), 3, "whole chapter");
    assert!(get("Rev.1.1").is_empty());
    assert!(get("John.9.9").is_empty());
    let kjv = s
        .passage(
            &VersionCode("KJV".into()),
            &Passage::parse_osis("John.3.16").unwrap(),
        )
        .unwrap();
    assert_eq!(kjv[0].text, "For God so loved the world.");
}

#[test]
fn bible_search_is_per_version_and_accent_insensitive() {
    let s = john_store();
    let all = s.search("senor", None, 10).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].reference, BibleRef::verse("John", 4, 1));
    assert_eq!(s.search("loved", None, 10).unwrap()[0].version.0, "KJV");
    assert!(s
        .search("loved", Some(&VersionCode("RVR1909".into())), 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        s.search("mundo", Some(&VersionCode("RVR1909".into())), 10)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(s.search("mundo", None, 1).unwrap().len(), 1);
}

#[test]
fn bible_default_and_secondary_stay_exclusive() {
    let s = john_store();
    let (es, en) = (VersionCode("RVR1909".into()), VersionCode("KJV".into()));
    let flags = |s: &Store| -> Vec<(String, bool, bool)> {
        s.list_versions()
            .unwrap()
            .into_iter()
            .map(|v| (v.code.0, v.is_default, v.is_secondary))
            .collect()
    };
    assert_eq!(
        flags(&s),
        [
            ("RVR1909".into(), true, false),
            ("KJV".into(), false, false)
        ]
    );
    s.set_secondary(Some(&en)).unwrap();
    s.set_default(&en).unwrap();
    assert_eq!(
        flags(&s),
        [
            ("RVR1909".into(), false, false),
            ("KJV".into(), true, false)
        ]
    );
    s.set_secondary(Some(&es)).unwrap();
    s.set_secondary(None).unwrap();
    assert!(flags(&s).iter().all(|f| !f.2));
    assert!(matches!(
        s.set_default(&VersionCode("nope".into())),
        Err(StorageError::NotFound(_))
    ));
    s.set_enabled(&en, false).unwrap();
    assert!(!s.get_version(&en).unwrap().unwrap().is_enabled);
    // Removing the default promotes another version and deletes verses and index rows.
    s.remove_version(&en).unwrap();
    assert!(s.get_version(&en).unwrap().is_none());
    assert!(s.get_version(&es).unwrap().unwrap().is_default);
    assert_eq!(s.verse_count(&en).unwrap(), 0);
    assert!(s.search("loved", None, 10).unwrap().is_empty());
}

#[test]
fn reinstalling_a_bible_replaces_it() {
    let s = john_store();
    let es = VersionCode("RVR1909".into());
    s.install_bible(
        &version("RVR1909", 1),
        &[verse("Ps", 23, 1, "Jehová es mi pastor.")],
    )
    .unwrap();
    assert_eq!(s.verse_count(&es).unwrap(), 1);
    assert!(s.search("mundo", Some(&es), 10).unwrap().is_empty());
    assert_eq!(s.search("pastor", None, 10).unwrap().len(), 1);
    assert!(
        s.get_version(&es).unwrap().unwrap().is_default,
        "kept as the only default"
    );
}

#[test]
fn verse_marks_round_trip() {
    use hollyshow_core::{MarkKind, TextMark};
    let s = Store::open_in_memory().unwrap();
    let v = Verse {
        marks: vec![TextMark {
            start: 0,
            end: 4,
            kind: MarkKind::WordsOfJesus,
        }],
        ..verse("John", 3, 16, "Dios amó")
    };
    s.install_bible(&version("X", 1), std::slice::from_ref(&v))
        .unwrap();
    let got = s
        .passage(
            &VersionCode("X".into()),
            &Passage::single(v.reference.clone()),
        )
        .unwrap();
    assert_eq!(got, [v]);
}

fn populated() -> Store {
    use hollyshow_storage_api::{PlaylistRepo, SettingsRepo, SongRepo, ThemeRepo};
    let s = Store::open_in_memory().unwrap();
    SongRepo::save(&s, &full_song()).unwrap();
    SongRepo::save(&s, &song("2", "Santo", "Santo es el Señor")).unwrap();
    ThemeRepo::save(&s, &Theme::default_theme()).unwrap();
    PlaylistRepo::save(&s, &playlist("p1", Some("2026-10-11"), false)).unwrap();
    SettingsRepo::set(&s, "ui.lang", "es").unwrap();
    SettingsRepo::set(&s, "session.v1", "{\"volatile\":true}").unwrap();
    SettingsRepo::set(&s, "server.devices.v1", "secret-tokens").unwrap();
    SongRepo::log_usage(&s, &SongId::new("full"), 5).unwrap();
    s
}

#[test]
fn library_export_import_round_trip() {
    let src = populated();
    src.install_bible(
        &version("RVR1909", 1),
        &[verse("John", 3, 16, "Porque de tal manera")],
    )
    .unwrap();
    let json = src
        .export_library_json(LibraryOptions {
            include_bibles: true,
            include_usage: true,
        })
        .unwrap();
    assert!(json.contains("\"format\": \"hollyshow-library\""));
    assert!(
        !json.contains("volatile"),
        "session settings are not exported"
    );
    assert!(
        !json.contains("secret-tokens"),
        "paired device data is not exported"
    );

    let dst = Store::open_in_memory().unwrap();
    let rep = dst.import_library_json(&json, ImportMode::Merge).unwrap();
    assert_eq!(
        (
            rep.songs_added,
            rep.themes,
            rep.playlists,
            rep.settings,
            rep.bibles
        ),
        (2, 1, 1, 1, 1)
    );
    assert_eq!(rep.usage, 1);
    let a = src.export_library(LibraryOptions::default()).unwrap();
    let b = dst.export_library(LibraryOptions::default()).unwrap();
    assert_eq!(
        (a.songs, a.themes, a.playlists, a.settings),
        (b.songs, b.themes, b.playlists, b.settings)
    );
    // Search index and verses came along.
    assert_eq!(
        hollyshow_storage_api::SongRepo::search(&dst, "senor", 5)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(dst.search("manera", None, 5).unwrap().len(), 1);
    // Importing again is idempotent, usage included.
    let again = dst.import_library_json(&json, ImportMode::Merge).unwrap();
    assert_eq!(
        (again.songs_added, again.songs_updated, again.usage),
        (0, 2, 0)
    );
}

#[test]
fn merge_keeps_newer_local_songs_and_replace_wipes() {
    let dst = populated();
    let mut older = full_song();
    older.title = "Versión antigua".into();
    older.updated_at = 1;
    let mut newer = song("2", "Santo (nuevo)", "Santo es el Señor");
    newer.updated_at = 999;
    let lib = Library {
        format: LIBRARY_FORMAT_FOR_TEST.into(),
        version: 1,
        exported_at: 0,
        songs: vec![older, newer],
        themes: vec![],
        playlists: vec![],
        settings: Default::default(),
        bibles: vec![],
        usage: vec![],
    };
    let rep = dst.import_library(&lib, ImportMode::Merge).unwrap();
    assert_eq!((rep.songs_skipped, rep.songs_updated), (1, 1));
    let got = |id: &str| {
        hollyshow_storage_api::SongRepo::get(&dst, &SongId::new(id))
            .unwrap()
            .unwrap()
            .title
    };
    assert_eq!(got("full"), "Sublime Gracia");
    assert_eq!(got("2"), "Santo (nuevo)");

    dst.import_library(&lib, ImportMode::Replace).unwrap();
    assert_eq!(dst.song_count().unwrap(), 2);
    let after = dst.export_library(LibraryOptions::default()).unwrap();
    assert!(after.themes.is_empty() && after.playlists.is_empty() && after.settings.is_empty());
    // Session state survives a replace; the old FTS rows are gone.
    assert_eq!(
        hollyshow_storage_api::SettingsRepo::get(&dst, "session.v1")
            .unwrap()
            .as_deref(),
        Some("{\"volatile\":true}")
    );
    assert!(hollyshow_storage_api::SongRepo::search(&dst, "sublime", 5)
        .unwrap()
        .is_empty());
}

const LIBRARY_FORMAT_FOR_TEST: &str = crate::library::LIBRARY_FORMAT;

#[test]
fn import_rejects_foreign_or_newer_documents_atomically() {
    let s = populated();
    assert!(s.import_library_json("{}", ImportMode::Merge).is_err());
    assert!(s
        .import_library_json("not json", ImportMode::Merge)
        .is_err());
    let newer = r#"{"format":"hollyshow-library","version":99,"exportedAt":0}"#;
    assert!(s.import_library_json(newer, ImportMode::Merge).is_err());
    let wrong = r#"{"format":"other","version":1,"exportedAt":0}"#;
    assert!(s.import_library_json(wrong, ImportMode::Replace).is_err());
    assert_eq!(s.song_count().unwrap(), 2, "nothing was wiped");

    // A document that fails midway rolls back, including the wipe of a Replace.
    let mut lib = s.export_library(LibraryOptions::default()).unwrap();
    let dup = lib.songs[0].sections[0].clone();
    lib.songs[0].sections.push(dup);
    assert!(s.import_library(&lib, ImportMode::Replace).is_err());
    assert_eq!(s.song_count().unwrap(), 2);
}

#[test]
fn data_survives_reopen_and_newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.db");
    {
        let s = Store::open(&path).unwrap();
        hollyshow_storage_api::SongRepo::save(&s, &full_song()).unwrap();
    }
    assert_eq!(Store::open(&path).unwrap().song_count().unwrap(), 1);
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.pragma_update(None, "user_version", migrations::SCHEMA_VERSION + 1)
            .unwrap();
    }
    assert!(matches!(Store::open(&path), Err(StorageError::Invalid(_))));
}

#[test]
fn concurrent_use_through_shared_repos() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let repos = store.repos();
            std::thread::spawn(move || {
                for i in 0..25 {
                    repos
                        .songs
                        .save(&song(
                            &format!("{t}-{i}"),
                            &format!("Canción {t}-{i}"),
                            "letra",
                        ))
                        .unwrap();
                    repos.songs.search("cancion", 5).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(store.song_count().unwrap(), 100);
    store.integrity_check().unwrap();
}

#[test]
fn bible_order_is_persisted_and_ignores_unknown_codes() {
    let store = Store::open_in_memory().unwrap();
    for (i, c) in ["A", "B", "C"].iter().enumerate() {
        store
            .install_bible(&version(c, i as i64), &[verse("Gen", 1, 1, "x")])
            .unwrap();
    }
    let codes = |s: &Store| -> Vec<String> {
        BibleRepo::list_versions(s)
            .unwrap()
            .into_iter()
            .map(|v| v.code.0)
            .collect()
    };
    store
        .set_bible_order(&[
            VersionCode("C".into()),
            VersionCode("nope".into()),
            VersionCode("A".into()),
        ])
        .unwrap();
    // Unlisted versions follow, keeping their relative order.
    assert_eq!(codes(&store), ["C", "A", "B"]);
}
