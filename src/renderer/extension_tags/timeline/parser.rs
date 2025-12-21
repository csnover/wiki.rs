//! Timeline command parser.
//!
//! Because the positions of bars are calculated relative to the plot area, but
//! there is no restriction on the order in which commands can be received, it
//! is necessary to buffer commands into an intermediate representation which is
//! only rendered once the whole thing has been read out since otherwise it is
//! impossible to calculate positioning information.

// Clippy: There are a lot of imports, and little value in listing them
// explicitly.
#[allow(clippy::wildcard_imports)]
use super::*;
use core::ops::Range;
use std::collections::{BTreeMap, HashMap};

/// Expands variables in the input text.
///
/// Because variable replacement is arbitrary text in any position, it is
/// simpler to do this separately than it is to try to try to make the grammar
/// support it.
pub(super) fn expand(input: &str) -> Result<(String, Vec<(usize, isize)>)> {
    let mut expanded = String::new();
    let mut defines = Defines::new();
    let mut deltas = Vec::new();
    let mut cursor = 0;
    while cursor != input.len() {
        let len = match grammar::easy_timeline::chunk(&input[cursor..]).map_err(|mut err| {
            err.location = peg::Parse::position_repr(input, cursor + err.location.offset);
            err
        })? {
            grammar::Chunk::Command(len) => {
                replace_idents(
                    &mut expanded,
                    &mut deltas,
                    &defines,
                    &input[cursor..cursor + len],
                );
                len
            }
            grammar::Chunk::Define(len, Define { key, value }) => {
                defines.insert(key.to_ascii_lowercase(), value);
                deltas.push((expanded.len(), -isize::try_from(len).unwrap()));
                len
            }
        };
        cursor += len;
    }
    Ok((expanded, deltas))
}

/// A collection of defined variables.
type Defines<'input> = HashMap<String, &'input str>;

/// A defined variable.
#[derive(Debug)]
pub(super) struct Define<'input> {
    /// The name of the variable, excluding the `$`.
    pub key: &'input str,
    /// The value of the variable.
    pub value: &'input str,
}

/// Parses an expanded input text into an intermediate representation.
pub(super) fn parse<'a>(
    input: &'a str,
    original: &str,
    deltas: &[(usize, isize)],
) -> Result<Timeline<'a>> {
    let mut pen = Timeline::new();
    let mut cursor = 0;
    while cursor != input.len() {
        let (len, command) = grammar::easy_timeline::timeline(&input[cursor..], pen.date_format)
            .map_err(|mut err| {
                let mut offset = cursor + err.location.offset;
                for delta in deltas
                    .iter()
                    .map_while(move |&(at, delta)| (at < offset).then_some(delta))
                {
                    offset = offset.strict_sub_signed(delta);
                }
                err.location = peg::Parse::position_repr(original, offset);
                err
            })?;
        // eprintln!("command: {command:#?}");
        pen.update_state(command)?;
        cursor += len;
    }

    Ok(pen)
}

/// A timeline series identifier.
pub(super) type BarId<'input> = &'input str;
/// A collection of timeline series.
///
/// This is a `BTreeMap` simply because `HashMap` has randomness which causes
/// iteration order to be inconsistent, which causes outputs to be different,
/// which breaks tests.
type Bars<'input> = BTreeMap<String, Bar<'input>>;
/// A map from timeline series set ID to index range.
type BarSets = HashMap<String, Range<usize>>;
/// A collection of colours.
type Colors<'input> = HashMap<String, ColorValue>;
/// A collection of legend entries.
type Legends<'input> = Vec<(ColorValue, Vec<TextSpan<'input>>)>;

/// A timeline series set identifier.
#[derive(Clone, Copy, Debug)]
pub(super) enum BarsetId<'input> {
    /// Identifier.
    Id(&'input str),
    /// Reset counter.
    Reset,
    /// Advance counter.
    Skip,
}

