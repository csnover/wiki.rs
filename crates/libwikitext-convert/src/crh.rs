//! Language conversion for the Crimean Tatar language.

use super::{
    iter_2::{Iter, convert_fast, next_1},
    split_roman_numerals,
};
use icu_locale::{Locale, locale};
use std::borrow::Cow;

/// A language conversion function for the Crimean Tatar language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to == CRH_CYRL {
        log::warn!("TODO: crh-Cyrl exceptions");
        split_roman_numerals(text, |text| convert_fast(text, to_cyrillic))
    } else if *to == CRH_LATN {
        log::warn!("TODO: crh-Latn exceptions");
        split_roman_numerals(text, |text| convert_fast(text, to_latin))
    } else {
        Cow::Borrowed(text)
    }
}

/// Latin to Cyrillic alphabet.
fn to_cyrillic(c: char, iter: &mut Iter<'_>) -> Option<&'static str> {
    match c {
        'Â' => Some("Я"),
        'â' => Some("я"),
        'B' => Some("Б"),
        'b' => Some("б"),
        'Ç' => Some("Ч"),
        'ç' => Some("ч"),
        'D' => Some("Д"),
        'd' => Some("д"),
        'F' => Some("Ф"),
        'f' => Some("ф"),
        'G' => Some("Г"),
        'g' => Some("г"),
        'H' => Some("Х"),
        'h' => Some("х"),
        'I' => Some("Ы"),
        'ı' => Some("ы"),
        'İ' => Some("И"),
        'i' => Some("и"),
        'J' => Some("Ж"),
        'j' => Some("ж"),
        'K' => Some("К"),
        'k' => Some("к"),
        'L' => Some("Л"),
        'l' => Some("л"),
        'M' => Some("М"),
        'm' => Some("м"),
        'N' => Some("Н"),
        'n' => Some("н"),
        'O' => Some("О"),
        'o' => Some("о"),
        'P' => Some("П"),
        'p' => Some("п"),
        'R' => Some("Р"),
        'r' => Some("р"),
        'S' => Some("С"),
        's' => Some("с"),
        'Ş' => Some("Ш"),
        'ş' => Some("ш"),
        'T' => Some("Т"),
        't' => Some("т"),
        'V' => Some("В"),
        'v' => Some("в"),
        'Z' => Some("З"),
        'z' => Some("з"),
        'A' => Some("А"),
        'a' => Some("а"),
        'E' => Some("Е"),
        'e' => Some("е"),
        'Ö' => Some("Ё"),
        'ö' => Some("ё"),
        'U' => Some("У"),
        'u' => Some("у"),
        'Ü' => Some("Ю"),
        'ü' => Some("ю"),
        'Y' if next_1(iter, 'a') || next_1(iter, 'A') => Some("Я"),
        'Y' if next_1(iter, 'e') || next_1(iter, 'E') => Some("Е"),
        'Y' => Some("Й"),
        'y' if next_1(iter, 'a') => Some("я"),
        'y' if next_1(iter, 'e') => Some("е"),
        'y' => Some("й"),
        'C' => Some("Дж"),
        'c' => Some("дж"),
        'Ğ' => Some("Гъ"),
        'ğ' => Some("гъ"),
        'Ñ' => Some("Нъ"),
        'ñ' => Some("нъ"),
        'Q' => Some("Къ"),
        'q' => Some("къ"),
        '“' => Some("«"),
        '”' => Some("»"),
        _ => None,
    }
}

/// Cyrillic to Latin alphabet.
fn to_latin(c: char, iter: &mut Iter<'_>) -> Option<&'static str> {
    match c {
        'А' => Some("A"),
        'а' => Some("a"),
        'Б' => Some("B"),
        'б' => Some("b"),
        'В' => Some("V"),
        'в' => Some("v"),
        'Г' if next_1(iter, 'ъ') || next_1(iter, 'Ъ') => Some("Ğ"),
        'Г' => Some("G"),
        'г' if next_1(iter, 'ъ') => Some("ğ"),
        'г' => Some("g"),
        'Д' if next_1(iter, 'ж') || next_1(iter, 'Ж') => Some("C"),
        'Д' => Some("D"),
        'д' if next_1(iter, 'ж') => Some("c"),
        'д' => Some("d"),
        'Ж' => Some("J"),
        'ж' => Some("j"),
        'З' => Some("Z"),
        'з' => Some("z"),
        'И' => Some("İ"),
        'и' => Some("i"),
        'Й' => Some("Y"),
        'й' => Some("y"),
        'К' if next_1(iter, 'ъ') || next_1(iter, 'Ъ') => Some("Q"),
        'К' => Some("K"),
        'к' if next_1(iter, 'ъ') => Some("q"),
        'к' => Some("k"),
        'Л' => Some("L"),
        'л' => Some("l"),
        'М' => Some("M"),
        'м' => Some("m"),
        'Н' if next_1(iter, 'ъ') || next_1(iter, 'Ъ') => Some("Ñ"),
        'Н' => Some("N"),
        'н' if next_1(iter, 'ъ') => Some("ñ"),
        'н' => Some("n"),
        'П' => Some("P"),
        'п' => Some("p"),
        'Р' => Some("R"),
        'р' => Some("r"),
        'С' => Some("S"),
        'с' => Some("s"),
        'Т' => Some("T"),
        'т' => Some("t"),
        'Ф' => Some("F"),
        'ф' => Some("f"),
        'Х' => Some("H"),
        'х' => Some("h"),
        'Ч' => Some("Ç"),
        'ч' => Some("ç"),
        'Ш' => Some("Ş"),
        'ш' => Some("ş"),
        'Ы' => Some("I"),
        'ы' => Some("ı"),
        'Э' | 'Е' => Some("E"),
        'э' | 'е' => Some("e"),
        'Я' => Some("Â"),
        'я' => Some("â"),
        'У' => Some("U"),
        'у' => Some("u"),
        'О' => Some("O"),
        'о' => Some("o"),
        'Ё' => Some("Yo"),
        'ё' => Some("yo"),
        'Ю' => Some("Yu"),
        'ю' => Some("yu"),
        'Ц' => Some("Ts"),
        'ц' => Some("ts"),
        'Щ' => Some("Şç"),
        'щ' => Some("şç"),
        'Ь' | 'ь' | 'Ъ' | 'ъ' => Some(""),
        '«' | '»' => Some("\""),
        _ => None,
    }
}

/// Crimean Tatar Cyrillic locale.
const CRH_CYRL: Locale = locale!("crh-cyrl");

/// Crimean Tatar Latin locale.
const CRH_LATN: Locale = locale!("crh-latn");
