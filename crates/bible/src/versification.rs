//! Versification mapping and parallel alignment (PLAN §2.4).
//!
//! Every reference travelling through the app (playlists, commands, slides) is expressed in the
//! **canonical** scheme, the English/KJV one used by OSIS and by almost every Spanish Protestant
//! Bible (Reina-Valera included). Versions that number differently declare a scheme and are
//! translated at the edge:
//!
//! * [`Versification::Hebrew`] (Masoretic text, used by Jewish and many modern critical editions):
//!   Joel 2:28-32 is 3:1-5 and Joel 3 is Joel 4, Malachi 4:1-6 is 3:19-24, Jonah 1:17 is 2:1,
//!   plus the chapter-boundary shifts in Genesis, Exodus, Numbers, Samuel, Kings, Chronicles,
//!   Nehemiah, Job, Isaiah, Daniel, Hosea, Micah, Nahum, Zechariah... Psalm *titles* (numbered as
//!   verses in the Hebrew) are **not** modelled: a Hebrew-numbered Psalter needs a per-psalm table.
//! * [`Versification::Vulgate`] (Septuagint/Vulgate Psalter, Douay-Rheims and most Catholic
//!   editions): Psalms 10-147 are numbered one lower and four psalms are split differently
//!   (Heb. 9-10 = Vulg. 9, Heb. 114-115 = Vulg. 113, Heb. 116 = Vulg. 114-115, Heb. 147 =
//!   Vulg. 146-147). Verse numbers inside a psalm are kept; editions that number the title as
//!   verse 1 are off by one inside the psalm (approximate by design).
//!
//! A verse that does not exist in a version (Acts 8:37 in critical texts, see
//! [`CRITICAL_TEXT_OMISSIONS`]) is simply absent: [`align_parallel`] yields `None` for it so the
//! UI shows a marker instead of failing.

use crate::books::{book_ord, verse_count};
use hollyshow_core::{BibleRef, Passage, Verse, VersionCode};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Versification {
    /// English/KJV numbering. The canonical scheme: mapping is the identity.
    #[default]
    Kjv,
    Vulgate,
    Hebrew,
}

impl Versification {
    /// Parses the free-form `BibleVersion::versification` string. Unknown values mean
    /// canonical numbering.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "vulgate" | "vulg" | "lxx" | "septuagint" | "catholic" | "douay" => Self::Vulgate,
            "hebrew" | "mt" | "masoretic" | "leningrad" | "original" | "nrsv-hebrew" => {
                Self::Hebrew
            }
            _ => Self::Kjv,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kjv => "kjv",
            Self::Vulgate => "vulgate",
            Self::Hebrew => "hebrew",
        }
    }

    fn rules(self) -> &'static [Rule] {
        static HEBREW: OnceLock<Vec<Rule>> = OnceLock::new();
        static VULGATE: OnceLock<Vec<Rule>> = OnceLock::new();
        match self {
            Self::Kjv => &[],
            Self::Hebrew => HEBREW.get_or_init(hebrew_rules),
            Self::Vulgate => VULGATE.get_or_init(vulgate_rules),
        }
    }

    /// Canonical reference to this scheme's numbering. A whole-chapter reference follows the
    /// rule that covers the chapter's first verse.
    pub fn from_canonical(self, r: &BibleRef) -> BibleRef {
        let probe = r.verse.unwrap_or(1);
        for rule in self.rules() {
            if rule.book == r.book.0
                && rule.ch == r.chapter
                && (rule.v_lo..=rule.v_hi).contains(&probe)
            {
                return BibleRef {
                    book: r.book.clone(),
                    chapter: rule.to_ch,
                    verse: r.verse.map(|v| shift(v, rule.off)),
                };
            }
        }
        r.clone()
    }

    /// This scheme's reference to the canonical numbering (inverse of [`Self::from_canonical`]).
    pub fn to_canonical(self, r: &BibleRef) -> BibleRef {
        let probe = r.verse.unwrap_or(1);
        for rule in self.rules() {
            if rule.book == r.book.0
                && rule.to_ch == r.chapter
                && (shift(rule.v_lo, rule.off)..=shift(rule.v_hi, rule.off)).contains(&probe)
            {
                return BibleRef {
                    book: r.book.clone(),
                    chapter: rule.ch,
                    verse: r.verse.map(|v| shift(v, -rule.off)),
                };
            }
        }
        r.clone()
    }

    /// Translates a canonical passage into the passages to ask this version's store for. The
    /// result has several entries when the passage straddles a renumbering boundary (Psalm 115 in
    /// a Vulgate Psalter is half of Psalm 113). Passages untouched by any rule come back as-is.
    pub fn native_passages(self, p: &Passage) -> Vec<Passage> {
        let affected = self.rules().iter().any(|r| {
            r.book == p.start.book.0
                && r.ch >= p.start.chapter
                && r.ch <= p.end.chapter.max(p.start.chapter)
        });
        if !affected || p.start.book != p.end.book {
            return vec![p.clone()];
        }
        let osis = p.start.book.0.as_str();
        let mut refs: Vec<BibleRef> = Vec::new();
        for ch in p.start.chapter..=p.end.chapter {
            let count = match verse_count(osis, ch) {
                Some(c) => c,
                None => return vec![p.clone()],
            };
            let lo = if ch == p.start.chapter {
                p.start.verse.unwrap_or(1)
            } else {
                1
            };
            let hi = if ch == p.end.chapter {
                p.end.verse.unwrap_or(count)
            } else {
                count
            };
            for v in lo..=hi {
                refs.push(self.from_canonical(&BibleRef::verse(osis, ch, v)));
            }
        }
        coalesce(&refs)
    }

    /// Re-keys verses read from a version using this scheme to canonical references, in
    /// canonical order, so they line up with other versions.
    pub fn canonicalise(self, verses: Vec<Verse>) -> Vec<Verse> {
        if self == Self::Kjv {
            return verses;
        }
        let mut out: Vec<Verse> = verses
            .into_iter()
            .map(|mut v| {
                v.reference = self.to_canonical(&v.reference);
                v
            })
            .collect();
        out.sort_by_key(|v| sort_key(&v.reference));
        out
    }
}

