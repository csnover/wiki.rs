//! Rendering functions for graph axes.

use super::{
    super::{
        Error, Node, Result, ScaleNode,
        axis::{Axis, Kind as AxisKind, Placement},
        data::ValueExt,
        mark::{Kind as MarkKind, Mark},
        propset::{Align, Baseline, FieldRef, Getter as _, Propset, ValueRefNumber},
        spec::Container,
    },
    Rect, defaults,
    format::make_formatter,
    mark::calculate_bounds,
    path::SvgPath,
};
use core::cmp::Ordering;
use serde_json_borrow::Value;
use std::borrow::Cow;

/// Converts an [`Axis`] to a renderable [`Mark`].
pub(super) fn axis_to_mark<'s>(axis: &Axis<'s>, node: &Node<'s, '_>) -> Result<(Mark<'s>, Rect)> {
    let Some(scale) = node.scale(&axis.scale) else {
        return Err(Error::AxisScale(axis.kind, axis.scale.to_string()));
    };

    // An x-axis with left/right placement, or a y-axis with top/bottom
    // placement, will render the scale and output range of the corresponding
    // axis but transposed
    let placement = axis.orient.unwrap_or(match axis.kind {
        AxisKind::X => Placement::Bottom,
        AxisKind::Y => Placement::Left,
    });

    let field_ref = {
        let offset = if scale.is_ordinal() {
            scale.range_band() / 2.0
        } else {
            0.0
        };
        ValueRefNumber::with_field_and_scale(
            "data",
            Some(axis.scale.clone()),
            None,
            Some(0.5 + offset),
        )
    };
    let offset = axis.offset.get(node);
    let (major_ticks, minor_ticks) = data(axis, scale)?;

    let mut marks = vec![];

    if axis.grid {
        let mut properties = axis.properties.grid.clone().unwrap_or_default();
        defaults::apply!(properties, {
            stroke => Axis::GRID_COLOR,
            stroke_opacity => Axis::GRID_OPACITY,
        });

        marks.push(ticks(
            properties,
            major_ticks.clone(),
            field_ref.clone(),
            placement,
            f64::INFINITY,
            Some(offset),
        ));
    }

    let tick_size = axis.tick_size.unwrap_or(Axis::TICK_SIZE);
    let tick_size_major = axis.tick_size_major.unwrap_or(tick_size);
    let common_ticks = axis.properties.ticks.as_ref();

    marks.push(ticks(
        merge_propsets(common_ticks, axis.properties.major_ticks.as_ref()),
        major_ticks.clone(),
        field_ref.clone(),
        placement,
        tick_size_major,
        None,
    ));

    if let Some(minor_ticks) = minor_ticks {
        marks.push(ticks(
            merge_propsets(common_ticks, axis.properties.minor_ticks.as_ref()),
            minor_ticks,
            field_ref.clone(),
            placement,
            axis.tick_size_minor.unwrap_or(tick_size),
            None,
        ));
    }

    let range = scale.range_extent();

    // Correct auto-positioning of the title requires knowing the bounds of the
    // axis domain and labels. Vega used a function call at display time to
    // mutate the title properties, but it is possible to just collect the
    // bounds immediately, which should be marginally faster according to no
    // evidence at all
    let title_edges = {
        let mut bounds = Rect::default();

        let labels = labels(
            axis.properties.labels.clone().unwrap_or_default(),
            major_ticks,
            field_ref.clone(),
            placement,
            tick_size_major,
            axis.tick_padding.unwrap_or(Axis::DEFAULT_PADDING),
        );

        // The axis title needs to know the dimensions of its siblings before it
        // can be created, but for `FieldRef::Group::level` and
        // `FieldGroup::Parent::level` to resolve to the correct parent
        // there has to be a placeholder where the group will eventually go.
        // Since axis children do not look up anything from the direct parent,
        // they can just be their own parents.
        let fake_group = node.with_child_mark(&labels);
        bounds = bounds.union(&calculate_bounds(&labels, &fake_group));

        marks.push(labels);

        let domain = domain(
            axis.properties.axis.clone().unwrap_or_default(),
            vec![Value::from([("data", 1.0)])],
            placement,
            range.0..range.1,
            axis.tick_size_end.unwrap_or(tick_size),
        );

        let fake_group = node.with_child_mark(&domain);
        bounds = bounds.union(&calculate_bounds(&domain, &fake_group));

        marks.push(domain);

        bounds
    };

    marks.push(title(
        axis.properties.title.clone().unwrap_or_default(),
        vec![Value::from([("data", axis.title.clone())])],
        node,
        placement,
        range.0..range.1,
        axis.title_offset,
        &title_edges,
    ));

    let group = container(marks, placement, offset);
    let bounds = calculate_bounds(&group, node);
    Ok((group, bounds))
}

