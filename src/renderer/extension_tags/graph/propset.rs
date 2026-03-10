//! Types for visual properties.

use super::{
    Node, ScaleNode,
    data::{ValueExt, get_nested_value},
    expr::Ast,
    mark::{Interpolate, Orient, Shape},
    predicate::Call as PredicateCall,
};
use serde_json_borrow::Value;
use std::borrow::Cow;

/// A trait for arbitrary value retrieval.
pub(super) trait Getter<'s> {
    /// The type of the item.
    type Item;

    /// Gets the item from the given node.
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item>;
}

impl<'s, T> Getter<'s> for Option<T>
where
    T: Getter<'s>,
{
    type Item = T::Item;

    #[inline]
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        self.as_ref().and_then(|item| item.get(node))
    }
}

impl<'s, T> Getter<'s> for &T
where
    T: Getter<'s>,
{
    type Item = T::Item;

    #[inline]
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        (*self).get(node)
    }
}

/// A number property.
pub(super) type NumberProperty<'s> = Rule<'s, ValueRefNumber<'s>>;

impl From<f64> for NumberProperty<'_> {
    fn from(value: f64) -> Self {
        Self::from(ValueRefNumber::from(value))
    }
}

/// A generic property.
pub(super) type Property<'s, T> = Rule<'s, ScaledValueRef<'s, ValueRefData<'s, T>>>;

impl<'s, T> Property<'s, T> {
    /// Creates a new `Property` with the given `field` and `scale`.
    pub fn with_field_and_scale(
        field: impl Into<FieldRef<'s>>,
        scale: Option<Cow<'s, str>>,
    ) -> Self {
        ScaledValueRef {
            scale: scale.map(Into::into),
            value: ValueRefData::with_field(field),
        }
        .into()
    }
}

impl<'s, T, IntoT: Into<ValueRefData<'s, T>>> From<IntoT> for Property<'s, T> {
    fn from(value: IntoT) -> Self {
        Self::from(ScaledValueRef::from(value.into()))
    }
}

/// A string property.
pub(super) type StringProperty<'s> = Rule<'s, ScaledValueRef<'s, ValueRefTemplateData<'s>>>;

impl<'s, T: Into<ValueRefTemplateData<'s>>> From<T> for StringProperty<'s> {
    fn from(value: T) -> Self {
        Self::from(ScaledValueRef::from(value.into()))
    }
}

