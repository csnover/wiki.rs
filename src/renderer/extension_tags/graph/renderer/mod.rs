//! Types and functions for converting a Vega specification to SVG.

use super::{
    super::svg::{NS_SVG, ValueDisplay, n},
    Node, Result,
    axis::{Axis, Layer},
    legend::Legend,
    propset::{Getter as _, Propset},
    spec::{Container, Padding, Spec},
};
use crate::php::DateTime;
use core::f64::consts::FRAC_PI_2;
pub(crate) use format::{Error as FormatError, NumberFormatter};
pub(crate) use mark::shape_text;
use minidom::Element;
use rustybuzz::{Face, GlyphBuffer, ttf_parser::GlyphId};
use tiny_skia::{Path, PathBuilder};

mod axis;
mod format;
mod interp;
mod legend;
mod mark;
mod path;

/// Common default visual properties.
mod defaults {
    /// Applies default values to a [`Propset`].
    macro_rules! apply {
        ($properties:ident, {
            $($key:ident => $value:expr),* $(,)?
        }) => {{
            $($properties.$key.get_or_insert_with(|| $value.into());)*
        }}
    }

    pub(crate) use apply;

    /// The default for the [`size`](Propset::size) of a symbol mark.
    pub(crate) const SYMBOL_SIZE: f64 = 50.0;
    /// The default for the [`fill`](Propset::fill) of an axis or legend title.
    pub(crate) const TITLE_COLOR: &str = "#000";
    /// The default for the [`font`](Propset::font) of an axis or legend title.
    pub(crate) const TITLE_FONT: &str = "sans-serif";
    /// The default for the [`font_size`](Propset::font_size) of an axis or
    /// legend title.
    pub(crate) const TITLE_FONT_SIZE: f64 = 11.0;
    /// The default for the [`font_weight`](Propset::font_weight) of an axis or
    /// legend title.
    pub(crate) const TITLE_FONT_WEIGHT: &str = "bold";
}

/// Renders a Vega specification to SVG.
pub(super) fn render(spec: &Spec<'_>, now: DateTime) -> Result<Element> {
    let mut svg = Element::builder("svg", NS_SVG).attr(n!("class"), "wiki-rs-graph");
    if let Some(bg) = &spec.background {
        svg = svg.attr(n!("background-color"), bg.to_string());
    }
    let mut svg = svg.build();
    let rng = RefCell::new(SmallRng::seed_from_u64(0));
    let node = Node::new(spec, &rng, now);
    let (box_bounds, axis_bounds) = draw_container(&mut svg, &spec.group, &node)?;
    let bounds = {
        let base = box_bounds.union(&Rect::from_xywh(0.0, 0.0, spec.width(), spec.height()));
        match &spec.padding {
            &Some(Padding::Uniform(p)) => base.inset(&Rect::new(-p, -p, p, p)),
            Some(Padding::Inset {
                bottom,
                left,
                right,
                top,
            }) => base.inset(&Rect::new(
                -left.unwrap_or(0.0),
                -top.unwrap_or(0.0),
                right.unwrap_or(0.0),
                bottom.unwrap_or(0.0),
            )),
            Some(Padding::Strict) => todo!(),
            Some(Padding::Auto) | None => {
                const AUTO_PAD_INSET: f64 = 5.0;
                base.union(&axis_bounds).inset(&Rect::new(
                    -AUTO_PAD_INSET,
                    -AUTO_PAD_INSET,
                    AUTO_PAD_INSET,
                    AUTO_PAD_INSET,
                ))
            }
        }
    };

    let (x, y, width, height) = bounds.to_xywh();
    let (width, height) = if let [vw, vh, ..] = spec.viewport.as_slice() {
        (*vw, *vh)
    } else {
        (width, height)
    };

    svg.set_attr("".into(), n!("width"), width.v());
    svg.set_attr("".into(), n!("height"), height.v());
    svg.set_attr(
        "".into(),
        n!("viewBox"),
        format!(
            "{x} {y} {width} {height}",
            x = x.v(),
            y = y.v(),
            width = width.v(),
            height = height.v()
        ),
    );

    Ok(svg)
}

