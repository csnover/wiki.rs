//! Types and functions for runtime data transformations.

use super::{
    super::svg::ValueDisplay as _,
    Node,
    data::{IgnoredAny, ValueExt, get_nested_value, unit_enum_to_str},
    expr::Ast,
    geo::{Projector, ProjectorSettings},
    make_extent,
    predicate::Signal,
    renderer::{Rect, Vec2},
    scale::quantile,
};
use core::{cmp::Ordering, f64::consts::PI};
use either::Either;
use indexmap::{IndexMap, IndexSet};
use rand::rngs::SmallRng;
use regex::{Regex, RegexBuilder};
use serde_json_borrow::Value;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

/// A data transformation.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum Transform<'s> {
    /// Computes aggregate summary statistics (e.g., median, min, max) over
    /// groups of data.
    #[serde(borrow)]
    Aggregate(Aggregate<'s>),
    /// Bins raw data values into quantitative bins (e.g., for a histogram).
    #[serde(borrow)]
    Bin(Bin<'s>),
    /// Counts the number of occurrences of a text pattern as defined by a
    /// regular expression.
    #[serde(borrow)]
    CountPattern(CountPattern<'s>),
    /// Compute the cross-product of two data sets.
    #[serde(borrow)]
    Cross(Cross<'s>),
    /// Organizes a data set into groups (facets).
    #[serde(borrow)]
    Facet(Facet<'s>),
    /// Removes unwanted items from a data set.
    #[serde(borrow)]
    Filter(Filter<'s>),
    /// Folds one or more data properties into two: a key property containing
    /// the original data property name, and a value property containing the
    /// data value.
    Fold(Todo),
    /// Performs force-directed layout for network data.
    #[serde(borrow)]
    Force(Force<'s>),
    /// Extends data elements with new values according to a calculation
    /// formula.
    #[serde(borrow)]
    Formula(Formula<'s>),
    /// Performs a cartographic projection.
    #[serde(borrow)]
    Geo(Geo<'s>),
    /// Creates paths for geographic regions, such as countries, states and
    /// counties, from GeoJSON features.
    #[serde(borrow)]
    Geopath(Geopath<'s>),
    /// Computes tidy, cluster, and partition layouts.
    Hierarchy(Todo),
    /// Performs imputation of missing values.
    Impute(Todo),
    /// Computes a path definition for connecting nodes within a node-link
    /// network or tree diagram.
    #[serde(borrow)]
    LinkPath(LinkPath<'s>),
    /// Extends a primary data set by looking up values on a secondary data set.
    ///
    /// In other words, performs a join that adds new properties onto the
    /// primary data set only.
    ///
    /// Lookup accepts one or more key values for the primary data set, each of
    /// which are then searched for within a single key field of the secondary
    /// data set. If a match is found, the full data object in the secondary
    /// data set is added as a property of the primary data set.
    #[serde(borrow)]
    Lookup(Lookup<'s>),
    /// Computes a pie chart layout.
    #[serde(borrow)]
    Pie(Pie<'s>),
    /// Computes an ascending rank score for data tuples based on their observed
    /// order and any key fields.
    Rank(Todo),
    /// Sorts the values of a data set.
    Sort(Todo),
    /// Computes layout values for stacked graphs, as in stacked bar charts or
    /// stream graphs.
    #[serde(borrow)]
    Stack(Stack<'s>),
    /// Computes a tree structure over a flat tabular dataset.
    Treeify(Todo),
    /// Computes a squarified treemap layout.
    #[serde(borrow)]
    Treemap(Treemap<'s>),
    /// Computes a voronoi diagram for a set of input seed points and returns
    /// the computed cell paths.
    Voronoi(Todo),
    /// Computes a word cloud layout, similar to Wordle (not that Wordle, the
    /// other Wordle).
    #[serde(borrow)]
    Wordcloud(Box<Wordcloud<'s>>),
}

impl<'s> Transform<'s> {
    /// Gets transformed data using the data set from `node`.
    #[must_use]
    pub fn transform(&self, node: &Node<'s, '_>, data: Cow<'_, [Value<'s>]>) -> Vec<Value<'s>> {
        match self {
            Transform::Aggregate(aggregate) => aggregate.transform(&data, None),
            Transform::Bin(bin) => bin.transform(data.into_owned()),
            Transform::CountPattern(count_pattern) => count_pattern.transform(&data),
            Transform::Cross(cross) => cross.transform(node, &data),
            Transform::Facet(facet) => facet.transform(node, &data),
            Transform::Filter(filter) => filter.transform(node, data.into_owned()),
            Transform::Fold(_) => todo!(),
            Transform::Force(force) => force.transform(node, data.into_owned()),
            Transform::Formula(formula) => formula.transform(node, data.into_owned()),
            Transform::Geo(geo) => geo.transform(data.into_owned()),
            Transform::Geopath(geopath) => geopath.transform(data.into_owned()),
            Transform::Hierarchy(_) => todo!(),
            Transform::Impute(_) => todo!(),
            Transform::LinkPath(link_path) => link_path.transform(data.into_owned()),
            Transform::Lookup(lookup) => lookup.transform(node, data.into_owned()),
            Transform::Pie(pie) => pie.transform(data.into_owned()),
            Transform::Rank(_) => todo!(),
            Transform::Sort(_) => todo!(),
            Transform::Stack(stack) => stack.transform(data.into_owned()),
            Transform::Treeify(_) => todo!(),
            Transform::Treemap(treemap) => treemap.transform(node, data.into_owned()),
            Transform::Voronoi(_) => todo!(),
            Transform::Wordcloud(wordcloud) => wordcloud.transform(node, data.into_owned()),
        }
    }
}

/// An accumulator for an aggregate operation on a collection of data.
pub(super) struct Accumulator<'s, 'b> {
    /// The argument containing the largest value.
    arg_max: Option<&'b Value<'s>>,
    /// The argument containing the smallest value.
    arg_min: Option<&'b Value<'s>>,
    /// The running sample deviation.
    dev: f64,
    /// The running set of distinct values.
    distinct: IndexSet<&'b Value<'s>>,
    /// A bit map of completed operations for the current item.
    finished_ops: u32,
    /// The running maximum value.
    max: f64,
    /// The running mean.
    mean: f64,
    /// The last mean calculation.
    mean_d: f64,
    /// The running minimum value.
    min: f64,
    /// The running count of missing/null items.
    missing: usize,
    /// The running sum.
    sum: f64,
    /// The running count of processed items.
    total: usize,
    /// The running count of items which are not missing, null, or NaN.
    valid: usize,
    /// The running list of collected values.
    values: Vec<&'b Value<'s>>,
}

impl Accumulator<'_, '_> {
    /// Resets the accumulator operator execution state for the next item.
    pub fn commit(&mut self) {
        self.finished_ops = 0;
    }
}

impl Default for Accumulator<'_, '_> {
    fn default() -> Self {
        Self {
            arg_max: <_>::default(),
            arg_min: <_>::default(),
            dev: <_>::default(),
            distinct: <_>::default(),
            finished_ops: <_>::default(),
            max: f64::NEG_INFINITY,
            mean: <_>::default(),
            mean_d: <_>::default(),
            min: f64::INFINITY,
            missing: <_>::default(),
            sum: <_>::default(),
            total: <_>::default(),
            valid: <_>::default(),
            values: <_>::default(),
        }
    }
}

/// A data aggregation transformer.
///
/// Emits a single item containing the requested summaries for each field.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Aggregate<'s> {
    /// The names of fields to optionally group by.
    #[serde(
        borrow,
        default,
        rename = "groupby",
        deserialize_with = "super::data::vec_or_str"
    )]
    group_by: Vec<Cow<'s, str>>,
    /// The aggregates to compute for each group.
    #[serde(borrow, default)]
    summarize: AggregateSpec<'s>,
}

/// A callback which transforms a list of value references into a list of
/// concrete values. Used by [`Facet`] to run transformations on subgroups.
type Facetor<'s, 'a> = &'a dyn Fn(&[&Value<'s>]) -> Vec<Value<'s>>;

impl<'s> Aggregate<'s> {
    /// Transforms the input data to a list of aggregates.
    #[must_use]
    fn transform(&self, data: &[Value<'s>], facetor: Option<Facetor<'s, '_>>) -> Vec<Value<'s>> {
        let faceting = facetor.is_some();
        // TODO: Aggregated unique keys are supposed to end up being sorted
        // lexicographically, or else the output ends up being different in e.g.
        // the force example.
        let mut rows = IndexMap::new();
        for item in data {
            let unique_key = self
                .group_by
                .iter()
                .map(|key| get_nested_value(item, key).unwrap_or(&Value::Null))
                .collect::<Vec<_>>();

            let acc = rows.entry(unique_key).or_default();

            for op in self.summarize.iter() {
                let value = if op.field == "*" {
                    item
                } else {
                    get_nested_value(item, op.field).unwrap_or(&Value::Null)
                };
                op.op.apply(acc, value, item);
            }

            if faceting {
                AggregateOp::Values.apply(acc, item, &Value::Null);
            }

            acc.commit();
        }

        rows.into_iter()
            .map(|(keys, acc)| {
                self.group_by
                    .iter()
                    .zip(keys)
                    .map(|(key, value)| {
                        // TODO: Not sure if something else should be done with
                        // missing/null here
                        (key.clone(), value.clone())
                    })
                    .chain(self.summarize.iter().filter_map(|op| {
                        (!faceting || !matches!(op.op, AggregateOp::Values))
                            .then(|| (op.out.clone(), op.op.finish(&acc)))
                    }))
                    .chain(facetor.map(|facetor| ("values".into(), facetor(&acc.values).into())))
                    .collect::<Value<'s>>()
            })
            .collect()
    }
}

/// A data aggregation operator.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub(super) enum AggregateOp {
    /// Find the input object that maximizes the value in a group.
    Argmax,
    /// Find the input object that minimizes the value in a group.
    Argmin,
    /// Count the total number of elements in the group.
    Count,
    /// Count the number of distinct values.
    Distinct,
    /// Compute the maximum value in a group.
    Max,
    /// Compute the mean (average) of values in a group.
    #[serde(alias = "average")]
    Mean,
    /// Compute the median of values in a group.
    Median,
    /// Compute the minimum value in a group.
    Min,
    /// Count the number of null or undefined values.
    Missing,
    /// Compute the mode skewness of values in a group.
    Modeskew,
    /// Compute the lower quartile boundary of values in a group.
    Q1,
    /// Compute the upper quartile boundary of values in a group.
    Q3,
    /// Compute the sample standard deviation of values in a group.
    Stdev,
    /// Compute the population standard deviation of values in a group.
    Stdevp,
    /// Compute the sum of values in a group.
    Sum,
    /// Count values that are not null, undefined or NaN.
    Valid,
    /// Build a list of all input objects in the group.
    Values,
    /// Compute the sample variance of values in a group.
    Variance,
    /// Compute the population variance of values in a group.
    Variancep,
}

impl AggregateOp {
    /// Processes one item, applying it to the accumulator.
    pub(super) fn apply<'b, 's>(
        self,
        acc: &mut Accumulator<'s, 'b>,
        value: &'b Value<'s>,
        parent: &'b Value<'s>,
    ) {
        if (acc.finished_ops & (1 << self as u8)) != 0 {
            return;
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "if there are ever ≥2**53 values, something sure happened"
        )]
        match self {
            Self::Argmax => {
                let value = value.as_f64().unwrap_or(0.0);
                if value > acc.max {
                    acc.arg_max = Some(parent);
                    acc.max = value;
                }
            }
            Self::Argmin => {
                let value = value.as_f64().unwrap_or(0.0);
                if value < acc.min {
                    acc.arg_min = Some(parent);
                    acc.min = value;
                }
            }
            Self::Count => acc.total += 1,
            Self::Distinct => {
                acc.distinct.insert(value);
            }
            Self::Max => acc.max = acc.max.max(value.as_f64().unwrap_or(0.0)),
            Self::Mean => {
                acc.mean_d = value.as_f64().unwrap_or(0.0) - acc.mean;
                acc.mean += acc.mean_d / acc.valid as f64;
            }
            Self::Median | Self::Q1 | Self::Q3 => {
                Self::Values.apply(acc, value, parent);
            }
            Self::Min => acc.min = acc.min.min(value.as_f64().unwrap_or(0.0)),
            Self::Missing => acc.missing += usize::from(value.is_null()),
            Self::Modeskew => {
                Self::Mean.apply(acc, value, parent);
                Self::Variance.apply(acc, value, parent);
                Self::Median.apply(acc, value, parent);
            }
            Self::Stdev | Self::Stdevp | Self::Variancep => {
                Self::Variance.apply(acc, value, parent);
            }
            Self::Sum => acc.sum += value.as_f64().unwrap_or(0.0),
            Self::Valid => {
                acc.valid +=
                    usize::from(!value.is_null() && value.as_f64().is_none_or(|n| !n.is_nan()));
            }
            Self::Values => acc.values.push(value),
            Self::Variance => {
                Self::Mean.apply(acc, value, parent);
                acc.dev += acc.mean_d * (value.as_f64().unwrap_or(0.0) - acc.mean);
            }
        }

        acc.finished_ops |= 1 << self as u8;
    }

    /// Finishes the processing of the operator, converting it to a final value.
    pub(super) fn finish<'s>(self, acc: &Accumulator<'s, '_>) -> Value<'s> {
        #[expect(
            clippy::cast_precision_loss,
            reason = "if there are ever ≥2**53 values, something sure happened"
        )]
        match self {
            Self::Argmax => acc.arg_max.cloned().unwrap_or_default(),
            Self::Argmin => acc.arg_min.cloned().unwrap_or_default(),
            Self::Count => Value::Number((acc.total as f64).into()),
            Self::Distinct => Value::Array(acc.distinct.iter().map(|v| (*v).clone()).collect()),
            Self::Max => Value::Number(acc.max.into()),
            Self::Mean => Value::Number(acc.mean.into()),
            Self::Median => Value::Number(quantile(acc.values.iter().copied(), 0.5).into()),
            Self::Min => Value::Number(acc.min.into()),
            Self::Missing => Value::Number((acc.missing as f64).into()),
            Self::Modeskew => Value::Number(
                if acc.dev == 0.0 {
                    0.0
                } else {
                    (acc.mean - quantile(acc.values.iter().copied(), 0.5))
                        / (acc.dev / (acc.valid - 1) as f64).sqrt()
                }
                .into(),
            ),
            Self::Stdev => Value::Number(
                if acc.valid > 1 {
                    (acc.dev / (acc.valid - 1) as f64).sqrt()
                } else {
                    0.0
                }
                .into(),
            ),
            Self::Stdevp => Value::Number(
                if acc.valid > 1 {
                    (acc.dev / acc.valid as f64).sqrt()
                } else {
                    0.0
                }
                .into(),
            ),
            Self::Sum => Value::Number(acc.sum.into()),
            Self::Q1 => Value::Number(quantile(acc.values.iter().copied(), 0.25).into()),
            Self::Q3 => Value::Number(quantile(acc.values.iter().copied(), 0.75).into()),
            Self::Valid => Value::Number((acc.valid as f64).into()),
            Self::Values => Value::Array(acc.values.iter().map(|v| (*v).clone()).collect()),
            Self::Variance => Value::Number(
                if acc.valid > 1 {
                    acc.dev / (acc.valid - 1) as f64
                } else {
                    0.0
                }
                .into(),
            ),
            Self::Variancep => Value::Number(
                if acc.valid > 1 {
                    acc.dev / acc.valid as f64
                } else {
                    0.0
                }
                .into(),
            ),
        }
    }

    /// Gets the output field name for the given input `field` name.
    #[must_use]
    #[inline]
    fn field_name(self, field: &str) -> String {
        if field == "*" {
            self.to_string()
        } else {
            format!("{self}_{field}")
        }
    }
}

impl core::fmt::Display for AggregateOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(unit_enum_to_str(*self))
    }
}

/// An aggregate specification.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum AggregateSpec<'s> {
    /// A short-hand specification.
    #[serde(borrow)]
    Short(IndexMap<Cow<'s, str>, AggregateSpecShort>),
    /// A long-hand specification.
    #[serde(borrow)]
    Long(Vec<AggregateSpecLong<'s>>),
}

impl<'s> AggregateSpec<'s> {
    /// Creates an iterator over all defined aggregate operations.
    fn iter(&self) -> impl Iterator<Item = AggregateSpecItem<'s, '_>> {
        match self {
            AggregateSpec::Short(kv) => Either::Left({
                kv.iter().flat_map(move |(k, ops)| {
                    ops.as_slice().iter().map(move |op| AggregateSpecItem {
                        field: k,
                        op: *op,
                        out: op.field_name(k).into(),
                    })
                })
            }),
            AggregateSpec::Long(v) => Either::Right({
                v.iter().flat_map(|op| {
                    let field = &op.field;
                    let mut names = op.output.iter();
                    op.ops.iter().map(move |op| AggregateSpecItem {
                        field,
                        op: *op,
                        out: names
                            .next()
                            .cloned()
                            .unwrap_or_else(|| op.field_name(field).into()),
                    })
                })
            }),
        }
    }
}

impl Default for AggregateSpec<'_> {
    fn default() -> Self {
        Self::Short(IndexMap::from([(
            "*".into(),
            AggregateSpecShort::Single(AggregateOp::Values),
        )]))
    }
}

/// A normalised aggregate spec item.
struct AggregateSpecItem<'s, 'b> {
    /// The name of the input field.
    field: &'b str,
    /// The operation to run.
    op: AggregateOp,
    /// The name of the output field.
    out: Cow<'s, str>,
}

/// A short-hand aggregate specification.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum AggregateSpecShort {
    /// A single aggregation.
    Single(AggregateOp),
    /// Multiple aggregations.
    Multiple(Vec<AggregateOp>),
}

impl AggregateSpecShort {
    /// Gets the aggregate specification as a slice.
    fn as_slice(&self) -> &[AggregateOp] {
        match self {
            AggregateSpecShort::Single(op) => core::slice::from_ref(op),
            AggregateSpecShort::Multiple(ops) => ops,
        }
    }
}

/// A long-hand aggregate specification.
#[derive(Debug, serde::Deserialize)]
struct AggregateSpecLong<'s> {
    /// The name of the field to aggregate.
    #[serde(borrow)]
    field: Cow<'s, str>,
    /// The aggregations.
    ops: Vec<AggregateOp>,
    /// The output field names to use for each aggregation. If unspecified, the
    /// output field names will be derived as `<input field name>_<operation>`.
    #[serde(borrow, default, rename = "as")]
    output: Vec<Cow<'s, str>>,
}

/// A data binning transformer.
///
/// Emits each input item with added bin fields.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Bin<'s> {
    /// The number base to use for automatic binning.
    #[serde(default = "Bin::default_base")]
    base: f64,
    /// Scale factors indicating allowable subdivisions.
    ///
    /// The default value is `[5, 2]`, which indicates that for base 10 numbers
    /// (the default base), the method may consider dividing bin sizes by 5
    /// and/or 2. For example, for an initial step size of 10, the method can
    /// check if bin sizes of 2 (= 10/5), 5 (= 10/2), or 1 (= 10/(5*2)) might
    /// also satisfy the given constraints.
    #[serde(default = "Bin::default_div")]
    div: Vec<f64>,
    /// The name of the field to bin values from.
    #[serde(borrow)]
    field: Cow<'s, str>,
    /// The minimum bin value to consider. If unspecified, the minimum value of
    /// the specified field is used.
    #[serde(default)]
    min: Option<f64>,
    /// The maximum bin value to consider. If unspecified, the maximum value of
    /// the specified field is used.
    #[serde(default)]
    max: Option<f64>,
    /// The maximum number of allowable bins.
    #[serde(default = "Bin::default_max_bins", rename = "maxbins")]
    max_bins: f64,
    /// The minimum allowable step size.
    #[serde(default, rename = "minstep")]
    min_step: Option<f64>,
    /// The output field names.
    #[serde(borrow, default)]
    output: BinOutput<'s>,
    /// The exact step size to use between bins. If provided, options such as
    /// [`Self::max_bins`] will be ignored.
    #[serde(default)]
    step: Option<f64>,
    /// The list of allowable step sizes to choose from.
    #[serde(default)]
    steps: Vec<f64>,
}

impl<'s> Bin<'s> {
    /// The default value for [`Self::base`].
    const fn default_base() -> f64 {
        10.0
    }

    /// The default value for [`Self::div`].
    fn default_div() -> Vec<f64> {
        vec![5.0, 2.0]
    }

    /// The default value for [`Self::max_bins`].
    const fn default_max_bins() -> f64 {
        20.0
    }

    /// Transforms the input data to add extra fields.
    fn transform(&self, data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let (min, max) = make_extent(
            self.min,
            self.max,
            data.iter()
                .filter_map(|item| get_nested_value(item, &self.field).map(ValueExt::to_f64)),
        );

        let bins = BinConfig::new(self, min, max);

        data.into_iter()
            .map(|mut item| {
                if let Some(value) = get_nested_value(&item, &self.field) {
                    let start = bins.value(value.to_f64());
                    item.insert(self.output.start.clone(), start);
                    item.insert(self.output.mid.clone(), start + bins.step / 2.0);
                    item.insert(self.output.end.clone(), start + bins.step);
                }
                item
            })
            .collect()
    }
}

/// A value bin.
struct BinConfig {
    /// The minimum value of the bin.
    min: f64,
    /// The bin width.
    step: f64,
}

impl BinConfig {
    /// Creates a new [`BinConfig`].
    fn new(options: &Bin<'_>, min: f64, max: f64) -> Self {
        let max_bins = if options.max_bins == 0.0 {
            15.0
        } else {
            options.max_bins
        };
        let base = if options.base == 0.0 {
            10.0
        } else {
            options.base
        };
        let base_ln = base.ln();
        let div = if options.div.is_empty() {
            &Bin::default_div()
        } else {
            &options.div
        };
        let span = max - min;

        let step = if let Some(step) = options.step {
            step
        } else if !options.steps.is_empty() {
            // Yes, datalib did not sort the data before using a binary search, so
            // good luck if the author did not sort it either
            let candidate = span / max_bins;
            let index = options
                .steps
                .binary_search_by(|value| value.total_cmp(&candidate))
                .unwrap_or_else(|index| index);
            options.steps[(options.steps.len() - 1).min(index)]
        } else {
            let level = (max_bins.ln() / base_ln).ceil();
            let minstep = options.min_step.unwrap_or(0.0);
            let mut step = minstep.max(base.powf((span.ln() / base_ln).round() - level));
            while (span / step).ceil() > max_bins {
                step *= base;
            }
            for v in div {
                let v = step / v;
                if v >= minstep && span / v <= max_bins {
                    step = v;
                }
            }
            step
        };

        let step_ln = step.ln();
        #[expect(clippy::cast_possible_truncation, reason = "intentional")]
        let precision = if step_ln >= 0.0 {
            0
        } else {
            (-step_ln / base_ln) as i32 + 1
        };
        let eps = base.powi(-precision - 1);
        let min = min.min((min / step + eps).floor() * step);

        Self { min, step }
    }

    /// Gets the binned value for the given `value`.
    #[inline]
    fn value(&self, value: f64) -> f64 {
        self.min + self.step * ((value - self.min) / self.step).floor()
    }
}

/// Bin output field names.
#[derive(Debug, serde::Deserialize)]
struct BinOutput<'s> {
    /// The name of the end field.
    #[serde(default = "BinOutput::default_end")]
    end: Cow<'s, str>,
    /// The name of the mid field.
    #[serde(default = "BinOutput::default_mid")]
    mid: Cow<'s, str>,
    /// The name of the start field.
    #[serde(default = "BinOutput::default_start")]
    start: Cow<'s, str>,
}

