//! Plain-text importer.
//!
//! Auto-detects:
//! * headings (`Verso 1`, `[Coro]`, `Chorus`, `Pre-coro`, `Puente`, `V1`...), also inline repeats
//!   such as a bare `Coro` line that replays the earlier chorus (`Coro x2` plays it twice);
//! * a title line and `Autor:`/`Tono:`/`Copyright:`/`CCLI:`/`BPM:` header lines;
//! * unlabeled blocks, where a block that appears more than once is the chorus (the first
//!   repeated block becomes `Coro`, the second `Puente`) and is stored once, with the play order
//!   kept in an arrangement;
//! * chords written above the lyrics, converted to inline ChordPro.

use crate::chords::{is_chord_line, merge_chord_line};
use crate::common::{
    assemble, clean_input, non_empty, now_ms, parse_bpm, parse_label, Meta, ParsedLabel,
    SectionBuilder,
};
use crate::{ImportError, SongImporter};
use hollyshow_core::text::fold;
use hollyshow_core::{SectionId, SectionKind, Song};
use std::collections::HashMap;

/// Importer for pasted or `.txt` lyrics.
#[derive(Debug, Default, Clone)]
pub struct TextImporter {
    /// Title to use instead of detecting one from the first line.
    pub title: Option<String>,
    /// Timestamp for `created_at`/`updated_at` (defaults to the current time).
    pub now_ms: Option<i64>,
}

struct Block {
    label: Option<ParsedLabel>,
    lines: Vec<String>,
}

/// Joins "chords over lyrics" pairs into inline ChordPro lines.
fn merge_chord_lines(lines: &[&str]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let next = lines.get(i + 1).copied();
        match next {
            Some(n)
                if is_chord_line(line)
                    && !n.trim().is_empty()
                    && !is_chord_line(n)
                    && parse_label(n, false).is_none() =>
            {
                out.push(merge_chord_line(line, n));
                i += 2;
            }
            _ => {
                out.push(line.to_owned());
                i += 1;
            }
        }
    }
    out
}

fn split_blocks(lines: &[String]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut cur: Option<Block> = None;
    for raw in lines {
        let t = raw.trim();
        if t.is_empty() {
            blocks.extend(cur.take());
            continue;
        }
        if let Some(l) = parse_label(t, false) {
            blocks.extend(cur.take());
            cur = Some(Block {
                label: Some(l),
                lines: vec![],
            });
            continue;
        }
        cur.get_or_insert_with(|| Block {
            label: None,
            lines: vec![],
        })
        .lines
        .push(t.to_owned());
    }
    blocks.extend(cur.take());
    blocks
}

/// `Autor: X` style header line: returns the folded key and value.
fn header_line(line: &str) -> Option<(String, String)> {
    let (k, v) = line.split_once(':').or_else(|| line.split_once('='))?;
    let key = fold(k.trim());
    let known = [
        "autor",
        "author",
        "tono",
        "key",
        "tonalidad",
        "copyright",
        "ccli",
        "bpm",
        "tempo",
        "titulo",
        "title",
        "etiquetas",
        "tags",
    ];
    (known.contains(&key.as_str()) && !v.trim().is_empty()).then(|| (key, v.trim().to_owned()))
}

fn apply_header(meta: &mut Meta, key: &str, value: &str) {
    match key {
        "autor" | "author" => meta.author = non_empty(value),
        "tono" | "key" | "tonalidad" => meta.key = non_empty(value),
        "copyright" => meta.copyright = non_empty(value),
        "ccli" => meta.ccli = non_empty(value),
        "bpm" | "tempo" => meta.bpm = parse_bpm(value),
        "titulo" | "title" => meta.title = non_empty(value),
        "etiquetas" | "tags" => {
            meta.tags = value.split(',').filter_map(non_empty).collect();
        }
        _ => {}
    }
}

