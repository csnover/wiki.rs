//! Types and functions for runtime data transformations.

use super::{
    Node,
    data::{ValueExt, get_nested_value, unit_enum_to_str},
    expr::Ast,
    geo::{Projector, ProjectorSettings},
    make_extent,
    renderer::{Pixels, Rect, Vec2, buffer_to_path, shape_text},
};
use core::cmp::Ordering;
use either::Either;
use indexmap::{IndexMap, IndexSet};
use rand::{Rng as _, rngs::SmallRng};
use regex::{Regex, RegexBuilder};
use serde_json_borrow::Value;
use std::borrow::Cow;
use tiny_skia::{FillRule, Mask, Point, Stroke, Transform as SkiaTransform};

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
    Cross(Todo),
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
    Force(Todo),
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
    LinkPath(Todo),
    /// Extends a primary data set by looking up values on a secondary data set.
    ///
    /// In other words, performs a join that adds new properties onto the
    /// primary data set only.
    ///
    /// Lookup accepts one or more key values for the primary data set, each of
    /// which are then searched for within a single key field of the secondary
    /// data set. If a match is found, the full data object in the secondary
    /// data set is added as a property of the primary data set.
    Lookup(Todo),
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
    Treemap(Todo),
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
    pub fn transform(&self, node: &Node<'s, '_>, data: Vec<Value<'s>>) -> Vec<Value<'s>> {
        match self {
            Transform::Aggregate(aggregate) => aggregate.transform(&data, None),
            Transform::Bin(bin) => bin.transform(data),
            Transform::CountPattern(count_pattern) => count_pattern.transform(&data),
            Transform::Cross(_) => todo!(),
            Transform::Facet(facet) => facet.transform(node, &data),
            Transform::Filter(filter) => filter.transform(node, data),
            Transform::Fold(_) => todo!(),
            Transform::Force(_) => todo!(),
            Transform::Formula(formula) => formula.transform(node, data),
            Transform::Geo(geo) => geo.transform(data),
            Transform::Geopath(geopath) => geopath.transform(data),
            Transform::Hierarchy(_) => todo!(),
            Transform::Impute(_) => todo!(),
            Transform::LinkPath(_) => todo!(),
            Transform::Lookup(_) => todo!(),
            Transform::Pie(pie) => pie.transform(data),
            Transform::Rank(_) => todo!(),
            Transform::Sort(_) => todo!(),
            Transform::Stack(stack) => stack.transform(data),
            Transform::Treeify(_) => todo!(),
            Transform::Treemap(_) => todo!(),
            Transform::Voronoi(_) => todo!(),
            Transform::Wordcloud(wordcloud) => wordcloud.transform(node, data),
        }
    }
}

