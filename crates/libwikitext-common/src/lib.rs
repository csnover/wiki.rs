//! Common Wikitext types and functions.

pub mod config;
pub mod db;
pub mod lru_limiter;
pub mod title;
pub mod url;

use core::{fmt::Write as _, iter};
use html_escape::NAMED_ENTITIES;
use libmisc::{CowExt as _, to_ascii_lower, to_ascii_upper};
use libphp_rs::{DateTime, DateTimeError, DateTimeZone, strtr, strval, ucfirst};
use regex::Regex;
use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

/// An i18n message dictionary.
#[derive(serde::Deserialize)]
pub struct Messages<'a> {
    /// Dictionary metadata.
    #[serde(rename = "@metadata")]
    _metadata: serde::de::IgnoredAny,
    /// Dictionary messages.
    #[serde(borrow, flatten)]
    messages: HashMap<&'a str, Cow<'a, str>>,
}

impl<'a> Messages<'a> {
    /// Returns a reference to the message with the corresponding key.
    #[must_use]
    pub fn get(&'a self, key: &str) -> Option<&'a str> {
        self.messages.get(key).map(Cow::as_ref)
    }

    /// Merges `other` into `self`.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        self.messages.extend(other.messages);
        self
    }
}

/// An anchor encoding algorithm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnchorEncodeMode {
    /// The HTML5 anchor encoding algorithm.
    Html5,
    /// The legacy anchor encoding algorithm.
    Legacy,
}

/// Encodes section heading text into a format suitable for use as a URL anchor.
///
/// This is equivalent to `escapeIdForAttribute`.
#[must_use]
pub fn anchor_encode(s: &str, mode: AnchorEncodeMode) -> Cow<'_, str> {
    let id = &s[..s.floor_char_boundary(1024)];
    match mode {
        AnchorEncodeMode::Html5 => strtr(
            id,
            &[
                ("\t", "_"),
                ("\n", "_"),
                ("\x0c", "_"),
                ("\r", "_"),
                (" ", "_"),
            ],
        ),
        AnchorEncodeMode::Legacy => {
            const ALPHABET: percent_encoding::AsciiSet =
                libphp_rs::URL_ENCODE_ALPHABET.remove(b':');
            Cow::from(percent_encoding::utf8_percent_encode(id, &ALPHABET))
                .map(|id| strtr(id, &[(" ", "_"), ("%", ".")]))
        }
    }
}

/// Decodes HTML entities according to the Wikitext rules.
pub fn decode_html(text: &str) -> Cow<'_, str> {
    const MAX_LEN: usize = {
        let mut max = 0;
        let mut entities = NAMED_ENTITIES.as_slice();
        while let [(name, _), rest @ ..] = entities {
            if name.len() > max {
                max = name.len();
            }
            entities = rest;
        }

        if "רלמ".len() > max {
            max = "רלמ".len();
        }

        if "رلم".len() > max {
            max = "رلم".len();
        }

        max + b";".len()
    };

    let bytes = text.as_bytes();
    let entity_ranges = memchr::memchr_iter(b'&', bytes).filter_map(|start| {
        let next = start + "&".len();
        memchr::memchr(b';', &bytes[next..(next + MAX_LEN).min(bytes.len())])
            .map(|len| start..(next + len + b";".len()))
    });

    let mut flushed = 0;
    let mut out = String::new();
    for range in entity_ranges {
        let mut char = [0; 4];
        let name = &text[range.start + 1..range.end - 1];
        let value = if let Some(name) = name.strip_prefix('#') {
            if let Some(name) = name.strip_prefix(|c: char| matches!(c, 'X' | 'x')) {
                u32::from_str_radix(name, 16)
            } else {
                name.parse::<u32>()
            }
            .ok()
            .and_then(char::from_u32)
            .map(|c| &*c.encode_utf8(&mut char))
        } else {
            NAMED_ENTITIES
                .binary_search_by(|(t_name, _)| t_name.cmp(&name.as_bytes()))
                .map_or_else(
                    |_| (name == "רלמ" || name == "رلم").then_some("\u{200f}"),
                    |index| Some(NAMED_ENTITIES[index].1),
                )
        };
        if let Some(value) = value {
            out += &text[flushed..range.start];
            out += value;
            flushed = range.end;
        }
    }

    if flushed != 0 {
        out += &text[flushed..];
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// Escapes all non-HTML Wikitext control sequences.
#[must_use]
pub fn escape(text: &str) -> Cow<'_, str> {
    static REPLS: &[(&str, &str)] = &[
        ("ISBN", "&#73;SBN"),
        ("PMID", "&#80;MID"),
        ("RFC", "&#82;FC"),
        ("＿", "&#xFF3F;"), // 3 bytes in UTF-8
        ("__", "&#95;_"),
        ("\'\'", "&#39;&#39;"),
        ("!", "&#33;"),
        (":", "&#58;"),
        (";", "&#59;"),
        ("[", "&#91;"),
        ("]", "&#93;"),
        ("{", "&#123;"),
        ("|", "&#124;"),
        ("}", "&#125;"),
    ];

    strtr(text, REPLS)
}