/// An intermediate representation (IR) of a timeline.
#[derive(Debug)]
pub(super) struct Timeline<'input> {
    /// Cross-axis alignment.
    pub align_bars: AlignBars,
    /// Colour for unfilled bar sections.
    pub bar_color: Option<ColorId<'input>>,
    /// The total number of bars in the timeline, including implicit bars
    /// created by bar sets.
    pub bar_count: usize,
    /// Accumulated timeline segments.
    pub bar_layer: Vec<Plot<'input>>,
    /// Timeline series sets.
    pub bar_sets: BarSets,
    /// Timeline series…es.
    pub bars: Bars<'input>,
    /// Canvas colour.
    pub canvas_color: Option<ColorId<'input>>,
    /// User-defined colours.
    pub colors: Colors<'input>,
    /// Expected date format.
    pub date_format: DateFormat,
    /// Image dimensions.
    pub image_size: Option<ImageSize>,
    /// Legend properties.
    pub legend: Legend,
    /// Legend series…es.
    pub legends: Legends<'input>,
    /// Background lines.
    pub line_layer_back: Vec<Line<'input>>,
    /// Foreground lines.
    pub line_layer_front: Vec<Line<'input>>,
    /// Line drawing pen.
    pub line_pen: LinePen<'input>,
    /// Plot area.
    pub plot_area: PlotArea,
    /// Timeline segment pen.
    pub plot_pen: PlotPen<'input>,
    /// Time range.
    pub period: Period,
    /// Major scale properties.
    pub scale_major: Scale<'input>,
    /// Minor scale properties.
    pub scale_minor: Scale<'input>,
    /// Free-text labels.
    pub text_layer: Vec<Text<'input>>,
    /// Text drawing pen.
    pub text_pen: TextPen<'input>,
    /// Time axis properties.
    pub time_axis: TimeAxis,
}

impl Timeline<'_> {
    /// Creates a new parser.
    fn new() -> Self {
        Self {
            align_bars: <_>::default(),
            bar_color: None,
            bar_count: 0,
            bar_layer: vec![],
            bars: <_>::default(),
            bar_sets: <_>::default(),
            canvas_color: None,
            colors: Colors::from([("barcoldefault".into(), ColorValue::Rgb(0.0, 0.6, 0.0))]),
            date_format: DateFormat::Decimal,
            line_layer_back: vec![],
            line_layer_front: vec![],
            image_size: None,
            legend: <_>::default(),
            legends: <_>::default(),
            line_pen: <_>::default(),
            period: Period {
                from: Time::Decimal(0.0),
                till: Time::Decimal(0.0),
            },
            plot_area: <_>::default(),
            plot_pen: <_>::default(),
            scale_major: <_>::default(),
            scale_minor: <_>::default(),
            text_layer: vec![],
            text_pen: <_>::default(),
            time_axis: <_>::default(),
        }
    }
}

impl<'input> Timeline<'input> {
    /// Adds new time series…es.
    fn update_bar_data(&mut self, bars: Vec<BarData<'input>>) {
        let mut index = 0;
        for bar in bars {
            let key = bar.key();
            if let BarData::Bar { link, text, .. } = bar {
                self.bars.insert(
                    key,
                    Bar {
                        index,
                        label: text.unwrap_or_default(),
                        link,
                    },
                );
                index += 1;
            } else {
                self.bar_sets.insert(key, index..index);
            }
        }
    }

    /// Adds new colour definitions.
    fn update_colors(&mut self, colors: Vec<Color<'input>>) {
        for color in colors {
            let id = color.id.to_ascii_lowercase();
            if let Some(legend) = color.legend {
                self.legends.push((color.value, legend));
            }
            self.colors.insert(id, color.value);
        }
    }

    /// Adds new line drawings.
    fn update_line_data(&mut self, lines: Vec<LineData<'input>>) {
        for line in lines {
            if let Some(instr) = line.instr {
                let mut line_pen = self.line_pen;
                line_pen.update(&line);
                let layer = match line_pen.layer {
                    Layer::Back => &mut self.line_layer_back,
                    Layer::Front => &mut self.line_layer_front,
                };
                layer.push(Line {
                    color: line_pen.color,
                    instr,
                    width: line_pen.width,
                });
            } else {
                self.line_pen.update(&line);
            }
        }
    }

