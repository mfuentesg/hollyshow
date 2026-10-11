//! Spanish to English keyword mapping for church terms, and the curated quick categories.
//! Providers index mostly English metadata, so "adoración" must be sent as "worship".

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

/// `(spanish phrase without accents, english keywords)`. Longer phrases are matched first.
const DICTIONARY: &[(&str, &str)] = &[
    ("santa cena", "communion bread wine"),
    ("semana santa", "easter"),
    ("viernes santo", "good friday cross"),
    ("espiritu santo", "holy spirit dove"),
    ("ultima cena", "last supper"),
    ("buen pastor", "good shepherd"),
    ("tierra santa", "holy land"),
    ("jesus cristo", "jesus christ"),
    ("cruz", "cross"),
    ("cruces", "cross"),
    ("crucifijo", "crucifix"),
    ("adoracion", "worship"),
    ("alabanza", "praise worship"),
    ("amanecer", "sunrise"),
    ("atardecer", "sunset"),
    ("puesta de sol", "sunset"),
    ("cielo", "sky clouds"),
    ("cielos", "sky clouds"),
    ("nubes", "clouds"),
    ("vitral", "stained glass"),
    ("vitrales", "stained glass"),
    ("navidad", "christmas nativity"),
    ("pesebre", "nativity manger"),
    ("belen", "bethlehem nativity"),
    ("pascua", "easter"),
    ("resurreccion", "resurrection easter"),
    ("bautismo", "baptism"),
    ("bautizo", "baptism"),
    ("comunion", "communion"),
    ("iglesia", "church"),
    ("templo", "church temple"),
    ("catedral", "cathedral"),
    ("capilla", "chapel"),
    ("biblia", "bible"),
    ("libro", "book"),
    ("oracion", "prayer"),
    ("orar", "prayer"),
    ("manos", "hands"),
    ("mano", "hand"),
    ("luz", "light"),
    ("rayos", "light rays"),
    ("vela", "candle"),
    ("velas", "candles"),
    ("fuego", "fire"),
    ("llama", "flame"),
    ("paloma", "dove"),
    ("cordero", "lamb"),
    ("pastor", "shepherd"),
    ("ovejas", "sheep"),
    ("pan", "bread"),
    ("vino", "wine"),
    ("copa", "chalice cup"),
    ("montana", "mountain"),
    ("montanas", "mountains"),
    ("monte", "mountain"),
    ("mar", "sea"),
    ("oceano", "ocean"),
    ("rio", "river"),
    ("lago", "lake"),
    ("bosque", "forest"),
    ("campo", "field meadow"),
    ("trigo", "wheat field"),
    ("desierto", "desert"),
    ("camino", "path road"),
    ("sendero", "path"),
    ("naturaleza", "nature landscape"),
    ("paisaje", "landscape"),
    ("flores", "flowers"),
    ("flor", "flower"),
    ("arbol", "tree"),
    ("arboles", "trees"),
    ("estrellas", "stars"),
    ("estrella", "star"),
    ("noche", "night"),
    ("luna", "moon"),
    ("sol", "sun"),
    ("agua", "water"),
    ("lluvia", "rain"),
    ("tormenta", "storm"),
    ("arcoiris", "rainbow"),
    ("fe", "faith"),
    ("esperanza", "hope"),
    ("amor", "love"),
    ("paz", "peace"),
    ("gracia", "grace"),
    ("gozo", "joy"),
    ("alegria", "joy"),
    ("familia", "family"),
    ("ninos", "children"),
    ("jovenes", "youth"),
    ("juventud", "youth"),
    ("musica", "music"),
    ("guitarra", "guitar"),
    ("piano", "piano"),
    ("coro", "choir"),
    ("pantalla", "screen"),
    ("fondo", "background"),
    ("abstracto", "abstract"),
    ("dorado", "golden"),
    ("azul", "blue"),
    ("jesus", "jesus"),
    ("dios", "god"),
    ("angel", "angel"),
    ("angeles", "angels"),
    ("reino", "kingdom"),
    ("corona", "crown"),
    ("espinas", "thorns"),
    ("tumba", "empty tomb"),
];

/// Words that carry no meaning in a keyword search.
const STOP_WORDS: &[&str] = &[
    "de", "del", "la", "el", "los", "las", "y", "e", "en", "un", "una", "unos", "unas", "con",
    "para", "por", "a", "al", "o",
];

fn fold(s: &str) -> String {
    s.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect::<String>()
        .to_lowercase()
}

