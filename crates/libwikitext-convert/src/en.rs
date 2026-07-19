//! Language conversion for the English language.

use icu_locale::Locale;
use libwikitext_common::{to_lower_first, to_upper_first};
use regex::{Captures, Regex};
use std::{borrow::Cow, sync::LazyLock};

/// A language conversion function for the English language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to != *EN_PIGLATIN {
        return Cow::Borrowed(text);
    }

    RE_WORDSTART.replace_all(text, |captures: &Captures<'_>| {
        let word = captures.get_match().as_str();
        if word.starts_with(|c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u'))
        {
            format!("{word}way")
        } else {
            RE_TRANSPOSE
                .replace(word, |captures: &Captures<'_>| {
                    let (_, [consonants, rest]) = captures.extract();
                    if consonants.starts_with(|c: char| c.is_ascii_uppercase()) {
                        format!("{}{}ay", to_upper_first(rest), to_lower_first(consonants))
                    } else {
                        format!("{rest}{consonants}ay")
                    }
                })
                .into_owned()
        }
    })
}

/// English Pig Latin locale.
static EN_PIGLATIN: LazyLock<Locale> = LazyLock::new(|| "en-x-piglatin".parse().unwrap());

/// Pig Latin transcription captures.
static RE_TRANSPOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^((?i)s?qu|[^aeiou][^aeiouy]*)(.*)$").unwrap());

/// Word match.
static RE_WORDSTART: LazyLock<Regex> = LazyLock::new(|| Regex::new("[A-Za-z][a-z']+").unwrap());