    /// Adds new timeline segments.
    fn update_plot_data(&mut self, data: Vec<PlotData<'input>>) -> Result {
        if self.bars.is_empty() {
            let mut index = 0;
            for plot in &data {
                match &plot.bar {
                    Some(PlotDataTarget::Bar(id)) => {
                        self.bars.insert(
                            id.to_ascii_lowercase(),
                            Bar {
                                index,
                                label: vec![TextSpan::Text(id)],
                                link: None,
                            },
                        );
                        index += 1;
                    }
                    Some(PlotDataTarget::BarSet(_)) => {
                        return Err(Error::ImplicitBarset);
                    }
                    None => {}
                }
            }
        }

        let mut bar_set = None;
        let mut index = 0;
        for plot in data {
            match plot.bar {
                Some(PlotDataTarget::Bar(id)) => {
                    index = self.bars.get(&id.to_ascii_lowercase()).unwrap().index;
                    bar_set = None;
                }
                Some(PlotDataTarget::BarSet(BarsetId::Id(id))) => {
                    let range = self
                        .bar_sets
                        .get(&id.to_ascii_lowercase())
                        .cloned()
                        .unwrap();
                    index = range.start;
                    bar_set = Some(range);
                }
                Some(PlotDataTarget::BarSet(BarsetId::Skip)) => {
                    if bar_set.is_some() {
                        self.reindex(&mut bar_set, index);
                        index += 1;
                    } else {
                        return Err(Error::ImplicitBarset);
                    }
                }
                Some(PlotDataTarget::BarSet(BarsetId::Reset)) => {
                    if let Some(bar_set) = &bar_set {
                        index = bar_set.start;
                    } else {
                        return Err(Error::ImplicitBarset);
                    }
                }
                None => {}
            }

            if let Some(at) = plot.at {
                self.reindex(&mut bar_set, index);

                let mut plot_pen = self.plot_pen;
                plot_pen.update(&plot);

                self.bar_layer.push(Plot {
                    at,
                    index,
                    pen: plot_pen,
                    text: plot.text,
                    link: plot.link,
                });

                self.bar_count = self.bar_count.max(index + 1);
                if bar_set.is_some() {
                    index += 1;
                }
            } else {
                self.plot_pen.update(&plot);
            }
        }

        Ok(())
    }

    /// Reindexes all plots, bars, and bar sets at or after the given index.
    ///
    /// Bar set is a nightmare feature. When it is used, plot data
    /// lines create new *implicit* bars which need to push down the
    /// indices of all the other bars as the set grows.
    fn reindex(&mut self, bar_set: &mut Option<Range<usize>>, index: usize) {
        if let Some(bar_set) = bar_set
            && index >= bar_set.end
        {
            for bar in self.bars.values_mut() {
                if bar.index >= index {
                    bar.index += 1;
                }
            }

            for set in self.bar_sets.values_mut() {
                if set.start == bar_set.start {
                    set.end += 1;
                } else if set.start >= index {
                    set.start += 1;
                    set.end += 1;
                }
            }

            for plot in &mut self.bar_layer {
                if plot.index >= index {
                    plot.index += 1;
                }
            }

            bar_set.end += 1;
        }
    }

