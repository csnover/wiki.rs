//! Types and functions for the root of a visualisation.

use super::{
    axis::Axis,
    data::{Data, IgnoredAny},
    legend::Legend,
    mark::Mark,
    predicate::{Definition, Signal},
    propset::StrokeDashArray,
    scale::Scale,
};
use std::borrow::Cow;

/// The root of a visualisation.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de")]
pub(super) struct Spec<'s> {
    /// Background colour, as a CSS string.
    #[serde(borrow, default)]
    pub background: Option<Cow<'s, str>>,
    /// Data sets.
    #[serde(borrow, default)]
    pub data: Vec<Data<'s>>,
    /// The root group.
    #[serde(borrow, flatten)]
    pub group: Container<'s>,
    /// The height of the graph area, in pixels.
    #[serde(default = "Spec::default_dim")]
    height: f64,
    /// The padding from the edges of the canvas to the graph area.
    #[serde(default)]
    pub padding: Option<Padding>,
    /// A list of named predicate definitions.
    #[serde(borrow, default)]
    pub predicates: Vec<Definition<'s>>,
    /// Registered dynamic variable signals.
    #[serde(borrow, default, rename = "signals")]
    _signals: IgnoredAny<Vec<Signal<'s>>>,
    /// The spec version.
    #[serde(default)]
    pub version: Option<i32>,
    /// The width and height of the viewport, in pixels.
    #[serde(default)]
    pub viewport: Vec<f64>,
    /// The width of the graph area, in pixels.
    #[serde(default = "Spec::default_dim")]
    width: f64,
}

impl<'s> Spec<'s> {
    /// Gets the data set with the given name.
    pub fn data(&self, name: &str) -> Option<&Data<'s>> {
        self.data.iter().find(|data| data.name == name)
    }

    /// Gets the height of the data rectangle, in pixels.
    pub fn height(&self) -> f64 {
        self.height
    }

    /// Gets the mark with the given name from this container.
    pub fn mark(&self, name: &str) -> Option<&Mark<'s>> {
        self.group.mark(name)
    }

    /// Gets the scale with the given name from this container.
    pub fn scale(&self, name: &str) -> Option<&Scale<'s>> {
        self.group.scale(name)
    }

    /// Gets the width of the data rectangle, in pixels.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// The default value for an unspecified data rectangle dimension, in
    /// pixels.
    const fn default_dim() -> f64 {
        500.0
    }
}

/// Properties common to container types (specs and group marks).
///
/// n.b. The JSON schema incorrectly claims that `scene` is a container
/// property, but it is actually only ever used for the visualisation root.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(bound = "'s: 'de")]
pub(super) struct Container<'s> {
    /// Axis definitions.
    #[serde(borrow, default)]
    pub axes: Vec<Axis<'s>>,
    /// Legend definitions.
    #[serde(borrow, default)]
    pub legends: Vec<Legend<'s>>,
    /// Mark definitions.
    #[serde(borrow, default)]
    pub marks: Vec<Mark<'s>>,
    /// Scale definitions.
    #[serde(borrow, default)]
    pub scales: Vec<Scale<'s>>,
    /// Scene definitions.
    #[serde(borrow, default)]
    pub scene: Option<Scene<'s>>,
}

impl<'s> Container<'s> {
    /// Gets the mark with the given name.
    pub fn mark(&self, name: &str) -> Option<&Mark<'s>> {
        self.marks
            .iter()
            .find(|mark| mark.name.as_deref() == Some(name))
    }

    /// Gets the scale with the given name.
    pub fn scale(&self, name: &str) -> Option<&Scale<'s>> {
        self.scales.iter().find(|scale| scale.name() == name)
    }
}

/// Canvas padding options.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Padding {
    /// Compute padding dynamically based on the contents of the visualization.
    /// All marks, including axes and legends, are used to compute the necessary
    /// padding.
    #[default]
    Auto,
    /// Adjust the padding such that the overall width and height of the
    /// visualization is unchanged. This mode can cause the visualization’s
    /// width and height parameters to be adjusted such that the total size,
    /// including padding, remains constant.
    ///
    /// In some cases, strict padding is not possible; for example, if the axis
    /// labels are much larger than the data rectangle.
    Strict,
    /// Uniform padding around all edges of the data rectangle.
    #[serde(untagged)]
    Uniform(f64),
    /// Inset padding for each edge.
    #[serde(untagged)]
    Inset {
        /// Bottom edge.
        #[serde(default)]
        bottom: Option<f64>,
        /// Left edge.
        #[serde(default)]
        left: Option<f64>,
        /// Right edge.
        #[serde(default)]
        right: Option<f64>,
        /// Top edge.
        #[serde(default)]
        top: Option<f64>,
    },
}

/// Heritable default visual styles.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Scene<'s> {
    /// Default fill colour.
    #[serde(borrow, default)]
    pub fill: Option<Cow<'s, str>>,
    /// Default fill opacity.
    #[serde(default)]
    pub fill_opacity: Option<f64>,
    /// Default stroke colour.
    #[serde(borrow, default)]
    pub stroke: Option<Cow<'s, str>>,
    /// Default stroke dash array.
    #[serde(default)]
    pub stroke_dash: Option<StrokeDashArray>,
    /// Default stroke dash offset.
    #[serde(default)]
    pub stroke_dash_offset: Option<f64>,
    /// Default stroke opacity.
    #[serde(default)]
    pub stroke_opacity: Option<f64>,
    /// Default stroke width.
    #[serde(default)]
    pub stroke_width: Option<f64>,
}
