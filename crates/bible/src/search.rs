//! In-memory full-text index of one Bible version: accent- and case-insensitive, AND semantics,
//! `"quoted phrases"`, and type-ahead prefix matching on the last word.
//!
//! The SQLite store has its own FTS5 index (the app searches through `BibleRepo::search`); this
//! index serves imports previews, tests, the command palette over an already loaded version and
//! any store without FTS.

use hollyshow_core::text::fold;
use hollyshow_core::{BibleHit, Verse, VersionCode};
use std::collections::BTreeMap;

pub struct BibleIndex {
    version: VersionCode,
    verses: Vec<Verse>,
    folded: Vec<String>,
    postings: BTreeMap<String, Vec<u32>>,
}

/// Words of `text`, folded: letters and digits only.
pub fn tokens(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Query {
    words: Vec<String>,
    phrases: Vec<String>,
    /// The last typed word matches as a prefix ("amo" finds "amó", "amor").
    prefix_last: bool,
}

fn parse_query(q: &str) -> Query {
    let mut out = Query::default();
    let mut rest = q;
    let mut free = String::new();
    while let Some(start) = rest.find('"') {
        free.push_str(&rest[..start]);
        free.push(' ');
        let after = &rest[start + 1..];
        match after.find('"') {
            Some(end) => {
                let phrase = tokens(&after[..end]).join(" ");
                if !phrase.is_empty() {
                    out.phrases.push(phrase);
                }
                rest = &after[end + 1..];
            }
            None => {
                free.push_str(after);
                rest = "";
            }
        }
    }
    free.push_str(rest);
    out.words = tokens(&free);
    out.prefix_last =
        !out.words.is_empty() && !free.ends_with(char::is_whitespace) && out.phrases.is_empty();
    out
}

impl BibleIndex {
    pub fn build(version: VersionCode, verses: Vec<Verse>) -> Self {
        let mut postings: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        let mut folded = Vec::with_capacity(verses.len());
        for (i, v) in verses.iter().enumerate() {
            let words = tokens(&v.text);
            folded.push(words.join(" "));
            let id = u32::try_from(i).unwrap_or(u32::MAX);
            let mut seen = std::collections::HashSet::new();
            for w in words {
                if seen.insert(w.clone()) {
                    postings.entry(w).or_default().push(id);
                }
            }
        }
        Self {
            version,
            verses,
            folded,
            postings,
        }
    }

    pub fn len(&self) -> usize {
        self.verses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.verses.is_empty()
    }

    pub fn version(&self) -> &VersionCode {
        &self.version
    }

    fn posting(&self, word: &str, prefix: bool) -> Vec<u32> {
        if !prefix {
            return self.postings.get(word).cloned().unwrap_or_default();
        }
        let mut all: Vec<u32> = self
            .postings
            .range(word.to_owned()..)
            .take_while(|(k, _)| k.starts_with(word))
            .flat_map(|(_, v)| v.iter().copied())
            .collect();
        all.sort_unstable();
        all.dedup();
        all
    }

    /// Verses containing every word (and every phrase) of `query`, in index order (canonical
    /// when built from sorted verses). An empty query matches nothing.
    pub fn search(&self, query: &str, limit: usize) -> Vec<BibleHit> {
        let q = parse_query(query);
        if q.words.is_empty() && q.phrases.is_empty() {
            return Vec::new();
        }
        let mut candidates: Option<Vec<u32>> = None;
        for (i, w) in q.words.iter().enumerate() {
            let p = self.posting(w, q.prefix_last && i + 1 == q.words.len());
            candidates = Some(match candidates {
                None => p,
                Some(c) => intersect(&c, &p),
            });
            if candidates.as_ref().is_some_and(Vec::is_empty) {
                return Vec::new();
            }
        }
        let ids: Vec<u32> = match candidates {
            Some(c) => c,
            // Phrases only: scan everything.
            None => (0..u32::try_from(self.verses.len()).unwrap_or(u32::MAX)).collect(),
        };
        ids.into_iter()
            .filter(|id| {
                let text = &self.folded[*id as usize];
                q.phrases.iter().all(|p| contains_words(text, p))
            })
            .take(limit)
            .map(|id| {
                let v = &self.verses[id as usize];
                BibleHit {
                    version: self.version.clone(),
                    reference: v.reference.clone(),
                    text: v.text.clone(),
                }
            })
            .collect()
    }
}

/// Whole-word substring match on space-joined folded words.
fn contains_words(haystack: &str, needle: &str) -> bool {
    let h = format!(" {haystack} ");
    h.contains(&format!(" {needle} "))
}

fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// Searches several versions and concatenates the hits (version order, then canonical order).
pub fn search_versions(indexes: &[&BibleIndex], query: &str, limit: usize) -> Vec<BibleHit> {
    let mut out = Vec::new();
    for idx in indexes {
        if out.len() >= limit {
            break;
        }
        out.extend(idx.search(query, limit - out.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::BibleRef;

    fn idx() -> BibleIndex {
        let v = |c, n, t: &str| Verse {
            reference: BibleRef::verse("John", c, n),
            text: t.into(),
            marks: vec![],
        };
        BibleIndex::build(
            VersionCode("T".into()),
            vec![
                v(3, 16, "Porque de tal manera amó Dios al mundo"),
                v(
                    3,
                    17,
                    "Porque no envió Dios a su Hijo al mundo para condenar",
                ),
                v(
                    1,
                    1,
                    "En el principio era el Verbo, y el Verbo era con Dios",
                ),
                v(1, 5, "La luz en las tinieblas resplandece"),
            ],
        )
    }

    #[test]
    fn matches_without_accents_or_case() {
        let i = idx();
        assert_eq!(i.search("AMO", 10).len(), 1); // prefix of "amó"
        assert_eq!(i.search("amó dios", 10).len(), 1);
        assert_eq!(i.search("dios", 10).len(), 3);
        assert_eq!(i.search("dios mundo", 10).len(), 2);
        assert_eq!(i.search("dios mundo ", 10).len(), 2);
        assert_eq!(i.search("zzz", 10).len(), 0);
        assert_eq!(i.search("", 10).len(), 0);
        assert_eq!(i.search("   ", 10).len(), 0);
    }

    #[test]
    fn prefix_only_applies_to_the_word_being_typed() {
        let i = idx();
        assert_eq!(i.search("dio", 10).len(), 3);
        // A finished word is exact: "dio" + space must not match "Dios".
        assert_eq!(i.search("dio ", 10).len(), 0);
    }

    #[test]
    fn phrases_need_adjacent_whole_words() {
        let i = idx();
        assert_eq!(i.search("\"al mundo\"", 10).len(), 2);
        assert_eq!(i.search("\"mundo al\"", 10).len(), 0);
        assert_eq!(i.search("\"verbo era\" dios", 10).len(), 1);
        assert_eq!(i.search("\"el verb\"", 10).len(), 0);
        assert_eq!(i.search("\"dios", 10).len(), 3);
    }

    #[test]
    fn limit_and_order() {
        let i = idx();
        let hits = i.search("dios", 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].reference.to_string(), "John.3.16");
        let a = idx();
        let b = idx();
        assert_eq!(search_versions(&[&a, &b], "luz", 10).len(), 2);
        assert_eq!(search_versions(&[&a, &b], "luz", 1).len(), 1);
    }
}