/// Escapes all Wikitext and HTML control sequences.
#[must_use]
pub fn escape_no_wiki(text: &str) -> Cow<'_, str> {
    static REPLS: &[(&str, &str)] = &[
        ("ISBN", "&#73;SBN"),
        ("PMID", "&#80;MID"),
        ("RFC", "&#82;FC"),
        ("＿", "&#xFF3F;"), // 3 bytes in UTF-8
        ("\'\'", "&#39;&#39;"),
        ("__", "&#95;_"),
        ("\"", "&quot;"),
        ("!", "&#33;"),
        ("&", "&amp;"),
        (":", "&#58;"),
        (";", "&#59;"),
        ("<", "&lt;"),
        ("=", "&#61;"),
        (">", "&gt;"),
        ("[", "&#91;"),
        ("]", "&#93;"),
        ("{", "&#123;"),
        ("|", "&#124;"),
        ("}", "&#125;"),
    ];

    strtr(text, REPLS)
}

/// Formats a date according to the given `format` string.
///
/// The `format` string is a MediaWiki extended time formatting string.
///
/// The `date` string is a modified form of the PHP date string format where
/// a four-digit number is treated as a year instead of a time.
///
/// If `local` is true, the time will be treated as being in the system time
/// zone; otherwise, it will be treated as UTC.
///
/// The value given in `now` will be used as the current time if no `date` is
/// given.
///
/// # Errors
///
/// * `date` cannot be parsed as a date
/// * `local` is true and the local time zone cannot be determined
/// * A write to the output buffer fails
pub fn format_date_mediawiki(
    now: &DateTime,
    format: &str,
    date: Option<&str>,
    local: bool,
) -> Result<String, DateTimeError> {
    let date = if let Some(date) = date {
        let date = if date.len() == 4 && date.chars().all(|c| c.is_ascii_digit()) {
            Cow::Owned(format!("00:00 {date}"))
        } else {
            date.into()
        };

        DateTime::new(&date, Some(&DateTimeZone::UTC), Some(now))?
    } else {
        *now
    };

    let tz = if local {
        DateTimeZone::local()?
    } else {
        DateTimeZone::UTC
    };

    date.into_offset(tz)?.format(format).map_err(Into::into)
}

