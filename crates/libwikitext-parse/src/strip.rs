//! Functions for handling strip markers.

use super::{MARKER_PREFIX, MARKER_SUFFIX};
use core::convert::Infallible;
use memchr::memmem;
use std::{borrow::Cow, sync::LazyLock};

/// Invokes callback `f` for each run of text delimited by strip markers.
///
/// The callback should return `Some(string)` if it wants to replace the
/// text run, or `None` if it wants the text to be kept as-is.
#[expect(clippy::missing_panics_doc, reason = "cannot panic")]
#[inline]
#[must_use]
pub fn for_each_non_marker<'a, F>(body: &'a str, mut f: F) -> Cow<'a, str>
where
    F: FnMut(&'a str) -> Option<Cow<'a, str>>,
{
    try_for_each_non_marker(body, |marker| Ok::<_, Infallible>(f(marker))).unwrap()
}

/// Invokes callback `f` for each strip marker index in the given text.
///
/// The callback should return `Some(string)` if it wants to replace the
/// marker, or `None` if it wants the marker to be kept as-is in the text.
#[must_use]
pub fn for_each_marker_key<'a, F>(body: &str, mut f: F) -> Cow<'_, str>
where
    F: FnMut(&str) -> Option<Cow<'a, str>>,
{
    let mut out = String::new();
    let mut flushed = 0;
    let mut cursor = 0;
    while let Some(before) = FIND_PREFIX.find(&body.as_bytes()[cursor..])
        && let before = cursor + before
        && let start = before + MARKER_PREFIX.len()
        && let Some(len) = FIND_SUFFIX.find(&body.as_bytes()[start..])
    {
        let end = start + len;
        let key = &body[start..end];
        cursor = end + MARKER_SUFFIX.len();
        if let Some(replacement) = f(key) {
            out += &body[flushed..before];
            out += &replacement;
            flushed = cursor;
        }
    }

    if flushed == 0 {
        Cow::Borrowed(body)
    } else {
        out += &body[cursor..];
        Cow::Owned(out)
    }
}

/// Returns the index of the strip marker from a strip marker key.
///
/// The strip marker key must be formatted in this specific way because it
/// is exposed to modules, and of course some of them like 'Module:Infobox'
/// rely on this implementation detail.
///
/// # Panics
///
/// * `key` is malformed
#[must_use]
pub fn key_index(key: &str) -> usize {
    let (_, index) = key
        .strip_prefix('-')
        .expect("buggy hyphen")
        .rsplit_once('-')
        .expect("hyphenated marker key");
    usize::from_str_radix(index, 16).expect("hexadecimal index")
}

/// Removes all strip markers from the given text.
#[inline]
#[must_use]
pub fn kill(body: &str) -> Cow<'_, str> {
    for_each_marker_key(body, |_| Some("".into()))
}

/// Invokes callback `f` for each run of text delimited by strip markers.
///
/// The callback should return `Some(string)` if it wants to replace the
/// text run, or `None` if it wants the text to be kept as-is.
///
/// # Errors
///
/// * `f` returns an error
pub fn try_for_each_non_marker<'a, E, F>(body: &'a str, mut f: F) -> Result<Cow<'a, str>, E>
where
    F: FnMut(&'a str) -> Result<Option<Cow<'a, str>>, E>,
{
    let mut out = String::new();
    let mut flushed = 0;
    let mut cursor = 0;

    while cursor != body.len() {
        let end = FIND_PREFIX
            .find(&body.as_bytes()[cursor..])
            .map_or(body.len(), |pos| cursor + pos);
        if let Some(replacement) = f(&body[cursor..end])? {
            out += &body[flushed..cursor];
            out += &replacement;
            flushed = end;
        }

        cursor = FIND_SUFFIX
            .find(&body.as_bytes()[end..])
            .map_or(body.len(), |pos| end + pos + MARKER_SUFFIX.len());
    }

    Ok(if flushed == 0 {
        Cow::Borrowed(body)
    } else {
        out += &body[flushed..];
        Cow::Owned(out)
    })
}

/// A precomputed finder for [`MARKER_PREFIX`].
static FIND_PREFIX: LazyLock<memmem::Finder<'static>> =
    LazyLock::new(|| memmem::Finder::new(MARKER_PREFIX.as_bytes()));

/// A precomputed finder for [`MARKER_SUFFIX`].
static FIND_SUFFIX: LazyLock<memmem::Finder<'static>> =
    LazyLock::new(|| memmem::Finder::new(MARKER_SUFFIX.as_bytes()));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markers() {
        let text = format!(
            "0123{MARKER_PREFIX}-a-0{MARKER_SUFFIX}{MARKER_PREFIX}-a-1{MARKER_SUFFIX}abcd{MARKER_PREFIX}-b-a{MARKER_SUFFIX}4567"
        );
        let result = for_each_marker_key(&text, |key| {
            let index = key_index(key);
            if index == 0 {
                Some(Cow::Borrowed("?"))
            } else if index == 10 {
                Some(Cow::Borrowed("!"))
            } else if index == 1 {
                None
            } else {
                panic!("bogus index {index:?}");
            }
        });
        assert_eq!(
            result,
            Cow::Owned::<str>(format!("0123?{MARKER_PREFIX}-a-1{MARKER_SUFFIX}abcd!4567"))
        );
    }

    #[test]
    fn test_strip_non_markers() {
        let text = format!(
            "0123{MARKER_PREFIX}-a-0{MARKER_SUFFIX}{MARKER_PREFIX}-a-1{MARKER_SUFFIX}abcd{MARKER_PREFIX}-b-a{MARKER_SUFFIX}4567"
        );
        let result = for_each_non_marker(&text, |text| {
            if text == "0123" {
                Some(Cow::Borrowed("?"))
            } else if text == "abcd" {
                Some(Cow::Borrowed("!"))
            } else if text == "4567" {
                None
            } else if text.is_empty() {
                Some(Cow::Borrowed("."))
            } else {
                panic!("bogus text {text:?}");
            }
        });
        assert_eq!(
            result,
            Cow::Owned::<str>(format!(
                "?{MARKER_PREFIX}-a-0{MARKER_SUFFIX}.{MARKER_PREFIX}-a-1{MARKER_SUFFIX}!{MARKER_PREFIX}-b-a{MARKER_SUFFIX}4567"
            ))
        );
    }

    #[test]
    fn test_strip_non_markers_end() {
        let text =
            format!("0123{MARKER_PREFIX}-a-0{MARKER_SUFFIX}{MARKER_PREFIX}-a-1{MARKER_SUFFIX}4567");
        let result = for_each_non_marker(&text, |text| {
            if text == "0123" || text.is_empty() {
                None
            } else if text == "4567" {
                Some(Cow::Borrowed("!"))
            } else {
                panic!("bogus text {text:?}");
            }
        });
        assert_eq!(
            result,
            Cow::Owned::<str>(format!(
                "0123{MARKER_PREFIX}-a-0{MARKER_SUFFIX}{MARKER_PREFIX}-a-1{MARKER_SUFFIX}!"
            ))
        );
    }
}