bag_o_crap! {
    /// A big bag of visual properties.
    ///
    /// Because these properties may be inherited, and because programmers are lazy,
    /// all possible properties are defined, even if they would not apply to a
    /// particular object.
    #[derive(Clone, Debug, Default, serde::Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    pub(super) struct Propset<'s> {
        /// For text labels. Horizontal alignment.
        #[serde(borrow, default)]
        pub align: Option<Property<'s, Align>>,
        /// For text labels. Rotation, in degrees.
        #[serde(borrow, default)]
        pub angle: Option<NumberProperty<'s>>,
        /// For text labels. Vertical alignment.
        #[serde(borrow, default)]
        pub baseline: Option<Property<'s, Baseline>>,
        /// For group marks. If true, the group is clipped to its
        /// [`width`](Self::width) and [`height`](Self::height). If these properties
        /// are undefined, they default to 0, so the whole group will be clipped.
        #[serde(borrow, default)]
        pub clip: Option<Property<'s, bool>>,
        /// For hover effects. CSS mouse cursor style. Only applicable in the
        /// [hover set](super::mark::MarkProperties::hover).
        #[serde(borrow, default)]
        pub cursor: Option<StringProperty<'s>>,
        /// For text labels. Horizontal offset relative to its origin, in pixels.
        /// Applied after [`angle`](Self::angle).
        #[serde(borrow, default)]
        pub dx: Option<NumberProperty<'s>>,
        /// For text labels. Vertical offset relative to its origin, in pixels.
        /// Applied after [`angle`](Self::angle).
        #[serde(borrow, default)]
        pub dy: Option<NumberProperty<'s>>,
        /// For arcs. The end angle, in radians. 0 is “north”.
        #[serde(borrow, default)]
        pub end_angle: Option<NumberProperty<'s>>,
        /// For shapes. The fill colour.
        #[serde(borrow, default)]
        pub fill: Option<ColorProperty<'s>>,
        /// For shapes. The fill opacity.
        #[serde(borrow, default)]
        pub fill_opacity: Option<NumberProperty<'s>>,
        /// For text labels. The font family.
        #[serde(borrow, default)]
        pub font: Option<StringProperty<'s>>,
        /// For text labels. The font size.
        #[serde(borrow, default)]
        pub font_size: Option<NumberProperty<'s>>,
        /// For text labels. The font style.
        #[serde(borrow, default)]
        pub font_style: Option<StringProperty<'s>>,
        /// For text labels. The font weight.
        #[serde(borrow, default)]
        pub font_weight: Option<StringProperty<'s>>,
        /// For marks. The height of the mark, in pixels.
        #[serde(borrow, default)]
        pub height: Option<NumberProperty<'s>>,
        /// For arcs. The inner radius, in pixels.
        #[serde(borrow, default)]
        pub inner_radius: Option<NumberProperty<'s>>,
        /// For lines. The interpolation method.
        #[serde(borrow, default)]
        pub interpolate: Option<Property<'s, Interpolate>>,
        /// The opacity.
        #[serde(borrow, default)]
        pub opacity: Option<NumberProperty<'s>>,
        /// For area marks. The orientation.
        #[serde(borrow, default)]
        pub orient: Option<Property<'s, Orient>>,
        /// For arcs. The outer radius, in pixels.
        #[serde(borrow, default)]
        pub outer_radius: Option<NumberProperty<'s>>,
        /// For paths. An SVG path definition.
        #[serde(borrow, default)]
        pub path: Option<StringProperty<'s>>,
        /// For text labels. The polar coordinate radial offset, in pixels, from the
        /// origin given in [`x`](Self::x) and [`y`](Self::y).
        #[serde(borrow, default)]
        pub radius: Option<NumberProperty<'s>>,
        /// For symbols. The shape.
        #[serde(borrow, default)]
        pub shape: Option<Property<'s, Shape>>,
        /// For symbols. The size, in pixels.
        #[serde(borrow, default)]
        pub size: Option<NumberProperty<'s>>,
        /// For arcs. The start angle, in radians. 0 is “north”.
        #[serde(borrow, default)]
        pub start_angle: Option<NumberProperty<'s>>,
        /// For shapes. The stroke colour.
        #[serde(borrow, default)]
        pub stroke: Option<ColorProperty<'s>>,
        /// For stroked shapes. A list of alternating (stroke, space) lengths.
        #[serde(borrow, default)]
        pub stroke_dash: Option<Property<'s, StrokeDashArray>>,
        /// For stroked shapes. The initial offset of the stroke dash.
        #[serde(borrow, default)]
        pub stroke_dash_offset: Option<NumberProperty<'s>>,
        /// For shapes. The stroke opacity.
        #[serde(borrow, default)]
        pub stroke_opacity: Option<NumberProperty<'s>>,
        /// For shapes. The stroke width.
        #[serde(borrow, default)]
        pub stroke_width: Option<NumberProperty<'s>>,
        #[serde(borrow, default)]
        /// For curved paths. The “tightness” of links, in the range 0..=1.
        pub tension: Option<NumberProperty<'s>>,
        /// For text labels. The text content.
        #[serde(borrow, default)]
        pub text: Option<StringProperty<'s>>,
        /// For text labels. The polar coordinate angle, in radians, from the origin
        /// given in [`x`](Self::x) and [`y`](Self::y). 0 is “north”.
        #[serde(borrow, default)]
        pub theta: Option<NumberProperty<'s>>,
        /// For images. The URL of the image.
        #[serde(borrow, default)]
        pub url: Option<StringProperty<'s>>,
        /// For marks. The width of the mark.
        #[serde(borrow, default)]
        pub width: Option<NumberProperty<'s>>,
        /// For marks. The first (typically left-most) horizontal coordinate.
        /// Ignored if [`xc`](Self::xc) is set.
        #[serde(borrow, default)]
        pub x: Option<NumberProperty<'s>>,
        /// For marks. The second (typically right-most) horizontal coordinate.
        /// Ignored if [`xc`](Self::xc) is set.
        #[serde(borrow, default)]
        pub x2: Option<NumberProperty<'s>>,
        /// For marks. A centred horizontal coordinate. Overrides [`x`](Self::x) and
        /// [`x2`](Self::x2).
        #[serde(borrow, default)]
        pub xc: Option<NumberProperty<'s>>,
        /// For marks. The first (typically top-most) vertical coordinate.
        /// Ignored if [`yc`](Self::yc) is set.
        #[serde(borrow, default)]
        pub y: Option<NumberProperty<'s>>,
        /// For marks. The second (typically bottom-most) vertical coordinate.
        /// Ignored if [`yc`](Self::yc) is set.
        #[serde(borrow, default)]
        pub y2: Option<NumberProperty<'s>>,
        /// For marks. A centred vertical coordinate. Overrides [`y`](Self::y) and
        /// [`y2`](Self::y2).
        #[serde(borrow, default)]
        pub yc: Option<NumberProperty<'s>>,
    }
}

impl<'s> Propset<'s> {
    /// Gets the output x-coordinate and width of a shape which may be defined
    /// by a combination of `x`, `x2`, `xc`, and `width`.
    pub fn x(&self, node: &Node<'s, '_>) -> (f64, f64) {
        Self::dim(
            self.x.get(node),
            self.x2.get(node),
            self.xc.get(node),
            self.width.get(node),
        )
    }