/// Finds the first valid message from the list of keys given in `keys` and
/// returns that message, formatted using `cb` to replace any `$N` placeholders.
/// If `cb` returns `None`, no replacement occurs.
///
/// If the message is not found, returns a not-found string.
///
/// # Errors
///
/// * `callback` returns an error
pub fn format_message<'o, 'n: 'o, F, I, R, E>(
    messages: &'o Messages<'n>,
    keys: I,
    callback: F,
) -> Result<Cow<'o, str>, E>
where
    R: AsRef<str> + Default,
    I: IntoIterator<Item = R>,
    F: FnMut(&str) -> Result<Option<Cow<'o, str>>, E>,
{
    let mut last = R::default();
    for key in keys {
        if let Some(message) = messages
            .get(key.as_ref())
            .filter(|message| !matches!(*message, "" | "-"))
        {
            return format_raw_message(message, callback);
        // TODO: This is not in the default MW dictionary, it is in some other
        // dictionary from mediawiki-gadgets-ConvenientDiscussions, but that one
        // is lowercase. This is used by 'Template:Ambox'
        } else if key.as_ref() == "dot-separator" {
            return Ok(Cow::Borrowed("&nbsp;<b>·</b>&#32;"));
        }
        last = key;
    }

    let last = html_escape::encode_text(last.as_ref());
    let last = strtr(&last, &[("\u{0338}", "&#x338;")]);
    Ok(format!("⧼{last}⧽").into())
}

/// Formats a number similar to [`number_format`](https://php.net/number_format).
#[must_use]
pub fn format_number(n: f64, no_separators: bool) -> Cow<'static, str> {
    match n {
        f64::INFINITY => Cow::Borrowed("∞"),
        f64::NEG_INFINITY => Cow::Borrowed("\u{2212}∞"),
        n if n.is_nan() => Cow::Borrowed("Not a number"),
        n => {
            let f = strval(n);
            if no_separators {
                Cow::Owned(f)
            } else {
                let (n, d) = f.split_once('.').unwrap_or((&f, ""));
                let mut out = String::new();
                for chunk in n.as_bytes().rchunks(3).rev() {
                    if !out.is_empty() {
                        out.push(',');
                    }
                    // SAFETY: The chunk string is a Rust-formatted f64 which
                    // contains only ASCII characters.
                    out += unsafe { str::from_utf8_unchecked(chunk) };
                }
                if !d.is_empty() {
                    out.push('.');
                    // SAFETY: The chunk string is a Rust-formatted f64 which
                    // contains only ASCII characters.
                    out += unsafe { str::from_utf8_unchecked(d.as_bytes()) };
                }
                Cow::Owned(out)
            }
        }
    }
}

/// Formats a message, using `callback` to replace any `$N` placeholders in the
/// message. If `callback` returns `None`, no replacement occurs.
///
/// # Errors
///
/// * `callback` returns an error
pub fn format_raw_message<'a, E, F>(message: &'a str, mut callback: F) -> Result<Cow<'a, str>, E>
where
    F: FnMut(&str) -> Result<Option<Cow<'a, str>>, E>,
{
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d+)").unwrap());

    let mut out = String::new();
    let mut flushed = 0;
    for capture in RE.captures_iter(message) {
        let (_, [key]) = capture.extract();
        if let Some(value) = callback(key)? {
            let range = capture.get_match().range();
            out += &message[flushed..range.start];
            out += &value;
            flushed = range.end;
        }
    }

    Ok(if flushed == 0 {
        Cow::Borrowed(message)
    } else {
        out += &message[flushed..];
        Cow::Owned(out)
    })
}

