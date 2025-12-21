//! Implementation of the EasyTimeline extension.

use crate::{renderer::LinkKind, title::Title};
use axum::http::Uri;
use either::Either;

mod grammar;
mod parser;
mod renderer;
#[cfg(test)]
mod tests;

/// Converts an EasyTimeline script into a serialised SVG image.
pub fn timeline_to_svg(input: &str, base_uri: &Uri) -> Result<String> {
    let (expanded, deltas) = parser::expand(input.trim_ascii_end())?;
    let pen = parser::parse(&expanded, input, &deltas)?;
    let svg = renderer::render(pen, base_uri)?;

    let mut out = Vec::new();
    svg.write_to(&mut out)?;
    // SAFETY: We just wrote this from strs.
    Ok(unsafe { String::from_utf8_unchecked(out) })
}

/// An EasyTimeline error.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Use of undefined colour.
    #[error("missing color '{0}'")]
    Color(String),
    /// Missing required `ImageSize` command.
    #[error("missing ImageSize")]
    ImageSize,
    /// Use of `barset` without corresponding `BarData`.
    #[error("barset must be defined in BarData")]
    ImplicitBarset,
    /// Script parsing error.
    #[error(transparent)]
    Parse(#[from] peg::error::ParseError<peg::str::LineCol>),
    /// Missing one or more plot area dimensions.
    #[error("incomplete PlotArea")]
    PlotArea,
    /// Out-of-range time.
    #[error("time {0:?} not in range {1:?}..={2:?}")]
    Time(Time, Time, Time),
    /// XML library error.
    #[error(transparent)]
    Xml(#[from] minidom::Error),
}

/// A colour identifier.
type ColorId<'input> = &'input str;
/// The EasyTimeline result type.
pub type Result<T = (), E = Error> = core::result::Result<T, E>;
/// A URL string.
type Url<'input> = &'input str;

/// Cross-axis alignment of timeline series bars.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AlignBars {
    /// Draw bars starting at the origin.
    ///
    /// ```text
    /// │▌
    /// │▌  ▌
    /// │▌  ▌  ▌
    /// └─────────
    ///  A  B  C
    /// ```
    #[default]
    Early,
    /// Draw bars starting away from the origin.
    ///
    /// ```text
    /// │  ▐
    /// │  ▐  ▐
    /// │  ▐  ▐  ▐
    /// └─────────
    ///    A  B  C
    /// ```
    Late,
    /// Evenly distribute space between the bars.
    ///
    /// ```text
    /// │▌
    /// │▌   ▌
    /// │▌   ▌   ▌
    /// └─────────
    ///  A   B   C
    /// ```
    Justify,
}

/// Item–container alignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Alignment {
    /// Align the item at the start of the container.
    Start,
    /// Align the item at the middle of the container.
    Middle,
    /// Align the item at the end of the container.
    End,
}

impl Alignment {
    /// Get the alignment as an SVG alignment string.
    fn to_svg(self) -> &'static str {
        match self {
            Alignment::Start => "start",
            Alignment::Middle => "middle",
            Alignment::End => "end",
        }
    }
}

/// Time series definition.
#[derive(Debug)]
struct Bar<'input> {
    /// The index of the time series.
    index: usize,
    /// The label for the time series.
    label: Vec<TextSpan<'input>>,
    /// The link URL for the time series label.
    link: Option<Url<'input>>,
}

/// A colour value.
#[derive(Clone, Copy, Debug)]
enum ColorValue {
    /// Hue–saturation–brightness.
    Hsb(f64, f64, f64),
    /// A CSS string name for a predefined colour.
    Named(&'static str),
    /// Red–green–blue.
    Rgb(f64, f64, f64),
}

impl Default for ColorValue {
    fn default() -> Self {
        Self::Named("#000")
    }
}

/// IR date format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum DateFormat {
    /// A date in the format "mm/dd/yyyy".
    American,
    /// A date in the format "x.y".
    #[default]
    Decimal,
    /// A date in the format "yyyy-mm-dd".
    Iso8601,
    /// A date in the format "dd/mm/yyyy".
    Normal,
    /// A date in the format "yyyy".
    Year,
}