/// An accumulator for an aggregate operation on a collection of data.
pub(super) struct Accumulator<'s, 'b> {
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
    #[serde(borrow, default, rename = "groupby")]
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
                op.op.apply(acc, value);
            }

            if faceting {
                AggregateOp::Values.apply(acc, item);
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
    pub(super) fn apply<'b, 's>(self, acc: &mut Accumulator<'s, 'b>, item: &'b Value<'s>) {
        if (acc.finished_ops & (1 << self as u8)) != 0 {
            return;
        }

        // Clippy: If there are ever >=2**53 values, something sure happened.
        #[allow(clippy::cast_precision_loss)]
        match self {
            Self::Argmax => {
                Self::Max.apply(acc, item);
                todo!()
            }
            Self::Argmin => {
                Self::Min.apply(acc, item);
                todo!()
            }
            Self::Count => acc.total += 1,
            Self::Distinct => {
                acc.distinct.insert(item);
            }
            Self::Max => acc.max = acc.max.max(item.as_f64().unwrap_or(0.0)),
            Self::Mean => {
                acc.mean_d = item.as_f64().unwrap_or(0.0) - acc.mean;
                acc.mean += acc.mean_d / acc.valid as f64;
            }
            Self::Median | Self::Q1 | Self::Q3 => {
                Self::Values.apply(acc, item);
            }
            Self::Min => acc.min = acc.min.min(item.as_f64().unwrap_or(0.0)),
            Self::Missing => acc.missing += usize::from(item.is_null()),
            Self::Modeskew => {
                Self::Mean.apply(acc, item);
                Self::Variance.apply(acc, item);
                Self::Median.apply(acc, item);
            }
            Self::Stdev | Self::Stdevp | Self::Variancep => {
                Self::Variance.apply(acc, item);
            }
            Self::Sum => acc.sum += item.as_f64().unwrap_or(0.0),
            Self::Valid => {
                acc.valid +=
                    usize::from(!item.is_null() && item.as_f64().is_none_or(|n| !n.is_nan()));
            }
            Self::Values => acc.values.push(item),
            Self::Variance => {
                Self::Mean.apply(acc, item);
                acc.dev += acc.mean_d * (item.as_f64().unwrap_or(0.0) - acc.mean);
            }
        }

        acc.finished_ops |= 1 << self as u8;
    }

    /// Finishes the processing of the operator, converting it to a final value.
    pub(super) fn finish<'s>(self, acc: &Accumulator<'s, '_>) -> Value<'s> {
        // Clippy: If there are ever >=2**53 values, something sure happened.
        #[allow(clippy::cast_precision_loss)]
        match self {
            Self::Argmax => todo!(),
            Self::Argmin => todo!(),
            Self::Count => Value::Number((acc.total as f64).into()),
            Self::Distinct => Value::Array(acc.distinct.iter().map(|v| (*v).clone()).collect()),
            Self::Max => Value::Number(acc.max.into()),
            Self::Mean => Value::Number(acc.mean.into()),
            Self::Median => Value::Number(quantile(&acc.values, 0.5).into()),
            Self::Min => Value::Number(acc.min.into()),
            Self::Missing => Value::Number((acc.missing as f64).into()),
            Self::Modeskew => Value::Number(
                if acc.dev == 0.0 {
                    0.0
                } else {
                    (acc.mean - quantile(&acc.values, 0.5))
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
            Self::Q1 => Value::Number(quantile(&acc.values, 0.25).into()),
            Self::Q3 => Value::Number(quantile(&acc.values, 0.75).into()),
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

impl Default for AggregateSpec<'_> {
    fn default() -> Self {
        Self::Short(IndexMap::from([(
            "*".into(),
            AggregateSpecShort::Single(AggregateOp::Values),
        )]))
    }
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
        // Clippy: Truncation is intended.
        #[allow(clippy::cast_possible_truncation)]
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
            start: Self::default_start(),
            mid: Self::default_mid(),
            end: Self::default_end(),
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
                    let count = counts.entry(key.to_string()).or_insert(0_u32);
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
            data = transform.transform(node, data);
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
    output: GeoOutput<'s>,
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

/// Geo output field names.
#[derive(Debug, serde::Deserialize)]
struct GeoOutput<'s> {
    /// The name of the x-coordinate field.
    #[serde(borrow, default = "GeoOutput::default_x")]
    pub x: Cow<'s, str>,
    /// The name of the y-coordinate field.
    #[serde(borrow, default = "GeoOutput::default_y")]
    pub y: Cow<'s, str>,
}

impl Default for GeoOutput<'_> {
    fn default() -> Self {
        Self {
            x: Self::default_x(),
            y: Self::default_y(),
        }
    }
}

impl GeoOutput<'_> {
    /// The default value for [`Self::x`].
    const fn default_x() -> Cow<'static, str> {
        Cow::Borrowed("layout_x")
    }

    /// The default value for [`Self::y`].
    const fn default_y() -> Cow<'static, str> {
        Cow::Borrowed("layout_y")
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
    pub path: Cow<'s, str>,
}

impl Default for GeopathOutput<'_> {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
        }
    }
}

impl GeopathOutput<'_> {
    /// The default value for [`Self::path`].
    const fn default_path() -> Cow<'static, str> {
        Cow::Borrowed("layout_path")
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

/// Pie output field names.
#[derive(Debug, serde::Deserialize)]
struct LayoutOutput<'s> {
    /// The name of the end field.
    #[serde(borrow, default = "LayoutOutput::default_end")]
    pub end: Cow<'s, str>,
    /// The name of the mid field.
    #[serde(borrow, default = "LayoutOutput::default_mid")]
    pub mid: Cow<'s, str>,
    /// The name of the start field.
    #[serde(borrow, default = "LayoutOutput::default_start")]
    pub start: Cow<'s, str>,
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