impl BinOutput<'_> {
    /// The default value for [`Self::end`].
    const fn default_end() -> Cow<'static, str> {
        Cow::Borrowed("bin_end")
    }

    /// The default value for [`Self::end`].
    const fn default_mid() -> Cow<'static, str> {
        Cow::Borrowed("bin_mid")
    }

    /// The default value for [`Self::start`].
    const fn default_start() -> Cow<'static, str> {
        Cow::Borrowed("bin_start")
    }
}

impl Default for BinOutput<'_> {
    fn default() -> Self {
        Self {
            end: Self::default_end(),
            mid: Self::default_mid(),
            start: Self::default_start(),
        }
    }
}

/// A text pattern match count transformer.
///
/// For each match of the pattern, emits a kv-pair containing the matched string
/// and the number of times the string was matched.
#[derive(Debug, serde::Deserialize)]
pub(super) struct CountPattern<'s> {
    /// The case folding to perform before matching.
    #[serde(default)]
    case: CountPatternCase,
    /// The name of the field to match.
    #[serde(borrow, default = "CountPattern::default_field")]
    field: Cow<'s, str>,
    /// The output field names.
    #[serde(borrow, default)]
    output: CountPatternOutput<'s>,
    /// The regular expression to match.
    #[serde(borrow, default = "CountPattern::default_pattern")]
    pattern: Cow<'s, str>,
    /// The regular expression for discarding matches.
    #[serde(borrow, default, rename = "stopwords")]
    stop_words: Cow<'s, str>,
}

impl<'s> CountPattern<'s> {
    /// The default value of [`Self::field`].
    const fn default_field() -> Cow<'static, str> {
        Cow::Borrowed("data")
    }

    /// The default value of [`Self::pattern`].
    const fn default_pattern() -> Cow<'static, str> {
        Cow::Borrowed(r"[\w']+")
    }

    /// Transforms the input data to a list of text matches.
    fn transform(&self, data: &[Value<'s>]) -> Vec<Value<'s>> {
        let re = Regex::new(&self.pattern).unwrap();
        let stop = RegexBuilder::new(&format!("^{}$", self.stop_words))
            .case_insensitive(true)
            .build()
            .unwrap();

        let mut counts = IndexMap::new();

        for value in data
            .iter()
            .filter_map(|item| get_nested_value(item, &self.field))
        {
            let value = ValueExt::to_string(value);
            let value = match self.case {
                CountPatternCase::Lower => Cow::Owned(value.to_lowercase()),
                CountPatternCase::Upper => Cow::Owned(value.to_uppercase()),
                CountPatternCase::None => value,
            };
            for token in re.find_iter(&value) {
                let key = token.as_str();
                if !stop.is_match(key) {
                    let count = counts.entry(key.to_owned()).or_insert(0_u32);
                    *count += 1;
                }
            }
        }

        counts
            .into_iter()
            .map(|(key, count)| {
                Value::from([
                    (self.output.count.clone(), f64::from(count).into()),
                    (self.output.text.clone(), Value::from(key)),
                ])
            })
            .collect()
    }
}

/// A case folding operation.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CountPatternCase {
    /// Convert to lowercase.
    #[default]
    Lower,
    /// Convert to uppercase.
    Upper,
    /// Do not convert.
    None,
}