/// Generates the group container mark for an axis.
fn container(marks: Vec<Mark<'_>>, placement: Placement, offset: f64) -> Mark<'_> {
    let mut properties = Propset::default();
    if placement.is_origin() {
        let value = Some((-offset).into());
        if placement == Placement::Top {
            properties.y = value;
        } else {
            properties.x = value;
        }
    } else {
        let group = if placement.is_vertical() {
            "height"
        } else {
            "width"
        };
        let value = Some(
            ValueRefNumber::with_field_and_scale(
                FieldRef::Group {
                    group: FieldRef::Field(group.into()).into(),
                    level: None,
                },
                None,
                None,
                Some(offset),
            )
            .into(),
        );
        if placement == Placement::Bottom {
            properties.y = value;
        } else {
            properties.x = value;
        }
    }

    #[rustfmt::skip]
    let kind = MarkKind::Group(Container { marks, ..Default::default() }.into());
    Mark::new(
        kind,
        Some("wiki-rs-graph-axis".into()),
        Some(vec![<_>::default()].into()),
        properties,
    )
}

/// Generates the data required to render tick marks.
fn data<'s>(
    axis: &Axis<'s>,
    scale: ScaleNode<'s, '_>,
) -> Result<(Vec<Value<'s>>, Option<Vec<Value<'s>>>)> {
    let major_ticks = if axis.values.is_empty() {
        let ticks = scale.ticks(axis.ticks.unwrap_or(Axis::DEFAULT_TICKS));
        Cow::Owned(ticks)
    } else {
        Cow::Borrowed(&axis.values)
    };

    let minor_ticks = axis.subdivide.map(|count| {
        if count.partial_cmp(&0.0) != Some(Ordering::Greater) {
            return vec![];
        }

        let [first, second, ..] = major_ticks.as_slice() else {
            return vec![];
        };

        let mut minor_ticks = vec![];
        // Clippy: There is no reasonable condition where there are >4B minor
        // ticks, and the value is checked for positivity earlier, and was found
        // to be reasonably happy.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let minor_count = count as u32;

        let (min, max) = scale.input_range();
        let distance = (second.to_f64() - first.to_f64()) / (count + 1.0);

        for major_tick in major_ticks.iter().map(ValueExt::to_f64) {
            for i in (1..minor_count).rev().map(f64::from) {
                let value = major_tick - i * distance;
                if value > min {
                    minor_ticks.push(Value::from([("data", value)]));
                }
            }
        }

        let last = major_ticks.last().unwrap().to_f64();
        for i in 1..minor_count {
            let value = last + f64::from(i) * distance;
            if value < max {
                minor_ticks.push(Value::from([("data", value)]));
            } else {
                break;
            }
        }

        minor_ticks
    });

    let formatter = make_formatter(scale, axis.format_kind, &axis.format, major_ticks.len())?;
    let major_ticks = major_ticks
        .iter()
        .map(|value| {
            let label = formatter(value);
            Value::from([("data", value.clone()), ("label", label.into())])
        })
        .collect::<Vec<_>>();

    Ok((major_ticks, minor_ticks))
}

/// Creates a mark to render the start and end ticks of an axis.
fn domain<'s>(
    mut properties: Propset<'s>,
    data: Vec<Value<'s>>,
    placement: Placement,
    range: core::ops::Range<f64>,
    size: f64,
) -> Mark<'s> {
    let size = if placement.is_origin() { -size } else { size };

    let mut path = SvgPath::default();
    if placement.is_vertical() {
        path.move_to((range.start, size).into())
            .vertical_to(0.0)
            .horizontal_to(range.end)
            .vertical_to(size);
    } else {
        path.move_to((size, range.start).into())
            .horizontal_to(0.0)
            .vertical_to(range.end)
            .horizontal_to(size);
    }
    let path = path.finish();

    defaults::apply!(properties, {
        stroke => Axis::AXIS_COLOR,
        stroke_width => Axis::AXIS_WIDTH,
        x => 0.5,
        y => 0.5,
        path => path,
    });

    Mark::new(
        MarkKind::Path,
        Some("wiki-rs-graph-axis-domain".into()),
        Some(data.into()),
        properties,
    )
}

