//! SVG renderer for EasyTimeline.
//!
//! Coordinates in timeline scripts are in quadrant I, but somehow
//! SVG/CSS still has no unit for the font descent, so there is no way to
//! set a correct transform origin to mirror text over its baseline, so
//! it is not possible to set a global `scaleY(-1)` and then undo that
//! transformation and still have text render in the correct place.
//! Instead, it is necessary to flip the y-coordinates everywhere in the
//! renderer. :-(

// Clippy: Values are guaranteed to be in range.
#![allow(clippy::cast_precision_loss)]

use super::{
    AlignBars, Alignment, ColorId, ColorValue, Dims, Either, Error, FontSize, ImageSize,
    LegendPosition, Line, LineDataInstr, Orientation, PREDEFINED_COLORS, PlotDataPos, Rect, Result,
    ScaleUnit, TabStop, TextSpan, Uri, Url, parser::Timeline,
};
use core::ops::Deref;
use minidom::{Element, ElementBuilder};

/// Renders a timeline into an SVG element.
pub(super) fn render(pen: Timeline<'_>, base_uri: &Uri) -> Result<Element> {
    let mut renderer = Renderer::new(pen, base_uri)?;

    renderer
        .add_global_styles()?
        .add_back_bars()?
        .add_back_lines()?
        .add_front_bars()?
        .add_front_lines()?
        .add_axes()?
        .add_text()?
        .add_legend()?;

    renderer.finish()
}

/// An SVG renderer.
#[derive(Debug)]
struct Renderer<'input> {
    /// The base URI for links.
    base_uri: &'input Uri,
    /// The background drawing layer.
    bg_layer: Element,
    /// Fully resolved image dimensions.
    image_dims: Dims,
    /// The maximum cross-axis size of all time series.
    max_bar_width: f64,
    /// The middle drawing layer.
    mid_layer: Element,
    /// Fully resolved plot area.
    plot_area: Rect,
    /// Accumulator.
    svg: Element,
    /// The top drawing layer.
    top_layer: Element,
    /// The timeline data.
    timeline: Timeline<'input>,
}

impl<'input> Renderer<'input> {
    /// Creates a new renderer for the given `timeline`, using the given
    /// `base_uri` for generating URLs.
    fn new(timeline: Timeline<'input>, base_uri: &'input Uri) -> Result<Self> {
        let image_dims = match timeline.image_size {
            Some(ImageSize::AutoHeight { bar, width }) => Dims(
                width,
                bar * timeline.bar_count as f64,
                timeline.time_axis.orientation,
            ),
            Some(ImageSize::AutoWidth { bar, height }) => Dims(
                bar * timeline.bar_count as f64,
                height,
                timeline.time_axis.orientation,
            ),
            Some(ImageSize::Fixed { width, height }) => {
                Dims(width, height, timeline.time_axis.orientation)
            }
            None => return Err(Error::ImageSize),
        };

        let plot_area = if let (Some(left), Some(top), Some(right), Some(bottom)) = (
            timeline.plot_area.left,
            timeline.plot_area.top,
            timeline.plot_area.right,
            timeline.plot_area.bottom,
        ) {
            let left = left.into_abs(image_dims.width());
            let bottom = bottom.into_abs(image_dims.height());
            let width = right.into_len(left, image_dims.width());
            let height = top.into_len(bottom, image_dims.height());
            Rect::new(
                left,
                image_dims.height() - bottom - height,
                width,
                height,
                timeline.time_axis.orientation,
            )
        } else {
            return Err(Error::PlotArea);
        };

        let max_bar_width = timeline.bar_layer.iter().fold(0.0_f64, |width, plot| {
            width.max(plot.pen.width.into_abs(plot_area.cross_len()))
        });

        let svg = Element::builder("svg", NS_SVG)
            .attr(
                n!("viewBox"),
                format!(
                    "0 0 {} {}",
                    image_dims.width().v(3),
                    image_dims.height().v(3)
                ),
            )
            .attr(n!("width"), image_dims.width().v(3))
            .attr(n!("height"), image_dims.height().v(3))
            .attr(n!("class"), "wiki-rs-timeline")
            .build();

        Ok(Self {
            base_uri,
            bg_layer: Element::bare("g", NS_SVG),
            image_dims,
            max_bar_width,
            mid_layer: Element::bare("g", NS_SVG),
            plot_area,
            svg,
            top_layer: Element::bare("g", NS_SVG),
            timeline,
        })
    }