/// Count pattern output field names.
#[derive(Debug, serde::Deserialize)]
struct CountPatternOutput<'s> {
    /// The name of the count field.
    #[serde(borrow, default = "CountPatternOutput::default_count")]
    count: Cow<'s, str>,
    /// The name of the matched text field.
    #[serde(borrow, default = "CountPatternOutput::default_text")]
    text: Cow<'s, str>,
}

impl CountPatternOutput<'_> {
    /// The default value for [`Self::count`].
    const fn default_count() -> Cow<'static, str> {
        Cow::Borrowed("count")
    }

    /// The default value for [`Self::text`].
    const fn default_text() -> Cow<'static, str> {
        Cow::Borrowed("text")
    }
}

impl Default for CountPatternOutput<'_> {
    fn default() -> Self {
        Self {
            count: Self::default_count(),
            text: Self::default_text(),
        }
    }
}

/// A cross-product transformer.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Cross<'s> {
    /// If false, items along the “diagonal” of the cross-product (those
    /// elements with the same index in their respective array) will not be
    /// included in the output. This parameter only applies when a dataset is
    /// crossed with itself. Defaults to `true`.
    #[serde(default = "Cross::default_diagonal")]
    diagonal: bool,
    /// An optional filtering predicate to apply to each cross-product data
    /// object.
    #[serde(default)]
    filter: Cow<'s, str>,
    /// The output field names.
    #[serde(borrow, default)]
    output: CrossOutput<'s>,
    /// The name of the secondary data set to cross with the primary data.
    /// If not specified, the primary data set is crossed with itself.
    #[serde(default)]
    with: Option<Cow<'s, str>>,
}

impl<'s> Cross<'s> {
    /// The default value for [`Self::diagonal`].
    const fn default_diagonal() -> bool {
        true
    }

    /// Transforms the input data into a list of cross-products.
    fn transform(&self, node: &Node<'s, '_>, data: &[Value<'s>]) -> Vec<Value<'s>> {
        let filter = if self.filter.is_empty() {
            None
        } else {
            Some(Ast::new(&self.filter).unwrap())
        };

        let cross = if let Some(with) = &self.with {
            node.data_values(with).unwrap_or_default()
        } else {
            data
        };

        let skip_diagonal = !self.diagonal && self.with.is_none();
        let mut out = if filter.is_none() {
            let data_len = data.len().saturating_sub(usize::from(skip_diagonal));
            Vec::with_capacity(data_len * cross.len())
        } else {
            vec![]
        };

        for (a_index, a) in data.iter().enumerate() {
            for (b_index, b) in cross.iter().enumerate() {
                let item = Value::Object(
                    [
                        (self.output.left.clone(), a.clone()),
                        (self.output.right.clone(), b.clone()),
                    ]
                    .into(),
                );

                if (!skip_diagonal || a_index != b_index)
                    && filter.as_ref().is_none_or(|filter| {
                        filter.eval(&node.with_child_data(&item)).unwrap().to_bool()
                    })
                {
                    out.push(item);
                }
            }
        }

        out
    }
}

/// Cross output field names.
#[derive(Debug, serde::Deserialize)]
struct CrossOutput<'s> {
    /// The name of the left field.
    #[serde(borrow, default = "CrossOutput::default_left")]
    left: Cow<'s, str>,
    /// The name of the right field.
    #[serde(borrow, default = "CrossOutput::default_right")]
    right: Cow<'s, str>,
}

impl CrossOutput<'_> {
    /// The default value for [`Self::left`].
    const fn default_left() -> Cow<'static, str> {
        Cow::Borrowed("a")
    }

    /// The default value for [`Self::right`].
    const fn default_right() -> Cow<'static, str> {
        Cow::Borrowed("b")
    }
}

impl Default for CrossOutput<'_> {
    fn default() -> Self {
        Self {
            left: Self::default_left(),
            right: Self::default_right(),
        }
    }
}

/// A facet transformer.
///
/// A facet transformer works like [`Aggregate`], except that it always runs the
/// [`AggregateOp::Values`] operator, then runs [transforms](Self::transform)
/// against each group of collected values.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Facet<'s> {
    /// The aggregations.
    #[serde(borrow, flatten)]
    aggregate: Aggregate<'s>,
    /// The transforms to run on each grouped set of aggregated values.
    #[serde(borrow, default)]
    transform: Vec<Transform<'s>>,
}

impl<'s> Facet<'s> {
    /// Runs a list of transformers against a facet subgroup.
    fn facetor<'a>(&'a self, node: &Node<'s, '_>, data: &'a [&Value<'s>]) -> Vec<Value<'s>> {
        let mut data = data.iter().map(|item| (*item).clone()).collect::<Vec<_>>();
        for transform in &self.transform {
            data = transform.transform(node, Cow::Owned(data));
        }
        data
    }

    /// Transforms the input data into a list of facets.
    fn transform(&self, node: &Node<'s, '_>, data: &[Value<'s>]) -> Vec<Value<'s>> {
        self.aggregate
            .transform(data, Some(&|data| self.facetor(node, data)))
            .into_iter()
            .map(|mut item| {
                let key = make_key_string(&item, &self.aggregate.group_by);
                item.insert("key", key);
                item
            })
            .collect()
    }
}

/// A static value or field reference.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum FieldRef<'s, T> {
    /// A literal value.
    Value(T),
    /// A named field reference shorthand.
    #[serde(borrow, untagged)]
    Short(Cow<'s, str>),
    /// A named field reference.
    #[serde(untagged)]
    Long {
        /// A named field reference.
        #[serde(borrow)]
        field: Cow<'s, str>,
    },
}

impl<'s> FieldRef<'s, Cow<'s, str>> {
    /// Gets the value.
    fn get(&self, item: &Value<'s>) -> Cow<'s, str> {
        match self {
            FieldRef::Value(value) => value.clone(),
            FieldRef::Short(field) | FieldRef::Long { field } => {
                get_nested_value(item, field).map_or(<_>::default(), ValueExt::to_string)
            }
        }
    }
}

impl<'s> FieldRef<'s, f64> {
    /// Gets the value.
    fn get(&self, item: &Value<'s>) -> f64 {
        match self {
            FieldRef::Value(value) => *value,
            FieldRef::Short(field) | FieldRef::Long { field } => {
                get_nested_value(item, field).map_or(<_>::default(), ValueExt::to_f64)
            }
        }
    }
}

/// A force-directed layout transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(bound = "'s: 'de")]
pub(super) struct Force<'s> {
    /// A signal value with information about a node that is being interacted
    /// with (e.g., dragged), if interaction were a thing, which it is not.
    #[serde(borrow, default, rename = "active")]
    _active: IgnoredAny<Option<Signal<'s>>>,
    /// A “temperature” parameter that determines how much node positions are
    /// adjusted at each step.
    #[serde(default = "Force::default_alpha")]
    alpha: f64,
    /// The strength of the charge each node exerts.
    #[serde(borrow, default = "Force::default_charge")]
    charge: ForceFieldRef<'s>,
    /// Undocumented.
    #[serde(default)]
    charge_distance: Option<f64>,
    /// The name of a data set containing nodes whose layout should be fixed.
    #[serde(borrow, default)]
    fixed: Option<Cow<'s, str>>,
    /// The strength of the friction force used to stabilize the layout.
    #[serde(default = "Force::default_friction")]
    friction: f64,
    /// The strength of the pseudo-gravity force that pulls nodes towards the
    /// center of the layout area.
    #[serde(default = "Force::default_gravity")]
    gravity: f64,
    /// It is never interactive, but we can pretend!
    #[serde(default, rename = "interactive")]
    _interactive: bool,
    /// The number of iterations to run the force directed layout when
    /// [`Self::interactive`] is false.
    #[serde(default = "Force::default_iterations")]
    iterations: f64,
    /// The length of edges, in pixels.
    #[serde(borrow, default = "Force::default_link_distance")]
    link_distance: ForceFieldRef<'s>,
    /// The tension of edges (the spring constant).
    #[serde(borrow, default = "Force::default_link_strength")]
    link_strength: ForceFieldRef<'s>,
    /// The name of the link (edge) data set.
    #[serde(borrow)]
    links: Cow<'s, str>,
    /// The output field names.
    #[serde(borrow, default)]
    output: XyOutput<'s>,
    /// The dimensions of this force layout, in pixels.
    #[serde(default)]
    size: Option<[f64; 2]>,
    /// The theta parameter for the Barnes-Hut algorithm, which is used to
    /// compute charge forces between nodes.
    #[serde(default = "Force::default_theta")]
    theta: f64,
}

impl<'s> Force<'s> {
    /// The default value for [`Self::alpha`].
    const fn default_alpha() -> f64 {
        0.1
    }

    /// The default value for [`Self::charge`].
    const fn default_charge() -> ForceFieldRef<'s> {
        ForceFieldRef::Value(-30.0)
    }

    /// The default value for [`Self::friction`].
    const fn default_friction() -> f64 {
        0.9
    }

    /// The default value for [`Self::gravity`].
    const fn default_gravity() -> f64 {
        0.1
    }

    /// The default value for [`Self::iterations`].
    const fn default_iterations() -> f64 {
        500.0
    }

    /// The default value for [`Self::link_distance`].
    const fn default_link_distance() -> ForceFieldRef<'s> {
        ForceFieldRef::Value(20.0)
    }

    /// The default value for [`Self::link_strength`].
    const fn default_link_strength() -> ForceFieldRef<'s> {
        ForceFieldRef::Value(1.0)
    }

    /// The default value for [`Self::iterations`].
    const fn default_theta() -> f64 {
        0.8
    }

    /// Transforms the input data to add extra fields.
    fn transform(&self, node: &Node<'s, '_>, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let fixed = if let Some(fixed) = self.fixed.as_ref().and_then(|name| node.data_values(name))
        {
            fixed
                .iter()
                .map(|item| item.get("_id").and_then(Value::as_u64))
                .collect::<HashSet<_>>()
        } else {
            <_>::default()
        };

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the input data is expected to be an integer index into the list of nodes"
        )]
        let links = node
            .data_values(&self.links)
            .unwrap_or_default()
            .iter()
            .map(|link| {
                let source = link.get("source").map_or(0.0, ValueExt::to_f64) as usize;
                let target = link.get("target").map_or(0.0, ValueExt::to_f64) as usize;
                assert!(source < data.len() && target < data.len());
                force::Link {
                    distance: self.link_distance.get(link),
                    source,
                    strength: self.link_strength.get(link),
                    target,
                }
            })
            .collect::<Vec<_>>();

        let [width, height] = self.size.unwrap_or_else(|| [node.width(), node.height()]);

        let mut rng = node.rng.borrow_mut();
        let force = force::Force::new(
            &mut rng,
            self,
            Vec2::new(width, height),
            &data,
            |item| fixed.contains(&item.get("_id").and_then(Value::as_u64)),
            links,
        );

        for (index, point) in force.run_to_finish().enumerate() {
            data[index].insert(self.output.x.clone(), point.x);
            data[index].insert(self.output.y.clone(), point.y);
        }

        data
    }
}

/// A static value or field reference, which is like [`FieldRef`] except
/// incompatible.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ForceFieldRef<'s> {
    /// A literal value.
    Value(f64),
    /// A field reference.
    #[serde(borrow)]
    Field(Cow<'s, str>),
}

impl<'s> ForceFieldRef<'s> {
    /// Gets the value.
    fn get(&self, item: &Value<'s>) -> f64 {
        match self {
            ForceFieldRef::Value(value) => *value,
            ForceFieldRef::Field(field) => {
                get_nested_value(item, field).map_or(<_>::default(), ValueExt::to_f64)
            }
        }
    }
}

/// A data filter.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Filter<'s> {
    /// A string containing a Vega expression. If the expression evaluates to
    /// `false`, the item is filtered.
    #[serde(borrow)]
    test: Cow<'s, str>,
}

impl<'s> Filter<'s> {
    /// Transforms the input data to remove items.
    fn transform(&self, node: &Node<'s, '_>, data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let ast = Ast::new(&self.test).unwrap();
        data.into_iter()
            .filter(|item| {
                let node = node.with_child_data(item);
                ast.eval(&node).unwrap().into_bool()
            })
            .collect()
    }
}

/// A Vega expression transformer.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Formula<'s> {
    /// The expression to invoke for each data item.
    #[serde(borrow)]
    expr: Cow<'s, str>,
    /// The output field name.
    #[serde(borrow)]
    field: Cow<'s, str>,
}

impl<'s> Formula<'s> {
    /// Transforms the input data to add extra fields.
    fn transform(&self, node: &Node<'s, '_>, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let ast = Ast::new(&self.expr).unwrap();

        for item in &mut data {
            let node = node.with_child_data(item);
            let value = ast.eval(&node).unwrap();
            item.insert(self.field.clone(), value);
        }

        data
    }
}

/// A cartographic projection transformer.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Geo<'s> {
    /// The base projection options.
    #[serde(flatten)]
    base: ProjectorSettings,
    /// The field containing the input latitude values.
    #[serde(borrow)]
    lat: Cow<'s, str>,
    /// The field containing the input longitude values.
    #[serde(borrow)]
    lon: Cow<'s, str>,
    /// The output field names.
    #[serde(borrow, default)]
    output: XyOutput<'s>,
}