/// Draws a container axis.
fn draw_axis<'s>(owner: &mut Element, axis: &Axis<'s>, parent: &Node<'s, '_>) -> Result<Rect> {
    let (mark, bounds) = axis::axis_to_mark(axis, parent)?;
    if let Some((element, _)) = mark::draw_mark(&mark, parent)? {
        owner.append_child(element);
    }
    Ok(bounds)
}

/// Draws an entire [`Container`] to the given `owner`.
fn draw_container<'s, 'b>(
    owner: &mut Element,
    container: &'b Container<'s>,
    node: &'b Node<'s, '_>,
) -> Result<(Rect, Rect)> {
    // TODO: Each mark needs to do hover effects by writing CSS.
    let is_root = node.parent.is_none();

    // Even though scene is allowed on group marks, it does not seem to actually
    // apply there. Only on the root does this clearly do anything.
    if is_root && let Some(scene) = &container.scene {
        let bg = to_svg!(Element::builder("rect", NS_SVG), scene, {
            fill => "fill",
            fill_opacity => "fill-opacity",
            stroke => "stroke",
            stroke_dash => "stroke-dasharray",
            stroke_dash_offset => "stroke-dashoffset",
            stroke_opacity => "stroke-opacity",
            stroke_width => "stroke-width"
        })
        .attr(n!("width"), node.width().v())
        .attr(n!("height"), node.height().v())
        .build();
        owner.append_child(bg);
    }

    // TODO: This bounds calculation is bad because initially it seemed crazy
    // that someone would design a library where the container size is not
    // precomputed, but then it turned out Vega does that, and then it seemed
    // crazy to have to calculate bounding boxes for every single thing since
    // only the axes go outside the data area, but then it turned out that Vega
    // needs a whole complete rasteriser just to calculate layout. So this
    // attempt at work-avoidance is now just a confusing contortion that should
    // go away and be replaced by something that just gives out bounding boxes
    // like candy.
    let mut box_bounds = Rect::default();
    let mut axis_bounds = Rect::default();

    for axis in container
        .axes
        .iter()
        .filter(|axis| axis.layer == Layer::Back)
    {
        axis_bounds = axis_bounds.union(&draw_axis(owner, axis, node)?);
    }

    for mark in &container.marks {
        if let Some((element, bounds)) = mark::draw_mark(mark, node)? {
            owner.append_child(element);
            if is_root && bounds.is_empty() {
                box_bounds = box_bounds.union(&mark::calculate_bounds(mark, node));
            } else {
                box_bounds = box_bounds.union(&bounds);
            }
        }
    }

    for legend in &container.legends {
        box_bounds = box_bounds.union(&draw_legend(owner, legend, node)?);
    }

    for axis in container
        .axes
        .iter()
        .filter(|axis| axis.layer == Layer::Front)
    {
        axis_bounds = axis_bounds.union(&draw_axis(owner, axis, node)?);
    }

    Ok((box_bounds, axis_bounds))
}

/// Creates an SVG element for a [`Legend`].
fn draw_legend<'b, 's>(
    owner: &mut Element,
    legend: &'b Legend<'s>,
    parent: &'b Node<'s, '_>,
) -> Result<Rect> {
    let (mark, bounds) = legend::legend_to_mark(legend, parent)?;
    if let Some((element, _)) = mark::draw_mark(&mark, parent)? {
        owner.append_child(element);
    }
    Ok(bounds)
}

