//! Free-text reference parser ("Jn 3:16", "juan 3 16-18", "1 Co 13", "Gn 1:1-2:3", "Sal 23")
//! and localised formatting of passages.
//!
//! Accepted shapes (accents, case and dots after abbreviations are ignored):
//!
//! * `Juan 3:16`, `Jn 3.16`, `juan 3 16`, `Juan capítulo 3 versículo 16`
//! * ranges `Jn 3:16-18`, `Jn 3:16 al 18`, `Gn 1:1-2:3`, `Sal 23-24` (chapters)
//! * chapter only `Sal 23`, whole book `Judas`
//! * lists `Jn 3:16,18`, `Jn 3:16; 4:1-3`, `Sal 23:1-3, 5`, `Jn 3:16 y 18`
//! * Catholic notation `Jn 3,16-18; 4,1` (comma between chapter and verse, dot between verses)
//!   whenever no colon is present. In that notation `Sal 23,24` is chapter 23 verse 24; write
//!   `Sal 23; 24` or `Sal 23-24` for chapters.
//! * single-chapter books take bare verses: `Judas 3`, `Abd 1-4` (`Judas 1:3` also works).

use crate::books::{resolve_book, Book, Lang};
use crate::{BibleError, ReferenceParser};
use hollyshow_core::text::fold;
use hollyshow_core::{BibleRef, Passage};

/// The default parser. Stateless; the book tables are static.
#[derive(Debug, Clone, Copy, Default)]
pub struct Parser;

impl ReferenceParser for Parser {
    /// Parses a single contiguous passage. Inputs that denote several disjoint passages
    /// (`Jn 3:16,18`) are rejected here; use [`Parser::parse_all`].
    fn parse(&self, input: &str, language: &str) -> Result<Passage, BibleError> {
        let mut all = self.parse_all(input, language)?;
        if all.len() == 1 {
            Ok(all.remove(0))
        } else {
            Err(bad(input))
        }
    }
}

impl Parser {
    /// Parses one or more passages (lists with `,` `;` `y`), merging adjacent verses.
    pub fn parse_all(&self, input: &str, language: &str) -> Result<Vec<Passage>, BibleError> {
        parse_all(input, Lang::from_tag(language))
    }
}

/// Convenience wrapper over [`Parser::parse_all`].
pub fn parse_references(input: &str, language: &str) -> Result<Vec<Passage>, BibleError> {
    Parser.parse_all(input, language)
}

