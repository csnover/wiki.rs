//! Language conversion for the Shilha language.

use icu_locale::{LanguageIdentifier, langid};
use std::borrow::Cow;

/// A language conversion function for the Shilha language.
pub fn convert<'a>(text: &'a str, to: &icu_locale::Locale) -> Cow<'a, str> {
    if to.id == SHI_TFNG {
        convert_fast(text, |c| to_tifinagh(to_latin_lower(c)))
    } else if to.id == SHI_LATN {
        convert_fast(text, to_latin)
    } else {
        Cow::Borrowed(text)
    }
}

/// Converts text with possible substitutions.
fn convert_fast<F>(text: &str, mut f: F) -> Cow<'_, str>
where
    F: FnMut(char) -> char,
{
    for (pos, c) in text.char_indices() {
        if c != f(c) {
            return Cow::Owned(convert_slow(text, pos, f));
        }
    }
    Cow::Borrowed(text)
}

/// Converts text with substitutions.
fn convert_slow<F>(text: &str, start: usize, mut f: F) -> String
where
    F: FnMut(char) -> char,
{
    let mut out = String::from(&text[..start]);
    for c in text[start..].chars() {
        out.push(f(c));
    }
    out
}

/// Tifinagh to Latin alphabet.
fn to_latin(c: char) -> char {
    match c {
        'ⴰ' => 'a',
        'ⴱ' => 'b',
        'ⴳ' => 'g',
        'ⴷ' => 'd',
        'ⴹ' => 'ḍ',
        'ⴻ' => 'e',
        'ⴼ' => 'f',
        'ⴽ' => 'k',
        'ⵀ' => 'h',
        'ⵃ' => 'ḥ',
        'ⵄ' => 'ɛ',
        'ⵅ' => 'x',
        'ⵇ' => 'q',
        'ⵉ' => 'i',
        'ⵊ' => 'j',
        'ⵍ' => 'l',
        'ⵎ' => 'm',
        'ⵏ' => 'n',
        'ⵓ' => 'u',
        'ⵔ' => 'r',
        'ⵕ' => 'ṛ',
        'ⵖ' => 'ɣ',
        'ⵙ' => 's',
        'ⵚ' => 'ṣ',
        'ⵛ' => 'c',
        'ⵜ' => 't',
        'ⵟ' => 'ṭ',
        'ⵡ' => 'w',
        'ⵢ' => 'y',
        'ⵣ' => 'z',
        'ⵥ' => 'ẓ',
        'ⵯ' => 'ʷ',
        c => c,
    }
}

/// Latin to lower Latin alphabet.
fn to_latin_lower(c: char) -> char {
    match c {
        'A' => 'a',
        'B' => 'b',
        'G' => 'g',
        'D' => 'd',
        'Ḍ' => 'ḍ',
        'E' => 'e',
        'F' => 'f',
        'K' => 'k',
        'H' => 'h',
        'Ḥ' => 'ḥ',
        'Ɛ' => 'ɛ',
        'X' => 'x',
        'Q' => 'q',
        'I' => 'i',
        'J' => 'j',
        'L' => 'l',
        'M' => 'm',
        'N' => 'n',
        'U' => 'u',
        'R' => 'r',
        'Ṛ' => 'ṛ',
        'Ɣ' => 'ɣ',
        'S' => 's',
        'Ṣ' => 'ṣ',
        'C' => 'c',
        'T' => 't',
        'Ṭ' => 'ṭ',
        'W' => 'w',
        'Y' => 'y',
        'Z' => 'z',
        'Ẓ' => 'ẓ',
        'O' => 'o',
        'P' => 'p',
        'V' => 'v',
        c => c,
    }
}

/// Latin to Tifinagh alphabet.
fn to_tifinagh(c: char) -> char {
    match c {
        'a' => 'ⴰ',
        'b' | 'p' => 'ⴱ',
        'g' => 'ⴳ',
        'd' => 'ⴷ',
        'ḍ' => 'ⴹ',
        'e' => 'ⴻ',
        'f' | 'v' => 'ⴼ',
        'k' => 'ⴽ',
        'h' => 'ⵀ',
        'ḥ' => 'ⵃ',
        'ɛ' => 'ⵄ',
        'x' => 'ⵅ',
        'q' => 'ⵇ',
        'i' => 'ⵉ',
        'j' => 'ⵊ',
        'l' => 'ⵍ',
        'm' => 'ⵎ',
        'n' => 'ⵏ',
        'o' | 'u' => 'ⵓ',
        'r' => 'ⵔ',
        'ṛ' => 'ⵕ',
        'ɣ' => 'ⵖ',
        's' => 'ⵙ',
        'ṣ' => 'ⵚ',
        'c' => 'ⵛ',
        't' => 'ⵜ',
        'ṭ' => 'ⵟ',
        'w' => 'ⵡ',
        'y' => 'ⵢ',
        'z' => 'ⵣ',
        'ẓ' => 'ⵥ',
        'ʷ' => 'ⵯ',
        c => c,
    }
}

/// Shilha Latin script.
const SHI_LATN: LanguageIdentifier = langid!("shi-latn");

/// Shilha Tifinagh script.
const SHI_TFNG: LanguageIdentifier = langid!("shi-tfng");
