//! USFM (Paratext, eBible.org, Door43). Accepts one book or many concatenated books.

use super::{plain, utf8, Collector, Declared, ImportMeta, TextBuilder};
use crate::books::{book_by_usfm, Book};
use crate::{BibleError, BibleImporter, ImportedBible};
use hollyshow_core::MarkKind;

#[derive(Debug, Default, Clone)]
pub struct UsfmImporter {
    meta: ImportMeta,
}

impl UsfmImporter {
    pub fn new(meta: ImportMeta) -> Self {
        Self { meta }
    }
}

/// Markers whose text runs to the end of the line and is not scripture.
fn is_line_skip(m: &str) -> bool {
    let base = m.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(
        base,
        "ide"
            | "sts"
            | "rem"
            | "h"
            | "toc"
            | "mt"
            | "mte"
            | "ms"
            | "mr"
            | "s"
            | "sr"
            | "r"
            | "d"
            | "sp"
            | "cl"
            | "cd"
            | "restore"
            | "usfm"
            | "imt"
            | "is"
            | "ip"
            | "ipi"
            | "im"
            | "imi"
            | "ipq"
            | "imq"
            | "ipr"
            | "iq"
            | "ib"
            | "ili"
            | "iot"
            | "io"
            | "iex"
            | "imte"
            | "ie"
            | "lit"
            | "cp"
            | "cat"
    )
}

/// Markers that open a span ended by `\marker*` which is not scripture.
fn is_note(m: &str) -> bool {
    matches!(
        m,
        "f" | "fe"
            | "x"
            | "fig"
            | "ef"
            | "ex"
            | "jmp"
            | "rq"
            | "ndx"
            | "pro"
            | "ph"
            | "va"
            | "vp"
            | "ca"
    )
}

impl BibleImporter for UsfmImporter {
    fn format(&self) -> &'static str {
        "usfm"
    }

    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError> {
        let bytes = plain(input)?;
        let text = utf8(&bytes)?;
        let mut st = State::default();
        let mut rest = text;
        while let Some(pos) = rest.find('\\') {
            st.text(&rest[..pos]);
            rest = &rest[pos + 1..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '+'))
                .unwrap_or(rest.len());
            let mut marker = &rest[..end];
            rest = &rest[end..];
            let closing = rest.starts_with('*');
            if closing {
                rest = &rest[1..];
            }
            marker = marker.trim_start_matches('+');
            if marker.is_empty() {
                continue;
            }
            rest = st.marker(marker, closing, rest);
        }
        st.text(rest);
        st.flush();
        for u in std::mem::take(&mut st.unknown) {
            st.out.unknown(&u);
        }
        st.out.finish("USFM", Declared::default(), &self.meta)
    }
}

#[derive(Default)]
struct State {
    out: Collector,
    book: Option<&'static Book>,
    chapter: u16,
    verse: u16,
    text: Option<TextBuilder>,
    /// Active span to ignore until `\x*`; `None` when reading scripture.
    skipping: Option<String>,
    skip_line: bool,
    unknown: Vec<String>,
    /// Open character spans by marker, to map closings to marks.
    spans: Vec<(String, Option<MarkKind>)>,
}

impl State {
    fn flush(&mut self) {
        if let (Some(book), Some(tb)) = (self.book, self.text.take()) {
            self.out.push(book, self.chapter, self.verse, tb);
        }
    }

    fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if self.skip_line {
            // Drop up to the end of the line; what follows the newline is text again.
            if let Some(i) = s.find('\n') {
                self.skip_line = false;
                self.text(&s[i..]);
            }
            return;
        }
        if self.skipping.is_some() {
            return;
        }
        let mut s = s;
        // Attribute list inside a word-level span (`\w word|strong="G1"\w*`): keep the word only.
        if self.spans.iter().any(|(m, _)| m == "w") {
            if let Some(i) = s.find('|') {
                s = &s[..i];
            }
        }
        if let Some(tb) = self.text.as_mut() {
            tb.push_str(s);
        }
    }

    /// Handles one marker and returns the remaining input.
    fn marker<'a>(&mut self, m: &str, closing: bool, rest: &'a str) -> &'a str {
        if let Some(open) = &self.skipping {
            if closing && m == open {
                self.skipping = None;
            }
            return rest;
        }
        if closing {
            if let Some(i) = self.spans.iter().rposition(|(n, _)| n == m) {
                if let (Some(kind), Some(tb)) = (self.spans.remove(i).1, self.text.as_mut()) {
                    tb.close(kind);
                }
            }
            return rest;
        }
        match m {
            "id" => {
                self.flush();
                let (line, tail) = split_line(rest);
                let code = line.split_whitespace().next().unwrap_or("");
                self.book = book_by_usfm(code);
                self.chapter = 0;
                self.verse = 0;
                if self.book.is_none() && !code.is_empty() {
                    self.unknown.push(code.to_owned());
                }
                return tail;
            }
            "c" => {
                self.flush();
                let (line, tail) = split_line(rest);
                self.chapter = line
                    .split_whitespace()
                    .next()
                    .and_then(super::parse_u16)
                    .unwrap_or(0);
                self.verse = 0;
                return tail;
            }
            "v" => {
                self.flush();
                let trimmed = rest.trim_start_matches([' ', '\t', '\r', '\n']);
                let end = trimmed
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(trimmed.len());
                self.verse = super::parse_u16(&trimmed[..end]).unwrap_or(0);
                self.text = (self.verse > 0 && self.book.is_some()).then(TextBuilder::default);
                return &trimmed[end..];
            }
            "toc1" | "toc2" | "toc3" => {
                return split_line(rest).1;
            }
            "add" => {
                self.open_span(m, MarkKind::Italic);
            }
            "wj" => {
                self.open_span(m, MarkKind::WordsOfJesus);
            }
            "w" => self.spans.push(("w".to_owned(), None)),
            _ if is_note(m) => {
                self.skipping = Some(m.to_owned());
            }
            _ if is_line_skip(m) => {
                self.skip_line = true;
            }
            // Paragraph / poetry / list markers break the line.
            "p" | "m" | "b" | "nb" | "pi" | "mi" | "cls" | "q" | "q1" | "q2" | "q3" | "q4"
            | "qr" | "qc" | "qm" | "li" | "li1" | "li2" | "li3" | "pc" | "pr" | "po" | "lh"
            | "lf" | "lim" | "tr" | "th" | "tc" => {
                if let Some(tb) = self.text.as_mut() {
                    tb.push_str(" ");
                }
            }
            _ => {}
        }
        rest
    }

    fn open_span(&mut self, name: &str, kind: MarkKind) {
        self.spans.push((name.to_owned(), Some(kind)));
        if let Some(tb) = self.text.as_mut() {
            tb.open(kind);
        }
    }
}

fn split_line(s: &str) -> (&str, &str) {
    match s.find('\n') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\\id JHN Test Bible\n\\ide UTF-8\n\\h John\n\\toc1 The Gospel of John\n\\mt1 John\n\\c 3\n\\s1 New Birth\n\\p\n\\v 16 For God so \\add loved\\add* the \\w world|strong=\"G2889\"\\w* that he gave\\f + \\ft A note.\\f* his Son.\n\\v 17 \\wj For God sent not\\wj*\n\\q1 his Son\n\\q2 to condemn.\n\\v 18-19 Joined verse text.\n\\c 4\n\\p\n\\v 1 Next chapter.\n\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning.\n";

    #[test]
    fn imports_books_chapters_and_inline_markup() {
        let b = UsfmImporter::new(ImportMeta::default().with_code("T"))
            .import(SAMPLE.as_bytes())
            .unwrap();
        let refs: Vec<String> = b.verses.iter().map(|v| v.reference.to_string()).collect();
        assert_eq!(
            refs,
            ["Gen.1.1", "John.3.16", "John.3.17", "John.3.18", "John.4.1"]
        );
        assert_eq!(
            b.verses[1].text,
            "For God so loved the world that he gave his Son."
        );
        assert_eq!(b.verses[1].marks.len(), 1);
        assert_eq!(b.verses[1].marks[0].kind, MarkKind::Italic);
        assert_eq!(b.verses[2].text, "For God sent not his Son to condemn.");
        assert_eq!(b.verses[2].marks[0].kind, MarkKind::WordsOfJesus);
        assert_eq!(b.verses[3].text, "Joined verse text.");
    }

    #[test]
    fn rejects_unknown_books_and_empty_input() {
        assert!(UsfmImporter::default()
            .import(b"\\id XXX\n\\c 1\n\\v 1 hi")
            .is_err());
        assert!(UsfmImporter::default().import(b"").is_err());
        assert!(UsfmImporter::default()
            .import(b"\\id GEN\n\\c 1\n")
            .is_err());
    }
}
