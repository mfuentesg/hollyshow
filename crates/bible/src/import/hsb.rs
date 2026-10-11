//! `.hsb` / `.hsb.gz`: the compact format of the Bibles bundled with the app.
//!
//! ```text
//! #hsbible 1
//! code=RVR1909
//! name=Reina-Valera 1909
//! language=es
//! license=Public Domain
//! copyright=...
//! public_domain=true
//! versification=kjv
//!
//! Gen.1.1<TAB>En el principio...<TAB>i:3-9;j:12-20
//! ```
//!
//! One verse per line in canonical order; the optional third column lists formatting marks as
//! `kind:start-end` (`i` italic, `j` words of Jesus) in char offsets. Gzip is detected by magic.

use super::{malformed, plain, utf8, Collector, Declared, ImportMeta};
use crate::{BibleError, BibleImporter, ImportedBible};
use hollyshow_core::{BibleRef, MarkKind, TextMark, Verse};

pub const HSB_MAGIC: &str = "#hsbible 1";

#[derive(Debug, Default, Clone)]
pub struct HsbImporter {
    meta: ImportMeta,
}

impl HsbImporter {
    pub fn new(meta: ImportMeta) -> Self {
        Self { meta }
    }
}

impl BibleImporter for HsbImporter {
    fn format(&self) -> &'static str {
        "hsb"
    }

    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError> {
        let bytes = plain(input)?;
        let text = utf8(&bytes)?;
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HSB_MAGIC) {
            return Err(malformed("HSB: missing `#hsbible 1` header"));
        }
        let mut d = Declared::default();
        let mut verses: Vec<Verse> = Vec::new();
        let mut in_body = false;
        for (n, line) in lines.enumerate() {
            if !in_body {
                if line.trim().is_empty() {
                    in_body = true;
                    continue;
                }
                let (k, v) = line
                    .split_once('=')
                    .ok_or_else(|| malformed(format!("HSB: bad header line {}", n + 2)))?;
                let v = v.trim().to_owned();
                match k.trim() {
                    "code" => d.code = Some(v),
                    "name" => d.name = Some(v),
                    "language" => d.language = Some(v),
                    "license" => d.license = Some(v),
                    "copyright" => d.copyright = Some(v),
                    "public_domain" => d.public_domain = Some(v == "true"),
                    "versification" => d.versification = Some(v),
                    _ => {}
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }
            let mut cols = line.splitn(3, '\t');
            let (Some(r), Some(t)) = (cols.next(), cols.next()) else {
                return Err(malformed(format!("HSB: bad verse line {}", n + 2)));
            };
            let reference: BibleRef = r
                .parse()
                .map_err(|_| malformed(format!("HSB: bad reference `{r}`")))?;
            if reference.verse.is_none() || crate::books::book(&reference.book.0).is_none() {
                return Err(malformed(format!("HSB: bad reference `{r}`")));
            }
            let marks = cols
                .next()
                .map(parse_marks)
                .transpose()?
                .unwrap_or_default();
            verses.push(Verse {
                reference,
                text: t.to_owned(),
                marks,
            });
        }
        // Reuse the collector's validation/ordering/merging.
        let mut out = Collector::default();
        for v in verses {
            out.push_verse(v);
        }
        out.finish("HSB", d, &self.meta)
    }
}

fn parse_marks(s: &str) -> Result<Vec<TextMark>, BibleError> {
    let bad = || malformed(format!("HSB: bad marks `{s}`"));
    let mut out = Vec::new();
    for part in s.split(';').filter(|p| !p.is_empty()) {
        let (k, range) = part.split_once(':').ok_or_else(bad)?;
        let (a, b) = range.split_once('-').ok_or_else(bad)?;
        out.push(TextMark {
            start: a.parse().map_err(|_| bad())?,
            end: b.parse().map_err(|_| bad())?,
            kind: match k {
                "i" => MarkKind::Italic,
                "j" => MarkKind::WordsOfJesus,
                _ => return Err(bad()),
            },
        });
    }
    Ok(out)
}

/// Serialises a Bible to the (uncompressed) `.hsb` text. Gzip it for distribution.
pub fn write_hsb(b: &ImportedBible) -> String {
    let v = &b.version;
    let clean = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    let mut s = String::with_capacity(b.verses.len() * 120);
    s.push_str(HSB_MAGIC);
    s.push('\n');
    for (k, val) in [
        ("code", v.code.0.as_str()),
        ("name", v.name.as_str()),
        ("language", v.language.as_str()),
        ("license", v.license.as_str()),
        ("copyright", v.copyright_text.as_str()),
        ("versification", v.versification.as_str()),
    ] {
        s.push_str(&format!("{k}={}\n", clean(val)));
    }
    s.push_str(&format!("public_domain={}\n\n", v.is_public_domain));
    for verse in &b.verses {
        s.push_str(&verse.reference.to_string());
        s.push('\t');
        s.push_str(&clean(&verse.text));
        if !verse.marks.is_empty() {
            s.push('\t');
            let marks: Vec<String> = verse
                .marks
                .iter()
                .map(|m| {
                    let k = match m.kind {
                        MarkKind::Italic => 'i',
                        MarkKind::WordsOfJesus => 'j',
                    };
                    format!("{k}:{}-{}", m.start, m.end)
                })
                .collect();
            s.push_str(&marks.join(";"));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{OsisImporter, UsfmImporter};
    use std::io::Write;

    #[test]
    fn round_trips_text_marks_and_metadata() {
        let usfm = "\\id JHN\n\\c 3\n\\p\n\\v 16 For \\add God\\add* so loved.\n\\v 17 \\wj Not to condemn\\wj*.\n";
        let src = UsfmImporter::new(
            ImportMeta::default()
                .with_code("TST")
                .with_name("Test")
                .with_language("en")
                .with_public_domain(true)
                .with_copyright("PD, tab\there"),
        )
        .import(usfm.as_bytes())
        .unwrap();
        let text = write_hsb(&src);
        let back = HsbImporter::default().import(text.as_bytes()).unwrap();
        assert_eq!(back.version.code, src.version.code);
        assert_eq!(back.version.language, "en");
        assert!(back.version.is_public_domain);
        assert_eq!(back.version.copyright_text, "PD, tab here");
        assert_eq!(back.verses, src.verses);

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        gz.write_all(text.as_bytes()).unwrap();
        let gz = gz.finish().unwrap();
        let again = HsbImporter::default().import(&gz).unwrap();
        assert_eq!(again.verses, src.verses);
    }

    #[test]
    fn rejects_malformed_files() {
        for bad in [
            "",
            "hello",
            "#hsbible 1\ncode=X\n\nNarnia.1.1\tx\n",
            "#hsbible 1\ncode=X\n\nGen.1\tx\n",
            "#hsbible 1\ncode=X\n\nGen.1.1\n",
            "#hsbible 1\nnonsense\n\nGen.1.1\tx\n",
            "#hsbible 1\ncode=X\n\nGen.1.1\tx\tq:1-2\n",
        ] {
            assert!(
                HsbImporter::default().import(bad.as_bytes()).is_err(),
                "{bad:?}"
            );
        }
        let _ = OsisImporter::default();
    }
}
