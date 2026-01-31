//! Rendering functions for graph marks.

use super::{
    super::{
        DoubleSizeIterator, Node, Result,
        mark::{Kind as MarkKind, Mark, Orient, Shape},
        propset::{Align, Baseline, Getter, Kind as PropsetKind, Propset},
    },
    NS_SVG, Rect, TextMetrics, ValueDisplay, Vec2, defaults, draw_container, interp, n,
    path::{Command, SvgPath, SvgPathIterator, arc_path},
    text_metrics, to_svg,
};
use core::fmt::Write as _;
use minidom::{Element, ElementBuilder};
use rustybuzz::{Face, GlyphBuffer, ShapePlan, UnicodeBuffer, ttf_parser::Tag};
use std::{borrow::Cow, cell::Cell, sync::LazyLock};

/// Creates an SVG element for a single mark.
pub(super) fn draw_mark<'s>(
    mark: &Mark<'s>,
    parent: &Node<'s, '_>,
) -> Result<Option<(Element, Rect)>> {
    let propset = if let Some(propset) = mark.propset(PropsetKind::Enter) {
        propset
    } else if matches!(mark.kind, MarkKind::Group(_)) {
        &<_>::default()
    } else {
        return Ok(None);
    };

    // TODO: stroke/fill should default for marks which are not axis/legend.
    let node = &parent.with_child_mark(mark);
    if propset.opacity.get(node) == Some(0.0) {
        return Ok(None);
    }

    let mut bounds = Rect::default();
    let g = Element::builder("g", NS_SVG);
    let g = match &mark.kind {
        MarkKind::Rect => g.append_all(draw_rect(propset, node)),
        MarkKind::Symbol => g.append_all(draw_symbol(propset, node)),
        MarkKind::Path => g.append_all(draw_path(propset, node)),
        MarkKind::Arc => g.append_all(draw_arc(propset, node)),
        MarkKind::Area => g.append_all(draw_area(propset, node)),
        MarkKind::Line => g.append_all(draw_line(propset, node)),
        MarkKind::Rule => g.append_all(draw_rule(propset, node)),
        MarkKind::Image => g.append_all(draw_image(propset, node)),
        MarkKind::Text => g.append_all(draw_text(propset, node)),
        MarkKind::Group(_) => {
            let mut g = g;
            for child in draw_group(propset, node) {
                let (element, child_bounds) = child?;
                g = g.append(element);
                bounds = bounds.union(&child_bounds);
            }
            g
        }
    };
    let mut class = format!("mark-{}", mark.kind);
    if let Some(name) = &mark.name {
        let _ = write!(class, " {name}");
    }
    let g = g.attr(n!("class"), class).build();
    Ok(if g.nodes().next().is_none() {
        None
    } else {
        Some((g, bounds))
    })
}