    /// Updates the parser with preset settings.
    fn update_preset(&mut self, preset: Preset) {
        match preset {
            Preset::TimeHorizontal => {
                let width = match self.image_size {
                    Some(ImageSize::AutoHeight { width, .. } | ImageSize::Fixed { width, .. }) => {
                        width
                    }
                    None | Some(ImageSize::AutoWidth { .. }) => 0.0,
                };
                self.image_size = Some(ImageSize::AutoHeight { bar: 20.0, width });
                self.plot_area = PlotArea {
                    left: Some(25.0.into()),
                    top: Some(PlotAreaEnd::Inset(15.0.into())),
                    right: Some(PlotAreaEnd::Inset(25.0.into())),
                    bottom: Some(30.0.into()),
                };
                self.time_axis.format = DateFormat::Year;
                self.time_axis.orientation = Orientation::Horizontal;
                self.colors
                    .insert("canvas".into(), ColorValue::Rgb(0.7, 0.7, 0.7));
                self.colors
                    .insert("grid1".into(), ColorValue::Rgb(0.4, 0.4, 0.4));
                self.colors
                    .insert("grid2".into(), ColorValue::Rgb(0.2, 0.2, 0.2));
                self.canvas_color = Some("canvas");
                self.date_format = DateFormat::Year;
                self.align_bars = AlignBars::Justify;
                self.scale_major.unit = ScaleUnit::Year;
                self.scale_major.grid_color = Some("grid1");
                self.scale_minor.unit = ScaleUnit::Year;
                self.legend.orientation = Orientation::Vertical;
                self.legend.left = Some(35.0.into());
                self.legend.top = Some(130.0.into());
                self.plot_pen.align = Alignment::Start;
                self.plot_pen.anchor = Alignment::Start;
                self.plot_pen.font_size = FontSize::Medium;
                self.plot_pen.width = 15.0.into();
                self.plot_pen.shift = Point(4.0, -6.0);
                self.plot_pen.text_color = "black";
            }
            Preset::TimeVertical => {
                self.plot_area = PlotArea {
                    left: Some(45.0.into()),
                    top: Some(PlotAreaEnd::Inset(10.0.into())),
                    right: Some(PlotAreaEnd::Inset(10.0.into())),
                    bottom: Some(10.0.into()),
                };
                self.time_axis.format = DateFormat::Year;
                self.time_axis.orientation = Orientation::Vertical;
                self.date_format = DateFormat::Year;
                self.align_bars = AlignBars::Early;
                self.scale_major.unit = ScaleUnit::Year;
                self.scale_minor.unit = ScaleUnit::Year;
                self.plot_pen.mark = Some("white");
                self.plot_pen.align = Alignment::Start;
                self.plot_pen.font_size = FontSize::Small;
                self.plot_pen.width = 20.0.into();
                self.plot_pen.shift = Point(20.0, 0.0);
            }
        }
    }

    /// Updates the parser state from a command.
    fn update_state(&mut self, command: Command<'input>) -> Result {
        match command {
            Command::AlignBars(align_bars) => self.align_bars = align_bars,
            Command::BackgroundColors(colors) => {
                replace(&mut self.bar_color, colors.bars);
                replace(&mut self.canvas_color, colors.canvas);
            }
            Command::BarData(bars) => {
                self.update_bar_data(bars);
            }
            Command::Colors(colors) => {
                self.update_colors(colors);
            }
            Command::DateFormat(date_format) => self.date_format = date_format,
            Command::Legend(legend) => self.legend = legend,
            Command::LineData(lines) => {
                self.update_line_data(lines);
            }
            Command::ImageSize(image_size) => self.image_size = Some(image_size),
            Command::Period(period) => self.period = period,
            Command::PlotArea(plot_area) => {
                replace(&mut self.plot_area.left, plot_area.left);
                replace(&mut self.plot_area.top, plot_area.top);
                replace(&mut self.plot_area.right, plot_area.right);
                replace(&mut self.plot_area.bottom, plot_area.bottom);
            }
            Command::PlotData(data) => {
                self.update_plot_data(data)?;
            }
            Command::Preset(preset) => self.update_preset(preset),
            Command::ScaleMajor(scale) => {
                self.scale_major = scale;
            }
            Command::ScaleMinor(scale) => {
                self.scale_minor = scale;
            }
            Command::TextData(data) => {
                self.update_text_data(data);
            }
            Command::TimeAxis(time_axis) => self.time_axis = time_axis,
        }

        Ok(())
    }

