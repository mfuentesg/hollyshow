//! Bible logic. Owner: builder "bible". Contracts fixed by the architect:
//! * [`ReferenceParser`] resolves free text ("1 Jn 3:16-18", "Juan 3") to a canonical [`Passage`].
//! * [`BibleImporter`] parses a Bible file into [`ImportedBible`] for the storage crate to persist.
//! * Only public-domain texts may be bundled (PLAN §5.2); sources are documented in
//!   `apps/desktop/src-tauri/resources/bibles/SOURCES.md`.
//!
//! Modules:
//! * [`books`]: canonical OSIS book list (with deuterocanon) and es/en/pt names.
//! * [`reference`]: tolerant reference parser and localised formatting.
//! * [`import`]: OSIS, USFM, Zefania, OpenSong, JSON, CSV and the bundled `.hsb` format.
//! * [`versification`]: KJV/Vulgate/Hebrew numbering maps and parallel alignment.
//! * [`search`]: in-memory full-text index per version.
//! * [`install`]: first-run installation of the bundled versions through a storage port.

pub mod books;
mod canon_data;
pub mod import;
pub mod install;
pub mod reference;
pub mod search;
pub mod versification;

pub use books::{Book, Lang, Testament};
pub use import::{import_auto, ImportMeta};
pub use install::{install_bundled, BibleSink, InstallReport};
pub use reference::{format_passage, format_ref, parse_references, Parser};
pub use search::BibleIndex;
pub use versification::{align_parallel, load_canonical, AlignedRow, Versification};

use hollyshow_core::{
    BibleColumn, BibleRef, BibleVersion, BookId, ParallelLayout, Passage, Slide, SlideId,
    SlideKind, Verse,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BibleError {
    #[error("could not parse reference `{0}`")]
    BadReference(String),
    #[error("malformed Bible file: {0}")]
    Malformed(String),
    #[error("could not install the bundled Bibles: {0}")]
    Install(String),
}

pub trait ReferenceParser {
    /// `language` is a BCP-47 hint (`es`, `en`, `pt`); implementations must accept any language's
    /// book names when unambiguous.
    fn parse(&self, input: &str, language: &str) -> Result<Passage, BibleError>;
}

pub struct ImportedBible {
    pub version: BibleVersion,
    pub verses: Vec<Verse>,
}

pub trait BibleImporter {
    /// `osis`, `usfm`, `zefania`, `opensong`, `csv`, `json`.
    fn format(&self) -> &'static str;
    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError>;
}

/// One version's verses for a passage, as read from the library.
pub struct VersionText<'a> {
    pub version: &'a BibleVersion,
    pub verses: &'a [Verse],
}

/// Builds the renderable Bible slide for `passage`, one column per version.
///
/// A verse missing from a version (different canon or versification) yields an explicit dash
/// instead of an error, so parallel display never breaks. Each column's reference is localised
/// in its own version's language ("Juan 3:16" next to "John 3:16"); a version in an unknown
/// language falls back to Spanish book names. Verses must be keyed canonically (see
/// [`load_canonical`]).
pub fn build_bible_slide(
    passage: &Passage,
    layout: ParallelLayout,
    columns: &[VersionText<'_>],
) -> Slide {
    let osis = passage.to_osis();
    let columns: Vec<BibleColumn> = columns
        .iter()
        .map(|c| BibleColumn {
            version: c.version.code.clone(),
            reference: format_passage(passage, lang_of(c.version)),
            text: join_verses(c.verses),
            copyright: Some(c.version.copyright_text.clone()).filter(|t| !t.is_empty()),
        })
        .collect();
    let mut slide = Slide::blank(format!("bible:{osis}"));
    slide.id = SlideId::new(format!("bible:{osis}"));
    slide.kind = SlideKind::Bible;
    slide.label = Some(
        columns
            .first()
            .map_or_else(|| osis.clone(), |c| c.reference.clone()),
    );
    slide.layout = layout;
    slide.bible = columns;
    slide
}

fn lang_of(v: &BibleVersion) -> Lang {
    Lang::from_tag(&v.language).unwrap_or(Lang::Es)
}

/// Result of comparing a version with the standard 66-book, 31,102-verse canon.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonReport {
    pub total: usize,
    /// Standard verses the version lacks (Acts 8:37 in critical editions, whole books...).
    pub missing: Vec<BibleRef>,
    /// Verses outside the standard canon (deuterocanon, extra numbering).
    pub extra: Vec<BibleRef>,
}