/// Calculates approximate bounds of a mark.
pub(super) fn calculate_bounds<'s>(mark: &Mark<'s>, node: &Node<'s, '_>) -> Rect {
    let mut bounds = Rect::default();
    let Some(propset) = mark.propset(PropsetKind::Enter) else {
        return bounds;
    };

    for ref node in node.with_child_mark(mark).for_each_item() {
        match &mark.kind {
            MarkKind::Group(group) => {
                let (x, width) = propset.x(node);
                let (y, height) = propset.y(node);

                for mark in &group.marks {
                    bounds = bounds.union(&calculate_bounds(mark, node));
                }

                bounds = bounds.offset((x, y).into());

                if propset.clip.get(node) == Some(true) {
                    bounds = bounds.intersect(&Rect::from_xywh(x, y, width, height));
                }
            }
            MarkKind::Image => {
                bounds = bounds.union(&image_dims(propset, node));
            }
            MarkKind::Line => {
                // TODO: This needs to calculate bounds of the interpolated
                // line and account for the stroke width.
                let x = propset.x.get(node).unwrap_or(0.0);
                let y = propset.y.get(node).unwrap_or(0.0);
                bounds = bounds.union(&Rect::from_xywh(x, y, 0.0, 0.0));
            }
            MarkKind::Path => {
                let Some(path) = propset.path.get(node) else {
                    continue;
                };

                let left = propset.x.get(node).unwrap_or(0.0);
                let top = propset.y.get(node).unwrap_or(0.0);
                let mut x = 0.0;
                let mut y = 0.0;
                // TODO: This needs to calculate bounds of the interpolated
                // line and account for the stroke width.
                for command in SvgPathIterator::new(&path) {
                    match command {
                        Command::Close => {}
                        Command::Arc {
                            point: (x1, y1), ..
                        }
                        | Command::Move(x1, y1) => {
                            x += x1;
                            y += y1;
                        }
                        Command::ArcTo {
                            point: (x2, y2), ..
                        }
                        | Command::LineTo(x2, y2)
                        | Command::MoveTo(x2, y2) => {
                            x = x2;
                            y = y2;
                        }
                        Command::HorizontalTo(x2) => {
                            x = x2;
                        }
                        Command::VerticalTo(y2) => {
                            y = y2;
                        }
                    }
                    bounds.left = bounds.left.min(left + x);
                    bounds.right = bounds.right.max(left + x);
                    bounds.top = bounds.top.min(top + y);
                    bounds.bottom = bounds.bottom.min(top + y);
                }
            }
            MarkKind::Rect | MarkKind::Rule => {
                let (x, width) = propset.x(node);
                let (y, height) = propset.y(node);

                // TODO: This should probably account for the stroke width
                bounds = bounds.union(&Rect::from_xywh(x, y, width, height));
            }
            MarkKind::Symbol => {
                let r = propset.size.get(node).unwrap_or(0.0).sqrt() / 2.0;
                let x = propset.x.get(node).unwrap_or(0.0);
                let y = propset.y.get(node).unwrap_or(0.0);
                bounds = bounds.union(&Rect::new(x - r, y - r, x + r, y + r));
            }
            MarkKind::Text => {
                if let Some(rect) = calculate_text_bounds(propset, node) {
                    bounds = bounds.union(&rect);
                }
            }
            kind => {
                // Calculating the bounds of different shapes becomes
                // progressively more complicated, and this is only needed for
                // calculating the automatic padding, so YAGNI.
                log::warn!("I guess we need to know how to calculate the bounds for {kind:?} now");
            }
        }
    }

    bounds
}

/// Calculates the approximate bounding box of a text mark.
///
/// Someone should try creating a pie chart for this data set:
///
/// ```json
/// [
///   { "thing": "the library for calculating font metrics", "sloc": 90000 },
///   { "thing": "just about everything else", "sloc": 15000 },
/// ]
/// ```
fn calculate_text_bounds<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> Option<Rect> {
    let text = propset.text.get(node)?;

    #[rustfmt::skip]
    let TextMetrics { x, y, font_size, dx, dy } = text_metrics(propset, node);

    let bold = propset.font_weight.get(node).as_deref() == Some("bold");
    let italic = propset.font_style.get(node).as_deref() == Some("italic");
    let (width, height) = calculate_text_dims(&text, bold, italic, font_size);

    // `(x,y)` is the *text anchor point* origin, so it needs to be translated
    // to the box origin first
    let rect = {
        let dx = match propset.align.get(node).unwrap_or_default() {
            Align::Left => 0.0,
            Align::Center => width / 2.0,
            Align::Right => width,
        };
        let dy = height * propset.baseline.get(node).unwrap_or_default().above();
        Rect::from_xywh(x - dx, y - dy, width, height)
    };

    let angle = propset
        .angle
        .get(node)
        .map_or(0.0, |angle| (angle % 360.0).to_radians());

    let bounds = if angle == 0.0 {
        rect
    } else {
        let a_cos = angle.cos().abs();
        let a_sin = angle.sin().abs();

        let rotate = |px: f64, py: f64| -> (f64, f64) {
            let px = px - x;
            let py = py - y;
            (x + px * a_cos + py * a_sin, y - px * a_sin + py * a_cos)
        };

        let p0 = rotate(rect.left, rect.top);
        let p1 = rotate(rect.right, rect.top);
        let p2 = rotate(rect.left, rect.bottom);
        let p3 = rotate(rect.right, rect.bottom);
        let left = p0.0.min(p1.0).min(p2.0).min(p3.0);
        let top = p0.1.min(p1.1).min(p2.1).min(p3.1);
        let right = p0.0.max(p1.0).max(p2.0).max(p3.0);
        let bottom = p0.1.max(p1.1).max(p2.1).max(p3.1);

        Rect::new(left, top, right, bottom)
    };

    Some(bounds.offset((dx, dy).into()))
}