    /// Adds text labels.
    fn update_text_data(&mut self, data: Vec<TextData<'input>>) {
        for mut line in data {
            // It seems like this should only advance when there
            // is actually text, but no, it does it *every* line, and
            // 'Template:Wikimedia Growth' actually relies on this
            // behaviour.
            if line.pos.is_none() {
                let advance = line
                    .line_height
                    .or(self.text_pen.line_height)
                    .unwrap_or_else(|| {
                        line.font_size
                            .unwrap_or(self.text_pen.font_size)
                            .line_height()
                    });
                let y = self.text_pen.pos.y_mut();
                *y = (*y - advance).max(0.0);
            }

            if let Some(text) = line.text.take() {
                let mut text_pen = self.text_pen.clone();
                text_pen.update(&line);
                self.text_layer.push(Text {
                    link: line.link,
                    pen: text_pen,
                    spans: text,
                });
            } else {
                self.text_pen.update(&line);
            }
        }
    }
}

/// IR background colour definitions.
#[derive(Debug)]
pub(super) struct BackgroundColors<'input> {
    /// Unfilled timeline series background.
    pub bars: Option<ColorId<'input>>,
    /// Canvas background.
    pub canvas: Option<ColorId<'input>>,
}

/// IR time series definition.
#[derive(Debug)]
pub(super) enum BarData<'input> {
    /// A time series definition.
    Bar {
        /// The time series identifier.
        id: BarId<'input>,
        /// The link URL for the time series label.
        link: Option<Url<'input>>,
        /// The time series label.
        text: Option<Vec<TextSpan<'input>>>,
    },
    /// A time series set definition.
    BarSet(BarId<'input>),
}

impl BarData<'_> {
    /// Gets the map key for the bar data.
    fn key(&self) -> String {
        match self {
            Self::Bar { id, .. } | Self::BarSet(id) => id.to_ascii_lowercase(),
        }
    }
}

/// IR colour.
#[derive(Debug)]
pub(super) struct Color<'input> {
    /// The colour identifier.
    pub id: &'input str,
    /// Associated text for the colour in the legend.
    pub legend: Option<Vec<TextSpan<'input>>>,
    /// The colour value.
    pub value: ColorValue,
}