    /// Adds the axes to the SVG output.
    fn add_axes(&mut self) -> Result<&mut Self> {
        if self.bar_layer.is_empty() {
            return Ok(self);
        }

        draw_axes(self)?;
        Ok(self)
    }

    /// Adds background bars to the SVG output.
    fn add_back_bars(&mut self) -> Result<&mut Self> {
        let Some(bg) = self.bar_color else {
            return Ok(self);
        };

        if self.bar_count == 0 {
            return Ok(self);
        }

        let bg = self.color(bg)?;

        let mut bgs = Element::builder("g", NS_SVG).attr(n!("fill"), bg.v(3));

        for bar in self.bars.values() {
            let rect = self.bar_dims(
                bar.index,
                0.0,
                self.plot_area.main_len(),
                self.max_bar_width,
            );
            bgs = bgs.append(make_rect(rect, None));
        }

        self.mid_layer.append_child(bgs.build());
        Ok(self)
    }

    /// Adds background lines to the SVG output.
    fn add_back_lines(&mut self) -> Result<&mut Self> {
        if self.line_layer_back.is_empty() {
            return Ok(self);
        }

        let lines = draw_lines(self, &self.line_layer_back)?;
        self.mid_layer.append_child(lines);
        Ok(self)
    }

    /// Adds foreground lines to the SVG output.
    fn add_front_lines(&mut self) -> Result<&mut Self> {
        if self.line_layer_front.is_empty() {
            return Ok(self);
        }

        let lines = draw_lines(self, &self.line_layer_front)?;
        self.mid_layer.append_child(lines);
        Ok(self)
    }

    /// Adds the time series to the SVG output.
    fn add_front_bars(&mut self) -> Result<&mut Self> {
        if self.bar_layer.is_empty() {
            return Ok(self);
        }

        let plots = draw_plots(self)?;
        self.mid_layer.append_child(plots);
        Ok(self)
    }

    /// Adds global styles to the SVG output.
    fn add_global_styles(&mut self) -> Result<&mut Self> {
        // TODO: Should default to 'transparent'?
        let canvas_bg = self.color(self.canvas_color.unwrap_or("white"))?;
        let css = format!(
            ".wiki-rs-timeline{{background-color:{};text a{{fill:#00f}}line{{transform:translate(.5px,.5px)}}}}",
            canvas_bg.v(3)
        );
        let style = Element::builder("style", NS_SVG).append(css).build();
        self.svg.append_child(style);
        Ok(self)
    }

    /// Adds text data to the SVG output.
    fn add_text(&mut self) -> Result<&mut Self> {
        for text in &self.timeline.text_layer {
            let y = text.pen.pos.y(self.image_dims.height());
            let x = text.pen.pos.x();
            let color = self.color(text.pen.text_color)?;
            self.top_layer.append_child(make_text(&MakeText {
                x,
                y,
                text: &text.spans,
                link: text.link,
                font_size: text.pen.font_size,
                color,
                alignment: Alignment::Start,
                tabs: &text.pen.tabs,
                line_height: text.pen.line_height,
                base_uri: self.base_uri,
            })?);
        }
        Ok(self)
    }

    /// Adds the legend to the SVG output.
    fn add_legend(&mut self) -> Result<&mut Self> {
        if self.legends.is_empty() {
            return Ok(self);
        }

        draw_legend(self)?;
        Ok(self)
    }

    /// Creates a [`Rect`] for the time series at the given `index` with the
    /// given main-axis `start` and `len` and cross-axis `width`.
    fn bar_dims(&self, index: usize, plot_start: f64, len: f64, width: f64) -> Rect {
        let cross = self.calc_bar_midpoint(index);

        match self.plot_area.orientation {
            Orientation::Horizontal => Rect::new(
                self.plot_area.left() + plot_start,
                cross - width / 2.0,
                len,
                width,
                self.plot_area.orientation,
            ),
            Orientation::Vertical => Rect::new(
                self.plot_area.top() + plot_start,
                cross - width / 2.0,
                len,
                width,
                self.plot_area.orientation,
            ),
        }
    }

