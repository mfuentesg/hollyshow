//! Canonical book list (OSIS ids), including the deuterocanon, with names and abbreviations in
//! Spanish, English and Portuguese.
//!
//! Order: the 66 books of the Protestant canon (1..=66, also the Zefania `bnumber`), followed by
//! the deuterocanonical books (67..). Every installed version stores its verses in this order, so
//! passages that cross books resolve the same way in every version.

use crate::canon_data::KJV_VERSE_COUNTS;
use hollyshow_core::text::fold;
use hollyshow_core::BookId;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Testament {
    Old,
    New,
    /// Deuterocanonical / apocryphal books.
    Deutero,
}

/// Languages with book-name tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Es,
    En,
    Pt,
}

impl Lang {
    /// Accepts BCP-47 tags (`es`, `es-MX`, `pt_BR`, `EN`). Unknown languages yield `None`.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let primary = tag
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match primary.as_str() {
            "es" | "spa" => Some(Lang::Es),
            "en" | "eng" => Some(Lang::En),
            "pt" | "por" => Some(Lang::Pt),
            _ => None,
        }
    }
}

/// One book of the canon. For each language `names[0]` is the full name, `names[1]` the
/// standard abbreviation and the rest are accepted aliases.
#[derive(Debug, Clone, Copy)]
pub struct Book {
    pub osis: &'static str,
    pub usfm: &'static str,
    /// 1-based position in the canonical order.
    pub ord: u8,
    pub chapters: u8,
    pub testament: Testament,
    es: &'static [&'static str],
    en: &'static [&'static str],
    pt: &'static [&'static str],
}

impl Book {
    pub fn names(&self, lang: Lang) -> &'static [&'static str] {
        match lang {
            Lang::Es => self.es,
            Lang::En => self.en,
            Lang::Pt => self.pt,
        }
    }
    pub fn name(&self, lang: Lang) -> &'static str {
        self.names(lang)[0]
    }
    pub fn abbreviation(&self, lang: Lang) -> &'static str {
        self.names(lang)[1]
    }
    pub fn is_deutero(&self) -> bool {
        self.testament == Testament::Deutero
    }
    /// Books with a single chapter, whose references omit the chapter ("Judas 3").
    pub fn is_single_chapter(&self) -> bool {
        self.chapters == 1
    }
}

macro_rules! book {
    ($osis:literal, $usfm:literal, $ord:literal, $ch:literal, $t:ident,
     [$($es:literal),+], [$($en:literal),+], [$($pt:literal),+]) => {
        Book {
            osis: $osis, usfm: $usfm, ord: $ord, chapters: $ch, testament: Testament::$t,
            es: &[$($es),+], en: &[$($en),+], pt: &[$($pt),+],
        }
    };
}