/// A parser command.
#[derive(Debug)]
pub(super) enum Command<'input> {
    /// `AlignBars`.
    AlignBars(AlignBars),
    /// `BackgroundColors`.
    BackgroundColors(BackgroundColors<'input>),
    /// `BarData`.
    BarData(Vec<BarData<'input>>),
    /// `Colors`.
    Colors(Vec<Color<'input>>),
    /// `DateFormat`.
    DateFormat(DateFormat),
    /// `Legend`.
    Legend(Legend),
    /// `LineData`.
    LineData(Vec<LineData<'input>>),
    /// `ImageSize`.
    ImageSize(ImageSize),
    /// `Period`.
    Period(Period),
    /// `PlotArea`.
    PlotArea(PlotArea),
    /// `PlotData`.
    PlotData(Vec<PlotData<'input>>),
    /// `Preset`.
    Preset(Preset),
    /// `ScaleMajor`.
    ScaleMajor(Scale<'input>),
    /// `ScaleMinor`.
    ScaleMinor(Scale<'input>),
    /// `TextData`.
    TextData(Vec<TextData<'input>>),
    /// `TimeAxis`.
    TimeAxis(TimeAxis),
}

/// IR line rendering layer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Layer {
    /// Draw behind the time series.
    #[default]
    Back,
    /// Draw above the time series.
    Front,
}

/// IR line drawing.
#[derive(Debug)]
pub(super) struct LineData<'input> {
    /// The colour of the line.
    pub color: Option<ColorId<'input>>,
    /// The draw instruction.
    pub instr: Option<LineDataInstr>,
    /// The target layer.
    pub layer: Option<Layer>,
    /// The stroke width.
    pub width: Option<f64>,
}

// TODO: It seems like lines use colours immediately but everything else does
// not?
/// A line drawing pen.
#[derive(Clone, Copy, Debug)]
pub(super) struct LinePen<'input> {
    /// Stroke colour.
    pub color: ColorId<'input>,
    /// Layer.
    pub layer: Layer,
    /// Stroke width.
    pub width: f64,
}

impl<'input> LinePen<'input> {
    /// Updates a line drawing pen from the given line data.
    fn update(&mut self, line: &LineData<'input>) {
        replace(&mut self.color, line.color);
        replace(&mut self.layer, line.layer);
        replace(&mut self.width, line.width);
    }
}

impl Default for LinePen<'_> {
    fn default() -> Self {
        Self {
            color: <_>::default(),
            layer: <_>::default(),
            width: 1.0,
        }
    }
}

/// IR timeline segment.
#[derive(Debug)]
pub(super) struct PlotData<'input> {
    /// Inline axis text label alignment.
    pub align: Option<Alignment>,
    /// Text label position relative to a timeline segment.
    pub anchor: Option<Alignment>,
    /// Instantaneous position of the segment.
    pub at: Option<PlotDataPos>,
    /// Associated series.
    pub bar: Option<PlotDataTarget<'input>>,
    /// Text label colour.
    pub color: Option<&'input str>,
    /// Text label font size.
    pub font_size: Option<FontSize>,
    /// Text label link URL.
    pub link: Option<&'input str>,
    /// Mark colour.
    pub mark: Option<&'input str>,
    /// Text label position offset.
    pub shift: Option<Point>,
    /// Text label colour.
    pub text_color: Option<&'input str>,
    /// Label text.
    pub text: Option<Vec<TextSpan<'input>>>,
    /// Cross-axis size.
    pub width: Option<Unit>,
}

/// Timeline segment identifier.
#[derive(Clone, Copy, Debug)]
pub(super) enum PlotDataTarget<'input> {
    /// Timeline series identifier.
    Bar(BarId<'input>),
    /// Bar set identifier.
    BarSet(BarsetId<'input>),
}

/// A timeline segment pen.
#[derive(Clone, Copy, Debug)]
pub(super) struct PlotPen<'input> {
    /// Inline axis text label alignment.
    pub align: Alignment,
    /// Text label position relative to a timeline segment.
    ///
    /// ```text
    ///     ⇒ time direction
    ///     ━━━━━     ▐ ← End
    ///     ↑ ↑ ↑     ▐ ← Middle
    /// Start │ End   ▐ ← Start
    ///    Middle     ⇑ time direction
    ///
    ///     ⇐ time direction
    ///     ━━━━━     ▐ ← Start
    ///     ↑ ↑ ↑     ▐ ← Middle
    ///   End │ Start ▐ ← End
    ///    Middle     ⇓ time direction
    /// ```
    pub anchor: Alignment,
    /// Timeline series.
    pub bar: Option<PlotDataTarget<'input>>,
    /// Fill colour.
    pub color: ColorId<'input>,
    /// Text label font size.
    pub font_size: FontSize,
    /// Mark colour.
    pub mark: Option<ColorId<'input>>,
    /// Text label position adjustment.
    pub shift: Point,
    /// Text label colour.
    pub text_color: ColorId<'input>,
    /// Cross-axis size.
    pub width: Unit,
}

impl<'input> PlotPen<'input> {
    /// Updates a timeline segment drawing pen from the given timeline segment
    /// data.
    fn update(&mut self, plot: &PlotData<'input>) {
        replace(&mut self.align, plot.align);
        replace(&mut self.anchor, plot.anchor);
        replace(&mut self.bar, plot.bar);
        replace(&mut self.color, plot.color);
        replace(&mut self.font_size, plot.font_size);
        replace(&mut self.mark, plot.mark);
        replace(&mut self.shift, plot.shift);
        replace(&mut self.text_color, plot.text_color);
        replace(&mut self.width, plot.width);
    }
}

impl Default for PlotPen<'_> {
    fn default() -> Self {
        Self {
            align: Alignment::Start,
            anchor: Alignment::Middle,
            bar: None,
            color: "barcoldefault",
            font_size: <_>::default(),
            mark: None,
            shift: <_>::default(),
            text_color: "black",
            width: 25.0.into(),
        }
    }
}

/// A timeline preset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Preset {
    /// Defaults suitable for basic horizontal timelines.
    TimeHorizontal,
    /// Defaults suitable for basic vertical timelines.
    TimeVertical,
}