/// 2D dimensions with orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Dims(f64, f64, Orientation);

impl Dims {
    /// The length of the cross axis.
    fn cross_axis(&self) -> f64 {
        match self.2 {
            Orientation::Horizontal => self.1,
            Orientation::Vertical => self.0,
        }
    }

    /// The vertical height.
    fn height(&self) -> f64 {
        self.1
    }

    /// The horizontal width.
    fn width(&self) -> f64 {
        self.0
    }
}

/// A font size.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FontSize {
    /// Absolute font size in points (1/72in).
    Absolute(i32),
    /// Extra small font.
    Smaller,
    /// Small font.
    #[default]
    Small,
    /// Medium font.
    Medium,
    /// Large font.
    Large,
    /// Extra large font.
    Larger,
}

impl FontSize {
    /// Calculates a line height for this font size.
    fn line_height(self) -> f64 {
        match self {
            FontSize::Absolute(n) => {
                let n = f64::from(n) * 96.0 / 72.0;
                (n * 1.2).max(n + 2.0)
            }
            FontSize::Smaller => 11.0,
            FontSize::Small => 13.0,
            FontSize::Medium => 15.5,
            FontSize::Large => 19.0,
            FontSize::Larger => 24.0,
        }
    }

    /// Returns the font size in CSS pixels.
    fn value(self) -> f64 {
        match self {
            FontSize::Absolute(n) => f64::from(n) * 96.0 / 72.0,
            FontSize::Smaller => 9.0,
            FontSize::Small => 11.0,
            FontSize::Medium => 13.0,
            FontSize::Large => 18.0,
            FontSize::Larger => 24.0,
        }
    }
}

/// An image size.
#[derive(Clone, Copy, Debug)]
enum ImageSize {
    /// The time axis is orientated horizontally and the height is calculated
    /// automatically according to `Unit * bars.len()`.
    AutoHeight {
        /// The cross-axis size of each bar.
        bar: f64,
        /// The width of the image, in CSS pixels.
        width: f64,
    },
    /// The time axis is orientated vertically and the width is calculated
    /// automatically according to `Unit * bars.len()`.
    AutoWidth {
        /// The cross-axis size of each bar.
        bar: f64,
        /// The height of the image, in CSS pixels.
        height: f64,
    },
    /// The image has a fixed width and height.
    Fixed {
        /// The width of the image, in CSS pixels.
        width: f64,
        /// The height of the image, in CSS pixels.
        height: f64,
    },
}

/// A timeline legend.
#[derive(Clone, Debug, Default)]
struct Legend {
    /// The orientation of the legend.
    orientation: Orientation,
    /// The position of the legend relative to the plot area.
    position: LegendPosition,
    /// The number of columns for the legend.
    columns: Option<u8>,
    /// The width of columns.
    column_width: Option<Unit>,
    /// The left edge of the legend relative to the left edge of the image.
    left: Option<Unit>,
    /// The top edge of the legend relative to the bottom edge of the image.
    top: Option<Unit>,
}

/// Plot-relative legend positioning.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum LegendPosition {
    /// Position the legend below the plot area.
    #[default]
    Bottom,
    /// Position the legend at the right of the plot area.
    Right,
    /// Position the legend at the top of the plot area.
    Top,
}

/// A line drawing.
#[derive(Clone, Copy, Debug)]
struct Line<'input> {
    /// The colour of the line.
    color: ColorId<'input>,
    /// The draw instruction.
    instr: LineDataInstr,
    /// The stroke width.
    width: f64,
}

