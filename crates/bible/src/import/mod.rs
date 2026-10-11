//! Bible file importers: OSIS, USFM, Zefania XML, OpenSong XML, JSON, CSV and the compact
//! bundled `.hsb` format. Every importer is pure (bytes in, [`ImportedBible`] out), never
//! panics on malformed input and returns [`BibleError::Malformed`] instead.
//!
//! Footnotes, cross references, section headings and Strong's numbers are dropped (nothing to
//! project); translator-added words (italics) and the words of Jesus are kept as [`TextMark`]s.
//! Verses come back sorted in canonical book order, which is what the SQLite store requires.

mod hsb;
mod opensong;
mod osis;
mod tabular;
mod usfm;
mod zefania;

pub use hsb::{write_hsb, HsbImporter};
pub use opensong::OpenSongImporter;
pub use osis::OsisImporter;
pub use tabular::{CsvImporter, JsonImporter};
pub use usfm::UsfmImporter;
pub use zefania::ZefaniaImporter;

use crate::books::{resolve_book, Book, Lang};
use crate::{BibleError, BibleImporter, ImportedBible};
use hollyshow_core::{BibleRef, BibleVersion, BookId, MarkKind, TextMark, Verse, VersionCode};
use std::collections::BTreeSet;
use std::io::Read;

/// Largest decompressed input accepted (a full Bible with markup is well under 20 MB).
const MAX_INPUT: u64 = 128 * 1024 * 1024;

/// Metadata supplied by the caller; wins over whatever the file declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportMeta {
    pub code: Option<String>,
    pub name: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub copyright: Option<String>,
    pub public_domain: Option<bool>,
    pub versification: Option<String>,
}

impl ImportMeta {
    pub fn with_code(mut self, c: &str) -> Self {
        self.code = Some(c.to_owned());
        self
    }
    pub fn with_name(mut self, c: &str) -> Self {
        self.name = Some(c.to_owned());
        self
    }
    pub fn with_language(mut self, c: &str) -> Self {
        self.language = Some(c.to_owned());
        self
    }
    pub fn with_license(mut self, c: &str) -> Self {
        self.license = Some(c.to_owned());
        self
    }
    pub fn with_copyright(mut self, c: &str) -> Self {
        self.copyright = Some(c.to_owned());
        self
    }
    pub fn with_public_domain(mut self, pd: bool) -> Self {
        self.public_domain = Some(pd);
        self
    }
    pub fn with_versification(mut self, c: &str) -> Self {
        self.versification = Some(c.to_owned());
        self
    }
}

/// Metadata found inside the file.
#[derive(Debug, Clone, Default)]
pub(crate) struct Declared {
    pub code: Option<String>,
    pub name: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub copyright: Option<String>,
    pub public_domain: Option<bool>,
    pub versification: Option<String>,
}

pub(crate) fn malformed(msg: impl Into<String>) -> BibleError {
    BibleError::Malformed(msg.into())
}

/// Decodes UTF-8 (BOM tolerated) or reports malformed input.
pub(crate) fn utf8(input: &[u8]) -> Result<&str, BibleError> {
    let input = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(input);
    std::str::from_utf8(input).map_err(|e| malformed(format!("not valid UTF-8: {e}")))
}

/// Accumulates one verse's text collapsing whitespace, tracking formatting marks by char offset.
#[derive(Debug, Default)]
pub(crate) struct TextBuilder {
    text: String,
    chars: u32,
    marks: Vec<TextMark>,
    open: Vec<(MarkKind, u32)>,
}

impl TextBuilder {
    pub fn push_str(&mut self, s: &str) {
        for c in s.chars() {
            if c.is_whitespace() || c == '\u{a0}' {
                if !self.text.is_empty() && !self.text.ends_with(' ') {
                    self.text.push(' ');
                    self.chars += 1;
                }
            } else if !c.is_control() {
                self.text.push(c);
                self.chars += 1;
            }
        }
    }

    pub fn open(&mut self, kind: MarkKind) {
        self.open.push((kind, self.chars));
    }

    pub fn close(&mut self, kind: MarkKind) {
        if let Some(i) = self.open.iter().rposition(|(k, _)| *k == kind) {
            let (_, start) = self.open.remove(i);
            self.marks.push(TextMark {
                start,
                end: self.chars,
                kind,
            });
        }
    }