impl CanonReport {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Compares `verses` (canonical numbering) with the standard canon. Used to verify bundled
/// texts (31,102 verses) and to tell the operator what an imported version lacks.
pub fn canon_report(verses: &[Verse]) -> CanonReport {
    use std::collections::HashSet;
    let have: HashSet<(&str, u16, u16)> = verses
        .iter()
        .filter_map(|v| {
            v.reference
                .verse
                .map(|n| (v.reference.book.0.as_str(), v.reference.chapter, n))
        })
        .collect();
    let mut expected: HashSet<(&str, u16, u16)> = HashSet::new();
    let mut missing = Vec::new();
    for b in &books::BOOKS[..books::PROTESTANT_BOOKS] {
        for ch in 1..=u16::from(b.chapters) {
            for v in 1..=books::verse_count(b.osis, ch).unwrap_or(0) {
                expected.insert((b.osis, ch, v));
                if !have.contains(&(b.osis, ch, v)) {
                    missing.push(BibleRef::verse(b.osis, ch, v));
                }
            }
        }
    }
    let extra = verses
        .iter()
        .filter(|v| {
            v.reference.verse.is_none_or(|n| {
                !expected.contains(&(v.reference.book.0.as_str(), v.reference.chapter, n))
            })
        })
        .map(|v| v.reference.clone())
        .collect();
    CanonReport {
        total: verses.len(),
        missing,
        extra,
    }
}

/// Canonical sort key of a reference (book order, chapter, verse).
pub fn canonical_key(r: &BibleRef) -> (usize, u16, u16) {
    (
        books::book_ord(&BookId(r.book.0.clone())).unwrap_or(usize::MAX),
        r.chapter,
        r.verse.unwrap_or(0),
    )
}

fn join_verses(verses: &[Verse]) -> String {
    if verses.is_empty() {
        return "\u{2014}".to_owned();
    }
    verses
        .iter()
        .map(|v| match v.reference.verse {
            Some(n) if verses.len() > 1 => format!("{n} {}", v.text),
            _ => v.text.clone(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{BibleRef, VersionCode};

    fn version(code: &str, copyright: &str) -> BibleVersion {
        BibleVersion {
            code: VersionCode(code.into()),
            name: code.into(),
            language: "es".into(),
            license: "PD".into(),
            copyright_text: copyright.into(),
            is_public_domain: true,
            versification: "kjv".into(),
            sort_order: 0,
            is_enabled: true,
            is_default: false,
            is_secondary: false,
        }
    }

    fn verses(range: std::ops::RangeInclusive<u16>) -> Vec<Verse> {
        range
            .map(|n| Verse {
                reference: BibleRef::verse("John", 3, n),
                text: format!("texto {n}"),
                marks: vec![],
            })
            .collect()
    }

    #[test]
    fn column_references_are_localised_per_version() {
        let mut en = version("KJV", "PD");
        en.language = "en".into();
        let es = version("RVR", "PD");
        let (v1, v2) = (verses(16..=17), verses(16..=17));
        let slide = build_bible_slide(
            &Passage::parse_osis("John.3.16-John.3.17").unwrap(),
            ParallelLayout::Columns,
            &[
                VersionText {
                    version: &es,
                    verses: &v1,
                },
                VersionText {
                    version: &en,
                    verses: &v2,
                },
            ],
        );
        assert_eq!(slide.bible[0].reference, "Juan 3:16\u{2013}17");
        assert_eq!(slide.bible[1].reference, "John 3:16\u{2013}17");
        assert_eq!(slide.label.as_deref(), Some("Juan 3:16\u{2013}17"));
        assert_eq!(slide.id.0, "bible:John.3.16-John.3.17");
    }

    #[test]
    fn canon_report_lists_missing_and_extra() {
        let v = |b: &str, c, n| Verse {
            reference: BibleRef::verse(b, c, n),
            text: "x".into(),
            marks: vec![],
        };
        let r = canon_report(&[v("Gen", 1, 1), v("Tob", 1, 1), v("Gen", 1, 99)]);
        assert_eq!(r.total, 3);
        assert_eq!(r.missing.len(), 31_102 - 1);
        assert_eq!(r.extra.len(), 2);
        assert!(!r.is_complete());
    }

    #[test]
    fn numbers_verses_only_in_ranges_and_marks_missing_ones() {
        let (a, b) = (version("A", "PD"), version("B", ""));
        let (va, vb) = (verses(16..=17), verses(16..=16));
        let none: Vec<Verse> = vec![];
        let c = version("C", "");
        let slide = build_bible_slide(
            &Passage::parse_osis("John.3.16-17").unwrap(),
            ParallelLayout::Columns,
            &[
                VersionText {
                    version: &a,
                    verses: &va,
                },
                VersionText {
                    version: &b,
                    verses: &vb,
                },
                VersionText {
                    version: &c,
                    verses: &none,
                },
            ],
        );
        assert_eq!(slide.kind, SlideKind::Bible);
        assert_eq!(slide.bible[0].text, "16 texto 16 17 texto 17");
        assert_eq!(slide.bible[1].text, "texto 16");
        assert_eq!(slide.bible[2].text, "\u{2014}");
        assert_eq!(slide.bible[0].copyright.as_deref(), Some("PD"));
        assert_eq!(slide.bible[1].copyright, None);
    }
}
