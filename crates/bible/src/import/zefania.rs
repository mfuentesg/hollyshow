//! Zefania XML (`<XMLBIBLE>`).

use super::{
    lookup_book, malformed, parse_u16, parse_xml, plain, utf8, Collector, Declared, ImportMeta,
    TextBuilder,
};
use crate::{BibleError, BibleImporter, ImportedBible};
use hollyshow_core::MarkKind;
use roxmltree::Node;

#[derive(Debug, Default, Clone)]
pub struct ZefaniaImporter {
    meta: ImportMeta,
}

impl ZefaniaImporter {
    pub fn new(meta: ImportMeta) -> Self {
        Self { meta }
    }
}

impl BibleImporter for ZefaniaImporter {
    fn format(&self) -> &'static str {
        "zefania"
    }

    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError> {
        let bytes = plain(input)?;
        let doc = parse_xml(utf8(&bytes)?)?;
        let root = doc.root_element();
        if !root.tag_name().name().eq_ignore_ascii_case("xmlbible") {
            return Err(malformed("Zefania: root element is not <XMLBIBLE>"));
        }
        let mut out = Collector::default();
        let mut declared = Declared {
            name: root.attribute("biblename").map(str::to_owned),
            ..Declared::default()
        };
        for n in root.children().filter(Node::is_element) {
            let tag = n.tag_name().name().to_ascii_lowercase();
            match tag.as_str() {
                "information" => read_info(n, &mut declared),
                "biblebook" => read_book(n, &mut out),
                _ => {}
            }
        }
        out.finish("Zefania", declared, &self.meta)
    }
}

fn read_info(info: Node<'_, '_>, d: &mut Declared) {
    for n in info.children().filter(Node::is_element) {
        let text = n
            .text()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        match n.tag_name().name().to_ascii_lowercase().as_str() {
            "title" => d.name = text.or(d.name.take()),
            "identifier" => d.code = text,
            "language" => d.language = text,
            "rights" => d.copyright = text,
            _ => {}
        }
    }
}

fn read_book(book: Node<'_, '_>, out: &mut Collector) {
    let number = book.attribute("bnumber").and_then(parse_u16);
    let resolved = book
        .attribute("bname")
        .and_then(lookup_book_by_name)
        .or_else(|| book.attribute("bsname").and_then(lookup_book_by_name))
        .or_else(|| number.and_then(|n| crate::books::book_by_ord(usize::from(n))));
    let Some(b) = resolved else {
        out.unknown(
            book.attribute("bname")
                .or(book.attribute("bsname"))
                .unwrap_or("?"),
        );
        return;
    };
    for ch in book.children().filter(Node::is_element) {
        if !ch.tag_name().name().eq_ignore_ascii_case("chapter") {
            continue;
        }
        let Some(cn) = ch.attribute("cnumber").and_then(parse_u16) else {
            continue;
        };
        for v in ch.children().filter(Node::is_element) {
            if !v.tag_name().name().eq_ignore_ascii_case("vers") {
                continue;
            }
            let Some(vn) = v.attribute("vnumber").and_then(parse_u16) else {
                continue;
            };
            let mut tb = TextBuilder::default();
            collect(v, &mut tb);
            out.push(b, cn, vn, tb);
        }
    }
}

/// Names (not bare numbers, which `lookup_book` would read as a canonical position).
fn lookup_book_by_name(s: &str) -> Option<&'static crate::books::Book> {
    if s.trim().chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    lookup_book(s)
}

fn collect(node: Node<'_, '_>, tb: &mut TextBuilder) {
    for c in node.children() {
        if c.is_text() {
            tb.push_str(c.text().unwrap_or(""));
        } else if c.is_element() {
            match c.tag_name().name().to_ascii_lowercase().as_str() {
                "note" | "xref" | "remark" | "caption" => {}
                "br" => tb.push_str(" "),
                "style" => {
                    let css = c.attribute("css").unwrap_or("").to_ascii_lowercase();
                    let kind = if css.contains("italic") {
                        Some(MarkKind::Italic)
                    } else if css.contains("red") || css.contains("#ff0000") || css.contains("#c00")
                    {
                        Some(MarkKind::WordsOfJesus)
                    } else {
                        None
                    };
                    if let Some(k) = kind {
                        tb.open(k);
                    }
                    collect(c, tb);
                    if let Some(k) = kind {
                        tb.close(k);
                    }
                }
                _ => collect(c, tb),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<XMLBIBLE biblename="Reina Test" type="x-bible">
 <INFORMATION><title>Reina Test</title><identifier>RT</identifier><language>SPA</language><rights>PD</rights></INFORMATION>
 <BIBLEBOOK bnumber="43" bname="Juan">
  <CHAPTER cnumber="3">
   <CAPTION vref="1">Nicodemo</CAPTION>
   <VERS vnumber="16">Porque de tal manera <STYLE css="font-style:italic">amó</STYLE> Dios<NOTE>n</NOTE>.</VERS>
   <VERS vnumber="17"><STYLE css="color:#ff0000">Porque no envió</STYLE> Dios.</VERS>
  </CHAPTER>
 </BIBLEBOOK>
 <BIBLEBOOK bnumber="1"><CHAPTER cnumber="1"><VERS vnumber="1">En el principio.</VERS></CHAPTER></BIBLEBOOK>
</XMLBIBLE>"#;

    #[test]
    fn imports_metadata_and_verses() {
        let b = ZefaniaImporter::default()
            .import(SAMPLE.as_bytes())
            .unwrap();
        assert_eq!(b.version.code.0, "RT");
        assert_eq!(b.version.language, "es");
        assert_eq!(b.verses.len(), 3);
        assert_eq!(b.verses[0].reference.to_string(), "Gen.1.1");
        assert_eq!(b.verses[1].text, "Porque de tal manera amó Dios.");
        assert_eq!(b.verses[1].marks[0].kind, MarkKind::Italic);
        assert_eq!(b.verses[2].marks[0].kind, MarkKind::WordsOfJesus);
    }

    #[test]
    fn rejects_other_roots_and_unknown_books() {
        assert!(ZefaniaImporter::default().import(b"<osis/>").is_err());
        let bad = r#"<XMLBIBLE><BIBLEBOOK bnumber="500" bname="Zzz"><CHAPTER cnumber="1"><VERS vnumber="1">x</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>"#;
        assert!(ZefaniaImporter::default().import(bad.as_bytes()).is_err());
    }
}
