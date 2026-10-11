//! Simple JSON and CSV Bibles.
//!
//! JSON shapes accepted:
//! * `{"translation": "...", "books": [{"name": "Genesis", "chapters": [{"chapter": 1,
//!   "verses": [{"verse": 1, "text": "..."}]}]}]}` (scrollmapper/bible_databases)
//! * `[{"abbrev": "gn", "name": "Genesis", "chapters": [["verse 1", "verse 2"]]}]`
//!   (array-of-arrays; books by name/abbreviation, or by position when there are exactly 66)
//! * flat `[{"book": "Gen", "chapter": 1, "verse": 1, "text": "..."}]` or `{"verses": [...]}`
//! * nested maps `{"Genesis": {"1": {"1": "text"}}}`
//!
//! CSV: `book,chapter,verse,text` with an optional header (any of `,` `;` tab as separator),
//! RFC 4180 quoting. `book` is a name, abbreviation, OSIS id, USFM code or 1-based number.

use super::{lookup_book, malformed, parse_u16, plain, utf8, Collector, Declared, ImportMeta};
use crate::books::{book_by_ord, Book};
use crate::{BibleError, BibleImporter, ImportedBible};
use serde_json::Value;

#[derive(Debug, Default, Clone)]
pub struct JsonImporter {
    meta: ImportMeta,
}

impl JsonImporter {
    pub fn new(meta: ImportMeta) -> Self {
        Self { meta }
    }
}

fn str_field<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str))
}

fn num(v: &Value) -> Option<u16> {
    match v {
        Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        Value::String(s) => parse_u16(s),
        _ => None,
    }
}

fn text_of(v: &Value) -> Option<&str> {
    v.as_str()
}

impl BibleImporter for JsonImporter {
    fn format(&self) -> &'static str {
        "json"
    }

    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError> {
        let bytes = plain(input)?;
        let root: Value =
            serde_json::from_str(utf8(&bytes)?).map_err(|e| malformed(format!("JSON: {e}")))?;
        let mut out = Collector::default();
        let mut declared = Declared::default();
        if root.is_object() {
            declared.name = str_field(&root, &["translation", "name", "title"]).map(str::to_owned);
            declared.code =
                str_field(&root, &["code", "abbreviation", "abbrev", "id"]).map(str::to_owned);
            declared.language = str_field(&root, &["language", "lang"]).map(str::to_owned);
            declared.copyright = str_field(&root, &["copyright", "rights"]).map(str::to_owned);
            declared.license = str_field(&root, &["license"]).map(str::to_owned);
            declared.versification = str_field(&root, &["versification"]).map(str::to_owned);
        }
        let books_value = match &root {
            Value::Object(o) => o.get("books").or_else(|| o.get("verses")),
            Value::Array(_) => Some(&root),
            _ => None,
        };
        match books_value {
            Some(Value::Array(items)) => read_array(items, &mut out)?,
            _ => match &root {
                Value::Object(map) if books_value.is_none() => read_nested_map(map, &mut out),
                _ => return Err(malformed("JSON: unrecognised Bible layout")),
            },
        }
        out.finish("JSON", declared, &self.meta)
    }
}

fn read_array(items: &[Value], out: &mut Collector) -> Result<(), BibleError> {
    if items.is_empty() {
        return Err(malformed("JSON: empty Bible"));
    }
    if items[0].get("chapters").is_some() {
        let positional = items.len() == 66;
        for (i, b) in items.iter().enumerate() {
            let name = str_field(b, &["name", "book", "abbrev", "abbreviation"]).unwrap_or("");
            let book = lookup_book_by_label(name, b).or_else(|| {
                if positional {
                    book_by_ord(i + 1)
                } else {
                    None
                }
            });
            let Some(book) = book else {
                out.unknown(name);
                continue;
            };
            read_chapters(book, b.get("chapters"), out);
        }
        return Ok(());
    }
    // Flat verse records.
    for r in items {
        let name = str_field(r, &["book", "osis", "usfm", "book_name", "bookName"]).unwrap_or("");
        let book = match lookup_book(name) {
            Some(b) => b,
            None => {
                out.unknown(name);
                continue;
            }
        };
        let (Some(ch), Some(v), Some(t)) = (
            r.get("chapter").and_then(num),
            r.get("verse").and_then(num),
            r.get("text").and_then(text_of),
        ) else {
            continue;
        };
        out.push_plain(book, ch, v, t);
    }
    Ok(())
}