/// Translates a Spanish query to English keywords. Words without a dictionary entry are kept
/// (they may already be English: "worship", "sunrise"); Spanish stop words are dropped.
/// Duplicated English words are removed, order preserved.
pub fn translate_query(query: &str) -> String {
    let words: Vec<String> = fold(query)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let mut out: Vec<String> = Vec::new();
    let push = |out: &mut Vec<String>, s: &str| {
        for w in s.split_whitespace() {
            if !out.iter().any(|x| x == w) {
                out.push(w.to_string());
            }
        }
    };
    let mut i = 0;
    'outer: while i < words.len() {
        for n in (1..=3).rev() {
            if i + n > words.len() {
                continue;
            }
            let phrase = words[i..i + n].join(" ");
            if let Some((_, en)) = DICTIONARY.iter().find(|(es, _)| *es == phrase) {
                push(&mut out, en);
                i += n;
                continue 'outer;
            }
        }
        let w = &words[i];
        if !STOP_WORDS.contains(&w.as_str()) {
            push(&mut out, w);
        }
        i += 1;
    }
    out.join(" ")
}

/// A curated quick category (a chip in the panel).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: &'static str,
    pub label_es: &'static str,
    pub label_en: &'static str,
    /// Spanish query shown in the search box when the chip is chosen.
    pub query_es: &'static str,
    /// English query sent to the providers.
    pub query_en: &'static str,
}

/// The eleven curated categories of PLAN §E2.
pub fn categories() -> Vec<Category> {
    const C: &[(&str, &str, &str, &str, &str)] = &[
        ("cross", "Cruz", "Cross", "cruz", "cross"),
        (
            "sky",
            "Cielo y nubes",
            "Sky and clouds",
            "cielo nubes",
            "sky clouds",
        ),
        ("sunrise", "Amanecer", "Sunrise", "amanecer", "sunrise"),
        (
            "nature",
            "Naturaleza",
            "Nature",
            "naturaleza paisaje",
            "nature landscape",
        ),
        ("church", "Iglesia", "Church", "iglesia", "church interior"),
        (
            "stained-glass",
            "Vitrales",
            "Stained glass",
            "vitrales",
            "stained glass",
        ),
        ("bible", "Biblia", "Bible", "biblia", "bible"),
        (
            "christmas",
            "Navidad",
            "Christmas",
            "navidad",
            "christmas nativity",
        ),
        (
            "easter",
            "Semana Santa",
            "Holy Week",
            "semana santa",
            "easter",
        ),
        ("baptism", "Bautismo", "Baptism", "bautismo", "baptism"),
        (
            "communion",
            "Santa Cena",
            "Communion",
            "santa cena",
            "communion bread wine",
        ),
    ];
    C.iter()
        .map(|&(id, label_es, label_en, query_es, query_en)| Category {
            id,
            label_es,
            label_en,
            query_es,
            query_en,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_church_terms() {
        assert_eq!(translate_query("cruz"), "cross");
        assert_eq!(translate_query("Adoración"), "worship");
        assert_eq!(translate_query("amanecer"), "sunrise");
        assert_eq!(translate_query("cielo"), "sky clouds");
        assert_eq!(translate_query("vitral"), "stained glass");
        assert_eq!(translate_query("Semana Santa"), "easter");
        assert_eq!(translate_query("Navidad"), "christmas nativity");
        assert_eq!(translate_query("bautismo"), "baptism");
        assert_eq!(translate_query("Santa Cena"), "communion bread wine");
        assert_eq!(translate_query("iglesia"), "church");
        assert_eq!(translate_query("Biblia"), "bible");
    }

    #[test]
    fn handles_phrases_stop_words_and_unknowns() {
        assert_eq!(translate_query("cruz en el cielo"), "cross sky clouds");
        assert_eq!(translate_query("manos de adoración"), "hands worship");
        assert_eq!(translate_query("lighthouse worship"), "lighthouse worship");
        assert_eq!(translate_query("cielo nubes"), "sky clouds");
        assert_eq!(translate_query("  "), "");
        assert_eq!(translate_query("de la y"), "");
    }

    #[test]
    fn categories_have_unique_ids_and_translate_consistently() {
        let cats = categories();
        assert_eq!(cats.len(), 11);
        let mut ids: Vec<_> = cats.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 11);
        for c in &cats {
            assert!(!c.query_en.is_empty());
        }
        assert_eq!(translate_query(cats[1].query_es), cats[1].query_en);
    }
}