/// A line drawing instruction.
#[derive(Clone, Copy, Debug)]
enum LineDataInstr {
    /// Draw a line segment with the given start and end coordinates relative
    /// to the bottom-left of the image.
    Independent(Point, Point),
    /// Draw a line segment along the cross-axis of the timeline.
    Orthogonal {
        /// The position of the line along the main-axis of the timeline.
        at: Time,
        /// Absolute start position of the line along the cross-axis, relative
        /// to the start edge of the image (i.e. either the bottom or left edge
        /// of the image, depending on the time axis orientation). If `None`,
        /// the plot area cross-axis start.
        from: Option<Unit>,
        /// Absolute end position of the line along the cross-axis, relative
        /// to the start edge of the image (i.e. either the bottom or left edge
        /// of the image, depending on the time axis orientation). If `None`,
        /// the plot area cross-axis end.
        till: Option<Unit>,
    },
    /// Draw a line segment along the main-axis of the timeline.
    Parallel {
        /// Absolute position of the line along the main-axis, relative to the
        /// start edge of the image (i.e. either the bottom or left edge of the
        /// image, depending on the time axis orientation).
        at: Unit,
        /// The start position of the line along the main-axis of the timeline.
        from: Option<Time>,
        /// The end position of the line along the main-axis of the timeline.
        till: Option<Time>,
    },
}

/// Time direction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Order {
    /// Time flows up or right, depending on the time axis orientation.
    #[default]
    Normal,
    /// Time flows down or left, depending on the time axis orientation.
    Reverse,
}

/// Time axis orientation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Orientation {
    /// The main-axis is horizontal.
    Horizontal,
    /// The main-axis is vertical.
    #[default]
    Vertical,
}

/// A time range.
#[derive(Clone, Copy, Debug)]
struct Period {
    /// The start instant.
    from: Time,
    /// The end instant (inclusive).
    till: Time,
}

impl Period {
    /// Gets the given time as a non-normalised decimal value.
    fn get(&self, start: Time) -> f64 {
        match start {
            Time::Decimal(d) => d,
            Time::End => self.get(self.till),
            Time::Start => self.get(self.from),
        }
    }

    /// Normalises the given `time` within the period to the range 0.0..=1.0.
    fn norm(self, time: Time, reverse: bool) -> Result<f64> {
        match time {
            Time::End => return Ok(if reverse { 0.0 } else { 1.0 }),
            Time::Start => return Ok(if reverse { 1.0 } else { 0.0 }),
            Time::Decimal(_) => {}
        }

        let result = (time - self.from) / (self.till - self.from);
        if (0.0..=1.0).contains(&result) {
            Ok(if reverse { 1.0 - result } else { result })
        } else {
            Err(Error::Time(time, self.from, self.till))
        }
    }

    /// The number of `unit`s in the period.
    fn units(self, unit: ScaleUnit) -> f64 {
        let len = self.till - self.from;
        match len {
            Time::Decimal(y) => match unit {
                // Wrong, but matches behaviour of EasyTimeline
                ScaleUnit::Day => y * 365.0,
                ScaleUnit::Month => y * 12.0,
                ScaleUnit::Year => y,
            },
            Time::End | Time::Start => panic!(),
        }
    }
}

/// The plot area.
#[derive(Clone, Copy, Debug, Default)]
struct PlotArea {
    /// The left edge of the plot area, inset from the left of the image.
    left: Option<Unit>,
    /// The top edge of the plot area, inset from the top of the image, or the
    /// height.
    top: Option<PlotAreaEnd>,
    /// The right edge of the plot area, inset from the right of the image, or
    /// the width.
    right: Option<PlotAreaEnd>,
    /// The bottom edge of the plot area, inset from the bottom of the image.
    bottom: Option<Unit>,
}

/// A plot area end coordinate.
#[derive(Clone, Copy, Debug)]
enum PlotAreaEnd {
    /// An inset value.
    Inset(Unit),
    /// A dimension.
    Size(Unit),
}

