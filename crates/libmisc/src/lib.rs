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
    fn borrowed_or<F>(self, other: F) -> Cow<'a, B>
    where
        F: FnOnce(<B as ToOwned>::Owned) -> <B as ToOwned>::Owned;

    /// Makes a new `Cow` for an optional component of the borrowed data,
    /// extending the borrow if `self` is borrowed.
    #[must_use]
    fn filter_map<F>(self, f: F) -> Option<Self>
    where
        F: for<'b> FnOnce(&'b B) -> Option<Cow<'b, B>>,
        Self: Sized;

    /// Makes a new `Cow` using a `Cow`-returning callback. If `self` is
    /// `Cow::Borrowed` and `f` returns `Cow::Borrowed`, the borrow is extended.
    /// Otherwise, the result is moved (if owned) or converted to `Cow::Owned`
    /// (if borrowed).
    #[must_use]
    fn map<F>(self, f: F) -> Self
    where
        F: for<'b> FnOnce(&'b B) -> Cow<'b, B>;

    /// Makes a new `Cow` using a reference-returning callback. If `self` is
    /// `Cow::Borrowed`, the borrow is extended. Otherwise, the result is
    /// converted to `Cow::Owned`.
    #[must_use]
    fn map_ref<F>(self, f: F) -> Self
    where
        F: for<'b> FnOnce(&'b B) -> &'b B;

    /// If `self` is owned, returns `Some(self)`. Otherwise, returns `None`.
    #[must_use]
    fn owned(self) -> Option<Cow<'static, B>>;

    /// If `self` is borrowed, returns `other`. Otherwise, returns the result of
    /// calling `f` with the inner owned value.
    #[must_use]
    fn owned_or<F, T>(self, other: T, f: F) -> T
    where
        F: for<'b> FnOnce(<B as ToOwned>::Owned) -> T;
}

impl<'a, B> CowExt<'a, B> for Cow<'a, B>
where
    B: 'a + ToOwned + ?Sized,
{
    fn borrowed_or<F>(self, other: F) -> Cow<'a, B>
    where
        F: FnOnce(<B as ToOwned>::Owned) -> <B as ToOwned>::Owned,
    {
        match self {
            b @ Cow::Borrowed(_) => b,
            Cow::Owned(o) => Cow::Owned(other(o)),
        }
    }

    fn filter_map<F>(self, f: F) -> Option<Self>
    where
        F: for<'b> FnOnce(&'b B) -> Option<Cow<'b, B>>,
        Self: Sized,
    {
        match self {
            Cow::Borrowed(v) => f(v),
            Cow::Owned(v) => f(v.borrow()).map(|v| Cow::Owned(v.into_owned())),
        }
    }

    fn map<F>(self, f: F) -> Self
    where
        F: for<'b> FnOnce(&'b B) -> Cow<'b, B>,
    {
        match self {
            Cow::Borrowed(v) => f(v),
            Cow::Owned(v) => Cow::Owned(f(v.borrow()).into_owned()),
        }
    }

    fn map_ref<F>(self, f: F) -> Self
    where
        F: for<'b> FnOnce(&'b B) -> &'b B,
    {
        match self {
            Cow::Borrowed(v) => Cow::Borrowed(f(v)),
            Cow::Owned(v) => Cow::Owned(f(v.borrow()).to_owned()),
        }
    }

    fn owned(self) -> Option<Cow<'static, B>> {
        match self {
            Cow::Borrowed(_) => None,
            Cow::Owned(o) => Some(Cow::Owned(o)),
        }
    }

    fn owned_or<F, T>(self, other: T, f: F) -> T
    where
        F: for<'b> FnOnce(<B as ToOwned>::Owned) -> T,
    {
        match self {
            Cow::Borrowed(_) => other,
            Cow::Owned(o) => f(o),
        }
    }
}
// SPDX-SnippetEnd

/// Lowercases ASCII in the given `text`.
#[must_use]
pub fn to_ascii_lower(input: &str) -> Cow<'_, str> {
    for index in 0..input.len() {
        if input.as_bytes()[index].is_ascii_uppercase() {
            return Cow::Owned(format!(
                "{}{}",
                &input[..index],
                input[index..].to_ascii_lowercase()
            ));
        }
    }
    Cow::Borrowed(input)
}

/// Uppercases ASCII in the given `text`.
#[must_use]
pub fn to_ascii_upper(input: &str) -> Cow<'_, str> {
    for index in 0..input.len() {
        if input.as_bytes()[index].is_ascii_lowercase() {
            return Cow::Owned(format!(
                "{}{}",
                &input[..index],
                input[index..].to_ascii_uppercase()
            ));
        }
    }
    Cow::Borrowed(input)
}