/// Reads the verses of a canonical `passage` from a version numbered with `scheme`, returning
/// them keyed canonically. `fetch` is called once per native passage (typically
/// `|p| repo.passage(&code, p)`).
pub fn load_canonical<E>(
    scheme: Versification,
    passage: &Passage,
    mut fetch: impl FnMut(&Passage) -> Result<Vec<Verse>, E>,
) -> Result<Vec<Verse>, E> {
    let mut all = Vec::new();
    for native in scheme.native_passages(passage) {
        all.extend(fetch(&native)?);
    }
    Ok(scheme.canonicalise(all))
}

fn shift(v: u16, off: i32) -> u16 {
    u16::try_from((i32::from(v) + off).max(1)).unwrap_or(u16::MAX)
}

fn sort_key(r: &BibleRef) -> (usize, u16, u16) {
    (
        book_ord(&r.book).unwrap_or(usize::MAX),
        r.chapter,
        r.verse.unwrap_or(0),
    )
}

/// Groups consecutive verses of one chapter into ranges.
fn coalesce(refs: &[BibleRef]) -> Vec<Passage> {
    let mut out: Vec<Passage> = Vec::new();
    for r in refs {
        if let Some(last) = out.last_mut() {
            if last.end.chapter == r.chapter
                && last.end.book == r.book
                && matches!((last.end.verse, r.verse), (Some(a), Some(b)) if b == a + 1)
            {
                last.end = r.clone();
                continue;
            }
        }
        out.push(Passage::single(r.clone()));
    }
    out
}

/// Canonical `(book, chapter, verse range)` -> native chapter and verse offset.
#[derive(Debug, Clone)]
struct Rule {
    book: &'static str,
    ch: u16,
    v_lo: u16,
    v_hi: u16,
    to_ch: u16,
    off: i32,
}

const ALL: u16 = 999;

fn rule(book: &'static str, ch: u16, v_lo: u16, v_hi: u16, to_ch: u16, off: i32) -> Rule {
    Rule {
        book,
        ch,
        v_lo,
        v_hi,
        to_ch,
        off,
    }
}

