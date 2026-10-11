//! Chord recognition and transposition.
//!
//! Understands English (`C D E F G A B`) and Spanish/Latin (`Do Re Mi Fa Sol La Si`) notation,
//! sharps/flats, qualities (`m7`, `maj7`, `sus4`, `add9`, `dim`, `m7b5`...) and slash chords. A
//! chord keeps its own notation when transposed, so a Spanish chart stays Spanish.

use hollyshow_core::Song;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notation {
    English,
    Spanish,
}

#[derive(Debug, Clone, Copy)]
struct Root {
    /// 0 = C ... 11 = B.
    pc: u8,
    notation: Notation,
    /// Bytes consumed from the input.
    len: usize,
}

const SHARP_EN: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const FLAT_EN: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];
const SHARP_ES: [&str; 12] = [
    "Do", "Do#", "Re", "Re#", "Mi", "Fa", "Fa#", "Sol", "Sol#", "La", "La#", "Si",
];
const FLAT_ES: [&str; 12] = [
    "Do", "Reb", "Re", "Mib", "Mi", "Fa", "Solb", "Sol", "Lab", "La", "Sib", "Si",
];

fn english_base(c: char) -> Option<u8> {
    Some(match c {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    })
}

fn spanish_base(s: &str) -> Option<(u8, usize)> {
    // "Sol" must be tried before "Si".
    for (name, pc) in [
        ("Sol", 7u8),
        ("Do", 0),
        ("Re", 2),
        ("Mi", 4),
        ("Fa", 5),
        ("La", 9),
        ("Si", 11),
    ] {
        if s.starts_with(name) {
            return Some((pc, name.len()));
        }
    }
    None
}

/// Accidental right after a root: returns (delta, bytes).
fn accidental(s: &str) -> (i8, usize) {
    match s.chars().next() {
        Some('#') => (1, 1),
        Some('♯') => (1, '♯'.len_utf8()),
        Some('b') => (-1, 1),
        Some('♭') => (-1, '♭'.len_utf8()),
        _ => (0, 0),
    }
}

fn parse_english_root(s: &str) -> Option<Root> {
    let base = english_base(s.chars().next()?)?;
    let (delta, n) = accidental(&s[1..]);
    Some(Root {
        pc: (base as i8 + delta).rem_euclid(12) as u8,
        notation: Notation::English,
        len: 1 + n,
    })
}

fn parse_spanish_root(s: &str) -> Option<Root> {
    let (base, l) = spanish_base(s)?;
    let (delta, n) = accidental(&s[l..]);
    Some(Root {
        pc: (base as i8 + delta).rem_euclid(12) as u8,
        notation: Notation::Spanish,
        len: l + n,
    })
}

/// Is `s` a plausible chord quality/extension (the part after the root, before any `/bass`)?
fn valid_suffix(s: &str) -> bool {
    const WORDS: [&str; 8] = ["maj", "min", "dim", "aug", "sus", "add", "omit", "alt"];
    let mut rest = s;
    while !rest.is_empty() {
        let lower = rest.to_lowercase();
        if let Some(w) = WORDS.iter().find(|w| lower.starts_with(**w)) {
            rest = &rest[w.len()..];
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        if matches!(
            c,
            'm' | 'M' | '+' | '-' | '°' | 'ø' | 'b' | '#' | '♯' | '♭' | '(' | ')' | ',' | '*'
        ) || c.is_ascii_digit()
        {
            rest = &rest[c.len_utf8()..];
            continue;
        }
        return false;
    }
    true
}

#[derive(Debug, Clone)]
struct Chord<'a> {
    root: Root,
    suffix: &'a str,
    bass: Option<Root>,
}

fn parse_chord(s: &str) -> Option<Chord<'_>> {
    if s.is_empty() || s.len() > 24 {
        return None;
    }
    let (main, bass_str) = match s.split_once('/') {
        Some((m, b)) => (m, Some(b)),
        None => (s, None),
    };
    // English first, then Spanish: "Fadd9" is English, "Fa" and "Dom" are Spanish.
    let root = [parse_english_root(main), parse_spanish_root(main)]
        .into_iter()
        .flatten()
        .find(|r| valid_suffix(&main[r.len..]))?;
    let bass = match bass_str {
        None => None,
        Some(b) => {
            let r = [parse_english_root(b), parse_spanish_root(b)]
                .into_iter()
                .flatten()
                .find(|r| r.len == b.len())?;
            Some(r)
        }
    };
    Some(Chord {
        root,
        suffix: &main[root.len..],
        bass,
    })
}

/// True when `token` is a chord such as `G`, `F#m7`, `Sol/Si`, `D/F#`.
pub fn is_chord(token: &str) -> bool {
    parse_chord(token).is_some()
}

fn spell(pc: u8, notation: Notation, flats: bool) -> &'static str {
    let table = match (notation, flats) {
        (Notation::English, false) => &SHARP_EN,
        (Notation::English, true) => &FLAT_EN,
        (Notation::Spanish, false) => &SHARP_ES,
        (Notation::Spanish, true) => &FLAT_ES,
    };
    table[usize::from(pc % 12)]
}