/// Calculates the width and height of a line of text with the given font
/// weight, style, and size using wiki.rs’s built-in sans-serif font face.
fn calculate_text_dims(text: &str, bold: bool, italic: bool, font_size: f64) -> (f64, f64) {
    shape_text(text, bold, italic, |face, buffer| {
        let upem = f64::from(face.units_per_em());
        let width = buffer
            .glyph_positions()
            .iter()
            .map(|pos| f64::from(pos.x_advance) * font_size / upem)
            .sum();
        (width, font_size)
    })
}

/// Calculates the shape of a line of text using wiki.rs’s built-in sans-serif
/// font face, passing this information to a callback and returning the result
/// of the callback.
pub(crate) fn shape_text<F, T>(text: &str, bold: bool, italic: bool, mut f: F) -> T
where
    F: FnMut(&Face<'_>, &GlyphBuffer) -> T,
{
    struct Shaper {
        normal: (Face<'static>, ShapePlan),
        bold: (Face<'static>, ShapePlan),
        italic: (Face<'static>, ShapePlan),
        bold_italic: (Face<'static>, ShapePlan),
    }

    // TODO: Reuse these already included resources
    static NORMAL_DATA: LazyLock<Vec<u8>> = LazyLock::new(|| {
        const NORMAL: &[u8] = include_bytes!("../../../../../res/fonts/Archivo.woff2");
        wuff::decompress_woff2(NORMAL).unwrap()
    });
    static ITALIC_DATA: LazyLock<Vec<u8>> = LazyLock::new(|| {
        const ITALIC: &[u8] = include_bytes!("../../../../../res/fonts/Archivo-Italic.woff2");
        wuff::decompress_woff2(ITALIC).unwrap()
    });

    static SHAPER: LazyLock<Shaper> = LazyLock::new(|| {
        // Using a two-step process here because, although this should not fail
        // (because whoever is writing this code should not be so dumb to try a
        // random, non-existent, font face index), if it *does* fail, it is
        // useful to see the error. This is otherwise exactly equivalent to
        // `from_slice`.
        let normal_face = rustybuzz::ttf_parser::Face::parse(&NORMAL_DATA, 0).unwrap();
        let mut normal_face = Face::from_face(normal_face);
        let italic_face = rustybuzz::ttf_parser::Face::parse(&ITALIC_DATA, 0).unwrap();
        let mut italic_face = Face::from_face(italic_face);

        normal_face.set_variation(Tag::from_bytes(b"wght"), 380.0);
        italic_face.set_variation(Tag::from_bytes(b"wght"), 380.0);

        let mut bold_face = normal_face.clone();
        bold_face.set_variation(Tag::from_bytes(b"wght"), 650.0);
        let mut bold_italic_face = italic_face.clone();
        bold_italic_face.set_variation(Tag::from_bytes(b"wght"), 650.0);

        let normal_plan = ShapePlan::new(
            &normal_face,
            rustybuzz::Direction::LeftToRight,
            Some(rustybuzz::script::LATIN),
            None,
            &[],
        );
        let bold_plan = ShapePlan::new(
            &bold_face,
            rustybuzz::Direction::LeftToRight,
            Some(rustybuzz::script::LATIN),
            None,
            &[],
        );
        let italic_plan = ShapePlan::new(
            &italic_face,
            rustybuzz::Direction::LeftToRight,
            Some(rustybuzz::script::LATIN),
            None,
            &[],
        );
        let bold_italic_plan = ShapePlan::new(
            &bold_italic_face,
            rustybuzz::Direction::LeftToRight,
            Some(rustybuzz::script::LATIN),
            None,
            &[],
        );

        Shaper {
            normal: (normal_face, normal_plan),
            bold: (bold_face, bold_plan),
            italic: (italic_face, italic_plan),
            bold_italic: (bold_italic_face, bold_italic_plan),
        }
    });

    thread_local! {
        static BUFFER: Cell<UnicodeBuffer> = Cell::new(UnicodeBuffer::new());
    }

    let (face, plan) = match (bold, italic) {
        (true, true) => &SHAPER.bold_italic,
        (true, false) => &SHAPER.bold,
        (false, true) => &SHAPER.italic,
        (false, false) => &SHAPER.normal,
    };
    let mut buffer = BUFFER.take();
    buffer.set_script(rustybuzz::script::LATIN);
    buffer.push_str(text);
    let buffer = rustybuzz::shape_with_plan(face, plan, buffer);
    let result = f(face, &buffer);
    BUFFER.set(buffer.clear());
    result
}

/// Creates an SVG element for an arc mark.
fn draw_arc<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    node.for_each_item().map(|ref node| {
        let x = propset.x.get(node).unwrap_or(0.0);
        let y = propset.y.get(node).unwrap_or(0.0);
        let start_angle = propset.start_angle.get(node).unwrap_or(0.0);
        let end_angle = propset.end_angle.get(node).unwrap_or(0.0);
        let inner_radius = propset.inner_radius.get(node).unwrap_or(0.0);
        let outer_radius = propset.outer_radius.get(node).unwrap_or(0.0);
        let path = arc_path(start_angle, end_angle, inner_radius, outer_radius);
        new_element("path", propset, node)
            .attr(n!("d"), path)
            .attr(
                n!("transform"),
                format!("translate({x} {y})", x = x.v(), y = y.v()),
            )
            .build()
    })
}

/// Creates an SVG element for an area mark.
fn draw_area<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    let orientation = propset.orient.get(node).unwrap_or_default();
    let interpolate = propset.interpolate.get(node).unwrap_or_default();
    let tension = propset.tension.get(node);

    let interpolate = interp::get(interpolate);
    let mut path = SvgPath::default();

    let mut forward = node.for_each_item().map(|node| {
        let x = propset.x.get(&node).unwrap_or(0.0);
        let y = propset.y.get(&node).unwrap_or(0.0);
        Vec2::new(x, y)
    });
    interpolate(&mut path, &mut forward, interp::Mode::AreaEven, tension);

    let backward: &mut dyn DoubleSizeIterator<Item = Vec2> = match orientation {
        Orient::Horizontal => &mut node.for_each_item().rev().map(|node| {
            let x = propset.x2.get(&node).unwrap_or(0.0);
            let y = propset.y.get(&node).unwrap_or(0.0);
            Vec2::new(x, y)
        }),
        Orient::Vertical => &mut node.for_each_item().rev().map(|node| {
            let x = propset.x.get(&node).unwrap_or(0.0);
            let y = propset.y2.get(&node).unwrap_or(0.0);
            Vec2::new(x, y)
        }),
    };

    interpolate(&mut path, backward, interp::Mode::AreaOdd, tension);

    core::iter::once(
        new_element("path", propset, node)
            .attr(n!("d"), path.finish())
            .build(),
    )
}

/// Creates an SVG element for a group mark.
fn draw_group<'s>(
    propset: &Propset<'s>,
    node: &Node<'s, '_>,
) -> impl Iterator<Item = Result<(Element, Rect)>> {
    let Some((MarkKind::Group(group), encoder)) = node
        .mark
        .as_ref()
        .map(|mark| (&mark.kind, mark.encoder.as_ref()))
    else {
        unreachable!()
    };

    node.for_each_item().map(move |ref node| {
        let propset = encoder.map_or(Cow::Borrowed(propset), |encoder| encoder(propset, node));
        let (x, width) = propset.x(node);
        let (y, height) = propset.y(node);

        let mut element = Element::builder("g", NS_SVG).attr(
            n!("transform"),
            format!("translate({x} {y})", x = x.v(), y = y.v()),
        );

        // TODO: This is still supposed to fill the group even if no
        // width/height is specified. In this case it is supposed to fill the
        // whole group according to its bounds.
        if width != 0.0 || height != 0.0 {
            // x/y do not go on this element so cannot use `draw_rect`
            let bg = new_element("rect", &propset, node)
                .attr(n!("width"), width.v())
                .attr(n!("height"), height.v())
                .build();
            element = element.append(bg);
        }

        if propset.clip.get(node) == Some(true) {
            element = element.attr(
                n!("clip-path"),
                format!("xywh(0 0 {w} {h})", w = width.v(), h = height.v()),
            );
        }

        let mut element = element.build();
        let (axis_bounds, box_bounds) = draw_container(&mut element, group, node)?;
        Ok((element, axis_bounds.union(&box_bounds)))
    })
}

/// Creates an SVG element for an image mark.
fn draw_image<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    node.for_each_item().filter_map(|ref node| {
        let bounds = image_dims(propset, node);
        propset.url.get(node).map(|href| {
            new_element("image", propset, node)
                .attr(
                    n!("transform"),
                    format!(
                        "translate({x} {y})",
                        x = bounds.left.v(),
                        y = bounds.top.v()
                    ),
                )
                .attr(n!("width"), bounds.width().v())
                .attr(n!("height"), bounds.height().v())
                .attr(n!("href"), href.to_string())
                .build()
        })
    })
}

/// Returns the bounding box of an image mark.
fn image_dims<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> Rect {
    let (x, width) = propset.x(node);
    let (y, height) = propset.y(node);
    let x = match propset.align.get(node) {
        None | Some(Align::Left) => x,
        Some(Align::Center) => x - width / 2.0,
        Some(Align::Right) => x - width,
    };
    let y = match propset.baseline.get(node) {
        None | Some(Baseline::Top | Baseline::Alphabetic) => y,
        Some(Baseline::Middle) => y - height / 2.0,
        Some(Baseline::Bottom) => y - height,
    };
    Rect::from_xywh(x, y, width, height)
}

/// Creates an SVG element for a line mark.
fn draw_line<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    let interpolate = propset.interpolate.get(node).unwrap_or_default();
    let tension = propset.tension.get(node);

    let mut points = node.for_each_item().map(|node| {
        let x = propset.x.get(&node).unwrap_or(0.0);
        let y = propset.y.get(&node).unwrap_or(0.0);
        Vec2::new(x, y)
    });

    let interpolate = interp::get(interpolate);
    let mut path = SvgPath::default();
    interpolate(&mut path, &mut points, interp::Mode::Line, tension);

    core::iter::once(
        new_element("path", propset, node)
            .attr(n!("d"), path.finish())
            .build(),
    )
}