/// Creates a mark to render the labels on the ticks of an axis.
fn labels<'s>(
    mut properties: Propset<'s>,
    data: Vec<Value<'s>>,
    field: ValueRefNumber<'s>,
    placement: Placement,
    size: f64,
    padding: f64,
) -> Mark<'s> {
    let sign = if placement.is_origin() { -1.0 } else { 1.0 };
    let size = sign * (size.max(0.0) + padding);

    defaults::apply!(properties, {
        fill => Axis::TICK_LABEL_COLOR,
        font => Axis::TICK_LABEL_FONT,
        font_size => Axis::TICK_LABEL_FONT_SIZE,
        text => FieldRef::Field("label".into()),
    });

    if placement.is_vertical() {
        defaults::apply!(properties, {
            x => field,
            y => size,
            align => Align::Center,
            baseline => if matches!(placement, Placement::Top) {
                Baseline::Bottom
            } else {
                Baseline::Top
            }
        });
    } else {
        defaults::apply!(properties, {
            x => size,
            y => field,
            align => if matches!(placement, Placement::Left) {
                Align::Right
            } else {
                Align::Left
            },
            baseline => Baseline::Middle,
        });
    }

    Mark::new(
        MarkKind::Text,
        Some("wiki-rs-graph-axis-labels".into()),
        Some(data.into()),
        properties,
    )
}

/// Creates a [`Propset`] by merging a `mixin` onto a `base`.
fn merge_propsets<'s>(base: Option<&Propset<'s>>, mixin: Option<&Propset<'s>>) -> Propset<'s> {
    match (base, mixin) {
        (Some(base), Some(mixin)) => base.merge(mixin),
        (Some(base), None) | (None, Some(base)) => base.clone(),
        (None, None) => <_>::default(),
    }
}

/// Creates a mark to render the ticks of an axis.
fn ticks<'s>(
    mut properties: Propset<'s>,
    data: Vec<Value<'s>>,
    field: ValueRefNumber<'s>,
    orient: Placement,
    size: f64,
    offset: Option<f64>,
) -> Mark<'s> {
    let sign = if orient.is_origin() { -1.0 } else { 1.0 };

    let size = if size == f64::INFINITY {
        #[rustfmt::skip]
        let group = if orient.is_vertical() { "height" } else { "width" };
        ValueRefNumber::with_field_and_scale(
            FieldRef::Group {
                group: FieldRef::Field(group.into()).into(),
                level: Some(2.0),
            },
            None,
            Some(-sign),
            offset.map(|offset| -sign * offset),
        )
    } else {
        ValueRefNumber::with_value(sign * size, None, offset)
    };

    defaults::apply!(properties, {
        stroke => Axis::TICK_COLOR,
        stroke_width => Axis::TICK_WIDTH,
    });

    if orient.is_vertical() {
        defaults::apply!(properties, {
            x => field,
            y => 0.0,
            y2 => size,
        });
    } else {
        defaults::apply!(properties, {
            x => 0.0,
            x2 => size,
            y => field,
        });
    }

    Mark::new(
        MarkKind::Rule,
        Some("wiki-rs-graph-axis-ticks".into()),
        Some(data.into()),
        properties,
    )
}

/// Creates a mark to render the title of an axis.
fn title<'s>(
    mut properties: Propset<'s>,
    data: Vec<Value<'s>>,
    node: &Node<'s, '_>,
    placement: Placement,
    range: core::ops::Range<f64>,
    offset: Option<f64>,
    bounds: &Rect,
) -> Mark<'s> {
    let sign = if placement.is_origin() { -1.0 } else { 1.0 };
    let mid = range.start.midpoint(range.end).floor();

    let dim = if placement.is_vertical() {
        bounds.height()
    } else {
        bounds.width()
    };

    let font_size = properties
        .font_size
        .get(node)
        .unwrap_or(defaults::TITLE_FONT_SIZE);
    let offset = if let Some(offset) = offset {
        offset.max(0.0)
    } else {
        (dim + font_size / 2.0 + Axis::TITLE_OFFSET_AUTO_MARGIN)
            .floor()
            .clamp(Axis::TITLE_OFFSET_AUTO_MIN, Axis::TITLE_OFFSET_AUTO_MAX)
    };

    defaults::apply!(properties, {
        align => Align::Center,
        baseline => Baseline::Middle,
        fill => defaults::TITLE_COLOR,
        font => defaults::TITLE_FONT,
        font_size => defaults::TITLE_FONT_SIZE,
        font_weight => defaults::TITLE_FONT_WEIGHT,
        text => FieldRef::Field("data".into()),
    });

    if placement.is_vertical() {
        defaults::apply!(properties, {
            x => mid,
            y => sign * offset,
            angle => 0.0,
        });
    } else {
        defaults::apply!(properties, {
            x => sign * offset,
            y => mid,
            angle => sign * 90.0,
        });
    }

    Mark::new(
        MarkKind::Text,
        Some("wiki-rs-graph-axis-title".into()),
        Some(data.into()),
        properties,
    )
}
