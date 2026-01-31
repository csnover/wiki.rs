//! Types and functions for visualisation axes.

use super::{
    Node,
    data::ValueExt as _,
    data::{scalar, vec_scalar},
    propset::Propset,
};
use serde_json_borrow::Value;
use std::borrow::Cow;

/// An axis definition.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Axis<'s> {
    /// A D3 format specifier for axis labels.
    #[serde(borrow, default)]
    pub format: Cow<'s, str>,
    /// The kind of formatter to use when generating labels.
    #[serde(default, rename = "formatType")]
    pub format_kind: Option<Format>,
    /// If true, draw grid lines.
    #[serde(default)]
    pub grid: bool,
    /// The axis z-index.
    #[serde(default)]
    pub layer: Layer,
    /// The offset of the axis line away from the edge of the data rectangle, in
    /// pixels. Positive values move away from the
    /// [placement edge](Self::orient).
    #[serde(borrow, default)]
    pub offset: Offset<'s>,
    /// The axis’s edge placement on the data rectangle.
    #[serde(default)]
    pub orient: Option<Placement>,
    /// Visual properties.
    #[serde(borrow, default)]
    pub properties: Properties<'s>,
    /// The name of the scale used by the axis.
    #[serde(borrow)]
    pub scale: Cow<'s, str>,
    /// The number of minor ticks between each major tick.
    #[serde(default)]
    pub subdivide: Option<f64>,
    /// The padding between ticks and text labels, in pixels.
    #[serde(default)]
    pub tick_padding: Option<f64>,
    /// The length of a tick, in pixels.
    #[serde(default)]
    pub tick_size: Option<f64>,
    /// The length of an end tick, in pixels. Overrides `tick_size`,
    /// `tick_size_major`, and `tick_size_minor`.
    #[serde(default)]
    pub tick_size_end: Option<f64>,
    /// The length of a major tick, in pixels. Overrides `tick_size`.
    #[serde(default)]
    pub tick_size_major: Option<f64>,
    /// The length of a minor tick, in pixels. Overrides `tick_size`.
    #[serde(default)]
    pub tick_size_minor: Option<f64>,
    /// The number of major ticks.
    #[serde(default)]
    pub ticks: Option<f64>,
    /// The title of the axis.
    #[serde(borrow, default)]
    pub title: Cow<'s, str>,
    /// The offset of the title origin away from the axis line, in pixels.
    #[serde(default)]
    pub title_offset: Option<f64>,
    /// The cartesian axis.
    #[serde(rename = "type")]
    pub kind: Kind,
    /// Explicit set of visible axis tick values. If unset, the placement of
    /// ticks will be determined automatically.
    #[serde(borrow, default, deserialize_with = "vec_scalar")]
    pub values: Vec<Value<'s>>,
}

impl Axis<'_> {
    /// The default for [`Self::ticks`].
    pub const DEFAULT_TICKS: f64 = 10.0;
    /// The default for [`Self::tick_padding`].
    pub const DEFAULT_PADDING: f64 = 3.0;
    /// The default for [`Self::properties`]`::`[`axis`](Properties::axis)`::`[`stroke`](Propset::stroke).
    pub const AXIS_COLOR: &'static str = "#000";
    /// The default for [`Self::properties`]`::`[`axis`](Properties::axis)`::`[`stroke_width`](Propset::stroke_width).
    pub const AXIS_WIDTH: f64 = 1.0;
    /// The default for [`Self::properties`]`::`[`grid`](Properties::grid)`::`[`stroke`](Propset::stroke).
    pub const GRID_COLOR: &'static str = "#000";
    /// The default for [`Self::properties`]`::`[`grid`](Properties::grid)`::`[`stroke_opacity`](Propset::stroke_opacity).
    pub const GRID_OPACITY: f64 = 0.15;
    /// The default for [`Self::properties`]`::`[`ticks`](Properties::ticks)`::`[`stroke`](Propset::stroke).
    pub const TICK_COLOR: &'static str = "#000";
    /// The default for [`Self::properties`]`::`[`labels`](Properties::labels)`::`[`fill`](Propset::fill).
    pub const TICK_LABEL_COLOR: &'static str = "#000";
    /// The default for [`Self::properties`]`::`[`ticks`](Properties::ticks)`::`[`stroke_width`](Propset::stroke_width).
    pub const TICK_WIDTH: f64 = 1.0;
    /// The default for [`Self::properties`]`::`[`ticks`](Properties::ticks)`::`[`size`](Propset::size).
    pub const TICK_SIZE: f64 = 6.0;
    /// The default for [`Self::properties`]`::`[`labels`](Properties::labels)`::`[`font_size`](Propset::font_size).
    pub const TICK_LABEL_FONT_SIZE: f64 = 11.0;
    /// The default for [`Self::properties`]`::`[`labels`](Properties::labels)`::`[`font`](Propset::font).
    pub const TICK_LABEL_FONT: &'static str = "sans-serif";
    /// The minimum automatic title offset.
    pub const TITLE_OFFSET_AUTO_MIN: f64 = 30.0;
    /// The maximum automatic title offset.
    pub const TITLE_OFFSET_AUTO_MAX: f64 = 10000.0;
    /// The amount of padding between the title and axis when auto-calculating
    /// the offset.
    pub const TITLE_OFFSET_AUTO_MARGIN: f64 = 4.0;
}

