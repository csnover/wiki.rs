//! Language conversion for the Shilha language.

use core::str::CharIndices;
use icu_locale::{LanguageIdentifier, langid};
use std::borrow::Cow;

/// A language conversion function for the Shilha language.
pub fn convert<'a>(text: &'a str, to: &icu_locale::Locale) -> Cow<'a, str> {
    if to.id == SHI_TFNG {
        convert_fast(text, to_tifinagh)
    } else if to.id == SHI_LATN {
        convert_fast(text, to_latin)
    } else {
        Cow::Borrowed(text)
    }
}

/// Converts text with possible substitutions.
fn convert_fast<F>(text: &str, mut f: F) -> Cow<'_, str>
where
    F: FnMut(char, &mut CharIndices<'_>) -> Option<&'static str>,
{
    let mut iter = text.char_indices();
    while let Some((pos, c)) = iter.next() {
        if let Some(repl) = f(c, iter.by_ref()) {
            return Cow::Owned(super::convert_slow(text, pos, repl, iter, f));
        }
    }
    Cow::Borrowed(text)
}

/// Tifinagh to Latin alphabet.
fn to_latin(c: char, _: &mut CharIndices<'_>) -> Option<&'static str> {
    match c {
        'ⴰ' => Some("a"),
        'ⴱ' => Some("b"),
        'ⴳ' => Some("g"),
        'ⴷ' => Some("d"),
        'ⴹ' => Some("ḍ"),
        'ⴻ' => Some("e"),
        'ⴼ' => Some("f"),
        'ⴽ' => Some("k"),
        'ⵀ' => Some("h"),
        'ⵃ' => Some("ḥ"),
        'ⵄ' => Some("ɛ"),
        'ⵅ' => Some("x"),
        'ⵇ' => Some("q"),
        'ⵉ' => Some("i"),
        'ⵊ' => Some("j"),
        'ⵍ' => Some("l"),
        'ⵎ' => Some("m"),
        'ⵏ' => Some("n"),
        'ⵓ' => Some("u"),
        'ⵔ' => Some("r"),
        'ⵕ' => Some("ṛ"),
        'ⵖ' => Some("ɣ"),
        'ⵙ' => Some("s"),
        'ⵚ' => Some("ṣ"),
        'ⵛ' => Some("c"),
        'ⵜ' => Some("t"),
        'ⵟ' => Some("ṭ"),
        'ⵡ' => Some("w"),
        'ⵢ' => Some("y"),
        'ⵣ' => Some("z"),
        'ⵥ' => Some("ẓ"),
        'ⵯ' => Some("ʷ"),
        _ => None,
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
fn to_tifinagh(c: char, _: &mut CharIndices<'_>) -> Option<&'static str> {
    match to_latin_lower(c) {
        'a' => Some("ⴰ"),
        'b' | 'p' => Some("ⴱ"),
        'g' => Some("ⴳ"),
        'd' => Some("ⴷ"),
        'ḍ' => Some("ⴹ"),
        'e' => Some("ⴻ"),
        'f' | 'v' => Some("ⴼ"),
        'k' => Some("ⴽ"),
        'h' => Some("ⵀ"),
        'ḥ' => Some("ⵃ"),
        'ɛ' => Some("ⵄ"),
        'x' => Some("ⵅ"),
        'q' => Some("ⵇ"),
        'i' => Some("ⵉ"),
        'j' => Some("ⵊ"),
        'l' => Some("ⵍ"),
        'm' => Some("ⵎ"),
        'n' => Some("ⵏ"),
        'o' | 'u' => Some("ⵓ"),
        'r' => Some("ⵔ"),
        'ṛ' => Some("ⵕ"),
        'ɣ' => Some("ⵖ"),
        's' => Some("ⵙ"),
        'ṣ' => Some("ⵚ"),
        'c' => Some("ⵛ"),
        't' => Some("ⵜ"),
        'ṭ' => Some("ⵟ"),
        'w' => Some("ⵡ"),
        'y' => Some("ⵢ"),
        'z' => Some("ⵣ"),
        'ẓ' => Some("ⵥ"),
        'ʷ' => Some("ⵯ"),
        _ => None,
    }
}

/// Shilha Latin script.
const SHI_LATN: LanguageIdentifier = langid!("shi-latn");

/// Shilha Tifinagh script.
const SHI_TFNG: LanguageIdentifier = langid!("shi-tfng");