/// Creates an SVG element for a path mark.
fn draw_path<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    node.for_each_item().filter_map(|ref node| {
        propset.path.get(node).map(|path| {
            let x = propset.x.get(node).unwrap_or(0.0);
            let y = propset.x.get(node).unwrap_or(0.0);

            let mut path = new_element("path", propset, node).attr(n!("d"), path.to_string());

            if x != 0.0 || y != 0.0 {
                path = path.attr(
                    n!("transform"),
                    format!("translate({x} {y})", x = x.v(), y = y.v()),
                );
            }

            path.build()
        })
    })
}

/// Creates an SVG element for a rect mark.
fn draw_rect<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    node.for_each_item().filter_map(|ref node| {
        let (x, width) = propset.x(node);
        let (y, height) = propset.y(node);

        // Although a dimensionless rect could still have a stroke, and thus could
        // have some visual output, Vega omits them from the scene
        (width > 0.0 && height > 0.0).then(|| {
            new_element("rect", propset, node)
                .attr(n!("x"), x.v())
                .attr(n!("y"), y.v())
                .attr(n!("width"), width.v())
                .attr(n!("height"), height.v())
                .build()
        })
    })
}

/// Creates an SVG element for a rule mark.
fn draw_rule<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    node.for_each_item().filter_map(|ref node| {
        let (x, width) = propset.x(node);
        let (y, height) = propset.y(node);

        (width > 0.0 || height > 0.0).then(|| {
            new_element("line", propset, node)
                .attr(n!("x1"), x.v())
                .attr(n!("x2"), (x + width).v())
                .attr(n!("y1"), y.v())
                .attr(n!("y2"), (y + height).v())
                .build()
        })
    })
}