    /// Gets the output y-coordinate and height of a shape which may be defined
    /// by a combination of `y`, `y2`, `yc`, and `height`.
    pub fn y(&self, node: &Node<'s, '_>) -> (f64, f64) {
        Self::dim(
            self.y.get(node),
            self.y2.get(node),
            self.yc.get(node),
            self.height.get(node),
        )
    }

    /// Calculates a point and length from the given inputs.
    fn dim(min: Option<f64>, max: Option<f64>, mid: Option<f64>, len: Option<f64>) -> (f64, f64) {
        // This algorithm is unhinged because in Vega it is a code generator
        // generating conditional code and not all branches are actually
        // considered in that code
        match (min, max, mid, len) {
            (None, None, None, None) => (0.0, 0.0),
            (None, None, None, Some(len)) => (0.0, len),
            (_, None, Some(mid), Some(len)) | (None, Some(_), Some(mid), Some(len)) => {
                (mid - len / 2.0, len)
            }
            (_, None, Some(p), None)
            | (None, Some(_), Some(p), None)
            | (None, Some(p), None, None)
            | (Some(p), None, None, None) => (p, 0.0),
            (None, Some(max), None, Some(len)) => (max - len, len),
            (Some(min), None, None, Some(len)) => (min, len),
            (Some(min), Some(max), None, _) => (min.min(max), (max - min).abs()),
            (Some(min), Some(max), Some(mid), _) => {
                let len = (max - min).abs();
                (mid - len / 2.0, len)
            }
        }
    }
}

/// Generates functions on the struct that require the list of struct fields.
macro_rules! bag_o_crap {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident <$gen_lt:lifetime> {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field_name:ident : $field_type:ty
            ),*
            $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $name <$gen_lt> {
            $(
                $(#[$field_meta])*
                $field_vis $field_name: $field_type
            ),*
        }

        impl <$gen_lt> $name <$gen_lt> {
            /// Gets a property by string.
            pub fn get(&self, name: &str, node: &Node<$gen_lt, '_>) -> Option<Value<$gen_lt>> {
                match name {
                    $(
                        stringify!($field_name) => self.$field_name.as_ref()
                            .and_then(|value| value.get(node))
                            .map(Value::from),
                    )*
                    _ => None
                }
            }

            /// Creates a new property set by overwriting any properties of
            /// `self` with non-`None` properties of `other`.
            pub fn merge(&self, other: &Self) -> Self {
                let mut merged = self.clone();
                $(
                    if other.$field_name.is_some() {
                        merged.$field_name = other.$field_name.clone();
                    }
                )*
                merged
            }

            /// Creates a [`Value::Object`] containing the defined properties
            /// from this property set.
            pub fn to_value(&self, node: &Node<$gen_lt, '_>) -> Value<$gen_lt> {
                const SNOWFLAKES: &[&str] = &[
                    "x", "x2", "xc", "width",
                    "y", "y2", "yc", "height"
                ];

                let mut fields = vec![];

                let needs_x = self.x.is_some() || self.x2.is_some() || self.xc.is_some() || self.width.is_some();
                let needs_y = self.y.is_some() || self.y2.is_some() || self.yc.is_some() || self.height.is_some();

                let (x, width) = if needs_x {
                    self.x(node)
                } else {
                    <_>::default()
                };

                let (y, height) = if needs_y {
                    self.y(node)
                } else {
                    <_>::default()
                };

                $(
                    if !SNOWFLAKES.contains(&stringify!($field_name)) && let Some(value) = self.$field_name.as_ref().and_then(|value| value.get(node)) {
                        fields.push((stringify!($field_name), Value::from(value)));
                    }
                )*

                if needs_x {
                    fields.push(("x", Value::from(x)));
                    fields.push(("x2", Value::from(x + width)));
                    fields.push(("xc", Value::from(x + width / 2.0)));
                    fields.push(("width", Value::from(width)));
                }

                if needs_y {
                    fields.push(("y", Value::from(y)));
                    fields.push(("y2", Value::from(y + height)));
                    fields.push(("yc", Value::from(y + height / 2.0)));
                    fields.push(("height", Value::from(height)));
                }

                Value::from(fields)
            }
        }
    }
}

use bag_o_crap;

/// Text horizontal alignment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Align {
    /// Left alignment.
    #[default]
    Left,
    /// Center alignment.
    Center,
    /// Right alignment.
    Right,
}

impl core::fmt::Display for Align {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Align::Left => "start",
            Align::Center => "middle",
            Align::Right => "end",
        })
    }
}

impl From<Value<'_>> for Align {
    fn from(value: Value<'_>) -> Self {
        super::data::value_to_unit_enum(&value)
    }
}

impl From<Align> for Value<'_> {
    fn from(value: Align) -> Self {
        super::data::unit_enum_to_value(value)
    }
}

from_value_impl!(Align);

/// Text vertical alignment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Baseline {
    /// Place the top of the text on the anchor point.
    Top,
    /// Place the middle of the x-height on the anchor point.
    Middle,
    /// Place the bottom of the text on the anchor point.
    Bottom,
    /// Place the alphabetic baseline on the anchor point.
    #[default]
    Alphabetic,
}