/// Converts a MediaWiki language code to its corresponding BCP-47 code.
#[must_use]
pub fn lang_to_bcp47(code: &str) -> Cow<'_, str> {
    const NON_STANDARD: phf::Map<&str, &str> = phf::phf_map! {
        "cbk-zam" => "cbk",
        "de-formal" => "de-x-formal",
        "eml" => "egl",
        "en-rtl" => "en-x-rtl",
        "es-formal" => "es-x-formal",
        "hu-formal" => "hu-x-formal",
        "map-bms" => "jv-x-bms",
        "mo" => "ro-Cyrl-MD",
        "nrm" => "nrf",
        "nl-informal" => "nl-x-informal",
        "roa-tara" => "nap-x-tara",
        "simple" => "en-simple",
        "sr-ec" => "sr-Cyrl",
        "sr-el" => "sr-Latn",
        "crh-ro" => "crh-Latn-RO",
        "kk-cn" => "kk-Arab-CN",
        "kk-tr" => "kk-Latn-TR",
        "zh-cn" => "zh-Hans-CN",
        "zh-sg" => "zh-Hans-SG",
        "zh-my" => "zh-Hans-MY",
        "zh-tw" => "zh-Hant-TW",
        "zh-hk" => "zh-Hant-HK",
        "zh-mo" => "zh-Hant-MO",
    };

    let code = DEPRECATED_LANGUAGE_CODES
        .get(&to_ascii_lower(code))
        .copied()
        .unwrap_or(code);
    let code = NON_STANDARD.get(code).copied().unwrap_or(code);
    let mut out = String::new();
    let mut flushed = 0;
    let mut last = 0;
    let mut after_x = false;
    let iter = code
        .match_indices('-')
        .map(|(index, _)| index)
        .chain(iter::once(code.len()));
    for index in iter {
        let text = &code[last..index];
        let text = if !after_x && last != 0 && text.len() == 2 {
            to_ascii_upper(text)
        } else if !after_x && last != 0 && text.len() == 4 {
            ucfirst(text)
        } else {
            to_ascii_lower(text)
        };
        after_x = text == "x";
        if let Cow::Owned(text) = text {
            out += &code[flushed..last];
            out += &text;
            flushed = index;
        }
        last = index + 1;
    }

    if flushed == 0 {
        Cow::Borrowed(code)
    } else {
        out += &code[flushed..];
        Cow::Owned(out)
    }
}

/// Creates a URL for the given title using the given protocol, base URI, path,
/// and query string.
///
/// To create a protocol-relative URL, pass `Some("")` to `proto`.
///
/// The `query` and `fragment` arguments should *not* include any `?` or `#`
/// prefix.
///
/// The value passed to `fragment` will be anchor-encoded.
///
/// # Panics
///
/// * A write to the output buffer fails
pub fn make_url<P: core::fmt::Display>(
    base_uri: &url::Url,
    proto: Option<&str>,
    path: P,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let authority = base_uri.authority().unwrap_or("//localhost");
    let base_path = base_uri.path().trim_start_matches('/');

    let mut url = String::new();
    if let Some(proto) = proto {
        write!(url, "{proto}{authority}").unwrap();
    }
    if !base_path.is_empty() {
        write!(url, "/{base_path}").unwrap();
    }
    write!(url, "/{path}").unwrap();
    if let Some(query) = query {
        write!(url, "?{query}").unwrap();
    }
    if let Some(fragment) = fragment
        && !fragment.is_empty()
    {
        write!(url, "#{}", anchor_encode(fragment, AnchorEncodeMode::Html5)).unwrap();
    }
    url
}

/// Strips formatting characters from a numeric string.
#[must_use]
pub fn parse_formatted_number(s: &str) -> Cow<'_, str> {
    match s {
        "NaN" => "NAN".into(),
        "∞" => "INF".into(),
        "-∞" | "\u{2212}∞" => "-INF".into(),
        s => strtr(s, &[("\u{2212}", "-"), (",", "")]),
    }
}

/// Normalises `text` by converting runs of whitespace that might exist in
/// a normalised [`Title`](title::Title) to a single space character and
/// trimming. This is like `Sanitizer::normalizeSectionNameWhitespace`.
#[must_use]
pub fn normalize_section_name(text: &str) -> Cow<'_, str> {
    #[inline]
    fn spacelike(c: char) -> bool {
        matches!(c, ' ' | '_')
    }
    normalize_whitespace::<true>(text, spacelike, spacelike)
}

