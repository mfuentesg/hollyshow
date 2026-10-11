//! ChordPro importer/exporter. Inline chords (`[G]Sublime [D]gracia`) are kept verbatim in the
//! section lyrics, which is the representation the stage display and transposition use.
//!
//! Supported directives: `title|t`, `subtitle|st`, `artist|composer|lyricist|author`, `copyright`,
//! `ccli`, `key`, `tempo`, `keywords|tag`, `meta`, `start_of_verse|sov`, `start_of_chorus|soc`,
//! `start_of_bridge|sob`, `start_of_<anything>`, `end_of_*|eov|eoc|eob`, `comment|c|ci|cb`
//! (a recognised heading names the next block), `chorus` (replays the last chorus) and
//! `new_song|ns` (several songs per file). Unknown directives and `#` comments are ignored.

use crate::common::{
    assemble, clean_input, non_empty, now_ms, parse_bpm, parse_label, Meta, ParsedLabel,
    SectionBuilder,
};
use crate::{ImportError, SongImporter};
use hollyshow_core::{SectionId, SectionKind, Song};

#[derive(Debug, Default, Clone)]
pub struct ChordProImporter {
    /// Timestamp for `created_at`/`updated_at` (defaults to the current time).
    pub now_ms: Option<i64>,
}

struct Open {
    kind: SectionKind,
    number: Option<u32>,
    label: Option<String>,
    lines: Vec<String>,
    /// Closed by `end_of_*` (blank lines stay inside) rather than by a blank line.
    explicit: bool,
}

#[derive(Default)]
struct Parser {
    meta: Meta,
    builder: SectionBuilder,
    open: Option<Open>,
    pending: Option<ParsedLabel>,
    last_chorus: Option<SectionId>,
    saw_content: bool,
}

impl Parser {
    fn close(&mut self) {
        let Some(o) = self.open.take() else { return };
        let lyrics = o.lines.join("\n");
        if lyrics.trim().is_empty() {
            return;
        }
        let id = self.builder.add(o.kind, o.number, lyrics, None, o.label);
        if o.kind == SectionKind::Chorus {
            self.last_chorus = Some(id.clone());
        }
        self.builder.play(&id);
    }

    fn begin(
        &mut self,
        kind: SectionKind,
        number: Option<u32>,
        label: Option<String>,
        explicit: bool,
    ) {
        self.close();
        self.open = Some(Open {
            kind,
            number,
            label,
            lines: vec![],
            explicit,
        });
    }

    fn lyric_line(&mut self, line: &str) {
        self.saw_content = true;
        if self.open.is_none() {
            let (kind, number) = match self.pending.take() {
                Some(l) => (l.kind, l.number),
                None => (SectionKind::Verse, None),
            };
            self.begin(kind, number, None, false);
        }
        if let Some(o) = &mut self.open {
            o.lines.push(line.to_owned());
        }
    }

    fn finish(mut self, now: i64) -> Option<Song> {
        self.close();
        if self.builder.is_empty() {
            return None;
        }
        Some(assemble(self.meta, self.builder, now))
    }