    /// Calculates the midpoint of the time series with the given `index` on the
    /// cross-axis relative to the top-left corner of the image.
    fn calc_bar_midpoint(&self, index: usize) -> f64 {
        let len = self.bar_count as f64;
        let norm_index = index as f64 / len;
        let cross_len = self.plot_area.cross_len();
        let half_max = self.max_bar_width / 2.0;
        self.plot_area.cross_start()
            + half_max
            + match self.align_bars {
                AlignBars::Early => norm_index * cross_len,
                AlignBars::Late => {
                    let bars_h = len * self.max_bar_width;
                    let space = (cross_len - bars_h) / len;
                    norm_index * cross_len + space
                }
                AlignBars::Justify => {
                    let bars_h = len * self.max_bar_width;
                    let space = if self.bar_count == 1 {
                        0.0
                    } else {
                        (cross_len - bars_h) / (len - 1.0)
                    };
                    norm_index * (cross_len + space)
                }
            }
    }

    /// Returns the resolved colour value for the colour with the given `id`.
    fn color(&self, id: ColorId<'_>) -> Result<ColorValue> {
        self.timeline
            .colors
            .get(&id.to_ascii_lowercase())
            .copied()
            .or_else(|| {
                PREDEFINED_COLORS
                    .get_key(id)
                    .copied()
                    .map(ColorValue::Named)
            })
            .ok_or_else(|| Error::Color(id.into()))
    }

    /// Finalises the SVG element and returns it, consuming the renderer.
    fn finish(mut self) -> Result<Element> {
        if self.bg_layer.nodes().next().is_some() {
            self.svg.append_child(self.bg_layer);
        }
        if self.mid_layer.nodes().next().is_some() {
            self.svg.append_child(self.mid_layer);
        }
        if self.top_layer.nodes().next().is_some() {
            self.svg.append_child(self.top_layer);
        }
        Ok(self.svg)
    }

    /// Gets the correct SVG alignment information for an axis label.
    fn label_align(&self, main_axis: bool) -> (&'static str, Alignment) {
        if (self.plot_area.orientation == Orientation::Vertical) ^ main_axis {
            ("text-before-edge", Alignment::Middle)
        } else {
            ("central", Alignment::End)
        }
    }

    /// Gets the correct position for an axis label.
    ///
    /// `Either::Left` is the main axis and `Either::Right` is the cross axis.
    fn label_pos(&self, len: Either<f64, f64>) -> (f64, f64) {
        match (self.plot_area.orientation, len) {
            (Orientation::Horizontal, Either::Left(x))
            | (Orientation::Vertical, Either::Right(x)) => {
                let offset = if len.is_left() { 5.0 } else { 2.0 };

                (x, self.plot_area.bottom() + offset)
            }
            (Orientation::Horizontal, Either::Right(y))
            | (Orientation::Vertical, Either::Left(y)) => (self.plot_area.left() - 10.0, y),
        }
    }
}

impl<'input> Deref for Renderer<'input> {
    type Target = Timeline<'input>;

    fn deref(&self) -> &Self::Target {
        &self.timeline
    }
}

/// Draws the timeline axes.
fn draw_axes(r: &mut Renderer<'_>) -> Result {
    let color = r.color("black")?;

    let has_labels = r.bar_count > 1;

    if r.plot_area.orientation == Orientation::Vertical || has_labels {
        let left_axis = make_line(
            r.plot_area.left(),
            r.plot_area.top(),
            r.plot_area.left(),
            r.plot_area.bottom(),
            color,
            1.0,
        );
        r.top_layer.append_child(left_axis);
    }

    if r.plot_area.orientation == Orientation::Horizontal || has_labels {
        let bottom_axis = make_line(
            r.plot_area.left(),
            r.plot_area.bottom(),
            r.plot_area.right(),
            r.plot_area.bottom(),
            color,
            1.0,
        );
        r.top_layer.append_child(bottom_axis);
    }

    draw_scale(r, color, false)?;
    draw_scale(r, color, true)?;

    if !has_labels || r.bars.values().all(|bar| bar.label.is_empty()) {
        return Ok(());
    }

    let font_size = FontSize::Small;
    let (baseline, alignment) = r.label_align(false);

    let mut labels = Element::builder("g", NS_SVG)
        .attr(n!("dominant-baseline"), baseline)
        .attr(n!("class"), "wiki-rs-timeline-cross-axis")
        .build();

    for bar in r.bars.values() {
        let (x, y) = r.label_pos(Either::Right(r.calc_bar_midpoint(bar.index)));

        labels.append_child(make_text(&MakeText {
            text: &bar.label,
            x,
            y,
            link: bar.link,
            font_size,
            color,
            alignment,
            tabs: &[],
            line_height: None,
            base_uri: r.base_uri,
        })?);
    }

    r.top_layer.append_child(labels);

    Ok(())
}