impl<'s> Geo<'s> {
    /// Transforms the input data to add extra fields.
    fn transform(&self, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let projector = Projector::new(&self.base);
        for item in &mut data {
            if let Some((lat, lon)) = item.get(&self.lat).zip(item.get(&self.lon)) {
                let point = projector.projection_degrees(Vec2::new(lat.to_f64(), lon.to_f64()));
                item.insert(self.output.x.clone(), point.x);
                item.insert(self.output.y.clone(), point.y);
            }
        }
        data
    }
}

/// A geographical region path builder transformer.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Geopath<'s> {
    /// The base projection options.
    #[serde(flatten)]
    base: ProjectorSettings,
    /// The data field containing GeoJSON feature data.
    #[serde(borrow, default)]
    field: Option<Cow<'s, str>>,
    /// The output field names.
    #[serde(borrow, default)]
    output: GeopathOutput<'s>,
}

impl<'s> Geopath<'s> {
    /// Transforms the input data to add extra fields.
    fn transform(&self, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let projector = Projector::new(&self.base);
        for item in &mut data {
            let object = if let Some(field) = &self.field {
                item.get(field)
            } else {
                Some(&*item)
            };
            if let Some(object) = object {
                let path = projector.path(object);
                item.insert(self.output.path.clone(), path);
            }
        }
        data
    }
}

/// Geopath output field names.
#[derive(Debug, serde::Deserialize)]
struct GeopathOutput<'s> {
    /// The name of the path field.
    #[serde(borrow, default = "GeopathOutput::default_path")]
    path: Cow<'s, str>,
}

impl GeopathOutput<'_> {
    /// The default value for [`Self::path`].
    const fn default_path() -> Cow<'static, str> {
        Cow::Borrowed("layout_path")
    }
}

impl Default for GeopathOutput<'_> {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
        }
    }
}

/// A pie chart layout transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Pie<'s> {
    /// An ending angle, in radians, for angular span calculations (default τ).
    #[serde(default = "Pie::default_end_angle")]
    end_angle: f64,
    /// The data values from this field will be encoded as angular spans. If
    /// this property is omitted, all pie slices will have equal spans.
    #[serde(borrow, default)]
    field: Option<Cow<'s, str>>,
    /// The output field names.
    #[serde(borrow, default)]
    output: LayoutOutput<'s>,
    /// If true, will sort the data prior to computing angles.
    #[serde(default)]
    sort: bool,
    /// A starting angle, in radians, for angular span calculations (default 0).
    #[serde(default)]
    start_angle: f64,
}

impl<'s> Pie<'s> {
    /// The default value for [`Self::end_angle`].
    const fn default_end_angle() -> f64 {
        core::f64::consts::TAU
    }

    /// Transforms the input data to add extra fields.
    fn transform(&self, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let mut accessor: &mut dyn FnMut(&Value<'s>) -> f64 = if let Some(name) = &self.field {
            &mut move |item| {
                get_nested_value(item, name)
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            }
        } else {
            &mut |_: &Value<'s>| 1.0
        };

        let sum: f64 = data.iter().map(&mut accessor).sum();

        let mut a = self.start_angle;
        let k = (self.end_angle - self.start_angle) / sum;

        if self.sort {
            data.sort_unstable_by(|a, b| accessor(a).total_cmp(&accessor(b)));
        }

        data.into_iter()
            .map(|mut item| {
                let v = accessor(&item);
                item.insert(self.output.start.clone(), a);
                item.insert(self.output.mid.clone(), a + 0.5 * v * k);
                a += v * k;
                item.insert(self.output.end.clone(), a);
                item
            })
            .collect()
    }
}

/// Pie and stack output field names.
#[derive(Debug, serde::Deserialize)]
struct LayoutOutput<'s> {
    /// The name of the end field.
    #[serde(borrow, default = "LayoutOutput::default_end")]
    end: Cow<'s, str>,
    /// The name of the mid field.
    #[serde(borrow, default = "LayoutOutput::default_mid")]
    mid: Cow<'s, str>,
    /// The name of the start field.
    #[serde(borrow, default = "LayoutOutput::default_start")]
    start: Cow<'s, str>,
}

impl LayoutOutput<'_> {
    /// The default value for [`Self::end`].
    const fn default_end() -> Cow<'static, str> {
        Cow::Borrowed("layout_end")
    }

    /// The default value for [`Self::mid`].
    const fn default_mid() -> Cow<'static, str> {
        Cow::Borrowed("layout_mid")
    }

    /// The default value for [`Self::start`].
    const fn default_start() -> Cow<'static, str> {
        Cow::Borrowed("layout_start")
    }
}

impl Default for LayoutOutput<'_> {
    fn default() -> Self {
        Self {
            end: Self::default_end(),
            mid: Self::default_mid(),
            start: Self::default_start(),
        }
    }
}

/// A lookup data transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LinkPath<'s> {
    /// The output field names.
    #[serde(borrow, default)]
    output: LinkPathOutput<'s>,
    /// The path shape to use.
    #[serde(default)]
    shape: LinkPathShape,
    /// The data field for the first x-coordinate.
    #[serde(borrow, default = "LinkPath::default_source_x")]
    source_x: Cow<'s, str>,
    /// The data field for the first y-coordinate.
    #[serde(borrow, default = "LinkPath::default_source_y")]
    source_y: Cow<'s, str>,
    /// The data field for the second x-coordinate.
    #[serde(borrow, default = "LinkPath::default_target_x")]
    target_x: Cow<'s, str>,
    /// The data field for the second y-coordinate.
    #[serde(borrow, default = "LinkPath::default_target_y")]
    target_y: Cow<'s, str>,
    /// The tightness of curved shapes, in the range `0..=1`.
    #[serde(default = "LinkPath::default_tension")]
    tension: f64,
}

impl<'s> LinkPath<'s> {
    /// The default value for [`Self::source_x`].
    const fn default_source_x() -> Cow<'static, str> {
        Cow::Borrowed("_source.layout_x")
    }

    /// The default value for [`Self::source_y`].
    const fn default_source_y() -> Cow<'static, str> {
        Cow::Borrowed("_source.layout_y")
    }

    /// The default value for [`Self::target_x`].
    const fn default_target_x() -> Cow<'static, str> {
        Cow::Borrowed("_target.layout_x")
    }

    /// The default value for [`Self::target_y`].
    const fn default_target_y() -> Cow<'static, str> {
        Cow::Borrowed("_target.layout_y")
    }

    /// The default value for [`Self::tension`].
    const fn default_tension() -> f64 {
        0.2
    }

    /// Transforms the input data to add extra fields.
    fn transform(&self, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        for item in &mut data {
            let x0 = get_nested_value(item, &self.source_x).map_or(0.0, ValueExt::to_f64);
            let y0 = get_nested_value(item, &self.source_y).map_or(0.0, ValueExt::to_f64);
            let x1 = get_nested_value(item, &self.target_x).map_or(0.0, ValueExt::to_f64);
            let y1 = get_nested_value(item, &self.target_y).map_or(0.0, ValueExt::to_f64);
            let path = self.shape.draw(x0, y0, x1, y1, self.tension);
            item.insert(self.output.path.clone(), path);
        }
        data
    }
}

/// Link path output field names.
#[derive(Debug, serde::Deserialize)]
struct LinkPathOutput<'s> {
    /// The name of the path field.
    #[serde(borrow, default = "LinkPathOutput::default_path")]
    path: Cow<'s, str>,
}

impl LinkPathOutput<'_> {
    /// The default value for [`Self::path`].
    const fn default_path() -> Cow<'static, str> {
        Cow::Borrowed("layout_path")
    }
}

impl Default for LinkPathOutput<'_> {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
        }
    }
}

/// A link path shape.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum LinkPathShape {
    /// Straight line.
    #[default]
    Line,
    /// Cubic Bézier curve.
    Curve,
    /// Right angle, vertical then horizontal.
    CornerX,
    /// Right angle, horizontal then vertical.
    CornerY,
    /// Rounded corner.
    CornerR,
    /// Diagonal curve, vertical then horizontal.
    DiagonalX,
    /// Diagonal curve, horizontal then vertical.
    DiagonalY,
    /// Diagonal curve, rounded.
    DiagonalR,
}

impl LinkPathShape {
    /// Returns the SVG shape for the given coordinates.
    fn draw(&self, sx: f64, sy: f64, tx: f64, ty: f64, tension: f64) -> String {
        match self {
            Self::Line => format!(
                "M{x0},{y0}L{x1},{y1}",
                x0 = sx.v(),
                y0 = sy.v(),
                x1 = tx.v(),
                y1 = ty.v()
            ),
            Self::Curve => {
                let dx = tx - sx;
                let dy = ty - sy;
                let ix = tension * (dx + dy);
                let iy = tension * (dy - dx);
                format!(
                    "M{x0},{y0}C{cx0},{cy0} {cx1},{cy1} {x1},{y1}",
                    x0 = sx.v(),
                    y0 = sy.v(),
                    x1 = tx.v(),
                    y1 = ty.v(),
                    cx0 = (sx + ix).v(),
                    cy0 = (sy + iy).v(),
                    cx1 = (tx + iy).v(),
                    cy1 = (ty - ix).v(),
                )
            }
            Self::CornerX => format!(
                "M{x0},{y0}V{y1}H{x1}",
                x0 = sx.v(),
                y0 = sy.v(),
                x1 = tx.v(),
                y1 = ty.v(),
            ),
            Self::CornerY => format!(
                "M{x0},{y0}H{x1}V{y1}",
                x0 = sx.v(),
                y0 = sy.v(),
                x1 = tx.v(),
                y1 = ty.v(),
            ),
            Self::CornerR => {
                let (ss, sc) = sx.sin_cos();
                let (ts, tc) = tx.sin_cos();
                let sf = if (tx - sx).abs() > PI {
                    tx <= sx
                } else {
                    tx > sx
                };
                format!(
                    "M{x0},{y0}A{r},{r} 0 0,{sf} {c0},{c1}L{x1},{y1}",
                    x0 = (sy * sc).v(),
                    y0 = (sy * ss).v(),
                    r = sy.v(),
                    sf = u8::from(sf),
                    c0 = (sy * tc).v(),
                    c1 = (sy * ts).v(),
                    x1 = (ty * tc).v(),
                    y1 = (ty * ts).v(),
                )
            }
            Self::DiagonalX => format!(
                "M{x0},{y0}C{m},{y0} {m},{y1} {x1},{y1}",
                x0 = sx.v(),
                y0 = sy.v(),
                x1 = tx.v(),
                y1 = ty.v(),
                m = sx.midpoint(tx).v(),
            ),
            Self::DiagonalY => format!(
                "M{x0},{y0}C{x0},{m} {x1},{m} {x1},{y1}",
                x0 = sx.v(),
                y0 = sy.v(),
                x1 = tx.v(),
                y1 = ty.v(),
                m = sy.midpoint(ty).v(),
            ),
            Self::DiagonalR => {
                let (ss, sc) = sx.sin_cos();
                let (ts, tc) = sy.sin_cos();
                let mr = tx.midpoint(ty);
                format!(
                    "M{x0},{y0}C{cx0},{cy0} {cx1},{cy1} {x1},{y1}",
                    x0 = (tx * sc).v(),
                    y0 = (tx * ss).v(),
                    cx0 = (mr * sc).v(),
                    cy0 = (mr * ss).v(),
                    cx1 = (mr * tc).v(),
                    cy1 = (mr * ts).v(),
                    x1 = (ty * tc).v(),
                    y1 = (ty * ts).v()
                )
            }
        }
    }
}

/// A lookup data transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de", rename_all = "camelCase")]
pub(super) struct Lookup<'s> {
    /// A default value to use if no matching key value is found.
    #[serde(borrow, default)]
    default: Value<'s>,
    /// An array of one or more key fields in the primary data set to match
    /// against the secondary data set.
    #[serde(borrow)]
    keys: Vec<Cow<'s, str>>,
    /// The name of the secondary data set to treat as a lookup table.
    #[serde(borrow)]
    on: Cow<'s, str>,
    /// The field in the secondary data set to match against the primary data
    /// set. If unspecified, the integer indices of the secondary data set will
    /// be used instead.
    #[serde(borrow, default)]
    on_key: Option<Cow<'s, str>>,
    /// An array of field names in which to store the results of the lookup.
    /// This array should have the same length as the [`Self::keys`] parameter.
    #[serde(borrow, rename = "as")]
    output: Vec<Cow<'s, str>>,
}

impl<'s> Lookup<'s> {
    /// Transforms the input data to add extra fields.
    fn transform(&self, node: &Node<'s, '_>, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let other = node.data_values(&self.on).unwrap_or_default();
        let map = self.on_key.as_ref().map(|on_key| {
            other
                .iter()
                .filter_map(|item| get_nested_value(item, on_key).map(|value| (value, item)))
                .collect::<HashMap<_, _>>()
        });

        for item in &mut data {
            for (key, out) in self.keys.iter().zip(self.output.iter()) {
                let key = get_nested_value(item, key).unwrap_or(&Value::Null);
                let value = if let Some(map) = &map {
                    map.get(key).copied()
                } else {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "truncation is desired; if sign is bogus, lookup will just fail"
                    )]
                    let key = key.to_f64() as usize;
                    other.get(key)
                }
                .cloned();
                item.insert(out.clone(), value.unwrap_or_else(|| self.default.clone()));
            }
        }

        data
    }
}