/// Converts shaped text from a rustybuzz [`GlyphBuffer`] into a tiny-skia
/// [`Path`].
pub(super) fn buffer_to_path(face: &Face<'_>, buffer: &GlyphBuffer) -> Option<Path> {
    /// A glyph-to-path converter for runs of text.
    #[derive(Default)]
    struct GlyphPath {
        /// The path for the combined text run.
        path: PathBuilder,
        /// The currently processing glyph.
        segment: PathBuilder,
        /// The x-position for the current segment.
        x: f32,
    }

    impl rustybuzz::ttf_parser::OutlineBuilder for GlyphPath {
        #[inline]
        fn move_to(&mut self, x: f32, y: f32) {
            self.segment.move_to(x + self.x, -y);
        }

        #[inline]
        fn line_to(&mut self, x: f32, y: f32) {
            self.segment.line_to(x + self.x, -y);
        }

        #[inline]
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.segment.quad_to(x1 + self.x, -y1, x + self.x, -y);
        }

        #[inline]
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.segment
                .cubic_to(x1 + self.x, -y1, x2 + self.x, -y2, x + self.x, -y);
        }

        #[inline]
        fn close(&mut self) {
            self.segment.close();
        }
    }

    let mut path = GlyphPath::default();
    for (info, pos) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
        // Clippy: `info.glyph_id` is guaranteed to be <= u16::MAX. And there is
        // even a helper function in rustybuzz to do this conversion, which as
        // of 0.20 is uselessly locked behind `pub(crate)` for no reason.
        #[allow(clippy::cast_possible_truncation)]
        let glyph_id = GlyphId(info.glyph_id as u16);

        // “… since ttf-parser is a pull parser, OutlineBuilder will emit
        // segments even when outline is partially malformed. You must check
        // outline_glyph() result before using OutlineBuilder's output.”
        if face.outline_glyph(glyph_id, &mut path).is_some()
            // “Returns None when Path is empty or has invalid bounds.”
            && let Some(segment) = core::mem::take(&mut path.segment).finish()
        {
            path.path.push_path(&segment);
        } else {
            path.segment.clear();
        }

        // Clippy: If the x-advance is >=2**24, something has gone wrong.
        #[allow(clippy::cast_precision_loss)]
        {
            path.x += pos.x_advance as f32;
        }
    }

    path.path.finish()
}

/// A trait for items which can be contained inside a `Rect`.
pub(super) trait Containable {
    /// Returns `true` if `self` is fully inside `container`.
    fn contained_by(self, container: &Rect) -> bool;
}

/// A 1bpp bitmap.
#[must_use]
pub(super) struct Pixels {
    /// The pixel data.
    pub data: Vec<u8>,
    /// The stride of a line, in bytes.
    stride: u16,
    /// The width of a line, in pixels.
    width: u16,
}

impl Pixels {
    /// Number of pixels per byte.
    const BITS: usize = u8::BITS as usize;
    /// Mask of the bit position of an index.
    const BIT_MASK: usize = Self::BITS - 1;

    /// Creates a new bitmap with the given `width` and `height`.
    pub fn new(width: u16, height: u16) -> Self {
        let stride = usize::from(width).div_ceil(Self::BITS);
        let size = stride * usize::from(height);
        assert!(
            size <= 4_096 * 4_096,
            "bitmap resource limit reached; maximum size is 4096×4096"
        );
        // Clippy: This number came from a u16.
        #[allow(clippy::cast_possible_truncation)]
        let stride = stride as u16;
        Self {
            data: vec![0u8; size],
            stride,
            width,
        }
    }

    /// Returns the bounds of the bitmap as a [`Rect`].
    #[inline]
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width().into(), self.height().into())
    }

    /// Returns the midpoint of the bitmap as a [`Vec2`].
    #[inline]
    pub fn midpoint(&self) -> Vec2 {
        Vec2::new(
            f64::from(self.width()) / 2.0,
            f64::from(self.height()) / 2.0,
        )
    }

    /// An overengineered iterator over the index and mask of pixels within the
    /// given `rect`.
    // Clippy: The rect is clamped to the bounds of the bitmap, which must
    // be positive, and truncation does not matter.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn iter_indexes(&self, rect: &Rect) -> impl Iterator<Item = (usize, u8)> + use<> {
        let rect = rect.intersect(&self.bounds());

        let (mut byte_index, start_mask, to_next_line) = {
            let left = rect.left as usize;
            let top = rect.top as usize;
            let right = rect.right as usize;
            let stride = usize::from(self.stride);
            let index = left / Self::BITS;
            let mask = 1_u8 << (left & Self::BIT_MASK);
            let skip = stride - (right.div_ceil(Self::BITS) - index);
            (top * stride + index, mask, skip)
        };

        let (pixel_width, pixel_count) = {
            let width = rect.width() as usize;
            let height = rect.height() as usize;
            (width, width * height)
        };

        let mut x = 0;
        let mut mask = start_mask;
        (0..pixel_count).map(move |_| {
            if x == pixel_width {
                x = 0;
                byte_index += to_next_line + 1 - usize::from(mask & 1);
                mask = start_mask;
            }
            let value = (byte_index, mask);
            x += 1;
            mask = mask.rotate_left(1);
            byte_index += usize::from(mask & 1);
            value
        })
    }

    /// The height of the bitmap, in pixels.
    #[inline]
    #[must_use]
    // Clippy: This number came from a u16.
    #[allow(clippy::cast_possible_truncation)]
    pub fn height(&self) -> u16 {
        (self.data.len() / usize::from(self.stride)) as u16
    }

    /// The width of the bitmap, in pixels.
    #[inline]
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }
}

