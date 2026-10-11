//! Song logic: expand a song + arrangement into slides, and import songs from other formats.
//!
//! Owner: builder "songs". Contracts fixed by the architect:
//! * [`SlideSplitter`] turns a [`Song`] into [`Slide`]s consumed by `Command::LoadShow`.
//! * [`SongImporter`] implementations parse a file into [`Song`]s (never network, never panic on
//!   malformed input; return [`ImportError`]).

pub mod chordpro;
pub mod chords;
mod common;
pub mod openlyrics;
pub mod opensong;
pub mod text;

pub use chordpro::{export_chordpro, ChordProImporter};
pub use chords::{
    is_chord, is_chord_line, merge_chord_line, transpose_chord, transpose_key, transpose_song,
    transpose_text,
};
pub use openlyrics::OpenLyricsImporter;
pub use opensong::OpenSongImporter;
pub use text::TextImporter;

use hollyshow_core::{ArrangementId, Slide, SlideKind, Song, Theme};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported or malformed input: {0}")]
    Malformed(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Splitting parameters. Derived from the theme (`max_lines_per_slide`) unless overridden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitOptions {
    pub max_lines: u8,
    /// Honour manual `---` separators inside section lyrics.
    pub honor_manual_breaks: bool,
    /// Semitones applied to chord charts (`ItemContent::Song::transpose`).
    pub transpose: i8,
}

impl SplitOptions {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            max_lines: theme.max_lines_per_slide.max(1),
            honor_manual_breaks: true,
            transpose: 0,
        }
    }
}

pub trait SlideSplitter: Send + Sync {
    fn split(
        &self,
        song: &Song,
        arrangement: Option<&ArrangementId>,
        opts: SplitOptions,
    ) -> Vec<Slide>;
}

pub trait SongImporter {
    /// Short format name: `openlyrics`, `chordpro`, `opensong`, `text`.
    fn format(&self) -> &'static str;
    fn import(&self, input: &str) -> Result<Vec<Song>, ImportError>;
}

/// Guesses the format of `input`: `openlyrics`, `opensong`, `chordpro` or `text`.
pub fn detect_format(input: &str) -> &'static str {
    let t = input.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with('<') {
        let head: String = t.chars().take(4000).collect::<String>().to_lowercase();
        if head.contains("openlyrics.info") || head.contains("<properties") {
            return "openlyrics";
        }
        if head.contains("<song") {
            return "opensong";
        }
    }
    let directive = t.lines().take(20).any(|l| {
        let l = l.trim();
        l.starts_with('{') && l.ends_with('}') && l.contains(|c: char| c.is_alphabetic())
    });
    if directive {
        return "chordpro";
    }
    "text"
}

/// Imports `input` with the importer chosen by [`detect_format`].
pub fn import_auto(input: &str) -> Result<Vec<Song>, ImportError> {
    match detect_format(input) {
        "openlyrics" => OpenLyricsImporter::default().import(input),
        "opensong" => OpenSongImporter::default().import(input),
        "chordpro" => ChordProImporter::default().import(input),
        _ => TextImporter::default().import(input),
    }
}

/// Straightforward splitter: sections in arrangement order, `max_lines` lines per slide, manual
/// `---` breaks honoured, chords stripped from projected lines and kept in `chart`.
/// `SplitOptions::transpose` shifts the chords of `chart` (and only those; the lyric lines carry
/// no chords).
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultSplitter;

/// Removes inline ChordPro chords: `[G]Sublime [D]gracia` becomes `Sublime gracia`.
pub fn strip_chords(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_chord = false;
    for c in line.chars() {
        match c {
            '[' => in_chord = true,
            ']' if in_chord => in_chord = false,
            _ if in_chord => {}
            _ => out.push(c),
        }
    }
    out
}

