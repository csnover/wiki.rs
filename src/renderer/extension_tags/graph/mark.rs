//! Types and functions for the plotted parts of a visualisation.

use super::{
    Node,
    data::{IgnoredAny, ValueExt as _},
    propset::{Getter, Kind as PropsetKind, NumberProperty, Propset, from_value_impl},
    scale::Scale,
    spec::Container,
    transform::Transform,
};
use serde_json_borrow::Value;
use std::{borrow::Cow, cell::OnceCell};

/// A visualisation.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Mark<'s> {
    /// A function to modify the properties of a mark immediately before it is
    /// drawn.
    #[serde(skip)]
    pub encoder: Option<Box<dyn Encoder>>,
    /// The kind of mark.
    #[serde(borrow, flatten)]
    pub kind: Kind<'s>,
    /// The name of the mark. Used to look up group marks and to give a CSS
    /// class to a mark.
    #[serde(borrow, default)]
    pub name: Option<Cow<'s, str>>,
    /// The data to visualise. If undefined, a “default, single element data
    /// set” is used.
    #[serde(borrow, default)]
    pub from: Option<MarkData<'s>>,
    /// The transition delay for mark updates, in milliseconds.
    #[serde(borrow, default, rename = "delay")]
    _delay: IgnoredAny<Option<NumberProperty<'s>>>,
    /// The easing function to use for mark updates.
    #[serde(default, rename = "ease")]
    _ease: IgnoredAny<Option<EasingFunction>>,
    /// Undocumented. If true, allows pointer interaction with the mark.
    #[serde(default, rename = "interactive")]
    _interactive: IgnoredAny<bool>,
    /// A data field to use as a unique key for data binding. When a
    /// visualization’s data is updated, the key value will be used to match
    /// data elements to existing mark instances.
    #[serde(borrow, default, rename = "key")]
    _key: IgnoredAny<Option<Cow<'s, str>>>,
    /// The visual properties of the mark. For group marks, these properties are
    /// inherited by child marks.
    #[serde(borrow, default)]
    properties: Option<Propsets<'s>>,
    /// Cached merged enter + update propset.
    #[serde(skip)]
    propset_cache: OnceCell<Option<Propset<'s>>>,
}

impl<'s> Mark<'s> {
    /// Creates a new mark with the given kind and initial properties.
    pub fn new(
        kind: Kind<'s>,
        name: Option<Cow<'s, str>>,
        from: Option<MarkData<'s>>,
        properties: Propset<'s>,
    ) -> Self {
        Self {
            encoder: <_>::default(),
            kind,
            name,
            from,
            _delay: <_>::default(),
            _ease: <_>::default(),
            _interactive: <_>::default(),
            _key: <_>::default(),
            properties: Some(Propsets {
                enter: Some(properties),
                ..Default::default()
            }),
            propset_cache: <_>::default(),
        }
    }

    /// Gets the width of the mark, if it is a group mark.
    pub fn height(&self, node: &Node<'s, '_>) -> Option<f64> {
        if matches!(self.kind, Kind::Group(_)) {
            self.property(node, PropsetKind::Enter, |p| &p.height)
        } else {
            None
        }
    }

    /// Returns true if this is a group mark.
    #[inline]
    pub fn is_group(&self) -> bool {
        matches!(self.kind, Kind::Group(_))
    }

    /// Gets a mark with the given name from a group mark.
    pub fn mark(&self, name: &str) -> Option<&Mark<'s>> {
        if let Kind::Group(group) = &self.kind {
            group.mark(name)
        } else {
            None
        }
    }

    /// Gets a property set from this mark.
    pub fn propset(&self, which: PropsetKind) -> Option<&Propset<'s>> {
        match which {
            PropsetKind::Enter => self
                .propset_cache
                .get_or_init(|| {
                    // Some specs (like vega:example/bar.json) put required
                    // properties on the `update` propset, so what is necessary
                    // for static rendering is actually the combination of
                    // `enter` + `update`
                    self.properties.as_ref().and_then(|property_sets| {
                        let enter = property_sets.enter.as_ref();
                        let update = property_sets.update.as_ref();
                        Some(match (enter, update) {
                            (Some(enter), None) => enter.clone(),
                            (None, Some(update)) => update.clone(),
                            (Some(enter), Some(update)) => enter.merge(update),
                            (None, None) => return None,
                        })
                    })
                })
                .as_ref(),
            PropsetKind::Hover => self
                .properties
                .as_ref()
                .and_then(|property_sets| property_sets.hover.as_ref()),
        }
    }

    /// Gets a scale with the given name from a group mark.
    pub fn scale(&self, name: &str) -> Option<&Scale<'s>> {
        if let Kind::Group(group) = &self.kind {
            group.scale(name)
        } else {
            None
        }
    }

    /// Gets the width of the mark, if it is a group mark.
    pub fn width(&self, node: &Node<'s, '_>) -> Option<f64> {
        if matches!(self.kind, Kind::Group(_)) {
            self.property(node, PropsetKind::Enter, |p| &p.width)
        } else {
            None
        }
    }

    /// Gets a property of this mark from the given property set.
    #[inline]
    fn property<F, T, G>(&self, node: &Node<'s, '_>, which: PropsetKind, f: F) -> Option<T>
    where
        F: for<'a> FnOnce(&'a Propset<'s>) -> &'a G,
        G: Getter<'s, Item = T>,
    {
        self.propset(which).and_then(|propset| f(propset).get(node))
    }
}

