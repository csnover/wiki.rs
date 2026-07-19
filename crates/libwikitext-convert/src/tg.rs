//! Language conversion for the Tajik language.

use super::iter_2::{Iter, convert_fast, next_2};
use icu_locale::{Locale, locale};
use std::borrow::Cow;

/// A language conversion function for the Tajik language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to == locale!("tg-latn") {
        convert_fast(text, to_latin)
    } else {
        Cow::Borrowed(text)
    }
}

/// Cyrillic to Latin alphabet.
fn to_latin(c: char, iter: &mut Iter<'_>) -> Option<&'static str> {
    match c {
        'а' => Some("a"),
        'б' => Some("b"),
        'в' => Some("v"),
        'г' => Some("g"),
        'д' => Some("d"),
        'е' | 'э' => Some("e"),
        'ё' => Some("jo"),
        'ж' => Some("ƶ"),
        'з' => Some("z"),
        'и' => {
            if next_2(iter, 'и', ' ') {
                Some("iji ")
            } else {
                Some("i")
            }
        }
        'й' => Some("j"),
        'к' => Some("k"),
        'л' => Some("l"),
        'м' => Some("m"),
        'н' => Some("n"),
        'о' => Some("o"),
        'п' => Some("p"),
        'р' => Some("r"),
        'с' => Some("s"),
        'т' => Some("t"),
        'у' => Some("u"),
        'ф' => Some("f"),
        'х' => Some("x"),
        'ч' => Some("c"),
        'ш' => Some("ş"),
        'ъ' | 'Ъ' => Some("'"),
        'ю' => Some("ju"),
        'я' => Some("ja"),
        'ғ' => Some("ƣ"),
        'ӣ' => Some("ī"),
        'қ' => Some("q"),
        'ӯ' => Some("ū"),
        'ҳ' => Some("h"),
        'ҷ' => Some("ç"),
        'ц' => Some("ts"),
        'А' => Some("A"),
        'Б' => Some("B"),
        'В' => Some("V"),
        'Г' => Some("G"),
        'Д' => Some("D"),
        'Е' | 'Э' => Some("E"),
        'Ё' => Some("Jo"),
        'Ж' => Some("Ƶ"),
        'З' => Some("Z"),
        'И' => Some("I"),
        'Й' => Some("J"),
        'К' => Some("K"),
        'Л' => Some("L"),
        'М' => Some("M"),
        'Н' => Some("N"),
        'О' => Some("O"),
        'П' => Some("P"),
        'Р' => Some("R"),
        'С' => Some("S"),
        'Т' => Some("T"),
        'У' => Some("U"),
        'Ф' => Some("F"),
        'Х' => Some("X"),
        'Ч' => Some("C"),
        'Ш' => Some("Ş"),
        'Ю' => Some("Ju"),
        'Я' => Some("Ja"),
        'Ғ' => Some("Ƣ"),
        'Ӣ' => Some("Ī"),
        'Қ' => Some("Q"),
        'Ӯ' => Some("Ū"),
        'Ҳ' => Some("H"),
        'Ҷ' => Some("Ç"),
        'Ц' => Some("Ts"),
        _ => None,
    }
}