impl Baseline {
    /// The approximate ascent of a font.
    const ASCENT: f64 = 0.79;

    /// The approximate font size multiplier for the proportion of the text
    /// which will sit above the anchor point.
    #[inline]
    pub fn above(self) -> f64 {
        Self::ASCENT - self.adjustment()
    }

    /// The approximate font size multiplier for adjusting text to the anchor
    /// point.
    ///
    /// These hard-coded values are from Vega. Nominally they should be the
    /// ascent, x-height / 2, and descent of whatever font is being used.
    #[inline]
    pub fn adjustment(self) -> f64 {
        match self {
            Baseline::Top => Self::ASCENT,
            Baseline::Middle => Self::ASCENT - 0.49,
            Baseline::Bottom => Self::ASCENT - 1.0,
            Baseline::Alphabetic => 0.0,
        }
    }
}

impl core::fmt::Display for Baseline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Baseline::Top => "text-top",
            Baseline::Middle => "middle",
            Baseline::Bottom => "text-bottom",
            Baseline::Alphabetic => "auto",
        })
    }
}

impl From<Value<'_>> for Baseline {
    fn from(value: Value<'_>) -> Self {
        super::data::value_to_unit_enum(&value)
    }
}

impl From<Baseline> for Value<'_> {
    fn from(value: Baseline) -> Self {
        super::data::unit_enum_to_value(value)
    }
}

from_value_impl!(Baseline);

/// A colour.
#[derive(Clone, Debug)]
pub(super) struct Color(csscolorparser::Color);

impl Color {
    /// Creates a colour from rgba values.
    #[inline]
    fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self(csscolorparser::Color::new(r, g, b, a))
    }

    /// Creates a colour from hsla values.
    #[inline]
    fn from_hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        Self(csscolorparser::Color::from_hsla(h, s, l, a))
    }

    /// Creates a colour from a CSS string.
    #[inline]
    fn from_html<S: AsRef<str>>(s: S) -> Self {
        Self(csscolorparser::Color::from_html(s).expect("valid css color"))
    }

    /// Creates a colour from laba values.
    #[inline]
    fn from_laba(l: f32, a: f32, b: f32, alpha: f32) -> Self {
        Self(csscolorparser::Color::from_laba(l, a, b, alpha))
    }

    /// Creates a colour from lcha values.
    #[inline]
    fn from_lcha(l: f32, c: f32, h: f32, alpha: f32) -> Self {
        Self(csscolorparser::Color::from_lcha(l, c, h, alpha))
    }
}

impl From<Color> for Value<'_> {
    fn from(value: Color) -> Self {
        Value::Str(value.0.to_css_hex().into())
    }
}

impl core::fmt::Display for Color {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0.to_css_hex())
    }
}

/// A colour property.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum ColorProperty<'s> {
    /// A string value or data reference.
    #[serde(borrow)]
    Named(StringProperty<'s>),
    /// An RGB value.
    Rgb {
        /// Red channel, `0..=255`.
        #[serde(borrow)]
        r: NumberProperty<'s>,
        /// Green channel, `0..=255`.
        #[serde(borrow)]
        g: NumberProperty<'s>,
        /// Blue channel, `0..=255`.
        #[serde(borrow)]
        b: NumberProperty<'s>,
    },
    /// An HSL value.
    Hsl {
        /// Hue, `0..=360`.
        #[serde(borrow)]
        h: NumberProperty<'s>,
        /// Saturation, `0..=1`.
        #[serde(borrow)]
        s: NumberProperty<'s>,
        /// Lightness, `0..=1`.
        #[serde(borrow)]
        l: NumberProperty<'s>,
    },
    /// A CIE LAB value.
    Lab {
        /// Luminance, `0..=100`.
        #[serde(borrow)]
        l: NumberProperty<'s>,
        /// Green-red, `-160..=160`.
        #[serde(borrow)]
        a: NumberProperty<'s>,
        /// Blue-yellow, `-160..=160`.
        #[serde(borrow)]
        b: NumberProperty<'s>,
    },
    /// An LCH value.
    Lch {
        /// Lightness, `0..=100`.
        #[serde(borrow)]
        l: NumberProperty<'s>,
        /// Chroma, `0..=230`.
        #[serde(borrow)]
        c: NumberProperty<'s>,
        /// Hue, `0..=360`.
        #[serde(borrow)]
        h: NumberProperty<'s>,
    },
}

impl<'s> ColorProperty<'s> {
    /// Creates a new `ColorProperty` with the given `field` and `scale`.
    pub fn with_field_and_scale(
        field: impl Into<FieldRef<'s>>,
        scale: Option<Cow<'s, str>>,
    ) -> Self {
        Self::Named(StringProperty::from(ScaledValueRef {
            scale: scale.map(Into::into),
            value: ValueRefTemplateData::ValueRef(ValueRefData::with_field(field.into())),
        }))
    }
}

