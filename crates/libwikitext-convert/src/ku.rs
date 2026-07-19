//! Language conversion for the Kurdish language.

use super::iter_2::{Iter, convert_fast, next_1};
use icu_locale::{Locale, locale};
use std::borrow::Cow;

/// A language conversion function for the Kurdish language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to == KU_ARAB {
        convert_fast(text, make_to_arabic())
    } else if *to == KU_LATN {
        convert_fast(text, to_latin)
    } else {
        Cow::Borrowed(text)
    }
}

/// Latin to Arabic conversion function.
fn make_to_arabic() -> impl FnMut(char, &mut Iter<'_>) -> Option<&'static str> {
    let (mut in_word, mut prev_was_vowel) = (false, false);
    move |c, _| {
        let vowel = to_arabic_vowel(c, !in_word || prev_was_vowel);
        let next = vowel.or_else(|| to_arabic(c));
        if next.is_none() && !c.is_ascii_digit() {
            in_word = false;
            prev_was_vowel = false;
        } else {
            in_word = true;
            prev_was_vowel = vowel.is_some();
        }
        next
    }
}

/// Latin to Arabic word alphabet.
fn to_arabic(c: char) -> Option<&'static str> {
    match c {
        'b' | 'B' => Some("ب"),
        'c' | 'C' => Some("ج"),
        'ç' | 'Ç' => Some("چ"),
        'd' | 'D' => Some("د"),
        'f' | 'F' => Some("ف"),
        'g' | 'G' => Some("گ"),
        'h' => Some("ه"),
        'j' | 'J' => Some("ژ"),
        'k' | 'K' => Some("ک"),
        'l' | 'L' => Some("ل"),
        'm' | 'M' => Some("م"),
        'n' | 'N' => Some("ن"),
        'p' | 'P' => Some("پ"),
        'q' | 'Q' => Some("ق"),
        'r' | 'R' => Some("ر"),
        's' | 'S' => Some("س"),
        'ş' | 'Ş' => Some("ش"),
        't' | 'T' => Some("ت"),
        'v' | 'V' => Some("ڤ"),
        'w' | 'W' => Some("و"),
        'x' | 'X' => Some("خ"),
        'y' | 'Y' => Some("ی"),
        'z' | 'Z' => Some("ز"),
        'ḧ' | 'H' | 'Ḧ' => Some("ح"),
        'ẍ' | 'Ẍ' => Some("غ"),
        ',' => Some("،"),
        '?' => Some("؟"),
        _ => None,
    }
}

/// Latin to Arabic vowel alphabet.
fn to_arabic_vowel(c: char, hemze: bool) -> Option<&'static str> {
    macro_rules! with_hemze {
        ($hemze:expr, $vowel:literal) => {
            if $hemze {
                Some(concat!("ئ", $vowel))
            } else {
                Some($vowel)
            }
        };
    }

    match c {
        'a' | 'A' => with_hemze!(hemze, "ا"),
        'e' | 'E' => with_hemze!(hemze, "ە"),
        'ê' | 'Ê' => with_hemze!(hemze, "ێ"),
        'i' | 'I' => with_hemze!(hemze, ""),
        'î' | 'Î' => with_hemze!(hemze, "ی"),
        'o' | 'O' => with_hemze!(hemze, "ۆ"),
        'u' | 'U' => with_hemze!(hemze, "و"),
        'û' | 'Û' => with_hemze!(hemze, "وو"),
        _ => None,
    }
}

/// Arabic to Latin alphabet.
fn to_latin(c: char, iter: &mut Iter<'_>) -> Option<&'static str> {
    match c {
        'ب' => Some("b"),
        'ج' => Some("c"),
        'چ' => Some("ç"),
        'د' => Some("d"),
        'ف' => Some("f"),
        'گ' => Some("g"),
        'ژ' => Some("j"),
        'ك' | 'ک' => Some("k"),
        'ل' => Some("l"),
        'م' => Some("m"),
        'ن' => Some("n"),
        'پ' => Some("p"),
        'ق' => Some("q"),
        'ر' => Some("r"),
        'س' => Some("s"),
        'ش' => Some("ş"),
        'ت' => Some("t"),
        'ڤ' => Some("v"),
        'خ' | 'غ' => Some("x"),
        'ز' => Some("z"),
        'ڵ' => Some("ll"),
        'ڕ' => Some("rr"),
        'ا' => Some("a"),
        'ە' | 'ة' => Some("e"),
        'ه' if next_1(iter, '\u{200c}') => {
            // One or two ZWNJs
            next_1(iter, '\u{200c}');
            Some("e")
        }
        'ھ' | 'ہ' | 'ه' | 'ح' => Some("h"),
        'ێ' => Some("ê"),
        'ي' | 'ی' | 'ى' => Some("î"),
        'ۆ' => Some("o"),
        'و' => Some("w"),
        'ئ' => Some(""),
        '،' => Some(","),
        'ع' => Some("\'"),
        '؟' => Some("?"),
        '٠' => Some("0"),
        '١' => Some("1"),
        '٢' => Some("2"),
        '٣' => Some("3"),
        '٤' => Some("4"),
        '٥' => Some("5"),
        '٦' => Some("6"),
        '٧' => Some("7"),
        '٨' => Some("8"),
        '٩' => Some("9"),
        _ => None,
    }
}

/// Kurdish Arabic locale.
const KU_ARAB: Locale = locale!("ku-arab");

/// Kurdish Latin locale.
const KU_LATN: Locale = locale!("ku-latn");