/// Creates an SVG element for a symbol mark.
fn draw_symbol<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    node.for_each_item().map(|ref node| {
        let x = propset.x.get(node).unwrap_or(0.0);
        let y = propset.y.get(node).unwrap_or(0.0);
        // In Vega, the default size fallback does not apply to axis or legend,
        // except legend does the exact same thing itself, and axis does not use
        // this kind of mark.
        let size = propset.size.get(node).unwrap_or(defaults::SYMBOL_SIZE);
        let shape = propset.shape.get(node).unwrap_or_default();

        let mut path = SvgPath::default();
        match shape {
            Shape::Circle => {
                let r = (size / core::f64::consts::PI).sqrt();
                path.arc(Vec2::new(0.0, 0.0), r, 0.0, core::f64::consts::TAU, true)
                    .close();
            }
            Shape::Cross => {
                let r = (size / 5.0).sqrt() / 2.0;
                let d = 3.0 * r;
                path.move_to(Vec2::new(-d, -r))
                    .horizontal_to(-r)
                    .vertical_to(-d)
                    .horizontal_to(r)
                    .vertical_to(-r)
                    .horizontal_to(d)
                    .vertical_to(r)
                    .horizontal_to(r)
                    .vertical_to(d)
                    .horizontal_to(-r)
                    .vertical_to(r)
                    .horizontal_to(-d)
                    .close();
            }
            Shape::Diamond => {
                let tan_30 = 30.0_f64.to_radians().tan();
                let ry = (size / (2.0 * tan_30)).sqrt();
                let rx = ry * tan_30;
                path.move_to(Vec2::new(0.0, -ry))
                    .line_to(Vec2::new(rx, 0.0))
                    .line_to(Vec2::new(0.0, ry))
                    .line_to(Vec2::new(-rx, 0.0))
                    .close();
            }
            Shape::Square => {
                let r = size.sqrt() / 2.0;
                path.move_to(Vec2::new(-r, -r))
                    .line_to(Vec2::new(r, -r))
                    .line_to(Vec2::new(r, r))
                    .line_to(Vec2::new(-r, r))
                    .close();
            }
            Shape::TriangleDown | Shape::TriangleUp => {
                let sqrt_3 = 3.0_f64.sqrt();
                let sign = if shape == Shape::TriangleDown {
                    -1.0
                } else {
                    1.0
                };
                let rx = (size / sqrt_3).sqrt();
                let ry = sign * rx * sqrt_3 / 2.0;
                path.move_to(Vec2::new(0.0, -ry))
                    .line_to(Vec2::new(rx, ry))
                    .line_to(Vec2::new(-rx, ry))
                    .close();
            }
        }

        new_element("path", propset, node)
            .attr(n!("d"), path.finish())
            .attr(
                n!("transform"),
                format!("translate({x} {y})", x = x.v(), y = y.v()),
            )
            .build()
    })
}