/// A generic function for normalising text by converting runs of whitespace to
/// a single space character and trimming the end and (optionally) start.
///
/// * `trimmable` should return true if the character `c` should be trimmed from
///   the start and end of `text`.
/// * `spacelike` should return true if the character `c` should be part of a
///   run of whitespace that gets collapsed to a single space character.
pub fn normalize_whitespace<const LTRIM: bool>(
    text: &str,
    mut trimmable: impl FnMut(char) -> bool,
    mut spacelike: impl FnMut(char) -> bool,
) -> Cow<'_, str> {
    let mut out = String::new();
    let mut flushed = 0;
    let mut iter = text.char_indices().peekable();

    while let Some((index, c)) = iter.next() {
        // Peek to avoid switching to owned-mode when encountering a single
        // space
        if trimmable(c) && (c != ' ' || matches!(iter.peek(), Some((_, c)) if trimmable(*c))) {
            // Non-space whitespace are converted to space, and runs of
            // whitespace are collapsed into a single character
            while iter.next_if(|(_, c)| trimmable(*c)).is_some() {}

            // This acts like `trim`, not emitting a space at the start
            // (`index == 0`) or end (`peek().is_none()`) of the text.
            if let Some((next_index, _)) = iter.peek() {
                out += &text[flushed..index];
                flushed = *next_index;
                // Bidi markers get stripped because “Sometimes they slip
                // into cut-n-pasted page titles”
                if (!LTRIM || index != 0) && spacelike(c) {
                    out.push(' ');
                }
            }
        }
    }

    if flushed == 0 {
        Cow::Borrowed(if LTRIM {
            text.trim_matches(trimmable)
        } else {
            text.trim_end_matches(trimmable)
        })
    } else {
        out += text[flushed..].trim_end_matches(trimmable);
        Cow::Owned(out)
    }
}

/// Converts an iterator of strings into a regular expression alternates
/// subexpression.
#[must_use]
pub fn regex_switch<I>(items: I) -> String
where
    I: Iterator,
    I::Item: AsRef<str>,
{
    let mut out = String::new();
    for item in items {
        if !out.is_empty() {
            out.push('|');
        }
        out += &regex::escape(item.as_ref());
    }
    out
}

/// Decodes a possibly URL-encoded title from a Wikitext link target.
#[must_use]
pub fn title_decode(target: &str) -> Cow<'_, str> {
    url_decode(target)
        .borrowed_or(|target| strtr(&target, &[("<", "&lt;"), (">", "&gt;")]).into_owned())
}

/// Converts the first letter in the string to Unicode lowercase, avoiding
/// allocation if it is already lowercase.
#[must_use]
pub fn to_lower_first(input: &str) -> Cow<'_, str> {
    let mut chars = input.chars();
    if let Some(first) = chars.next()
        && !first.is_lowercase()
    {
        Cow::Owned(format!("{}{}", first.to_lowercase(), chars.as_str()))
    } else {
        Cow::Borrowed(input)
    }
}

/// Converts the first letter in the string to Unicode uppercase, avoiding
/// allocation if it is already uppercase.
#[must_use]
pub fn to_upper_first(input: &str) -> Cow<'_, str> {
    let mut chars = input.chars();
    if let Some(first) = chars.next()
        && !first.is_uppercase()
    {
        Cow::Owned(format!("{}{}", first.to_uppercase(), chars.as_str()))
    } else {
        Cow::Borrowed(input)
    }
}

/// Percent-decodes a URL part.
#[inline]
#[must_use]
pub fn url_decode(input: &str) -> Cow<'_, str> {
    percent_encoding::percent_decode_str(input).decode_utf8_lossy()
}

/// Percent-encodes a URL part using the MediaWiki rules.
///
/// This is equivalent to `wfUrlencode`.
#[inline]
#[must_use]
pub fn url_encode(input: &str) -> Cow<'_, str> {
    libphp_rs::url_encode_alphabet(input, &WM_ALPHABET)
}

/// Percent-encodes a URL part.
#[inline]
#[must_use]
pub fn url_encode_bytes(input: &[u8]) -> Cow<'_, str> {
    libphp_rs::url_encode_bytes_alphabet(input, &WM_ALPHABET)
}

