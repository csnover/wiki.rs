//! Implementation of the Graph extension.
//!
//! ```text
//!    ┌─────────────┐
//!    │ mod data    │
//!    ├╌╌╌╌╌╌╌╌╌╌╌╌╌┤
//!    │ (serde)     │
//!    │↓Source      │ · Describes where to get the raw data
//!    │↓Format      │ · Converts the types of fields on data objects
//!    │↓Transform   │ · Adds/modifies fields on data objects
//!    │↓Modify      │ · Adds/removes data objects in response to signals
//!    └↑────────────┘
//!   ┌→• Node₁        · A processing node for the renderer, one per data object per mark
//!  ┌┼→• Scale        · Translates data values to visual values
//!  ┆┆ ┊ domain         · The data input values
//!  ┆┊ ┊ range          · The visual output values
//!  ┆┆┌─────────────┐
//!  ┆┆│ mod propset │
//!  ┆┆├╌╌╌╌╌╌╌╌╌╌╌╌╌┤
//!  ┆└┼  FieldRef   │ · Looks up a value from a data object or parent mark
//!  └╌┼↕ ValueRef   │ · Combines a literal value or field lookup with a scale lookup
//!    │↑ Rule       │ · Picks one value from a set of options according to a condition
//!    │↑ Propset    │ · The defined visual properties for a mark
//!    └↑────────────┘
//!     • Mark         · A visual object (rect, symbol, line, etc.)
//!     ┊ ↖ Axis         ※ Axis and Legend are just sugar for marks, and are
//!     ┊ ↖ Legend         internally converted to marks
//!     ↑ Container    · Defines a collection of visualisations (axes, marks, etc.)
//!     • Spec           ※ Spec is just a Container + globals
//!     ┊ ↖ Predicates · Defines conditions for `Rule` using a domain-specific language
//!     ┊ ↖ Signals₂   · Defines dynamic data sources for `ValueRef` value lookups
//!     ↑ Renderer     · Walks all the marks, querying their visual properties,
//!                      and emits an SVG image
//! ```
//!
//! ₁. Node objects are a surrogate for the much more complex scene tree that
//!    the Vega runtime uses. This does not work perfectly because Vega breaks
//!    the abstraction and leaks all of its internals via `FieldRef::Group`.
//! ₂. Signals are not supported or implemented since this is a static image
//!    generator, and signals are entirely about event-driven data updates.

use crate::php::{DateTime, DateTimeZone};
use core::cell::RefCell;
use either::Either;
use rand::rngs::SmallRng;
use serde_json_borrow::Value;
use std::borrow::Cow;
use time::Duration;

mod axis;
mod data;
mod expr;
mod geo;
mod legend;
mod mark;
mod predicate;
mod propset;
mod renderer;
mod scale;
mod spec;
mod template;
#[cfg(test)]
mod tests;
mod tick;
mod transform;

/// A Graph error.
#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    /// Axis references an unknown scale.
    #[error("could not find scale '{1}' for {0:?} axis")]
    AxisScale(axis::Kind, String),
    /// Formatter spec error.
    #[error(transparent)]
    Format(#[from] renderer::FormatError),
    /// Legend references an unknown scale.
    #[error("could not find scale '{0}' for legend")]
    LegendScale(String),
    /// Serialisation failed.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// Unsupported version.
    #[error("unsupported spec version {0}")]
    Version(i32),
    /// XML library dun goofed.
    #[error(transparent)]
    Xml(#[from] minidom::Error),
}

/// The Graph result type.
pub(super) type Result<T, E = Error> = core::result::Result<T, E>;

/// Converts a Graph specification to an SVG.
pub(super) fn spec_to_svg(spec: &str, now: DateTime) -> Result<String> {
    let spec = serde_json::from_str::<spec::Spec<'_>>(spec)?;

    if let Some(version) = spec.version
        && version != 2
    {
        return Err(Error::Version(version));
    }

    let svg = renderer::render(&spec, now)?;

    let mut out = Vec::new();
    minidom::Element::write_to(&svg, &mut out)?;
    // SAFETY: We just wrote this from strs.
    Ok(unsafe { String::from_utf8_unchecked(out) })
}

/// Zero epsilon.
const EPSILON: f64 = 1.0e-6;

/// A double-ended sized iterator.
trait DoubleSizeIterator: DoubleEndedIterator + ExactSizeIterator {}
impl<T> DoubleSizeIterator for T where T: DoubleEndedIterator + ExactSizeIterator + ?Sized {}

/// A data processing node.
#[derive(Clone, Debug)]
struct Node<'s, 'b> {
    /// The currently processing data object.
    pub item: &'b Value<'s>,
    /// The data set for this node.
    pub data: Cow<'b, [Value<'s>]>,
    /// The mark for this node.
    pub mark: Option<&'b mark::Mark<'s>>,
    /// The “current” time.
    pub now: DateTime,
    /// The parent node, if any.
    pub parent: Option<&'b Node<'s, 'b>>,
    /// The pseudo-random number generator.
    pub rng: &'b RefCell<SmallRng>,
    /// The root spec.
    pub spec: &'b spec::Spec<'s>,
}