/// A stacked chart layout transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Stack<'s> {
    /// The data field that determines the thickness or height of each stack.
    #[serde(borrow)]
    field: Cow<'s, str>,
    /// A list of fields to partition the data into groups (stacks). When values
    /// are stacked vertically, this corresponds to the x-coordinates.
    #[serde(
        borrow,
        rename = "groupby",
        deserialize_with = "super::data::vec_or_str"
    )]
    group_by: Vec<Cow<'s, str>>,
    /// The baseline offset style.
    #[serde(default)]
    offset: StackOffset,
    /// The output field names.
    #[serde(borrow, default)]
    output: LayoutOutput<'s>,
    /// A list of fields to determine the order of stack layers.
    #[serde(
        borrow,
        default,
        rename = "sortby",
        deserialize_with = "super::data::vec_or_str"
    )]
    sort_by: Vec<Cow<'s, str>>,
}

impl<'s> Stack<'s> {
    /// Partitions the data into sorted stacks according to [`Self::group_by`]
    /// and [`Self::sort_by`].
    fn partition<'b>(&self, data: &'b mut [Value<'s>]) -> (Vec<PartitionGroup<'s, 'b>>, f64) {
        let mut max = 0.0_f64;
        let mut groups = IndexMap::new();
        for item in data {
            let key = make_key_string(item, &self.group_by);
            let group = groups.entry(key).or_insert_with(PartitionGroup::default);
            let value = get_nested_value(item, &self.field).map_or(0.0, ValueExt::to_f64);
            max = max.max(value);
            group.sum += value;
            group.values.push(item);
        }
        if !self.sort_by.is_empty() {
            for group in groups.values_mut() {
                group
                    .values
                    .sort_unstable_by(|a, b| comparator(a, b, &self.sort_by));
            }
        }
        (groups.into_values().collect::<Vec<_>>(), max)
    }

    /// Transforms the input data to add extra fields.
    fn transform(&self, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let (groups, max) = self.partition(&mut data);

        for group in groups {
            let sum = group.sum;
            let offset = if matches!(self.offset, StackOffset::Center) {
                (max - sum) / 2.0
            } else {
                0.0
            };
            let scale = if matches!(self.offset, StackOffset::Normalize) {
                1.0 / sum
            } else {
                1.0
            };

            let mut start;
            let mut end = offset;
            let mut acc = 0.0;
            for item in group.values {
                start = end;
                acc += get_nested_value(item, &self.field).map_or(0.0, ValueExt::to_f64);
                end = scale * acc + offset;
                item.insert(self.output.start.clone(), start);
                item.insert(self.output.mid.clone(), start.midpoint(end));
                item.insert(self.output.end.clone(), end);
            }
        }

        data
    }
}

/// Compares all of the given `keys` on objects `a` and `b` in order.
fn comparator(a: &Value<'_>, b: &Value<'_>, keys: &[Cow<'_, str>]) -> Ordering {
    for key in keys {
        let (key, reverse) = comparator_key(key);
        let cmp = compare_object_key(a, b, key);
        if cmp.is_ne() {
            return comparator_reverse(cmp, reverse);
        }
    }
    Ordering::Equal
}

/// Splits a sort field into its corresponding key and sort order.
#[inline]
fn comparator_key(key: &str) -> (&str, bool) {
    let (key, reverse) = if let Some(key) = key.strip_prefix("-") {
        (key, true)
    } else {
        (key.strip_prefix("+").unwrap_or(key), false)
    };
    (key, reverse)
}

/// Reverses the given `cmp` if `reverse` is true.
#[inline]
fn comparator_reverse(cmp: Ordering, reverse: bool) -> Ordering {
    if reverse { cmp.reverse() } else { cmp }
}

/// Performs a fuzzy comparison of the value with the given `key` on objects `a`
/// and `b`.
#[inline]
fn compare_object_key(a: &Value<'_>, b: &Value<'_>, key: &str) -> Ordering {
    let a = get_nested_value(a, key).unwrap_or(&Value::Null);
    let b = get_nested_value(b, key).unwrap_or(&Value::Null);
    a.fuzzy_total_cmp(b)
}

/// A stack chart layout group.
#[derive(Debug, Default)]
struct PartitionGroup<'s, 'b> {
    /// The sum of all the [`Stack::field`] values in the group.
    sum: f64,
    /// All the values in the group.
    values: Vec<&'b mut Value<'s>>,
}

/// A stack chart layout offset.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum StackOffset {
    /// Start at zero.
    #[default]
    Zero,
    /// Center the stacks.
    Center,
    /// Compute percentage values for each stack point; the output values will
    /// be in the range 0..=1.
    Normalize,
}

/// Unimplemented transform.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Todo;

/// A tree map layout transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Treemap<'s> {
    /// The name of the field containing the child nodes.
    #[serde(borrow, default = "Treemap::default_children")]
    children: Cow<'s, str>,
    /// The values to use to determine the area of each leaf-level treemap cell.
    #[serde(borrow, default = "Treemap::default_field")]
    field: Cow<'s, str>,
    /// The layout mode to use. Undocumented.
    #[serde(default)]
    mode: TreemapMode,
    /// The output field names.
    #[serde(borrow, default)]
    output: TreemapOutput<'s>,
    /// The padding, in pixels, around internal nodes in the treemap.
    #[serde(default)]
    padding: TreemapPadding,
    /// The name of the field containing the parent node.
    #[serde(borrow, default = "Treemap::default_parent")]
    parent: Cow<'s, str>,
    /// The target aspect ratio for the layout to optimize. The default value is
    /// the golden ratio (~1.6).
    #[serde(default = "Treemap::default_ratio")]
    ratio: f64,
    /// If true, treemap cell dimensions will be rounded to whole pixels.
    #[serde(default = "Treemap::default_round")]
    round: bool,
    /// The width and height of the treemap layout. Defaults to the width and
    /// height of the enclosing data rectangle or group.
    #[serde(default)]
    size: Option<[f64; 2]>,
    /// A list of fields to use as sort criteria for sibling nodes.
    #[serde(borrow, default = "Treemap::default_sort")]
    sort: Vec<Cow<'s, str>>,
    /// If true, repeated runs of the treemap will use cached partition
    /// boundaries. This would result in smoother transition animations, at the
    /// cost of unoptimized aspect ratios, if animations were implemented, which
    /// they are not.
    #[serde(default, rename = "sticky")]
    _sticky: bool,
}

impl<'s> Treemap<'s> {
    /// The default value for [`Self::children`].
    const fn default_children() -> Cow<'static, str> {
        Cow::Borrowed("children")
    }

    /// The default value for [`Self::field`].
    const fn default_field() -> Cow<'static, str> {
        Cow::Borrowed("value")
    }

    /// The default value for [`Self::parent`].
    const fn default_parent() -> Cow<'static, str> {
        Cow::Borrowed("parent")
    }

    /// The default value for [`Self::ratio`].
    fn default_ratio() -> f64 {
        0.5 * (1.0 + 5.0_f64.sqrt())
    }

    /// The default value for [`Self::round`].
    const fn default_round() -> bool {
        true
    }

    /// The default value for [`Self::sort`].
    fn default_sort() -> Vec<Cow<'s, str>> {
        vec![Cow::Borrowed("-value")]
    }

    /// Transforms the input data to add extra fields.
    fn transform(&self, node: &Node<'s, '_>, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let root = data.iter().position(|item| {
            item.as_object()
                .and_then(|item| item.get(&self.parent))
                .is_none_or(Value::is_null)
        });

        let Some(root) = root else {
            return data;
        };

        let mut treemap = treemap::Treemap::new(self, node, &data);

        for node in treemap.layout(root) {
            let item = &mut data[node.object];
            item.insert(self.output.depth.clone(), f64::from(node.depth));
            item.insert(self.output.height.clone(), node.rect.height());
            item.insert(self.output.width.clone(), node.rect.width());
            item.insert(self.output.x.clone(), node.rect.left);
            item.insert(self.output.y.clone(), node.rect.top);

            // D3 scribbles all over the original objects and the treemap
            // example relies on this, lol.
            item.insert("depth", f64::from(node.depth));
            item.insert("dy", node.rect.height());
            item.insert("dx", node.rect.width());
            item.insert("x", node.rect.left);
            item.insert("y", node.rect.top);
        }

        data
    }
}

/// Treemap node padding.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum TreemapPadding {
    /// Uniform padding around all edges of the node.
    Uniform(f64),
    /// Inset padding in CSS order (top, right, bottom, left).
    Sides([f64; 4]),
}

impl Default for TreemapPadding {
    fn default() -> Self {
        Self::Uniform(0.0)
    }
}

/// Treemap renderer mode.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TreemapMode {
    /// Horizontal subdivision.
    Dice,
    /// Vertical subdivision.
    Slice,
    /// Vertical subdivision for odd-depth nodes and horizontal subdivision for
    /// even ones.
    SliceDice,
    /// Use the squarified treemap algorithm to produce rectangles that match
    /// the given [`Treemap::ratio`].
    #[default]
    Squarify,
}

/// Tree map output field names.
#[derive(Debug, serde::Deserialize)]
struct TreemapOutput<'s> {
    /// The name of the depth field.
    #[serde(borrow, default = "TreemapOutput::default_depth")]
    depth: Cow<'s, str>,
    /// The name of the height field.
    #[serde(borrow, default = "TreemapOutput::default_height")]
    height: Cow<'s, str>,
    /// The name of the width field.
    #[serde(borrow, default = "TreemapOutput::default_width")]
    width: Cow<'s, str>,
    /// The name of the x field.
    #[serde(borrow, default = "TreemapOutput::default_x")]
    x: Cow<'s, str>,
    /// The name of the y field.
    #[serde(borrow, default = "TreemapOutput::default_y")]
    y: Cow<'s, str>,
}

impl TreemapOutput<'_> {
    /// The default value for [`Self::depth`].
    const fn default_depth() -> Cow<'static, str> {
        Cow::Borrowed("layout_depth")
    }

    /// The default value for [`Self::height`].
    const fn default_height() -> Cow<'static, str> {
        Cow::Borrowed("layout_height")
    }

    /// The default value for [`Self::width`].
    const fn default_width() -> Cow<'static, str> {
        Cow::Borrowed("layout_width")
    }

    /// The default value for [`Self::x`].
    const fn default_x() -> Cow<'static, str> {
        Cow::Borrowed("layout_x")
    }

    /// The default value for [`Self::y`].
    const fn default_y() -> Cow<'static, str> {
        Cow::Borrowed("layout_y")
    }
}

impl Default for TreemapOutput<'_> {
    fn default() -> Self {
        Self {
            depth: Self::default_depth(),
            height: Self::default_height(),
            width: Self::default_width(),
            x: Self::default_x(),
            y: Self::default_y(),
        }
    }
}

/// A word cloud layout transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Wordcloud<'s> {
    /// The font face.
    #[serde(borrow, default = "Wordcloud::default_font")]
    font: FieldRef<'s, Cow<'s, str>>,
    /// Rescale [`Self::font_size`] into the given output range using a square
    /// root scale, using the font sizes from the data set as the input domain.
    #[serde(default = "Wordcloud::default_font_scale")]
    font_scale: Option<[f64; 2]>,
    /// The font size, in pixels.
    #[serde(borrow, default = "Wordcloud::default_font_size")]
    font_size: FieldRef<'s, f64>,
    /// The font style.
    #[serde(borrow, default = "Wordcloud::default_font_style")]
    font_style: FieldRef<'s, Cow<'s, str>>,
    /// The font weight.
    #[serde(borrow, default = "Wordcloud::default_font_weight")]
    font_weight: FieldRef<'s, Cow<'s, str>>,
    /// The output field names.
    #[serde(borrow, default)]
    output: WordcloudOutput<'s>,
    /// Extra padding around each word, in pixels.
    #[serde(borrow, default = "Wordcloud::default_padding")]
    padding: FieldRef<'s, f64>,
    /// The rotation angle of a word, in degrees.
    #[serde(borrow, default = "Wordcloud::default_rotate")]
    rotate: FieldRef<'s, f64>,
    /// The dimensions of the wordcloud layout, in pixels.
    #[serde(default = "Wordcloud::default_size")]
    size: [f64; 2],
    /// The type of spiral used to position words.
    #[serde(default = "Wordcloud::default_spiral")]
    spiral: WordcloudSpiral,
    /// The word text.
    #[serde(borrow, default = "Wordcloud::default_text")]
    text: FieldRef<'s, Cow<'s, str>>,
}

impl<'s> Wordcloud<'s> {
    /// The default value for [`Self::font`].
    const fn default_font() -> FieldRef<'s, Cow<'s, str>> {
        FieldRef::Value(Cow::Borrowed("sans-serif"))
    }

    /// The default value for [`Self::font_scale`].
    const fn default_font_scale() -> Option<[f64; 2]> {
        Some([10.0, 50.0])
    }