impl<'s, T: Into<StringProperty<'s>>> From<T> for ColorProperty<'s> {
    fn from(value: T) -> Self {
        Self::Named(value.into())
    }
}

impl<'s> Getter<'s> for ColorProperty<'s> {
    type Item = Color;

    fn get(&self, node: &Node<'s, '_>) -> Option<Color> {
        match self {
            ColorProperty::Named(named) => named.get(node).map(Color::from_html),
            ColorProperty::Rgb { r, g, b } => {
                let r = r.get(node).unwrap_or(128.0);
                let g = g.get(node).unwrap_or(128.0);
                let b = b.get(node).unwrap_or(128.0);
                #[expect(clippy::cast_possible_truncation, reason = "loss does not matter")]
                let color = Color::new(r as f32, g as f32, b as f32, 1.0);
                Some(color)
            }
            ColorProperty::Hsl { h, s, l } => {
                let h = h.get(node).unwrap_or(0.0);
                let s = s.get(node).unwrap_or(0.0);
                let l = l.get(node).unwrap_or(0.5);
                #[expect(clippy::cast_possible_truncation, reason = "loss does not matter")]
                let color = Color::from_hsla(h as f32, s as f32, l as f32, 1.0);
                Some(color)
            }
            ColorProperty::Lab { l, a, b } => {
                let l = l.get(node).unwrap_or(50.0);
                let a = a.get(node).unwrap_or(0.0);
                let b = b.get(node).unwrap_or(0.0);
                #[expect(clippy::cast_possible_truncation, reason = "loss does not matter")]
                let color = Color::from_laba(l as f32, a as f32, b as f32, 1.0);
                Some(color)
            }
            ColorProperty::Lch { l, c, h } => {
                let l = l.get(node).unwrap_or(50.0);
                let c = c.get(node).unwrap_or(0.0);
                let h = h.get(node).unwrap_or(0.0);
                #[expect(clippy::cast_possible_truncation, reason = "loss does not matter")]
                let color = Color::from_lcha(l as f32, c as f32, h as f32, 1.0);
                Some(color)
            }
        }
    }
}

/// A reference to a data value.
///
/// The corresponding data set is defined by
/// [`Mark::from`](super::mark::Mark::from).
///
/// These properties can be arbitrarily nested in order to perform indirect
/// field lookups. For example, `"field": {"parent": {"datum": "f"}}` will first
/// retrieve the value of the `f` field on the current mark’s data object. This
/// value will then be used as the property name to lookup on the enclosing
/// group mark’s data object.
///
/// Dot notation ("price.min") is used to access nested properties. If a dot
/// character is actually part of the property name, it must be escaped with
/// a backslash: "some\.field".
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum FieldRef<'s> {
    /// Data key. Synonymous with `{ "datum": value }`.
    #[serde(borrow)]
    Field(Cow<'s, str>),
    /// Look up a value from the mark’s own data object.
    Datum {
        /// Data key.
        #[serde(borrow)]
        datum: Box<FieldRef<'s>>,
    },
    /// Look up a value from the mark’s parent group Vega scene graph object.
    Group {
        /// Mark property.
        #[serde(borrow)]
        group: Box<FieldRef<'s>>,
        /// Number of levels to ascend. Defaults to 1.
        level: Option<f64>,
    },
    /// Look up a value from the mark’s parent group’s data object.
    Parent {
        /// Data key.
        #[serde(borrow)]
        parent: Box<FieldRef<'s>>,
        /// Number of levels to ascend. Defaults to 1.
        level: Option<f64>,
    },
}

impl<'s> FieldRef<'s> {
    /// Returns the computed key for a field reference.
    fn key(&self, node: &Node<'s, '_>) -> Option<Cow<'_, str>> {
        match self {
            FieldRef::Field(name) => Some(name.clone()),
            _ => self.get(node).map(ValueExt::into_string),
        }
    }
}

impl<'s, T: Into<Cow<'s, str>>> From<T> for FieldRef<'s> {
    fn from(value: T) -> Self {
        Self::Field(value.into())
    }
}

impl<'s> Getter<'s> for FieldRef<'s> {
    type Item = Value<'s>;

    /// Recursively resolves field references until a value is found. Returns
    /// `None` if there is no corresponding value for this field lookup.
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        match self {
            FieldRef::Field(name) => get_nested_value(node.item, name).cloned(),
            FieldRef::Datum { datum } => {
                let key = datum.key(node)?;
                get_nested_value(node.item, &key).cloned()
            }
            FieldRef::Group { group, level } => {
                let parent_node = ascend(node, *level)?;
                let key = group.key(node)?;
                parent_node.visual(&key)
            }
            FieldRef::Parent { parent, level } => {
                let parent_node = ascend(node, *level)?;
                let key = parent.key(node)?;
                get_nested_value(parent_node.item, &key).cloned()
            }
        }
    }
}

