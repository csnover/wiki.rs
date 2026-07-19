//! Language conversion for the Uzbek language.

use super::iter_2::{Iter, convert_fast, next_1};
use icu_locale::{Locale, locale};
use std::borrow::Cow;

/// A language conversion function for the Uzbek language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to == UZ_CYRL {
        convert_fast(text, to_cyrillic)
    } else if *to == UZ_LATN {
        convert_fast(text, to_latin)
    } else {
        Cow::Borrowed(text)
    }
}

/// Handles the `[bvgdjzyklmnprstfxcwqʻ‘h][Ee]` rule.
macro_rules! consonant_e {
    ($c:expr, $iter:expr, $consonant:literal) => {
        if next_1($iter, 'e') {
            Some(concat!($consonant, "е"))
        } else if !$c.is_ascii_lowercase() && next_1($iter, 'E') {
            Some(concat!($consonant, "Е"))
        } else {
            Some($consonant)
        }
    };
}

/// Latin to Cyrillic alphabet.
fn to_cyrillic(c: char, iter: &mut Iter<'_>) -> Option<&'static str> {
    if matches!(c, 'Y' | 'y') && next_1(iter, 'e') {
        return to_cyrillic('e', iter);
    } else if c == 'Y' && next_1(iter, 'E') {
        return to_cyrillic('E', iter);
    }

    match c {
        'a' => Some("а"),
        'A' => Some("А"),
        'b' => consonant_e!(c, iter, "б"),
        'B' => consonant_e!(c, iter, "Б"),
        'c' if next_1(iter, 'h') => Some("ч"),
        'c' => consonant_e!(c, iter, "c"),
        'C' if next_1(iter, 'h') => Some("Ч"),
        'C' => consonant_e!(c, iter, "C"),
        'd' => consonant_e!(c, iter, "д"),
        'D' => consonant_e!(c, iter, "Д"),
        'e' => Some("э"),
        'E' => Some("Э"),
        'f' => consonant_e!(c, iter, "ф"),
        'F' => consonant_e!(c, iter, "Ф"),
        'g' if next_1(iter, '‘') || next_1(iter, 'ʻ') => Some("ғ"),
        'g' => consonant_e!(c, iter, "г"),
        'G' if next_1(iter, '‘') || next_1(iter, 'ʻ') => Some("Ғ"),
        'G' => consonant_e!(c, iter, "Г"),
        'h' => consonant_e!(c, iter, "ҳ"),
        'H' => consonant_e!(c, iter, "Ҳ"),
        'i' => Some("и"),
        'I' => Some("И"),
        'k' => consonant_e!(c, iter, "к"),
        'K' => consonant_e!(c, iter, "К"),
        'l' => consonant_e!(c, iter, "л"),
        'L' => consonant_e!(c, iter, "Л"),
        'm' => consonant_e!(c, iter, "м"),
        'M' => consonant_e!(c, iter, "М"),
        'n' => consonant_e!(c, iter, "н"),
        'N' => consonant_e!(c, iter, "Н"),
        'o' if next_1(iter, '‘') || next_1(iter, 'ʻ') => Some("ў"),
        'o' => Some("о"),
        'O' if next_1(iter, '‘') || next_1(iter, 'ʻ') => Some("Ў"),
        'O' => Some("О"),
        'p' => consonant_e!(c, iter, "п"),
        'P' => consonant_e!(c, iter, "П"),
        'r' => consonant_e!(c, iter, "р"),
        'R' => consonant_e!(c, iter, "Р"),
        's' if next_1(iter, 'h') => Some("ш"),
        's' => consonant_e!(c, iter, "с"),
        'S' if next_1(iter, 'h') => Some("Ш"),
        'S' => consonant_e!(c, iter, "С"),
        't' if next_1(iter, 's') => Some("ц"),
        't' => consonant_e!(c, iter, "т"),
        'T' if next_1(iter, 's') => Some("Ц"),
        'T' => consonant_e!(c, iter, "Т"),
        'u' => Some("у"),
        'U' => Some("У"),
        'v' => consonant_e!(c, iter, "в"),
        'V' => consonant_e!(c, iter, "В"),
        'w' => consonant_e!(c, iter, "w"),
        'W' => consonant_e!(c, iter, "W"),
        'x' => consonant_e!(c, iter, "х"),
        'X' => consonant_e!(c, iter, "Х"),
        'z' => consonant_e!(c, iter, "з"),
        'Z' => consonant_e!(c, iter, "З"),
        'j' => consonant_e!(c, iter, "ж"),
        'J' => consonant_e!(c, iter, "Ж"),
        'q' => consonant_e!(c, iter, "қ"),
        'Q' => consonant_e!(c, iter, "Қ"),
        'y' if next_1(iter, 'a') => Some("я"),
        'y' if next_1(iter, 'o') => {
            if next_1(iter, '‘') || next_1(iter, 'ʻ') {
                Some("йў")
            } else {
                Some("ё")
            }
        }
        'y' if next_1(iter, 'u') => Some("ю"),
        'y' => consonant_e!(c, iter, "й"),
        'Y' if next_1(iter, 'a') => Some("Я"),
        'Y' if next_1(iter, 'o') => {
            if next_1(iter, '‘') || next_1(iter, 'ʻ') {
                Some("Йў")
            } else {
                Some("Ё")
            }
        }
        'Y' if next_1(iter, 'u') => Some("Ю"),
        'Y' => consonant_e!(c, iter, "Й"),
        'ʼ' => consonant_e!(c, iter, "ъ"),
        'ʻ' => consonant_e!(c, iter, "ʻ"),
        '‘' => consonant_e!(c, iter, "‘"),
        _ => None,
    }
}

/// Cyrillic to Latin alphabet.
fn to_latin(c: char, _: &mut Iter<'_>) -> Option<&'static str> {
    match c {
        'а' => Some("a"),
        'А' => Some("A"),
        'б' => Some("b"),
        'Б' => Some("B"),
        'д' => Some("d"),
        'Д' => Some("D"),
        'е' | 'э' => Some("e"),
        'Е' | 'Э' => Some("E"),
        'в' => Some("v"),
        'В' => Some("V"),
        'х' => Some("x"),
        'Х' => Some("X"),
        'ғ' => Some("gʻ"),
        'Ғ' => Some("Gʻ"),
        'г' => Some("g"),
        'Г' => Some("G"),
        'ҳ' => Some("h"),
        'Ҳ' => Some("H"),
        'ж' => Some("j"),
        'Ж' => Some("J"),
        'з' => Some("z"),
        'З' => Some("Z"),
        'и' => Some("i"),
        'И' => Some("I"),
        'к' => Some("k"),
        'К' => Some("K"),
        'л' => Some("l"),
        'Л' => Some("L"),
        'м' => Some("m"),
        'М' => Some("M"),
        'н' => Some("n"),
        'Н' => Some("N"),
        'о' => Some("o"),
        'О' => Some("O"),
        'п' => Some("p"),
        'П' => Some("P"),
        'р' => Some("r"),
        'Р' => Some("R"),
        'с' => Some("s"),
        'С' => Some("S"),
        'т' => Some("t"),
        'Т' => Some("T"),
        'у' => Some("u"),
        'У' => Some("U"),
        'ф' => Some("f"),
        'Ф' => Some("F"),
        'ў' => Some("oʻ"),
        'Ў' => Some("Oʻ"),
        'ц' => Some("ts"),
        'Ц' => Some("Ts"),
        'қ' => Some("q"),
        'Қ' => Some("Q"),
        'ё' => Some("yo"),
        'Ё' => Some("Yo"),
        'ю' => Some("yu"),
        'Ю' => Some("Yu"),
        'ч' => Some("ch"),
        'Ч' => Some("Ch"),
        'ш' => Some("sh"),
        'Ш' => Some("Sh"),
        'й' => Some("y"),
        'Й' => Some("Y"),
        'я' => Some("ya"),
        'Я' => Some("Ya"),
        'ъ' => Some("ʼ"),
        _ => None,
    }
}

/// Uzbek Cyrillic locale.
const UZ_CYRL: Locale = locale!("uz-cyrl");

/// Uzbek Latin locale.
const UZ_LATN: Locale = locale!("uz-latn");