    fn directive(&mut self, name: &str, value: &str) {
        let n = name.to_ascii_lowercase();
        let v = value.trim();
        match n.as_str() {
            "title" | "t" => self.meta.title = non_empty(v),
            "subtitle" | "st" | "sorttitle" => {
                if self.meta.alt_title.is_none() {
                    self.meta.alt_title = non_empty(v);
                }
            }
            "artist" | "composer" | "lyricist" | "author" | "words" | "music" => {
                if let Some(x) = non_empty(v) {
                    self.meta.author = Some(match self.meta.author.take() {
                        Some(prev) if !prev.contains(&x) => format!("{prev}, {x}"),
                        Some(prev) => prev,
                        None => x,
                    });
                }
            }
            "copyright" => self.meta.copyright = non_empty(v),
            "ccli" | "ccli_number" => self.meta.ccli = non_empty(v),
            "key" => self.meta.key = non_empty(v),
            "tempo" | "bpm" => self.meta.bpm = parse_bpm(v),
            "keywords" | "tag" | "tags" => {
                self.meta.tags.extend(v.split(',').filter_map(non_empty));
            }
            "meta" => {
                if let Some((k, rest)) = v.split_once(char::is_whitespace) {
                    self.directive(k, rest);
                }
            }
            "comment" | "c" | "comment_italic" | "ci" | "comment_box" | "cb" => {
                // Only headings matter ("Verso 1", "Coro"); a blank line is not required before it.
                if let Some(l) = parse_label(v, false) {
                    self.close();
                    self.pending = Some(l);
                }
            }
            "chorus" => {
                self.close();
                if let Some(id) = self.last_chorus.clone() {
                    self.builder.play(&id);
                }
            }
            "new_song" | "ns" => {}
            "eov" | "eoc" | "eob" | "eot" | "eog" => self.close(),
            _ => {
                if n.starts_with("end_of_") {
                    self.close();
                } else if let Some(rest) = n.strip_prefix("start_of_") {
                    self.start_env(rest, v);
                } else {
                    match n.as_str() {
                        "sov" => self.start_env("verse", v),
                        "soc" => self.start_env("chorus", v),
                        "sob" => self.start_env("bridge", v),
                        _ => {}
                    }
                }
            }
        }
    }

    fn start_env(&mut self, env: &str, value: &str) {
        let from_env = parse_label(env, true);
        let from_value = parse_label(value, false);
        let kind = from_env
            .or(from_value)
            .map_or(SectionKind::Other, |l| l.kind);
        // A label like "Verso 2" supplies the number; free text becomes the display label.
        let number = from_value.filter(|l| l.kind == kind).and_then(|l| l.number);
        let label = if from_value.is_none() {
            non_empty(value)
        } else {
            None
        };
        self.begin(kind, number, label, true);
    }
}

impl SongImporter for ChordProImporter {
    fn format(&self) -> &'static str {
        "chordpro"
    }

    fn import(&self, input: &str) -> Result<Vec<Song>, ImportError> {
        let now = self.now_ms.unwrap_or_else(now_ms);
        let cleaned = clean_input(input);
        let mut songs = Vec::new();
        let mut p = Parser::default();
        let mut skipping = false;
        for raw in cleaned.lines() {
            let line = raw.trim_end();
            let t = line.trim();
            if t.starts_with('#') {
                continue;
            }
            if t.starts_with('{') && t.ends_with('}') {
                let inner = t[1..t.len() - 1].trim();
                let (name, value) = inner
                    .split_once(':')
                    .or_else(|| inner.split_once(char::is_whitespace))
                    .unwrap_or((inner, ""));
                let lname = name.trim().to_ascii_lowercase();
                if matches!(lname.as_str(), "new_song" | "ns") {
                    if let Some(s) = std::mem::take(&mut p).finish(now) {
                        songs.push(s);
                    }
                    skipping = false;
                    continue;
                }
                if skipping {
                    if lname.starts_with("end_of_") {
                        skipping = false;
                        p.open = None;
                    }
                    continue;
                }
                if matches!(
                    lname.as_str(),
                    "start_of_tab" | "sot" | "start_of_grid" | "sog"
                ) {
                    skipping = true;
                    p.close();
                    continue;
                }
                p.directive(name.trim(), value);
                continue;
            }
            if skipping {
                continue;
            }
            if t.is_empty() {
                if p.open.as_ref().is_some_and(|o| !o.explicit) {
                    p.close();
                }
                continue;
            }
            p.lyric_line(line);
        }
        if let Some(s) = p.finish(now) {
            songs.push(s);
        }
        if songs.is_empty() {
            return Err(ImportError::Malformed("ChordPro: no lyrics found".into()));
        }
        Ok(songs)
    }
}