/// Percent-encodes a URL part following the MediaWiki rules for sanitized URLs,
/// which only encodes a subset of ASCII characters.
#[must_use]
pub fn url_encode_sanitized(input: &str) -> Cow<'_, str> {
    let mut out = String::new();
    let mut flushed = 0;
    for (cursor, b) in input.bytes().enumerate() {
        if matches!(
            b,
            b'\x00'..=b'\x1f' | b'"' | b'<' | b'>' | b'[' | b']' | b'|' | b'\x7f'
        ) {
            out += &input[flushed..cursor];
            out += percent_encoding::percent_encode_byte(b);
            flushed = cursor + 1;
        } else if b == b' ' {
            out += &input[flushed..cursor];
            out.push('+');
            flushed = cursor + 1;
        }
    }

    if flushed == 0 {
        Cow::Borrowed(input)
    } else {
        out += &input[flushed..];
        Cow::Owned(out)
    }
}

/// A map from deprecated MediaWiki language codes to their non-deprecated
/// replacements.
pub const DEPRECATED_LANGUAGE_CODES: phf::Map<&str, &str> = phf::phf_map! {
    "als" => "gsw",
    "bat-smg" => "sgs",
    "be-x-old" => "be-tarask",
    "fiu-vro" => "vro",
    "roa-rup" => "rup",
    "zh-classical" => "lzh",
    "zh-min-nan" => "nan",
    "zh-yue" => "yue",
};