fn bad(input: &str) -> BibleError {
    BibleError::BadReference(input.trim().to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Num(u16),
    Colon,
    Dot,
    Dash,
    Comma,
    Semi,
}

fn parse_all(input: &str, lang: Option<Lang>) -> Result<Vec<Passage>, BibleError> {
    let err = || bad(input);
    let cleaned = clean(input);
    let (book_text, tail) = split_book(&cleaned);
    let book = resolve_book(book_text, lang).ok_or_else(err)?;
    if tail.trim().is_empty() {
        return Ok(vec![whole_book(book)]);
    }
    let toks = tokenize(tail).ok_or_else(err)?;
    let toks = normalise_separators(toks);
    let passages = items(book, &toks).ok_or_else(err)?;
    if passages.is_empty() {
        return Err(err());
    }
    Ok(merge_adjacent(passages))
}

/// Folds case/accents, unifies dashes and replaces the words "capítulo"/"versículo" and the
/// connectors ("al", "y"...) by punctuation, so the rest of the parser only sees symbols.
fn clean(input: &str) -> String {
    let s = fold(input)
        .replace(['\u{2013}', '\u{2014}', '\u{2212}', '\u{2012}'], "-")
        .replace(['\u{ba}', '\u{aa}'], "");
    let mut out: Vec<String> = Vec::new();
    // The book name (everything before the first digit that follows a letter) must stay verbatim,
    // so connectors are only rewritten once a numeric tail has started.
    let mut in_tail = false;
    let mut seen_letter = false;
    for raw in s.split_whitespace() {
        let word = raw.trim_matches(|c: char| c == '.' || c == ',');
        if !in_tail {
            if raw.chars().any(char::is_alphabetic) {
                seen_letter = true;
            }
            if seen_letter && raw.chars().any(|c| c.is_ascii_digit()) {
                in_tail = true;
            }
        }
        if in_tail {
            let rep = match word {
                "capitulo" | "capitulos" | "cap" | "chapter" | "chapters" | "capitulo:" => Some(""),
                "versiculo" | "versiculos" | "vers" | "ver" | "verso" | "versos" | "verse"
                | "verses" | "vv" | "vs" | "v" => Some(":"),
                "al" | "a" | "hasta" | "to" | "through" | "thru" | "ao" | "ate" => Some("-"),
                "y" | "and" | "e" | "&" => Some(","),
                _ => None,
            };
            if let Some(r) = rep {
                out.push(r.to_owned());
                continue;
            }
        } else if matches!(word, "capitulo" | "cap" | "chapter") {
            // "Juan capítulo 3": drop the word, the number that follows starts the tail.
            seen_letter = true;
            continue;
        }
        out.push(raw.to_owned());
    }
    out.join(" ")
}

/// Splits `"1 co 13:4"` into the book (`"1 co"`) and the tail (`"13:4"`). The tail starts at the
/// first digit that comes after a letter, so a numeral prefix ("1 co", "2 reyes") stays in the
/// book name.
fn split_book(s: &str) -> (&str, &str) {
    let mut seen_letter = false;
    for (i, c) in s.char_indices() {
        if c.is_alphabetic() {
            seen_letter = true;
        } else if c.is_ascii_digit() && seen_letter {
            let (b, t) = s.split_at(i);
            return (b.trim().trim_end_matches('.'), t);
        }
    }
    (s.trim().trim_end_matches('.'), "")
}

fn tokenize(tail: &str) -> Option<Vec<Tok>> {
    let mut toks = Vec::new();
    let chars: Vec<char> = tail.chars().collect();
    let mut i = 0;
    let mut pending_space = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            _ if c.is_whitespace() => {
                pending_space = true;
                i += 1;
                continue;
            }
            '0'..='9' => {
                let mut n: u32 = 0;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    n = n * 10 + chars[i].to_digit(10)?;
                    if n > 9999 {
                        return None;
                    }
                    i += 1;
                }
                // "3 16": two numbers separated only by space mean chapter and verse.
                if pending_space && matches!(toks.last(), Some(Tok::Num(_))) {
                    toks.push(Tok::Colon);
                }
                toks.push(Tok::Num(u16::try_from(n).ok()?));
                pending_space = false;
                continue;
            }
            ':' => toks.push(Tok::Colon),
            '.' => {
                if matches!(toks.last(), Some(Tok::Num(_))) {
                    toks.push(Tok::Dot);
                }
            }
            '-' => toks.push(Tok::Dash),
            ',' => toks.push(Tok::Comma),
            ';' => toks.push(Tok::Semi),
            _ => return None,
        }
        pending_space = false;
        i += 1;
    }
    Some(toks)
}

/// Resolves the ambiguity of `.` and `,`: the first separator decides the notation.
fn normalise_separators(toks: Vec<Tok>) -> Vec<Tok> {
    let has_colon = toks.contains(&Tok::Colon);
    let first_sep = toks
        .iter()
        .find(|t| matches!(t, Tok::Comma | Tok::Dot | Tok::Colon));
    let catholic = !has_colon && first_sep == Some(&Tok::Comma);
    toks.into_iter()
        .map(|t| match (t, catholic) {
            (Tok::Comma, true) => Tok::Colon,
            (Tok::Dot, true) => Tok::Comma,
            (Tok::Dot, false) => Tok::Colon,
            (t, _) => t,
        })
        .collect()
}