    pub fn finish(mut self) -> (String, Vec<TextMark>) {
        // Unclosed marks end with the verse.
        while let Some((kind, start)) = self.open.pop() {
            self.marks.push(TextMark {
                start,
                end: self.chars,
                kind,
            });
        }
        let trimmed = self.text.trim_end().to_owned();
        let chars: Vec<char> = trimmed.chars().collect();
        let len = u32::try_from(chars.len()).unwrap_or(u32::MAX);
        let mut marks = Vec::new();
        for m in self.marks {
            let (mut s, mut e) = (m.start.min(len), m.end.min(len));
            while s < e && chars[s as usize] == ' ' {
                s += 1;
            }
            while e > s && chars[e as usize - 1] == ' ' {
                e -= 1;
            }
            if s < e {
                marks.push(TextMark {
                    start: s,
                    end: e,
                    kind: m.kind,
                });
            }
        }
        marks.sort_by_key(|m| (m.start, m.end));
        (trimmed, marks)
    }
}

/// Collects verses, resolves books, and finally orders and validates everything.
#[derive(Debug, Default)]
pub(crate) struct Collector {
    verses: Vec<Verse>,
    unknown_books: BTreeSet<String>,
}

impl Collector {
    pub fn push(&mut self, book: &Book, chapter: u16, verse: u16, text: TextBuilder) {
        if chapter == 0 || verse == 0 {
            return;
        }
        let (text, marks) = text.finish();
        if text.is_empty() {
            return;
        }
        self.verses.push(Verse {
            reference: BibleRef {
                book: BookId(book.osis.to_owned()),
                chapter,
                verse: Some(verse),
            },
            text,
            marks,
        });
    }

    /// Adds an already-built verse (marks included).
    pub fn push_verse(&mut self, verse: Verse) {
        if verse.reference.chapter == 0 || verse.reference.verse.unwrap_or(0) == 0 {
            return;
        }
        if verse.text.trim().is_empty() {
            return;
        }
        self.verses.push(verse);
    }

    pub fn push_plain(&mut self, book: &Book, chapter: u16, verse: u16, text: &str) {
        let mut b = TextBuilder::default();
        b.push_str(text);
        self.push(book, chapter, verse, b);
    }

    pub fn unknown(&mut self, name: &str) {
        self.unknown_books.insert(name.to_owned());
    }

    pub fn finish(
        self,
        format: &str,
        declared: Declared,
        meta: &ImportMeta,
    ) -> Result<ImportedBible, BibleError> {
        if !self.unknown_books.is_empty() {
            let names: Vec<_> = self.unknown_books.into_iter().take(5).collect();
            return Err(malformed(format!(
                "{format}: unrecognised book(s): {}",
                names.join(", ")
            )));
        }
        let mut verses = self.verses;
        if verses.is_empty() {
            return Err(malformed(format!("{format}: no verses found")));
        }
        verses.sort_by_key(|v| {
            (
                crate::books::book_ord(&v.reference.book).unwrap_or(usize::MAX),
                v.reference.chapter,
                v.reference.verse,
            )
        });
        // A verse appearing twice (split paragraphs) is joined rather than lost.
        let mut merged: Vec<Verse> = Vec::with_capacity(verses.len());
        for v in verses {
            match merged.last_mut() {
                Some(last) if last.reference == v.reference => {
                    let offset = u32::try_from(last.text.chars().count() + 1).unwrap_or(u32::MAX);
                    last.text.push(' ');
                    last.text.push_str(&v.text);
                    last.marks.extend(v.marks.into_iter().map(|m| TextMark {
                        start: m.start + offset,
                        end: m.end + offset,
                        kind: m.kind,
                    }));
                }
                _ => merged.push(v),
            }
        }
        Ok(ImportedBible {
            version: build_version(declared, meta),
            verses: merged,
        })
    }
}