/// A trait for types which can be constructed from a [`Value`].
///
/// This is regrettably required because there is no `From<Value<'_>> for f64`
/// upstream, and probably there should not be.
pub(super) trait FromValue<'a> {
    /// Converts a [`Value`] into `Self`.
    fn from_value(value: Value<'a>) -> Self;
}

impl FromValue<'_> for bool {
    #[inline]
    fn from_value(value: Value<'_>) -> bool {
        ValueExt::into_bool(value)
    }
}

impl<'a> FromValue<'a> for Cow<'a, str> {
    #[inline]
    fn from_value(value: Value<'a>) -> Cow<'a, str> {
        ValueExt::into_string(value)
    }
}

impl FromValue<'_> for f64 {
    #[inline]
    fn from_value(value: Value<'_>) -> f64 {
        ValueExt::into_f64(value)
    }
}

/// Implements `FromValue` for some types which implement `From<Value>`.
/// This cannot be a blanket implementation due to the reason that we can never
/// have nice things <https://github.com/rust-lang/rust/issues/31844>.
macro_rules! from_value_impl {
    ($($ty:ty),* $(,)?) => {
        $(impl $crate::renderer::extension_tags::graph::propset::FromValue<'_> for $ty {
            #[inline]
            fn from_value(value: Value<'_>) -> $ty {
                <$ty as From<Value<'_>>>::from(value)
            }
        })*
    }
}

pub(super) use from_value_impl;

/// A kind of property set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    /// Normal properties.
    Enter,
    /// Mouse cursor hover properties.
    Hover,
}

/// A list of production rules or a value.
///
/// Visual properties can be set by evaluating an if-then-else style chain of
/// production rules. The visual property is set to the first item whose
/// predicate evaluates to true.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum Rule<'s, T> {
    /// A value.
    Value(T),
    /// A sequence of production rules.
    #[serde(borrow, alias = "rule")]
    Rule(Vec<RuleItem<'s, T>>),
}

impl<T> From<T> for Rule<'_, T> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}

impl<'s, T> Getter<'s> for Rule<'s, T>
where
    T: Getter<'s>,
{
    type Item = T::Item;

    #[inline]
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        match self {
            Rule::Value(value) => value.get(node),
            Rule::Rule(rules) => rules.iter().find_map(|rule| rule.get(node)),
        }
    }
}

/// A production rule.
#[derive(Clone, Debug, serde::Deserialize)]
pub(super) struct RuleItem<'s, T> {
    /// A production rule test.
    #[serde(borrow, default, flatten)]
    test: Option<RulePredicate<'s>>,
    /// The value.
    #[serde(flatten)]
    value: T,
}

impl<'s, T> Getter<'s> for RuleItem<'s, T>
where
    T: Getter<'s>,
{
    type Item = T::Item;

    #[inline]
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        if self.test.as_ref().is_none_or(|test| test.eval(node)) {
            self.value.get(node)
        } else {
            None
        }
    }
}

/// A production rule test expression.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RulePredicate<'s> {
    /// A predicate call.
    #[serde(borrow)]
    Predicate(PredicateCall<'s>),
    /// A test expression string.
    #[serde(borrow)]
    Test(Cow<'s, str>),
}

impl<'s> RulePredicate<'s> {
    /// Evaluates the test expression.
    #[inline]
    fn eval(&self, node: &Node<'s, '_>) -> bool {
        match self {
            RulePredicate::Predicate(predicate) => predicate.eval(node),
            RulePredicate::Test(test) => {
                // TODO: Self-referential struct to cache the AST.
                match Ast::new(test).and_then(|test| test.eval(node)) {
                    Ok(result) => result.into_bool(),
                    Err(err) => {
                        log::error!("predicate eval failed: {err}");
                        false
                    }
                }
            }
        }
    }
}

/// A trait for types whose values can be scaled.
///
/// This is a hack for this difficult-to-model-and-DRY data format. It is mostly
/// normal enough for composition, except for this one case where
/// [`ValueRefData`] sometimes says to return a value from the scale.
trait ScaledValue<'s> {
    /// Returns the value as a raw value.
    fn get_raw(&self, node: &Node<'s, '_>) -> Option<Value<'s>>;

    /// Returns true if the returned value should actually just be the scale’s
    /// bandwidth.
    fn use_scale_band(&self) -> bool;
}

/// A generic reference to a scaled value.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) struct ScaledValueRef<'s, T> {
    /// The scale to apply to the value.
    #[serde(borrow, default)]
    scale: Option<ScaleRef<'s>>,
    /// The value.
    #[serde(flatten)]
    value: T,
}

impl<T> From<T> for ScaledValueRef<'_, T> {
    fn from(value: T) -> Self {
        Self { scale: None, value }
    }
}