impl PlotAreaEnd {
    /// Converts an end coordinate into an absolute length (width/height).
    fn into_len(self, start: f64, len: f64) -> f64 {
        match self {
            PlotAreaEnd::Inset(inset) => len - inset.into_abs(len) - start,
            PlotAreaEnd::Size(size) => size.into_abs(len),
        }
    }
}

impl Default for PlotAreaEnd {
    fn default() -> Self {
        Self::Inset(<_>::default())
    }
}

/// Timeline segment.
#[derive(Clone, Debug)]
struct Plot<'input> {
    /// The position of the segment in the series.
    at: PlotDataPos,
    /// The index of the corresponding series.
    index: usize,
    /// Drawing properties.
    pen: parser::PlotPen<'input>,
    /// Text label.
    text: Option<Vec<TextSpan<'input>>>,
    /// Text label URL.
    link: Option<Url<'input>>,
}

/// The position of a timeline segment.
#[derive(Clone, Copy, Debug)]
enum PlotDataPos {
    /// An instant.
    At(Time),
    /// A time range.
    Range(Time, Time),
}

/// A 2D point in quadrant I.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Point(f64, f64);

impl Point {
    /// Gets the x-coordinate.
    fn x(&self) -> f64 {
        self.0
    }

    /// Gets the y-coordinate in quadrant IV as an absolute value.
    fn y(&self, height: f64) -> f64 {
        height - self.1
    }

    /// Gets the y-coordinate in quadrant IV as a delta.
    fn y_shift(&self) -> f64 {
        -self.1
    }

    /// Gets a mutable reference to the y-coordinate in quadrant I.
    fn y_mut(&mut self) -> &mut f64 {
        &mut self.1
    }
}

/// A 2D rectangle with orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rect {
    /// The left edge of the rectangle.
    left: f64,
    /// The top edge of the rectangle.
    top: f64,
    /// The width of the rectangle.
    width: f64,
    /// The height of the rectangle.
    height: f64,
    /// The orientation of the rectangle.
    orientation: Orientation,
}

impl Rect {
    /// Creates a new [`Rect`].
    fn new(left: f64, top: f64, width: f64, height: f64, orientation: Orientation) -> Self {
        Self {
            left,
            top,
            width,
            height,
            orientation,
        }
    }

    /// The top edge in CSS pixels.
    fn top(&self) -> f64 {
        self.top
    }

    /// The left edge in CSS pixels.
    fn left(&self) -> f64 {
        self.left
    }

    /// The right edge in CSS pixels.
    fn right(&self) -> f64 {
        self.left + self.width
    }

    /// The bottom edge in CSS pixels.
    fn bottom(&self) -> f64 {
        self.top + self.height
    }

    /// The width in CSS pixels.
    fn width(&self) -> f64 {
        self.width
    }

    /// The height in CSS pixels.
    fn height(&self) -> f64 {
        self.height
    }

    /// The end of the main axis.
    ///
    /// ```text
    ///   │▐            A│━━━━━
    ///   │▐ ▐          B│━━━
    ///   │▐ ▐ ▐        C│━
    ///   └─────         └───── ← end
    ///   ↑A B C
    ///  end
    /// ```
    fn main_end(&self) -> f64 {
        match self.orientation {
            Orientation::Horizontal => self.right(),
            Orientation::Vertical => self.bottom(),
        }
    }

    /// The start of the main axis.
    ///
    /// ```text
    /// start
    ///   ↓
    ///   │▐            A│━━━━━
    ///   │▐ ▐          B│━━━
    ///   │▐ ▐ ▐        C│━
    ///   └─────  start →└─────
    ///    A B C
    /// ```
    fn main_start(&self) -> f64 {
        match self.orientation {
            Orientation::Horizontal => self.left(),
            Orientation::Vertical => self.top(),
        }
    }