/// The alphabet of characters to percent-encode when encoding URLs.
///
/// This is a combination of “all non-alphanumeric characters except `-_.`” from
/// PHP’s `urlencode`, and then MediaWiki also excludes `!$()*,/:;@~`.
const WM_ALPHABET: percent_encoding::AsciiSet = libphp_rs::URL_ENCODE_ALPHABET
    .remove(b'!')
    .remove(b'$')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b',')
    .remove(b'/')
    .remove(b':')
    .remove(b';')
    .remove(b'@')
    .remove(b'~');

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_html() {
        assert_eq!(
            decode_html("hello & world"),
            Cow::Borrowed("hello & world"),
            "non-entity should remain as-is"
        );
        assert_eq!(
            decode_html("hello&nbsp;world"),
            Cow::Owned::<str>(String::from("hello\u{00a0}world")),
            "entity should decode"
        );
        assert_eq!(
            decode_html("hello&oops;world"),
            Cow::Borrowed("hello&oops;world"),
            "invalid entity should remain as-is"
        );
        assert_eq!(
            decode_html("hello&;world"),
            Cow::Borrowed("hello&;world"),
            "invalid empty entity should remain as-is"
        );
        assert_eq!(
            decode_html("hello&nbsp world"),
            Cow::Borrowed("hello&nbsp world"),
            "html5 entity termination rules should not be used"
        );
        assert_eq!(
            decode_html("hello&רלמ;world"),
            Cow::Borrowed("hello\u{200f}world"),
            "special Hebrew RTL entity should decode"
        );
        assert_eq!(
            decode_html("hello&رلم;world"),
            Cow::Borrowed("hello\u{200f}world"),
            "special Arabic RTL entity should decode"
        );
        assert_eq!(
            decode_html("hello&#42;world"),
            Cow::Borrowed("hello*world"),
            "decimal entity should decode"
        );
        assert_eq!(
            decode_html("hello&#x42;world"),
            Cow::Borrowed("helloBworld"),
            "hexadecimal entity should decode"
        );
        assert_eq!(
            decode_html("hello&&nbsp;world"),
            Cow::Owned::<str>(String::from("hello&\u{00a0}world")),
            "incomplete entity should not interfere with later entity"
        );
    }

    #[test]
    fn test_lang_to_bcp47() {
        const TESTS: &[(&str, &str)] = &[
            ("en-ca-x-ca", "en-CA-x-ca"),
            ("sgn-be-fr", "sgn-BE-FR"),
            ("az-latn-x-latn", "az-Latn-x-latn"),
            ("sr-Latn-RS", "sr-Latn-RS"),
            ("az-arab-ir", "az-Arab-IR"),
            ("sl-nedis", "sl-nedis"),
            ("de-ch-1996", "de-CH-1996"),
            (
                "en-latn-gb-boont-r-extended-sequence-x-private",
                "en-Latn-GB-boont-r-extended-sequence-x-private",
            ),
            ("DE", "de"),
            ("fR", "fr"),
            ("ja", "ja"),
            ("zh-hans", "zh-Hans"),
            ("sr-cyrl", "sr-Cyrl"),
            ("sr-latn", "sr-Latn"),
            ("zh-cmn-hans-cn", "zh-cmn-Hans-CN"),
            ("cmn-hans-cn", "cmn-Hans-CN"),
            ("zh-yue-hk", "zh-yue-HK"),
            ("yue-hk", "yue-HK"),
            ("zh-hans-cn", "zh-Hans-CN"),
            ("sr-latn-RS", "sr-Latn-RS"),
            ("sl-rozaj", "sl-rozaj"),
            ("sl-rozaj-biske", "sl-rozaj-biske"),
            ("sl-nedis", "sl-nedis"),
            ("de-ch-1901", "de-CH-1901"),
            ("sl-it-nedis", "sl-IT-nedis"),
            ("hy-latn-it-arevela", "hy-Latn-IT-arevela"),
            ("de-de", "de-DE"),
            ("en-us", "en-US"),
            ("es-419", "es-419"),
            ("de-ch-x-phonebk", "de-CH-x-phonebk"),
            ("az-arab-x-aze-derbend", "az-Arab-x-aze-derbend"),
            ("x-whatever", "x-whatever"),
            ("qaa-qaaa-qm-x-southern", "qaa-Qaaa-QM-x-southern"),
            ("de-qaaa", "de-Qaaa"),
            ("sr-latn-qm", "sr-Latn-QM"),
            ("sr-qaaa-rs", "sr-Qaaa-RS"),
            ("en-us-u-islamcal", "en-US-u-islamcal"),
            ("zh-cn-a-myext-x-private", "zh-CN-a-myext-x-private"),
            ("en-a-myext-b-another", "en-a-myext-b-another"),
            ("als", "gsw"),
            ("bat-smg", "sgs"),
            ("be-x-old", "be-tarask"),
            ("fiu-vro", "vro"),
            ("roa-rup", "rup"),
            ("zh-classical", "lzh"),
            ("zh-min-nan", "nan"),
            ("zh-yue", "yue"),
            ("cbk-zam", "cbk"),
            ("de-formal", "de-x-formal"),
            ("eml", "egl"),
            ("en-rtl", "en-x-rtl"),
            ("es-formal", "es-x-formal"),
            ("hu-formal", "hu-x-formal"),
            ("kk-Arab", "kk-Arab"),
            ("kk-Cyrl", "kk-Cyrl"),
            ("kk-Latn", "kk-Latn"),
            ("map-bms", "jv-x-bms"),
            ("mo", "ro-Cyrl-MD"),
            ("nrm", "nrf"),
            ("nl-informal", "nl-x-informal"),
            ("roa-tara", "nap-x-tara"),
            ("simple", "en-simple"),
            ("sr-ec", "sr-Cyrl"),
            ("sr-el", "sr-Latn"),
            ("zh-cn", "zh-Hans-CN"),
            ("zh-sg", "zh-Hans-SG"),
            ("zh-my", "zh-Hans-MY"),
            ("zh-tw", "zh-Hant-TW"),
            ("zh-hk", "zh-Hant-HK"),
            ("zh-mo", "zh-Hant-MO"),
            ("zh-hans", "zh-Hans"),
            ("zh-hant", "zh-Hant"),
        ];

        for (code, expected) in TESTS {
            assert_eq!(lang_to_bcp47(code), *expected);
        }
    }
}