fn build_version(d: Declared, m: &ImportMeta) -> BibleVersion {
    let name = m.name.clone().or(d.name).filter(|s| !s.trim().is_empty());
    let code = m
        .code
        .clone()
        .or(d.code)
        .map(|c| sanitize_code(&c))
        .filter(|c| !c.is_empty())
        .or_else(|| name.as_deref().map(sanitize_code).filter(|c| !c.is_empty()))
        .unwrap_or_else(|| "IMPORTED".to_owned());
    let language = m
        .language
        .clone()
        .or(d.language)
        .map(|l| normalise_language(&l))
        .unwrap_or_else(|| "und".to_owned());
    let public_domain = m.public_domain.or(d.public_domain).unwrap_or(false);
    let license = m.license.clone().or(d.license).unwrap_or_else(|| {
        if public_domain {
            "Public Domain"
        } else {
            "unknown"
        }
        .to_owned()
    });
    BibleVersion {
        name: name.unwrap_or_else(|| code.clone()),
        code: VersionCode(code),
        language,
        license,
        copyright_text: m.copyright.clone().or(d.copyright).unwrap_or_default(),
        is_public_domain: public_domain,
        versification: m
            .versification
            .clone()
            .or(d.versification)
            .unwrap_or_else(|| "kjv".to_owned()),
        sort_order: 0,
        is_enabled: true,
        is_default: false,
        is_secondary: false,
    }
}

/// Version codes are short identifiers: ASCII letters and digits, at most 16 characters.
fn sanitize_code(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(16)
        .collect::<String>()
        .to_ascii_uppercase()
}

fn normalise_language(l: &str) -> String {
    match Lang::from_tag(l) {
        Some(Lang::Es) => "es".to_owned(),
        Some(Lang::En) => "en".to_owned(),
        Some(Lang::Pt) => "pt".to_owned(),
        None => {
            let p = l
                .split(['-', '_'])
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if p.is_empty() {
                "und".to_owned()
            } else {
                p
            }
        }
    }
}

/// Resolves the book column/attribute of a tabular or XML file: a name, abbreviation, OSIS id,
/// USFM code or the 1-based canonical number.
pub(crate) fn lookup_book(name: &str) -> Option<&'static Book> {
    let name = name.trim();
    if let Ok(n) = name.parse::<usize>() {
        return crate::books::book_by_ord(n);
    }
    crate::books::book(name)
        .or_else(|| crate::books::book_by_usfm(name))
        .or_else(|| resolve_book(name, None))
}

