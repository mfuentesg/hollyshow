//! Shared machinery of the importers: section label parsing, section/arrangement building and
//! stable song ids.

use hollyshow_core::text::fold;
use hollyshow_core::{Arrangement, ArrangementId, Section, SectionId, SectionKind, Song, SongId};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current time in Unix milliseconds (0 if the clock is before the epoch).
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A recognised section heading such as `Verso 2`, `[Coro]`, `Chorus x2`, `V1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParsedLabel {
    pub kind: SectionKind,
    pub number: Option<u32>,
    /// How many times the section is played (`x2`); at least 1.
    pub repeat: u32,
}

fn kind_of(word: &str, strict: bool) -> Option<SectionKind> {
    use SectionKind::*;
    Some(match word {
        "verso" | "estrofa" | "verse" | "strofa" | "estancia" => Verse,
        "coro" | "estribillo" | "chorus" | "refrain" | "refran" | "stribillo" => Chorus,
        "precoro" | "pre-coro" | "pre coro" | "prechorus" | "pre-chorus" | "pre chorus"
        | "preestribillo" | "pre-estribillo" => PreChorus,
        "puente" | "bridge" => Bridge,
        "intro" | "introduccion" | "introduction" | "entrada" => Intro,
        "final" | "outro" | "ending" | "coda" | "cierre" | "fin" => Outro,
        "tag" | "remate" => Tag,
        "interludio" | "interlude" | "instrumental" | "solo" => Interlude,
        // One-letter codes (OpenLyrics/OpenSong/ChordPro) only where they cannot be lyrics.
        "v" if !strict => Verse,
        "c" if !strict => Chorus,
        "p" if !strict => PreChorus,
        "b" if !strict => Bridge,
        "i" if !strict => Intro,
        "e" | "o" if !strict => Outro,
        "t" if !strict => Tag,
        _ => return None,
    })
}

/// Parses a heading line. `strict` is true for free text, where a bare `C` could be a lyric: then
/// one-letter codes are only accepted inside brackets or with a number (`V1`).
pub(crate) fn parse_label(raw: &str, mut bracketed_hint: bool) -> Option<ParsedLabel> {
    let mut s = raw.trim();
    // "[Coro] x2": heading in brackets followed by a repeat marker.
    let joined;
    if s.starts_with('[') {
        if let Some(i) = s.find(']') {
            if i + 1 < s.len() {
                joined = format!("{} {}", &s[1..i], s[i + 1..].trim());
                s = joined.as_str();
                bracketed_hint = true;
            }
        }
    }
    if s.is_empty() || s.chars().count() > 32 {
        return None;
    }
    let mut bracketed = bracketed_hint;
    for (o, c) in [('[', ']'), ('(', ')'), ('{', '}')] {
        if s.starts_with(o) && s.ends_with(c) && s.len() >= 2 {
            s = s[1..s.len() - 1].trim();
            bracketed = true;
            break;
        }
    }
    let s = s.trim_end_matches([':', '.']).trim();
    let folded = fold(s);
    let mut text = folded.as_str();

    // Trailing repeat marker: "x2", "(x2)", "×2", "2x", "bis".
    let mut repeat = 1u32;
    let t = text.trim_end_matches(')').trim_end();
    if let Some(idx) = t.rfind(['x', '×']) {
        let tail = t[idx..].trim_start_matches(['x', '×']);
        let head = t[..idx].trim_end_matches('(').trim_end();
        if let Ok(n) = tail.trim().parse::<u32>() {
            if (2..=9).contains(&n) && !head.is_empty() && t[..idx].ends_with([' ', '(']) {
                repeat = n;
                text = head;
            }
        }
    } else if let Some(h) = t.strip_suffix(" bis") {
        repeat = 2;
        text = h.trim_end();
    }

    // Split trailing number: "verso 2", "v2", "coro2".
    let digits_at = text
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, _)| i);
    let (word, number) = match digits_at {
        Some(i) => {
            let num: u32 = text[i..].trim().parse().ok()?;
            (text[..i].trim().trim_end_matches('-').trim(), Some(num))
        }
        None => (text.trim(), None),
    };
    if word.is_empty() {
        // "[1]" in OpenSong means verse 1.
        return (bracketed && number.is_some()).then_some(ParsedLabel {
            kind: SectionKind::Verse,
            number,
            repeat,
        });
    }
    let strict = !(bracketed || number.is_some());
    let kind = kind_of(word, strict)?;
    Some(ParsedLabel {
        kind,
        number,
        repeat,
    })
}

fn prefix(kind: SectionKind) -> &'static str {
    use SectionKind::*;
    match kind {
        Verse => "v",
        Chorus => "c",
        PreChorus => "pc",
        Bridge => "b",
        Intro => "i",
        Outro => "o",
        Tag => "t",
        Interlude => "x",
        Other => "s",
    }
}

/// Spanish display label of a section.
pub(crate) fn display_label(kind: SectionKind, number: u32, numbered: bool) -> String {
    use SectionKind::*;
    let base = match kind {
        Verse => "Verso",
        Chorus => "Coro",
        PreChorus => "Pre-coro",
        Bridge => "Puente",
        Intro => "Intro",
        Outro => "Final",
        Tag => "Tag",
        Interlude => "Interludio",
        Other => "Sección",
    };
    if numbered || kind == Verse {
        format!("{base} {number}")
    } else {
        base.to_owned()
    }
}

/// Accumulates sections (in written order) and the play sequence.
#[derive(Default)]
pub(crate) struct SectionBuilder {
    pub sections: Vec<Section>,
    pub sequence: Vec<SectionId>,
    ids: HashSet<String>,
}

impl SectionBuilder {
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    fn count_of(&self, kind: SectionKind) -> u32 {
        self.sections.iter().filter(|s| s.kind == kind).count() as u32
    }

    /// Adds a section; `number` is the explicit number from the source, if any. Returns its id.
    /// Does not touch the play sequence.
    pub fn add(
        &mut self,
        kind: SectionKind,
        number: Option<u32>,
        lyrics: String,
        translation: Option<String>,
        custom_label: Option<String>,
    ) -> SectionId {
        let mut n = number.unwrap_or_else(|| self.count_of(kind) + 1);
        let numbered = number.is_some() || n > 1;
        let id = loop {
            let candidate = if n == 1 && kind != SectionKind::Verse {
                prefix(kind).to_owned()
            } else {
                format!("{}{}", prefix(kind), n)
            };
            if self.ids.insert(candidate.clone()) {
                break candidate;
            }
            n += 1;
        };
        let label = custom_label.unwrap_or_else(|| display_label(kind, n, numbered));
        let id = SectionId::new(id);
        self.sections.push(Section {
            id: id.clone(),
            kind,
            label,
            lyrics,
            translation,
        });
        id
    }

    pub fn section(&self, id: &SectionId) -> Option<&Section> {
        self.sections.iter().find(|s| &s.id == id)
    }

    pub fn play(&mut self, id: &SectionId) {
        self.sequence.push(id.clone());
    }

    /// Last section of `kind` (and `number` when given).
    pub fn find(&self, kind: SectionKind, number: Option<u32>) -> Option<SectionId> {
        self.sections
            .iter()
            .rev()
            .find(|s| {
                s.kind == kind
                    && number.is_none_or(|n| s.id.as_str() == format!("{}{}", prefix(kind), n))
            })
            .map(|s| s.id.clone())
    }

    /// An arrangement only when the play order differs from the written order.
    pub fn arrangements(&self) -> Vec<Arrangement> {
        let written: Vec<&SectionId> = self.sections.iter().map(|s| &s.id).collect();
        let played: Vec<&SectionId> = self.sequence.iter().collect();
        if self.sequence.is_empty() || written == played {
            return vec![];
        }
        vec![Arrangement {
            id: ArrangementId::new("a1"),
            name: "Orden".into(),
            sequence: self.sequence.clone(),
        }]
    }
}

/// FNV-1a 64: stable across Rust versions (unlike `DefaultHasher`).
fn fnv1a(data: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Content-derived id: importing the same song twice yields the same id (no duplicates).
pub(crate) fn song_id(title: &str, sections: &[Section]) -> SongId {
    let mut material = fold(title);
    for s in sections {
        material.push('\u{1f}');
        material.push_str(&fold(&crate::strip_chords(&s.lyrics)));
    }
    SongId::new(format!("song-{:016x}", fnv1a(&material)))
}

/// Metadata shared by all importers before assembly.
#[derive(Default, Debug, Clone)]
pub(crate) struct Meta {
    pub title: Option<String>,
    pub alt_title: Option<String>,
    pub author: Option<String>,
    pub copyright: Option<String>,
    pub ccli: Option<String>,
    pub key: Option<String>,
    pub bpm: Option<u16>,
    pub tags: Vec<String>,
}

pub(crate) fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

/// Parses a tempo such as `72`, `72 bpm`, `72.5`.
pub(crate) fn parse_bpm(s: &str) -> Option<u16> {
    let digits: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    digits
        .parse::<f32>()
        .ok()
        .map(|v| v.round() as u16)
        .filter(|v| *v > 0)
}

pub(crate) fn assemble(meta: Meta, b: SectionBuilder, now: i64) -> Song {
    let title = meta.title.unwrap_or_else(|| "Sin título".to_owned());
    let arrangements = b.arrangements();
    let id = song_id(&title, &b.sections);
    Song {
        id,
        title,
        alt_title: meta.alt_title,
        author: meta.author,
        copyright: meta.copyright,
        ccli: meta.ccli,
        key: meta.key,
        bpm: meta.bpm,
        tags: meta.tags,
        theme_id: None,
        sections: b.sections,
        arrangements,
        created_at: now,
        updated_at: now,
    }
}

/// Normalises newlines and removes a UTF-8 BOM.
pub(crate) fn clean_input(input: &str) -> String {
    input
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<(SectionKind, Option<u32>, u32)> {
        parse_label(s, false).map(|l| (l.kind, l.number, l.repeat))
    }

    #[test]
    fn parses_spanish_and_english_labels() {
        use SectionKind::*;
        assert_eq!(p("Verso 1"), Some((Verse, Some(1), 1)));
        assert_eq!(p("VERSO 2:"), Some((Verse, Some(2), 1)));
        assert_eq!(p("Coro"), Some((Chorus, None, 1)));
        assert_eq!(p("Estribillo"), Some((Chorus, None, 1)));
        assert_eq!(p("Chorus"), Some((Chorus, None, 1)));
        assert_eq!(p("[Coro]"), Some((Chorus, None, 1)));
        assert_eq!(p("Pre-coro"), Some((PreChorus, None, 1)));
        assert_eq!(p("Precoro"), Some((PreChorus, None, 1)));
        assert_eq!(p("Pre-Chorus 2"), Some((PreChorus, Some(2), 1)));
        assert_eq!(p("Puente"), Some((Bridge, None, 1)));
        assert_eq!(p("Bridge"), Some((Bridge, None, 1)));
        assert_eq!(p("Introducción"), Some((Intro, None, 1)));
        assert_eq!(p("Final"), Some((Outro, None, 1)));
        assert_eq!(p("Coro x2"), Some((Chorus, None, 2)));
        assert_eq!(p("Coro (x3)"), Some((Chorus, None, 3)));
        assert_eq!(p("V1"), Some((Verse, Some(1), 1)));
        assert_eq!(p("C2"), Some((Chorus, Some(2), 1)));
    }

    #[test]
    fn one_letter_codes_need_brackets_or_numbers() {
        assert_eq!(p("C"), None);
        assert_eq!(p("Si"), None);
        assert!(parse_label("[C]", false).is_some());
        assert!(parse_label("C", true).is_some());
        assert_eq!(
            parse_label("[1]", false).map(|l| l.kind),
            Some(SectionKind::Verse)
        );
    }

    #[test]
    fn rejects_lyrics() {
        assert_eq!(p("Santo, santo, santo"), None);
        assert_eq!(p("Coro de ángeles cantando gloria al Señor"), None);
        assert_eq!(p(""), None);
    }

    #[test]
    fn builder_assigns_unique_ids_and_labels() {
        let mut b = SectionBuilder::default();
        let v1 = b.add(SectionKind::Verse, None, "a".into(), None, None);
        let c = b.add(SectionKind::Chorus, None, "b".into(), None, None);
        let v2 = b.add(SectionKind::Verse, None, "c".into(), None, None);
        let c2 = b.add(SectionKind::Chorus, None, "d".into(), None, None);
        assert_eq!(
            [v1.as_str(), c.as_str(), v2.as_str(), c2.as_str()],
            ["v1", "c", "v2", "c2"]
        );
        assert_eq!(b.sections[0].label, "Verso 1");
        assert_eq!(b.sections[1].label, "Coro");
        assert_eq!(b.sections[3].label, "Coro 2");
        // Same explicit number twice must not collide.
        let dup = b.add(SectionKind::Verse, Some(1), "z".into(), None, None);
        assert_ne!(dup.as_str(), "v1");
    }

    #[test]
    fn arrangement_only_when_order_differs() {
        let mut b = SectionBuilder::default();
        let v1 = b.add(SectionKind::Verse, None, "a".into(), None, None);
        let c = b.add(SectionKind::Chorus, None, "b".into(), None, None);
        b.play(&v1);
        b.play(&c);
        assert!(b.arrangements().is_empty());
        b.play(&v1);
        assert_eq!(b.arrangements()[0].sequence.len(), 3);
    }

    #[test]
    fn song_id_is_stable_and_content_based() {
        let s = |l: &str| Section {
            id: SectionId::new("v1"),
            kind: SectionKind::Verse,
            label: "Verso 1".into(),
            lyrics: l.into(),
            translation: None,
        };
        let a = song_id("Sublime Gracia", &[s("[G]Cuán dulce")]);
        assert_eq!(a, song_id("sublime gracia", &[s("Cuan dulce")]));
        assert_ne!(a, song_id("Sublime Gracia", &[s("otro")]));
        assert!(a.as_str().starts_with("song-"));
    }
}