/// Draws the timeline legend.
fn draw_legend(r: &mut Renderer<'_>) -> Result<()> {
    const SWATCH_SIZE: f64 = 12.0;
    const MARGIN: f64 = SWATCH_SIZE + 8.0;

    let font_size = FontSize::Small;
    let line_height = font_size.line_height();

    let columns = if let Some(columns) = r.legend.columns {
        usize::from(columns)
    } else if r.legend.orientation == Orientation::Horizontal
        || r.legend.position == LegendPosition::Right
        || r.legends.len() <= 5
    {
        1
    } else if r.legends.len() <= 10 {
        2
    } else {
        3
    };

    let x = r.legend.left.map_or_else(
        || {
            if r.legend.position == LegendPosition::Right {
                r.plot_area.right() + MARGIN
            } else {
                0.0
            }
        },
        |left| left.into_abs(r.image_dims.width()),
    );

    let y = r.legend.top.map_or_else(
        || match r.legend.position {
            LegendPosition::Bottom => r.plot_area.bottom() + MARGIN * -2.0,
            LegendPosition::Top => r.image_dims.height() - MARGIN,
            LegendPosition::Right => r.plot_area.top() - MARGIN,
        },
        |top| r.image_dims.height() - top.into_abs(r.image_dims.height()),
    );

    let column_width = r.legend.column_width.map_or_else(
        || (r.plot_area.width() - MARGIN) / columns as f64,
        |w| w.into_abs(r.image_dims.width()),
    );
    let per_column = r.legends.len().div_ceil(columns);

    let mut dx = 0.0;
    let mut dy = 0.0;
    let mut index = 0;
    let text_color = r.color("black")?;
    for (color, text) in &r.timeline.legends {
        let swatch = Rect::new(
            x + dx,
            y + dy,
            SWATCH_SIZE,
            SWATCH_SIZE,
            Orientation::Horizontal,
        );

        r.top_layer.append_child(make_rect(swatch, Some(*color)));
        r.top_layer.append_child(make_text(&MakeText {
            x: x + dx + MARGIN,
            y: y + dy + /* TODO: No magic numbers */ 10.0,
            text,
            link: None,
            font_size,
            color: text_color,
            alignment: Alignment::Start,
            tabs: &[],
            line_height: Some(line_height),
            base_uri: r.base_uri,
        })?);

        index += 1;
        if index == per_column {
            dx += column_width;
            dy = 0.0;
            index = 0;
        } else {
            dy += line_height + 4.0;
        }
    }

    Ok(())
}