impl SlideSplitter for DefaultSplitter {
    fn split(
        &self,
        song: &Song,
        arrangement: Option<&ArrangementId>,
        opts: SplitOptions,
    ) -> Vec<Slide> {
        let max = usize::from(opts.max_lines.max(1));
        let shift = i32::from(opts.transpose);
        let flats = shift % 12 != 0 && {
            let all: String = song
                .sections
                .iter()
                .map(|s| s.lyrics.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            chords::prefers_flats(song.key.as_deref(), &all, shift)
        };
        let mut slides = Vec::new();
        for section in song.ordered_sections(arrangement) {
            let raw: Vec<&str> = section.lyrics.lines().collect();
            let trans: Vec<&str> = section
                .translation
                .as_deref()
                .map(|t| t.lines().collect())
                .unwrap_or_default();
            // Chunk by index so translation lines stay aligned with lyric lines.
            let mut chunks: Vec<(usize, usize)> = Vec::new();
            let mut start = 0;
            let mut count = 0;
            for (i, line) in raw.iter().enumerate() {
                if opts.honor_manual_breaks && line.trim() == "---" {
                    if count > 0 {
                        chunks.push((start, i));
                    }
                    start = i + 1;
                    count = 0;
                    continue;
                }
                if line.trim().is_empty() && count == 0 {
                    start = i + 1;
                    continue;
                }
                count += 1;
                if count == max {
                    chunks.push((start, i + 1));
                    start = i + 1;
                    count = 0;
                }
            }
            if count > 0 {
                chunks.push((start, raw.len()));
            }
            for (a, b) in chunks {
                let part = &raw[a..b];
                let mut slide = Slide::blank(format!("{}:{}", section.id, slides.len()));
                slide.kind = SlideKind::Lyrics;
                slide.label = Some(section.label.clone());
                slide.lines = part.iter().map(|l| strip_chords(l)).collect();
                slide.chart = part
                    .iter()
                    .map(|l| chords::transpose_text(l, shift, flats))
                    .collect();
                if !trans.is_empty() {
                    slide.translation = (a..b)
                        .map(|i| trans.get(i).copied().unwrap_or("").to_owned())
                        .collect();
                }
                slide.theme_id = song.theme_id.clone();
                slides.push(slide);
            }
        }
        slides
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{Arrangement, Section, SectionId, SectionKind, SongId};

    fn song(lyrics: &str) -> Song {
        let sec = |id: &str, lyrics: &str| Section {
            id: SectionId::new(id),
            kind: SectionKind::Verse,
            label: id.into(),
            lyrics: lyrics.into(),
            translation: None,
        };
        Song {
            id: SongId::new("s"),
            title: "T".into(),
            alt_title: None,
            author: None,
            copyright: None,
            ccli: None,
            key: None,
            bpm: None,
            tags: vec![],
            theme_id: None,
            sections: vec![sec("v1", lyrics), sec("c", "[G]Coro")],
            arrangements: vec![Arrangement {
                id: ArrangementId::new("a"),
                name: "A".into(),
                sequence: vec![
                    SectionId::new("v1"),
                    SectionId::new("c"),
                    SectionId::new("v1"),
                ],
            }],
            created_at: 0,
            updated_at: 0,
        }
    }

    fn opts(max: u8) -> SplitOptions {
        SplitOptions {
            max_lines: max,
            honor_manual_breaks: true,
            transpose: 0,
        }
    }

    #[test]
    fn strips_inline_chords() {
        assert_eq!(strip_chords("[G]Sublime [D/F#]gracia"), "Sublime gracia");
        assert_eq!(strip_chords("sin acordes"), "sin acordes");
    }

    #[test]
    fn follows_arrangement_and_chunks_by_max_lines() {
        let s = song("uno\ndos\ntres\ncuatro\ncinco");
        let slides = DefaultSplitter.split(&s, None, opts(2));
        // v1 -> 3 slides, c -> 1, v1 again -> 3.
        assert_eq!(slides.len(), 7);
        assert_eq!(slides[0].lines, ["uno", "dos"]);
        assert_eq!(slides[2].lines, ["cinco"]);
        assert_eq!(slides[3].lines, ["Coro"]);
        assert_eq!(slides[3].chart, ["[G]Coro"]);
        let ids: std::collections::HashSet<_> = slides.iter().map(|x| &x.id).collect();
        assert_eq!(ids.len(), slides.len(), "slide ids must be unique");
    }

    #[test]
    fn transposes_the_chart_but_not_the_projected_lines() {
        let s = song("[G]uno [D]dos");
        let o = SplitOptions {
            transpose: 2,
            ..opts(4)
        };
        let slides = DefaultSplitter.split(&s, None, o);
        assert_eq!(slides[0].lines, ["uno dos"]);
        assert_eq!(slides[0].chart, ["[A]uno [E]dos"]);
    }

    #[test]
    fn detects_formats() {
        assert_eq!(
            detect_format("<song xmlns=\"http://openlyrics.info/namespace/2009/song\">"),
            "openlyrics"
        );
        assert_eq!(
            detect_format("<?xml version=\"1.0\"?>\n<song><title>x</title></song>"),
            "opensong"
        );
        assert_eq!(detect_format("{title: Hola}\n[G]hola"), "chordpro");
        assert_eq!(detect_format("Verso 1\nhola"), "text");
        let songs = import_auto("{title: Hola}\n[G]hola mundo").unwrap();
        assert_eq!(songs[0].title, "Hola");
    }

    #[test]
    fn manual_breaks_force_a_new_slide() {
        let s = song("uno\ndos\n---\ntres");
        let slides = DefaultSplitter.split(&s, None, opts(4));
        assert_eq!(slides[0].lines, ["uno", "dos"]);
        assert_eq!(slides[1].lines, ["tres"]);
    }
}