/// A rectangle.
#[derive(Clone, Copy, Default, PartialEq)]
#[must_use]
pub(super) struct Rect {
    /// Left edge.
    pub left: f64,
    /// Top edge.
    pub top: f64,
    /// Right edge, exclusive.
    pub right: f64,
    /// Bottom edge, exclusive.
    pub bottom: f64,
}

impl Rect {
    /// Creates a new `Rect`.
    #[inline]
    pub const fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            left: left.min(right),
            top: top.min(bottom),
            right: right.max(left),
            bottom: bottom.max(top),
        }
    }

    /// Creates a new `Rect` from `(x, y)`, width, and height. If width/height
    /// are negative, the box will be normalised rather than being made empty.
    #[inline]
    pub const fn from_xywh(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    /// Gets the bottom-left corner of the rect as a `Vec2`.
    #[inline]
    pub const fn bl(&self) -> Vec2 {
        Vec2::new(self.left, self.bottom)
    }

    /// Returns true if `self` fully contains `other`.
    #[inline]
    #[must_use]
    pub fn contains<C: Containable>(&self, other: C) -> bool {
        other.contained_by(self)
    }

    /// Creates a new `Rect` by extending the edges by the given amounts.
    #[inline]
    pub const fn inset(&self, inset: &Self) -> Self {
        Self {
            left: self.left + inset.left,
            top: self.top + inset.top,
            right: self.right + inset.right,
            bottom: self.bottom + inset.bottom,
        }
    }

    /// Returns true if `self` intersects `other`.
    #[inline]
    #[must_use]
    pub const fn intersects(&self, other: &Self) -> bool {
        self.left < other.right
            && self.top < other.bottom
            && self.right > other.left
            && self.bottom > other.top
    }

    /// Creates a new `Rect` which is the intersection of two rects.
    #[inline]
    pub const fn intersect(&self, other: &Self) -> Self {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right.min(other.right);
        let bottom = self.bottom.min(other.bottom);
        Self {
            left,
            top,
            right: right.max(left),
            bottom: bottom.max(top),
        }
    }

    /// Gets the height of the rectangle.
    #[inline]
    #[must_use]
    pub const fn height(&self) -> f64 {
        (self.bottom - self.top).max(0.0)
    }

    /// Returns true if the rectangle has a zero dimension on either side.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width() == 0.0 || self.height() == 0.0
    }

    /// Creates a new `Rect` offset by the given distance `offset`.
    #[inline]
    pub const fn offset(&self, offset: Vec2) -> Self {
        Self {
            left: self.left + offset.x,
            top: self.top + offset.y,
            right: self.right + offset.x,
            bottom: self.bottom + offset.y,
        }
    }

    /// Gets the `(x, y)`, width, and height.
    #[inline]
    #[must_use]
    fn to_xywh(self) -> (f64, f64, f64, f64) {
        (self.left, self.top, self.width(), self.height())
    }

    /// Creates a new `Rect` which is the union of two rects.
    #[inline]
    pub const fn union(&self, other: &Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    /// Gets the width of the rectangle.
    #[inline]
    #[must_use]
    pub const fn width(&self) -> f64 {
        (self.right - self.left).max(0.0)
    }
}

