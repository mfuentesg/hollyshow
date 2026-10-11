//! OpenSong Bible XML: `<bible><b n="Genesis"><c n="1"><v n="1">text</v></c></b></bible>`.

use super::{
    lookup_book, malformed, parse_u16, parse_xml, plain, utf8, Collector, Declared, ImportMeta,
};
use crate::{BibleError, BibleImporter, ImportedBible};
use roxmltree::Node;

#[derive(Debug, Default, Clone)]
pub struct OpenSongImporter {
    meta: ImportMeta,
}

impl OpenSongImporter {
    pub fn new(meta: ImportMeta) -> Self {
        Self { meta }
    }
}

impl BibleImporter for OpenSongImporter {
    fn format(&self) -> &'static str {
        "opensong"
    }

    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError> {
        let bytes = plain(input)?;
        let doc = parse_xml(utf8(&bytes)?)?;
        let root = doc.root_element();
        if !root.tag_name().name().eq_ignore_ascii_case("bible") {
            return Err(malformed("OpenSong: root element is not <bible>"));
        }
        let mut out = Collector::default();
        for b in root.children().filter(Node::is_element) {
            if b.tag_name().name() != "b" {
                continue;
            }
            let name = b.attribute("n").unwrap_or("");
            let Some(book) = lookup_book(name) else {
                out.unknown(name);
                continue;
            };
            for c in b
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "c")
            {
                let Some(cn) = c.attribute("n").and_then(parse_u16) else {
                    continue;
                };
                for v in c
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "v")
                {
                    let Some(vn) = v.attribute("n").and_then(parse_u16) else {
                        continue;
                    };
                    let text: String = v
                        .descendants()
                        .filter(Node::is_text)
                        .filter_map(|t| t.text())
                        .collect::<Vec<_>>()
                        .join(" ");
                    out.push_plain(book, cn, vn, &text);
                }
            }
        }
        let declared = Declared {
            name: root.attribute("n").map(str::to_owned),
            ..Declared::default()
        };
        out.finish("OpenSong", declared, &self.meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_books_by_name_and_skips_empty_verses() {
        let xml = r#"<bible><b n="Genesis"><c n="1"><v n="1">In the beginning</v><v n="2"> </v></c></b>
            <b n="I Samuel"><c n="1"><v n="1">Now there was</v></c></b></bible>"#;
        let b = OpenSongImporter::default().import(xml.as_bytes()).unwrap();
        assert_eq!(b.verses.len(), 2);
        assert_eq!(b.verses[1].reference.to_string(), "1Sam.1.1");
    }

    #[test]
    fn errors() {
        assert!(OpenSongImporter::default().import(b"<song/>").is_err());
        let xml = r#"<bible><b n="Narnia"><c n="1"><v n="1">x</v></c></b></bible>"#;
        assert!(OpenSongImporter::default().import(xml.as_bytes()).is_err());
    }
}