    /// The length of the main axis.
    ///
    /// ```text
    /// start
    ///   ↓
    ///   │▐            A│━━━━━
    ///   │▐ ▐          B│━━━
    ///   │▐ ▐ ▐        C│━
    ///   └─────  start →└───── ← end
    ///   ↑A B C
    ///  end
    /// ```
    fn main_len(&self) -> f64 {
        match self.orientation {
            Orientation::Horizontal => self.width(),
            Orientation::Vertical => self.height(),
        }
    }

    /// The end of the cross axis.
    ///
    /// ```text
    ///  A│━━━━━  │▐
    ///  B│━━━    │▐ ▐
    ///  C│━      │▐ ▐ ▐
    ///   └─────  └───── ← end
    ///   ↑        A B C
    ///  end
    /// ```
    fn cross_end(&self) -> f64 {
        match self.orientation {
            Orientation::Horizontal => self.bottom(),
            Orientation::Vertical => self.right(),
        }
    }

    /// The start of the cross axis.
    ///
    /// ```text
    /// start
    ///   ↓
    ///  A│━━━━━         │▐
    ///  B│━━━           │▐ ▐
    ///  C│━             │▐ ▐ ▐
    ///   └─────  start →└─────
    ///                   A B C
    /// ```
    fn cross_start(&self) -> f64 {
        match self.orientation {
            Orientation::Horizontal => self.top(),
            Orientation::Vertical => self.left(),
        }
    }

    /// The length of the cross axis.
    ///
    /// ```text
    /// start
    ///   ↓
    ///  A│━━━━━         │▐
    ///  B│━━━           │▐ ▐
    ///  C│━             │▐ ▐ ▐
    ///   └─────  start →└───── ← end
    ///   ↑               A B C
    ///  end
    /// ```
    fn cross_len(&self) -> f64 {
        match self.orientation {
            Orientation::Horizontal => self.height(),
            Orientation::Vertical => self.width(),
        }
    }
}

/// A grid scale.
#[derive(Clone, Debug, Default)]
struct Scale<'input> {
    /// The colour of the grid lines.
    grid_color: Option<ColorId<'input>>,
    /// The scale grid line interval.
    interval: i32,
    /// The scale origin.
    start: Option<Time>,
    /// The time unit for the grid lines.
    unit: ScaleUnit,
}

/// A unit kind for scale intervals.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ScaleUnit {
    /// Each interval is one day.
    Day,
    /// Each interval is one month.
    Month,
    /// Each interval is one year.
    #[default]
    Year,
}

impl ScaleUnit {
    /// Gets the scale unit in fractions of a year.
    fn into_f64(self) -> f64 {
        match self {
            ScaleUnit::Day => 1.0 / 365.0,
            ScaleUnit::Month => 1.0 / 12.0,
            ScaleUnit::Year => 1.0,
        }
    }
}

/// A tab stop for a text label.
#[derive(Clone, Copy, Debug)]
struct TabStop {
    /// The alignment of the text relative to its origin.
    alignment: Alignment,
    /// The offset of the tab stop on the inline axis relative to the text
    /// origin.
    displacement: f64,
}

/// A text label.
#[derive(Clone, Debug)]
struct Text<'input> {
    /// Label URL.
    link: Option<Url<'input>>,
    /// Drawing properties.
    pen: parser::TextPen<'input>,
    /// Text.
    spans: Vec<TextSpan<'input>>,
}

/// A span of text within a text label.
#[derive(Clone, Copy, Debug)]
enum TextSpan<'input> {
    /// An external link.
    ExternalLink {
        /// The target URL.
        target: Url<'input>,
        /// The text.
        text: &'input str,
    },
    /// An internal link.
    Link {
        /// The target URL.
        target: Url<'input>,
        /// The text.
        text: &'input str,
    },
    /// Plain text.
    Text(&'input str),
}