/// A trait for functions that update a mark immediately before it is rendered.
pub(super) trait Encoder:
    for<'a, 's> Fn(&'a Propset<'s>, &Node<'s, '_>) -> Cow<'a, Propset<'s>> + private::Sealed
{
}

#[doc(hidden)]
mod private {
    pub trait Sealed {}
    impl<T> Sealed for T where T: super::Encoder {}
}

impl<T> Encoder for T where T: for<'a, 's> Fn(&'a Propset<'s>, &Node<'s, '_>) -> Cow<'a, Propset<'s>>
{}

impl core::fmt::Debug for dyn Encoder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Encoder")
    }
}

/// Data for a mark.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum MarkData<'s> {
    /// A reference to a data source.
    #[serde(borrow)]
    DataRef(DataRef<'s>),
    /// Inline data. This is used by generated axis and legend marks.
    #[serde(skip)]
    Values(Vec<Value<'s>>),
}

impl<'s> MarkData<'s> {
    /// Gets data values from the data source.
    pub(super) fn get<'b>(&'b self, node: &'b Node<'s, '_>) -> Option<Cow<'b, [Value<'s>]>> {
        match self {
            Self::DataRef(data_ref) => data_ref.get(node),
            Self::Values(data) => Some(Cow::Borrowed(data)),
        }
    }
}

impl<'s> From<Vec<Value<'s>>> for MarkData<'s> {
    fn from(value: Vec<Value<'s>>) -> Self {
        Self::Values(value)
    }
}

/// A reference to a data source.
#[derive(Debug, serde::Deserialize)]
pub(super) struct DataRef<'s> {
    /// The name of the data set.
    #[serde(borrow, flatten)]
    target: DataRefTarget<'s>,
    /// A list of transformations to apply to the source data.
    #[serde(borrow, default)]
    transform: Vec<Transform<'s>>,
    /// Cached synthesised data.
    #[serde(skip)]
    data_cache: OnceCell<Vec<Value<'s>>>,
}

