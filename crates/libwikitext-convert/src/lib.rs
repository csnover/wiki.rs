//! Types and functions for Wikitext language conversion.

use icu_locale::Locale;
use std::borrow::Cow;
use uncased::UncasedStr;

mod crh;
mod en;
mod shi;
mod sr;
mod tg;

/// A language conversion function.
pub type Converter = for<'a> fn(&'a str, &Locale) -> Cow<'a, str>;

/// Gets a language converter for the given locale.
#[must_use]
pub fn converter(locale: &Locale) -> Option<Converter> {
    CONVERTERS.get(locale.to_string().as_str().into()).copied()
}

/// Gets a language converter for the given locale, or the identity converter
/// if there is no explicit converter for the given locale.
#[must_use]
pub fn converter_or_default(locale: &Locale) -> Converter {
    CONVERTERS
        .get(locale.to_string().as_str().into())
        .copied()
        .unwrap_or(identity)
}

/// The identity converter.
#[inline]
#[must_use]
pub fn identity<'a>(text: &'a str, _: &Locale) -> Cow<'a, str> {
    Cow::Borrowed(text)
}

/// Runs the given callback `f` on `text`, split by runs of roman numerals.
pub fn split_roman_numerals<F>(text: &str, mut f: F) -> Cow<'_, str>
where
    F: FnMut(&str) -> Cow<'_, str>,
{
    log::warn!("TODO: Split roman numerals");
    f(text)
}

/// A map from supported BCP-47 codes to conversion functions.
static CONVERTERS: phf::Map<&UncasedStr, Converter> = phf::phf_map! {
    UncasedStr::new("crh") | UncasedStr::new("crh-cyrl") | UncasedStr::new("crh-latn") => crh::convert,
    UncasedStr::new("en") | UncasedStr::new("en-x-piglatin") => en::convert,
    UncasedStr::new("shi") | UncasedStr::new("shi-latn") | UncasedStr::new("shi-tfng") => shi::convert,
    // TODO: This is sr-ec and sr-el in MediaWiki, but these are supposedly
    // non-standard codes that language conversion is never supposed to see.
    UncasedStr::new("sr") | UncasedStr::new("sr-cyrl") | UncasedStr::new("sr-latn") => sr::convert,
    UncasedStr::new("tg") | UncasedStr::new("tg-latn") => tg::convert,
};

/// Common functions for 2-character look-ahead conversion.
mod iter_2 {
    use core::str::CharIndices;
    use peeknth::SizedPeekN;
    use std::borrow::Cow;

    /// A character iterator with 2-character look-ahead.
    pub type Iter<'a> = SizedPeekN<CharIndices<'a>, 2>;

    /// Converts text with possible substitutions.
    pub fn convert_fast<F>(text: &str, mut f: F) -> Cow<'_, str>
    where
        F: FnMut(char, &mut Iter<'_>) -> Option<&'static str>,
    {
        let mut iter = Iter::new(text.char_indices());
        while let Some((pos, c)) = iter.next() {
            if let Some(s) = f(c, iter.by_ref()) {
                return Cow::Owned(convert_slow(text, pos, s, iter, f));
            }
        }

        Cow::Borrowed(text)
    }

    /// Converts text with substitutions.
    fn convert_slow<F>(
        text: &str,
        start: usize,
        first: &str,
        mut iter: Iter<'_>,
        mut f: F,
    ) -> String
    where
        F: FnMut(char, &mut Iter<'_>) -> Option<&'static str>,
    {
        let mut out = String::from(&text[..start]);
        out += first;
        while let Some((_, c)) = iter.next() {
            if let Some(s) = f(c, iter.by_ref()) {
                out += s;
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Takes the next character and returns true if it is `a`.
    pub fn next_1(iter: &mut Iter<'_>, a: char) -> bool {
        iter.next_if(|(_, c)| *c == a).is_some()
    }

    /// Takes the next 2 characters and returns true if they are `a` and `b`.
    pub fn next_2(iter: &mut Iter<'_>, a: char, b: char) -> bool {
        if matches!(iter.peek(), Some((_, c)) if *c == a)
            && matches!(iter.peek_nth(1), Some((_, c)) if *c == b)
        {
            iter.clear_peeked();
            true
        } else {
            false
        }
    }
}