/// Draws the given lines and returns the result as an SVG group element.
fn draw_lines(r: &Renderer<'_>, lines: &[Line<'_>]) -> Result<Element> {
    let mut group = Element::builder("g", NS_SVG);
    let reverse = r.time_axis.reverse_order();
    for line in lines {
        let color = r.color(line.color)?;
        let (x1, y1, x2, y2) = match line.instr {
            LineDataInstr::Independent(p0, p1) => {
                let x1 = p0.x();
                let y1 = p0.y(r.image_dims.height());
                let x2 = p1.x();
                let y2 = p1.y(r.image_dims.height());
                (x1, y1, x2, y2)
            }
            LineDataInstr::Orthogonal { at, from, till } => {
                let main =
                    r.plot_area.main_start() + r.period.norm(at, reverse)? * r.plot_area.main_len();
                let cross1 = from.map_or(r.plot_area.cross_end(), |from| {
                    let from = from.into_abs(r.plot_area.cross_len());
                    match r.plot_area.orientation {
                        Orientation::Horizontal => r.image_dims.height() - from,
                        Orientation::Vertical => from,
                    }
                });
                let cross2 = till.map_or(r.plot_area.cross_start(), |till| {
                    let till = till.into_abs(r.plot_area.cross_len());
                    match r.plot_area.orientation {
                        Orientation::Horizontal => r.image_dims.height() - till,
                        Orientation::Vertical => till,
                    }
                });

                match r.plot_area.orientation {
                    Orientation::Horizontal => (main, cross1, main, cross2),
                    Orientation::Vertical => (cross1, main, cross2, main),
                }
            }
            LineDataInstr::Parallel { at, from, till } => {
                let cross = at.into_abs(r.plot_area.cross_len());
                let main1 = r.plot_area.main_start()
                    + from.map_or(Ok(0.0), |from| r.period.norm(from, reverse))?
                        * r.plot_area.main_len();
                let main2 = r.plot_area.main_start()
                    + till.map_or(Ok(1.0), |till| r.period.norm(till, reverse))?
                        * r.plot_area.main_len();

                match r.plot_area.orientation {
                    Orientation::Horizontal => {
                        let y = r.image_dims.height() - cross;
                        (main1, y, main2, y)
                    }
                    Orientation::Vertical => {
                        let x = r.plot_area.cross_start() + cross;
                        (x, main1, x, main2)
                    }
                }
            }
        };
        // The sequence for whole integers 1..=10 is 2, 3, 4, 5, 6, 7, 8, 8, 8, 8
        // This is probably an artefact of rounding in a small coordinate space
        // and using some modified Bresenham algorithm. The only care here is
        // approximating the correct visual appearance.
        let width = (line.width + 1.0).clamp(1.0, 8.0);
        group = group.append(make_line(x1, y1, x2, y2, color, width));
    }
    Ok(group.build())
}

/// Draws all time series segments.
fn draw_plots(r: &mut Renderer<'_>) -> Result<Element> {
    let mut group = Element::bare("g", NS_SVG);
    let mut marks = Vec::new();

    let reverse = r.time_axis.reverse_order();

    for plot in &r.timeline.bar_layer {
        let (start, end) = match plot.at {
            PlotDataPos::At(time) => {
                let at = r.period.norm(time, reverse)? * r.plot_area.main_len();
                (at, at)
            }
            PlotDataPos::Range(from, till) => {
                let start = r.period.norm(from, reverse)? * r.plot_area.main_len();
                let end = r.period.norm(till, reverse)? * r.plot_area.main_len();
                (start.min(end), end.max(start))
            }
        };

        let width = plot.pen.width.into_abs(r.image_dims.cross_axis());

        if matches!(plot.at, PlotDataPos::Range(..)) {
            let color = r.color(plot.pen.color)?;
            let rect = r.bar_dims(plot.index, start, end - start, width);
            group.append_child(make_rect(rect, Some(color)));
        }

        if let Some(mark) = plot.pen.mark {
            let color = r.color(mark)?;
            let rect = r.bar_dims(plot.index, if reverse { start } else { end }, 1.0, width);
            marks.push(make_rect(rect, Some(color)));
        }

        if let Some(text) = &plot.text {
            let color = r.color(plot.pen.text_color)?;
            let main = r.plot_area.main_start()
                + match plot.pen.anchor {
                    Alignment::Start => start,
                    Alignment::Middle => start + (end - start) / 2.0,
                    Alignment::End => end,
                };
            let cross = r.calc_bar_midpoint(plot.index);
            let (x, y) = match r.plot_area.orientation {
                Orientation::Horizontal => (main, cross),
                Orientation::Vertical => (cross, main),
            };

            let mt = MakeText {
                x: x + plot.pen.shift.x(),
                y: y + plot.pen.shift.y_shift(),
                text,
                link: plot.link,
                font_size: plot.pen.font_size,
                color,
                alignment: plot.pen.align,
                tabs: &[],
                line_height: None,
                base_uri: r.base_uri,
            };

            r.top_layer.append_child(make_text(&mt)?);
        }
    }

    for mark in marks {
        group.append_child(mark);
    }

    Ok(group)
}