impl<'s, T> Getter<'s> for ScaledValueRef<'s, T>
where
    T: Getter<'s> + ScaledValue<'s>,
    T::Item: FromValue<'s>,
{
    type Item = T::Item;

    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        if self.value.use_scale_band() {
            self.scale
                .as_ref()
                .and_then(|scale| scale.get(node))
                .map(|scale| T::Item::from_value(scale.range_band().into()))
        } else {
            self.value.get_raw(node).map(|value| {
                if let Some((scale, invert)) = self.scale.as_ref().and_then(|scale_ref| {
                    scale_ref.get(node).map(|scale| (scale, scale_ref.invert))
                }) {
                    T::Item::from_value(scale.apply(&value, invert))
                } else {
                    T::Item::from_value(value)
                }
            })
        }
    }
}

/// A reference to a scale.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(from = "ScaleRefRepr<'_>")]
struct ScaleRef<'s> {
    /// If true, an inverse of the scale transform is applied (i.e.,
    /// transforming from the visual range to the data domain).
    #[serde(default)]
    invert: bool,
    /// The scale name.
    #[serde(borrow)]
    name: FieldRef<'s>,
}

impl<'s, T> From<T> for ScaleRef<'s>
where
    T: Into<FieldRef<'s>>,
{
    fn from(value: T) -> Self {
        Self {
            invert: false,
            name: value.into(),
        }
    }
}

impl<'s> ScaleRef<'s> {
    /// Gets the scale.
    fn get<'b>(&self, node: &'b Node<'s, 'b>) -> Option<ScaleNode<'s, 'b>> {
        let name = if let FieldRef::Field(name) = &self.name {
            Cow::Borrowed(&**name)
        } else {
            self.name.get(node)?.into_string()
        };
        node.scale(&name)
    }
}

/// A serialised reference to a scale.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum ScaleRefRepr<'s> {
    /// A scale name.
    #[serde(borrow)]
    Short(FieldRef<'s>),
    /// A scale name with options.
    Long {
        /// If true, an inverse of the scale transform is applied (i.e.,
        /// transforming from the visual range to the data domain).
        #[serde(default)]
        invert: bool,
        /// The scale name.
        #[serde(borrow)]
        name: FieldRef<'s>,
    },
}

impl<'s> From<ScaleRefRepr<'s>> for ScaleRef<'s> {
    fn from(value: ScaleRefRepr<'s>) -> Self {
        let (invert, name) = match value {
            ScaleRefRepr::Short(name) => (false, name),
            ScaleRefRepr::Long { invert, name } => (invert, name),
        };
        Self { invert, name }
    }
}

/// A pattern of dashes and gaps used to paint the outline of a shape.
#[derive(Clone, Debug, serde::Deserialize)]
pub(super) struct StrokeDashArray(Vec<f64>);

impl From<Vec<f64>> for StrokeDashArray {
    fn from(value: Vec<f64>) -> Self {
        Self(value)
    }
}

impl From<Value<'_>> for StrokeDashArray {
    fn from(value: Value<'_>) -> Self {
        Self(
            value
                .into_string()
                .split([' ', ','])
                .map(|value| value.parse::<f64>().unwrap_or(f64::NAN))
                .collect(),
        )
    }
}

impl From<StrokeDashArray> for Value<'_> {
    fn from(value: StrokeDashArray) -> Self {
        Value::Str(Cow::Owned(value.to_string()))
    }
}

from_value_impl!(StrokeDashArray);

impl core::fmt::Display for StrokeDashArray {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut first = true;
        for value in &self.0 {
            if first {
                first = false;
            } else {
                write!(f, " ")?;
            }
            write!(f, "{value}")?;
        }
        Ok(())
    }
}

/// A literal value or indirect data reference.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ValueRefData<'s, T> {
    /// Literal value.
    Value(T),
    /// Indirect data reference.
    #[serde(borrow)]
    Field(FieldRef<'s>),
    /// Dynamic data reference.
    #[serde(borrow)]
    Signal(Cow<'s, str>),
    /// If true, and `scale` is specified, uses the range band of the scale as
    /// the retrieved value. This option is useful for determining widths with a
    /// band scale (an ordinal scale where `points` is `false`).
    // TODO: Since the JSON schema says this is exclusive with value and field,
    // it is unclear what is supposed to happen if it is `false` or if `scale`
    // is unspecified.
    Band(bool),
}

impl<'s, T> ValueRefData<'s, T> {
    /// Creates a new `ValueRefData` using a field reference.
    #[inline]
    pub fn with_field(field: impl Into<FieldRef<'s>>) -> Self {
        Self::Field(field.into())
    }
}

impl<'s, T> ScaledValue<'s> for ValueRefData<'s, T>
where
    T: Clone + Into<Value<'s>>,
{
    #[inline]
    fn get_raw(&self, node: &Node<'s, '_>) -> Option<Value<'s>> {
        match self {
            Self::Value(value) => Some(value.clone().into()),
            Self::Field(field) => field.get(node),
            Self::Signal(name) => match &**name {
                "width" => Some(Value::from(node.width())),
                "height" => Some(Value::from(node.height())),
                _ => {
                    log::warn!("TODO: signal {name}");
                    None
                }
            },
            Self::Band(true) => {
                unreachable!("ScaledValueRef should not delegate to `get` in this case")
            }
            Self::Band(false) => None,
        }
    }

    #[inline]
    fn use_scale_band(&self) -> bool {
        matches!(self, Self::Band(true))
    }
}

