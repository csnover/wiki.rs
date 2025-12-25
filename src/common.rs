//! Functions used by both the Scribunto and ParserFunctions extensions which do
//! not correspond to a host language’s standard library functions (i.e. do not
//! correspond to PHP nor Lua built-ins).

use crate::{
    php::{DateTime, DateTimeError, DateTimeParseError, DateTimeZone, strtr, strval},
    title::{self, Title},
};
use axum::http::Uri;
use core::fmt::{self, Write as _};
use html_escape::NAMED_ENTITIES;
use regex::Regex;
use std::{borrow::Cow, sync::LazyLock};
use time::UtcOffset;

/// The i18n dictionary from MediaWiki.
pub(crate) static MESSAGES: LazyLock<serde_json::Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("../res/i18n/en.json")).unwrap());

/// Encodes section heading text into a format suitable for use as a URL anchor.
pub fn anchor_encode(s: &str) -> String {
    let s = decode_html(s.trim_ascii());
    let id = title::normalize(&s);
    let end = id.floor_char_boundary(1024);
    url_encode(&strtr(&id[..end], &[(" ", "_")])).to_string()
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
pub fn format_date(
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

        DateTime::new(&date, Some(&DateTimeZone::UTC))?
    } else {
        now.clone()
    };

    let tz = if local {
        DateTimeZone::Offset(UtcOffset::current_local_offset().map_err(DateTimeParseError::from)?)
    } else {
        DateTimeZone::Offset(UtcOffset::UTC)
    };

    date.into_offset(tz)?.format(format).map_err(Into::into)
}

/// Finds the first valid message from the list of keys given in `keys` and
/// returns that message, formatted using `cb` to replace any `$N` placeholders.
/// If `cb` returns `None`, no replacement occurs.
///
/// If the message is not found, returns a not-found string.
pub fn format_message<'a, F, I, R, E>(keys: I, cb: F) -> Result<Cow<'static, str>, E>
where
    R: AsRef<str> + Default,
    I: IntoIterator<Item = R>,
    F: FnMut(&str) -> Result<Option<Cow<'a, str>>, E>,
{
    let mut last = R::default();
    for key in keys {
        if let Some(message) = MESSAGES
            .get(key.as_ref().to_lowercase())
            .and_then(serde_json::Value::as_str)
            .filter(|message| !matches!(*message, "" | "-"))
        {
            return format_raw_message(message, cb);
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

/// Formats a message, using `cb` to replace any `$N` placeholders in the
/// message. If `cb` returns `None`, no replacement occurs.
pub fn format_raw_message<'a, E, F>(message: &str, mut cb: F) -> Result<Cow<'_, str>, E>
where
    F: FnMut(&str) -> Result<Option<Cow<'a, str>>, E>,
{
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d+)").unwrap());

    let mut out = String::new();
    let mut flushed = 0;
    for capture in RE.captures_iter(message) {
        let (_, [key]) = capture.extract();
        if let Some(value) = cb(key)? {
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

/// Creates a URL for the given title using the given protocol, base URI, path,
/// and query string.
pub fn make_url(
    proto: Option<&str>,
    base_uri: &Uri,
    title: &str,
    query: Option<&str>,
    is_local: bool,
) -> Result<String, fmt::Error> {
    let title = Title::new(title, None);
    let mut url = String::new();
    if let Some(proto) = proto {
        url += proto;
    }
    if !is_local {
        url += "//";
    }
    let (authority, base_path) = if let Some(authority) = base_uri.authority() {
        (authority.as_str(), base_uri.path())
    } else if base_uri.path().starts_with("//") {
        let (_, authority) = base_uri.path().split_at(2);
        authority.split_once('/').unwrap_or((authority, ""))
    } else {
        ("localhost", base_uri.path())
    };
    if !is_local {
        url += authority;
    }
    if !base_path.is_empty() {
        url.push('/');
        url += base_path;
    }
    write!(url, "/article/{}", title.partial_url())?;
    if let Some(query) = query {
        write!(url, "?{query}")?;
    }
    Ok(url)
}

/// Strips formatting characters from a numeric string.
pub fn parse_formatted_number(s: &str) -> Cow<'_, str> {
    match s {
        "NaN" => "NAN".into(),
        "∞" => "INF".into(),
        "-∞" | "\u{2212}∞" => "-INF".into(),
        s => strtr(s, &[("\u{2212}", "-"), (",", "")]),
    }
}

/// Decodes a possibly URL-encoded title from a Wikitext link target.
pub(super) fn title_decode(target: &str) -> Cow<'_, str> {
    let mut target = Cow::Borrowed(target);
    if target.contains('%') {
        if let Cow::Owned(text) = url_decode(&target) {
            target = Cow::Owned(text);
        }
        if let Cow::Owned(text) = strtr(&target, &[("<", "&lt;"), (">", "&gt;")]) {
            target = Cow::Owned(text);
        }
    }
    target
}

/// Percent-decodes a URL part.
#[inline]
pub fn url_decode(input: &str) -> Cow<'_, str> {
    percent_encoding::percent_decode_str(input).decode_utf8_lossy()
}

/// Percent-encodes a URL part.
#[inline]
pub fn url_encode(input: &str) -> percent_encoding::PercentEncode<'_> {
    percent_encoding::utf8_percent_encode(input, &ALPHABET)
}

/// Percent-encodes a URL part.
#[inline]
pub fn url_encode_bytes(input: &[u8]) -> percent_encoding::PercentEncode<'_> {
    percent_encoding::percent_encode(input, &ALPHABET)
}

/// The alphabet of characters to percent-encode when encoding URLs.
const ALPHABET: percent_encoding::AsciiSet = percent_encoding::CONTROLS
    .add(b'%')
    .add(b'#')
    .add(b'\'')
    .add(b'"')
    .add(b'&')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b']')
    .add(b' ');

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
}