/// Draws a main axis scale.
fn draw_scale(r: &mut Renderer<'_>, color: ColorValue, is_major: bool) -> Result {
    let scale = if is_major {
        &r.scale_major
    } else {
        &r.scale_minor
    };

    let Some(start) = scale.start else {
        return Ok(());
    };

    let grid_color = scale.grid_color.map(|color| r.color(color)).transpose()?;

    let interval = f64::from(scale.interval);
    // Scales seem just totally broken in EasyTimeline. The major scale only
    // ever emits tick marks on years, and if they are emitted starting at
    // non-integral positions, the labels are decimalised.
    let unit = if is_major {
        ScaleUnit::Year
    } else {
        scale.unit
    };

    let reverse = r.time_axis.reverse_order();
    let sign = if reverse { -1.0 } else { 1.0 };

    let d_at = interval * unit.into_f64();
    let units = r.period.units(unit);
    let px_per_interval = sign * interval * r.plot_area.main_len() / units;
    let mut main =
        r.period.norm(start, reverse)? * r.plot_area.main_len() + r.plot_area.main_start();
    let cross = if is_major { 5.0 } else { 2.5 };

    let (baseline, alignment) = r.label_align(true);
    let mut labels = Element::builder("g", NS_SVG)
        .attr(n!("dominant-baseline"), baseline)
        .attr(n!("class"), "wiki-rs-timeline-main-axis")
        .build();
    let mut at = r.period.get(start);

    while (0.0..=r.plot_area.main_end() + 1.0).contains(&main) {
        let (x1, x2, y1, y2) = match r.plot_area.orientation {
            Orientation::Horizontal => (
                main,
                main,
                r.plot_area.bottom(),
                r.plot_area.bottom() + cross,
            ),
            Orientation::Vertical => (r.plot_area.left(), r.plot_area.left() - cross, main, main),
        };

        r.top_layer
            .append_child(make_line(x1, y1, x2, y2, color, 1.0));

        if let Some(grid_color) = grid_color {
            let (x2, y2) = match r.plot_area.orientation {
                Orientation::Horizontal => (x2, r.plot_area.top()),
                Orientation::Vertical => (r.plot_area.right(), y2),
            };
            r.bg_layer
                .append_child(make_line(x1, y1, x2, y2, grid_color, 1.0));
        }

        if is_major {
            let (x, y) = r.label_pos(Either::Left(main));

            labels.append_child(make_text(&MakeText {
                x,
                y,
                text: &[TextSpan::Text(&at.v(2))],
                link: None,
                font_size: FontSize::Small,
                color,
                alignment,
                tabs: &[],
                line_height: None,
                base_uri: r.base_uri,
            })?);
            at += d_at;
        }

        main += px_per_interval;
    }

    if labels.nodes().next().is_some() {
        r.top_layer.append_child(labels);
    }

    Ok(())
}

/// Creates an SVG line with the given properties.
fn make_line(x1: f64, y1: f64, x2: f64, y2: f64, color: ColorValue, width: f64) -> Element {
    Element::builder("line", NS_SVG)
        .attr(n!("x1"), x1.v(3))
        .attr(n!("y1"), y1.v(3))
        .attr(n!("x2"), x2.v(3))
        .attr(n!("y2"), y2.v(3))
        .attr(n!("stroke"), color.v(3))
        .attr(n!("stroke-width"), width.v(3))
        .build()
}

/// Creates an SVG rect with the given dimensions and optional fill.
fn make_rect(rect: Rect, color: Option<ColorValue>) -> Element {
    let rect = Element::builder("rect", NS_SVG)
        .attr(n!("x"), rect.main_start().v(3))
        .attr(n!("y"), rect.cross_start().v(3))
        .attr(n!("width"), rect.main_len().v(3))
        .attr(n!("height"), rect.cross_len().v(3));
    if let Some(color) = color {
        rect.attr(n!("fill"), color.v(3))
    } else {
        rect
    }
    .build()
}

/// Text properties.
struct MakeText<'input> {
    /// The text alignment relative to the text origin.
    alignment: Alignment,
    /// The base URI for generating links.
    base_uri: &'input Uri,
    /// The text colour.
    color: ColorValue,
    /// The font size.
    font_size: FontSize,
    /// A line height override. Used only with multi-line text.
    line_height: Option<f64>,
    /// The link URL to use when rendering a [`TextSpan::Text`].
    link: Option<Url<'input>>,
    /// The tabs stops for the text run.
    tabs: &'input [TabStop],
    /// The text to render.
    text: &'input [TextSpan<'input>],
    /// Baseline origin x-coordinate, relative to the left edge of the image.
    x: f64,
    /// Baseline origin y-coordinate, relative to the top edge of the image.
    y: f64,
}