impl<T> From<T> for ValueRefData<'_, T> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}

impl<'s, T> Getter<'s> for ValueRefData<'s, T>
where
    T: Clone + FromValue<'s>,
{
    type Item = T;

    #[inline]
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        match self {
            Self::Value(value) => Some(value.clone()),
            Self::Field(field) => field.get(node).map(T::from_value),
            Self::Signal(name) => {
                log::warn!("Signal {name}");
                None
            }
            Self::Band(true) => {
                unreachable!("ScaledValueRef should not delegate to `get` in this case")
            }
            Self::Band(false) => None,
        }
    }
}

/// A template string or value reference.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) struct ValueRefNumber<'s> {
    /// A multiplier for the value, equivalent to `(mult * value)`. Multipliers
    /// are applied after any scale transformation.
    #[serde(default)]
    mult: Option<f64>,
    /// A simple additive offset to bias the final value, equivalent to
    /// `(value + offset)`. Offsets are added after any scale transformation
    /// and multipliers.
    #[serde(default)]
    offset: Option<f64>,
    /// The value.
    #[serde(borrow, flatten)]
    value: ScaledValueRef<'s, ValueRefData<'s, f64>>,
}

impl<'s> ValueRefNumber<'s> {
    /// Creates a new `ValueRefNumber` using a field reference.
    #[inline]
    pub fn with_field_and_scale(
        field: impl Into<FieldRef<'s>>,
        scale: Option<Cow<'s, str>>,
        mult: Option<f64>,
        offset: Option<f64>,
    ) -> Self {
        Self {
            mult,
            offset,
            value: ScaledValueRef {
                scale: scale.map(Into::into),
                value: ValueRefData::Field(field.into()),
            },
        }
    }

    /// Creates a new `ValueRefNumber` using a value.
    #[inline]
    pub fn with_value(value: f64, mult: Option<f64>, offset: Option<f64>) -> Self {
        Self {
            mult,
            offset,
            value: ValueRefData::Value(value).into(),
        }
    }
}

impl<'s, T: Into<ValueRefData<'s, f64>>> From<T> for ValueRefNumber<'s> {
    fn from(value: T) -> Self {
        Self {
            mult: None,
            offset: None,
            value: ScaledValueRef::from(value.into()),
        }
    }
}

impl<'s> Getter<'s> for ValueRefNumber<'s> {
    type Item = f64;

    fn get(&self, node: &Node<'s, '_>) -> Option<f64> {
        self.value
            .get(node)
            .map(|value| value * self.mult.unwrap_or(1.0) + self.offset.unwrap_or(0.0))
    }
}

/// A template string or value reference.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ValueRefTemplateData<'s> {
    /// A template string.
    #[serde(borrow)]
    Template(Cow<'s, str>),
    /// A value reference.
    #[serde(borrow, untagged)]
    ValueRef(ValueRefData<'s, Cow<'s, str>>),
}

impl<'s> ScaledValue<'s> for ValueRefTemplateData<'s> {
    #[inline]
    fn get_raw(&self, node: &Node<'s, '_>) -> Option<Value<'s>> {
        match self {
            ValueRefTemplateData::Template(template) => {
                let ast = super::template::Ast::new(template).unwrap();
                Some(ast.eval(node).into())
            }
            ValueRefTemplateData::ValueRef(value) => value.get_raw(node),
        }
    }

    #[inline]
    fn use_scale_band(&self) -> bool {
        matches!(self, Self::ValueRef(ValueRefData::Band(true)))
    }
}

impl<'s> From<FieldRef<'s>> for ValueRefTemplateData<'s> {
    fn from(value: FieldRef<'s>) -> Self {
        Self::ValueRef(ValueRefData::Field(value))
    }
}

impl<'s, T: Into<Cow<'s, str>>> From<T> for ValueRefTemplateData<'s> {
    fn from(value: T) -> Self {
        Self::ValueRef(ValueRefData::Value(value.into()))
    }
}

impl<'s> Getter<'s> for ValueRefTemplateData<'s> {
    type Item = Cow<'s, str>;

    #[inline]
    fn get(&self, node: &Node<'s, '_>) -> Option<Self::Item> {
        self.get_raw(node).map(Value::into_string)
    }
}

/// Finds the node `level` steps up the node tree. If level is `None`, it will
/// return the parent `Node`.
fn ascend<'b, 's>(node: &'b Node<'s, 'b>, level: Option<f64>) -> Option<&'b Node<'s, 'b>> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bogus values will just cause the loop to terminate at the root"
    )]
    let mut level = level.map_or(1, |level| level as u32);
    let mut node = node;
    #[rustfmt::skip]
    while level != 0 && let Some(parent) = node.parent {
        node = parent;
        level -= 1;
    };

    (level == 0).then_some(node)
}
