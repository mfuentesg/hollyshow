//! Pure slide-splitting algorithms (PLAN §2.1 "slides automáticos", §2.4 parallel Bible).
//!
//! * [`split_song`]: sections in arrangement order, manual `---` breaks, at most `max_lines` per
//!   slide, balanced chunks (no orphan last line) and a shared font-size group.
//! * [`split_passage`]: one passage in 1..=4 versions cut at the *same* verse boundaries in every
//!   version, so columns advance together.
//!
//! Everything here is deterministic and free of I/O so the songs/bible crates and the app layer
//! can share it and tests can pin the behaviour.

use crate::bible::{BibleRef, ParallelLayout, Passage, Verse, VersionCode};
use crate::ids::{ArrangementId, SlideId};
use crate::song::Song;
use crate::state::{BibleColumn, Slide, SlideKind};
use crate::text::strip_chords;
use std::collections::HashMap;
use thiserror::Error;

/// Most versions shown side by side (PLAN §2.4).
pub const MAX_PARALLEL_VERSIONS: usize = 4;
const MISSING: &str = "\u{2014}";
const MANUAL_BREAK: &str = "---";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LyricsSplit {
    /// Maximum lyric lines per slide (clamped to at least 1).
    pub max_lines: usize,
    /// `---` lines and blank lines end the current slide.
    pub honor_manual_breaks: bool,
    /// Spread lines evenly over the needed slides (5 lines, max 4 -> 3 + 2) instead of filling
    /// greedily (4 + 1).
    pub balance: bool,
    /// Give every slide of the song one `size_group`, so the renderer uses a single font size.
    pub uniform_size: bool,
}

impl Default for LyricsSplit {
    fn default() -> Self {
        Self {
            max_lines: 4,
            honor_manual_breaks: true,
            balance: true,
            uniform_size: true,
        }
    }
}

/// Splits `n` consecutive items into groups of at most `max`; balanced groups differ by at most
/// one item, larger groups first.
fn group_sizes(n: usize, max: usize, balance: bool) -> Vec<usize> {
    if n == 0 {
        return vec![];
    }
    let max = max.max(1);
    let groups = n.div_ceil(max);
    if balance {
        let (base, extra) = (n / groups, n % groups);
        (0..groups).map(|i| base + usize::from(i < extra)).collect()
    } else {
        (0..groups).map(|i| max.min(n - i * max)).collect()
    }
}

/// Groups the indices of the non-separator `lines` into slides. Indices refer to `lines`, so
/// parallel arrays (translation, chord chart) stay aligned.
pub fn chunk_line_indices(lines: &[&str], cfg: LyricsSplit) -> Vec<Vec<usize>> {
    let mut blocks: Vec<Vec<usize>> = vec![];
    let mut cur: Vec<usize> = vec![];
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let is_break = cfg.honor_manual_breaks && (t == MANUAL_BREAK || t.is_empty());
        if is_break {
            if !cur.is_empty() {
                blocks.push(std::mem::take(&mut cur));
            }
        } else if !t.is_empty() {
            cur.push(i);
        }
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }
    let mut out = vec![];
    for block in blocks {
        let mut at = 0;
        for size in group_sizes(block.len(), cfg.max_lines, cfg.balance) {
            out.push(block[at..at + size].to_vec());
            at += size;
        }
    }
    out
}