pub const BOOKS: &[Book] = &[
    book!(
        "Gen",
        "GEN",
        1,
        50,
        Old,
        ["Génesis", "Gn", "Gen", "Gé"],
        ["Genesis", "Gen", "Gn", "Ge"],
        ["Gênesis", "Gn", "Gen"]
    ),
    book!(
        "Exod",
        "EXO",
        2,
        40,
        Old,
        ["Éxodo", "Éx", "Ex", "Exo"],
        ["Exodus", "Ex", "Exod", "Exo"],
        ["Êxodo", "Êx", "Ex", "Exo"]
    ),
    book!(
        "Lev",
        "LEV",
        3,
        27,
        Old,
        ["Levítico", "Lv", "Lev", "Le"],
        ["Leviticus", "Lev", "Lv", "Le"],
        ["Levítico", "Lv", "Lev"]
    ),
    book!(
        "Num",
        "NUM",
        4,
        36,
        Old,
        ["Números", "Nm", "Nú", "Num", "Nu"],
        ["Numbers", "Num", "Nm", "Nu", "Nb"],
        ["Números", "Nm", "Num"]
    ),
    book!(
        "Deut",
        "DEU",
        5,
        34,
        Old,
        ["Deuteronomio", "Dt", "Deut", "Deu"],
        ["Deuteronomy", "Deut", "Dt", "De"],
        ["Deuteronômio", "Dt", "Deut"]
    ),
    book!(
        "Josh",
        "JOS",
        6,
        24,
        Old,
        ["Josué", "Jos", "Js"],
        ["Joshua", "Josh", "Jos", "Jsh"],
        ["Josué", "Js", "Jos"]
    ),
    book!(
        "Judg",
        "JDG",
        7,
        21,
        Old,
        ["Jueces", "Jue", "Jc", "Jz"],
        ["Judges", "Judg", "Jdg", "Jg", "Jdgs"],
        ["Juízes", "Jz", "Juiz"]
    ),
    book!(
        "Ruth",
        "RUT",
        8,
        4,
        Old,
        ["Rut", "Rt", "Ru"],
        ["Ruth", "Ru", "Rth", "Rt"],
        ["Rute", "Rt", "Rut"]
    ),
    book!(
        "1Sam",
        "1SA",
        9,
        31,
        Old,
        ["1 Samuel", "1 S", "1 Sam", "1 Sm", "1 Sa"],
        ["1 Samuel", "1 Sam", "1 Sm", "1 Sa", "1 S"],
        ["1 Samuel", "1 Sm", "1 Sam", "1 Sa"]
    ),
    book!(
        "2Sam",
        "2SA",
        10,
        24,
        Old,
        ["2 Samuel", "2 S", "2 Sam", "2 Sm", "2 Sa"],
        ["2 Samuel", "2 Sam", "2 Sm", "2 Sa", "2 S"],
        ["2 Samuel", "2 Sm", "2 Sam", "2 Sa"]
    ),
    book!(
        "1Kgs",
        "1KI",
        11,
        22,
        Old,
        ["1 Reyes", "1 R", "1 Re", "1 Rey", "1 Rs"],
        ["1 Kings", "1 Kgs", "1 Ki", "1 K", "1 Kg"],
        ["1 Reis", "1 Rs", "1 Re"]
    ),
    book!(
        "2Kgs",
        "2KI",
        12,
        25,
        Old,
        ["2 Reyes", "2 R", "2 Re", "2 Rey", "2 Rs"],
        ["2 Kings", "2 Kgs", "2 Ki", "2 K", "2 Kg"],
        ["2 Reis", "2 Rs", "2 Re"]
    ),
    book!(
        "1Chr",
        "1CH",
        13,
        29,
        Old,
        ["1 Crónicas", "1 Cr", "1 Cró", "1 Cron", "1 Crón"],
        ["1 Chronicles", "1 Chr", "1 Ch", "1 Chron"],
        ["1 Crônicas", "1 Cr", "1 Crô", "1 Cron"]
    ),
    book!(
        "2Chr",
        "2CH",
        14,
        36,
        Old,
        ["2 Crónicas", "2 Cr", "2 Cró", "2 Cron", "2 Crón"],
        ["2 Chronicles", "2 Chr", "2 Ch", "2 Chron"],
        ["2 Crônicas", "2 Cr", "2 Crô", "2 Cron"]
    ),
    book!(
        "Ezra",
        "EZR",
        15,
        10,
        Old,
        ["Esdras", "Esd", "Esdr"],
        ["Ezra", "Ezr", "Ez"],
        ["Esdras", "Ed", "Esd"]
    ),
    book!(
        "Neh",
        "NEH",
        16,
        13,
        Old,
        ["Nehemías", "Neh", "Ne"],
        ["Nehemiah", "Neh", "Ne"],
        ["Neemias", "Ne", "Nee"]
    ),
    book!(
        "Esth",
        "EST",
        17,
        10,
        Old,
        ["Ester", "Est", "Es"],
        ["Esther", "Esth", "Est", "Es"],
        ["Ester", "Et", "Est"]
    ),
    book!(
        "Job",
        "JOB",
        18,
        42,
        Old,
        ["Job", "Jb"],
        ["Job", "Jb"],
        ["Jó", "Jb", "Job"]
    ),
    book!(
        "Ps",
        "PSA",
        19,
        150,
        Old,
        ["Salmos", "Sal", "Salmo", "Slm", "Sl"],
        ["Psalms", "Ps", "Psa", "Psalm", "Pss", "Psm"],
        ["Salmos", "Sl", "Sal", "Salmo"]
    ),
    book!(
        "Prov",
        "PRO",
        20,
        31,
        Old,
        ["Proverbios", "Pr", "Prov", "Pro"],
        ["Proverbs", "Prov", "Pr", "Prv"],
        ["Provérbios", "Pv", "Prov", "Pr"]
    ),
    book!(
        "Eccl",
        "ECC",
        21,
        12,
        Old,
        ["Eclesiastés", "Ec", "Ecl", "Eclesiastes", "Qohélet"],
        ["Ecclesiastes", "Eccl", "Ecc", "Ec", "Qoh"],
        ["Eclesiastes", "Ec", "Ecl"]
    ),
    book!(
        "Song",
        "SNG",
        22,
        8,
        Old,
        [
            "Cantares",
            "Cnt",
            "Cant",
            "Cantar de los Cantares",
            "Cantar",
            "Ct",
            "Cantico de los Cantares"
        ],
        [
            "Song of Solomon",
            "Song",
            "SS",
            "Song of Songs",
            "Canticles",
            "Cant",
            "SoS"
        ],
        [
            "Cânticos",
            "Ct",
            "Cântico dos Cânticos",
            "Cantares",
            "Cant",
            "Cantares de Salomão"
        ]
    ),
    book!(
        "Isa",
        "ISA",
        23,
        66,
        Old,
        ["Isaías", "Is", "Isa"],
        ["Isaiah", "Isa", "Is"],
        ["Isaías", "Is", "Isa"]
    ),
    book!(
        "Jer",
        "JER",
        24,
        52,
        Old,
        ["Jeremías", "Jer", "Jr"],
        ["Jeremiah", "Jer", "Je", "Jr"],
        ["Jeremias", "Jr", "Jer"]
    ),
    book!(
        "Lam",
        "LAM",
        25,
        5,
        Old,
        ["Lamentaciones", "Lm", "Lam"],
        ["Lamentations", "Lam", "La"],
        ["Lamentações", "Lm", "Lam"]
    ),
    book!(
        "Ezek",
        "EZK",
        26,
        48,
        Old,
        ["Ezequiel", "Ez", "Eze", "Ezeq"],
        ["Ezekiel", "Ezek", "Eze", "Ezk"],
        ["Ezequiel", "Ez", "Eze"]
    ),
    book!(
        "Dan",
        "DAN",
        27,
        12,
        Old,
        ["Daniel", "Dn", "Dan"],
        ["Daniel", "Dan", "Dn", "Da"],
        ["Daniel", "Dn", "Dan"]
    ),
    book!(
        "Hos",
        "HOS",
        28,
        14,
        Old,
        ["Oseas", "Os", "Ose"],
        ["Hosea", "Hos", "Ho"],
        ["Oseias", "Os", "Ose"]
    ),
    book!(
        "Joel",
        "JOL",
        29,
        3,
        Old,
        ["Joel", "Jl", "Joe"],
        ["Joel", "Jl", "Joe"],
        ["Joel", "Jl", "Joe"]
    ),
    book!(
        "Amos",
        "AMO",
        30,
        9,
        Old,
        ["Amós", "Am", "Amos"],
        ["Amos", "Am"],
        ["Amós", "Am", "Amos"]
    ),
    book!(
        "Obad",
        "OBA",
        31,
        1,
        Old,
        ["Abdías", "Abd", "Abdias"],
        ["Obadiah", "Obad", "Ob", "Oba"],
        ["Obadias", "Ob", "Abd"]
    ),
    book!(
        "Jonah",
        "JON",
        32,
        4,
        Old,
        ["Jonás", "Jon", "Jonas"],
        ["Jonah", "Jon", "Jnh"],
        ["Jonas", "Jn", "Jon"]
    ),
    book!(
        "Mic",
        "MIC",
        33,
        7,
        Old,
        ["Miqueas", "Miq", "Mi"],
        ["Micah", "Mic", "Mi"],
        ["Miqueias", "Mq", "Miq"]
    ),
    book!(
        "Nah",
        "NAM",
        34,
        3,
        Old,
        ["Nahúm", "Nah", "Na", "Nahum"],
        ["Nahum", "Nah", "Na"],
        ["Naum", "Na", "Nau"]
    ),
    book!(
        "Hab",
        "HAB",
        35,
        3,
        Old,
        ["Habacuc", "Hab", "Hc"],
        ["Habakkuk", "Hab", "Hb"],
        ["Habacuque", "Hc", "Hab"]
    ),
    book!(
        "Zeph",
        "ZEP",
        36,
        3,
        Old,
        ["Sofonías", "Sof", "So", "Sofonias"],
        ["Zephaniah", "Zeph", "Zep", "Zp"],
        ["Sofonias", "Sf", "Sof"]
    ),
    book!(
        "Hag",
        "HAG",
        37,
        2,
        Old,
        ["Hageo", "Hag", "Hg"],
        ["Haggai", "Hag", "Hg"],
        ["Ageu", "Ag", "Age"]
    ),
    book!(
        "Zech",
        "ZEC",
        38,
        14,
        Old,
        ["Zacarías", "Zac", "Zc", "Zacarias"],
        ["Zechariah", "Zech", "Zec", "Zc"],
        ["Zacarias", "Zc", "Zac"]
    ),
    book!(
        "Mal",
        "MAL",
        39,
        4,
        Old,
        ["Malaquías", "Mal", "Ml", "Malaquias"],
        ["Malachi", "Mal", "Ml"],
        ["Malaquias", "Ml", "Mal"]
    ),
    book!(
        "Matt",
        "MAT",
        40,
        28,
        New,
        ["Mateo", "Mt", "Mat"],
        ["Matthew", "Matt", "Mt", "Mat"],
        ["Mateus", "Mt", "Mat"]
    ),
    book!(
        "Mark",
        "MRK",
        41,
        16,
        New,
        ["Marcos", "Mr", "Mc", "Mar", "Marc"],
        ["Mark", "Mk", "Mrk", "Mr"],
        ["Marcos", "Mc", "Mar"]
    ),
    book!(
        "Luke",
        "LUK",
        42,
        24,
        New,
        ["Lucas", "Lc", "Luc"],
        ["Luke", "Lk", "Luk", "Lu"],
        ["Lucas", "Lc", "Luc"]
    ),
    book!(
        "John",
        "JHN",
        43,
        21,
        New,
        ["Juan", "Jn", "Jua"],
        ["John", "Jn", "Jhn", "Joh"],
        ["João", "Jo", "Joao"]
    ),
    book!(
        "Acts",
        "ACT",
        44,
        28,
        New,
        ["Hechos", "Hch", "Hech", "He", "Hechos de los Apóstoles"],
        ["Acts", "Ac", "Act", "Acts of the Apostles"],
        ["Atos", "At", "Atos dos Apóstolos"]
    ),
    book!(
        "Rom",
        "ROM",
        45,
        16,
        New,
        ["Romanos", "Ro", "Rom", "Rm"],
        ["Romans", "Rom", "Ro", "Rm"],
        ["Romanos", "Rm", "Rom"]
    ),
    book!(
        "1Cor",
        "1CO",
        46,
        16,
        New,
        ["1 Corintios", "1 Co", "1 Cor"],
        ["1 Corinthians", "1 Cor", "1 Co"],
        ["1 Coríntios", "1 Co", "1 Cor"]
    ),
    book!(
        "2Cor",
        "2CO",
        47,
        13,
        New,
        ["2 Corintios", "2 Co", "2 Cor"],
        ["2 Corinthians", "2 Cor", "2 Co"],
        ["2 Coríntios", "2 Co", "2 Cor"]
    ),
    book!(
        "Gal",
        "GAL",
        48,
        6,
        New,
        ["Gálatas", "Gá", "Gal", "Ga"],
        ["Galatians", "Gal", "Ga"],
        ["Gálatas", "Gl", "Gal"]
    ),
    book!(
        "Eph",
        "EPH",
        49,
        6,
        New,
        ["Efesios", "Ef", "Efe"],
        ["Ephesians", "Eph", "Ep", "Ephes"],
        ["Efésios", "Ef", "Efe"]
    ),
    book!(
        "Phil",
        "PHP",
        50,
        4,
        New,
        ["Filipenses", "Fil", "Flp", "Fp"],
        ["Philippians", "Phil", "Php", "Pp"],
        ["Filipenses", "Fp", "Fil", "Flp"]
    ),
    book!(
        "Col",
        "COL",
        51,
        4,
        New,
        ["Colosenses", "Col", "Cl"],
        ["Colossians", "Col", "Co"],
        ["Colossenses", "Cl", "Col"]
    ),
    book!(
        "1Thess",
        "1TH",
        52,
        5,
        New,
        ["1 Tesalonicenses", "1 Ts", "1 Tes", "1 Tesal"],
        ["1 Thessalonians", "1 Thess", "1 Th", "1 Thes"],
        ["1 Tessalonicenses", "1 Ts", "1 Tes"]
    ),
    book!(
        "2Thess",
        "2TH",
        53,
        3,
        New,
        ["2 Tesalonicenses", "2 Ts", "2 Tes", "2 Tesal"],
        ["2 Thessalonians", "2 Thess", "2 Th", "2 Thes"],
        ["2 Tessalonicenses", "2 Ts", "2 Tes"]
    ),
    book!(
        "1Tim",
        "1TI",
        54,
        6,
        New,
        ["1 Timoteo", "1 Ti", "1 Tim", "1 Tm"],
        ["1 Timothy", "1 Tim", "1 Ti", "1 Tm"],
        ["1 Timóteo", "1 Tm", "1 Tim", "1 Ti"]
    ),
    book!(
        "2Tim",
        "2TI",
        55,
        4,
        New,
        ["2 Timoteo", "2 Ti", "2 Tim", "2 Tm"],
        ["2 Timothy", "2 Tim", "2 Ti", "2 Tm"],
        ["2 Timóteo", "2 Tm", "2 Tim", "2 Ti"]
    ),
    book!(
        "Titus",
        "TIT",
        56,
        3,
        New,
        ["Tito", "Tit", "Tt"],
        ["Titus", "Tit", "Ti"],
        ["Tito", "Tt", "Tit"]
    ),
    book!(
        "Phlm",
        "PHM",
        57,
        1,
        New,
        ["Filemón", "Flm", "Filem", "Filemon", "Fm"],
        ["Philemon", "Phlm", "Phm", "Pm"],
        ["Filemom", "Fm", "Flm", "Filem"]
    ),
    book!(
        "Heb",
        "HEB",
        58,
        13,
        New,
        ["Hebreos", "Heb"],
        ["Hebrews", "Heb", "He"],
        ["Hebreus", "Hb", "Heb"]
    ),
    book!(
        "Jas",
        "JAS",
        59,
        5,
        New,
        ["Santiago", "Stg", "Sant", "Snt", "Sg"],
        ["James", "Jas", "Jm", "Jam"],
        ["Tiago", "Tg", "Tia"]
    ),
    book!(
        "1Pet",
        "1PE",
        60,
        5,
        New,
        ["1 Pedro", "1 P", "1 Pe", "1 Ped"],
        ["1 Peter", "1 Pet", "1 Pe", "1 Pt", "1 P"],
        ["1 Pedro", "1 Pe", "1 Pd", "1 Ped"]
    ),
    book!(
        "2Pet",
        "2PE",
        61,
        3,
        New,
        ["2 Pedro", "2 P", "2 Pe", "2 Ped"],
        ["2 Peter", "2 Pet", "2 Pe", "2 Pt", "2 P"],
        ["2 Pedro", "2 Pe", "2 Pd", "2 Ped"]
    ),
    book!(
        "1John",
        "1JN",
        62,
        5,
        New,
        ["1 Juan", "1 Jn", "1 Jua"],
        ["1 John", "1 Jn", "1 Jhn", "1 Jo"],
        ["1 João", "1 Jo", "1 Joao"]
    ),
    book!(
        "2John",
        "2JN",
        63,
        1,
        New,
        ["2 Juan", "2 Jn", "2 Jua"],
        ["2 John", "2 Jn", "2 Jhn", "2 Jo"],
        ["2 João", "2 Jo", "2 Joao"]
    ),
    book!(
        "3John",
        "3JN",
        64,
        1,
        New,
        ["3 Juan", "3 Jn", "3 Jua"],
        ["3 John", "3 Jn", "3 Jhn", "3 Jo"],
        ["3 João", "3 Jo", "3 Joao"]
    ),
    book!(
        "Jude",
        "JUD",
        65,
        1,
        New,
        ["Judas", "Jud", "Jds"],
        ["Jude", "Jud", "Jd"],
        ["Judas", "Jd", "Jud"]
    ),
    book!(
        "Rev",
        "REV",
        66,
        22,
        New,
        ["Apocalipsis", "Ap", "Apoc", "Apo", "Rev"],
        [
            "Revelation",
            "Rev",
            "Re",
            "Revelation of John",
            "Apocalypse",
            "Rv"
        ],
        ["Apocalipse", "Ap", "Apoc"]
    ),
    // Deuterocanon.
    book!(
        "Tob",
        "TOB",
        67,
        14,
        Deutero,
        ["Tobías", "Tob", "Tb", "Tobias"],
        ["Tobit", "Tob", "Tb"],
        ["Tobias", "Tb", "Tob"]
    ),
    book!(
        "Jdt",
        "JDT",
        68,
        16,
        Deutero,
        ["Judit", "Jdt"],
        ["Judith", "Jdt", "Jth"],
        ["Judite", "Jt", "Jdt"]
    ),
    book!(
        "AddEsth",
        "ESG",
        69,
        16,
        Deutero,
        ["Adiciones a Ester", "Est gr", "Ester griego", "AdEst"],
        [
            "Additions to Esther",
            "Add Esth",
            "AddEsth",
            "Rest of Esther"
        ],
        ["Adições a Ester", "Ad Et", "AdEst"]
    ),
    book!(
        "Wis",
        "WIS",
        70,
        19,
        Deutero,
        ["Sabiduría", "Sab", "Sabiduria"],
        ["Wisdom of Solomon", "Wis", "Wisdom", "Ws"],
        ["Sabedoria", "Sb", "Sab"]
    ),
    book!(
        "Sir",
        "SIR",
        71,
        51,
        Deutero,
        ["Eclesiástico", "Eclo", "Sir", "Sirácida", "Siracida"],
        ["Sirach", "Sir", "Ecclesiasticus", "Ecclus"],
        ["Eclesiástico", "Sr", "Sir", "Eclo"]
    ),
    book!(
        "Bar",
        "BAR",
        72,
        6,
        Deutero,
        ["Baruc", "Bar", "Ba"],
        ["Baruch", "Bar", "Ba"],
        ["Baruque", "Br", "Bar"]
    ),
    book!(
        "EpJer",
        "LJE",
        73,
        1,
        Deutero,
        ["Carta de Jeremías", "Carta Jer", "CartJer", "Ep Jer"],
        ["Letter of Jeremiah", "Ep Jer", "EpJer", "Let Jer"],
        ["Carta de Jeremias", "Carta Jer", "Ep Jr"]
    ),
    book!(
        "PrAzar",
        "S3Y",
        74,
        1,
        Deutero,
        [
            "Oración de Azarías",
            "Or Az",
            "Azarías",
            "Cántico de los tres jóvenes"
        ],
        [
            "Prayer of Azariah",
            "Pr Azar",
            "PrAzar",
            "Song of the Three"
        ],
        ["Oração de Azarias", "Or Az", "Cântico dos três jovens"]
    ),
    book!(
        "Sus",
        "SUS",
        75,
        1,
        Deutero,
        ["Susana", "Sus"],
        ["Susanna", "Sus"],
        ["Susana", "Sus"]
    ),
    book!(
        "Bel",
        "BEL",
        76,
        1,
        Deutero,
        ["Bel y el Dragón", "Bel", "Bel y el Dragon"],
        ["Bel and the Dragon", "Bel"],
        ["Bel e o Dragão", "Bel", "Bel e o Dragao"]
    ),
    book!(
        "1Macc",
        "1MA",
        77,
        16,
        Deutero,
        ["1 Macabeos", "1 Mac", "1 M", "1 Mc"],
        ["1 Maccabees", "1 Macc", "1 Mac", "1 Ma"],
        ["1 Macabeus", "1 Mc", "1 Mac"]
    ),
    book!(
        "2Macc",
        "2MA",
        78,
        15,
        Deutero,
        ["2 Macabeos", "2 Mac", "2 M", "2 Mc"],
        ["2 Maccabees", "2 Macc", "2 Mac", "2 Ma"],
        ["2 Macabeus", "2 Mc", "2 Mac"]
    ),
    book!(
        "3Macc",
        "3MA",
        79,
        7,
        Deutero,
        ["3 Macabeos", "3 Mac", "3 M", "3 Mc"],
        ["3 Maccabees", "3 Macc", "3 Mac", "3 Ma"],
        ["3 Macabeus", "3 Mc", "3 Mac"]
    ),
    book!(
        "4Macc",
        "4MA",
        80,
        18,
        Deutero,
        ["4 Macabeos", "4 Mac", "4 M", "4 Mc"],
        ["4 Maccabees", "4 Macc", "4 Mac", "4 Ma"],
        ["4 Macabeus", "4 Mc", "4 Mac"]
    ),
    book!(
        "1Esd",
        "1ES",
        81,
        9,
        Deutero,
        ["1 Esdras", "1 Esd"],
        ["1 Esdras", "1 Esd"],
        ["1 Esdras", "1 Esd"]
    ),
    book!(
        "2Esd",
        "2ES",
        82,
        16,
        Deutero,
        ["2 Esdras", "2 Esd"],
        ["2 Esdras", "2 Esd"],
        ["2 Esdras", "2 Esd"]
    ),
    book!(
        "PrMan",
        "MAN",
        83,
        1,
        Deutero,
        ["Oración de Manasés", "Or Man", "Manasés"],
        ["Prayer of Manasseh", "Pr Man", "PrMan"],
        ["Oração de Manassés", "Or Man", "Manassés"]
    ),
    book!(
        "Ps151",
        "PS2",
        84,
        1,
        Deutero,
        ["Salmo 151", "Sal 151"],
        ["Psalm 151", "Ps 151"],
        ["Salmo 151", "Sl 151"]
    ),
];

