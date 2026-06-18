//! Simple things which are useful.

pub mod svg;

use core::borrow::Borrow as _;
use std::borrow::Cow;

// SPDX-SnippetBegin
// SPDX-License-Identifier: CC-BY-SA-4.0
// SPDX-SnippetComment: https://stackoverflow.com/a/72179625/252087
/// An ergonomic extension trait for extending [`Cow`] borrows.
pub trait CowExt<'a, B>
where
    B: 'a + ToOwned + ?Sized,
{
    /// If `self` is borrowed, returns `self`. Otherwise, returns the result of
    /// calling `f` with the inner owned value.
    #[must_use]
    fn borrowed_or_else<F>(self, other: F) -> Cow<'a, B>
    where
        F: FnOnce(<B as ToOwned>::Owned) -> <B as ToOwned>::Owned;

    /// Makes a new `Cow` for an optional component of the borrowed data,
    /// extending the borrow if `self` is borrowed.
    #[must_use]
    fn filter_map<F, U>(self, f: F) -> Option<Cow<'a, U>>
    where
        F: for<'b> FnOnce(&'b B) -> Option<Cow<'b, U>>,
        U: 'a + ToOwned + ?Sized;

    /// Makes a new `Cow` using a `Cow`-returning callback. If `self` is
    /// `Cow::Borrowed` and `f` returns `Cow::Borrowed`, the borrow is extended.
    /// Otherwise, the result is moved (if owned) or converted to `Cow::Owned`
    /// (if borrowed).
    #[must_use]
    fn map<F, U>(self, f: F) -> Cow<'a, U>
    where
        F: for<'b> FnOnce(&'b B) -> Cow<'b, U>,
        U: 'a + ToOwned + ?Sized;

    /// Makes a new `Cow` using a reference-returning callback. If `self` is
    /// `Cow::Borrowed`, the borrow is extended. Otherwise, the result is
    /// converted to `Cow::Owned`.
    #[must_use]
    fn map_ref<F, U>(self, f: F) -> Cow<'a, U>
    where
        F: for<'b> FnOnce(&'b B) -> &'b U,
        U: 'a + ToOwned + ?Sized;

    /// If `self` is owned, returns the inner value of `self`. Otherwise,
    /// returns `None`.
    #[must_use]
    fn owned(self) -> Option<<B as ToOwned>::Owned>;

    /// If `self` is borrowed, returns `other`. Otherwise, returns the inner
    /// value of `self`.
    #[must_use]
    fn owned_or(self, other: <B as ToOwned>::Owned) -> <B as ToOwned>::Owned;

    /// If `self` is borrowed, returns `other`. Otherwise, returns the result of
    /// calling `f` with the inner owned value.
    #[must_use]
    fn owned_or_else<F, T>(self, other: T, f: F) -> T
    where
        F: for<'b> FnOnce(<B as ToOwned>::Owned) -> T;

    /// Makes a new `Cow` using a fallible `Cow`-returning callback. If `self`
    /// is `Cow::Borrowed` and `f` returns `Cow::Borrowed`, the borrow is
    /// extended. Otherwise, the result is moved (if owned) or converted to
    /// `Cow::Owned` (if borrowed).
    ///
    /// # Errors
    ///
    /// * if the callback returns an error
    fn try_map<F, U, E>(self, f: F) -> Result<Cow<'a, U>, E>
    where
        F: for<'b> FnOnce(&'b B) -> Result<Cow<'b, U>, E>,
        U: 'a + ToOwned + ?Sized;
}

