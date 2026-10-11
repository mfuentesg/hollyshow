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

fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

#[cfg(test)]
mod tests {
    use super::fold;

    #[test]
    fn strips_accents_and_case() {
        assert_eq!(fold("Señor"), "senor");
        assert_eq!(fold("ÁÉÍÓÚ Üü"), "aeiou uu");
        assert_eq!(fold("Cristo"), "cristo");
    }
}
