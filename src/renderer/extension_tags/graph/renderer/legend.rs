//! Rendering functions for graph legends.

use super::{
    super::{
        Error, Node, ScaleNode,
        data::ValueExt,
        legend::{Kind as LegendKind, Legend, Orient},
        mark::{Kind as MarkKind, Mark},
        propset::{
            Baseline, ColorProperty, FieldRef, Getter as _, Property, Propset, ValueRefNumber,
        },
        scale::{Kind as ScaleKind, Scale},
        spec::Container,
    },
    Rect, Result, defaults,
    format::make_formatter,
    mark::calculate_bounds,
};
use serde_json_borrow::Value;
use std::borrow::Cow;

/// Converts a [`Legend`] to a renderable [`Mark`].
pub(super) fn legend_to_mark<'s>(
    legend: &Legend<'s>,
    node: &Node<'s, '_>,
) -> Result<(Mark<'s>, Rect)> {
    let scale = legend
        .scale(node)
        .ok_or(Error::LegendScale(legend.scale_name().to_string()))?;

    let mark = if legend.is_color() && !scale.is_discrete() {
        quantitative_legend(legend, node, scale)
    } else {
        ordinal_legend(legend, node, scale)?
    };

    let bounds = calculate_bounds(&mark, node);
    Ok((mark, bounds))
}

/// Hard-coded padding around labels.
const PAD: f64 = 5.0;

/// Generates the group container mark for a legend.
fn container<'s>(legend: &Legend<'s>, container: Container<'s>) -> Mark<'s> {
    let mut properties = properties(&legend.properties.legend);

    // There is a lot of dead code in the Vega legend generator. Orient values
    // which are not valid according to the schema, a baseline taken from the
    // config file which is not set by default, transition stuff that is not
    // relevant here…

    let offset = legend.offset.unwrap_or(Legend::OFFSET);
    let x = if matches!(legend.orient, Orient::Left) {
        ValueRefNumber::with_value(offset, None, None)
    } else {
        ValueRefNumber::with_field_and_scale(
            FieldRef::Group {
                group: FieldRef::Field("width".into()).into(),
                level: None,
            },
            None,
            None,
            Some(offset),
        )
    };

    defaults::apply!(properties, {
        x => x,
        y => 0.5
    });

    Mark::new(
        MarkKind::Group(container.into()),
        None,
        Some(vec![<_>::default()].into()),
        properties,
    )
}

/// Calculates the y-coordinates of lines for a non-symbolic legend.
fn fixed_line_ys<'s>(
    legend: &Legend<'s>,
    node: &Node<'s, '_>,
    data: &[Value<'s>],
    y: f64,
) -> (Vec<Value<'s>>, f64) {
    // Vega only used font size if it was a fixed value and not a lookup.
    // This probably does not matter since it would be more broken their
    // way.
    let font_size = legend
        .properties
        .labels
        .as_ref()
        .and_then(|labels| labels.font_size.as_ref())
        .and_then(|font_size| font_size.get(node))
        .unwrap_or(Legend::LABEL_FONT_SIZE);

    let height = font_size + PAD;
    let offset = defaults::SYMBOL_SIZE.sqrt().round();
    // Clippy: If there are ever >=2**53 items, something sure happened.
    #[allow(clippy::cast_precision_loss)]
    let range = (0..data.len())
        .map(|index| Value::from(y + (offset / 2.0 + (index as f64) * height).round()));
    (range.collect(), offset)
}

/// Creates a mark to render a quantitative legend as a gradient.
fn gradient(mut properties: Propset<'_>) -> Mark<'_> {
    defaults::apply!(properties, {
        x => 0.0,
        y => 0.0,
        width => Legend::GRADIENT_WIDTH,
        height => Legend::GRADIENT_HEIGHT,
        stroke => Legend::GRADIENT_STROKE_COLOR,
        stroke_width => Legend::GRADIENT_STROKE_WIDTH,
    });

    Mark::new(MarkKind::Rect, None, None, properties)
}

/// Creates a mark to render the labels of a legend.
fn labels<'s>(mut properties: Propset<'s>, horizontal: bool, data: Vec<Value<'s>>) -> Mark<'s> {
    defaults::apply!(properties, {
        fill => Legend::LABEL_COLOR,
        font => Legend::LABEL_FONT,
        font_size => Legend::LABEL_FONT_SIZE,
        text => FieldRef::Field("label".into()),
    });

    if horizontal {
        defaults::apply!(properties, {
            align => Property::with_field_and_scale("align", None),
            baseline => Baseline::Top,
            dy => 2.0,
            x => ValueRefNumber::with_field_and_scale(
                "data",
                Some("legend".into()),
                None,
                None
            ),
            y => 20.0,
        });
    } else {
        defaults::apply!(properties, {
            align => Legend::LABEL_ALIGN,
            baseline => Legend::LABEL_BASELINE,
            x => ValueRefNumber::with_field_and_scale(
                "offset",
                None,
                None,
                Some(5.0 + Legend::PADDING + 1.0),
            ),
            y => ValueRefNumber::with_field_and_scale(
                "index",
                Some("legend".into()),
                None,
                None,
            ),
        });
    }

    Mark::new(MarkKind::Text, None, Some(data.into()), properties)
}

/// Creates an ordinal (categorical) legend.
fn ordinal_legend<'s>(
    legend: &Legend<'s>,
    node: &Node<'s, '_>,
    scale: ScaleNode<'s, '_>,
) -> Result<Mark<'s>> {
    let data = if legend.values.is_empty() {
        Cow::Owned(scale.ticks(5.0))
    } else {
        Cow::Borrowed(&legend.values)
    };

    // Clippy: If there are ever >=2**53 items, something sure happened.
    #[allow(clippy::cast_precision_loss)]
    let domain = vec![
        Value::from(0.0),
        Value::from(data.len().saturating_sub(1) as f64),
    ];

    let padding_start = Legend::PADDING
        + if legend.title.is_some() {
            // Vega only used font size if it was a fixed value and not a lookup.
            // This probably does not matter since it would be more broken their
            // way.
            PAD + legend
                .properties
                .title
                .as_ref()
                .and_then(|labels| labels.font_size.as_ref())
                .and_then(|font_size| font_size.get(node))
                .unwrap_or(defaults::TITLE_FONT_SIZE)
        } else {
            0.0
        };

    let (range, offset) = if legend.is_size() {
        symbolic_line_ys(scale, &data, padding_start)
    } else {
        fixed_line_ys(legend, node, &data, padding_start)
    };

    let formatter = make_formatter(scale, legend.format_kind, &legend.format, data.len())?;
    let data = data
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let label = formatter(value);
            // Clippy: If there are ever >=2**53 items, something sure happened.
            #[allow(clippy::cast_precision_loss)]
            Value::from([
                ("data", value.clone()),
                ("index", (index as f64).into()),
                ("label", label.into()),
                ("offset", offset.into()),
            ])
        })
        .collect::<Vec<_>>();

    let mut marks = vec![];
    if let Some(data) = legend.title.clone() {
        marks.push(title(properties(&legend.properties.title), data));
    }
    marks.push(symbols(
        properties(&legend.properties.symbols),
        legend.kind(),
        data.clone(),
    ));
    marks.push(labels(properties(&legend.properties.labels), false, data));

    let kind = Container {
        scales: vec![Scale::new(
            ScaleKind::Ordinal {
                band_size: None,
                outer_padding: None,
                padding: None,
                points: true,
            },
            "legend".into(),
            domain,
            range,
        )],
        marks,
        ..Default::default()
    };

    Ok(container(legend, kind))
}