impl<'s> DataRef<'s> {
    /// Gets data values from the data source.
    pub(super) fn get<'b>(&'b self, node: &'b Node<'s, '_>) -> Option<Cow<'b, [Value<'s>]>> {
        if let Some(cached) = self.data_cache.get() {
            return Some(Cow::Borrowed(cached));
        }

        Some(match &self.target {
            DataRefTarget::Data(name) => {
                let data = node.data_values(name)?;
                Cow::Borrowed(if self.transform.is_empty() {
                    data
                } else {
                    self.data_cache.get_or_init(|| {
                        let mut data = data.to_vec();
                        for transform in &self.transform {
                            data = transform.transform(node, data);
                        }
                        data
                    })
                })
            }
            DataRefTarget::Mark(name) => {
                // It is not possible to cache this data on the mark because
                // it takes data from `Node`, and this data can change when a
                // mark is used in a group, because each group data is
                // different.
                let (mark, ref node) = node.mark(name)?;
                Cow::Owned(
                    mark.propset(PropsetKind::Enter)
                        .map(|propset| {
                            node.for_each_item()
                                .map(|ref node| {
                                    let mut item = propset.to_value(node);
                                    // TODO: This is gross, it clones a bunch of
                                    // data. Avoid doing this by making everything
                                    // get item data through a function on Node
                                    // instead of directly from node.item so that
                                    // `datum` can be proxied everywhere. (Or
                                    // encapsulate `node.item` in another type I
                                    // guess.)
                                    item.insert("datum", node.item.clone());
                                    item
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                )
            }
        })
    }
}

/// The target of a data source reference.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum DataRefTarget<'s> {
    #[serde(borrow)]
    /// A named source data set.
    Data(Cow<'s, str>),
    #[serde(borrow)]
    /// Undocumented. Use a named mark as the source data. The structure of the
    /// mark data is unspecified, but analysis shows that it consists of the
    /// mark’s visual properties plus a `datum` key pointing to the mark’s
    /// associated data object.
    Mark(Cow<'s, str>),
}

/// An easing function.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
// Clippy: Since this is not actually used right now, it is a waste of time to
// document.
#[allow(clippy::missing_docs_in_private_items)]
pub enum EasingFunction {
    LinearIn,
    LinearOut,
    LinearInOut,
    LinearOutIn,
    QuadIn,
    QuadOut,
    QuadInOut,
    QuadOutIn,
    CubicIn,
    CubicOut,
    CubicInOut,
    CubicOutIn,
    SinIn,
    SinOut,
    SinInOut,
    SinOutIn,
    ExpIn,
    ExpOut,
    ExpInOut,
    ExpOutIn,
    CircleIn,
    CircleOut,
    CircleInOut,
    CircleOutIn,
    BounceIn,
    BounceOut,
    BounceInOut,
    BounceOutIn,
}

/// A line interpolation method.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Interpolate {
    /// Cubic basis spline. The first and last points are triplicated such that
    /// the spline starts at the first point and ends at the last point, and is
    /// tangent to the line between the first and second points, and to the line
    /// between the penultimate and last points.
    Basis,
    /// Closed cubic basis spline. When a line segment ends, the first three
    /// control points are repeated, producing a closed loop with C2 continuity.
    BasisClosed,
    /// Cubic basis spline. The first and last points are not repeated, and thus
    /// the curve typically does not intersect these points.
    BasisOpen,
    /// Straightened cubic basis spline. The spline is straightened according to
    /// the curve’s beta, which defaults to 0.85.
    Bundle,
    /// Cubic cardinal spline, with one-sided differences used for the first and
    /// last piece. The default tension is 0.
    Cardinal,
    /// Closed cubic cardinal spline. When a line segment ends, the first three
    /// control points are repeated, producing a closed loop. The default
    /// tension is 0.
    CardinalClosed,
    /// Cubic cardinal spline. One-sided differences are not used for the first
    /// and last piece, and thus the curve starts at the second point and ends
    /// at the penultimate point. The default tension is 0.
    CardinalOpen,
    /// Linear spline.
    #[default]
    Linear,
    /// Linear spline. The first point is repeated when the line segment ends.
    LinearClosed,
    /// Cubic spline that preserves monotonicity in *y*. Assumes that the input
    /// data is monotonic in *x*.
    Monotone,
    /// Piecewise constant function (a step function) consisting of alternating
    /// horizontal and vertical lines. The y-value changes at the midpoint of
    /// each pair of adjacent x-values.
    Step,
    /// Piecewise constant function (a step function) consisting of alternating
    /// horizontal and vertical lines. The y-value changes after the x-value.
    StepAfter,
    /// Piecewise constant function (a step function) consisting of alternating
    /// horizontal and vertical lines. The y-value changes before the x-value.
    StepBefore,
}

impl From<Value<'_>> for Interpolate {
    fn from(value: Value<'_>) -> Self {
        super::data::value_to_unit_enum(&value)
    }
}

impl From<Interpolate> for Value<'_> {
    fn from(value: Interpolate) -> Self {
        super::data::unit_enum_to_value(value)
    }
}

from_value_impl!(Interpolate);

/// A mark kind.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Kind<'s> {
    /// A plain rectangle.
    Rect,
    /// A symbol drawn at a point.
    Symbol,
    /// An arbitrary SVG path.
    Path,
    /// A circular arc.
    Arc,
    /// A filled area.
    Area,
    /// A line.
    Line,
    /// A horizontal or vertical rule.
    Rule,
    /// A bitmap.
    Image,
    /// A text label.
    Text,
    /// A group of marks.
    #[serde(borrow)]
    Group(Box<Container<'s>>),
}

impl core::fmt::Display for Kind<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Kind::Rect => "rect",
            Kind::Symbol => "symbol",
            Kind::Path => "path",
            Kind::Arc => "arc",
            Kind::Area => "area",
            Kind::Line => "line",
            Kind::Rule => "rule",
            Kind::Image => "image",
            Kind::Text => "text",
            Kind::Group(_) => "group",
        })
    }
}

/// An area mark orientation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Orient {
    /// Draw the mark in a horizontal orientation using the `x`, `x2`, and `y`
    /// properties.
    Horizontal,
    /// Draw the mark in a vertical orientation using the `x`, `y`, and `y2`
    /// properties.
    #[default]
    Vertical,
}

impl From<Value<'_>> for Orient {
    fn from(value: Value<'_>) -> Self {
        super::data::value_to_unit_enum(&value)
    }
}

impl From<Orient> for Value<'_> {
    fn from(value: Orient) -> Self {
        super::data::unit_enum_to_value(value)
    }
}

from_value_impl!(Orient);

/// Mark properties.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Propsets<'s> {
    /// Initial mark values.
    #[serde(borrow, default)]
    enter: Option<Propset<'s>>,
    /// Target values for a mark that is being removed from the scene. Unused.
    #[serde(borrow, default, rename = "exit")]
    _exit: IgnoredAny<Option<Propset<'s>>>,
    /// Target values for a mouse-hovered mark.
    #[serde(borrow, default)]
    hover: Option<Propset<'s>>,
    /// Target values for a non-mouse-hovered mark.
    #[serde(borrow, default)]
    update: Option<Propset<'s>>,
}

/// A symbol mark shape.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Shape {
    /// ○
    #[default]
    Circle,
    /// □
    Square,
    /// +
    Cross,
    /// ◇
    Diamond,
    /// △
    TriangleUp,
    /// ▽
    TriangleDown,
}

impl From<Value<'_>> for Shape {
    fn from(value: Value<'_>) -> Self {
        super::data::value_to_unit_enum(&value)
    }
}

impl From<Shape> for Value<'_> {
    fn from(value: Shape) -> Self {
        super::data::unit_enum_to_value(value)
    }
}

from_value_impl!(Shape);
