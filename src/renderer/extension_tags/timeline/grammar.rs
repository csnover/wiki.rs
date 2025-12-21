//! Parsing expression grammar for EasyTimeline scripts.

// Clippy: There are a lot of imports, and little value in listing them
// explicitly.
#[allow(clippy::wildcard_imports)]
use super::{parser::*, *};

/// The expected DPI for physical units.
const DPI: f64 = 100.0;

peg::parser! {pub(super) grammar easy_timeline() for str {
  #[no_eof]
  pub rule chunk() -> Chunk<'input>
  = &sol()
    empty_line()*
    chunk:(d:define() { Some(d) } / any_command() { None })
    end:position!()
  { chunk.map_or(
        Chunk::Command(end),
        |command| Chunk::Define(end, command)
  ) }

  rule any_command()
  = alpha()+ space()? "="
    chunk_line()
    (space() chunk_line() / eol())*

  rule chunk_line()
  = (space() / !eolf() [_])* eolf()

  #[no_eof]
  pub rule timeline(date_format: DateFormat) -> (usize, Command<'input>)
  = &sol()
    empty_line()*
    c:command(date_format)
    end:position!()
  { (end, c) }

  rule command(format: DateFormat) -> Command<'input>
  = align_bars()
  / background_colors()
  / bar_data()
  / colors()
  / date_format()
  / legend()
  / line_data(format)
  / image_size()
  / period(format)
  / plot_area()
  / plot_data(format)
  / preset()
  / scale_major(format)
  / scale_minor(format)
  / text_data()
  / time_axis()

  rule single_line_command<T>(name: &'static str, value: rule<T>) -> T
  = start_command(name)
    v:value()
  { v }

  rule multi_line_command<T>(name: &'static str, value: rule<T>) -> Vec<T>
  = start_command(name)
    v:(empty_line()* space() !eolf() v:value() { v })*
  { v }

  rule start_command(lit: &'static str)
  = &sol() i(lit) space()? "=" space()?

  rule empty_line()
  = space()? eol()

  ///////////////
  // AlignBars //
  ///////////////

  rule align_bars() -> Command<'input>
  = v:single_line_command("AlignBars", <align_bars_value()>)
  { Command::AlignBars(v) }

  rule align_bars_value() -> AlignBars
  = i("early")   { AlignBars::Early }
  / i("justify") { AlignBars::Justify }
  / i("late")    { AlignBars::Late }

  //////////////////////
  // BackgroundColors //
  //////////////////////

  rule background_colors() -> Command<'input>
  = v:single_line_command("BackgroundColors", <background_colors_values()>)
  { Command::BackgroundColors(v) }

  rule background_colors_values() -> BackgroundColors<'input>
  = parts:background_colors_value()**space()
  {
    let mut bars = None;
    let mut canvas = None;
    for part in parts {
        match part {
            BackgroundColorsPart::Bars(b) => bars = Some(b),
            BackgroundColorsPart::Canvas(c) => canvas = Some(c),
        }
    }
    BackgroundColors { bars, canvas }
  }

  rule background_colors_value() -> BackgroundColorsPart<'input>
  = attr("bars") c:color_id()   { BackgroundColorsPart::Bars(c) }
  / attr("canvas") c:color_id() { BackgroundColorsPart::Canvas(c) }

  /////////////
  // BarData //
  /////////////

  rule bar_data() -> Command<'input>
  = v:multi_line_command("BarData", <bar_data_values()>)
  { Command::BarData(v) }

  rule bar_data_values() -> BarData<'input>
  = parts:bar_data_value()**space()
  {?
    let mut bar = None;
    let mut link = None;
    let mut bar_set = None;
    let mut text = None;
    for part in parts {
        match part {
            BarDataPart::Id(b) => bar = Some(b),
            BarDataPart::Link(l) => link = Some(l),
            BarDataPart::SetId(s) => bar_set = Some(s),
            BarDataPart::Text(t) => text = Some(t),
        }
    }

    match (bar, bar_set) {
        (Some(id), None) => Ok(BarData::Bar { id, link, text }),
        (None, Some(id)) => Ok(BarData::BarSet(id)),
        (Some(_), Some(_)) => Err("bar *or* barset"),
        (None, None) => Err("bar or barset"),
    }
  }

  rule bar_data_value() -> BarDataPart<'input>
  = attr("barset") t:bar_id() { BarDataPart::SetId(t) }
  / attr("bar") t:bar_id()    { BarDataPart::Id(t) }
  / attr("link") t:url()      { BarDataPart::Link(t) }
  / attr("text") t:text()     { BarDataPart::Text(t) }

  ////////////
  // Colors //
  ////////////

  rule colors() -> Command<'input>
  = v:multi_line_command("Colors", <colors_values()>)
  { Command::Colors(v) }

  rule colors_values() -> Color<'input>
  = parts:colors_value()**space()
  {?
    let mut id = None;
    let mut legend = None;
    let mut value = None;
    for part in parts {
        match part {
            ColorPart::Id(i) => id = Some(i),
            ColorPart::Legend(l) => legend = Some(l),
            ColorPart::Value(v) => value = Some(v),
        }
    }
    match (id, value) {
        (Some(id), Some(value)) => Ok(Color { id, legend, value }),
        (None, _) => Err("id"),
        (_, None) => Err("value"),
    }
  }

  rule colors_value() -> ColorPart<'input>
  = attr("id") t:color_id()       { ColorPart::Id(t) }
  / attr("legend") t:text()       { ColorPart::Legend(t) }
  / attr("value") t:color_value() { ColorPart::Value(t) }

  rule color_value() -> ColorValue
  = n:color_predefined()
  { ColorValue::Named(n) }
  / i("rgb(")
        space()? r:norm_decimal() space()?
    "," space()? g:norm_decimal() space()?
    "," space()? b:norm_decimal() space()? ")"
  { ColorValue::Rgb(r, g, b) }
  / i("gray(") space()? g:norm_decimal() space()? ")"
  { ColorValue::Rgb(g, g, g) }
  / i("hsb(")
        space()? h:norm_decimal() space()?
    "," space()? s:norm_decimal() space()?
    "," space()? b:norm_decimal() space()? ")"
  { ColorValue::Hsb(h, s, b) }
  / i("rgbx(")
    space()? v:$(hexdigit()*<6,6> (hexdigit()*<6,6>)?) space()? ")"
  {
    let (r, g, b) = if v.len() == 6 {
        (hex_dec_8(&v[0..2]), hex_dec_8(&v[2..4]), hex_dec_8(&v[4..6]))
    } else {
        (hex_dec_16(&v[0..4]), hex_dec_16(&v[4..8]), hex_dec_16(&v[8..12]))
    };
    ColorValue::Rgb(r, g, b)
  }

  // Must remain sorted longest prefix first for names with common prefixes
  rule color_predefined() -> &'static str
  = key:$(alpha()+ digit()?)
  {? PREDEFINED_COLORS.get_key(key).ok_or("predefined color") }

  ////////////////
  // DateFormat //
  ////////////////

  rule date_format() -> Command<'input>
  = v:single_line_command("DateFormat", <date_format_value()>)
  { Command::DateFormat(v) }

  rule date_format_value() -> DateFormat
  = "dd/mm/yyyy" { DateFormat::Normal }
  / "mm/dd/yyyy" { DateFormat::American }
  / "yyyy-mm-dd" { DateFormat::Iso8601 }
  / "yyyy"       { DateFormat::Year }
  / "x.y"        { DateFormat::Decimal }

  ////////////
  // Define //
  ////////////

  rule define() -> Define<'input>
  = i("Define")
    space()
    key:ident()
    space()?
    "="
    space()?
    value:$([^'#'|'\n']+)
    empty_line()*
  { Define { key, value } }

  rule ident() -> &'input str
  = "$" i:alnum()
  { i }

  ///////////////
  // ImageSize //
  ///////////////

  rule image_size() -> Command<'input>
  = v:single_line_command("ImageSize", <image_size_values()>)
  { Command::ImageSize(v) }

  rule image_size_values() -> ImageSize
  = parts:image_size_value()**space()
  {?
    let mut bar_increment = None;
    let mut height = None;
    let mut width = None;
    for part in parts {
        match part {
            ImageSizePart::BarIncrement(i) => bar_increment = Some(i),
            ImageSizePart::Height(h) => height = Some(h),
            ImageSizePart::Width(w) => width = Some(w),
        }
    }
    match (width, height, bar_increment) {
        (None | Some(Dim::Auto), Some(Dim::Size(height)), Some(bar)) => {
            Ok(ImageSize::AutoWidth { bar, height })
        },
        (Some(Dim::Size(width)), None | Some(Dim::Auto), Some(bar)) => {
            Ok(ImageSize::AutoHeight { bar, width })
        },
        (Some(Dim::Size(width)), Some(Dim::Size(height)), _) => {
            Ok(ImageSize::Fixed { width, height })
        },
        (Some(Dim::Auto), Some(Dim::Auto), _) => Err("auto width *or* auto height"),
        (Some(Dim::Auto), _, None) | (_, Some(Dim::Auto), None) => Err("barincrement"),
        (None, None, _) => Err("width or height"),
        (None, _, _) => Err("width"),
        (_, None, _) => Err("height"),
    }
  }

  rule image_size_value() -> ImageSizePart
  = attr("barincrement") i:abs_size() { ImageSizePart::BarIncrement(i) }
  / attr("height") h:dim()            { ImageSizePart::Height(h) }
  / attr("width") w:dim()             { ImageSizePart::Width(w) }

  rule dim() -> Dim
  = t:abs_size() { Dim::Size(t) }
  / i("auto")    { Dim::Auto }

  ////////////
  // Legend //
  ////////////

  rule legend() -> Command<'input>
  = v:single_line_command("Legend", <legend_values()>)
  { Command::Legend(v) }

  rule legend_values() -> Legend
  = parts:legend_value()**space()
  {?
    let mut legend = Legend::default();
    for part in parts {
        match part {
            LegendPart::Columns(c) => legend.columns = Some(c),
            LegendPart::ColumnWidth(w) => legend.column_width = Some(w),
            LegendPart::Left(l) => legend.left = Some(l),
            LegendPart::Orientation(o) => legend.orientation = o,
            LegendPart::Position(p) => legend.position = p,
            LegendPart::Top(t) => legend.top = Some(t),
        }
    }

    if matches!(
        (legend.orientation, legend.position),
        (Orientation::Horizontal, LegendPosition::Right)
    ) {
        Err("vertical orientation or non-right position")
    } else {
        Ok(legend)
    }
  }

  rule legend_value() -> LegendPart
  = attr("columns") c:['1'..='4']
  { LegendPart::Columns(c.to_digit(10).unwrap().try_into().unwrap()) }
  / attr("columnwidth") w:rel_size()
  { LegendPart::ColumnWidth(w) }
  / attr("left") x:rel_size()
  { LegendPart::Left(x) }
  / attr("orientation") o:orientation()
  { LegendPart::Orientation(o) }
  / attr("position") p:legend_position()
  { LegendPart::Position(p) }
  / attr("top") y:rel_size()
  { LegendPart::Top(y) }

  rule legend_position() -> LegendPosition
  = i("bottom") { LegendPosition::Bottom }
  / i("right")  { LegendPosition::Right }
  / i("top")    { LegendPosition::Top }

  rule orientation() -> Orientation
  = i("hor") i("izontal")? { Orientation::Horizontal }
  / i("ver") i("tical")?   { Orientation::Vertical }

  //////////////
  // LineData //
  //////////////

  rule line_data(date_format: DateFormat) -> Command<'input>
  = v:multi_line_command("LineData", <line_data_values(date_format)>)
  { Command::LineData(v) }

  rule line_data_values(date_format: DateFormat) -> LineData<'input>
  = parts:line_data_value(date_format)**space()
  {?
    let mut at = None;
    let mut at_pos = None;
    let mut color = None;
    let mut from = None;
    let mut from_pos = None;
    let mut layer = None;
    let mut points = None;
    let mut till = None;
    let mut till_pos = None;
    let mut width = None;
    for part in parts {
        match part {
            LineDataPart::At(a) => at = Some(a),
            LineDataPart::AtPos(a) => at_pos = Some(a),
            LineDataPart::Color(c) => color = Some(c),
            LineDataPart::From(f) => from = Some(f),
            LineDataPart::FromPos(f) => from_pos = Some(f),
            LineDataPart::Layer(l) => layer = Some(l),
            LineDataPart::Points(p0, p1) => points = Some((p0, p1)),
            LineDataPart::Till(t) => till = Some(t),
            LineDataPart::TillPos(t) => till_pos = Some(t),
            LineDataPart::Width(w) => width = Some(w),
        }
    }

    let instr = match (at_pos, from, till, at, from_pos, till_pos, points) {
        (Some(at), from, till, None, None, None, None) => {
            Some(LineDataInstr::Parallel { at, from, till })
        }
        (None, None, None, Some(at), from, till, None) => {
            Some(LineDataInstr::Orthogonal { at, from, till })
        }
        (None, None, None, None, None, None, Some((p0, p1))) => {
            Some(LineDataInstr::Independent(p0, p1))
        }
        (None, None, None, None, None, None, None) => {
            None
        }
        (_) => {
            return Err("valid combination of line instructions");
        }
    };

    Ok(LineData { color, instr, layer, width })
  }

  rule line_data_value(date_format: DateFormat) -> LineDataPart<'input>
  = attr("at") t:time(date_format)
  { LineDataPart::At(t) }
  / attr("atpos") t:rel_size()
  { LineDataPart::AtPos(t) }
  / attr("color") c:color_id()
  { LineDataPart::Color(c) }
  / attr("frompos") t:rel_size()
  { LineDataPart::FromPos(t) }
  / attr("from") t:time(date_format)
  { LineDataPart::From(t) }
  / attr("layer") l:layer()
  { LineDataPart::Layer(l) }
  / attr("points") "(" x1:integer() "," y1:integer() ")(" x2:integer() "," y2:integer() ")"
  { LineDataPart::Points(Point(x1.into(), y1.into()), Point(x2.into(), y2.into())) }
  / attr("tillpos") t:rel_size()
  { LineDataPart::TillPos(t) }
  / attr("till") t:time(date_format)
  { LineDataPart::Till(t) }
  / attr("width") t:abs_size()
  { LineDataPart::Width(t) }

  rule layer() -> Layer
  = i("back")  { Layer::Back }
  / i("front") { Layer::Front }

  ////////////
  // Period //
  ////////////

  rule period(date_format: DateFormat) -> Command<'input>
  = v:single_line_command("Period", <period_values(date_format)>)
  { Command::Period(v) }

  rule period_values(date_format: DateFormat) -> Period
  = parts:period_value(date_format)**space()
  {?
    let mut from = None;
    let mut till = None;
    for part in parts {
        match part {
            PeriodPart::From(f) => from = Some(f),
            PeriodPart::Till(t) => till = Some(t),
        }
    }

    match (from, till) {
        (Some(from), Some(till)) => Ok(Period { from, till }),
        (None, _) => Err("till"),
        (_, None) => Err("from"),
        (None, None) => Err("from and till"),
    }
  }

  rule period_value(date_format: DateFormat) -> PeriodPart
  = attr("from") f:time(date_format)
  { PeriodPart::From(f) }
  / attr("till") t:time(date_format)
  { PeriodPart::Till(t) }

  //////////////
  // PlotArea //
  //////////////

  rule plot_area() -> Command<'input>
  = v:single_line_command("PlotArea", <plot_area_values()>)
  { Command::PlotArea(v) }

  rule plot_area_values() -> PlotArea
  = parts:plot_area_value()**space()
  {?
    let mut bottom = None;
    let mut height = None;
    let mut left = None;
    let mut right = None;
    let mut top = None;
    let mut width = None;
    for part in parts {
        match part {
            PlotAreaPart::Bottom(b) => bottom = Some(b),
            PlotAreaPart::Height(h) => height = Some(h),
            PlotAreaPart::Left(l) => left = Some(l),
            PlotAreaPart::Right(r) => right = Some(r),
            PlotAreaPart::Top(t) => top = Some(t),
            PlotAreaPart::Width(w) => width = Some(w),
        }
    }

    let right = match (right, width) {
        (Some(right), None) => Some(PlotAreaEnd::Inset(right)),
        (None, Some(width)) => Some(PlotAreaEnd::Size(width)),
        (Some(_), Some(_)) => return Err("right *or* width"),
        (None, None) => None,
    };

    let top = match (top, height) {
        (Some(top), None) => Some(PlotAreaEnd::Inset(top)),
        (None, Some(height)) => Some(PlotAreaEnd::Size(height)),
        (Some(_), Some(_)) => return Err("top *or* height"),
        (None, None) => None,
    };

    Ok(PlotArea { left, top, right, bottom })
  }

  rule plot_area_value() -> PlotAreaPart
  = attr("bottom") b:rel_size()
  { PlotAreaPart::Bottom(b) }
  / attr("height") h:rel_size()
  { PlotAreaPart::Height(h) }
  / attr("left") l:rel_size()
  { PlotAreaPart::Left(l) }
  / attr("right") r:rel_size()
  { PlotAreaPart::Right(r) }
  / attr("top") t:rel_size()
  { PlotAreaPart::Top(t) }
  / attr("width") w:rel_size()
  { PlotAreaPart::Width(w) }

  //////////////
  // PlotData //
  //////////////

  rule plot_data(date_format: DateFormat) -> Command<'input>
  = v:multi_line_command("PlotData", <plot_data_values(date_format)>)
  { Command::PlotData(v) }

  rule plot_data_values(date_format: DateFormat) -> PlotData<'input>
  = parts:plot_data_value(date_format)**space()
  {?
    let mut align = None;
    let mut anchor = None;
    let mut at = None;
    let mut bar = None;
    let mut bar_set = None;
    let mut color = None;
    let mut font_size = None;
    let mut from = None;
    let mut link = None;
    let mut mark = None;
    let mut shift = None;
    let mut text = None;
    let mut text_color = None;
    let mut till = None;
    let mut width = None;
    for part in parts {
        match part {
            PlotDataPart::Alignment(a) => align = Some(a),
            PlotDataPart::Anchor(a) => anchor = Some(a),
            PlotDataPart::At(a) => at = Some(a),
            PlotDataPart::Bar(b) => bar = Some(b),
            PlotDataPart::Barset(b) => bar_set = Some(b),
            PlotDataPart::Color(c) => color = Some(c),
            PlotDataPart::FontSize(f) => font_size = Some(f),
            PlotDataPart::From(f) => from = Some(f),
            PlotDataPart::Link(l) => link = Some(l),
            PlotDataPart::Mark(m) => mark = Some(m),
            PlotDataPart::Shift(s) => shift = Some(s),
            PlotDataPart::Text(t) => text = Some(t),
            PlotDataPart::TextColor(t) => text_color = Some(t),
            PlotDataPart::Till(t) => till = Some(t),
            PlotDataPart::Width(w) => width = Some(w),
        }
    }

    let at = match (at, from, till) {
        (Some(at), None, None) => Some(PlotDataPos::At(at)),
        (None, Some(from), Some(till)) => Some(PlotDataPos::Range(from, till)),
        (None, None, None) => None,
        _ => return Err("at *or* from and till")
    };

    if bar_set.is_some() && link.is_some() {
        return Err("barset *or* link");
    }

    let bar = match (bar, bar_set) {
        (Some(bar), None) => Some(PlotDataTarget::Bar(bar)),
        (None, Some(bar_set)) => Some(PlotDataTarget::BarSet(bar_set)),
        (Some(_), Some(_)) => return Err("bar *or* barset"),
        (None, None) => None,
    };

    Ok(PlotData {
        align, anchor, at, bar, color, font_size, link,
        mark, shift, text_color, text, width
    })
  }

  rule plot_data_value(date_format: DateFormat) -> PlotDataPart<'input>
  = attr("align") a:alignment()
  { PlotDataPart::Alignment(a) }
  / attr("anchor") a:anchor()
  { PlotDataPart::Anchor(a) }
  / attr("at") t:time(date_format)
  { PlotDataPart::At(t) }
  / attr("barset") b:barset_id()
  { PlotDataPart::Barset(b) }
  / attr("bar") b:bar_id()
  { PlotDataPart::Bar(b) }
  / attr("color") c:color_id()
  { PlotDataPart::Color(c) }
  / attr("fontsize") s:font_size()
  { PlotDataPart::FontSize(s) }
  / attr("from") t:time(date_format)
  { PlotDataPart::From(t) }
  / attr("link") u:url()
  { PlotDataPart::Link(u) }
  / i("mark:(line") c:("," c:color_id() { c } / { "black" }) ")"
  { PlotDataPart::Mark(c) }
  / attr("shift") p:point()
  { PlotDataPart::Shift(p) }
  / attr("textcolor") c:color_id()
  { PlotDataPart::TextColor(c) }
  / attr("text") t:text()
  { PlotDataPart::Text(t) }
  / attr("till") t:time(date_format)
  { PlotDataPart::Till(t) }
  / attr("width") w:rel_size()
  { PlotDataPart::Width(w) }

  ////////////
  // Preset //
  ////////////

  rule preset() -> Command<'input>
  = v:single_line_command("Preset", <preset_value()>)
  { Command::Preset(v) }

  rule preset_value() -> Preset
  = i("TimeHorizontal_AutoPlaceBars_UnitYear")
  { Preset::TimeHorizontal }
  / i("TimeVertical_OneBar_UnitYear")
  { Preset::TimeVertical }

  /////////////////////////////
  // ScaleMajor & ScaleMinor //
  /////////////////////////////

  rule scale_major(date_format: DateFormat) -> Command<'input>
  = v:single_line_command("ScaleMajor", <scale_values(date_format)>)
  { Command::ScaleMajor(v) }

  rule scale_minor(date_format: DateFormat) -> Command<'input>
  = v:single_line_command("ScaleMinor", <scale_values(date_format)>)
  { Command::ScaleMinor(v) }

  rule scale_values(date_format: DateFormat) -> Scale<'input>
  = parts:scale_value(date_format)**space()
  {
    let mut grid_color = None;
    let mut interval = 1;
    let mut start = None;
    let mut unit = ScaleUnit::Year;
    for part in parts {
        match part {
            ScalePart::GridColor(c) => grid_color = Some(c),
            ScalePart::Increment(i) => interval = i,
            ScalePart::Start(s) => start = Some(s),
            ScalePart::Unit(u) => unit = u,
        }
    }

    Scale { grid_color, interval, start, unit }
  }

  rule scale_value(date_format: DateFormat) -> ScalePart<'input>
  = (attr("gridcolor") / attr("grid")) c:color_id()
  { ScalePart::GridColor(c) }
  / attr("increment") i:integer()
  { ScalePart::Increment(i) }
  / attr("start") t:time(date_format)
  { ScalePart::Start(t) }
  / attr("unit") u:scale_unit()
  { ScalePart::Unit(u) }

  rule scale_unit() -> ScaleUnit
  = i("day") { ScaleUnit::Day }
  / i("month") { ScaleUnit::Month }
  / i("year") { ScaleUnit::Year }

  //////////////
  // TextData //
  //////////////

  rule text_data() -> Command<'input>
  = v:multi_line_command("TextData", <text_data_values()>)
  { Command::TextData(v) }

  rule text_data_values() -> TextData<'input>
  = parts:text_data_value()**space()
  {
    let mut font_size = None;
    let mut line_height = None;
    let mut link = None;
    let mut pos = None;
    let mut tabs = None;
    let mut text = None;
    let mut text_color = None;
    for part in parts {
        match part {
            TextDataPart::FontSize(f) => font_size = Some(f),
            TextDataPart::LineHeight(l) => line_height = Some(l),
            TextDataPart::Link(l) => link = Some(l),
            TextDataPart::Position(p) => pos = Some(p),
            TextDataPart::Tabs(t) => tabs = Some(t),
            TextDataPart::Text(t) => text = Some(t),
            TextDataPart::TextColor(t) => text_color = Some(t),
        }
    }

    TextData { font_size, line_height, link, pos, tabs, text, text_color }
  }

  rule text_data_value() -> TextDataPart<'input>
  = attr("fontsize") s:font_size()
  { TextDataPart::FontSize(s) }
  / attr("lineheight") h:abs_size()
  {? (1.0..=40.0).contains(&h)
        .then_some(TextDataPart::LineHeight(h))
        .ok_or("number 1.0..=40")
  }
  / attr("link") u:url()
  { TextDataPart::Link(u) }
  / attr("pos") p:point()
  { TextDataPart::Position(p) }
  / attr("tabs") t:"(" t:tab()**"," ")"
  { TextDataPart::Tabs(t) }
  / attr("textcolor") c:color_id()
  { TextDataPart::TextColor(c) }
  / attr("text") t:text()
  { TextDataPart::Text(t) }

  rule tab() -> TabStop
  = displacement:abs_size() "-" alignment:alignment()
  { TabStop { alignment, displacement } }

  //////////////
  // TimeAxis //
  //////////////

  rule time_axis() -> Command<'input>
  = v:single_line_command("TimeAxis", <time_axis_values()>)
  { Command::TimeAxis(v) }

  rule time_axis_values() -> TimeAxis
  = parts:time_axis_value()**space()
  {
    let mut format = DateFormat::Year;
    let mut order = Order::Normal;
    let mut orientation = Orientation::Horizontal;
    for part in parts {
        match part {
            TimeAxisPart::Format(f) => format = f,
            TimeAxisPart::Orientation(o) => orientation = o,
            TimeAxisPart::Order(o) => order = o,
        }
    }

    TimeAxis { format, order, orientation }
  }

  rule time_axis_value() -> TimeAxisPart
  = attr("format") d:date_format_value()
  { TimeAxisPart::Format(d) }
  / attr("order") o:order()
  { TimeAxisPart::Order(o) }
  / attr("orientation") o:orientation()
  { TimeAxisPart::Orientation(o) }

  rule order() -> Order
  = i("normal") { Order::Normal }
  / i("reverse") { Order::Reverse }

  rule time(date_format: DateFormat) -> Time
  = i("start")
  { Time::Start }
  / i("end")
  { Time::End }
  / &assert(
      matches!(date_format, DateFormat::American | DateFormat::Normal),
      "mm/dd/yyyy or dd/mm/yyyy")
    dm:$(digit()*<2,2>) "/" md:$(digit()*<2,2>) "/" y:$(digit()*<4,4>)
  {?
    if date_format == DateFormat::Normal {
        make_time(y, md, dm)
    } else {
        make_time(y, dm, md)
    }
  }
  / &assert(matches!(date_format, DateFormat::Iso8601), "yyyy-mm-dd")
    y:$("-"? digit()*<4,4>) "-" m:$(digit()*<2,2>) "-" d:$(digit()*<2,2>)
  {? make_time(y, m, d) }
  / &assert(
      matches!(date_format, DateFormat::Year | DateFormat::Decimal),
      "yyyy or x[.y]")
    d:$("-"? digit()+ "."? digit()*)
  { Time::Decimal(d.parse::<f64>().unwrap()) }
  / y:$(digit()*<4,4>)
  { Time::Decimal(y.parse::<f64>().unwrap()) }

  ///////////
  // Atoms //
  ///////////

  rule attr(lit: &'static str)
  = i(lit) space()? ":" space()?

  rule abs_size() -> f64
  = value:decimal()
    pixels:to_px(value)
  { pixels }

  rule to_px(value: f64) -> f64
  = i("px") { value }
  / i("in") { value * DPI }
  / i("cm") { value * DPI / 2.54 }
  / !"%"    { value }

  rule alignment() -> Alignment
  = i("left")   { Alignment::Start }
  / i("center") { Alignment::Middle }
  / i("right")  { Alignment::End }

  rule anchor() -> Alignment
  = i("from")   { Alignment::Start }
  / i("middle") { Alignment::Middle }
  / i("till")   { Alignment::End }

  rule bar_id() -> BarId<'input>
  = alnum()

  rule barset_id() -> BarsetId<'input>
  = i("break") { BarsetId::Reset }
  / i("skip")  { BarsetId::Skip }
  / i:alnum()  { BarsetId::Id(i) }

  rule color_id() -> ColorId<'input>
  = alnum()

  rule font_size() -> FontSize
  = i("xs")
  { FontSize::Smaller }
  / i("s")
  { FontSize::Small }
  / i("m")
  { FontSize::Medium }
  / i("l")
  { FontSize::Large }
  / i("xl")
  { FontSize::Larger }
  / t:integer()
  {? (6..=30).contains(&t).then_some(FontSize::Absolute(t)).ok_or("integer 6..=30") }

  rule point() -> Point
  = "(" space()? x:abs_size() space()? "," space()? y:abs_size() space()? ")"
  { Point(x, y) }

  rule rel_size() -> Unit
  = value:decimal()
    unit:(
      value:to_px(value) { Unit::Absolute(value) }
      / "%"              { Unit::Relative(value / 100.0) }
    )
    /* any amount of garbage is allowed to follow the number part */
    any()?
  { unit }

  rule comment()
  = "#>" (!"<#" [_])* "<#"
  // &#x; is not a comment
  / !"&" "#" (!eol() [_])*

  rule text() -> Vec<TextSpan<'input>>
  = "\"" t:text_span(<['"'|'\n'] {}>)* "\"" { t }
  / text_span(<['#'|'\n'|'\t'|'\r'|'\x0c'] {} / " " alpha()+ ":" {}>)*

  rule text_span(terminator: rule<()>) -> TextSpan<'input>
  = "[[" link:text_span_link(<"]]" / terminator()>) "]]"
  { TextSpan::Link { target: link.0, text: link.1 } }
  / "[" link:text_span_link(<"]" / terminator()>) "]"
  { TextSpan::ExternalLink { target: link.0, text: link.1 }}
  / text:$((!("[" / terminator()) [_])+)
  { TextSpan::Text(text) }

  rule text_span_link(terminator: rule<()>) -> (&'input str, &'input str)
  = target:$((!("|" / terminator()) [_])*)
    text:("|" t:$((!terminator() [_])*) { t })?
  { (target, text.unwrap_or(target)) }

  rule url() -> &'input str
  = $((scheme() ":")? "//" any())

  rule scheme()
  = alpha() (alpha() / digit() / ['+'|'-'|'.'])*

  rule decimal() -> f64
  = t:$("-"? digit()+ "."? digit()*)
  { t.parse::<f64>().unwrap() }

  rule norm_decimal() -> f64
  = t:$("0" ("." digit()+)? / "1")
  {? t.parse::<f64>().map_err(|_| "decimal 0..=1") }

  rule integer() -> i32
  = t:$(digit()*<1,10>)
  {? t.parse::<i32>().map_err(|_| "integer") }

  rule any()
  = [^' '|'\t'|'\r'|'\x0c'|'\n'|'#']+

  rule alnum() -> &'input str
  = $((alpha() / digit() / "_")+)

  rule alpha()
  = ['a'..='z'|'A'..='Z']

  rule digit()
  = ['0'..='9']

  rule hexdigit()
  = (digit() / ['a'..='f'|'A'..='F'])

  rule space()
  = (comment() / [' '|'\t'|'\r'|'\x0c'])+

  rule eof()
  = ![_]

  rule eol()
  = "\n"

  rule eolf()
  = eol() / eof()

  rule sol()
  = #{|input, pos| {
    if matches!(input[..pos].chars().nth_back(0), None | Some('\n')) {
        peg::RuleResult::Matched(pos, ())
    } else {
        peg::RuleResult::Failed
    }
  }}

  rule assert(cond: bool, msg: &'static str)
  = {? if cond { Ok(())} else { Err(msg) } }

  rule i(lit: &'static str)
  = quiet!{
    input:$([_]*<{lit.chars().count()}>)
    {? if input.eq_ignore_ascii_case(lit) { Ok(()) } else { Err(lit) } }
  } / expected!(lit)
}}

/// Converts 8-bit hexadecimal to normalised float.
#[inline]
fn hex_dec_8(input: &str) -> f64 {
    f64::from(u8::from_str_radix(input, 16).unwrap()) / f64::from(u8::MAX)
}

/// Converts 16-bit hexadecimal to normalised float.
#[inline]
fn hex_dec_16(input: &str) -> f64 {
    f64::from(u16::from_str_radix(input, 16).unwrap()) / f64::from(u16::MAX)
}

/// Converts calendar year digit strings to decimal `year.fract` time.
fn make_time(y: &str, m: &str, d: &str) -> Result<Time, &'static str> {
    let m = m.parse::<u8>().unwrap();
    time::Date::from_calendar_date(
        y.parse().unwrap(),
        m.try_into().map_err(|_| "valid month")?,
        d.parse().unwrap(),
    )
    .map(|date| {
        let (year, days) = date.to_ordinal_date();
        let fract = f64::from(days) / f64::from(time::util::days_in_year(year));
        Time::Decimal(f64::from(year) + fract)
    })
    .map_err(|_| "valid date")
}

/// `BackgroundColors` attribute.
enum BackgroundColorsPart<'input> {
    /// Bar background colour.
    Bars(ColorId<'input>),
    /// Canvas background colour.
    Canvas(ColorId<'input>),
}

/// `BarData` attribute.
enum BarDataPart<'input> {
    /// Bar ID.
    Id(BarId<'input>),
    /// Text label URL.
    Link(Url<'input>),
    /// Barset ID.
    SetId(BarId<'input>),
    /// Text label.
    Text(Vec<TextSpan<'input>>),
}

/// Unprocessed command chunk.
#[derive(Debug)]
pub(super) enum Chunk<'input> {
    /// End position of other command chunk.
    Command(usize),
    /// End position of define command chunk, with define.
    Define(usize, Define<'input>),
}

/// `Colors` attribute.
enum ColorPart<'input> {
    /// ID.
    Id(ColorId<'input>),
    /// Value.
    Value(ColorValue),
    /// Legend text.
    Legend(Vec<TextSpan<'input>>),
}

/// Image dimension.
enum Dim {
    /// Fixed size.
    Size(f64),
    /// Auto size.
    Auto,
}

/// `ImageSize` attribute.
enum ImageSizePart {
    /// Cross-axis bar size for auto height/width.
    BarIncrement(f64),
    /// Height.
    Height(Dim),
    /// Width.
    Width(Dim),
}

/// `Legend` attribute.
enum LegendPart {
    /// Column count.
    Columns(u8),
    /// Column width.
    ColumnWidth(Unit),
    /// Left position, relative to the bottom-left corner of the image.
    Left(Unit),
    /// Orientation.
    Orientation(Orientation),
    /// Position, relative to the plot area.
    Position(LegendPosition),
    /// Top position, relative to the bottom-left corner of the image.
    Top(Unit),
}

/// `Period` attribute.
enum PeriodPart {
    /// Start time.
    From(Time),
    /// End time.
    Till(Time),
}

/// `LineData` attribute.
enum LineDataPart<'input> {
    /// Instantaneous position for cross-axis lines.
    At(Time),
    /// Absolute start position for cross-axis lines, relative to the start edge
    /// of the image (i.e. either the bottom or left edge of the image,
    /// depending on the time axis orientation).
    AtPos(Unit),
    /// Stroke colour.
    Color(ColorId<'input>),
    /// Start instant for main-axis lines.
    From(Time),
    /// Absolute position for cross-axis lines, relative to the start ege of the
    /// image.
    FromPos(Unit),
    /// Drawing layer.
    Layer(Layer),
    /// Absolute position for an arbitrary line segment, relative to the
    /// bottom-left corner of the image.
    Points(Point, Point),
    /// End instant for main-axis lines.
    Till(Time),
    /// Absolute end position for cross-axis lines, relative to the start edge
    /// of the image (i.e. either the bottom or left edge of the image,
    /// depending on the time axis orientation).
    TillPos(Unit),
    /// Stroke width.
    Width(f64),
}

/// `PlotArea` attribute.
enum PlotAreaPart {
    /// Bottom inset, relative to the bottom edge of the image.
    Bottom(Unit),
    /// Height (deprecated).
    Height(Unit),
    /// Left inset, relative to the left edge of the image.
    Left(Unit),
    /// Right inset, relative to the right edge of the image.
    Right(Unit),
    /// Top inset, relative to the top edge of the image.
    Top(Unit),
    /// Width (deprecated).
    Width(Unit),
}

/// `PlotData` attribute.
enum PlotDataPart<'input> {
    /// Text label alignment.
    Alignment(Alignment),
    /// Main axis text label alignment.
    Anchor(Alignment),
    /// Instantaneous position.
    At(Time),
    /// Related bar ID.
    Bar(BarId<'input>),
    /// Related bar set ID.
    Barset(BarsetId<'input>),
    /// Fill colour.
    Color(ColorId<'input>),
    /// Text label font size.
    FontSize(FontSize),
    /// Start instant.
    From(Time),
    /// Text label URL.
    Link(Url<'input>),
    /// Mark colour.
    Mark(ColorId<'input>),
    /// Text label offset.
    Shift(Point),
    /// Text label.
    Text(Vec<TextSpan<'input>>),
    /// Label text colour.
    TextColor(ColorId<'input>),
    /// End instant.
    Till(Time),
    /// Cross-axis bar size.
    Width(Unit),
}

/// `MajorScale` and `MinorScale` attribute.
enum ScalePart<'input> {
    /// Grid line colour.
    GridColor(ColorId<'input>),
    /// Interval between ticks, in `Unit` units.
    Increment(i32),
    /// Start instant.
    Start(Time),
    /// Tick interval scale.
    Unit(ScaleUnit),
}

/// `TextData` attribute.
enum TextDataPart<'input> {
    /// Font size.
    FontSize(FontSize),
    /// Line height.
    LineHeight(f64),
    /// Link URL.
    Link(Url<'input>),
    /// Text baseline position, relative to the bottom-left corner of the image.
    Position(Point),
    /// Tab stops.
    Tabs(Vec<TabStop>),
    /// Text.
    Text(Vec<TextSpan<'input>>),
    /// Text colour.
    TextColor(ColorId<'input>),
}

/// `TimeAxis` attribute.
enum TimeAxisPart {
    /// Expected format for times in [`ScalePart`].
    Format(DateFormat),
    /// Time flow direction.
    Order(Order),
    /// Main axis orientation.
    Orientation(Orientation),
}