/// The chapter `ch` shifts as a whole by `off` verses into the same chapter (the tail of the
/// previous chapter moved into it): verses `1..=ALL` of `ch` -> `v + off`.
fn whole(book: &'static str, ch: u16, off: i32) -> Rule {
    rule(book, ch, 1, ALL, ch, off)
}

fn hebrew_rules() -> Vec<Rule> {
    vec![
        // Genesis 31:55 is 32:1 in the Hebrew.
        rule("Gen", 31, 55, 55, 32, -54),
        whole("Gen", 32, 1),
        // Exodus 8:1-4 is 7:26-29, Exodus 22:1 is 21:37.
        rule("Exod", 8, 1, 4, 7, 25),
        rule("Exod", 8, 5, ALL, 8, -4),
        rule("Exod", 22, 1, 1, 21, 36),
        rule("Exod", 22, 2, ALL, 22, -1),
        // Leviticus 6:1-7 is 5:20-26.
        rule("Lev", 6, 1, 7, 5, 19),
        rule("Lev", 6, 8, ALL, 6, -7),
        // Numbers 16:36-50 is 17:1-15; 29:40 is 30:1.
        rule("Num", 16, 36, 50, 17, -35),
        rule("Num", 17, 1, ALL, 17, 15),
        rule("Num", 29, 40, 40, 30, -39),
        whole("Num", 30, 1),
        // Deuteronomy 12:32 is 13:1 (13:1-18 is 13:2-19); 22:30 is 23:1 (23:1-25 is 23:2-26);
        // 29:1 is 28:69.
        rule("Deut", 12, 32, 32, 13, -31),
        whole("Deut", 13, 1),
        rule("Deut", 22, 30, 30, 23, -29),
        whole("Deut", 23, 1),
        rule("Deut", 29, 1, 1, 28, 68),
        rule("Deut", 29, 2, ALL, 29, -1),
        // 1 Samuel 20:42-... 21 begins one verse earlier; 23:29 is 24:1.
        whole("1Sam", 21, 1),
        rule("1Sam", 23, 29, 29, 24, -28),
        whole("1Sam", 24, 1),
        // 2 Samuel 18:33 is 19:1.
        rule("2Sam", 18, 33, 33, 19, -32),
        whole("2Sam", 19, 1),
        // 1 Kings 4:21-34 is 5:1-14, 5:1-18 is 5:15-32.
        rule("1Kgs", 4, 21, 34, 5, -20),
        rule("1Kgs", 5, 1, ALL, 5, 14),
        // 2 Kings 11:21 is 12:1.
        rule("2Kgs", 11, 21, 21, 12, -20),
        whole("2Kgs", 12, 1),
        // 1 Chronicles 6:1-15 is 5:27-41; 6:16-81 is 6:1-66.
        rule("1Chr", 6, 1, 15, 5, 26),
        rule("1Chr", 6, 16, ALL, 6, -15),
        // Nehemiah 4:1-6 is 3:33-38; 4:7-23 is 4:1-17; 9:38 is 10:1.
        rule("Neh", 4, 1, 6, 3, 32),
        rule("Neh", 4, 7, ALL, 4, -6),
        rule("Neh", 9, 38, 38, 10, -37),
        whole("Neh", 10, 1),
        // Job 41:1-8 is 40:25-32; 41:9-34 is 41:1-26.
        rule("Job", 41, 1, 8, 40, 24),
        rule("Job", 41, 9, ALL, 41, -8),
        // Song 6:13 is 7:1.
        rule("Song", 6, 13, 13, 7, -12),
        whole("Song", 7, 1),
        // Isaiah 9:1 is 8:23; 9:2-21 is 9:1-20. (Isaiah 63:19b/64:1 is one verse split in two in
        // the English and is deliberately not modelled: the mapping must stay one-to-one.)
        rule("Isa", 9, 1, 1, 8, 22),
        rule("Isa", 9, 2, ALL, 9, -1),
        // Daniel 4:1-3 is 3:31-33; 4:4-37 is 4:1-34; 5:31 is 6:1; 6:1-28 is 6:2-29.
        rule("Dan", 4, 1, 3, 3, 30),
        rule("Dan", 4, 4, ALL, 4, -3),
        rule("Dan", 5, 31, 31, 6, -30),
        whole("Dan", 6, 1),
        // Hosea 1:10-11 is 2:1-2; 2:1-23 is 2:3-25; 11:12 is 12:1; 13:16 is 14:1.
        rule("Hos", 1, 10, 11, 2, -9),
        rule("Hos", 2, 1, ALL, 2, 2),
        rule("Hos", 11, 12, 12, 12, -11),
        whole("Hos", 12, 1),
        rule("Hos", 13, 16, 16, 14, -15),
        whole("Hos", 14, 1),
        // Joel 2:28-32 is 3:1-5; Joel 3 is Joel 4.
        rule("Joel", 2, 28, 32, 3, -27),
        rule("Joel", 3, 1, ALL, 4, 0),
        // Jonah 1:17 is 2:1; 2:1-10 is 2:2-11.
        rule("Jonah", 1, 17, 17, 2, -16),
        whole("Jonah", 2, 1),
        // Micah 5:1 is 4:14; 5:2-15 is 5:1-14.
        rule("Mic", 5, 1, 1, 4, 13),
        rule("Mic", 5, 2, ALL, 5, -1),
        // Nahum 1:15 is 2:1; 2:1-13 is 2:2-14.
        rule("Nah", 1, 15, 15, 2, -14),
        whole("Nah", 2, 1),
        // Zechariah 1:18-21 is 2:1-4; 2:1-13 is 2:5-17.
        rule("Zech", 1, 18, 21, 2, -17),
        rule("Zech", 2, 1, ALL, 2, 4),
        // Malachi 4:1-6 is 3:19-24.
        rule("Mal", 4, 1, 6, 3, 18),
    ]
}