/// Returns a clone of the given property set.
// Clippy: This is a convenience function to reduce line noise.
#[allow(clippy::ref_option)]
#[inline]
fn properties<'s>(propset: &Option<Propset<'s>>) -> Propset<'s> {
    propset.clone().unwrap_or_default()
}

/// Creates a legend for a quantitative scale.
fn quantitative_legend<'s>(
    _legend: &Legend<'s>,
    _node: &Node<'s, '_>,
    _scale: ScaleNode<'s, '_>,
) -> Mark<'s> {
    todo!()
}

/// Calculates the y-coordinates of lines for a symbolic legend.
fn symbolic_line_ys<'s>(
    scale: ScaleNode<'s, '_>,
    data: &[Value<'s>],
    y: f64,
) -> (Vec<Value<'s>>, f64) {
    let mut offset = f64::NEG_INFINITY;
    let mut last = None;
    let range = data
        .iter()
        .map(|value| {
            let value = scale.apply(value, false).into_f64().sqrt();
            offset = offset.max(value);

            let extra = if let Some((last, out_last)) = last {
                out_last + last / 2.0 + PAD
            } else {
                0.0
            };

            let new_value = extra + value / 2.0;
            last = Some((value, new_value));
            Value::from(y + new_value.round())
        })
        .collect::<Vec<_>>();
    (range, offset)
}

/// Creates a mark to render the symbols of a legend.
fn symbols<'s>(
    mut properties: Propset<'s>,
    kind: &LegendKind<'s>,
    data: Vec<Value<'s>>,
) -> Mark<'s> {
    match kind {
        LegendKind::Fill(name) => defaults::apply!(properties, {
            fill => ColorProperty::with_field_and_scale(
                "data",
                Some(name.clone())
            ),
        }),
        LegendKind::Opacity(name) => defaults::apply!(properties, {
            opacity => ValueRefNumber::with_field_and_scale(
                "data",
                Some(name.clone()),
                None,
                None
            )
        }),
        LegendKind::Shape(name) => defaults::apply!(properties, {
            shape => Property::with_field_and_scale("data", Some(name.clone())),
        }),
        LegendKind::Size(name) => defaults::apply!(properties, {
            size => ValueRefNumber::with_field_and_scale(
                "data",
                Some(name.clone()),
                None,
                None
            )
        }),
        LegendKind::Stroke(name) => defaults::apply!(properties, {
            stroke => ColorProperty::with_field_and_scale(
                "data",
                Some(name.clone()),
            )
        }),
    }

    defaults::apply!(properties, {
        shape => Legend::SYMBOL_SHAPE,
        size => defaults::SYMBOL_SIZE,
        stroke => Legend::SYMBOL_COLOR,
        stroke_width => Legend::SYMBOL_STROKE_WIDTH,
        x => ValueRefNumber::with_field_and_scale(
            FieldRef::Field("offset".into()),
            None,
            Some(0.5),
            Some(Legend::PADDING + 1.0),
        ),
        y => ValueRefNumber::with_field_and_scale(
            FieldRef::Field("index".into()),
            Some("legend".into()),
            None,
            None,
        )
    });

    Mark::new(MarkKind::Symbol, None, Some(data.into()), properties)
}

/// Creates a mark to render the title of a legend.
fn title<'s>(mut properties: Propset<'s>, title: Cow<'s, str>) -> Mark<'s> {
    defaults::apply!(properties, {
        baseline => Baseline::Top,
        fill => defaults::TITLE_COLOR,
        font => defaults::TITLE_FONT,
        font_size => defaults::TITLE_FONT_SIZE,
        font_weight => defaults::TITLE_FONT_WEIGHT,
        text => FieldRef::Field("data".into()),
        x => Legend::PADDING,
        y => Legend::PADDING
    });

    let data = vec![Value::from([("data", title)])];
    Mark::new(MarkKind::Text, None, Some(data.into()), properties)
}