/// Transposes one chord by `semitones`; text that is not a chord is returned unchanged.
pub fn transpose_chord(chord: &str, semitones: i32, flats: bool) -> String {
    let Some(c) = parse_chord(chord) else {
        return chord.to_owned();
    };
    let shift = semitones.rem_euclid(12) as u8;
    let mut out = String::from(spell((c.root.pc + shift) % 12, c.root.notation, flats));
    out.push_str(c.suffix);
    if let Some(b) = c.bass {
        out.push('/');
        out.push_str(spell((b.pc + shift) % 12, b.notation, flats));
    }
    out
}

/// Transposes a key signature such as `G`, `F#m`, `Sol`.
pub fn transpose_key(key: &str, semitones: i32) -> String {
    let flats = key_prefers_flats(&transpose_chord(key, semitones, false));
    transpose_chord(key, semitones, flats)
}

/// Does the key `key` conventionally use flats? (F, Bb, Eb, Ab, Db and their relative minors.)
fn key_prefers_flats(key: &str) -> bool {
    let Some(c) = parse_chord(key) else {
        return false;
    };
    let minor = c.suffix.starts_with('m') && !c.suffix.to_lowercase().starts_with("maj");
    let major_pc = if minor {
        (c.root.pc + 3) % 12
    } else {
        c.root.pc
    };
    matches!(major_pc, 5 | 10 | 3 | 8 | 1)
}

/// Iterates over the `[...]` markers of a ChordPro line: (start of the inside, end of the inside).
fn bracket_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open: Option<usize> = None;
    for (i, c) in text.char_indices() {
        match c {
            '[' => open = Some(i + 1),
            ']' => {
                if let Some(s) = open.take() {
                    spans.push((s, i));
                }
            }
            _ => {}
        }
    }
    spans
}

/// Chooses flats or sharps for a transposition of `text` (inline ChordPro) by `semitones`.
///
/// With a known `key` the target key decides; otherwise the existing spelling of the chart does,
/// and only when it has no accidentals does the direction of the shift.
pub fn prefers_flats(key: Option<&str>, text: &str, semitones: i32) -> bool {
    if let Some(k) = key.filter(|k| is_chord(k)) {
        return key_prefers_flats(&transpose_chord(k, semitones, false));
    }
    let mut saw_flat = false;
    let mut saw_sharp = false;
    for (s, e) in bracket_spans(text) {
        let raw = &text[s..e];
        let Some(c) = parse_chord(raw) else { continue };
        let bass_raw = raw.split_once('/').map(|(_, b)| b);
        for root_txt in std::iter::once(&raw[..c.root.len]).chain(bass_raw) {
            saw_flat |= root_txt.ends_with(['b', '♭']);
            saw_sharp |= root_txt.ends_with(['#', '♯']);
        }
    }
    match (saw_flat, saw_sharp) {
        (true, false) => true,
        (false, true) => false,
        _ => semitones < 0,
    }
}