/// Makes a text element from a [`MakeText`].
fn make_text(mt: &MakeText<'_>) -> Result<Element> {
    let mut group = Element::builder("g", NS_SVG);
    let mut x = mt.x;
    let mut y = mt.y;
    let mut align = mt.alignment;
    let mut line = next_line(x, y, mt.font_size, mt.color, align);
    let mut tabs = mt.tabs.iter();
    let line_height = mt.line_height.unwrap_or_else(|| mt.font_size.line_height());
    for span in mt.text {
        let (text, target) = span.into_content_link(mt.link, mt.base_uri);
        for (index, line_text) in text.split('~').enumerate() {
            if index != 0 {
                tabs = mt.tabs.iter();
                x = mt.x;
                align = mt.alignment;
                y += line_height;
                group = group.append(line.build());
                line = next_line(x, y, mt.font_size, mt.color, align);
            }

            for (index, text_span) in line_text.split('^').enumerate() {
                if index != 0
                    && let Some(tab) = tabs.next()
                {
                    x = mt.x + tab.displacement;
                    align = tab.alignment;
                    group = group.append(line.build());
                    line = next_line(x, y, mt.font_size, mt.color, align);
                }

                let text_span = text_span.replace('_', " ");

                if let Some(target) = &target {
                    line = line.append(
                        Element::builder("a", NS_SVG)
                            .attr(n!("href"), target)
                            .append(text_span)
                            .build(),
                    );
                } else {
                    line = line.append(text_span);
                }
            }
        }
    }

    let line = line.build();
    if line.nodes().next().is_some() {
        group = group.append(line);
    }

    let mut group = group.build();
    Ok(if group.nodes().as_slice().len() == 1 {
        group.unshift_child().unwrap()
    } else {
        group
    })
}

/// Creates a new line of text with the given properties.
fn next_line(
    x: f64,
    y: f64,
    font_size: FontSize,
    color: ColorValue,
    align: Alignment,
) -> ElementBuilder {
    Element::builder("text", NS_SVG)
        .attr(n!("x"), x.v(3))
        .attr(n!("y"), y.v(3))
        .attr(n!("font-size"), font_size.value().v(3))
        .attr(n!("fill"), color.v(3))
        .attr(n!("text-anchor"), align.to_svg())
}

/// The SVG namespace.
const NS_SVG: &str = "http://www.w3.org/2000/svg";

/// Shorthand for XML attribute names.
macro_rules! n {
    ($name:literal) => {
        // Clippy: Upstream lints are leaking in.
        #[allow(clippy::transmute_ptr_to_ptr)]
        {
            ::minidom::rxml::xml_ncname!($name).into()
        }
    };
}

use n;

/// A trait for formatting floating point values with limited precision.
trait ValueDisplay {
    /// Formats the value as a floating point string with *maximum* precision
    /// `precision`.
    fn v(self, precision: u8) -> String;
}

impl ValueDisplay for f64 {
    fn v(self, precision: u8) -> String {
        let precision = 10.0_f64.powi(precision.into());
        let v = (self * precision).round() / precision;
        format!("{v}")
    }
}

impl ValueDisplay for ColorValue {
    // Clippy: Values are guaranteed to be in range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn v(self, precision: u8) -> String {
        match self {
            ColorValue::Hsb(h, s, b) => {
                format!(
                    "hsl({} {}% {}%)",
                    (h * 360.0).v(precision),
                    (s * 100.0).v(precision),
                    (b * 100.0).v(precision)
                )
            }
            ColorValue::Named(n) => PREDEFINED_COLORS.get(n).copied().unwrap().to_string(),
            ColorValue::Rgb(r, g, b) => {
                format!(
                    "#{:02x}{:02x}{:02x}",
                    (r * f64::from(u8::MAX)) as u8,
                    (g * f64::from(u8::MAX)) as u8,
                    (b * f64::from(u8::MAX)) as u8
                )
            }
        }
    }
}