/// Creates an SVG element for a text mark.
fn draw_text<'s>(propset: &Propset<'s>, node: &Node<'s, '_>) -> impl Iterator<Item = Element> {
    let encoder = node.mark.as_ref().and_then(|mark| mark.encoder.as_ref());
    node.for_each_item().filter_map(move |ref node| {
        let propset = encoder.map_or(Cow::Borrowed(propset), |encoder| encoder(propset, node));
        let text = propset
            .text
            .get(node)
            .and_then(|text| (!text.is_empty()).then_some(text))?;
        let TextMetrics {
            x,
            y,
            dx,
            mut dy,
            font_size,
        } = text_metrics(&propset, node);

        if let Some(baseline) = propset.baseline.get(node)
            && !matches!(baseline, Baseline::Alphabetic)
        {
            dy += font_size * baseline.adjustment();
        }

        let mut element = new_element("text", &propset, node).append(text.to_string());

        let transform = if let Some(angle) = propset.angle.get(node) {
            let mut transform = format!(
                "translate({x} {y}) rotate({angle})",
                x = x.v(),
                y = y.v(),
                angle = angle.v()
            );
            if dx != 0.0 || dy != 0.0 {
                let _ = write!(transform, " translate({dx} {dy})", dx = dx.v(), dy = dy.v());
            }
            transform
        } else {
            let x = x + dx;
            let y = y + dy;
            format!("translate({x} {y})", x = x.v(), y = y.v())
        };

        element = element
            .attr(n!("transform"), transform)
            .attr(n!("font-size"), font_size.v());

        Some(
            to_svg!(element, propset, node, {
                align => "text-anchor",
                font => "font-family",
                font_style => "font-style",
                font_weight => "font-weight"
            })
            .build(),
        )
    })
}

/// Creates an SVG element builder with common visual properties set.
fn new_element<'s>(name: &str, propset: &Propset<'s>, node: &Node<'s, '_>) -> ElementBuilder {
    let element = to_svg!(Element::builder(name, NS_SVG), propset, node, {
        cursor => "cursor",
        fill => "fill",
        fill_opacity => "fill-opacity",
        opacity => "opacity",
        stroke => "stroke",
        stroke_dash => "stroke-dasharray",
        stroke_dash_offset => "stroke-dashoffset",
        stroke_opacity => "stroke-opacity",
        stroke_width => "stroke-width",
    });

    if propset.fill.is_none() {
        element.attr(n!("fill"), "none".to_string())
    } else {
        element
    }
}