/// Transposes every chord inside `[...]` markers; other bracketed text (`[Coro]`) is untouched.
pub fn transpose_text(text: &str, semitones: i32, flats: bool) -> String {
    if semitones.rem_euclid(12) == 0 {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (s, e) in bracket_spans(text) {
        out.push_str(&text[last..s]);
        out.push_str(&transpose_chord(&text[s..e], semitones, flats));
        last = e;
    }
    out.push_str(&text[last..]);
    out
}

/// Returns a copy of `song` with every chord and the key transposed.
pub fn transpose_song(song: &Song, semitones: i32) -> Song {
    let mut out = song.clone();
    if semitones.rem_euclid(12) == 0 {
        return out;
    }
    let all: String = song
        .sections
        .iter()
        .map(|s| s.lyrics.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let flats = prefers_flats(song.key.as_deref(), &all, semitones);
    for s in &mut out.sections {
        s.lyrics = transpose_text(&s.lyrics, semitones, flats);
    }
    if let Some(k) = &song.key {
        out.key = Some(transpose_chord(k, semitones, flats));
    }
    out
}

/// A line made only of chords (and bar/repeat marks), as written in "chords over lyrics" charts.
pub fn is_chord_line(line: &str) -> bool {
    let mut any = false;
    for tok in line.split_whitespace() {
        if matches!(tok, "|" | "||" | "-" | "/" | "%") || tok.starts_with('x') && tok.len() <= 3 {
            continue;
        }
        if !is_chord(tok) {
            return false;
        }
        any = true;
    }
    any
}

/// Merges a chord line written above a lyric line into inline ChordPro:
/// `"G   D"` over `"Sublime gracia"` becomes `"[G]Subl[D]ime gracia"`.
pub fn merge_chord_line(chords: &str, lyric: &str) -> String {
    let mut marks: Vec<(usize, String)> = Vec::new();
    let mut cur = String::new();
    let mut start = 0usize;
    for (col, c) in chords.chars().chain(std::iter::once(' ')).enumerate() {
        if c.is_whitespace() {
            if !cur.is_empty() {
                if is_chord(&cur) {
                    marks.push((start, std::mem::take(&mut cur)));
                } else {
                    cur.clear();
                }
            }
        } else {
            if cur.is_empty() {
                start = col;
            }
            cur.push(c);
        }
    }
    let mut chars: Vec<char> = lyric.chars().collect();
    for (pos, chord) in marks.into_iter().rev() {
        if pos > chars.len() {
            chars.resize(pos, ' ');
        }
        let ins: Vec<char> = format!("[{chord}]").chars().collect();
        chars.splice(pos..pos, ins);
    }
    chars.into_iter().collect::<String>().trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{Section, SectionId, SectionKind, SongId};

    #[test]
    fn recognises_chords_in_both_notations() {
        for c in [
            "G", "F#m7", "Bbmaj7", "D/F#", "Csus4", "Fadd9", "Am7b5", "Do", "Dom", "Sol/Si",
            "Fa#m", "Lab7", "Sib", "Re",
        ] {
            assert!(is_chord(c), "{c} should be a chord");
        }
        for c in ["Hola", "Dios", "Santo", "Hi", "X", "", "Gracia", "Amor"] {
            assert!(!is_chord(c), "{c} should not be a chord");
        }
    }

    #[test]
    fn transposes_with_sharps_and_flats() {
        assert_eq!(transpose_chord("G", 2, false), "A");
        assert_eq!(transpose_chord("G", 3, true), "Bb");
        assert_eq!(transpose_chord("G", 3, false), "A#");
        assert_eq!(transpose_chord("F#m7", 1, false), "Gm7");
        assert_eq!(transpose_chord("D/F#", 2, false), "E/G#");
        assert_eq!(transpose_chord("C", -1, false), "B");
        assert_eq!(transpose_chord("C", 12, false), "C");
        assert_eq!(transpose_chord("Hola", 5, false), "Hola");
    }

    #[test]
    fn spanish_chords_stay_spanish() {
        assert_eq!(transpose_chord("Do", 2, false), "Re");
        assert_eq!(transpose_chord("Lam", 3, false), "Dom");
        assert_eq!(transpose_chord("Sol/Si", 2, false), "La/Do#");
        assert_eq!(transpose_chord("Mi7", 1, false), "Fa7");
    }

    #[test]
    fn inline_text_transposes_only_chords() {
        let t = transpose_text("[G]Sublime [D/F#]gracia [Coro]", 2, false);
        assert_eq!(t, "[A]Sublime [E/G#]gracia [Coro]");
        assert_eq!(transpose_text("[G]x", 0, false), "[G]x");
    }

    #[test]
    fn flat_preference_follows_key_then_chart() {
        assert!(prefers_flats(Some("G"), "", 3), "G up 3 = Bb major");
        assert!(!prefers_flats(Some("G"), "", 2), "A major uses sharps");
        assert!(prefers_flats(None, "[Bb]x", 2));
        assert!(!prefers_flats(None, "[F#]x", -2));
        assert!(prefers_flats(None, "[G]x", -2));
        assert!(!prefers_flats(None, "[G]x", 2));
    }

    #[test]
    fn key_transposition() {
        assert_eq!(transpose_key("G", 3), "Bb");
        assert_eq!(transpose_key("Em", 2), "F#m");
        assert_eq!(transpose_key("Sol", 2), "La");
    }

    #[test]
    fn chord_line_detection() {
        assert!(is_chord_line("G   D/F#   Em7  C"));
        assert!(is_chord_line("| G | D |"));
        assert!(!is_chord_line("Sublime gracia del Señor"));
        assert!(!is_chord_line(""));
    }

    #[test]
    fn merges_chords_over_lyrics() {
        assert_eq!(
            merge_chord_line("G       D", "Sublime gracia"),
            "[G]Sublime [D]gracia"
        );
        // Chord beyond the end of the lyric is padded.
        assert_eq!(merge_chord_line("G    C", "Hola"), "[G]Hola [C]");
        // Chord only line (instrumental).
        assert_eq!(merge_chord_line("Am F", ""), "[Am]   [F]");
    }

    #[test]
    fn transposes_a_whole_song() {
        let song = Song {
            id: SongId::new("s"),
            title: "t".into(),
            alt_title: None,
            author: None,
            copyright: None,
            ccli: None,
            key: Some("G".into()),
            bpm: None,
            tags: vec![],
            theme_id: None,
            sections: vec![Section {
                id: SectionId::new("v1"),
                kind: SectionKind::Verse,
                label: "Verso 1".into(),
                lyrics: "[G]uno [C]dos".into(),
                translation: None,
            }],
            arrangements: vec![],
            created_at: 0,
            updated_at: 0,
        };
        let up = transpose_song(&song, 2);
        assert_eq!(up.key.as_deref(), Some("A"));
        assert_eq!(up.sections[0].lyrics, "[A]uno [D]dos");
        assert_eq!(transpose_song(&song, 12), song);
    }
}