fn whole_book(book: &Book) -> Passage {
    Passage::range(
        BibleRef::chapter(book.osis, 1),
        BibleRef::chapter(book.osis, u16::from(book.chapters)),
    )
}

struct Cursor<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.pos).copied()
    }
    fn num(&mut self) -> Option<u16> {
        if let Some(Tok::Num(n)) = self.peek() {
            self.pos += 1;
            Some(n)
        } else {
            None
        }
    }
    fn eat(&mut self, t: Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}

/// One `A[:B][-C[:D]]` item before its context is applied.
struct Item {
    a: u16,
    b: Option<u16>,
    c: Option<u16>,
    d: Option<u16>,
    dash: bool,
}

fn read_item(cur: &mut Cursor<'_>) -> Option<Item> {
    let a = cur.num()?;
    let b = if cur.eat(Tok::Colon) {
        Some(cur.num()?)
    } else {
        None
    };
    let (mut c, mut d, mut dash) = (None, None, false);
    if cur.eat(Tok::Dash) {
        dash = true;
        c = Some(cur.num()?);
        if cur.eat(Tok::Colon) {
            d = Some(cur.num()?);
        }
    }
    Some(Item { a, b, c, d, dash })
}

fn items(book: &Book, toks: &[Tok]) -> Option<Vec<Passage>> {
    let osis = book.osis;
    let max_ch = u16::from(book.chapters);
    let single = book.is_single_chapter();
    let mut cur = Cursor { toks, pos: 0 };
    let mut out = Vec::new();
    // Chapter in force for bare verse numbers (`Jn 3:16,18`); reset by `;`.
    let mut ctx: Option<u16> = single.then_some(1);
    let check_ch = |c: u16| (1..=max_ch).contains(&c).then_some(c);
    let check_v = |v: u16| (v >= 1).then_some(v);
    loop {
        let it = read_item(&mut cur)?;
        let passage = match (it.b, it.dash) {
            // `N` alone.
            (None, false) => match ctx {
                Some(ch) => {
                    let v = check_v(it.a)?;
                    Passage::single(BibleRef::verse(osis, ch, v))
                }
                None => {
                    let ch = check_ch(it.a)?;
                    // A bare number after a chapter-only item is another chapter, so `ctx` stays unset.
                    Passage::single(BibleRef::chapter(osis, ch))
                }
            },
            // `N:M`.
            (Some(m), false) => {
                let ch = check_ch(it.a)?;
                ctx = Some(ch);
                Passage::single(BibleRef::verse(osis, ch, check_v(m)?))
            }
            // `A-C` (verses in context, or chapters) or `A-C:D` (chapter A to C:D).
            (None, true) => {
                let c = it.c?;
                match (ctx, it.d) {
                    (Some(ch), None) => {
                        let (v1, v2) = (check_v(it.a)?, check_v(c)?);
                        Passage::range(BibleRef::verse(osis, ch, v1), BibleRef::verse(osis, ch, v2))
                    }
                    (None, None) => {
                        let (c1, c2) = (check_ch(it.a)?, check_ch(c)?);
                        Passage::range(BibleRef::chapter(osis, c1), BibleRef::chapter(osis, c2))
                    }
                    (_, Some(d)) => {
                        let (c1, c2) = (check_ch(it.a)?, check_ch(c)?);
                        ctx = Some(c2);
                        Passage::range(
                            BibleRef::chapter(osis, c1),
                            BibleRef::verse(osis, c2, check_v(d)?),
                        )
                    }
                }
            }
            // `N:M-K` or `N:M-K:L`.
            (Some(m), true) => {
                let ch = check_ch(it.a)?;
                let k = it.c?;
                let start = BibleRef::verse(osis, ch, check_v(m)?);
                match it.d {
                    None => {
                        ctx = Some(ch);
                        Passage::range(start, BibleRef::verse(osis, ch, check_v(k)?))
                    }
                    Some(l) => {
                        let ch2 = check_ch(k)?;
                        ctx = Some(ch2);
                        Passage::range(start, BibleRef::verse(osis, ch2, check_v(l)?))
                    }
                }
            }
        };
        if passage_order_ok(&passage) {
            out.push(passage);
        } else {
            return None;
        }
        match cur.peek() {
            None => break,
            Some(Tok::Comma) => {
                cur.pos += 1;
            }
            Some(Tok::Semi) => {
                cur.pos += 1;
                ctx = single.then_some(1);
            }
            Some(_) => return None,
        }
        if cur.peek().is_none() {
            // Trailing separator: tolerated ("Jn 3:16,").
            break;
        }
    }
    Some(out)
}