fn content_key(lines: &[String]) -> String {
    fold(&crate::strip_chords(&lines.join(" ")))
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

impl SongImporter for TextImporter {
    fn format(&self) -> &'static str {
        "text"
    }

    fn import(&self, input: &str) -> Result<Vec<Song>, ImportError> {
        let cleaned = clean_input(input);
        let raw_lines: Vec<&str> = cleaned.lines().collect();
        let lines = merge_chord_lines(&raw_lines);
        let mut blocks = split_blocks(&lines);
        if blocks.is_empty() {
            return Err(ImportError::Malformed("empty text".into()));
        }

        let mut meta = Meta::default();
        // Header: a lone title line, or title + `Clave: valor` lines, before the first section.
        if blocks[0].label.is_none() {
            let first = &blocks[0].lines;
            let metas: Vec<_> = first.iter().map(|l| header_line(l)).collect();
            let all_meta = metas.iter().all(Option::is_some);
            let title_then_meta = metas[0].is_none() && metas[1..].iter().all(Option::is_some);
            let lone_title = first.len() == 1 && blocks.len() > 1;
            if all_meta || title_then_meta || lone_title {
                let header = blocks.remove(0);
                for (i, l) in header.lines.iter().enumerate() {
                    match &metas[i] {
                        Some((k, v)) => apply_header(&mut meta, k, v),
                        None => meta.title = Some(l.clone()),
                    }
                }
            }
        }
        if let Some(t) = self.title.as_deref().and_then(non_empty) {
            meta.title = Some(t);
        }
        if blocks.is_empty() {
            return Err(ImportError::Malformed("no lyrics found".into()));
        }

        // Unlabeled blocks that occur more than once are choruses.
        let mut counts: HashMap<String, u32> = HashMap::new();
        for b in blocks.iter().filter(|b| b.label.is_none()) {
            *counts.entry(content_key(&b.lines)).or_default() += 1;
        }

        let mut builder = SectionBuilder::default();
        let mut by_content: HashMap<String, SectionId> = HashMap::new();
        let mut repeated_assigned = 0u32;
        for blk in &blocks {
            match blk.label {
                Some(l) if blk.lines.is_empty() => {
                    if let Some(id) = builder.find(l.kind, l.number) {
                        for _ in 0..l.repeat {
                            builder.play(&id);
                        }
                    }
                }
                Some(l) => {
                    let key = content_key(&blk.lines);
                    let existing = by_content
                        .get(&key)
                        .filter(|id| builder.section(id).is_some_and(|s| s.kind == l.kind));
                    let id = match existing {
                        Some(id) => id.clone(),
                        None => {
                            let id =
                                builder.add(l.kind, l.number, blk.lines.join("\n"), None, None);
                            by_content.insert(key, id.clone());
                            id
                        }
                    };
                    for _ in 0..l.repeat {
                        builder.play(&id);
                    }
                }
                None => {
                    let key = content_key(&blk.lines);
                    if let Some(id) = by_content.get(&key) {
                        let id = id.clone();
                        builder.play(&id);
                        continue;
                    }
                    let kind = if counts.get(&key).copied().unwrap_or(0) > 1 {
                        repeated_assigned += 1;
                        match repeated_assigned {
                            1 => SectionKind::Chorus,
                            2 => SectionKind::Bridge,
                            _ => SectionKind::Other,
                        }
                    } else {
                        SectionKind::Verse
                    };
                    let id = builder.add(kind, None, blk.lines.join("\n"), None, None);
                    by_content.insert(key, id.clone());
                    builder.play(&id);
                }
            }
        }
        if builder.is_empty() {
            return Err(ImportError::Malformed("no lyrics found".into()));
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
    use hollyshow_core::SectionKind::*;

    fn import(text: &str) -> Song {
        TextImporter {
            now_ms: Some(1),
            ..Default::default()
        }
        .import(text)
        .unwrap()
        .remove(0)
    }

    const AMAZING: &str = "Amazing Grace
Autor: John Newton
Tono: G

Verso 1
Amazing grace, how sweet the sound,
That saved a wretch like me.

Chorus
Praise God, praise God,
Praise God, praise God.

Verso 2
'Twas grace that taught my heart to fear,
And grace my fears relieved.

Chorus
";

    #[test]
    fn labelled_text_with_header_and_chorus_repeat() {
        let s = import(AMAZING);
        assert_eq!(s.title, "Amazing Grace");
        assert_eq!(s.author.as_deref(), Some("John Newton"));
        assert_eq!(s.key.as_deref(), Some("G"));
        let kinds: Vec<_> = s.sections.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, [Verse, Chorus, Verse]);
        assert_eq!(s.sections[0].label, "Verso 1");
        assert_eq!(s.sections[1].label, "Coro");
        let seq: Vec<_> = s.arrangements[0]
            .sequence
            .iter()
            .map(|i| i.as_str())
            .collect();
        assert_eq!(seq, ["v1", "c", "v2", "c"]);
    }

    #[test]
    fn unlabelled_repeated_block_becomes_the_chorus() {
        let s = import(
            "Himno de prueba

Cantemos al Señor
con todo el corazón

Gloria, gloria, gloria
al Rey de reyes

Alabemos su nombre
por siempre y siempre

Gloria, gloria, gloria
al rey de reyes!

Final de la canción
en paz y alegría

Gloria, gloria, gloria
al Rey de reyes
",
        );
        assert_eq!(s.sections.len(), 4, "chorus stored once");
        let c = s.sections.iter().find(|x| x.kind == Chorus).unwrap();
        assert_eq!(c.label, "Coro");
        assert!(c.lyrics.starts_with("Gloria"));
        assert_eq!(s.sections.iter().filter(|x| x.kind == Verse).count(), 3);
        // v1 c v2 c v3 c
        assert_eq!(s.arrangements[0].sequence.len(), 6);
    }

    #[test]
    fn no_arrangement_when_written_order_is_play_order() {
        let s = import("Titulo\n\nUno\ndos\n\nTres\ncuatro\n");
        assert_eq!(s.sections.len(), 2);
        assert!(s.arrangements.is_empty());
    }

    #[test]
    fn single_block_has_no_title_line_when_alone() {
        let s = TextImporter {
            title: Some("Mi canción".into()),
            now_ms: Some(0),
        }
        .import("Línea uno\nLínea dos")
        .unwrap()
        .remove(0);
        assert_eq!(s.title, "Mi canción");
        assert_eq!(s.sections[0].lyrics, "Línea uno\nLínea dos");
    }

    #[test]
    fn english_labels_and_x2() {
        let s =
            import("[Verse 1]\nline a\nline b\n[Chorus]\nsing out\n[Chorus] x2\n[Bridge]\nwait\n");
        let seq: Vec<_> = s.arrangements[0]
            .sequence
            .iter()
            .map(|i| i.as_str())
            .collect();
        assert_eq!(seq, ["v1", "c", "c", "c", "b"]);
        assert_eq!(s.sections[2].label, "Puente");
    }

    #[test]
    fn chords_above_lyrics_become_inline() {
        let s = import(
            "Gracia

Verso 1
G        D
Amazing grace
Em       C
how sweet the sound
",
        );
        assert_eq!(
            s.sections[0].lyrics,
            "[G]Amazing g[D]race\n[Em]how sweet[C] the sound"
        );
    }

    #[test]
    fn handles_crlf_bom_and_empty() {
        let s = import("\u{feff}T\r\n\r\nUno\r\ndos\r\n");
        assert_eq!(s.title, "T");
        assert_eq!(s.sections[0].lyrics, "Uno\ndos");
        assert!(TextImporter::default().import("  \n\n ").is_err());
    }

    #[test]
    fn same_text_gives_same_id() {
        assert_eq!(import(AMAZING).id, import(AMAZING).id);
    }
}