impl<'a, B> CowExt<'a, B> for Cow<'a, B>
where
    B: 'a + ToOwned + ?Sized,
{
    fn borrowed_or_else<F>(self, other: F) -> Cow<'a, B>
    where
        F: FnOnce(<B as ToOwned>::Owned) -> <B as ToOwned>::Owned,
    {
        match self {
            b @ Cow::Borrowed(_) => b,
            Cow::Owned(o) => Cow::Owned(other(o)),
        }
    }

    fn filter_map<F, U>(self, f: F) -> Option<Cow<'a, U>>
    where
        F: for<'b> FnOnce(&'b B) -> Option<Cow<'b, U>>,
        U: 'a + ToOwned + ?Sized,
    {
        match self {
            Cow::Borrowed(v) => f(v),
            Cow::Owned(v) => f(v.borrow()).map(|v| Cow::Owned(v.into_owned())),
        }
    }

    fn map<F, U>(self, f: F) -> Cow<'a, U>
    where
        F: for<'b> FnOnce(&'b B) -> Cow<'b, U>,
        U: 'a + ToOwned + ?Sized,
    {
        match self {
            Cow::Borrowed(v) => f(v),
            Cow::Owned(v) => Cow::Owned(f(v.borrow()).into_owned()),
        }
    }

    fn map_ref<F, U>(self, f: F) -> Cow<'a, U>
    where
        F: for<'b> FnOnce(&'b B) -> &'b U,
        U: 'a + ToOwned + ?Sized,
    {
        match self {
            Cow::Borrowed(v) => Cow::Borrowed(f(v)),
            Cow::Owned(v) => Cow::Owned(f(v.borrow()).to_owned()),
        }
    }

    fn owned(self) -> Option<<B as ToOwned>::Owned> {
        match self {
            Cow::Borrowed(_) => None,
            Cow::Owned(o) => Some(o),
        }
    }

    fn owned_or(self, other: <B as ToOwned>::Owned) -> <B as ToOwned>::Owned {
        match self {
            Cow::Borrowed(_) => other,
            Cow::Owned(o) => o,
        }
    }

    fn owned_or_else<F, T>(self, other: T, f: F) -> T
    where
        F: for<'b> FnOnce(<B as ToOwned>::Owned) -> T,
    {
        match self {
            Cow::Borrowed(_) => other,
            Cow::Owned(o) => f(o),
        }
    }

    fn try_map<F, U, E>(self, f: F) -> Result<Cow<'a, U>, E>
    where
        F: for<'b> FnOnce(&'b B) -> Result<Cow<'b, U>, E>,
        U: 'a + ToOwned + ?Sized,
    {
        match self {
            Cow::Borrowed(v) => f(v),
            Cow::Owned(v) => Ok(Cow::Owned(f(v.borrow())?.into_owned())),
        }
    }
}
// SPDX-SnippetEnd

/// Scans `input` with `check`; if `check` returns true, runs `transform` on
/// the current and all subsequent bytes and returns an owned string.
#[expect(clippy::inline_always, reason = "hot path")]
#[inline(always)]
fn mutate_ascii(
    input: &str,
    check: impl Fn(&u8) -> bool,
    transform: impl Fn(&u8) -> u8,
) -> Cow<'_, str> {
    let bytes = input.as_bytes();
    for index in 0..bytes.len() {
        if check(&bytes[index]) {
            let mut out = Vec::with_capacity(input.len());
            out.extend(&bytes[..index]);
            out.extend(bytes[index..].iter().map(transform));
            // SAFETY: Changing ASCII bytes only does not invalidate UTF-8.
            return Cow::Owned(unsafe { String::from_utf8_unchecked(out) });
        }
    }
    Cow::Borrowed(input)
}

/// Scans `input` with `check`; if `check` returns true, run `transform` on
/// the rest of `input` starting from the current position and returns an owned
/// string.
#[expect(clippy::inline_always, reason = "hot path")]
#[inline(always)]
fn mutate_unicode(
    input: &str,
    check: impl Fn(char) -> bool,
    transform: impl Fn(&str) -> String,
) -> Cow<'_, str> {
    for (index, c) in input.char_indices() {
        if check(c) {
            let mut out = String::with_capacity(input.len());
            out += &input[..index];
            out += &transform(&input[index..]);
            return Cow::Owned(out);
        }
    }
    Cow::Borrowed(input)
}

/// Lowercases ASCII in the given `text` in `O(n+m)` time and without allocating
/// if the string is already lowercase.
#[must_use]
pub fn to_ascii_lower(input: &str) -> Cow<'_, str> {
    mutate_ascii(input, u8::is_ascii_uppercase, u8::to_ascii_lowercase)
}

/// Uppercases ASCII in the given `text` in `O(n+m)` time and without allocating
/// if the string is already uppercase.
#[must_use]
pub fn to_ascii_upper(input: &str) -> Cow<'_, str> {
    mutate_ascii(input, u8::is_ascii_lowercase, u8::to_ascii_uppercase)
}

/// Lowercases Unicode in the given `text`.
#[must_use]
pub fn to_lower(input: &str) -> Cow<'_, str> {
    mutate_unicode(input, char::is_uppercase, str::to_lowercase)
}

/// Uppercases Unicode in the given `text`.
#[must_use]
pub fn to_upper(input: &str) -> Cow<'_, str> {
    mutate_unicode(input, char::is_lowercase, str::to_uppercase)
}