/// Expands `song` into lyric slides following `arrangement` (or the first one / written order).
///
/// Slide ids are `"{section}:{n}"` with `n` the running slide number, unique even when a section
/// repeats. Projected `lines` have chords stripped; `chart` keeps them. Translation lines are
/// aligned by index and padded with empty strings.
pub fn split_song(
    song: &Song,
    arrangement: Option<&ArrangementId>,
    cfg: LyricsSplit,
) -> Vec<Slide> {
    let mut slides: Vec<Slide> = vec![];
    for section in song.ordered_sections(arrangement) {
        let raw: Vec<&str> = section.lyrics.lines().collect();
        let trans: Vec<&str> = section
            .translation
            .as_deref()
            .map(|t| t.lines().collect())
            .unwrap_or_default();
        for chunk in chunk_line_indices(&raw, cfg) {
            let mut slide = Slide::blank(format!("{}:{}", section.id, slides.len()));
            slide.kind = SlideKind::Lyrics;
            slide.label = Some(section.label.clone());
            slide.lines = chunk.iter().map(|&i| strip_chords(raw[i])).collect();
            slide.chart = chunk.iter().map(|&i| raw[i].to_owned()).collect();
            if !trans.is_empty() {
                slide.translation = chunk
                    .iter()
                    .map(|&i| trans.get(i).copied().unwrap_or("").to_owned())
                    .collect();
            }
            slide.theme_id = song.theme_id.clone();
            slide.size_group = cfg.uniform_size.then(|| format!("song:{}", song.id));
            slides.push(slide);
        }
    }
    slides
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SplitError {
    #[error("at least one Bible version is required")]
    NoVersions,
    #[error("at most {MAX_PARALLEL_VERSIONS} versions can be shown in parallel (got {0})")]
    TooManyVersions(usize),
    #[error("the same version `{0}` was requested twice")]
    DuplicateVersion(String),
}

/// One version's text of the passage plus the metadata a column needs.
#[derive(Debug, Clone)]
pub struct VersionVerses {
    pub version: VersionCode,
    pub copyright: Option<String>,
    pub verses: Vec<Verse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BibleSplit {
    /// Budget of characters per column per slide. The worst column decides, so every version
    /// fits; a single verse longer than the budget still gets its own slide (never cut mid-verse,
    /// because that cannot stay in sync across languages).
    pub max_chars: usize,
    pub layout: ParallelLayout,
    pub uniform_size: bool,
}

impl Default for BibleSplit {
    fn default() -> Self {
        Self {
            max_chars: 260,
            layout: ParallelLayout::Columns,
            uniform_size: true,
        }
    }
}

/// Cuts `passage` into slides that cover the same verses in every version.
///
/// A verse absent from some version (different versification) is shown as a dash in that column,
/// so the columns never drift apart.
pub fn split_passage(
    passage: &Passage,
    versions: &[VersionVerses],
    cfg: BibleSplit,
) -> Result<Vec<Slide>, SplitError> {
    if versions.is_empty() {
        return Err(SplitError::NoVersions);
    }
    if versions.len() > MAX_PARALLEL_VERSIONS {
        return Err(SplitError::TooManyVersions(versions.len()));
    }
    for (i, v) in versions.iter().enumerate() {
        if versions[..i].iter().any(|w| w.version == v.version) {
            return Err(SplitError::DuplicateVersion(v.version.0.clone()));
        }
    }

    let keys = ordered_union(versions);
    let lookup: Vec<HashMap<&BibleRef, &str>> = versions
        .iter()
        .map(|v| {
            v.verses
                .iter()
                .map(|x| (&x.reference, x.text.as_str()))
                .collect()
        })
        .collect();
    let group_id = format!("bible:{}", passage.to_osis());
    let make_slide = |range: &[&BibleRef], n: usize| -> Slide {
        let columns: Vec<BibleColumn> = versions
            .iter()
            .zip(&lookup)
            .map(|(v, l)| BibleColumn {
                version: v.version.clone(),
                reference: reference_label(range),
                text: column_text(range, l),
                copyright: v.copyright.clone().filter(|c| !c.is_empty()),
            })
            .collect();
        let label = columns[0].reference.clone();
        let mut slide = Slide::blank(format!("{group_id}#{n}"));
        slide.kind = SlideKind::Bible;
        slide.label = Some(label);
        slide.layout = cfg.layout;
        slide.bible = columns;
        slide.size_group = cfg.uniform_size.then(|| group_id.clone());
        slide
    };

    let mut slides = vec![];
    let mut start = 0;
    let mut used = vec![0usize; versions.len()];
    for (i, key) in keys.iter().enumerate() {
        let costs: Vec<usize> = lookup
            .iter()
            .map(|l| l.get(key).map_or(MISSING.len(), |t| t.chars().count()) + 4)
            .collect();
        let overflow = used
            .iter()
            .zip(&costs)
            .any(|(u, c)| u + c > cfg.max_chars.max(1));
        if i > start && overflow {
            slides.push(make_slide(&keys[start..i], slides.len()));
            start = i;
            used.iter_mut().for_each(|u| *u = 0);
        }
        used.iter_mut().zip(&costs).for_each(|(u, c)| *u += c);
    }
    if start < keys.len() {
        slides.push(make_slide(&keys[start..], slides.len()));
    }
    if slides.is_empty() {
        // Nothing found in any version: still produce one slide so the operator sees the dashes.
        slides.push(make_slide(&[], 0));
        for c in &mut slides[0].bible {
            c.reference = passage.to_osis();
        }
        slides[0].label = Some(passage.to_osis());
    }
    Ok(slides)
}

/// Verse references of all versions in reading order: books by first appearance, then chapter
/// and verse. Chapter-only references sort before verse 1.
fn ordered_union(versions: &[VersionVerses]) -> Vec<&BibleRef> {
    let mut book_order: HashMap<&str, usize> = HashMap::new();
    let mut keys: Vec<&BibleRef> = vec![];
    let mut seen = std::collections::HashSet::new();
    for v in versions {
        for verse in &v.verses {
            let next = book_order.len();
            book_order
                .entry(verse.reference.book.0.as_str())
                .or_insert(next);
            if seen.insert(&verse.reference) {
                keys.push(&verse.reference);
            }
        }
    }
    keys.sort_by_key(|r| {
        (
            book_order[r.book.0.as_str()],
            r.chapter,
            r.verse.unwrap_or(0),
        )
    });
    keys
}

/// `John 3:16`, `John 3:16-18`, `John 3:16-4:2`. Book names are the OSIS ids; the bible crate
/// localises them when composing the final footer.
fn reference_label(range: &[&BibleRef]) -> String {
    let (Some(a), Some(b)) = (range.first(), range.last()) else {
        return MISSING.to_owned();
    };
    let v = |r: &BibleRef| r.verse.map(|v| v.to_string());
    let head = match v(a) {
        Some(n) => format!("{} {}:{}", a.book.0, a.chapter, n),
        None => format!("{} {}", a.book.0, a.chapter),
    };
    if a == b {
        return head;
    }
    match (v(b), a.book == b.book, a.chapter == b.chapter) {
        (Some(n), true, true) => format!("{head}-{n}"),
        (Some(n), true, false) => format!("{head}-{}:{n}", b.chapter),
        (Some(n), false, _) => format!("{head}-{} {}:{n}", b.book.0, b.chapter),
        (None, _, _) => head,
    }
}

fn column_text(range: &[&BibleRef], lookup: &HashMap<&BibleRef, &str>) -> String {
    if range.is_empty() {
        return MISSING.to_owned();
    }
    let numbered = range.len() > 1;
    range
        .iter()
        .map(|r| {
            let text = lookup.get(*r).copied().unwrap_or(MISSING);
            match r.verse {
                Some(n) if numbered => format!("{n} {text}"),
                _ => text.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convenience used by tests and the app layer: ids of a slide list.
pub fn slide_ids(slides: &[Slide]) -> Vec<&SlideId> {
    slides.iter().map(|s| &s.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SectionId, SongId};
    use crate::song::{Arrangement, Section, SectionKind};

    fn song(sections: &[(&str, &str)], seq: &[&str]) -> Song {
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
            sections: sections
                .iter()
                .map(|(id, lyrics)| Section {
                    id: SectionId::new(*id),
                    kind: SectionKind::Verse,
                    label: (*id).into(),
                    lyrics: (*lyrics).into(),
                    translation: None,
                })
                .collect(),
            arrangements: if seq.is_empty() {
                vec![]
            } else {
                vec![Arrangement {
                    id: ArrangementId::new("a"),
                    name: "A".into(),
                    sequence: seq.iter().map(|s| SectionId::new(*s)).collect(),
                }]
            },
            created_at: 0,
            updated_at: 0,
        }
    }

    fn cfg(max: usize) -> LyricsSplit {
        LyricsSplit {
            max_lines: max,
            ..LyricsSplit::default()
        }
    }

    #[test]
    fn group_sizes_balanced_and_greedy() {
        assert_eq!(group_sizes(5, 4, true), [3, 2]);
        assert_eq!(group_sizes(5, 4, false), [4, 1]);
        assert_eq!(group_sizes(8, 4, true), [4, 4]);
        assert_eq!(group_sizes(1, 4, true), [1]);
        assert_eq!(group_sizes(0, 4, true), Vec::<usize>::new());
        assert_eq!(group_sizes(3, 0, true), [1, 1, 1], "max 0 behaves as 1");
    }

    #[test]
    fn balanced_split_avoids_orphan_line() {
        let s = song(&[("v", "1\n2\n3\n4\n5")], &[]);
        let slides = split_song(&s, None, cfg(4));
        assert_eq!(slides[0].lines, ["1", "2", "3"]);
        assert_eq!(slides[1].lines, ["4", "5"]);
    }

    #[test]
    fn greedy_when_balance_is_off() {
        let s = song(&[("v", "1\n2\n3\n4\n5")], &[]);
        let c = LyricsSplit {
            balance: false,
            ..cfg(4)
        };
        let slides = split_song(&s, None, c);
        assert_eq!(slides[0].lines.len(), 4);
        assert_eq!(slides[1].lines, ["5"]);
    }

    #[test]
    fn manual_and_blank_breaks_start_new_slides() {
        let s = song(&[("v", "a\nb\n---\nc\n\nd\n  \n---\n---\ne")], &[]);
        let slides = split_song(&s, None, cfg(4));
        let lines: Vec<_> = slides.iter().map(|s| s.lines.clone()).collect();
        assert_eq!(lines, [vec!["a", "b"], vec!["c"], vec!["d"], vec!["e"]]);
    }

    #[test]
    fn breaks_ignored_when_not_honoured() {
        let s = song(&[("v", "a\n---\nb")], &[]);
        let c = LyricsSplit {
            honor_manual_breaks: false,
            ..cfg(4)
        };
        let slides = split_song(&s, None, c);
        assert_eq!(slides.len(), 1);
        assert_eq!(slides[0].lines, ["a", "---", "b"]);
    }

    #[test]
    fn follows_arrangement_with_unique_ids_and_shared_size_group() {
        let s = song(
            &[("v1", "uno\ndos"), ("c", "[G]coro")],
            &["v1", "c", "v1", "c"],
        );
        let slides = split_song(&s, None, cfg(4));
        assert_eq!(slides.len(), 4);
        assert_eq!(slides[1].lines, ["coro"]);
        assert_eq!(slides[1].chart, ["[G]coro"]);
        let ids: std::collections::HashSet<_> = slide_ids(&slides).into_iter().collect();
        assert_eq!(ids.len(), 4);
        assert!(slides
            .iter()
            .all(|s| s.size_group.as_deref() == Some("song:s")));
        let c = LyricsSplit {
            uniform_size: false,
            ..cfg(4)
        };
        assert!(split_song(&s, None, c)[0].size_group.is_none());
    }

    #[test]
    fn unknown_arrangement_falls_back_and_empty_section_yields_nothing() {
        let s = song(&[("v", "  \n"), ("c", "x")], &[]);
        let slides = split_song(&s, Some(&ArrangementId::new("zzz")), cfg(2));
        assert_eq!(slides.len(), 1);
        assert_eq!(slides[0].lines, ["x"]);
    }

    #[test]
    fn translation_stays_aligned_and_is_padded() {
        let mut s = song(&[("v", "a\nb\nc")], &[]);
        s.sections[0].translation = Some("A\nB".into());
        let slides = split_song(&s, None, cfg(2));
        assert_eq!(slides[0].translation, ["A", "B"]);
        assert_eq!(slides[1].translation, [""]);
    }

    // ---- Bible ----

    fn verses(chapter: u16, texts: &[(u16, &str)]) -> Vec<Verse> {
        texts
            .iter()
            .map(|(n, t)| Verse {
                reference: BibleRef::verse("John", chapter, *n),
                text: (*t).into(),
                marks: vec![],
            })
            .collect()
    }

    fn vv(code: &str, v: Vec<Verse>) -> VersionVerses {
        VersionVerses {
            version: VersionCode(code.into()),
            copyright: Some(format!("{code} PD")),
            verses: v,
        }
    }

    fn passage(s: &str) -> Passage {
        Passage::parse_osis(s).unwrap()
    }

    #[test]
    fn single_version_short_passage_is_one_slide() {
        let v = vv("A", verses(3, &[(16, "uno"), (17, "dos")]));
        let slides = split_passage(&passage("John.3.16-17"), &[v], BibleSplit::default()).unwrap();
        assert_eq!(slides.len(), 1);
        assert_eq!(slides[0].bible[0].text, "16 uno 17 dos");
        assert_eq!(slides[0].bible[0].reference, "John 3:16-17");
        assert_eq!(slides[0].label.as_deref(), Some("John 3:16-17"));
        assert_eq!(slides[0].bible[0].copyright.as_deref(), Some("A PD"));
    }

    #[test]
    fn single_verse_is_not_numbered() {
        let v = vv("A", verses(3, &[(16, "uno")]));
        let s = split_passage(&passage("John.3.16"), &[v], BibleSplit::default()).unwrap();
        assert_eq!(s[0].bible[0].text, "uno");
        assert_eq!(s[0].bible[0].reference, "John 3:16");
    }

    #[test]
    fn versions_are_cut_at_the_same_verses_driven_by_the_longest() {
        let long = "x".repeat(100);
        let short = "y";
        let a = vv(
            "A",
            verses(3, &[(16, short), (17, short), (18, short), (19, short)]),
        );
        let b = vv(
            "B",
            verses(3, &[(16, &long), (17, &long), (18, &long), (19, &long)]),
        );
        let cfg = BibleSplit {
            max_chars: 230,
            ..BibleSplit::default()
        };
        let slides = split_passage(&passage("John.3.16-19"), &[a, b], cfg).unwrap();
        assert_eq!(slides.len(), 2);
        for s in &slides {
            assert_eq!(
                s.bible[0].reference, s.bible[1].reference,
                "same verse range"
            );
        }
        assert_eq!(slides[0].bible[0].reference, "John 3:16-17");
        assert_eq!(slides[1].bible[0].reference, "John 3:18-19");
    }

    #[test]
    fn oversized_verse_gets_its_own_slide_and_nothing_is_lost() {
        let big = "z".repeat(500);
        let a = vv("A", verses(3, &[(1, "a"), (2, &big), (3, "c")]));
        let slides = split_passage(&passage("John.3.1-3"), &[a], BibleSplit::default()).unwrap();
        let refs: Vec<_> = slides
            .iter()
            .map(|s| s.bible[0].reference.as_str())
            .collect();
        assert_eq!(refs, ["John 3:1", "John 3:2", "John 3:3"]);
    }

    #[test]
    fn missing_verse_is_a_dash_in_that_column() {
        let a = vv("A", verses(3, &[(16, "uno"), (17, "dos")]));
        let b = vv("B", verses(3, &[(16, "one")]));
        let s = split_passage(&passage("John.3.16-17"), &[a, b], BibleSplit::default()).unwrap();
        assert_eq!(s[0].bible[1].text, "16 one 17 \u{2014}");
    }

    #[test]
    fn version_count_is_validated() {
        let v = |c: &str| vv(c, verses(3, &[(16, "t")]));
        let p = passage("John.3.16");
        assert_eq!(
            split_passage(&p, &[], BibleSplit::default()).unwrap_err(),
            SplitError::NoVersions
        );
        let five: Vec<_> = ["A", "B", "C", "D", "E"].map(v).into();
        assert_eq!(
            split_passage(&p, &five, BibleSplit::default()).unwrap_err(),
            SplitError::TooManyVersions(5)
        );
        let four: Vec<_> = ["A", "B", "C", "D"].map(v).into();
        assert_eq!(
            split_passage(&p, &four, BibleSplit::default()).unwrap()[0]
                .bible
                .len(),
            4
        );
        assert_eq!(
            split_passage(&p, &[v("A"), v("A")], BibleSplit::default()).unwrap_err(),
            SplitError::DuplicateVersion("A".into())
        );
    }

    #[test]
    fn crossing_chapters_orders_and_labels_correctly() {
        let mut vs = verses(3, &[(35, "a"), (36, "b")]);
        vs.extend(verses(4, &[(1, "c")]));
        // Supplied out of order on purpose.
        vs.reverse();
        let s = split_passage(
            &passage("John.3.35-John.4.1"),
            &[vv("A", vs)],
            BibleSplit::default(),
        )
        .unwrap();
        assert_eq!(s[0].bible[0].reference, "John 3:35-4:1");
        assert_eq!(s[0].bible[0].text, "35 a 36 b 1 c");
    }

    #[test]
    fn empty_result_still_yields_one_slide_with_dashes() {
        let s = split_passage(
            &passage("John.3.16"),
            &[vv("A", vec![]), vv("B", vec![])],
            BibleSplit::default(),
        )
        .unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].bible[0].text, "\u{2014}");
        assert_eq!(s[0].label.as_deref(), Some("John.3.16"));
    }

    #[test]
    fn slide_ids_are_unique_and_size_group_shared() {
        let t: Vec<(u16, String)> = (1..=10).map(|n| (n, "w".repeat(80))).collect();
        let t: Vec<(u16, &str)> = t.iter().map(|(n, s)| (*n, s.as_str())).collect();
        let a = vv("A", verses(1, &t));
        let slides = split_passage(&passage("John.1.1-10"), &[a], BibleSplit::default()).unwrap();
        assert!(slides.len() > 2);
        let ids: std::collections::HashSet<_> = slide_ids(&slides).into_iter().collect();
        assert_eq!(ids.len(), slides.len());
        assert!(slides
            .iter()
            .all(|s| s.size_group.as_deref() == Some("bible:John.1.1-John.1.10")));
    }
}