    /// The default value for [`Self::font_size`].
    const fn default_font_size() -> FieldRef<'s, f64> {
        FieldRef::Value(14.0)
    }

    /// The default value for [`Self::font_style`].
    const fn default_font_style() -> FieldRef<'s, Cow<'s, str>> {
        FieldRef::Value(Cow::Borrowed("normal"))
    }

    /// The default value for [`Self::font_weight`].
    const fn default_font_weight() -> FieldRef<'s, Cow<'s, str>> {
        FieldRef::Value(Cow::Borrowed("normal"))
    }

    /// The default value for [`Self::padding`].
    const fn default_padding() -> FieldRef<'s, f64> {
        FieldRef::Value(1.0)
    }

    /// The default value for [`Self::rotate`].
    const fn default_rotate() -> FieldRef<'s, f64> {
        FieldRef::Value(0.0)
    }

    /// The default value for [`Self::size`].
    const fn default_size() -> [f64; 2] {
        [900.0, 500.0]
    }

    /// The default value for [`Self::spiral`].
    const fn default_spiral() -> WordcloudSpiral {
        WordcloudSpiral::Archimedean
    }

    /// The default value for [`Self::text`].
    const fn default_text() -> FieldRef<'s, Cow<'s, str>> {
        FieldRef::Short(Cow::Borrowed("data"))
    }

    /// Transforms the input data to add extra fields.
    fn transform(&self, node: &Node<'s, '_>, mut data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        let scale = self.font_scale.map(|[out_min, out_max]| {
            let (in_min, in_max) =
                make_extent(None, None, data.iter().map(|item| self.font_size.get(item)));
            move |value: f64| {
                let value = (value - in_min) / (in_max - in_min);
                let value = value.signum() * value.abs().sqrt();
                (out_max - out_min) * value + out_min
            }
        });

        let mut words = data
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let font = self.font.get(item);
                let font_size = {
                    let font_size = self.font_size.get(item);
                    if let Some(scale) = &scale {
                        scale(font_size)
                    } else {
                        font_size
                    }
                };

                word_cloud::Word {
                    index,
                    font,
                    padding: self.padding.get(item),
                    rotate: self.rotate.get(item),
                    size: font_size,
                    style: self.font_style.get(item),
                    text: self.text.get(item),
                    weight: self.font_weight.get(item),
                }
            })
            .collect::<Vec<_>>();
        words.sort_unstable_by(|a, b| a.size.total_cmp(&b.size).reverse());

        for (word, pos) in word_cloud::word_cloud(
            &mut node.rng.borrow_mut(),
            Vec2::new(self.size[0], self.size[1]),
            self.spiral,
            words.into_iter(),
        ) {
            let item = &mut data[word.index];
            item.insert(self.output.font.clone(), word.font);
            item.insert(self.output.font_size.clone(), word.size);
            item.insert(self.output.font_style.clone(), word.style);
            item.insert(self.output.font_weight.clone(), word.weight);
            item.insert(self.output.rotate.clone(), word.rotate);
            item.insert(self.output.x.clone(), pos.x);
            item.insert(self.output.y.clone(), pos.y);
        }

        data
    }
}

/// Wordcloud output field names.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WordcloudOutput<'s> {
    /// The the name of the font field.
    #[serde(borrow, default = "WordcloudOutput::default_font")]
    font: Cow<'s, str>,
    /// The name of the font size field.
    #[serde(borrow, default = "WordcloudOutput::default_font_size")]
    font_size: Cow<'s, str>,
    /// The name of the font style field.
    #[serde(borrow, default = "WordcloudOutput::default_font_style")]
    font_style: Cow<'s, str>,
    /// The name of the font weight field.
    #[serde(borrow, default = "WordcloudOutput::default_font_weight")]
    font_weight: Cow<'s, str>,
    /// The name of the text rotation field.
    #[serde(borrow, default = "WordcloudOutput::default_rotate")]
    rotate: Cow<'s, str>,
    /// The name of the x-coordinate field.
    #[serde(borrow, default = "WordcloudOutput::default_x")]
    x: Cow<'s, str>,
    /// The name of the y-coordinate field.
    #[serde(borrow, default = "WordcloudOutput::default_y")]
    y: Cow<'s, str>,
}

impl WordcloudOutput<'_> {
    /// The default value for [`Self::font`].
    const fn default_font() -> Cow<'static, str> {
        Cow::Borrowed("layout_font")
    }

    /// The default value for [`Self::font_size`].
    const fn default_font_size() -> Cow<'static, str> {
        Cow::Borrowed("layout_fontSize")
    }

    /// The default value for [`Self::font_style`].
    const fn default_font_style() -> Cow<'static, str> {
        Cow::Borrowed("layout_fontStyle")
    }

    /// The default value for [`Self::font_weight`].
    const fn default_font_weight() -> Cow<'static, str> {
        Cow::Borrowed("layout_fontWeight")
    }

    /// The default value for [`Self::rotate`].
    const fn default_rotate() -> Cow<'static, str> {
        Cow::Borrowed("layout_rotate")
    }

    /// The default value for [`Self::x`].
    const fn default_x() -> Cow<'static, str> {
        Cow::Borrowed("layout_x")
    }

    /// The default value for [`Self::y`].
    const fn default_y() -> Cow<'static, str> {
        Cow::Borrowed("layout_y")
    }
}

impl Default for WordcloudOutput<'_> {
    fn default() -> Self {
        Self {
            font: Self::default_font(),
            font_size: Self::default_font_size(),
            font_style: Self::default_font_style(),
            font_weight: Self::default_font_weight(),
            rotate: Self::default_rotate(),
            x: Self::default_x(),
            y: Self::default_y(),
        }
    }
}

/// A wordcloud layout spiral.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum WordcloudSpiral {
    /// An archimedean spiral.
    #[default]
    Archimedean,
    /// A rectangular spiral.
    Rectangular,
}

impl WordcloudSpiral {
    /// Creates a generator which, when called with an angle, returns a new
    /// coordinate for the given `dims`.
    fn generator(self, dims: Vec2) -> Either<impl Fn(f64) -> Vec2, impl FnMut(f64) -> Vec2> {
        match self {
            WordcloudSpiral::Archimedean => {
                let e = dims.x / dims.y;
                Either::Left(move |t: f64| {
                    let t = t * 0.1;
                    Vec2::new(e * t * t.cos(), t * t.sin())
                })
            }
            WordcloudSpiral::Rectangular => {
                let dy = 4.0;
                let dx = dy * dims.x / dims.y;
                let mut x = 0.0;
                let mut y = 0.0;
                Either::Right(move |t: f64| {
                    let sign = t.signum();
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "this only cares about the whole number part"
                    )]
                    match ((t * sign * 4.0 + 1.0).sqrt() - sign) as i32 & 3 {
                        0 => x += dx,
                        1 => y += dy,
                        2 => x -= dx,
                        _ => y -= dy,
                    }
                    Vec2::new(x, y)
                })
            }
        }
    }
}

/// Geo and force output field names.
#[derive(Debug, serde::Deserialize)]
struct XyOutput<'s> {
    /// The name of the x-coordinate field.
    #[serde(borrow, default = "XyOutput::default_x")]
    x: Cow<'s, str>,
    /// The name of the y-coordinate field.
    #[serde(borrow, default = "XyOutput::default_y")]
    y: Cow<'s, str>,
}

impl XyOutput<'_> {
    /// The default value for [`Self::x`].
    const fn default_x() -> Cow<'static, str> {
        Cow::Borrowed("layout_x")
    }

    /// The default value for [`Self::y`].
    const fn default_y() -> Cow<'static, str> {
        Cow::Borrowed("layout_y")
    }
}

impl Default for XyOutput<'_> {
    fn default() -> Self {
        Self {
            x: Self::default_x(),
            y: Self::default_y(),
        }
    }
}

/// Creates a key string for the given `item` by concatenating the values in
/// the given `fields` according to the Vega documentation.
fn make_key_string(item: &Value<'_>, fields: &[Cow<'_, str>]) -> String {
    let mut key = String::new();
    for field in fields {
        let value = get_nested_value(item, field).map_or(<_>::default(), ValueExt::to_string);
        if !key.is_empty() {
            key.push('|');
        }
        key += &value;
    }
    key
}

// SPDX-SnippetBegin
// SPDX-License-Identifier: ISC
// SPDX-SnippetComment: Adapted from d3 3.5.17 by Mike Bostock

/// Types and functions for Gauss-Seidel force layout.
mod force {
    use super::{ForceFieldRef, Rect, SmallRng, Value, Vec2};
    use rand::Rng as _;

    /// A force layout engine.
    pub(super) struct Force<'b, 's> {
        /// The remaining inertia.
        alpha: f64,
        /// The layout configuration.
        config: &'b super::Force<'s>,
        /// The spatial dimensions of the layout.
        dims: Vec2,
        /// The list of links between nodes.
        links: Vec<Link>,
        /// The objects being laid out.
        nodes: Vec<Node>,
        /// The random number generator used to perturb object positions.
        rng: &'b mut SmallRng,
        /// The precomputed square of the configured charge distance.
        sq_charge_distance: f64,
        /// The precompued square of [`super::Force::theta`].
        sq_theta: f64,
    }

    impl<'b, 's> Force<'b, 's> {
        /// Creates a new `Force` layout engine.
        pub fn new<F>(
            rng: &'b mut SmallRng,
            config: &'b super::Force<'s>,
            dims: Vec2,
            data: &[Value<'s>],
            fixed: F,
            links: Vec<Link>,
        ) -> Self
        where
            F: Fn(&Value<'_>) -> bool,
        {
            let mut nodes = data
                .iter()
                .enumerate()
                .map(|(index, item)| Node {
                    charge: config.charge.get(item),
                    fixed: fixed(item),
                    index,
                    pos: Vec2::invalid(),
                    velocity: Vec2::invalid(),
                    weight: 0.0,
                })
                .collect::<Vec<_>>();

            for link in &links {
                nodes[link.source].weight += 1.0;
                nodes[link.target].weight += 1.0;
            }

            let mut neighbors = None;
            let mut position = |nodes: &[Node], index: usize, horizontal: bool| {
                const INVALID: usize = usize::MAX;
                let neighbors = neighbors.get_or_insert_with(|| {
                    let mut neighbors = vec![vec![INVALID; nodes.len()]; nodes.len()];
                    for link in &links {
                        neighbors[link.source].push(link.target);
                        neighbors[link.target].push(link.source);
                    }
                    neighbors
                });

                let candidates = &neighbors[index];
                for candidate in candidates {
                    let dim = if *candidate == INVALID {
                        continue;
                    } else if horizontal {
                        nodes[*candidate].pos.x
                    } else {
                        nodes[*candidate].pos.y
                    };
                    if !dim.is_nan() {
                        return dim;
                    }
                }
                rng.random::<f64>() * if horizontal { dims.x } else { dims.y }
            };

            for index in 0..nodes.len() {
                if nodes[index].pos.x.is_nan() {
                    let x = position(&nodes, index, true);
                    nodes[index].pos.x = x;
                }
                if nodes[index].pos.y.is_nan() {
                    let y = position(&nodes, index, false);
                    nodes[index].pos.y = y;
                }
                let node = &mut nodes[index];
                if node.velocity.x.is_nan() {
                    node.velocity.x = node.pos.x;
                }
                if node.velocity.y.is_nan() {
                    node.velocity.y = node.pos.y;
                }
                node.charge = config.charge.get(&data[node.index]);
            }

            let alpha = config.alpha;

            Self {
                alpha,
                config,
                dims,
                links,
                nodes,
                rng,
                sq_charge_distance: config.charge_distance.map_or(f64::INFINITY, |d| d * d),
                sq_theta: config.theta * config.theta,
            }
        }

        /// Runs layout calculation up to
        /// [`Self::config::iterations`](super::Force::iterations) times,
        /// terminating early if the layout is considered stable.
        pub fn run_to_finish(mut self) -> impl Iterator<Item = Vec2> {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "this should always be an integer"
            )]
            for _ in 0..self.config.iterations as i64 {
                if self.tick() {
                    break;
                }
            }
            self.nodes.into_iter().map(|node| node.pos)
        }

        /// Runs the layout algorithm for one tick.
        fn tick(&mut self) -> bool {
            self.alpha *= 0.99;
            if self.alpha < 0.005 {
                self.alpha = 0.0;
                return true;
            }

            // gauss-seidel relaxation for links
            for link in &self.links {
                let [source, target] = self
                    .nodes
                    .get_disjoint_mut([link.source, link.target])
                    .unwrap();
                let delta = target.pos - source.pos;
                let sq_len = delta.square_len();
                if sq_len != 0.0 {
                    let len = sq_len.sqrt();
                    let len = self.alpha * link.strength * (len - link.distance) / len;
                    let delta = delta * len;
                    let total_weight = source.weight + target.weight;
                    let magnitude = if total_weight == 0.0 {
                        0.5
                    } else {
                        source.weight / total_weight
                    };
                    target.pos -= delta * magnitude;
                    source.pos += delta * (1.0 - magnitude);
                }
            }

            // apply gravity forces
            let magnitude = self.alpha * self.config.gravity;
            if magnitude != 0.0 {
                let dims_mid = self.dims / 2.0;
                for node in &mut self.nodes {
                    node.pos += (dims_mid - node.pos) * magnitude;
                }
            }

            // compute quadtree center of mass and apply charge forces
            if !matches!(self.config.charge, ForceFieldRef::Value(0.0)) {
                let mut root = Quadtree::new(&self.nodes);
                force_accumulate(self.rng, &mut self.nodes, &mut root.root, self.alpha);
                for node in &mut self.nodes {
                    if !node.fixed {
                        root.visit(repulse(self.sq_charge_distance, self.sq_theta, node));
                    }
                }
            }

            // position verlet integration
            for node in &mut self.nodes {
                if node.fixed {
                    node.pos = node.velocity;
                } else {
                    let delta = node.velocity - node.pos;
                    node.velocity = node.pos;
                    node.pos -= delta * self.config.friction;
                }
            }

            false
        }
    }