/// A stacked chart layout transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Stack<'s> {
    /// The data field that determines the thickness or height of each stack.
    #[serde(borrow)]
    field: Cow<'s, str>,
    /// A list of fields to partition the data into groups (stacks). When values
    /// are stacked vertically, this corresponds to the x-coordinates.
    #[serde(borrow, rename = "groupby")]
    group_by: Vec<Cow<'s, str>>,
    /// The baseline offset style.
    #[serde(default)]
    offset: StackOffset,
    /// The output field names.
    #[serde(borrow, default)]
    output: LayoutOutput<'s>,
    /// A list of fields to determine the order of stack layers.
    #[serde(borrow, default, rename = "sortby")]
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
                group.values.sort_unstable_by(|a, b| {
                    for key in &self.sort_by {
                        let (key, reverse) = if let Some(key) = key.strip_prefix("-") {
                            (key, true)
                        } else {
                            (key.strip_prefix("+").unwrap_or(key), false)
                        };

                        let a = get_nested_value(a, key).unwrap_or(&Value::Null);
                        let b = get_nested_value(b, key).unwrap_or(&Value::Null);
                        let cmp = a.fuzzy_total_cmp(b);
                        if cmp.is_ne() {
                            return if reverse { cmp.reverse() } else { cmp };
                        }
                    }
                    Ordering::Equal
                });
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

/// A stack chart layout group.
#[derive(Default)]
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

/// TODO.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Todo;

/// A word cloud layout transformer.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Wordcloud<'s> {
    /// The font face.
    #[serde(borrow, default = "Wordcloud::default_font")]
    font: WordcloudProperty<'s, Cow<'s, str>>,
    /// Rescale [`Self::font_size`] into the given output range using a square
    /// root scale, using the font sizes from the data set as the input domain.
    #[serde(default = "Wordcloud::default_font_scale")]
    font_scale: Option<[f64; 2]>,
    /// The font size, in pixels.
    #[serde(borrow, default = "Wordcloud::default_font_size")]
    font_size: WordcloudProperty<'s, f64>,
    /// The font style.
    #[serde(borrow, default = "Wordcloud::default_font_style")]
    font_style: WordcloudProperty<'s, Cow<'s, str>>,
    /// The font weight.
    #[serde(borrow, default = "Wordcloud::default_font_weight")]
    font_weight: WordcloudProperty<'s, Cow<'s, str>>,
    /// The output field names.
    #[serde(borrow, default)]
    output: WordcloudOutput<'s>,
    /// Extra padding around each word, in pixels.
    #[serde(borrow, default = "Wordcloud::default_padding")]
    padding: WordcloudProperty<'s, f64>,
    /// The rotation angle of a word, in degrees.
    #[serde(borrow, default = "Wordcloud::default_rotate")]
    rotate: WordcloudProperty<'s, f64>,
    /// The dimensions of the wordcloud layout, in pixels.
    #[serde(default = "Wordcloud::default_size")]
    size: [f64; 2],
    /// The type of spiral used to position words.
    #[serde(default = "Wordcloud::default_spiral")]
    spiral: WordcloudSpiral,
    /// The word text.
    #[serde(borrow, default = "Wordcloud::default_text")]
    text: WordcloudProperty<'s, Cow<'s, str>>,
}

impl<'s> Wordcloud<'s> {
    /// The default value for [`Self::font`].
    const fn default_font() -> WordcloudProperty<'s, Cow<'s, str>> {
        WordcloudProperty::Value(Cow::Borrowed("sans-serif"))
    }

    /// The default value for [`Self::font_scale`].
    const fn default_font_scale() -> Option<[f64; 2]> {
        Some([10.0, 50.0])
    }

