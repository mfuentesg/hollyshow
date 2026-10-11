//! Bible logic. Owner: builder "bible". Contracts fixed by the architect:
//! * [`ReferenceParser`] resolves free text ("1 Jn 3:16-18", "Juan 3") to a canonical [`Passage`].
//! * [`BibleImporter`] parses a Bible file into [`ImportedBible`] for the storage crate to persist.
//! * Only public-domain texts may be bundled (PLAN §5.2); document sources in `docs/`.

use hollyshow_core::{
    BibleColumn, BibleVersion, ParallelLayout, Passage, Slide, SlideId, SlideKind, Verse,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BibleError {
    #[error("could not parse reference `{0}`")]
    BadReference(String),
    #[error("malformed Bible file: {0}")]
    Malformed(String),
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
/// instead of an error, so parallel display never breaks. The reference is the canonical OSIS
/// string until the localized book-name table lands in this crate.
pub fn build_bible_slide(
    passage: &Passage,
    layout: ParallelLayout,
    columns: &[VersionText<'_>],
) -> Slide {
    let osis = passage.to_osis();
    let columns = columns
        .iter()
        .map(|c| BibleColumn {
            version: c.version.code.clone(),
            reference: osis.clone(),
            text: join_verses(c.verses),
            copyright: Some(c.version.copyright_text.clone()).filter(|t| !t.is_empty()),
        })
        .collect();
    let mut slide = Slide::blank(format!("bible:{osis}"));
    slide.id = SlideId::new(format!("bible:{osis}"));
    slide.kind = SlideKind::Bible;
    slide.label = Some(osis);
    slide.layout = layout;
    slide.bible = columns;
    slide
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