/// The 66 books of the Protestant canon.
pub const PROTESTANT_BOOKS: usize = 66;

pub fn book(osis: &str) -> Option<&'static Book> {
    BOOKS.iter().find(|b| b.osis == osis)
}

pub fn book_by_ord(ord: usize) -> Option<&'static Book> {
    ord.checked_sub(1).and_then(|i| BOOKS.get(i))
}

pub fn book_by_usfm(code: &str) -> Option<&'static Book> {
    let code = code.trim().to_ascii_uppercase();
    BOOKS.iter().find(|b| b.usfm == code)
}

/// Position of `book` in the canonical order (`None` for unknown ids).
pub fn book_ord(book: &BookId) -> Option<usize> {
    self::book(&book.0).map(|b| usize::from(b.ord))
}

/// Localised full name of an OSIS book id; unknown ids are returned unchanged.
pub fn display_name(osis: &str, lang: Lang) -> String {
    book(osis).map_or_else(|| osis.to_owned(), |b| b.name(lang).to_owned())
}

/// Number of chapters of `osis` (`None` for unknown books).
pub fn chapter_count(osis: &str) -> Option<u16> {
    book(osis).map(|b| u16::from(b.chapters))
}

/// Verses in `chapter` of `osis` in the standard (KJV) versification. Only the 66 canonical
/// books are tabulated; deuterocanonical chapters return `None`.
pub fn verse_count(osis: &str, chapter: u16) -> Option<u16> {
    let b = book(osis)?;
    let counts = KJV_VERSE_COUNTS.get(usize::from(b.ord).checked_sub(1)?)?;
    let idx = usize::from(chapter.checked_sub(1)?);
    counts.get(idx).copied().map(u16::from)
}

