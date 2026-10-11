//! OpenSong importer (<http://opensong.org>): `<song>` XML whose `<lyrics>` text uses `[V1]`
//! headings, lines starting with `.` for chords and a space for lyrics, `;` comments and `---`
//! manual slide breaks. `<presentation>` (`V1 C V2 C`) becomes the arrangement.

use crate::chords::merge_chord_line;
use crate::common::{
    assemble, clean_input, non_empty, now_ms, parse_bpm, parse_label, Meta, SectionBuilder,
};
use crate::{ImportError, SongImporter};
use hollyshow_core::{SectionId, SectionKind, Song};
use roxmltree::Document;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct OpenSongImporter {
    pub now_ms: Option<i64>,
}

/// One `[tag]` block of the lyrics text.
struct RawSection {
    tag: String,
    lines: Vec<String>,
}

fn parse_lyrics(text: &str) -> Vec<RawSection> {
    let mut out: Vec<RawSection> = Vec::new();
    let mut pending_chords: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim_end();
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') && t.len() > 2 {
            flush_chords(&mut out, &mut pending_chords);
            out.push(RawSection {
                tag: t[1..t.len() - 1].trim().to_owned(),
                lines: vec![],
            });
            continue;
        }
        if out.is_empty() {
            if t.is_empty() || t.starts_with(';') {
                continue;
            }
            out.push(RawSection {
                tag: "V1".into(),
                lines: vec![],
            });
        }
        let Some(cur) = out.last_mut() else { continue };
        if line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('.') {
            if let Some(prev) = pending_chords.take() {
                // Two chord lines in a row: the first is instrumental.
                cur.lines.push(merge_chord_line(&prev, ""));
            }
            pending_chords = Some(rest.to_owned());
            continue;
        }
        if t == "---" {
            if let Some(prev) = pending_chords.take() {
                cur.lines.push(merge_chord_line(&prev, ""));
            }
            cur.lines.push("---".into());
            continue;
        }
        if t.is_empty() {
            continue;
        }
        // Lyric line: the first character is the (space) marker; `_` extends a syllable.
        let body = line.strip_prefix(' ').unwrap_or(line).replace('_', "");
        for piece in body.split("||") {
            let piece = piece.to_owned();
            match pending_chords.take() {
                Some(ch) => cur.lines.push(merge_chord_line(&ch, &piece)),
                None => cur.lines.push(piece.trim_end().to_owned()),
            }
        }
    }
    flush_chords(&mut out, &mut pending_chords);
    out
}

/// A chord line with no lyric line under it is an instrumental line.
fn flush_chords(out: &mut [RawSection], pending: &mut Option<String>) {
    if let (Some(ch), Some(cur)) = (pending.take(), out.last_mut()) {
        cur.lines.push(merge_chord_line(&ch, ""));
    }
}

fn tag_label(tag: &str) -> Option<(SectionKind, Option<u32>)> {
    if let Some(l) = parse_label(tag, true) {
        return Some((l.kind, l.number));
    }
    None
}

impl SongImporter for OpenSongImporter {
    fn format(&self) -> &'static str {
        "opensong"
    }

    fn import(&self, input: &str) -> Result<Vec<Song>, ImportError> {
        let cleaned = clean_input(input);
        let doc = Document::parse(&cleaned)
            .map_err(|e| ImportError::Malformed(format!("OpenSong XML: {e}")))?;
        let root = doc.root_element();
        if root.tag_name().name() != "song" {
            return Err(ImportError::Malformed(
                "OpenSong: root is not <song>".into(),
            ));
        }
        let field = |name: &str| -> Option<String> {
            root.children()
                .find(|c| c.is_element() && c.tag_name().name() == name)
                .map(|n| {
                    n.descendants()
                        .filter(|d| d.is_text())
                        .filter_map(|d| d.text())
                        .collect::<String>()
                })
                .and_then(|s| non_empty(&s))
        };
        let meta = Meta {
            title: field("title"),
            alt_title: None,
            author: field("author"),
            copyright: field("copyright"),
            ccli: field("ccli"),
            key: field("key"),
            bpm: field("tempo").and_then(|t| parse_bpm(&t)),
            tags: field("theme")
                .map(|t| t.split([',', ';']).filter_map(non_empty).collect())
                .unwrap_or_default(),
        };
        let lyrics = root
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "lyrics")
            .map(|n| {
                n.descendants()
                    .filter(|d| d.is_text())
                    .filter_map(|d| d.text())
                    .collect::<String>()
            })
            .ok_or_else(|| ImportError::Malformed("OpenSong: missing <lyrics>".into()))?;

        let mut builder = SectionBuilder::default();
        let mut by_tag: HashMap<String, SectionId> = HashMap::new();
        for raw in parse_lyrics(&lyrics) {
            let body = raw.lines.join("\n");
            if body.trim().is_empty() || by_tag.contains_key(&raw.tag.to_uppercase()) {
                continue;
            }
            let id = match tag_label(&raw.tag) {
                Some((kind, number)) => builder.add(kind, number, body, None, None),
                None => builder.add(SectionKind::Other, None, body, None, Some(raw.tag.clone())),
            };
            by_tag.insert(raw.tag.to_uppercase(), id);
        }
        if builder.is_empty() {
            return Err(ImportError::Malformed("OpenSong: no lyrics".into()));
        }
        if let Some(p) = field("presentation") {
            for tok in p.split_whitespace() {
                if let Some(id) = by_tag.get(&tok.to_uppercase()) {
                    let id = id.clone();
                    builder.play(&id);
                }
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

    const XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<song>
  <title>Amazing Grace</title>
  <author>John Newton</author>
  <copyright>Public Domain</copyright>
  <presentation>V1 C V2 C</presentation>
  <ccli>22025</ccli>
  <key>G</key>
  <tempo>72</tempo>
  <theme>Gracia; Salvación</theme>
  <lyrics>[V1]
;comentario ignorado
.G        D
 Amazing gr_ace||how sweet
.Em
 the sound
---
 That saved a wretch
[C]
 Praise God
[V2]
 'Twas grace
[Interludio]
.G C
</lyrics>
</song>";

    #[test]
    fn parses_tags_chords_and_presentation() {
        let s = &OpenSongImporter { now_ms: Some(1) }.import(XML).unwrap()[0];
        assert_eq!(s.title, "Amazing Grace");
        assert_eq!(s.ccli.as_deref(), Some("22025"));
        assert_eq!(s.bpm, Some(72));
        assert_eq!(s.tags, ["Gracia", "Salvación"]);
        let kinds: Vec<_> = s.sections.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, [Verse, Chorus, Verse, Interlude]);
        assert_eq!(
            s.sections[0].lyrics,
            "[G]Amazing g[D]race\nhow sweet\n[Em]the sound\n---\nThat saved a wretch"
        );
        assert_eq!(s.sections[3].lyrics, "[G]  [C]");
        let seq: Vec<_> = s.arrangements[0]
            .sequence
            .iter()
            .map(|i| i.as_str())
            .collect();
        assert_eq!(seq, ["v1", "c", "v2", "c"]);
    }

    #[test]
    fn rejects_non_song_xml() {
        let imp = OpenSongImporter::default();
        assert!(imp.import("<x/>").is_err());
        assert!(imp.import("<song><title>t</title></song>").is_err());
        assert!(imp.import("nope").is_err());
    }
}