    /// A link between two [`Node`]s.
    #[derive(Debug)]
    pub(super) struct Link {
        /// The length of the edge.
        pub distance: f64,
        /// An index into [`Force::nodes`] for the left node.
        pub source: usize,
        /// The tension of the edge.
        pub strength: f64,
        /// An index into [`Force::nodes`] for the right node.
        pub target: usize,
    }

    /// A force layout node.
    #[derive(Debug)]
    pub(super) struct Node {
        /// The charge of the node.
        charge: f64,
        /// If true, the node position is fixed.
        fixed: bool,
        /// The index of this node in the list of nodes. As an implementation
        /// detail, this is also the same as the index of the associated item
        /// in the source data.
        index: usize,
        /// The current position of the node.
        pos: Vec2,
        /// The current velocity of the node.
        velocity: Vec2,
        /// The weight of the node.
        weight: f64,
    }

    /// The root of a quadtree.
    struct Quadtree {
        /// The total bounds of the spatial tree.
        bounds: Rect,
        /// The root node.
        root: QuadtreeNode,
    }

    impl Quadtree {
        /// Creates a new quadtree from the given list of nodes.
        fn new(nodes: &[Node]) -> Self {
            let mut bounds = Rect::new(
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );

            for node in nodes {
                bounds.left = bounds.left.min(node.pos.x);
                bounds.top = bounds.top.min(node.pos.y);
                bounds.right = bounds.right.max(node.pos.x);
                bounds.bottom = bounds.bottom.max(node.pos.y);
            }

            // The quadtree root should be a square
            let width = bounds.width();
            let height = bounds.height();
            if width > height {
                bounds.bottom = bounds.top + width;
            } else {
                bounds.right = bounds.left + height;
            }

            let mut root = QuadtreeNode::default();
            for node in nodes {
                root.insert(node.index, node.pos, &bounds);
            }

            Self { bounds, root }
        }

        /// Performs depth-first zig-zag traversal of all nodes.
        fn visit<F>(&self, mut f: F)
        where
            F: FnMut(&QuadtreeNode, &Rect) -> bool,
        {
            quadtree_visit(&mut f, &self.root, &self.bounds);
        }
    }

    /// A four-quadrant spatial subdivision.
    struct QuadtreeNode {
        /// The midpoint of this node.
        center: Vec2,
        /// The charge magnitude.
        charge: f64,
        /// The charge vector.
        charge_vector: Vec2,
        /// The child nodes of a branch node. If this is `None`, the node is a
        /// leaf node.
        children: Option<[Option<Box<QuadtreeNode>>; 4]>,
        /// The charge for this node according to
        /// [`Force::alpha`]` * `[`Node::charge`].
        node_charge: f64,
        /// The index of the associated [`Node`] in [`Force::nodes`].
        node_id: usize,
    }

    impl QuadtreeNode {
        /// A surrogate value for an empty node. This is used instead of
        /// `Option<usize>` because there is no `NonMaxUsize` niche.
        const EMPTY: usize = usize::MAX;

        /// Inserts `id` at the given `point` and quad `bounds`.
        fn insert(&mut self, id: usize, point: Vec2, bounds: &Rect) {
            if !point.is_valid() {
                return;
            }

            if self.children.is_none() {
                if self.node_id == Self::EMPTY {
                    self.center = point;
                    self.node_id = id;
                } else {
                    // If the point at this leaf node is at the same position as
                    // the new point we are adding, we leave the point
                    // associated with the internal node while adding the new
                    // point to a child node. This avoids infinite recursion.
                    let delta = (self.center - point).abs();
                    if delta.x + delta.y < 0.01 {
                        self.insert_child(id, point, bounds);
                    } else {
                        let this_id = self.node_id;
                        let this_center = self.center;
                        self.center = Vec2::invalid();
                        self.node_id = Self::EMPTY;
                        self.insert_child(this_id, this_center, bounds);
                        self.insert_child(id, point, bounds);
                    }
                }
            } else {
                self.insert_child(id, point, bounds);
            }
        }

        /// Recursively inserts the specified `point` into a descendant of
        /// `self` according to the given `bounds`.
        fn insert_child(&mut self, id: usize, point: Vec2, bounds: &Rect) {
            let x_mid = bounds.left.midpoint(bounds.right);
            let y_mid = bounds.top.midpoint(bounds.bottom);

            let to_right = point.x >= x_mid;
            let to_bottom = point.y >= y_mid;

            let mut bounds = *bounds;
            if to_right {
                bounds.left = x_mid;
            } else {
                bounds.right = x_mid;
            }
            if to_bottom {
                bounds.top = y_mid;
            } else {
                bounds.bottom = y_mid;
            }

            let children = self.children.get_or_insert_default();
            let which = usize::from(to_bottom) << 1 | usize::from(to_right);
            let node = children[which].get_or_insert_default();
            node.insert(id, point, &bounds);
        }
    }

    impl Default for QuadtreeNode {
        fn default() -> Self {
            Self {
                center: Vec2::invalid(),
                charge: f64::NAN,
                charge_vector: Vec2::invalid(),
                children: <_>::default(),
                node_charge: f64::NAN,
                node_id: Self::EMPTY,
            }
        }
    }

    /// Recursively updates the charge of the given `quad`, randomly perturbing
    /// the position of its associated node in `nodes` if necessary.
    fn force_accumulate(
        rng: &mut SmallRng,
        nodes: &mut [Node],
        quad: &mut QuadtreeNode,
        alpha: f64,
    ) {
        let mut charge_vector = Vec2::zero();
        quad.charge = 0.0;
        if let Some(children) = &mut quad.children {
            for node in children.iter_mut().flatten() {
                force_accumulate(rng, nodes, node, alpha);
                quad.charge += node.charge;
                charge_vector += node.charge_vector * node.charge;
            }
        }
        if quad.node_id != QuadtreeNode::EMPTY {
            // jitter internal nodes that are coincident
            if quad.children.is_some() {
                let pos = &mut nodes[quad.node_id].pos;
                pos.x += rng.random::<f64>() - 0.5;
                pos.y += rng.random::<f64>() - 0.5;
            }
            let remaining_charge = alpha * nodes[quad.node_id].charge;
            quad.node_charge = remaining_charge;
            quad.charge += remaining_charge;
            charge_vector += nodes[quad.node_id].pos * remaining_charge;
        }
        quad.charge_vector = charge_vector / quad.charge;
    }

    /// Performs depth-first zig-zag traversal of all nodes starting from
    /// `node`.
    fn quadtree_visit<F>(f: &mut F, node: &QuadtreeNode, bounds: &Rect)
    where
        F: FnMut(&QuadtreeNode, &Rect) -> bool,
    {
        if !f(node, bounds)
            && let Some(children) = &node.children
        {
            let x_mid = bounds.left.midpoint(bounds.right);
            let y_mid = bounds.top.midpoint(bounds.bottom);

            if let Some(child) = &children[0] {
                let bounds = Rect::new(bounds.left, bounds.top, x_mid, y_mid);
                quadtree_visit(f, child, &bounds);
            }
            if let Some(child) = &children[1] {
                let bounds = Rect::new(x_mid, bounds.top, bounds.right, y_mid);
                quadtree_visit(f, child, &bounds);
            }
            if let Some(child) = &children[2] {
                let bounds = Rect::new(bounds.left, y_mid, x_mid, bounds.bottom);
                quadtree_visit(f, child, &bounds);
            }
            if let Some(child) = &children[3] {
                let bounds = Rect::new(x_mid, y_mid, bounds.right, bounds.bottom);
                quadtree_visit(f, child, &bounds);
            }
        }
    }

    /// Creates a visitor function for [`QuadtreeRoot::visit`] which adjusts
    /// the position of `node` according to its distance from other nodes.
    fn repulse(
        sq_charge_distance: f64,
        sq_theta: f64,
        node: &mut Node,
    ) -> impl FnMut(&QuadtreeNode, &Rect) -> bool {
        move |quad, bounds| {
            if quad.node_id != node.index {
                let delta = quad.charge_vector - node.pos;
                let width = bounds.width();
                let sq_distance = delta.square_len();

                if width * width / sq_theta < sq_distance {
                    if sq_distance < sq_charge_distance {
                        let magnitude = quad.charge / sq_distance;
                        node.velocity -= delta * magnitude;
                    }
                    return true;
                }

                if quad.node_id != QuadtreeNode::EMPTY
                    && sq_distance != 0.0
                    && sq_distance < sq_charge_distance
                {
                    let magnitude = quad.node_charge / sq_distance;
                    node.velocity -= delta * magnitude;
                }
            }

            quad.charge == 0.0
        }
    }
}

/// Types and functions for creating a map of hierarchical data.
mod treemap {
    use super::{
        Cow, Node, Ordering, Rect, TreemapMode, TreemapPadding, Value, ValueExt, comparator_key,
        comparator_reverse, compare_object_key,
    };

    /// A layout subdivision for a node.
    #[derive(Default)]
    struct Row {
        /// The pixel area of the subdivision.
        area: f64,
        /// The indexes of the nodes that participate in this subdivision.
        values: Vec<usize>,
    }

    /// A node in a tree map.
    #[derive(Debug)]
    pub(super) struct TreeNode {
        /// The computed area of the node.
        area: f64,
        /// The indexes of the children of this node in the flat list.
        children: Vec<usize>,
        /// The tree depth of this node.
        pub(super) depth: u16,
        /// The index of the associated data object in the data list.
        pub(super) object: usize,
        /// The index of the parent of this node in the flat list.
        parent: Option<usize>,
        /// The visual bounds of this node.
        pub(super) rect: Rect,
        /// The computed value of this node.
        value: f64,
    }

    impl TreeNode {
        /// Creates a new `TreeNode`.
        fn new(object: usize, depth: u16, parent: Option<usize>) -> Self {
            Self {
                area: <_>::default(),
                children: <_>::default(),
                depth,
                object,
                parent,
                rect: <_>::default(),
                value: <_>::default(),
            }
        }
    }

    /// A hierarchical tree map.
    pub(super) struct Treemap<'b, 's> {
        /// The configuration for the tree map.
        config: &'b super::Treemap<'s>,
        /// The height of the output, in pixels.
        height: f64,
        /// The flat list of nodes in the tree.
        nodes: Vec<TreeNode>,
        /// The width of the output, in pixels.
        width: f64,
    }

    impl<'b, 's> Treemap<'b, 's> {
        /// Creates a new `Treemap`.
        pub fn new(
            treemap: &'b super::Treemap<'s>,
            node: &Node<'s, '_>,
            data: &[Value<'s>],
        ) -> Self {
            let [width, height] = treemap
                .size
                .unwrap_or_else(|| [node.width(), node.height()]);
            Self {
                config: treemap,
                height,
                nodes: hierarchy(data, &treemap.children, &treemap.field, &treemap.sort),
                width,
            }
        }

        /// Updates the layout of the tree map starting with the given `root`
        /// node index.
        pub fn layout(&mut self, root: usize) -> &[TreeNode] {
            let root = &mut self.nodes[root];
            if root.value != 0.0 {
                root.rect.right = self.width;
                root.rect.bottom = self.height;
            }
            let scale = self.width * self.height / root.value;
            self.scale(0, scale);
            self.squarify(0);
            &self.nodes
        }

        /// Pads the given `rect` according to the padding configuration.
        fn pad(&self, rect: &Rect) -> Rect {
            match self.config.padding {
                TreemapPadding::Uniform(pad) => rect.inset(&Rect::new(pad, pad, pad, pad)),
                TreemapPadding::Sides([top, right, bottom, left]) => {
                    rect.inset(&Rect::new(left, top, right, bottom))
                }
            }
        }

        /// Given the parent `rect`, computes the layout of nodes in a given
        /// `row` of the parent, using `row_len` as the total length of the row.
        fn position(&mut self, row: &Row, row_len: f64, rect: &mut Rect, flush: bool) {
            let mut left = rect.left;
            let mut top = rect.top;
            let mut len = div_round(row.area, row_len, self.config.round);
            #[expect(
                clippy::float_cmp,
                reason = "if it is good enough for D3, it is good enough for here for now"
            )]
            let horizontal = row_len == rect.width();

            let max = if horizontal {
                rect.height()
            } else {
                rect.width()
            };

            if flush || len > max {
                len = max;
            }

            for index in &row.values {
                let node = &mut self.nodes[*index];
                let min = div_round(node.area, len, self.config.round);
                node.rect.left = left;
                node.rect.top = top;
                if horizontal {
                    node.rect.bottom = top + len;
                    let width = (rect.right - left).min(min);
                    node.rect.right = left + width;
                    left += width;
                } else {
                    node.rect.right = left + len;
                    let height = (rect.bottom - top).min(min);
                    node.rect.bottom = top + height;
                    top += height;
                }
            }
            let node = &mut self.nodes[*row.values.last().unwrap()];
            if horizontal {
                node.rect.right += rect.right - left;
                rect.top += len;
            } else {
                node.rect.bottom += rect.bottom - top;
                rect.left += len;
            }
        }

        /// Scales the areas of all of the direct children of the node at
        /// `index` by `scale`.
        fn scale(&mut self, index: usize, scale: f64) {
            let children = core::mem::take(&mut self.nodes[index].children);
            for child in &children {
                let child = &mut self.nodes[*child];
                let area = child.value * scale.max(0.0);
                child.area = area.max(0.0);
            }
            self.nodes[index].children = children;
        }