fn passage_order_ok(p: &Passage) -> bool {
    let key = |r: &BibleRef| (r.chapter, r.verse.unwrap_or(0));
    key(&p.start) <= key(&p.end)
}

/// Joins `Jn 3:16` + `Jn 3:17` into `Jn 3:16-17` (same chapter, consecutive verses).
fn merge_adjacent(list: Vec<Passage>) -> Vec<Passage> {
    let mut out: Vec<Passage> = Vec::with_capacity(list.len());
    for p in list {
        if let Some(last) = out.last_mut() {
            let joinable = last.end.book == p.start.book
                && last.end.chapter == p.start.chapter
                && matches!((last.end.verse, p.start.verse), (Some(a), Some(b)) if b == a + 1)
                && p.end.chapter == p.start.chapter;
            if joinable {
                last.end = p.end;
                continue;
            }
        }
        out.push(p);
    }
    out
}

/// Formats `r` as `Juan 3:16` / `Salmos 23` in `lang`.
pub fn format_ref(r: &BibleRef, lang: Lang) -> String {
    let name = crate::books::display_name(&r.book.0, lang);
    let single = crate::books::book(&r.book.0).is_some_and(Book::is_single_chapter);
    match (r.verse, single) {
        (Some(v), true) => format!("{name} {v}"),
        (Some(v), false) => format!("{name} {}:{v}", r.chapter),
        (None, true) => name,
        (None, false) => format!("{name} {}", r.chapter),
    }
}

