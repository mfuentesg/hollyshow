//! OSIS XML (CrossWire, eBible.org, SWORD exports).
//!
//! Both encodings of verses are understood: containers
//! (`<verse osisID="Gen.1.1">...</verse>`) and milestones
//! (`<verse sID="Gen.1.1" osisID="Gen.1.1"/>...<verse eID="Gen.1.1"/>`).

use super::{
    lookup_book, malformed, parse_u16, parse_xml, plain, utf8, Collector, Declared, ImportMeta,
    TextBuilder,
};
use crate::books::Book;
use crate::{BibleError, BibleImporter, ImportedBible};
use hollyshow_core::MarkKind;
use roxmltree::Node;

#[derive(Debug, Default, Clone)]
pub struct OsisImporter {
    meta: ImportMeta,
}

impl OsisImporter {
    pub fn new(meta: ImportMeta) -> Self {
        Self { meta }
    }
}

impl BibleImporter for OsisImporter {
    fn format(&self) -> &'static str {
        "osis"
    }

    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError> {
        let bytes = plain(input)?;
        let doc = parse_xml(utf8(&bytes)?)?;
        let root = doc.root_element();
        if root.tag_name().name() != "osis" {
            return Err(malformed("OSIS: root element is not <osis>"));
        }
        let mut w = Walker::default();
        w.walk(root);
        w.close();
        w.finish(declared(&doc), &self.meta)
    }
}

fn declared(doc: &roxmltree::Document<'_>) -> Declared {
    let mut d = Declared::default();
    let text_of = |name: &str| {
        doc.descendants()
            .find(|n| n.is_element() && n.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    };
    let root = doc.root_element();
    let osis_text = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "osisText");
    d.code = osis_text
        .and_then(|n| n.attribute("osisIDWork").map(str::to_owned))
        .or_else(|| {
            doc.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "work")
                .and_then(|n| n.attribute("osisWork").map(str::to_owned))
        });
    d.language = osis_text
        .and_then(|n| {
            n.attributes()
                .find(|a| a.name() == "lang")
                .map(|a| a.value().to_owned())
        })
        .or_else(|| text_of("language"));
    d.name = text_of("title");
    d.copyright = text_of("rights");
    d
}

/// The verse being filled, possibly addressed by several ids ("Gen.1.1 Gen.1.2").
struct Open {
    book: &'static Book,
    chapter: u16,
    verse: u16,
    text: TextBuilder,
}

#[derive(Default)]
struct Walker {
    out: Collector,
    cur: Option<Open>,
    unknown_seen: bool,
}

impl Walker {
    fn finish(self, d: Declared, m: &ImportMeta) -> Result<ImportedBible, BibleError> {
        self.out.finish("OSIS", d, m)
    }

    fn close(&mut self) {
        if let Some(o) = self.cur.take() {
            self.out.push(o.book, o.chapter, o.verse, o.text);
        }
    }

    fn open_from_id(&mut self, osis_id: &str) {
        self.close();
        match parse_osis_id(osis_id) {
            Some((Some(book), ch, v)) => {
                self.cur = Some(Open {
                    book,
                    chapter: ch,
                    verse: v,
                    text: TextBuilder::default(),
                });
            }
            Some((None, ..)) => {
                self.out
                    .unknown(osis_id.split('.').next().unwrap_or(osis_id));
                self.unknown_seen = true;
            }
            None => {}
        }
    }

    fn walk(&mut self, node: Node<'_, '_>) {
        for child in node.children() {
            if child.is_text() {
                if let (Some(o), Some(t)) = (self.cur.as_mut(), child.text()) {
                    o.text.push_str(t);
                }
                continue;
            }
            if !child.is_element() {
                continue;
            }
            let name = child.tag_name().name();
            match name {
                "verse" => self.verse(child),
                // Not scripture text.
                "note" | "title" | "rdg" | "catchWord" | "milestone" | "index" | "header" => {}
                "lb" | "p" | "l" | "br" => {
                    if let Some(o) = self.cur.as_mut() {
                        o.text.push_str(" ");
                    }
                    self.walk(child);
                }
                "transChange" => self.marked(child, MarkKind::Italic),
                "q" => {
                    let jesus = child
                        .attribute("who")
                        .is_some_and(|w| w.eq_ignore_ascii_case("jesus"));
                    if jesus {
                        self.marked(child, MarkKind::WordsOfJesus);
                    } else {
                        self.walk(child);
                    }
                }
                "seg" => {
                    // SWORD marks translator additions as <seg type="x-added">.
                    let added = child.attribute("type").is_some_and(|t| t.contains("added"));
                    if added {
                        self.marked(child, MarkKind::Italic);
                    } else {
                        self.walk(child);
                    }
                }
                "chapter" => {
                    // A container chapter closes any milestone verse left open.
                    if child.attribute("eID").is_some() || child.attribute("sID").is_some() {
                        self.close();
                    }
                    self.walk(child);
                }
                _ => self.walk(child),
            }
        }
    }

    fn marked(&mut self, node: Node<'_, '_>, kind: MarkKind) {
        if let Some(o) = self.cur.as_mut() {
            o.text.open(kind);
        }
        self.walk(node);
        if let Some(o) = self.cur.as_mut() {
            o.text.close(kind);
        }
    }