        /// Recursively lays out the node at the given `index`.
        fn squarify(&mut self, index: usize) {
            let node = &self.nodes[index];
            if node.children.is_empty() {
                return;
            }

            let mut rect = self.pad(&node.rect);

            let mut fill_len = match self.config.mode {
                TreemapMode::Dice => rect.height(),
                TreemapMode::Slice => rect.width(),
                TreemapMode::SliceDice => {
                    if node.depth & 1 == 0 {
                        rect.width()
                    } else {
                        rect.height()
                    }
                }
                TreemapMode::Squarify => rect.width().min(rect.height()),
            };

            let mut row = Row::default();
            let mut best = f64::INFINITY;
            let mut child_index = node.children.len().wrapping_sub(1);

            let scale = rect.area() / node.value;
            self.scale(index, scale);

            while child_index != usize::MAX {
                let candidate = self.nodes[index].children[child_index];
                row.area += self.nodes[candidate].area;
                row.values.push(candidate);

                let success = if matches!(self.config.mode, TreemapMode::Squarify) {
                    let score = self.worst(&row, fill_len);
                    if score <= best {
                        best = score;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                };

                if success {
                    child_index = child_index.wrapping_sub(1);
                } else {
                    // Abort and try a different orientation
                    let mut row = core::mem::take(&mut row);
                    row.area -= self.nodes[candidate].area;
                    self.position(&row, fill_len, &mut rect, false);
                    fill_len = rect.width().min(rect.height());
                    best = f64::INFINITY;
                }
            }

            if !row.values.is_empty() {
                self.position(&row, fill_len, &mut rect, true);
            }

            for child in 0..self.nodes[index].children.len() {
                let child = self.nodes[index].children[child];
                self.squarify(child);
            }
        }

        /// Computes the score for the specified row as the worst aspect ratio.
        fn worst(&self, row: &Row, u: f64) -> f64 {
            let area = row.area;
            if area == 0.0 {
                f64::INFINITY
            } else {
                let mut min = f64::INFINITY;
                let mut max = 0.0_f64;
                for candidate in &row.values {
                    let area = self.nodes[*candidate].area;
                    if area != 0.0 {
                        min = min.min(area);
                        max = max.max(area);
                    }
                }
                let sq_area = area * area;
                let sq_u = u * u;
                ((sq_u * max * self.config.ratio) / sq_area)
                    .max(sq_area / (sq_u * min * self.config.ratio))
            }
        }
    }

    /// Divides `n` by `d`, optionally rounding if `round` is true. If `d` is
    /// zero, returns zero.
    fn div_round(n: f64, d: f64, round: bool) -> f64 {
        if d == 0.0 {
            0.0
        } else if round {
            (n / d).round()
        } else {
            n / d
        }
    }

    /// Creates a flat list of [`TreeNode`] from the given `data`, using
    /// `children_key` and `value_key` to look up children, node weights, and
    /// sort values from the data objects in `data`.
    fn hierarchy(
        data: &[Value<'_>],
        children_key: &str,
        value_key: &str,
        sort_keys: &[Cow<'_, str>],
    ) -> Vec<TreeNode> {
        let mut stack = vec![TreeNode::new(0, 0, None)];
        let mut nodes = Vec::<TreeNode>::with_capacity(data.len());

        while let Some(mut node) = stack.pop() {
            if let Some(parent) = node.parent {
                let index = nodes.len();
                nodes[parent].children.push(index);
            }

            if let Some(children) = data[node.object]
                .get(children_key)
                .and_then(Value::as_array)
                && !children.is_empty()
            {
                for child in children.iter().rev() {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "the value came from a usize"
                    )]
                    stack.push(TreeNode::new(
                        child.as_u64().unwrap() as usize,
                        node.depth + 1,
                        Some(nodes.len()),
                    ));
                }
            } else {
                node.value = data[node.object]
                    .get(value_key)
                    .map_or(0.0, ValueExt::to_f64);
            }

            nodes.push(node);
        }

        hierarchy_visit_after(&mut nodes, 0, |nodes, index| {
            if !sort_keys.is_empty() {
                let mut children = core::mem::take(&mut nodes[index].children);
                children.sort_unstable_by(|a, b| {
                    for key in sort_keys {
                        let (key, reverse) = comparator_key(key);
                        let cmp = match key {
                            "value" => nodes[*a].value.total_cmp(&nodes[*b].value),
                            key => {
                                let a = &data[nodes[*a].object];
                                let b = &data[nodes[*b].object];
                                compare_object_key(a, b, key)
                            }
                        };
                        if cmp.is_ne() {
                            return comparator_reverse(cmp, reverse);
                        }
                    }
                    Ordering::Equal
                });
                nodes[index].children = children;
            }
            if let Some(parent) = nodes[index].parent {
                let value = nodes[index].value;
                nodes[parent].value += value;
            }
        });

        nodes
    }

    /// Performs depth-first traversal of all nodes in `data` starting from
    /// `index`.
    fn hierarchy_visit_after<F>(data: &mut [TreeNode], index: usize, mut callback: F)
    where
        F: FnMut(&mut [TreeNode], usize),
    {
        let mut pending = vec![index];
        let mut processed = vec![];
        while let Some(index) = pending.pop() {
            let node = &data[index];
            processed.push(index);
            pending.extend(&node.children);
        }
        while let Some(index) = processed.pop() {
            callback(data, index);
        }
    }
}

// SPDX-SnippetEnd

// SPDX-SnippetBegin
// SPDX-License-Identifier: BSD-3-clause
// SPDX-SnippetComment: Adapted from d3-cloud 1.2.8 by Jason Davies

/// Types and functions for making a word cloud.
mod word_cloud {
    use super::{
        super::renderer::{Pixels, buffer_to_path, shape_text},
        Cow, Either, Rect, SmallRng, Vec2, WordcloudSpiral,
    };
    use rand::Rng as _;
    use tiny_skia::{FillRule, Mask, Point, Stroke, Transform as SkiaTransform};

    /// A word cloud word.
    pub(super) struct Word<'s> {
        /// The original index of the associated item.
        pub index: usize,
        /// The font family.
        pub font: Cow<'s, str>,
        /// The padding around the word.
        pub padding: f64,
        /// The rotation angle, in degrees.
        pub rotate: f64,
        /// The font size.
        pub size: f64,
        /// The font style.
        pub style: Cow<'s, str>,
        /// The word text.
        pub text: Cow<'s, str>,
        /// The font weight.
        pub weight: Cow<'s, str>,
    }

    /// Calculates coordinates for the list of `words` using the given `spiral` and
    /// `rng` for the initial position randomisation. The list of words should be
    /// pre-sorted in descending font size.
    pub(super) fn word_cloud<'s>(
        rng: &mut SmallRng,
        dims: Vec2,
        spiral: WordcloudSpiral,
        words: impl Iterator<Item = Word<'s>>,
    ) -> impl Iterator<Item = (Word<'s>, Vec2)> {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "values are clamped into bounds"
        )]
        let mut cloud = Pixels::new(
            dims.x.clamp(0.0, u16::MAX.into()) as u16,
            dims.y.clamp(0.0, u16::MAX.into()) as u16,
        );
        let mut all_words = Rect::default();
        words.filter_map(move |word| {
            let (sprite, offset) = make_sprite(&word)?;
            let pos = place(rng, &mut cloud, dims, &all_words, spiral, &sprite)?;
            all_words = all_words.union(&sprite.bounds().offset(pos - sprite.midpoint()));
            Some((word, pos + offset))
        })
    }

    /// Checks whether the given word `sprite` intersects with some word already
    /// drawn to the word `cloud`.
    #[inline]
    fn collides(cloud: &Pixels, sprite: &Pixels, word_tl: Vec2) -> bool {
        let iter = bit_iter(cloud, sprite, word_tl);
        for ((in_index, in_mask), (out_index, out_mask)) in iter {
            if (sprite.data[in_index] & in_mask) != 0 && (cloud.data[out_index] & out_mask) != 0 {
                return true;
            }
        }
        false
    }

    /// Returns a zipped iterator that can be used to compare or copy the bitmap of
    /// the given `word` at a given candidate position in the `cloud`.
    #[inline]
    fn bit_iter(
        cloud: &Pixels,
        word_sprite: &Pixels,
        word_tl: Vec2,
    ) -> impl Iterator<Item = ((usize, u8), (usize, u8))> + use<> {
        let sprite_bounds = word_sprite.bounds();
        let sprite_iter = word_sprite.iter_indexes(&sprite_bounds);
        let board_iter = cloud.iter_indexes(&sprite_bounds.offset(word_tl));
        sprite_iter.zip(board_iter)
    }

    /// Tries to place the given `word` in the given `cloud`. If there is no room
    /// for the word, `None` is returned.
    fn place(
        rng: &mut SmallRng,
        cloud: &mut Pixels,
        dims: Vec2,
        all_words: &Rect,
        spiral: WordcloudSpiral,
        sprite: &Pixels,
    ) -> Option<Vec2> {
        let start = Vec2::new(
            ((dims.x * (rng.random::<f64>() + 0.5)) / 2.0).trunc(),
            ((dims.y * (rng.random::<f64>() + 0.5)) / 2.0).trunc(),
        );
        let mid = sprite.midpoint();

        let max_delta = dims.len();
        let mut spiral = spiral.generator(dims);
        let d_angle = if rng.random::<f64>() < 0.5 { 1.0 } else { -1.0 };
        let mut angle = -d_angle;

        loop {
            angle += d_angle;

            let delta = match &mut spiral {
                Either::Left(f) => f(angle),
                Either::Right(f) => f(angle),
            }
            .trunc();

            if delta.x.abs().min(delta.y.abs()) > max_delta {
                break None;
            }

            let origin = start + delta;
            let word_tl = origin - mid;
            let word_bounds = sprite.bounds().offset(word_tl);

            if !cloud.bounds().contains(word_bounds) {
                continue;
            }

            if (all_words.is_empty() || all_words.intersects(&word_bounds))
                && !collides(cloud, sprite, word_tl)
            {
                let iter = bit_iter(cloud, sprite, word_tl);
                for ((in_index, in_mask), (out_index, out_mask)) in iter {
                    if (sprite.data[in_index] & in_mask) != 0 {
                        cloud.data[out_index] |= out_mask;
                    }
                }
                break Some(origin);
            }
        }
    }

    /// Renders the given word to a bitmap for hit testing, returning the bitmap
    /// and an offset from the midpoint of the sprite box to the midpoint of the
    /// text anchor within the box.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "any value out of range of f32 is malformed and UB is fine"
    )]
    fn make_sprite(word: &Word<'_>) -> Option<(Pixels, Vec2)> {
        let bold =
            word.weight == "bold" || word.weight.parse::<u16>().is_ok_and(|weight| weight >= 600);
        let italic = word.style == "italic";

        let (path, mut offset) = shape_text(&word.text, bold, italic, |face, buffer| {
            let path = buffer_to_path(face, buffer)?;

            let stroke = Stroke {
                width: (word.padding * 2.0) as f32,
                ..Default::default()
            };

            let transform = {
                let scale = (word.size / f64::from(face.units_per_em())) as f32;
                SkiaTransform::from_rotate(word.rotate as f32).post_scale(scale, scale)
            };

            // The word cloud algorithm uses the post-transform bounding box
            // midpoint for the origin of a word, but the Vega word cloud example
            // thinks that the output coordinate will be at the alphabetic baseline.
            // This means it is necessary to calculate the offset from the bounding
            // box midpoint to the text anchor so it can be translated later.
            let mut origin = Point::from_xy(path.bounds().width() / 2.0, 0.0);
            transform.map_point(&mut origin);

            Some((path.transform(transform)?.stroke(&stroke, 1.0)?, origin))
        })?;

        let bounds = path.bounds();

        // This converts the text anchor so that it is relative to the bounding box
        // midpoint
        SkiaTransform::from_translate(
            -bounds.left() - bounds.width() / 2.0,
            -bounds.top() - bounds.height() / 2.0,
        )
        .map_point(&mut offset);

        #[expect(clippy::cast_sign_loss, reason = "width and height cannot be negative")]
        let (width, height) = (bounds.width().ceil() as u16, bounds.height().ceil() as u16);

        // Since the mask is 8bpp instead of 1bpp, the pixel limit has to be 8×
        // smaller. This could be avoided entirely by using tiled rendering instead,
        // which would allow the mask to be a single reusable allocation, but this
        // is a toy project, so who cares?
        assert!(
            usize::from(width) * usize::from(height) <= 4_096 * 4_096 / 8,
            "sprite resource limit reached; maximum size is 4096×4096"
        );

        let mut mask = Mask::new(width.into(), height.into()).unwrap();
        mask.fill_path(
            &path,
            FillRule::Winding,
            false,
            SkiaTransform::from_translate(-bounds.left(), -bounds.top()),
        );

        let mut sprite = Pixels::new(width, height);
        for ((index, mask), pixel) in sprite.iter_indexes(&sprite.bounds()).zip(mask.data()) {
            // pixel is either 0 or 0xFF, so `& mask` is either 0 or `mask`
            sprite.data[index] |= mask & *pixel;
        }

        Some((sprite, Vec2::new(offset.x.into(), offset.y.into())))
    }
}

// SPDX-SnippetEnd
