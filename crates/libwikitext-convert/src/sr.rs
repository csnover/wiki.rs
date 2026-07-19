//! Language conversion for the Serbian language.

use super::{
    iter_2::{Iter, convert_fast, next_1, next_2},
    split_roman_numerals,
};
use icu_locale::{Locale, locale};
use std::borrow::Cow;

/// A language conversion function for the Serbian language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to == SR_CYRL {
        split_roman_numerals(text, |text| convert_fast(text, to_cyrillic))
    } else if *to == SR_LATN {
        split_roman_numerals(text, |text| convert_fast(text, to_latin))
    } else {
        Cow::Borrowed(text)
    }
}

/// Latin to Cyrillic alphabet.
fn to_cyrillic(c: char, iter: &mut Iter<'_>) -> Option<&'static str> {
    match c {
        'a' => Some("а"),
        'b' => Some("б"),
        'c' => Some("ц"),
        'č' => Some("ч"),
        'ć' => Some("ћ"),
        'd' if next_1(iter, 'ž') => Some("џ"),
        'd' if next_2(iter, '!', 'ž') => Some("дж"),
        'd' => Some("д"),
        'đ' => Some("ђ"),
        'e' => Some("е"),
        'f' => Some("ф"),
        'g' => Some("г"),
        'h' => Some("х"),
        'i' => Some("и"),
        'j' => Some("ј"),
        'k' => Some("к"),
        'l' if next_1(iter, 'j') => Some("љ"),
        'l' if next_2(iter, '!', 'ž') => Some("лј"),
        'l' => Some("л"),
        'm' => Some("м"),
        'n' if next_1(iter, 'j') => Some("њ"),
        'n' if next_2(iter, '!', 'j') => Some("нј"),
        'n' => Some("н"),
        'o' => Some("о"),
        'p' => Some("п"),
        'r' => Some("р"),
        's' => Some("с"),
        'š' => Some("ш"),
        't' => Some("т"),
        'u' => Some("у"),
        'v' => Some("в"),
        'z' => Some("з"),
        'ž' => Some("ж"),
        'A' => Some("А"),
        'B' => Some("Б"),
        'C' => Some("Ц"),
        'Č' => Some("Ч"),
        'Ć' => Some("Ћ"),
        'D' if next_1(iter, 'ž') || next_1(iter, 'Ž') => Some("Џ"),
        'D' if next_2(iter, '!', 'ž') || next_2(iter, '!', 'Ž') => Some("Дж"),
        'D' => Some("Д"),
        'Đ' => Some("Ђ"),
        'E' => Some("Е"),
        'F' => Some("Ф"),
        'G' => Some("Г"),
        'H' => Some("Х"),
        'I' => Some("И"),
        'J' => Some("Ј"),
        'K' => Some("К"),
        'L' if next_1(iter, 'J') || next_1(iter, 'j') => Some("Љ"),
        'L' if next_2(iter, '!', 'j') => Some("Лј"),
        'L' if next_2(iter, '!', 'J') => Some("ЛЈ"),
        'L' => Some("Л"),
        'M' => Some("М"),
        'N' if next_1(iter, 'J') || next_1(iter, 'j') => Some("Њ"),
        'N' if next_2(iter, '!', 'j') => Some("Нј"),
        'N' if next_2(iter, '!', 'J') => Some("НЈ"),
        'N' => Some("Н"),
        'O' => Some("О"),
        'P' => Some("П"),
        'R' => Some("Р"),
        'S' => Some("С"),
        'Š' => Some("Ш"),
        'T' => Some("Т"),
        'U' => Some("У"),
        'V' => Some("В"),
        'Z' => Some("З"),
        'Ž' => Some("Ж"),
        _ => None,
    }
}

/// Cyrillic to Latin alphabet.
fn to_latin(c: char, _: &mut Iter<'_>) -> Option<&'static str> {
    match c {
        'а' => Some("a"),
        'б' => Some("b"),
        'в' => Some("v"),
        'г' => Some("g"),
        'д' => Some("d"),
        'ђ' => Some("đ"),
        'е' => Some("e"),
        'ж' => Some("ž"),
        'з' => Some("z"),
        'и' => Some("i"),
        'ј' => Some("j"),
        'к' => Some("k"),
        'л' => Some("l"),
        'љ' => Some("lj"),
        'м' => Some("m"),
        'н' => Some("n"),
        'њ' => Some("nj"),
        'о' => Some("o"),
        'п' => Some("p"),
        'р' => Some("r"),
        'с' => Some("s"),
        'т' => Some("t"),
        'ћ' => Some("ć"),
        'у' => Some("u"),
        'ф' => Some("f"),
        'х' => Some("h"),
        'ц' => Some("c"),
        'ч' => Some("č"),
        'џ' => Some("dž"),
        'ш' => Some("š"),
        'А' => Some("A"),
        'Б' => Some("B"),
        'В' => Some("V"),
        'Г' => Some("G"),
        'Д' => Some("D"),
        'Ђ' => Some("Đ"),
        'Е' => Some("E"),
        'Ж' => Some("Ž"),
        'З' => Some("Z"),
        'И' => Some("I"),
        'Ј' => Some("J"),
        'К' => Some("K"),
        'Л' => Some("L"),
        'Љ' => Some("Lj"),
        'М' => Some("M"),
        'Н' => Some("N"),
        'Њ' => Some("Nj"),
        'О' => Some("O"),
        'П' => Some("P"),
        'Р' => Some("R"),
        'С' => Some("S"),
        'Т' => Some("T"),
        'Ћ' => Some("Ć"),
        'У' => Some("U"),
        'Ф' => Some("F"),
        'Х' => Some("H"),
        'Ц' => Some("C"),
        'Ч' => Some("Č"),
        'Џ' => Some("Dž"),
        'Ш' => Some("Š"),
        _ => None,
    }
}

/// Serbian Cyrillic locale.
const SR_CYRL: Locale = locale!("sr-cyrl");

/// Serbian Latin locale.
const SR_LATN: Locale = locale!("sr-latn");