pub(crate) fn parse_u16(s: &str) -> Option<u16> {
    let digits: String = s.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Reads at most [`MAX_INPUT`] bytes of gzip output.
pub(crate) fn gunzip(input: &[u8]) -> Result<Vec<u8>, BibleError> {
    let mut out = Vec::new();
    let dec = flate2::read::GzDecoder::new(input);
    dec.take(MAX_INPUT + 1)
        .read_to_end(&mut out)
        .map_err(|e| malformed(format!("gzip: {e}")))?;
    if out.len() as u64 > MAX_INPUT {
        return Err(malformed("decompressed input is too large"));
    }
    Ok(out)
}

/// Transparent gzip support for every importer.
pub(crate) fn plain(input: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>, BibleError> {
    if input.starts_with(&[0x1f, 0x8b]) {
        Ok(std::borrow::Cow::Owned(gunzip(input)?))
    } else {
        Ok(std::borrow::Cow::Borrowed(input))
    }
}

/// Parses XML without DTD support (entity-expansion bombs cannot occur).
pub(crate) fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>, BibleError> {
    roxmltree::Document::parse(text).map_err(|e| malformed(format!("XML: {e}")))
}

/// Guesses the format of `input` (gzip is looked through): `osis`, `usfm`, `zefania`,
/// `opensong`, `json`, `csv` or `hsb`.
pub fn detect_format(input: &[u8]) -> Option<&'static str> {
    if input.starts_with(&[0x1f, 0x8b]) {
        let inner = gunzip(input).ok()?;
        return detect_format(&inner);
    }
    let input = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(input);
    let head = String::from_utf8_lossy(&input[..input.len().min(4096)]);
    let t = head.trim_start();
    if t.starts_with("#hsbible") {
        return Some("hsb");
    }
    if t.starts_with('<') {
        let lower = head.to_ascii_lowercase();
        return if lower.contains("<osis") {
            Some("osis")
        } else if lower.contains("<xmlbible") {
            Some("zefania")
        } else if lower.contains("<bible") {
            Some("opensong")
        } else {
            None
        };
    }
    if t.starts_with('{') || t.starts_with('[') {
        return Some("json");
    }
    if t.starts_with("\\id ") || t.contains("\n\\c ") || t.starts_with("\\c ") {
        return Some("usfm");
    }
    if t.contains(',') || t.contains('\t') || t.contains(';') {
        return Some("csv");
    }
    None
}

/// The importer for a format name returned by [`BibleImporter::format`].
pub fn importer_for(format: &str, meta: ImportMeta) -> Option<Box<dyn BibleImporter>> {
    Some(match format {
        "osis" => Box::new(OsisImporter::new(meta)),
        "usfm" => Box::new(UsfmImporter::new(meta)),
        "zefania" => Box::new(ZefaniaImporter::new(meta)),
        "opensong" => Box::new(OpenSongImporter::new(meta)),
        "json" => Box::new(JsonImporter::new(meta)),
        "csv" => Box::new(CsvImporter::new(meta)),
        "hsb" => Box::new(HsbImporter::new(meta)),
        _ => return None,
    })
}

/// Detects the format (gzip-aware) and imports. A multi-file format (USFM, one file per book) is
/// not handled here: concatenate the books first.
pub fn import_auto(input: &[u8], meta: ImportMeta) -> Result<ImportedBible, BibleError> {
    let format = detect_format(input).ok_or_else(|| malformed("unrecognised Bible file format"))?;
    let importer = importer_for(format, meta).ok_or_else(|| malformed("unsupported format"))?;
    importer.import(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_builder_collapses_space_and_tracks_marks() {
        let mut b = TextBuilder::default();
        b.push_str("  In the \n beginning ");
        b.open(MarkKind::Italic);
        b.push_str(" was ");
        b.close(MarkKind::Italic);
        b.push_str("the   Word  ");
        let (t, m) = b.finish();
        assert_eq!(t, "In the beginning was the Word");
        assert_eq!(m.len(), 1);
        let chars: Vec<char> = t.chars().collect();
        let marked: String = chars[m[0].start as usize..m[0].end as usize]
            .iter()
            .collect();
        assert_eq!(marked, "was");
    }

    #[test]
    fn unclosed_marks_end_with_the_verse() {
        let mut b = TextBuilder::default();
        b.open(MarkKind::WordsOfJesus);
        b.push_str("Follow me");
        let (t, m) = b.finish();
        assert_eq!(t, "Follow me");
        assert_eq!((m[0].start, m[0].end), (0, 9));
    }

    #[test]
    fn code_and_language_are_normalised() {
        assert_eq!(sanitize_code("rvr-1909 (es)"), "RVR1909ES");
        assert_eq!(normalise_language("spa"), "es");
        assert_eq!(normalise_language("en-US"), "en");
        assert_eq!(normalise_language("GRC"), "grc");
        assert_eq!(normalise_language(""), "und");
    }

    #[test]
    fn book_lookup_accepts_every_identifier_style() {
        for s in ["Genesis", "Gen", "GEN", "1", "Génesis"] {
            assert_eq!(lookup_book(s).unwrap().osis, "Gen", "{s}");
        }
        assert_eq!(lookup_book("I Samuel").unwrap().osis, "1Sam");
        assert_eq!(lookup_book("Revelation of John").unwrap().osis, "Rev");
        assert!(lookup_book("Front Matter").is_none());
    }

    #[test]
    fn detects_formats() {
        assert_eq!(detect_format(b"<osis xmlns='x'></osis>"), Some("osis"));
        assert_eq!(
            detect_format(b"<?xml version='1.0'?><XMLBIBLE/>"),
            Some("zefania")
        );
        assert_eq!(
            detect_format(b"<bible><b n='Genesis'/></bible>"),
            Some("opensong")
        );
        assert_eq!(detect_format(b"\\id GEN\n\\c 1\n\\v 1 x"), Some("usfm"));
        assert_eq!(detect_format(b"{\"books\":[]}"), Some("json"));
        assert_eq!(detect_format(b"Book,Chapter,Verse,Text\n"), Some("csv"));
        assert_eq!(detect_format(b"#hsbible 1\n"), Some("hsb"));
        assert_eq!(detect_format(b"hello"), None);
    }
}