impl Containable for Rect {
    fn contained_by(self, container: &Rect) -> bool {
        self.left >= container.left
            && self.top >= container.top
            && self.right <= container.right
            && self.bottom <= container.bottom
    }
}

impl core::fmt::Debug for Rect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rect")
            .field("left", &self.left)
            .field("top", &self.top)
            .field("right", &self.right)
            .field("bottom", &self.bottom)
            .field("(width)", &self.width())
            .field("(height)", &self.height())
            .finish()
    }
}

/// Basic text metrics.
struct TextMetrics {
    /// The origin x-coordinate.
    x: f64,
    /// The origin y-coordinate.
    y: f64,
    /// The displacement x-coordinate.
    dx: f64,
    /// The displacement y-coordinate.
    dy: f64,
    /// The font size.
    font_size: f64,
}

/// Calculates basic text metrics required for both drawing and calculating
/// the bounding box of some text mark.
fn text_metrics<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> TextMetrics {
    let mut x = propset.x.get(node).unwrap_or(0.0);
    let mut y = propset.y.get(node).unwrap_or(0.0);
    if let Some(radius) = propset.radius.get(node) {
        let theta = propset.theta.get(node).unwrap_or(0.0) - FRAC_PI_2;
        x += radius * theta.cos();
        y += radius * theta.sin();
    }
    let dx = propset.dx.get(node).unwrap_or(0.0);
    let dy = propset.dy.get(node).unwrap_or(0.0);

    let font_size = propset
        .font_size
        .get(node)
        .unwrap_or(Axis::TICK_LABEL_FONT_SIZE);

    TextMetrics {
        x,
        y,
        dx,
        dy,
        font_size,
    }
}

/// A two-dimensional vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[must_use]
pub(super) struct Vec2 {
    /// The x-coordinate.
    pub x: f64,
    /// The y-coordinate.
    pub y: f64,
}

impl Vec2 {
    /// Creates a new `Vec2` from `(x, y)`.
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Creates a new `Vec2` containing a latitude and logitude, in radians,
    /// from a spherical coordinate.
    #[inline]
    pub fn from_spherical(spherical: Vec3) -> Self {
        Self {
            x: spherical.y.atan2(spherical.x),
            y: spherical.z.asin(),
        }
    }

    /// Creates an invalid vector.
    #[inline]
    pub const fn invalid() -> Self {
        Self {
            x: f64::NAN,
            y: f64::NAN,
        }
    }

    /// Creates a new `Vec2` from `(θ, r)`.
    #[inline]
    fn polar(angle: f64, radius: f64) -> Self {
        Self {
            x: angle.cos() * radius,
            y: angle.sin() * radius,
        }
    }

    /// Creates a zero vector.
    #[inline]
    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Gets the angle of the vector, in radians.
    #[inline]
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    /// Gets the cross-product of `self` and `other`.
    #[inline]
    pub const fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// Gets the dot-product of `self` and `other`.
    #[inline]
    pub const fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Extends the `Vec2` into a `Vec3`.
    #[inline]
    pub const fn extend(self, z: f64) -> Vec3 {
        Vec3::with_z(self, z)
    }

    /// Gets the Euclidean length of `self`.
    #[inline]
    pub fn len(self) -> f64 {
        self.square_len().sqrt()
    }

    /// Gets the midpoint between `self` and `other`.
    #[inline]
    pub const fn mid(self, other: Self) -> Self {
        Self {
            x: self.x.midpoint(other.x),
            y: self.y.midpoint(other.y),
        }
    }

    /// Gets `self` rounded to whole numbers.
    #[cfg(test)]
    #[inline]
    pub const fn round(self) -> Self {
        Self {
            x: self.x.round(),
            y: self.y.round(),
        }
    }

    /// Gets the square length of `self`.
    #[inline]
    pub const fn square_len(self) -> f64 {
        self.dot(self)
    }

    /// Gets `self` truncated to whole numbers.
    #[inline]
    pub const fn trunc(self) -> Self {
        Self {
            x: self.x.trunc(),
            y: self.y.trunc(),
        }
    }