impl<'input> TextSpan<'input> {
    /// Gets the text content and optional URL suitable for use in an `href`
    /// attribute.
    fn into_content_link(
        self,
        fallback: Option<&'input str>,
        base_uri: &Uri,
    ) -> (&'input str, Option<String>) {
        let (text, link) = match self {
            TextSpan::ExternalLink { target, text } => {
                (text, Some(LinkKind::External(target.into())))
            }
            TextSpan::Link { target, text } => {
                (text, Some(LinkKind::Internal(Title::new(target, None))))
            }
            TextSpan::Text(text) => (
                text,
                fallback.map(|target| LinkKind::External(target.into())),
            ),
        };
        (text, link.map(|link| link.to_string(base_uri)))
    }
}

/// A time.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Time {
    /// Absolute time in `year.fract`.
    Decimal(f64),
    /// The [end](Period::till) of a time [period](Period).
    End,
    /// The [start](Period::from) of a time [period](Period).
    Start,
}

impl core::ops::Sub for Time {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Time::Decimal(lhs), Time::Decimal(rhs)) => Time::Decimal(lhs - rhs),
            _ => panic!("incompatible time values"),
        }
    }
}

impl core::ops::Div for Time {
    type Output = f64;

    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Time::Decimal(lhs), Time::Decimal(rhs)) => lhs / rhs,
            _ => panic!("incompatible time values"),
        }
    }
}

/// Time axis configuration.
#[derive(Clone, Copy, Debug, Default)]
struct TimeAxis {
    /// The expected date format for scale dates.
    format: DateFormat,
    /// Time flow direction.
    order: Order,
    /// The orientation.
    orientation: Orientation,
}

impl TimeAxis {
    /// Whether the visual direction of time should be down or to the right.
    fn reverse_order(self) -> bool {
        if self.orientation == Orientation::Horizontal {
            self.order == Order::Reverse
        } else {
            self.order == Order::Normal
        }
    }
}

/// A position.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    /// An absolute position, in CSS pixels.
    Absolute(f64),
    /// A position relative to some container.
    Relative(f64),
}

impl Unit {
    /// Gets the value as an absolute position in CSS pixels, using `len` as the
    /// container size for converting relative positions.
    fn into_abs(self, len: f64) -> f64 {
        match self {
            Unit::Absolute(value) => value,
            Unit::Relative(value) => value * len,
        }
    }
}

impl Default for Unit {
    fn default() -> Self {
        Self::Absolute(0.0)
    }
}

impl From<f64> for Unit {
    fn from(value: f64) -> Self {
        Self::Absolute(value)
    }
}

/// The list of predefined colours and their corresponding CSS values.
static PREDEFINED_COLORS: phf::Map<&str, &str> = phf::phf_map! {
    "black"        => "#000",
    "white"        => "#fff",
    "tan1"         => "#e5d3c9",
    "tan2"         => "#b29999",
    "claret"       => "#b24c4c",
    "red"          => "#f00",
    "magenta"      => "#ff4c7f",
    "coral"        => "#f99",
    "pink"         => "#fcc",
    "redorange"    => "#ff7f00",
    "orange"       => "#ff9e23",
    "lightorange"  => "#fc9",
    "yelloworange" => "#ffd800",
    "yellow"       => "#ff0",
    "yellow2"      => "#eaea00",
    "dullyellow"   => "#ffe599",
    "teal"         => "#007f33",
    "kelleygreen"  => "#4c994c",
    "green"        => "#00b200",
    "brightgreen"  => "#0f0",
    "drabgreen"    => "#9c9",
    "yellowgreen"  => "#99e599",
    "limegreen"    => "#ccffb2",
    "darkblue"     => "#009",
    "brightblue"   => "#00f",
    "blue"         => "#06c",
    "oceanblue"    => "#007fcc",
    "powderblue"   => "#99f",
    "powderblue2"  => "#b2b2ff",
    "skyblue"      => "#b2ccff",
    "purple"       => "#707",
    "lightpurple"  => "#aa4caa",
    "lavender"     => "#ccb2cc",
};