/// A cartesian axis data format.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Format {
    /// Local time.
    Time,
    /// UTC time.
    Utc,
    /// A string.
    String,
    /// A number.
    Number,
}

/// A cartesian axis kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    /// X-axis.
    X,
    /// Y-axis.
    Y,
}

/// An axis z-index.
///
/// The default value for this type is confusing because the hard-coded fallback
/// value is `Front`, but Vega comes with a configuration file which defaults it
/// to `Back`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Layer {
    /// Draw axis above marks.
    Front,
    /// Draw axis below marks.
    #[default]
    Back,
}

/// An axis offset, relative to the edge of the enclosing group or data
/// rectangle, in pixels.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Offset<'s> {
    /// Fixed offset.
    Fixed(f64),
    /// Scaled offset.
    Scaled {
        /// The scale name.
        #[serde(borrow)]
        scale: Cow<'s, str>,
        /// The value to pass to the scale.
        #[serde(borrow, deserialize_with = "scalar")]
        value: Value<'s>,
    },
}

impl Default for Offset<'_> {
    fn default() -> Self {
        Self::Fixed(0.0)
    }
}

impl<'s> Offset<'s> {
    /// Gets the scaled offset.
    pub fn get(&self, node: &Node<'s, '_>) -> f64 {
        match self {
            Offset::Fixed(value) => *value,
            // In `axisUpdate` in Vega, only values that come from a scale get
            // negated. (TODO: It also will just crash if the scale name is
            // invalid instead of returning the input as the output, which means
            // probably this should return an error.)
            Offset::Scaled { scale, value } => -node
                .scale(scale)
                .map_or(value.to_f64(), |scale| scale.apply(value, false).to_f64()),
        }
    }
}

/// An axis placement on the edge of the data rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Placement {
    /// Top edge of the data rectangle.
    Top,
    /// Bottom edge of the data rectangle.
    Bottom,
    /// Left edge of the data rectangle.
    Left,
    /// Right edge of the data rectangle.
    Right,
}

impl Placement {
    /// Returns true if this placement is nearest to the SVG view box origin
    /// (top left).
    pub fn is_origin(self) -> bool {
        matches!(self, Self::Top | Self::Left)
    }

    /// Returns true if this placement is on a vertical axis (top or bottom).
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

/// Axis component properties.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Properties<'s> {
    /// Axis line properties.
    #[serde(borrow, default)]
    pub axis: Option<Propset<'s>>,
    /// Grid line properties.
    #[serde(borrow, default)]
    pub grid: Option<Propset<'s>>,
    /// Label properties.
    #[serde(borrow, default)]
    pub labels: Option<Propset<'s>>,
    /// Major tick mark properties.
    #[serde(borrow, default)]
    pub major_ticks: Option<Propset<'s>>,
    /// Minor tick mark properties.
    #[serde(borrow, default)]
    pub minor_ticks: Option<Propset<'s>>,
    /// Common (both major and minor) tick mark properties.
    #[serde(borrow, default)]
    pub ticks: Option<Propset<'s>>,
    /// Axis title properties.
    #[serde(borrow, default)]
    pub title: Option<Propset<'s>>,
}