    /// Gets `self` converted from radians to degrees.
    #[inline]
    pub const fn to_degrees(self) -> Self {
        Self {
            x: self.x.to_degrees(),
            y: self.y.to_degrees(),
        }
    }

    /// Gets `self` converted from degrees to radians.
    #[inline]
    pub const fn to_radians(self) -> Self {
        Self {
            x: self.x.to_radians(),
            y: self.y.to_radians(),
        }
    }

    /// Swaps `x` and `y`.
    #[inline]
    pub const fn yx(self) -> Self {
        Self {
            x: self.y,
            y: self.x,
        }
    }
}

impl core::ops::Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl core::ops::Add<f64> for Vec2 {
    type Output = Self;

    fn add(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
        }
    }
}

impl core::ops::AddAssign<f64> for Vec2 {
    fn add_assign(&mut self, rhs: f64) {
        *self = *self + rhs;
    }
}

impl core::ops::Div<f64> for Vec2 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl core::ops::Mul<f64> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl core::ops::Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

impl core::ops::Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl From<(f64, f64)> for Vec2 {
    fn from(value: (f64, f64)) -> Self {
        Self {
            x: value.0,
            y: value.1,
        }
    }
}

impl Containable for Vec2 {
    #[inline]
    fn contained_by(self, container: &Rect) -> bool {
        (container.left..=container.right).contains(&self.x)
            && (container.top..=container.bottom).contains(&self.y)
    }
}

/// A three-dimensional vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[must_use]
pub(super) struct Vec3 {
    /// The x-coordinate.
    pub x: f64,
    /// The y-coordinate.
    pub y: f64,
    /// The z-coordinate.
    pub z: f64,
}

impl Vec3 {
    /// Creates a new `Vec3` from `(x, y, z)`.
    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Creates a new `Vec3` of spherical coordinates from a `Vec2` of a
    /// latitude and longitude in radians.
    #[inline]
    pub fn from_cartesian(Vec2 { x: lambda, y: phi }: Vec2) -> Self {
        let cos_phi = phi.cos();
        Self {
            x: cos_phi * lambda.cos(),
            y: cos_phi * lambda.sin(),
            z: phi.sin(),
        }
    }

    /// Creates a new `Vec3` from a `Vec2` plus `z`.
    #[inline]
    pub const fn with_z(p: Vec2, z: f64) -> Self {
        Self { x: p.x, y: p.y, z }
    }

    /// Gets the cross-product of `self` and `other`.
    #[inline]
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    /// Gets the dot-product of `self` and `other`.
    #[inline]
    #[must_use]
    pub const fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Gets the Euclidean length of `self`.
    #[inline]
    #[must_use]
    pub fn len(self) -> f64 {
        self.square_len().sqrt()
    }

    /// Gets the vector with a length of one unit.
    #[inline]
    pub fn norm(self) -> Self {
        self / self.len()
    }

    /// Gets the square length of `self`.
    #[inline]
    #[must_use]
    pub const fn square_len(self) -> f64 {
        self.dot(self)
    }

    /// Gets the x- and y-coordinates as a `Vec2`.
    #[inline]
    pub const fn xy(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

impl core::ops::Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl core::ops::AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl core::ops::Div<f64> for Vec3 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

impl core::ops::Mul<f64> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl core::ops::Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}

impl core::ops::Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

/// Shorthand for applying a [`Propset`] or [`Scene`] to an SVG element.
macro_rules! to_svg {
    ($element:expr, $scene:ident, {
        $($prop:ident => $attr:literal),* $(,)?
    }) => {{
        let mut element = $element;
        $(
            if let Some(value) = $scene.$prop.as_ref().map(|value| value.to_string()) {
                element = element.attr(n!($attr), value);
            }
        )*
        element
    }};

    ($element:expr, $propset:ident, $node:ident, {
        $($prop:ident => $attr:literal),* $(,)?
    }) => {{
        let mut element = $element;
        $(
            if let Some(value) = $propset.$prop.get($node).map(|value| value.to_string()) {
                element = element.attr(n!($attr), value);
            }
        )*
        element
    }}
}

use rand::{SeedableRng, rngs::SmallRng};
use std::cell::RefCell;
use to_svg;