impl<'s, 'b> Node<'s, 'b> {
    /// Creates a new node.
    fn new(spec: &'b spec::Spec<'s>, rng: &'b RefCell<SmallRng>, now: DateTime) -> Self {
        Self {
            item: &Value::Null,
            data: Cow::Borrowed(core::slice::from_ref(&Value::Null)),
            mark: None,
            now,
            parent: None,
            rng,
            spec,
        }
    }

    /// Returns the data from the data set with the given `name`.
    fn data_values(&self, name: &str) -> Option<&[Value<'s>]> {
        self.spec.data(name).map(|data| data.values(self))
    }

    /// Returns the correct data set for a faceted mark group child.
    ///
    /// Deep within the bowels of Vega lies a mechanism that causes a group mark
    /// with a faceted data set to pass the `values` property to the child
    /// marks. Four hours of tracing, breakpoints, watchpoints, and static
    /// analysis did not fully illuminate exactly how it does this, other than
    /// that when `Facetor` runs and has a `_facet` property (which happens only
    /// `Facetor#facet` is called, and the only place that is called is from
    /// `Facet#aggr`), it generates extra global data sets, then sets a
    /// `_facetID` on some data object with the name of the new data set, which
    /// is then passed by `buildMarks` to `Builder#init` and used as the
    /// fallback if a mark does not have its own `from`.
    fn faceted_data(&'b self) -> &'b [Value<'s>] {
        if self.mark.as_ref().is_some_and(|mark| mark.is_group())
            && let Some(Value::Array(data)) = self.item.get("values")
        {
            data
        } else {
            &self.data
        }
    }

    /// Walks up the node tree starting with the current node, invoking `f` for
    /// each node until it returns `Some` value.
    #[inline]
    fn find<'a, F, T>(&'a self, f: F) -> Option<T>
    where
        F: Fn(&'a Node<'s, '_>, Either<&'a mark::Mark<'s>, &'a spec::Spec<'s>>) -> Option<T>,
    {
        let mut last = self;
        let mut candidate = Some(self);
        while let Some(node) = candidate {
            if let value @ Some(_) = node.mark.and_then(|mark| f(node, Either::Left(mark))) {
                return value;
            }
            last = node;
            candidate = node.parent;
        }
        f(last, Either::Right(self.spec))
    }

    /// Returns an iterator that generates a `Node` for each data object in the
    /// node.
    fn for_each_item(&'b self) -> impl DoubleSizeIterator<Item = Node<'s, 'b>> {
        self.data.iter().map(|item| {
            if let Some(mark) = self.mark {
                mark.invalidate_caches();
            }
            Self {
                item,
                data: Cow::Borrowed(&self.data),
                mark: self.mark,
                now: self.now,
                parent: self.parent,
                rng: self.rng,
                spec: self.spec,
            }
        })
    }

    /// Gets the height of the nearest container.
    fn height(&self) -> f64 {
        self.find(|node, candidate| match candidate {
            Either::Left(mark) => mark.height(node),
            Either::Right(spec) => Some(spec.height()),
        })
        .unwrap()
    }

    /// Gets the nearest mark with the given name.
    fn mark(&self, name: &str) -> Option<(&mark::Mark<'s>, Node<'s, '_>)> {
        self.find(|node, candidate| {
            match candidate {
                Either::Left(mark) => mark.mark(name),
                Either::Right(spec) => spec.mark(name),
            }
            .map(|mark| (mark, node.with_child_mark(mark)))
        })
    }

    /// Gets the nearest scale with the given name.
    fn scale(&'b self, name: &str) -> Option<ScaleNode<'s, 'b>> {
        self.find(|node, candidate| {
            match candidate {
                Either::Left(mark) => mark.scale(name),
                Either::Right(spec) => spec.scale(name),
            }
            .map(|scale| ScaleNode { node, scale })
        })
    }

    /// Gets the width of the nearest container.
    fn width(&self) -> f64 {
        self.find(|node, candidate| match candidate {
            Either::Left(mark) => mark.width(node),
            Either::Right(spec) => Some(spec.width()),
        })
        .unwrap()
    }

    /// Creates a new node for a child mark of a group.
    fn with_child_mark(&'b self, mark: &'b mark::Mark<'s>) -> Node<'s, 'b> {
        let data = if let Some(data) = mark.from.as_ref().and_then(|from| from.get(self)) {
            data
        } else {
            Cow::Borrowed(self.faceted_data())
        };

        Self {
            item: self.item,
            data,
            mark: Some(mark),
            now: self.now,
            parent: Some(self),
            rng: self.rng,
            spec: self.spec,
        }
    }

    /// Creates a new node for an expression evaluator.
    fn with_child_data(&'b self, item: &'b Value<'s>) -> Node<'s, 'b> {
        Self {
            item,
            data: Cow::Borrowed(core::slice::from_ref(item)),
            mark: self.mark,
            now: self.now,
            parent: Some(self),
            rng: self.rng,
            spec: self.spec,
        }
    }

    /// Gets a visual property of the current node.
    fn visual(&self, key: &str) -> Option<Value<'s>> {
        match key {
            "width" => Some(self.width().into()),
            "height" => Some(self.height().into()),
            key => self
                .mark
                .and_then(|mark| mark.propset(propset::Kind::Enter)?.get(key, self)),
        }
    }
}

