//! Text normalisation shared by search (songs FTS, Bible FTS, command palette).

use unicode_normalization::UnicodeNormalization;

/// Lowercases and strips diacritics so "Señor" matches "senor" and "SEÑOR".
///
/// `ñ` is folded to `n` on purpose: users on keyboards without accents type "senor".
pub fn fold(input: &str) -> String {
    input
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

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
    // An unterminated `[` swallowed the rest of the line: keep what followed it as text.
    if in_chord {
        if let Some(i) = line.rfind('[') {
            out.push_str(&line[i..]);
        }
    }
    out
}

fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

#[cfg(test)]
mod tests {
    use super::{fold, strip_chords};

    #[test]
    fn strips_chords_and_keeps_unterminated_bracket() {
        assert_eq!(strip_chords("[G]Sublime [D/F#]gracia"), "Sublime gracia");
        assert_eq!(strip_chords("sin acordes"), "sin acordes");
        assert_eq!(strip_chords("a [b"), "a [b");
    }

    #[test]
    fn strips_accents_and_case() {
        assert_eq!(fold("Señor"), "senor");
        assert_eq!(fold("ÁÉÍÓÚ Üü"), "aeiou uu");
        assert_eq!(fold("Cristo"), "cristo");
    }
}