/// Tries the long name, then the abbreviation field.
fn lookup_book_by_label(name: &str, b: &Value) -> Option<&'static Book> {
    if !name.is_empty() {
        if let Some(book) = lookup_book(name) {
            return Some(book);
        }
    }
    str_field(b, &["abbrev", "abbreviation"]).and_then(lookup_book)
}

fn read_chapters(book: &'static Book, chapters: Option<&Value>, out: &mut Collector) {
    let Some(Value::Array(chs)) = chapters else {
        return;
    };
    for (ci, c) in chs.iter().enumerate() {
        match c {
            // [["v1", "v2"], ...]: positions are numbers.
            Value::Array(verses) => {
                for (vi, t) in verses.iter().enumerate() {
                    if let (Some(t), Ok(ch), Ok(v)) =
                        (t.as_str(), u16::try_from(ci + 1), u16::try_from(vi + 1))
                    {
                        out.push_plain(book, ch, v, t);
                    }
                }
            }
            // [{"chapter": 1, "verses": [{"verse": 1, "text": ".."}]}]
            Value::Object(_) => {
                let ch = c
                    .get("chapter")
                    .and_then(num)
                    .or_else(|| u16::try_from(ci + 1).ok());
                let Some(ch) = ch else { continue };
                if let Some(Value::Array(vs)) = c.get("verses") {
                    for (vi, vv) in vs.iter().enumerate() {
                        let v = vv
                            .get("verse")
                            .and_then(num)
                            .or_else(|| u16::try_from(vi + 1).ok());
                        if let (Some(v), Some(t)) = (v, vv.get("text").and_then(text_of)) {
                            out.push_plain(book, ch, v, t);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn read_nested_map(map: &serde_json::Map<String, Value>, out: &mut Collector) {
    for (name, chapters) in map {
        let Some(book) = lookup_book(name) else {
            if chapters.is_object() {
                out.unknown(name);
            }
            continue;
        };
        let Some(chs) = chapters.as_object() else {
            continue;
        };
        for (cn, verses) in chs {
            let (Some(ch), Some(vs)) = (parse_u16(cn), verses.as_object()) else {
                continue;
            };
            for (vn, t) in vs {
                if let (Some(v), Some(t)) = (parse_u16(vn), t.as_str()) {
                    out.push_plain(book, ch, v, t);
                }
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct CsvImporter {
    meta: ImportMeta,
}

impl CsvImporter {
    pub fn new(meta: ImportMeta) -> Self {
        Self { meta }
    }
}

impl BibleImporter for CsvImporter {
    fn format(&self) -> &'static str {
        "csv"
    }

    fn import(&self, input: &[u8]) -> Result<ImportedBible, BibleError> {
        let bytes = plain(input)?;
        let text = utf8(&bytes)?;
        let first_line = text.lines().next().unwrap_or("");
        let delim = [',', ';', '\t']
            .into_iter()
            .max_by_key(|d| first_line.matches(*d).count())
            .unwrap_or(',');
        let rows = parse_csv(text, delim)?;
        let mut rows = rows.into_iter();
        let mut cols = (0usize, 1usize, 2usize, 3usize);
        let mut pending: Option<Vec<String>> = None;
        if let Some(first) = rows.next() {
            let lower: Vec<String> = first.iter().map(|c| c.trim().to_lowercase()).collect();
            if lower
                .iter()
                .any(|c| c == "book" || c == "text" || c == "verse")
            {
                let find = |names: &[&str]| lower.iter().position(|c| names.contains(&c.as_str()));
                cols = (
                    find(&["book", "book_name", "osis", "usfm"]).unwrap_or(0),
                    find(&["chapter", "chap", "ch"]).unwrap_or(1),
                    find(&["verse", "vers", "v"]).unwrap_or(2),
                    find(&["text", "scripture", "content"]).unwrap_or(3),
                );
            } else {
                pending = Some(first);
            }
        }
        let mut out = Collector::default();
        for row in pending.into_iter().chain(rows) {
            let get = |i: usize| row.get(i).map(String::as_str).unwrap_or("");
            let name = get(cols.0);
            if name.trim().is_empty() {
                continue;
            }
            let Some(book) = lookup_book(name) else {
                out.unknown(name);
                continue;
            };
            let (Some(ch), Some(v)) = (parse_u16(get(cols.1)), parse_u16(get(cols.2))) else {
                continue;
            };
            out.push_plain(book, ch, v, get(cols.3));
        }
        out.finish("CSV", Declared::default(), &self.meta)
    }
}

/// RFC 4180 reader: quoted fields may contain the delimiter, quotes (`""`) and newlines.
fn parse_csv(text: &str, delim: char) -> Result<Vec<Vec<String>>, BibleError> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            in_quotes = true;
        } else if c == delim {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            if row.iter().any(|f| !f.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            field.push(c);
        }
    }
    if in_quotes {
        return Err(malformed("CSV: unterminated quoted field"));
    }
    row.push(field);
    if row.iter().any(|f| !f.is_empty()) {
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_scrollmapper_layout() {
        let j = r#"{"translation":"Test (1909)","books":[
          {"name":"John","chapters":[{"chapter":3,"verses":[{"verse":16,"text":"Porque así amó"},{"verse":17,"text":"Dios"}]}]},
          {"name":"Genesis","chapters":[{"chapter":1,"verses":[{"verse":1,"text":"En el principio"}]}]}]}"#;
        let b = JsonImporter::default().import(j.as_bytes()).unwrap();
        assert_eq!(b.version.name, "Test (1909)");
        assert_eq!(b.verses.len(), 3);
        assert_eq!(b.verses[0].reference.to_string(), "Gen.1.1");
    }

    #[test]
    fn json_array_of_arrays_flat_and_nested() {
        let a = r#"[{"abbrev":"gn","name":"Genesis","chapters":[["a","b"],["c"]]}]"#;
        let b = JsonImporter::default().import(a.as_bytes()).unwrap();
        assert_eq!(b.verses.len(), 3);
        assert_eq!(b.verses[2].reference.to_string(), "Gen.2.1");
        let f = r#"[{"book":"Jn","chapter":"3","verse":16,"text":"x"}]"#;
        let b = JsonImporter::default().import(f.as_bytes()).unwrap();
        assert_eq!(b.verses[0].reference.to_string(), "John.3.16");
        let n = r#"{"Genesis":{"1":{"1":"one","2":"two"}},"Exodus":{"1":{"1":"three"}}}"#;
        let b = JsonImporter::default().import(n.as_bytes()).unwrap();
        assert_eq!(b.verses.len(), 3);
    }

    #[test]
    fn json_errors() {
        for bad in [
            "",
            "null",
            "42",
            "[]",
            "{}",
            "{\"books\":[{\"name\":\"Narnia\",\"chapters\":[]}]}",
        ] {
            assert!(
                JsonImporter::default().import(bad.as_bytes()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn csv_with_header_quotes_and_alt_delimiter() {
        let c = "Book,Chapter,Verse,Text\nGenesis,1,1,\"In the beginning, God\"\nGenesis,1,2,\"He said \"\"hi\"\"\nsecond line\"\r\n";
        let b = CsvImporter::default().import(c.as_bytes()).unwrap();
        assert_eq!(b.verses.len(), 2);
        assert_eq!(b.verses[0].text, "In the beginning, God");
        assert_eq!(b.verses[1].text, "He said \"hi\" second line");
        let s = "text;book;chapter;verse\nhello;Jn;3;16\n";
        let b = CsvImporter::default().import(s.as_bytes()).unwrap();
        assert_eq!(b.verses[0].reference.to_string(), "John.3.16");
        let nohead = "Gen\t1\t1\tIn the beginning\n";
        let b = CsvImporter::default().import(nohead.as_bytes()).unwrap();
        assert_eq!(b.verses.len(), 1);
    }

    #[test]
    fn csv_errors() {
        assert!(CsvImporter::default().import(b"").is_err());
        assert!(CsvImporter::default()
            .import(b"Book,Chapter,Verse,Text\n\"unterminated")
            .is_err());
        assert!(CsvImporter::default().import(b"Narnia,1,1,x\n").is_err());
    }
}