fn vulgate_rules() -> Vec<Rule> {
    let mut r = vec![
        // Heb. 10 continues Vulg. 9 after Heb. 9's 20 verses.
        rule("Ps", 10, 1, 18, 9, 20),
    ];
    // Heb. 11-113 are Vulg. 10-112.
    r.extend((11..=113).map(|n| rule("Ps", n, 1, ALL, n - 1, 0)));
    // Heb. 114 and 115 together are Vulg. 113 (114 has 8 verses).
    r.push(rule("Ps", 114, 1, 8, 113, 0));
    r.push(rule("Ps", 115, 1, 18, 113, 8));
    // Heb. 116 is split into Vulg. 114 (v. 1-9) and 115 (v. 10-19).
    r.push(rule("Ps", 116, 1, 9, 114, 0));
    r.push(rule("Ps", 116, 10, ALL, 115, -9));
    // Heb. 117-146 are Vulg. 116-145.
    r.extend((117..=146).map(|n| rule("Ps", n, 1, ALL, n - 1, 0)));
    // Heb. 147 is Vulg. 146 (v. 1-11) and 147 (v. 12-20).
    r.push(rule("Ps", 147, 1, 11, 146, 0));
    r.push(rule("Ps", 147, 12, ALL, 147, -11));
    r
}

/// Verses that critical-text editions (and most modern versions) leave out or relocate to a
/// footnote, in canonical numbering. They are legitimately missing from such a version.
pub const CRITICAL_TEXT_OMISSIONS: &[(&str, u16, u16)] = &[
    ("Matt", 17, 21),
    ("Matt", 18, 11),
    ("Matt", 23, 14),
    ("Mark", 7, 16),
    ("Mark", 9, 44),
    ("Mark", 9, 46),
    ("Mark", 11, 26),
    ("Mark", 15, 28),
    ("Luke", 17, 36),
    ("Luke", 23, 17),
    ("John", 5, 4),
    ("Acts", 8, 37),
    ("Acts", 15, 34),
    ("Acts", 24, 7),
    ("Acts", 28, 29),
    ("Rom", 16, 24),
];

/// True when `r` is a verse that critical editions commonly omit (Acts 8:37...).
pub fn is_commonly_omitted(r: &BibleRef) -> bool {
    r.verse.is_some_and(|v| {
        CRITICAL_TEXT_OMISSIONS
            .iter()
            .any(|(b, c, vv)| *b == r.book.0 && *c == r.chapter && *vv == v)
    })
}

/// One row of a parallel display: a canonical reference and, per column, the verse text (or
/// `None` when that version lacks the verse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedRow {
    pub reference: BibleRef,
    pub cells: Vec<Option<String>>,
}

