//! Types and functions for scale legends.

use super::{
    Node, ScaleNode,
    axis::Format,
    mark::Shape,
    propset::{Align, Baseline, Propset},
};
use serde_json_borrow::Value;
use std::borrow::Cow;

/// A scale legend.
///
/// Legends take one or more scales as their primary input. At least one of the
/// `size`, `shape`, `fill`, `stroke`, or `opacity` properties must be
/// specified. If more than one of these properties are specified and the scales
/// do not share the same input domain, the behaviour is unspecified.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de", rename_all = "camelCase")]
pub(super) struct Legend<'s> {
    /// An optional D3 format specifier for legend labels.
    #[serde(borrow, default)]
    pub format: Cow<'s, str>,
    /// The kind of formatter to use when generating labels.
    #[serde(default, rename = "formatType")]
    pub format_kind: Option<Format>,
    /// The horizontal offset relative to the edge of the enclosing group or
    /// data rectangle, in pixels.
    #[serde(default)]
    pub offset: Option<f64>,
    /// The placement of the legend.
    #[serde(default)]
    pub orient: Orient,
    /// Optional mark property definitions for custom legend styling.
    #[serde(borrow, default)]
    pub properties: Properties<'s>,
    /// The name of the associated scale.
    #[serde(borrow, flatten)]
    scale: Kind<'s>,
    /// The title of the legend.
    #[serde(borrow, default)]
    pub title: Option<Cow<'s, str>>,
    /// Explicit set of visible legend values.
    #[serde(borrow, default)]
    pub values: Vec<Value<'s>>,
}

impl<'s> Legend<'s> {
    /// The default alignment for legend labels.
    pub const LABEL_ALIGN: Align = Align::Left;
    /// The default baseline for legend labels.
    pub const LABEL_BASELINE: Baseline = Baseline::Middle;
    /// The default text colour for legend labels.
    pub const LABEL_COLOR: &'static str = "#000";
    /// The default font for legend labels.
    pub const LABEL_FONT: &'static str = "sans-serif";
    /// The default font size for legend labels.
    pub const LABEL_FONT_SIZE: f64 = 10.0;
    /// The default offset from the edge of the data rectangle.
    pub const OFFSET: f64 = 20.0;
    /// The default padding between legend items.
    pub const PADDING: f64 = 3.0;
    /// The default symbolic legend symbol colour.
    pub const SYMBOL_COLOR: &'static str = "#888";
    /// The default symbolic legend symbol shape.
    pub const SYMBOL_SHAPE: Shape = Shape::Circle;
    /// The default symbolic legend symbol size.
    pub const SYMBOL_SIZE: f64 = 50.0;
    /// The default symbolic legend symbol stroke width.
    pub const SYMBOL_STROKE_WIDTH: f64 = 1.0;

    /// Returns true if the legend values are taken from colour properties.
    pub fn is_color(&self) -> bool {
        matches!(self.scale, Kind::Fill(_) | Kind::Stroke(_))
    }

    /// Returns true if the legend values are taken from size properties.
    pub fn is_size(&self) -> bool {
        matches!(self.scale, Kind::Size(_))
    }

    /// Gets the kind of the legend scale.
    pub fn kind(&self) -> &Kind<'s> {
        &self.scale
    }

    /// Gets the name of the corresponding scale for the legend.
    pub fn scale_name(&self) -> &str {
        match &self.scale {
            Kind::Fill(name)
            | Kind::Opacity(name)
            | Kind::Shape(name)
            | Kind::Size(name)
            | Kind::Stroke(name) => name,
        }
    }

    /// Gets the corresponding scale for the legend.
    pub fn scale<'b>(&self, node: &'b Node<'s, '_>) -> Option<ScaleNode<'s, 'b>> {
        node.scale(self.scale_name())
    }
}

/// The kind of legend.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Kind<'s> {
    /// The name of the scale that determines an item’s fill colour.
    #[serde(borrow)]
    Fill(Cow<'s, str>),
    /// The name of the scale that determines an item’s opacity.
    #[serde(borrow)]
    Opacity(Cow<'s, str>),
    /// The name of the scale that determines an item’s shape.
    #[serde(borrow)]
    Shape(Cow<'s, str>),
    /// The name of the scale that determines an item’s size.
    #[serde(borrow)]
    Size(Cow<'s, str>),
    /// The name of the scale that determines an item’s stroke colour.
    #[serde(borrow)]
    Stroke(Cow<'s, str>),
}

/// Legend placement.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Orient {
    /// Place to the left of the data rectangle.
    Left,
    /// Place to the right of the data rectangle.
    #[default]
    Right,
}

/// Legend properties.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(bound = "'de: 's", deny_unknown_fields)]
pub(super) struct Properties<'s> {
    /// Visual styles for a continuous colour gradient.
    #[serde(default, rename = "gradient")]
    pub _gradient: Option<Propset<'s>>,
    /// Visual styles for discrete legend items.
    #[serde(default)]
    pub labels: Option<Propset<'s>>,
    /// Visual styles for the overall legend group.
    #[serde(default)]
    pub legend: Option<Propset<'s>>,
    /// Visual styles for discrete legend items.
    #[serde(default)]
    pub symbols: Option<Propset<'s>>,
    /// Visual styles for discrete legend items.
    #[serde(default)]
    pub title: Option<Propset<'s>>,
}