/// Human label of a passage: `Juan 3:16-18`, `Génesis 1:1–2:3`, `Salmos 23`, `Judas`.
pub fn format_passage(p: &Passage, lang: Lang) -> String {
    if p.start == p.end {
        return format_ref(&p.start, lang);
    }
    let book = crate::books::book(&p.start.book.0);
    let single = book.is_some_and(Book::is_single_chapter);
    if p.start.book != p.end.book {
        return format!(
            "{} \u{2013} {}",
            format_ref(&p.start, lang),
            format_ref(&p.end, lang)
        );
    }
    let name = crate::books::display_name(&p.start.book.0, lang);
    let (s, e) = (&p.start, &p.end);
    let last_ch = book.map_or(0, |b| u16::from(b.chapters));
    match (s.verse, e.verse) {
        (None, None) if s.chapter == 1 && e.chapter == last_ch => name,
        (None, None) => format!("{name} {}\u{2013}{}", s.chapter, e.chapter),
        (Some(a), Some(b)) if s.chapter == e.chapter => {
            if single {
                format!("{name} {a}\u{2013}{b}")
            } else {
                format!("{name} {}:{a}\u{2013}{b}", s.chapter)
            }
        }
        (Some(a), Some(b)) => format!("{name} {}:{a}\u{2013}{}:{b}", s.chapter, e.chapter),
        _ => format!("{} \u{2013} {}", format_ref(s, lang), format_ref(e, lang)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osis(input: &str) -> String {
        Parser
            .parse(input, "es")
            .unwrap_or_else(|e| panic!("{input}: {e}"))
            .to_osis()
    }

    #[test]
    fn simple_verse_and_ranges() {
        assert_eq!(osis("Jn 3:16"), "John.3.16");
        assert_eq!(osis("juan 3 16-18"), "John.3.16-John.3.18");
        assert_eq!(osis("Juan 3:16-18"), "John.3.16-John.3.18");
        assert_eq!(osis("Jn. 3.16"), "John.3.16");
        assert_eq!(osis("JUAN 3:16"), "John.3.16");
        assert_eq!(osis("  jn   3 : 16 "), "John.3.16");
        assert_eq!(osis("Jn 3:16 al 18"), "John.3.16-John.3.18");
        assert_eq!(osis("Jn 3:16–18"), "John.3.16-John.3.18");
    }

    #[test]
    fn numbered_books_and_roman_numerals() {
        assert_eq!(osis("1 Co 13"), "1Cor.13");
        assert_eq!(osis("1Co 13:4-7"), "1Cor.13.4-1Cor.13.7");
        assert_eq!(osis("I Juan 1:9"), "1John.1.9");
        assert_eq!(osis("II Reyes 2:11"), "2Kgs.2.11");
        assert_eq!(osis("2 Timoteo 3:16"), "2Tim.3.16");
        assert_eq!(osis("Primera de Juan 1:9"), "1John.1.9");
    }

    #[test]
    fn chapters_and_books() {
        assert_eq!(osis("Sal 23"), "Ps.23");
        assert_eq!(osis("Salmo 23"), "Ps.23");
        assert_eq!(osis("Sal 23-24"), "Ps.23-Ps.24");
        assert_eq!(osis("Ap 21:1-4"), "Rev.21.1-Rev.21.4");
        assert_eq!(osis("Gn 1:1-2:3"), "Gen.1.1-Gen.2.3");
        assert_eq!(osis("Judas"), "Jude.1");
        let p = Parser.parse("Juan", "es").unwrap();
        assert_eq!(p.to_osis(), "John.1-John.21");
    }

    #[test]
    fn verbose_words() {
        assert_eq!(osis("Juan capítulo 3 versículo 16"), "John.3.16");
        assert_eq!(osis("Juan cap. 3 vers. 16 al 18"), "John.3.16-John.3.18");
        assert_eq!(osis("John chapter 3 verse 16"), "John.3.16");
    }

    #[test]
    fn lists() {
        let all = Parser.parse_all("Jn 3:16,18", "es").unwrap();
        assert_eq!(
            all.iter().map(Passage::to_osis).collect::<Vec<_>>(),
            ["John.3.16", "John.3.18"]
        );
        let all = Parser.parse_all("Sal 23:1-3, 5; 24:1", "es").unwrap();
        assert_eq!(
            all.iter().map(Passage::to_osis).collect::<Vec<_>>(),
            ["Ps.23.1-Ps.23.3", "Ps.23.5", "Ps.24.1"]
        );
        // Adjacent verses merge.
        let all = Parser.parse_all("Jn 3:16,17", "es").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].to_osis(), "John.3.16-John.3.17");
        let all = Parser.parse_all("Jn 3:16 y 18", "es").unwrap();
        assert_eq!(all.len(), 2);
        // The single-passage entry point refuses disjoint lists.
        assert!(Parser.parse("Jn 3:16,18", "es").is_err());
        let all = Parser.parse_all("Sal 1; 3", "es").unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn catholic_notation() {
        assert_eq!(osis("Jn 3,16-18"), "John.3.16-John.3.18");
        let all = Parser.parse_all("Jn 3,16.18; 4,1", "es").unwrap();
        assert_eq!(
            all.iter().map(Passage::to_osis).collect::<Vec<_>>(),
            ["John.3.16", "John.3.18", "John.4.1"]
        );
    }

    #[test]
    fn single_chapter_books_take_bare_verses() {
        assert_eq!(osis("Judas 3"), "Jude.1.3");
        assert_eq!(osis("Jud 3-5"), "Jude.1.3-Jude.1.5");
        assert_eq!(osis("Judas 1:3"), "Jude.1.3");
        assert_eq!(osis("3 Jn 4"), "3John.1.4");
    }

    #[test]
    fn language_hint_and_foreign_names() {
        assert_eq!(
            Parser.parse("John 3:16", "es").unwrap().to_osis(),
            "John.3.16"
        );
        assert_eq!(
            Parser.parse("João 3:16", "pt").unwrap().to_osis(),
            "John.3.16"
        );
        assert_eq!(Parser.parse("Jn 1:1", "pt").unwrap().to_osis(), "Jonah.1.1");
        assert_eq!(Parser.parse("Jn 1:1", "es").unwrap().to_osis(), "John.1.1");
        assert_eq!(Parser.parse("Jn 1:1", "xx").unwrap().to_osis(), "John.1.1");
    }

    #[test]
    fn rejects_garbage() {
        for s in [
            "",
            "   ",
            "Jn",
            "3:16",
            "Jn 0:1",
            "Jn 22:1",
            "Jn 3:0",
            "Jn 3:18-16",
            "Jn 3:16-",
            "Jn 3:x",
            "Jn 3:16:17",
            "Libro 3:16",
            "Jn 99999",
        ] {
            let r = Parser.parse(s, "es");
            // A bare book name is a valid whole-book reference.
            if s == "Jn" {
                assert!(r.is_ok());
                continue;
            }
            assert!(r.is_err(), "`{s}` should fail, got {r:?}");
        }
    }

    #[test]
    fn deuterocanon_and_accents() {
        assert_eq!(osis("Sabiduría 3:1"), "Wis.3.1");
        assert_eq!(osis("sab 3:1"), "Wis.3.1");
        assert_eq!(osis("2 Macabeos 7:1"), "2Macc.7.1");
        assert_eq!(osis("Génesis 1:1"), "Gen.1.1");
        assert_eq!(osis("genesis 1:1"), "Gen.1.1");
        assert_eq!(osis("Éx 20:1-17"), "Exod.20.1-Exod.20.17");
    }

    #[test]
    fn formats_in_each_language() {
        let p = Parser.parse("1 Co 13:4-7", "es").unwrap();
        assert_eq!(format_passage(&p, Lang::Es), "1 Corintios 13:4\u{2013}7");
        assert_eq!(format_passage(&p, Lang::En), "1 Corinthians 13:4\u{2013}7");
        let p = Parser.parse("Jn 3:16", "es").unwrap();
        assert_eq!(format_passage(&p, Lang::Pt), "João 3:16");
        let p = Parser.parse("Gn 1:1-2:3", "es").unwrap();
        assert_eq!(format_passage(&p, Lang::Es), "Génesis 1:1\u{2013}2:3");
        let p = Parser.parse("Sal 23", "es").unwrap();
        assert_eq!(format_passage(&p, Lang::Es), "Salmos 23");
        let p = Parser.parse("Sal 23-24", "es").unwrap();
        assert_eq!(format_passage(&p, Lang::Es), "Salmos 23\u{2013}24");
        let p = Parser.parse("Judas 3", "es").unwrap();
        assert_eq!(format_passage(&p, Lang::Es), "Judas 3");
        let p = Parser.parse("Judas", "es").unwrap();
        assert_eq!(format_passage(&p, Lang::Es), "Judas");
    }

    #[test]
    fn format_output_parses_back() {
        for s in ["Jn 3:16-18", "Gn 1:1-2:3", "Sal 23", "Judas 3-5", "1 Co 13"] {
            let p = Parser.parse(s, "es").unwrap();
            let text = format_passage(&p, Lang::Es);
            assert_eq!(Parser.parse(&text, "es").unwrap(), p, "{s} -> {text}");
        }
    }
}