/// A scale node.
///
/// Referential scales are built from data associated with the [`Container`]
/// in which they were declared, not the node which is currently being
/// processed. In particular, nested width/height scales must not try to use
/// the currently processing node as the source of data or this will lead to
/// the wrong values being used, or infinite recursion if a child mark uses
/// a parent width/height scale to do resolve their own width/height
/// properties.
///
/// This struct binds together a scale and its associated container node so that
/// calls always use the correct associated node.
///
/// [`Container`]: super::spec::Container
#[derive(Clone, Copy, Debug)]
struct ScaleNode<'s, 'b> {
    /// The associated container node for the scale.
    node: &'b Node<'s, 'b>,
    /// The scale.
    scale: &'b scale::Scale<'s>,
}

impl<'s> ScaleNode<'s, '_> {
    /// Applies this scale to the given input value, returning the scaled value.
    fn apply(&self, value: &Value<'s>, invert: bool) -> Value<'s> {
        self.scale.apply(self.node, value, invert)
    }

    /// Gets the minimum and maximum for a quantitative domain. The returned
    /// values are guaranteed to be sorted.
    fn input_range(&self) -> (f64, f64) {
        self.scale.input_range(self.node)
    }

    /// Returns true if this is a scale with discrete output values.
    fn is_discrete(&self) -> bool {
        self.scale.is_discrete()
    }

    /// Returns true if this is an ordinal (categorical) scale.
    fn is_ordinal(&self) -> bool {
        self.scale.is_ordinal()
    }

    /// Returns true if this is a time scale.
    fn is_time(&self) -> bool {
        self.scale.is_time()
    }

    /// Gets an iterator of approximately `count` representative values from the
    /// domain, or the domain itself if this is not a quantitative scale.
    fn ticks(&self, count: f64) -> Vec<Value<'s>> {
        self.scale.ticks(self.node, count)
    }

    /// Gets the calculated width of the bands in the range.
    fn range_band(&self) -> f64 {
        self.scale.range_band(self.node)
    }

    /// Gets the sorted minimum and maximum for a range. The returned values are
    /// guaranteed to be sorted.
    fn range_extent(&self) -> (f64, f64) {
        self.scale.range_extent(self.node)
    }
}

/// Creates an extent from an optional `min` and `max` and list of `values`.
fn make_extent(
    min: Option<f64>,
    max: Option<f64>,
    values: impl IntoIterator<Item = f64>,
) -> (f64, f64) {
    let values = values.into_iter();
    // Things get weird if only a `min` or `max` is given and then it turns out
    // that value is larger/smaller than the actual maximum/minimum because the
    // values are not swapped, but this matches the behaviour of Vega/datalib.
    match (min, max) {
        (Some(min), Some(max)) => (min, max),
        (Some(min), None) => (min, values.reduce(f64::max).unwrap_or(min)),
        (None, Some(max)) => (values.reduce(f64::min).unwrap_or(max), max),
        (None, None) => values.fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        }),
    }
}

/// Extension trait for timestamps.
trait TimeExt: Sized {
    /// Gets a [`DateTime`] object for the given value in milliseconds since
    /// the Unix epoch in the given time zone.
    fn from_f64(value: f64, utc: bool) -> Self;

    /// Gets a timestamp as milliseconds since the Unix epoch.
    #[inline]
    fn into_f64(self) -> f64 {
        self.to_f64()
    }

    /// Gets a timestamp as milliseconds since the Unix epoch.
    fn to_f64(&self) -> f64;
}

impl TimeExt for DateTime {
    fn from_f64(value: f64, utc: bool) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "matches ES2026 §§21.4.2.1, 21.4.1.31"
        )]
        let date = DateTime::UNIX_EPOCH + Duration::milliseconds(value as i64);
        if utc {
            date
        } else {
            date.into_offset(DateTimeZone::local().unwrap()).unwrap()
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "ms is smallest precision so truncation is fine, and ECMAScript defines valid date range of ±8.65e15 so no loss will occur"
    )]
    #[inline]
    fn to_f64(&self) -> f64 {
        const MS_PER_NS: i128 = 1_000_000;
        (self.unix_timestamp_nanos() / MS_PER_NS) as f64
    }
}