/// IR text label.
#[derive(Debug)]
pub(super) struct TextData<'input> {
    /// The font size.
    pub font_size: Option<FontSize>,
    /// The line height.
    pub line_height: Option<f64>,
    /// The link URL.
    pub link: Option<&'input str>,
    /// The position relative to the bottom-left corner of the image.
    pub pos: Option<Point>,
    /// Tab stops.
    pub tabs: Option<Vec<TabStop>>,
    /// The text.
    pub text: Option<Vec<TextSpan<'input>>>,
    /// Text colour.
    pub text_color: Option<&'input str>,
}

/// A text drawing pen.
#[derive(Clone, Debug)]
pub(super) struct TextPen<'input> {
    /// Font size.
    pub font_size: FontSize,
    /// Line height.
    pub line_height: Option<f64>,
    /// Position, relative to the bottom-left of the image.
    pub pos: Point,
    /// Tab stops.
    pub tabs: Vec<TabStop>,
    /// Colour.
    pub text_color: ColorId<'input>,
}

impl<'input> TextPen<'input> {
    /// Updates a text drawing pen from the given text data.
    fn update(&mut self, line: &TextData<'input>) {
        replace(&mut self.font_size, line.font_size);
        // The title position of 'Template:Wikimedia Growth' relies on this
        // specific (broken) initialisation behaviour
        if line.line_height.is_some() {
            self.line_height = line.line_height;
        } else if self.line_height.is_none() {
            self.line_height = Some(line.font_size.unwrap_or(self.font_size).line_height());
        }
        replace(&mut self.pos, line.pos);
        if let Some(tabs) = &line.tabs {
            self.tabs.clone_from(tabs);
        }
        replace(&mut self.text_color, line.text_color);
    }
}

impl Default for TextPen<'_> {
    fn default() -> Self {
        Self {
            font_size: <_>::default(),
            line_height: None,
            pos: <_>::default(),
            tabs: <_>::default(),
            text_color: "black",
        }
    }
}

/// Replaces the value in `to` if `from.is_some()`.
#[inline]
fn replace<T, O>(to: &mut O, from: Option<T>)
where
    O: From<T>,
{
    if let Some(from) = from {
        *to = O::from(from);
    }
}

/// Replaces any variables in the given `input` with corresponding values from
/// `defines` and emits the replaced input text to `out`.
fn replace_idents(
    out: &mut String,
    deltas: &mut Vec<(usize, isize)>,
    defines: &Defines<'_>,
    input: &str,
) {
    let mut flushed = 0;
    for at in memchr::memchr_iter(b'$', input.as_bytes()) {
        let start = at + "$".len();
        let end = input[start..]
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map_or(input.len(), |p| p + start);

        if start != end {
            let key = &input[start..end];
            // Because this code runs on the raw unprocessed text, it will fail
            // to find a replacement even if all use sites of a define are
            // actually commented out. To handle this, just emit the idents
            // as-is and let the next stage fail to parse.
            *out += &input[flushed..at];
            if let Some(value) = defines.get(&key.to_lowercase()) {
                let old_len = key.len() + "$".len();
                let new_len = value.len();
                deltas.push((out.len(), new_len.checked_signed_diff(old_len).unwrap()));
                *out += value;
                flushed = end;
            } else {
                flushed = at;
            }
        }
    }

    *out += &input[flushed..];
}