    fn verse(&mut self, node: Node<'_, '_>) {
        if node.attribute("eID").is_some() {
            self.close();
            return;
        }
        let id = node.attribute("osisID").or_else(|| node.attribute("sID"));
        let milestone = node.attribute("sID").is_some();
        if let Some(id) = id {
            self.open_from_id(id);
        }
        if !milestone {
            self.walk(node);
            self.close();
        }
    }
}

/// `Gen.1.1`, `Bible:Gen.1.1`, `Gen.1.1-Gen.1.3`, `Gen.1.1 Gen.1.2`, `Gen.1.1!b`: the first
/// verse. Returns `(None, ..)` for an unknown book so the caller can report it.
fn parse_osis_id(id: &str) -> Option<(Option<&'static Book>, u16, u16)> {
    let first = id.split_whitespace().next()?;
    let first = first.split('-').next()?;
    let first = match first.split_once(':') {
        Some((_, rest)) => rest,
        None => first,
    };
    let mut parts = first.split('.');
    let book = parts.next()?;
    let chapter = parse_u16(parts.next()?)?;
    let verse = parse_u16(parts.next()?)?;
    Some((lookup_book(book), chapter, verse))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
 <osisText osisIDWork="TST" xml:lang="en" osisRefWork="Bible">
  <header><work osisWork="TST"><title>Test Bible</title><rights>Public Domain</rights></work></header>
  <div type="book" osisID="John">
   <chapter osisID="John.3">
    <title>Heading</title>
    <verse osisID="John.3.16">For God so <transChange type="added">loved</transChange> the world<note>footnote</note>,
      that he gave.</verse>
    <verse osisID="John.3.17"><q who="Jesus">Not to condemn</q> it.</verse>
   </chapter>
  </div>
  <div type="book" osisID="Gen"><chapter osisID="Gen.1"><verse osisID="Gen.1.1">In the beginning.</verse></chapter></div>
 </osisText>
</osis>"#;

    const MILESTONE: &str = r#"<osis><osisText osisIDWork="MS" xml:lang="es"><div type="book" osisID="Ps">
<chapter osisID="Ps.23" sID="Ps.23"/>
<l><verse osisID="Ps.23.1" sID="Ps.23.1"/>Jehová es mi pastor;<note>x</note> <w lemma="x">nada</w> me faltará.<verse eID="Ps.23.1"/></l>
<l><verse osisID="Ps.23.2 Ps.23.3" sID="Ps.23.2"/>En lugares de delicados pastos<verse eID="Ps.23.2"/></l>
<chapter eID="Ps.23"/>
</div></osisText></osis>"#;

    #[test]
    fn imports_container_verses_in_canonical_order() {
        let b = OsisImporter::default()
            .import(CONTAINER.as_bytes())
            .unwrap();
        assert_eq!(b.version.code.0, "TST");
        assert_eq!(b.version.name, "Test Bible");
        assert_eq!(b.version.language, "en");
        assert_eq!(b.version.copyright_text, "Public Domain");
        assert_eq!(b.verses.len(), 3);
        assert_eq!(b.verses[0].reference.to_string(), "Gen.1.1");
        let v = &b.verses[1];
        assert_eq!(v.text, "For God so loved the world, that he gave.");
        assert_eq!(v.marks.len(), 1);
        assert_eq!(v.marks[0].kind, MarkKind::Italic);
        let chars: Vec<char> = v.text.chars().collect();
        let s: String = chars[v.marks[0].start as usize..v.marks[0].end as usize]
            .iter()
            .collect();
        assert_eq!(s, "loved");
        assert_eq!(b.verses[2].marks[0].kind, MarkKind::WordsOfJesus);
    }

    #[test]
    fn imports_milestone_verses() {
        let b = OsisImporter::default()
            .import(MILESTONE.as_bytes())
            .unwrap();
        assert_eq!(b.version.language, "es");
        assert_eq!(b.verses.len(), 2);
        assert_eq!(b.verses[0].text, "Jehová es mi pastor; nada me faltará.");
        assert_eq!(b.verses[1].reference.to_string(), "Ps.23.2");
    }

    #[test]
    fn overrides_win_and_garbage_fails_cleanly() {
        let meta = ImportMeta::default().with_code("x-1").with_language("spa");
        let b = OsisImporter::new(meta)
            .import(MILESTONE.as_bytes())
            .unwrap();
        assert_eq!(b.version.code.0, "X1");
        assert_eq!(b.version.language, "es");
        for bad in [
            "",
            "not xml",
            "<a/>",
            "<osis></osis>",
            "<osis><verse osisID=\"Foo.1.1\">x</verse></osis>",
        ] {
            assert!(
                OsisImporter::default().import(bad.as_bytes()).is_err(),
                "{bad}"
            );
        }
        assert!(OsisImporter::default().import(&[0xff, 0xfe, 0x00]).is_err());
    }

    #[test]
    fn osis_ids_parse() {
        assert_eq!(parse_osis_id("Gen.1.1").unwrap().1, 1);
        assert_eq!(parse_osis_id("Bible:Gen.1.2").unwrap().2, 2);
        assert_eq!(parse_osis_id("Gen.1.3-Gen.1.4").unwrap().2, 3);
        assert_eq!(parse_osis_id("Gen.1.5!b").unwrap().2, 5);
        assert!(parse_osis_id("Gen.1").is_none());
    }
}
