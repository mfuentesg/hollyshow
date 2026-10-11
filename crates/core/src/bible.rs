//! Version independent Bible references (canonical OSIS) and installed-version metadata.

use crate::ids::ThemeId;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use ts_rs::TS;

/// OSIS book id such as `Gen`, `John`, `1John`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BookId(pub String);

/// Short code of an installed Bible version, e.g. `RVR1909`, `KJV`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VersionCode(pub String);

/// A position in the canonical OSIS system. `verse == None` addresses a whole chapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BibleRef {
    pub book: BookId,
    pub chapter: u16,
    pub verse: Option<u16>,
}

impl BibleRef {
    pub fn verse(book: &str, chapter: u16, verse: u16) -> Self {
        Self {
            book: BookId(book.to_owned()),
            chapter,
            verse: Some(verse),
        }
    }
    pub fn chapter(book: &str, chapter: u16) -> Self {
        Self {
            book: BookId(book.to_owned()),
            chapter,
            verse: None,
        }
    }
}

impl fmt::Display for BibleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.verse {
            Some(v) => write!(f, "{}.{}.{}", self.book.0, self.chapter, v),
            None => write!(f, "{}.{}", self.book.0, self.chapter),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OsisParseError {
    #[error("empty OSIS reference")]
    Empty,
    #[error("malformed OSIS reference `{0}`")]
    Malformed(String),
}

impl std::str::FromStr for BibleRef {
    type Err = OsisParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(OsisParseError::Empty);
        }
        let bad = || OsisParseError::Malformed(s.to_owned());
        let mut parts = s.split('.');
        let book = parts.next().filter(|b| is_book_id(b)).ok_or_else(bad)?;
        let chapter: u16 = parts
            .next()
            .and_then(|c| c.parse().ok())
            .filter(|c| *c > 0)
            .ok_or_else(bad)?;
        let verse = match parts.next() {
            None => None,
            Some(v) => Some(v.parse::<u16>().ok().filter(|v| *v > 0).ok_or_else(bad)?),
        };
        if parts.next().is_some() {
            return Err(bad());
        }
        Ok(Self {
            book: BookId(book.to_owned()),
            chapter,
            verse,
        })
    }
}

fn is_book_id(b: &str) -> bool {
    !b.is_empty() && b.len() <= 12 && b.chars().all(|c| c.is_ascii_alphanumeric())
}

/// A contiguous passage `start..=end` in canonical order, e.g. `John.3.16-John.3.18`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Passage {
    pub start: BibleRef,
    pub end: BibleRef,
}

impl Passage {
    pub fn single(r: BibleRef) -> Self {
        Self {
            start: r.clone(),
            end: r,
        }
    }

    pub fn range(start: BibleRef, end: BibleRef) -> Self {
        Self { start, end }
    }

    /// Canonical OSIS string: `John.3.16`, `John.3.16-John.3.18`, `Ps.23`.
    pub fn to_osis(&self) -> String {
        if self.start == self.end {
            self.start.to_string()
        } else {
            format!("{}-{}", self.start, self.end)
        }
    }

    /// Parses `John.3.16`, `John.3.16-John.3.18` and the shorthand `John.3.16-18`.
    pub fn parse_osis(s: &str) -> Result<Self, OsisParseError> {
        let s = s.trim();
        match s.split_once('-') {
            None => Ok(Self::single(s.parse()?)),
            Some((a, b)) => {
                let start: BibleRef = a.parse()?;
                let end: BibleRef = match b.parse::<BibleRef>() {
                    Ok(end) => end,
                    Err(_) => {
                        let v: u16 = b
                            .trim()
                            .parse()
                            .ok()
                            .filter(|v| *v > 0)
                            .ok_or_else(|| OsisParseError::Malformed(s.to_owned()))?;
                        BibleRef {
                            book: start.book.clone(),
                            chapter: start.chapter,
                            verse: Some(v),
                        }
                    }
                };
                Ok(Self { start, end })
            }
        }
    }
}

impl fmt::Display for Passage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_osis())
    }
}

/// How several versions of the same passage share the screen (PLAN §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ParallelLayout {
    #[default]
    Columns,
    Stacked,
    /// Primary version large, secondary smaller (subtitle style).
    Subtitle,
}

/// Inline formatting kept for projection; footnotes are discarded at import time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MarkKind {
    /// Translator-added words.
    Italic,
    WordsOfJesus,
}

/// Formatting span over `Verse::text`, in Unicode scalar (char) offsets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TextMark {
    pub start: u32,
    pub end: u32,
    pub kind: MarkKind,
}

/// One verse of one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Verse {
    pub reference: BibleRef,
    pub text: String,
    #[serde(default)]
    pub marks: Vec<TextMark>,
}

/// Metadata of an installed Bible version (`bibles` table, PLAN §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BibleVersion {
    pub code: VersionCode,
    pub name: String,
    /// BCP-47 language tag, e.g. `es`, `en`, `pt`.
    pub language: String,
    pub license: String,
    pub copyright_text: String,
    pub is_public_domain: bool,
    pub versification: String,
    #[ts(type = "number")]
    pub sort_order: i64,
    pub is_enabled: bool,
    pub is_default: bool,
    pub is_secondary: bool,
}

/// Full-text hit across one or all versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BibleHit {
    pub version: VersionCode,
    pub reference: BibleRef,
    pub text: String,
}

/// Optional per-theme override used when rendering Bible slides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BibleDisplayOptions {
    pub layout: ParallelLayout,
    pub show_version_label: bool,
    pub show_copyright: bool,
    pub theme_id: Option<ThemeId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osis_round_trip() {
        for s in [
            "John.3.16",
            "John.3.16-John.3.18",
            "Ps.23",
            "1John.1.9",
            "Gen.1.31-Gen.2.3",
        ] {
            assert_eq!(Passage::parse_osis(s).unwrap().to_osis(), s);
        }
    }

    #[test]
    fn osis_shorthand_range() {
        let p = Passage::parse_osis("John.3.16-18").unwrap();
        assert_eq!(p.end, BibleRef::verse("John", 3, 18));
        assert_eq!(p.to_osis(), "John.3.16-John.3.18");
    }

    #[test]
    fn osis_rejects_garbage() {
        for s in [
            "",
            "John",
            "John.0.1",
            "John.3.x",
            "Jo hn.3.1",
            "John.3.16.2",
            "John.3.16-",
        ] {
            assert!(Passage::parse_osis(s).is_err(), "{s} should fail");
        }
    }

    #[test]
    fn canonical_order_within_book() {
        assert!(BibleRef::verse("John", 3, 16) < BibleRef::verse("John", 3, 17));
        assert!(BibleRef::chapter("John", 3) < BibleRef::verse("John", 3, 1));
    }
}
