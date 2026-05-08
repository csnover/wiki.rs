//! Simple things which are useful.

use core::borrow::Borrow as _;
use std::borrow::Cow;

pub mod svg;

// SPDX-SnippetBegin
// SPDX-License-Identifier: CC-BY-SA-4.0
// SPDX-SnippetComment: https://stackoverflow.com/a/72179625/252087
/// An ergonomic extension trait for extending [`Cow`] borrows.
pub trait CowExt<'a, B>
where
    B: 'a + ToOwned + ?Sized,
{
    /// Makes a new `Cow` for an optional component of the borrowed data,
    /// extending the borrow if `self` is borrowed.
    #[must_use]
    fn filter_map<F>(self, f: F) -> Option<Self>
    where
        F: for<'b> FnOnce(&'b B) -> Option<Cow<'b, B>>,
        Self: Sized;

    /// Maps the value in a `Cow`, extending the borrow if `self` is borrowed.
    #[must_use]
    fn map<F>(self, f: F) -> Self
    where
        F: for<'b> FnOnce(&'b B) -> Cow<'b, B>;

    /// If `self` is borrowed, reborrows the value. Otherwise, converts the
    /// result of `f` into an owned value.
    #[must_use]
    fn map_ref<F>(self, f: F) -> Self
    where
        F: for<'b> FnOnce(&'b B) -> &'b B;

    /// If `self` is owned, returns `Some(self)`. Otherwise, returns `None`.
    #[must_use]
    fn owned(self) -> Option<Cow<'static, B>>;

    /// If `self` is borrowed, returns `other`. Otherwise, takes the result of
    /// `f` as an owned value.
    #[must_use]
    fn owned_or<F, T>(self, other: T, f: F) -> T
    where
        F: for<'b> FnOnce(<B as ToOwned>::Owned) -> T;
}

impl<'a, B> CowExt<'a, B> for Cow<'a, B>
where
    B: 'a + ToOwned + ?Sized,
{
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