/// Total verses of the 66-book standard canon (31,102).
pub fn standard_verse_total() -> u32 {
    KJV_VERSE_COUNTS
        .iter()
        .flat_map(|b| b.iter())
        .map(|n| u32::from(*n))
        .sum()
}

/// Normalises a book name into a lookup key: folded, ordinal prefix turned into a digit
/// (`I`, `1º`, `primera`, `first` -> `1`), everything that is not a letter or digit removed.
pub fn book_key(input: &str) -> String {
    let folded = fold(input.trim()).replace(['\u{ba}', '\u{aa}'], "");
    let folded: String = folded
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut words: Vec<&str> = folded.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    // Numeral prefix as a separate word ("1 sam", "i juan", "primera juan").
    let mut prefix: Option<char> = None;
    if let Some(p) = ordinal_word(words[0]) {
        if words.len() > 1 {
            prefix = Some(p);
            words.remove(0);
        }
    }
    let mut key = String::new();
    if let Some(p) = prefix {
        key.push(p);
    }
    for w in words {
        // "Primera de Juan", "First of John".
        if prefix.is_some() && key.len() == 1 && matches!(w, "de" | "del" | "of") {
            continue;
        }
        key.push_str(w);
    }
    key
}

fn ordinal_word(w: &str) -> Option<char> {
    Some(match w {
        "1" | "i" | "1o" | "1a" | "1st" | "1ra" | "1ro" | "1er" | "primero" | "primera"
        | "primeiro" | "primeira" | "first" | "1e" => '1',
        "2" | "ii" | "2o" | "2a" | "2nd" | "2da" | "2do" | "segundo" | "segunda" | "second" => '2',
        "3" | "iii" | "3o" | "3a" | "3rd" | "3ra" | "3ro" | "3er" | "tercero" | "tercera"
        | "terceiro" | "terceira" | "third" => '3',
        "4" | "iv" | "4o" | "4a" | "4th" | "4ta" | "4to" | "cuarto" | "cuarta" | "quarto"
        | "quarta" | "fourth" => '4',
        _ => return None,
    })
}

