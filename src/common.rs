//! Functions used by both the Scribunto and ParserFunctions extensions which do
//! not correspond to a host language’s standard library functions (i.e. do not
//! correspond to PHP nor Lua built-ins).

use crate::{
    php::{DateTime, DateTimeError, DateTimeParseError, DateTimeZone, strtr},
    title::{self, Title},
};
use axum::http::Uri;
use core::fmt::{self, Write as _};
use html_escape::decode_html_entities;
use std::borrow::Cow;
use time::UtcOffset;

/// The alphabet of characters to percent-encode when encoding URLs.
const ALPHABET: percent_encoding::AsciiSet = percent_encoding::CONTROLS
    .add(b'%')
    .add(b'#')
    .add(b'\'')
    .add(b'"')
    .add(b'&')
    .add(b'<')
    .add(b'>')
    .add(b' ');

/// Percent-encodes a URL part.
pub fn url_encode(input: &str) -> percent_encoding::PercentEncode<'_> {
    percent_encoding::utf8_percent_encode(input, &ALPHABET)
}

/// Percent-encodes a URL part.
pub fn url_encode_bytes(input: &[u8]) -> percent_encoding::PercentEncode<'_> {
    percent_encoding::percent_encode(input, &ALPHABET)
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

/// Encodes section heading text into a format suitable for use as a URL anchor.
pub fn anchor_encode(s: &str) -> String {
    let s = decode_html_entities(s.trim_ascii());
    let id = title::normalize(&s);
    let mut end = 1024.min(id.len());
    while !id.is_char_boundary(end) {
        end -= 1;
    }
    url_encode(&strtr(&id[..end], &[(" ", "_")])).to_string()
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