/// Aligns canonical-keyed verses of several versions (see [`Versification::canonicalise`]) into
/// rows in canonical order. The union of references is used, so a verse present in only some
/// versions still appears with `None` cells for the others.
pub fn align_parallel(columns: &[(VersionCode, Vec<Verse>)]) -> Vec<AlignedRow> {
    use std::collections::BTreeMap;
    let mut rows: BTreeMap<(usize, u16, u16), AlignedRow> = BTreeMap::new();
    for (i, (_, verses)) in columns.iter().enumerate() {
        for v in verses {
            let row = rows
                .entry(sort_key(&v.reference))
                .or_insert_with(|| AlignedRow {
                    reference: v.reference.clone(),
                    cells: vec![None; columns.len()],
                });
            row.cells[i] = Some(v.text.clone());
        }
    }
    rows.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(osis: &str) -> BibleRef {
        osis.parse().unwrap()
    }

    #[test]
    fn kjv_is_identity() {
        let x = r("John.3.16");
        assert_eq!(Versification::Kjv.from_canonical(&x), x);
        assert_eq!(Versification::Kjv.to_canonical(&x), x);
        assert_eq!(Versification::parse("whatever"), Versification::Kjv);
        assert_eq!(Versification::parse("Vulgate"), Versification::Vulgate);
        assert_eq!(Versification::parse("MT"), Versification::Hebrew);
    }

    #[test]
    fn malachi_and_joel() {
        let h = Versification::Hebrew;
        assert_eq!(h.from_canonical(&r("Mal.4.1")), r("Mal.3.19"));
        assert_eq!(h.from_canonical(&r("Mal.4.6")), r("Mal.3.24"));
        assert_eq!(h.to_canonical(&r("Mal.3.19")), r("Mal.4.1"));
        assert_eq!(h.to_canonical(&r("Mal.3.18")), r("Mal.3.18"));
        assert_eq!(h.from_canonical(&r("Joel.2.28")), r("Joel.3.1"));
        assert_eq!(h.from_canonical(&r("Joel.2.32")), r("Joel.3.5"));
        assert_eq!(h.from_canonical(&r("Joel.2.27")), r("Joel.2.27"));
        assert_eq!(h.from_canonical(&r("Joel.3.1")), r("Joel.4.1"));
        assert_eq!(h.to_canonical(&r("Joel.4.21")), r("Joel.3.21"));
        assert_eq!(h.to_canonical(&r("Joel.3.5")), r("Joel.2.32"));
        assert_eq!(h.from_canonical(&r("Jonah.1.17")), r("Jonah.2.1"));
        assert_eq!(h.from_canonical(&r("Jonah.2.1")), r("Jonah.2.2"));
        assert_eq!(h.to_canonical(&r("Jonah.2.11")), r("Jonah.2.10"));
    }

    #[test]
    fn vulgate_psalms() {
        let v = Versification::Vulgate;
        // The Shepherd Psalm: 23 in Hebrew/English, 22 in Vulgate/Douay.
        assert_eq!(v.from_canonical(&r("Ps.23.1")), r("Ps.22.1"));
        assert_eq!(v.to_canonical(&r("Ps.22.1")), r("Ps.23.1"));
        assert_eq!(v.from_canonical(&r("Ps.8.1")), r("Ps.8.1"));
        assert_eq!(v.from_canonical(&r("Ps.10.1")), r("Ps.9.21"));
        assert_eq!(v.to_canonical(&r("Ps.9.21")), r("Ps.10.1"));
        assert_eq!(v.from_canonical(&r("Ps.51.1")), r("Ps.50.1"));
        assert_eq!(v.from_canonical(&r("Ps.115.1")), r("Ps.113.9"));
        assert_eq!(v.from_canonical(&r("Ps.116.10")), r("Ps.115.1"));
        assert_eq!(v.to_canonical(&r("Ps.115.1")), r("Ps.116.10"));
        assert_eq!(v.from_canonical(&r("Ps.147.12")), r("Ps.147.1"));
        assert_eq!(v.to_canonical(&r("Ps.146.1")), r("Ps.147.1"));
        assert_eq!(v.from_canonical(&r("Ps.150.1")), r("Ps.150.1"));
        assert_eq!(v.from_canonical(&r("Ps.119.105")), r("Ps.118.105"));
    }

    #[test]
    fn every_rule_round_trips_and_is_a_bijection() {
        for scheme in [Versification::Hebrew, Versification::Vulgate] {
            let mut seen = std::collections::HashSet::new();
            for b in &crate::books::BOOKS[..crate::books::PROTESTANT_BOOKS] {
                for ch in 1..=u16::from(b.chapters) {
                    let n = verse_count(b.osis, ch).unwrap();
                    for v in 1..=n {
                        let c = BibleRef::verse(b.osis, ch, v);
                        let native = scheme.from_canonical(&c);
                        assert_eq!(scheme.to_canonical(&native), c, "{scheme:?} {c}");
                        assert!(seen.insert(native.clone()), "{scheme:?}: {native} collides");
                    }
                }
            }
        }
    }

    #[test]
    fn passages_straddling_a_boundary_split() {
        let p = Passage::parse_osis("Ps.115.1-Ps.116.12").unwrap();
        let native: Vec<String> = Versification::Vulgate
            .native_passages(&p)
            .iter()
            .map(Passage::to_osis)
            .collect();
        assert_eq!(
            native,
            [
                "Ps.113.9-Ps.113.26",
                "Ps.114.1-Ps.114.9",
                "Ps.115.1-Ps.115.3"
            ]
        );
        // Chapter-only canonical references expand with the KJV verse counts.
        let p = Passage::parse_osis("Ps.23").unwrap();
        let native = Versification::Vulgate.native_passages(&p);
        assert_eq!(native.len(), 1);
        assert_eq!(native[0].to_osis(), "Ps.22.1-Ps.22.6");
        // Untouched passages pass through, as does everything in the canonical scheme.
        let p = Passage::parse_osis("John.3.16").unwrap();
        assert_eq!(Versification::Vulgate.native_passages(&p), vec![p.clone()]);
        assert_eq!(Versification::Kjv.native_passages(&p), vec![p]);
        let p = Passage::parse_osis("Mal.4").unwrap();
        let native = Versification::Hebrew.native_passages(&p);
        assert_eq!(native[0].to_osis(), "Mal.3.19-Mal.3.24");
    }

    #[test]
    fn load_canonical_rekeys_verses() {
        let store: Vec<Verse> = (19..=24)
            .map(|n| Verse {
                reference: BibleRef::verse("Mal", 3, n),
                text: format!("t{n}"),
                marks: vec![],
            })
            .collect();
        let got = load_canonical::<()>(
            Versification::Hebrew,
            &Passage::parse_osis("Mal.4").unwrap(),
            |p| {
                Ok(store
                    .iter()
                    .filter(|v| {
                        v.reference.chapter == p.start.chapter
                            && v.reference.verse >= p.start.verse
                            && v.reference.verse <= p.end.verse
                    })
                    .cloned()
                    .collect())
            },
        )
        .unwrap();
        assert_eq!(got.len(), 6);
        assert_eq!(got[0].reference, r("Mal.4.1"));
        assert_eq!(got[5].reference, r("Mal.4.6"));
        assert_eq!(got[0].text, "t19");
    }

    #[test]
    fn alignment_marks_missing_verses() {
        let v = |b: &str, c, n, t: &str| Verse {
            reference: BibleRef::verse(b, c, n),
            text: t.into(),
            marks: vec![],
        };
        let kjv = vec![
            v("Acts", 8, 36, "a36"),
            v("Acts", 8, 37, "a37"),
            v("Acts", 8, 38, "a38"),
        ];
        let nvi_like = vec![v("Acts", 8, 36, "b36"), v("Acts", 8, 38, "b38")];
        let rows = align_parallel(&[
            (VersionCode("KJV".into()), kjv),
            (VersionCode("CRIT".into()), nvi_like),
        ]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].reference, r("Acts.8.37"));
        assert_eq!(rows[1].cells, vec![Some("a37".to_owned()), None]);
        assert!(is_commonly_omitted(&r("Acts.8.37")));
        assert!(!is_commonly_omitted(&r("Acts.8.38")));
        assert!(!is_commonly_omitted(&BibleRef::chapter("Acts", 8)));
    }
}