type AliasMap = HashMap<String, &'static Book>;

struct Aliases {
    by_lang: [AliasMap; 3],
}

fn lang_index(l: Lang) -> usize {
    match l {
        Lang::Es => 0,
        Lang::En => 1,
        Lang::Pt => 2,
    }
}

fn aliases() -> &'static Aliases {
    static CELL: OnceLock<Aliases> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut by_lang: [AliasMap; 3] = [HashMap::new(), HashMap::new(), HashMap::new()];
        for lang in [Lang::Es, Lang::En, Lang::Pt] {
            let map = &mut by_lang[lang_index(lang)];
            for b in BOOKS {
                for n in b.names(lang) {
                    // "Jó" is special-cased in `resolve_book`: its folded key is João's "Jo".
                    if *n == "Jó" {
                        continue;
                    }
                    map.entry(book_key(n)).or_insert(b);
                }
                // The OSIS id and USFM code are accepted everywhere.
                map.entry(book_key(b.osis)).or_insert(b);
                map.entry(book_key(b.usfm)).or_insert(b);
            }
        }
        Aliases { by_lang }
    })
}

/// Resolves a book name or abbreviation ("Juan", "jn", "1 Cor", "I Juan", "Cantar de los
/// Cantares") to its book. `lang` is a hint that wins when an abbreviation means different books
/// in different languages ("Jn" is Juan in Spanish, Jonas in Portuguese); without a usable hint
/// Spanish, English then Portuguese are tried in turn. A unique prefix of at least three letters
/// of a full name is accepted as a last resort ("apoc" for Apocalipsis).
pub fn resolve_book(name: &str, lang: Option<Lang>) -> Option<&'static Book> {
    let key = book_key(name);
    if key.is_empty() {
        return None;
    }
    // "Jó" (Job in Portuguese) loses its accent in the key and would clash with "Jo" (João).
    if name.trim().to_lowercase() == "jó" {
        return book("Job");
    }
    let a = aliases();
    let mut order: Vec<Lang> = lang.into_iter().collect();
    for l in [Lang::Es, Lang::En, Lang::Pt] {
        if !order.contains(&l) {
            order.push(l);
        }
    }
    for l in &order {
        if let Some(b) = a.by_lang[lang_index(*l)].get(&key) {
            return Some(b);
        }
    }
    // Unique-prefix fallback (letters only, after the numeral).
    let letters = key.trim_start_matches(|c: char| c.is_ascii_digit());
    if letters.chars().count() < 3 {
        return None;
    }
    let mut found: Option<&'static Book> = None;
    for l in &order {
        for (k, b) in &a.by_lang[lang_index(*l)] {
            if k.starts_with(&key) {
                match found {
                    None => found = Some(b),
                    Some(f) if f.osis == b.osis => {}
                    Some(_) => return None,
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_consistent() {
        assert_eq!(BOOKS.len(), 84);
        for (i, b) in BOOKS.iter().enumerate() {
            assert_eq!(usize::from(b.ord), i + 1, "{}", b.osis);
            for l in [Lang::Es, Lang::En, Lang::Pt] {
                assert!(b.names(l).len() >= 2, "{} {l:?}", b.osis);
            }
            assert_eq!((i < PROTESTANT_BOOKS), !b.is_deutero(), "{}", b.osis);
        }
        let mut ids: Vec<_> = BOOKS.iter().map(|b| b.osis).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BOOKS.len());
        let mut usfm: Vec<_> = BOOKS.iter().map(|b| b.usfm).collect();
        usfm.sort_unstable();
        usfm.dedup();
        assert_eq!(usfm.len(), BOOKS.len());
    }

    #[test]
    fn chapter_counts_match_verse_table() {
        for b in &BOOKS[..PROTESTANT_BOOKS] {
            let n = KJV_VERSE_COUNTS[usize::from(b.ord) - 1].len();
            assert_eq!(n, usize::from(b.chapters), "{}", b.osis);
        }
        assert_eq!(standard_verse_total(), 31_102);
        assert_eq!(verse_count("John", 3), Some(36));
        assert_eq!(verse_count("Ps", 119), Some(176));
        assert_eq!(verse_count("Tob", 1), None);
        assert_eq!(chapter_count("Tob"), Some(14));
    }

    #[test]
    fn keys_normalise_numerals_and_accents() {
        for s in ["1 Juan", "I Juan", "1Jn", "1 jn", "Primera Juan", "1º Juan"] {
            let k = book_key(s);
            assert!(k == "1juan" || k == "1jn", "{s} -> {k}");
        }
        assert_eq!(book_key("Cantar de los Cantares"), "cantardeloscantares");
        assert_eq!(book_key("Génesis"), "genesis");
        assert_eq!(book_key("Ii Reyes"), "2reyes");
        assert_eq!(book_key("1ra Corintios"), "1corintios");
        // A real name that starts with an ordinal-looking word is left alone.
        assert_eq!(book_key("Is"), "is");
        assert_eq!(book_key("Ex"), "ex");
    }

    #[test]
    fn resolves_names_in_three_languages() {
        let osis = |s: &str, l| resolve_book(s, l).map(|b| b.osis);
        assert_eq!(osis("Juan", Some(Lang::Es)), Some("John"));
        assert_eq!(osis("JOÃO", None), Some("John"));
        assert_eq!(osis("john", None), Some("John"));
        assert_eq!(osis("I Juan", None), Some("1John"));
        assert_eq!(osis("1 Co", None), Some("1Cor"));
        assert_eq!(osis("Sal", None), Some("Ps"));
        assert_eq!(osis("Ap", Some(Lang::Es)), Some("Rev"));
        assert_eq!(osis("Hch", None), Some("Acts"));
        assert_eq!(osis("Cantares", None), Some("Song"));
        assert_eq!(osis("Revelation of John", None), Some("Rev"));
        assert_eq!(osis("2 Macabeos", None), Some("2Macc"));
        assert_eq!(osis("apoc", None), Some("Rev"));
        assert_eq!(osis("xyz", None), None);
        assert_eq!(osis("", None), None);
    }

    #[test]
    fn language_hint_disambiguates() {
        assert_eq!(resolve_book("Jn", Some(Lang::Es)).unwrap().osis, "John");
        assert_eq!(resolve_book("Jn", Some(Lang::Pt)).unwrap().osis, "Jonah");
        assert_eq!(resolve_book("Jo", Some(Lang::Pt)).unwrap().osis, "John");
        assert_eq!(resolve_book("Jó", Some(Lang::Pt)).unwrap().osis, "Job");
    }

    #[test]
    fn every_full_name_resolves_to_itself() {
        for b in BOOKS {
            for l in [Lang::Es, Lang::En, Lang::Pt] {
                for n in b.names(l) {
                    // Ambiguous abbreviations resolve through the hint language.
                    let r = resolve_book(n, Some(l)).unwrap_or_else(|| panic!("{n}"));
                    // "Jo" is João in Portuguese; Job's own name is "Jó" which is special-cased.
                    assert_eq!(r.osis, b.osis, "{n} ({l:?})");
                }
            }
        }
    }

    #[test]
    fn lang_tags() {
        assert_eq!(Lang::from_tag("es-MX"), Some(Lang::Es));
        assert_eq!(Lang::from_tag("pt_BR"), Some(Lang::Pt));
        assert_eq!(Lang::from_tag("fr"), None);
    }
}