    /// The default value for [`Self::font_size`].
    const fn default_font_size() -> WordcloudProperty<'s, f64> {
        WordcloudProperty::Value(14.0)
    }

    /// The default value for [`Self::font_style`].
    const fn default_font_style() -> WordcloudProperty<'s, Cow<'s, str>> {
        WordcloudProperty::Value(Cow::Borrowed("normal"))
    }

    /// The default value for [`Self::font_weight`].
    const fn default_font_weight() -> WordcloudProperty<'s, Cow<'s, str>> {
        WordcloudProperty::Value(Cow::Borrowed("normal"))
    }

    /// The default value for [`Self::padding`].
    const fn default_padding() -> WordcloudProperty<'s, f64> {
        WordcloudProperty::Value(1.0)
    }

    /// The default value for [`Self::rotate`].
    const fn default_rotate() -> WordcloudProperty<'s, f64> {
        WordcloudProperty::Value(0.0)
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
    const fn default_text() -> WordcloudProperty<'s, Cow<'s, str>> {
        WordcloudProperty::Short(Cow::Borrowed("data"))
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

                Word {
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

        for (word, pos) in word_cloud(
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

/// A wordcloud transform property.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum WordcloudProperty<'s, T> {
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

impl<'s> WordcloudProperty<'s, Cow<'s, str>> {
    /// Gets the value.
    fn get(&self, item: &Value<'s>) -> Cow<'s, str> {
        match self {
            WordcloudProperty::Value(value) => value.clone(),
            WordcloudProperty::Short(field) | WordcloudProperty::Long { field } => {
                get_nested_value(item, field).map_or(<_>::default(), ValueExt::to_string)
            }
        }
    }
}

impl<'s> WordcloudProperty<'s, f64> {
    /// Gets the value.
    fn get(&self, item: &Value<'s>) -> f64 {
        match self {
            WordcloudProperty::Value(value) => *value,
            WordcloudProperty::Short(field) | WordcloudProperty::Long { field } => {
                get_nested_value(item, field).map_or(<_>::default(), ValueExt::to_f64)
            }
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
                    // Clippy: Truncation does not matter since the purpose is
                    // to check the low bits of the whole number part.
                    #[allow(clippy::cast_possible_truncation)]
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

/// Returns the value at the given quantile `p` from the given list of `values`.
fn quantile(values: &[&Value<'_>], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut values = values
        .iter()
        .filter_map(|value| {
            if value.is_null() {
                return None;
            }
            let value = value.to_f64();
            if value.is_nan() {
                return None;
            }
            Some(value)
        })
        .collect::<Vec<_>>();
    values.sort_unstable_by(f64::total_cmp);

    // Clippy: If there are ever >=2**53 values, something sure happened.
    #[allow(clippy::cast_precision_loss)]
    let split_at = (values.len() - 1) as f64 * p + 1.0;
    let index = split_at.floor();
    let interp = split_at - index;
    // Clippy: Truncation and sign loss are impossible since the original value
    // was a usize and the multiplier is in the range 0..=1.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = index as usize;
    let value = values[index - 1];
    if interp == 0.0 {
        value
    } else {
        value + interp * (values[index] - value)
    }
}

// SPDX-SnippetBegin
// SPDX-License-Identifier: BSD-3-clause
// SPDX-SnippetComment: Adapted from d3-cloud 1.2.8 by Jason Davies

/// A word cloud word.
struct Word<'s> {
    /// The original index of the associated item.
    index: usize,
    /// The font family.
    font: Cow<'s, str>,
    /// The padding around the word.
    padding: f64,
    /// The rotation angle, in degrees.
    rotate: f64,
    /// The font size.
    size: f64,
    /// The font style.
    style: Cow<'s, str>,
    /// The word text.
    text: Cow<'s, str>,
    /// The font weight.
    weight: Cow<'s, str>,
}

/// Calculates coordinates for the list of `words` using the given `spiral` and
/// `rng` for the initial position randomisation. The list of words should be
/// pre-sorted in descending font size.
fn word_cloud<'s>(
    rng: &mut SmallRng,
    dims: Vec2,
    spiral: WordcloudSpiral,
    words: impl Iterator<Item = Word<'s>>,
) -> impl Iterator<Item = (Word<'s>, Vec2)> {
    // Clippy: The values are clamped to the range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut cloud = Pixels::new(
        dims.x.clamp(0.0, u16::MAX.into()) as u16,
        dims.y.clamp(0.0, u16::MAX.into()) as u16,
    );
    let mut all_words = Rect::default();
    words.filter_map(move |word| {
        let (sprite, offset) = word_cloud_sprite(&word)?;
        let pos = word_cloud_place(rng, &mut cloud, dims, &all_words, spiral, &sprite)?;
        all_words = all_words.union(&sprite.bounds().offset(pos - sprite.midpoint()));
        Some((word, pos + offset))
    })
}

/// Checks whether the given word `sprite` intersects with some word already
/// drawn to the word `cloud`.
#[inline]
fn word_cloud_collide(cloud: &Pixels, sprite: &Pixels, word_tl: Vec2) -> bool {
    let iter = word_cloud_iter(cloud, sprite, word_tl);
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
fn word_cloud_iter(
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
fn word_cloud_place(
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
            && !word_cloud_collide(cloud, sprite, word_tl)
        {
            let iter = word_cloud_iter(cloud, sprite, word_tl);
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
// Clippy: Truncation does not matter; if any value is outside the range of f32,
// it is malformed input.
#[allow(clippy::cast_possible_truncation)]
fn word_cloud_sprite(word: &Word<'_>) -> Option<(Pixels, Vec2)> {
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

    // Clippy: Width and height cannot be negative.
    #[allow(clippy::cast_sign_loss)]
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

// SPDX-SnippetEnd
