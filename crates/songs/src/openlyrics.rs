//! OpenLyrics (<http://openlyrics.info>) importer, versions 0.8 and 0.9.
//!
//! Chords (`<chord name="G"/>` or 0.9 `<chord root="G" structure="m7" bass="B"/>`) become inline
//! ChordPro; `<br/>` becomes a line break; formatting `<tag>` wrappers keep their text; verses
//! sharing a `name` in another `lang` become the section's translation. `<verseOrder>` becomes the
//! arrangement.

use crate::common::{
    assemble, clean_input, non_empty, now_ms, parse_bpm, parse_label, Meta, SectionBuilder,
};
use crate::{ImportError, SongImporter};
use hollyshow_core::{SectionId, SectionKind, Song};
use roxmltree::{Document, Node};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct OpenLyricsImporter {
    pub now_ms: Option<i64>,
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a, 'i>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn text_of(n: Node) -> String {
    n.descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_owned()
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

fn chord_text(n: Node) -> String {
    if let Some(name) = n.attribute("name") {
        return format!("[{name}]");
    }
    let root = n.attribute("root").unwrap_or("");
    let structure = n.attribute("structure").unwrap_or("");
    let bass = n.attribute("bass").filter(|b| !b.is_empty());
    match bass {
        Some(b) => format!("[{root}{structure}/{b}]"),
        None => format!("[{root}{structure}]"),
    }
}

fn walk_inline(n: Node, out: &mut String) {
    for c in n.children() {
        if c.is_text() {
            out.push_str(&collapse_ws(c.text().unwrap_or("")));
        } else if c.is_element() {
            match c.tag_name().name() {
                "br" => out.push('\n'),
                "chord" => out.push_str(&chord_text(c)),
                "comment" => {}
                _ => walk_inline(c, out),
            }
        }
    }
}

fn verse_text(verse: Node) -> String {
    let mut parts = Vec::new();
    for lines in children(verse, "lines") {
        let mut s = String::new();
        walk_inline(lines, &mut s);
        parts.push(s);
    }
    parts
        .join("\n")
        .split('\n')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_owned()
}

impl SongImporter for OpenLyricsImporter {
    fn format(&self) -> &'static str {
        "openlyrics"
    }

    fn import(&self, input: &str) -> Result<Vec<Song>, ImportError> {
        let cleaned = clean_input(input);
        let doc = Document::parse(&cleaned)
            .map_err(|e| ImportError::Malformed(format!("OpenLyrics XML: {e}")))?;
        let root = doc.root_element();
        if root.tag_name().name() != "song" {
            return Err(ImportError::Malformed(
                "OpenLyrics: root is not <song>".into(),
            ));
        }
        let props = child(root, "properties");
        let mut meta = Meta::default();
        if let Some(p) = props {
            if let Some(titles) = child(p, "titles") {
                let mut it = children(titles, "title")
                    .map(text_of)
                    .filter(|t| !t.is_empty());
                meta.title = it.next();
                meta.alt_title = it.next();
            }
            if let Some(a) = child(p, "authors") {
                let names: Vec<String> = children(a, "author")
                    .map(text_of)
                    .filter(|t| !t.is_empty())
                    .collect();
                meta.author = non_empty(&names.join(", "));
            }
            meta.copyright = child(p, "copyright").and_then(|n| non_empty(&text_of(n)));
            meta.ccli = child(p, "ccliNo").and_then(|n| non_empty(&text_of(n)));
            meta.key = child(p, "key").and_then(|n| non_empty(&text_of(n)));
            if let Some(t) = child(p, "tempo") {
                if t.attribute("type").is_none_or(|ty| ty == "bpm") {
                    meta.bpm = parse_bpm(&text_of(t));
                }
            }
            if let Some(th) = child(p, "themes") {
                meta.tags = children(th, "theme")
                    .map(text_of)
                    .filter(|t| !t.is_empty())
                    .collect();
            }
        }

        let lyrics = child(root, "lyrics")
            .ok_or_else(|| ImportError::Malformed("OpenLyrics: missing <lyrics>".into()))?;
        let mut builder = SectionBuilder::default();
        let mut by_name: HashMap<String, SectionId> = HashMap::new();
        let mut written: Vec<SectionId> = Vec::new();
        for verse in children(lyrics, "verse") {
            let name = verse.attribute("name").unwrap_or("v1").trim().to_owned();
            let text = verse_text(verse);
            if let Some(id) = by_name.get(&name) {
                // Same verse in another language: attach as translation.
                if let Some(sec) = builder.sections.iter_mut().find(|s| &s.id == id) {
                    if sec.translation.is_none() && !text.is_empty() {
                        sec.translation = Some(text);
                    }
                }
                continue;
            }
            if text.is_empty() {
                continue;
            }
            let id = match parse_label(&name, true) {
                Some(l) => builder.add(l.kind, l.number, text, None, None),
                None => builder.add(SectionKind::Other, None, text, None, Some(name.clone())),
            };
            by_name.insert(name, id.clone());
            written.push(id);
        }
        if builder.is_empty() {
            return Err(ImportError::Malformed("OpenLyrics: no verses".into()));
        }
        let order = props
            .and_then(|p| child(p, "verseOrder"))
            .map(text_of)
            .unwrap_or_default();
        for tok in order.split_whitespace() {
            if let Some(id) = by_name.get(tok) {
                let id = id.clone();
                builder.play(&id);
            }
        }
        Ok(vec![assemble(
            meta,
            builder,
            self.now_ms.unwrap_or_else(now_ms),
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SectionKind::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<song xmlns="http://openlyrics.info/namespace/2009/song" version="0.8" createdIn="test">
  <properties>
    <titles><title>Amazing Grace</title><title>Gracia asombrosa</title></titles>
    <authors><author>John Newton</author><author>Anónimo</author></authors>
    <copyright>Public Domain</copyright>
    <ccliNo>22025</ccliNo>
    <key>G</key>
    <tempo type="bpm">72</tempo>
    <verseOrder>v1 c v2 c</verseOrder>
    <themes><theme>Gracia</theme></themes>
  </properties>
  <lyrics>
    <verse name="v1">
      <lines>
        <chord name="G"/>Amazing grace, how sweet the sound<br/>
        That saved a <tag name="i">wretch</tag> like me
      </lines>
    </verse>
    <verse name="c">
      <lines>Praise God<br/>Praise <chord name="D"/>God</lines>
    </verse>
    <verse name="c" lang="es">
      <lines>Alabado sea Dios</lines>
    </verse>
    <verse name="v2"><lines part="1">'Twas grace</lines><lines part="2">that taught my heart</lines></verse>
  </lyrics>
</song>"#;

    #[test]
    fn parses_properties_verses_chords_and_order() {
        let s = &OpenLyricsImporter { now_ms: Some(1) }.import(XML).unwrap()[0];
        assert_eq!(s.title, "Amazing Grace");
        assert_eq!(s.alt_title.as_deref(), Some("Gracia asombrosa"));
        assert_eq!(s.author.as_deref(), Some("John Newton, Anónimo"));
        assert_eq!(s.ccli.as_deref(), Some("22025"));
        assert_eq!(s.key.as_deref(), Some("G"));
        assert_eq!(s.bpm, Some(72));
        assert_eq!(s.tags, ["Gracia"]);
        let kinds: Vec<_> = s.sections.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, [Verse, Chorus, Verse]);
        assert_eq!(
            s.sections[0].lyrics,
            "[G]Amazing grace, how sweet the sound\nThat saved a wretch like me"
        );
        assert_eq!(s.sections[1].lyrics, "Praise God\nPraise [D]God");
        assert_eq!(
            s.sections[1].translation.as_deref(),
            Some("Alabado sea Dios")
        );
        assert_eq!(s.sections[2].lyrics, "'Twas grace\nthat taught my heart");
        let seq: Vec<_> = s.arrangements[0]
            .sequence
            .iter()
            .map(|i| i.as_str())
            .collect();
        assert_eq!(seq, ["v1", "c", "v2", "c"]);
    }

    #[test]
    fn v09_chord_attributes_and_unknown_verse_names() {
        let xml = r#"<song><properties><titles><title>T</title></titles></properties>
<lyrics><verse name="intro-riff"><lines><chord root="B" structure="m7" bass="F#"/>la</lines></verse></lyrics></song>"#;
        let s = &OpenLyricsImporter::default().import(xml).unwrap()[0];
        assert_eq!(s.sections[0].lyrics, "[Bm7/F#]la");
        assert_eq!(s.sections[0].label, "intro-riff");
        assert_eq!(s.sections[0].kind, Other);
    }

    #[test]
    fn rejects_malformed_and_dtd_documents() {
        let imp = OpenLyricsImporter::default();
        assert!(imp.import("<song><lyrics>").is_err());
        assert!(imp.import("<other/>").is_err());
        assert!(imp.import("<song><properties/></song>").is_err());
        let bomb = r#"<!DOCTYPE song [<!ENTITY a "aaaa">]><song><lyrics><verse name="v1"><lines>&a;</lines></verse></lyrics></song>"#;
        assert!(imp.import(bomb).is_err());
    }
}