/// Serialises a song to ChordPro (chords inline, sections as `start_of_*` environments).
pub fn export_chordpro(song: &Song) -> String {
    let mut out = String::new();
    let mut push = |k: &str, v: &str| {
        out.push_str(&format!("{{{k}: {v}}}\n"));
    };
    push("title", &song.title);
    if let Some(v) = &song.alt_title {
        push("subtitle", v);
    }
    if let Some(v) = &song.author {
        push("artist", v);
    }
    if let Some(v) = &song.copyright {
        push("copyright", v);
    }
    if let Some(v) = &song.ccli {
        push("ccli", v);
    }
    if let Some(v) = &song.key {
        push("key", v);
    }
    if let Some(v) = song.bpm {
        push("tempo", &v.to_string());
    }
    for s in &song.sections {
        let env = match s.kind {
            SectionKind::Chorus => "chorus",
            SectionKind::Bridge => "bridge",
            _ => "verse",
        };
        out.push_str(&format!("\n{{start_of_{env}: {}}}\n", s.label));
        out.push_str(&s.lyrics);
        out.push_str(&format!("\n{{end_of_{env}}}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use SectionKind::*;

    fn import(text: &str) -> Vec<Song> {
        ChordProImporter { now_ms: Some(1) }.import(text).unwrap()
    }

    const SONG: &str = "{title: Amazing Grace}
{subtitle: Gracia asombrosa}
{artist: John Newton}
{key: G}
{tempo: 72}
{copyright: Dominio público}
# a comment
{start_of_verse: Verso 1}
[G]Amazing [C]grace, how [G]sweet the [Em]sound
That [G]saved a [D]wretch like [G]me
{end_of_verse}

{start_of_chorus}
[C]Praise [G]God
{end_of_chorus}

{comment: Verso 2}
'Twas [G]grace that [C]taught my [G]heart to fear

{chorus}
";

    #[test]
    fn parses_metadata_sections_and_chord_retention() {
        let s = &import(SONG)[0];
        assert_eq!(s.title, "Amazing Grace");
        assert_eq!(s.alt_title.as_deref(), Some("Gracia asombrosa"));
        assert_eq!(s.author.as_deref(), Some("John Newton"));
        assert_eq!(s.key.as_deref(), Some("G"));
        assert_eq!(s.bpm, Some(72));
        let kinds: Vec<_> = s.sections.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, [Verse, Chorus, Verse]);
        assert!(s.sections[0].lyrics.contains("[G]Amazing [C]grace"));
        assert_eq!(s.sections[0].label, "Verso 1");
        assert_eq!(s.sections[2].label, "Verso 2");
        let seq: Vec<_> = s.arrangements[0]
            .sequence
            .iter()
            .map(|i| i.as_str())
            .collect();
        assert_eq!(seq, ["v1", "c", "v2", "c"]);
    }

    #[test]
    fn short_directives_and_blank_line_blocks() {
        let s = &import("{t: Dos bloques}\n{sov}\n[A]uno\n\ndos\n{eov}\n\ntres\n\ncuatro")[0];
        // Inside an explicit environment a blank line does not split (and is dropped).
        assert_eq!(s.sections[0].lyrics, "[A]uno\ndos");
        assert_eq!(s.sections.len(), 3);
    }

    #[test]
    fn new_song_splits_files() {
        let songs = import("{title: A}\nuno\n{new_song}\n{title: B}\ndos\n");
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[1].title, "B");
    }

    #[test]
    fn tabs_are_skipped_and_garbage_errors() {
        let s = &import("{t: T}\n{start_of_tab}\ne|---0---|\n{end_of_tab}\nletra\n")[0];
        assert_eq!(s.sections.len(), 1);
        assert_eq!(s.sections[0].lyrics, "letra");
        assert!(ChordProImporter::default()
            .import("{title: Solo título}")
            .is_err());
    }

    #[test]
    fn export_round_trips() {
        let s = import(SONG).remove(0);
        let again = import(&export_chordpro(&s)).remove(0);
        assert_eq!(again.title, s.title);
        assert_eq!(again.sections.len(), s.sections.len());
        assert_eq!(again.sections[0].lyrics, s.sections[0].lyrics);
        assert_eq!(again.key, s.key);
    }
}
