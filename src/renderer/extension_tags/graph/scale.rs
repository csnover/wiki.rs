//! Types and functions for scaling values between input domains and output
//! ranges.

use super::{
    Node, TimeExt as _,
    data::{ValueExt, get_nested_value, option_scalar, vec_scalar},
    make_extent,
    tick::{TimeInterval, tick_increment, ticks, time_ticks},
    transform::{Accumulator, AggregateOp},
};
use crate::php::DateTime;
use either::Either;
use indexmap::IndexMap;
use serde_json_borrow::Value;
use std::{
    borrow::Cow,
    cell::{OnceCell, RefCell, RefMut},
};

/// Maps an input value to an output value.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Scale<'s> {
    /// The input domain of the scale.
    ///
    /// For quantitative data, this is nominally a two-element array
    /// `[min, max]`, and is required unless `domain_min` and `domain_max` are
    /// both specified. The input value is normalised to the unit range.
    ///
    /// For ordinal (categorical) data, this is an array of discrete values, and
    /// the input value is converted to the matching index. If an input value
    /// is missing from the domain, it will be added.
    #[serde(borrow, default)]
    domain: Option<Domain<'s>>,
    /// The cached derived domain.
    #[serde(skip)]
    domain_cache: RefCell<Option<Vec<Value<'s>>>>,
    /// Sets or overrides the maximum of a quantitative domain.
    #[serde(borrow, default)]
    domain_max: Option<DomainValueOverride<'s>>,
    /// Sets or overrides the minimum of a quantitative domain.
    #[serde(borrow, default)]
    domain_min: Option<DomainValueOverride<'s>>,
    /// The scale kind.
    #[serde(default, flatten)]
    kind: Kind,
    /// The name of the scale.
    #[serde(borrow)]
    name: Cow<'s, str>,
    /// The output range of the scale.
    ///
    /// For quantitative data, this is a two-element array `[min, max]`, and is
    /// required unless `range_min` and `range_max` are both specified. The
    /// unit value is converted to the output range.
    ///
    /// For ordinal (categorical) data, this is an array of discrete values,
    /// mapped from the matching `domain` index by `index % range.len()`.
    #[serde(borrow, default)]
    range: Option<Range<'s>>,
    /// The cached derived range.
    #[serde(skip)]
    range_cache: OnceCell<CachedRange<'s>>,
    /// Sets or overrides the maximum of the output range.
    #[serde(borrow, default, deserialize_with = "option_scalar")]
    range_max: Option<Value<'s>>,
    /// Sets or overrides the minimum of the output range.
    #[serde(borrow, default, deserialize_with = "option_scalar")]
    range_min: Option<Value<'s>>,
    /// If true, reverses the output range.
    #[serde(borrow, default)]
    reverse: Reverse<'s>,
    /// If true, rounds the output to integer values. The default is `true` for
    /// ordinal scales, and `false` otherwise.
    #[serde(default)]
    round: Option<bool>,
}

impl<'s> Scale<'s> {
    /// Creates a new scale of the given `kind`, with the given `name`,
    /// `domain`, and `range`.
    pub fn new(
        kind: Kind,
        name: Cow<'s, str>,
        domain: Vec<Value<'s>>,
        range: Vec<Value<'s>>,
    ) -> Self {
        Self {
            domain: Some(domain.into()),
            domain_cache: <_>::default(),
            domain_max: <_>::default(),
            domain_min: <_>::default(),
            kind,
            name,
            range: Some(range.into()),
            range_cache: <_>::default(),
            range_max: <_>::default(),
            range_min: <_>::default(),
            reverse: <_>::default(),
            round: <_>::default(),
        }
    }

    /// Applies this scale to the given input value, returning the scaled value.
    pub fn apply(&self, node: &Node<'s, '_>, value: &Value<'s>, invert: bool) -> Value<'s> {
        match self.kind {
            Kind::Ordinal { .. } => {
                let position = {
                    // Because `self.domain` returns a `RefMut` it is important
                    // to minimise the lifetime of this reference. In
                    // particular, building the range cache may require
                    // accessing the domain from `calc_ordinal_range`, which
                    // would result in a double-borrow.
                    let domain = if invert {
                        Either::Left(self.range(node))
                    } else {
                        Either::Right(self.domain(node))
                    };

                    let (position, len) = {
                        let domain = domain
                            .as_ref()
                            .either(|range| &**range, |domain| domain.as_slice());
                        (
                            domain
                                .iter()
                                .position(|candidate| value.fuzzy_eq(candidate)),
                            domain.len(),
                        )
                    };

                    if let Some(position) = position {
                        position
                    } else if let Either::Right(mut domain) = domain {
                        // TODO: This should invalidate the range cache for an
                        // ordinal range since the band size will change.
                        domain.push(value.clone());
                        len
                    } else {
                        0
                    }
                };

                let range = if invert {
                    Either::Right(self.domain(node))
                } else {
                    Either::Left(self.range(node))
                };
                let range = range
                    .as_ref()
                    .either(|range| &**range, |domain| domain.as_slice());

                if range.is_empty() {
                    todo!("empty range")
                } else {
                    range[position % range.len()].clone()
                }
            }
            Kind::Time { clamp, .. } => {
                self.apply_quantitative(node, value, invert, QuantitativeScale::Linear, clamp)
            }
            Kind::Quantitative { kind, clamp, .. } => {
                self.apply_quantitative(node, value, invert, kind, clamp)
            }
        }
    }

    /// Applies a quantitative scale to the given input value, returning the
    /// scaled value.
    fn apply_quantitative(
        &self,
        node: &Node<'s, '_>,
        value: &Value<'s>,
        invert: bool,
        kind: QuantitativeScale,
        clamp: bool,
    ) -> Value<'s> {
        let unit_value = {
            let (min, max) = if invert {
                self.output_range(node)
            } else {
                self.input_range(node)
            };

            let value = (value.to_f64() - min) / (max - min);
            let value = match kind {
                QuantitativeScale::Log => value.log10(),
                QuantitativeScale::Linear
                | QuantitativeScale::Pow {
                    exponent: Some(1.0),
                } => value,
                QuantitativeScale::Sqrt
                | QuantitativeScale::Pow {
                    exponent: None | Some(0.5),
                } => value.signum() * value.abs().sqrt(),
                QuantitativeScale::Pow {
                    exponent: Some(exp),
                } => value.signum() * value.abs().powf(exp),
                kind => {
                    log::warn!("TODO: {kind:?} scale");
                    value
                }
            };
            if clamp { value.clamp(min, max) } else { value }
        };

        let (min, max) = if invert {
            self.input_range(node)
        } else {
            self.output_range(node)
        };

        let value = (max - min) * unit_value + min;
        Value::from(if self.round == Some(true) {
            value.round()
        } else {
            value
        })
    }

    /// Gets the minimum and maximum for a quantitative domain. The returned
    /// values are guaranteed to be sorted.
    pub fn input_range(&self, node: &Node<'s, '_>) -> (f64, f64) {
        let domain = self.domain(node);
        let min = domain.first().map_or(<_>::default(), ValueExt::to_f64);
        let max = domain.last().map_or(<_>::default(), ValueExt::to_f64);
        (min.min(max), max.max(min))
    }

    /// Returns true if this is a scale with discrete output values.
    pub fn is_discrete(&self) -> bool {
        matches!(
            self.kind,
            Kind::Ordinal { .. }
                | Kind::Quantitative {
                    kind: QuantitativeScale::Quantile
                        | QuantitativeScale::Quantize
                        | QuantitativeScale::Threshold,
                    ..
                }
        )
    }

    /// Returns true if this is an ordinal (categorical) scale.
    pub fn is_ordinal(&self) -> bool {
        matches!(self.kind, Kind::Ordinal { .. })
    }

    /// Returns true if this is a time scale.
    // TODO: Technically, this does need to discriminate local vs UTC.
    pub fn is_time(&self) -> bool {
        matches!(self.kind, Kind::Time { .. })
    }

    /// Gets the name of this scale.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the calculated width of the bands in the range.
    pub fn range_band(&self, node: &Node<'s, '_>) -> f64 {
        self.cached_range(node).band_width
    }

    /// Gets the sorted minimum and maximum for a range. The returned values are
    /// guaranteed to be sorted.
    pub fn range_extent(&self, node: &Node<'s, '_>) -> (f64, f64) {
        let (min, max) = self.output_range(node);
        (min.min(max), max.max(min))
    }

    /// Gets an iterator of approximately `count` representative values from
    /// the domain, or the domain itself if this is not a quantitative scale.
    pub fn ticks(&self, node: &Node<'s, '_>, count: f64) -> Vec<Value<'s>> {
        match self.kind {
            Kind::Time { .. } => {
                let (min, max) = self.input_range(node);
                time_ticks(min, max, count).map(Value::from).collect()
            }
            Kind::Quantitative { kind, .. } if kind.can_ticks() => {
                let (min, max) = self.input_range(node);
                kind.ticks(min, max, count).into_iter().flatten().collect()
            }
            _ => self.domain(node).iter().cloned().collect(),
        }
    }

    /// Calculates and returns the cached range.
    fn cached_range(&self, node: &Node<'s, '_>) -> &CachedRange<'s> {
        self.range_cache.get_or_init(|| {
            let mut range = self
                .range
                .as_ref()
                .map(|range| range.get(node))
                .unwrap_or_default();

            range_override(
                &mut range,
                self.range_min.as_ref(),
                <[_]>::first,
                <[_]>::first_mut,
            );
            range_override(
                &mut range,
                self.range_max.as_ref(),
                <[_]>::last,
                <[_]>::last_mut,
            );

            if matches!(self.reverse, Reverse::Value(true)) {
                range.to_mut().reverse();
            }

            match self.kind {
                Kind::Ordinal {
                    band_size,
                    outer_padding,
                    padding,
                    points,
                } => {
                    self.calc_ordinal_range(node, range, band_size, outer_padding, padding, points)
                }
                Kind::Time { .. } | Kind::Quantitative { .. } => {
                    let range = if matches!(self.range, Some(Range::Height)) {
                        range.to_mut().reverse();
                        range
                    } else {
                        range
                    };
                    CachedRange {
                        band_width: 0.0,
                        extent: None,
                        range,
                    }
                }
            }
        })
    }

    /// Calculates an ordinal range.
    fn calc_ordinal_range(
        &self,
        node: &Node<'s, '_>,
        range: Cow<'s, [Value<'s>]>,
        band_size: Option<f64>,
        outer_padding: Option<f64>,
        padding: Option<f64>,
        points: bool,
    ) -> CachedRange<'s> {
        let padding = padding.unwrap_or(0.0);
        let outer_padding = outer_padding.unwrap_or(padding);
        let range = if let Some(band_size) = band_size {
            // Clippy: If there are ever >=2**53 items, something sure happened.
            #[allow(clippy::cast_precision_loss)]
            let len = self.domain(node).len() as f64;
            let space = if points {
                padding * band_size
            } else {
                padding * band_size * (len - 1.0) + 2.0 * outer_padding
            };

            let start = range.first().map_or(0.0, ValueExt::to_f64);
            let end = range.get(1).map_or(0.0, ValueExt::to_f64);
            let (start, end) = if (start > end) == matches!(self.reverse, Reverse::Value(true)) {
                (start, start + (band_size * len + space))
            } else {
                (end + (band_size * len + space), end)
            };
            vec![start.into(), end.into()].into()
        } else {
            range
        };

        if matches!(range.first(), Some(Value::Str(_)))
            || !matches!(range.len(), 0 | 2)
            || matches!(self.range, Some(Range::Derived(_)))
        {
            CachedRange {
                band_width: 0.0,
                extent: None,
                range,
            }
        } else {
            let start = range.first().map_or(0.0, ValueExt::to_f64);
            let end = range.get(1).map_or(0.0, ValueExt::to_f64);
            // Clippy: There is no reasonable condition where there are >4B
            // values in the domain.
            #[allow(clippy::cast_possible_truncation)]
            let len = self.domain(node).len() as u32;
            let round = self.round.unwrap_or(true);

            let (step_start, step, reverse, band_width) = if points {
                let (start, step) = if len < 2 {
                    (start.midpoint(end), 0.0)
                } else {
                    (start, (end - start) / (f64::from(len) - 1.0 + padding))
                };

                let (start, step) = if round {
                    // rangeRoundPoints
                    let (start, end, step) = if len < 2 {
                        (start.round(), start.round(), 0.0)
                    } else {
                        (start, end, step.trunc())
                    };
                    let start = start
                        + (step * padding / 2.0
                            + (end - start - (f64::from(len) - 1.0 + padding) * step) / 2.0)
                            .round();
                    (start, step)
                } else {
                    // rangePoints
                    let start = start + step * padding / 2.0;
                    (start, step)
                };

                (start, step, false, 0.0)
            } else {
                let (start, end, reverse) = if end < start {
                    (end, start, true)
                } else {
                    (start, end, false)
                };

                let step = (end - start) / (f64::from(len) - padding + 2.0 * outer_padding);

                let (start, step, range_band) = if round {
                    // rangeRoundBands
                    let step = step.floor();
                    let range_band = (step * (1.0 - padding)).round();
                    (
                        start + ((end - start - (f64::from(len) - padding) * step) / 2.0).round(),
                        step,
                        range_band,
                    )
                } else {
                    // rangeBands
                    let range_band = step * (1.0 - padding);
                    (start + step * outer_padding, step, range_band)
                };

                (start, step, reverse, range_band)
            };

            let iter = (0..len).map(|i| Value::from(step_start + step * f64::from(i)));

            let range = Cow::Owned(if reverse {
                iter.rev().collect::<Vec<_>>()
            } else {
                iter.collect::<Vec<_>>()
            });

            CachedRange {
                band_width,
                extent: Some((start, end)),
                range,
            }
        }
    }

    /// Calculates a quantitative domain.
    fn calc_quantitative_domain(
        &self,
        node: &Node<'s, '_>,
        domain: Vec<Value<'s>>,
        kind: QuantitativeScale,
        zero: Option<bool>,
    ) -> (f64, f64) {
        // TODO: How the domain is calculated depends on the aggregate type as
        // determined by dataRef in src/scene/Scale.js and then expanded into
        // a transform in getCache. Very descriptive function names that
        // are certainly self-documenting. This is all technically supposed to
        // happen before this point so that the domain that this function
        // receives contains only aggregate data.

        let min = self.domain_min.as_ref().and_then(|min| min.get(node, 0));
        let max = self.domain_max.as_ref().and_then(|max| max.get(node, 1));
        let domain = domain.into_iter().map(ValueExt::into_f64);
        let (mut min, mut max) = make_extent(min, max, domain);

        if !matches!(zero, Some(false)) && !matches!(kind, QuantitativeScale::Log) {
            min = min.min(0.0);
            max = max.max(0.0);
        }

        (min, max)
    }

    /// Gets the calculated domain of the scale.
    fn domain(&self, node: &Node<'s, '_>) -> RefMut<'_, Vec<Value<'s>>> {
        RefMut::map(self.domain_cache.borrow_mut(), |cache| {
            cache.get_or_insert_with(|| {
                // TODO: Self-referential struct to avoid clones.
                let domain = self
                    .domain
                    .as_ref()
                    .map(|domain| domain.get(node))
                    .unwrap_or_default();

                match self.kind {
                    Kind::Ordinal { .. } => domain,
                    Kind::Time { nice, .. } => {
                        let (mut min, mut max) = self.calc_quantitative_domain(
                            node,
                            domain,
                            QuantitativeScale::Linear,
                            Some(false),
                        );
                        if let Some(nice) = nice {
                            (min, max) = make_nice_time(min, max, nice);
                        }
                        vec![min.into(), max.into()]
                    }
                    Kind::Quantitative {
                        kind, nice, zero, ..
                    } => {
                        if matches!(kind, QuantitativeScale::Quantile) {
                            domain
                        } else {
                            let (mut min, mut max) =
                                self.calc_quantitative_domain(node, domain, kind, zero);
                            if nice {
                                (min, max) = make_nice(min, max);
                            }
                            vec![min.into(), max.into()]
                        }
                    }
                }
            })
        })
    }

    /// Gets the minimum and maximum for a quantitative range.
    fn output_range(&self, node: &Node<'s, '_>) -> (f64, f64) {
        let cached_range = self.cached_range(node);
        if let Some(extent) = cached_range.extent {
            extent
        } else {
            let range = self.reverse(node, &cached_range.range);
            if range.len() < 3 {
                let min = range.first().map_or(0.0, ValueExt::to_f64);
                let max = range.last().map_or(0.0, ValueExt::to_f64);
                (min, max)
            } else {
                todo!("polymap");
            }
        }
    }

    /// Gets the calculated range of the scale.
    fn range(&self, node: &Node<'s, '_>) -> Cow<'_, [Value<'s>]> {
        self.reverse(node, &self.cached_range(node).range)
    }

    /// Reverses the output range if required by the associated node.
    fn reverse<'b>(&self, node: &Node<'s, '_>, range: &'b [Value<'s>]) -> Cow<'b, [Value<'s>]> {
        // This references a field on the associated group data object which is
        // different for each iteration, so cannot be cached. :-(
        if let Reverse::Field { field } = &self.reverse
            && get_nested_value(node.item, field).is_some_and(ValueExt::to_bool)
        {
            let mut range = range.to_vec();
            range.reverse();
            Cow::Owned(range)
        } else {
            Cow::Borrowed(range)
        }
    }
}

/// A cached range.
#[derive(Debug)]
struct CachedRange<'s> {
    /// For ordinal ranges with numeric inputs, the width of each band (bin).
    band_width: f64,
    /// The range extent. This is required for ordinal scales where
    /// [`Self::range`] is a calculated set of discrete values within the full
    /// range.
    extent: Option<(f64, f64)>,
    /// The range.
    range: Cow<'s, [Value<'s>]>,
}

/// A data input domain (range).
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum Domain<'s> {
    /// A list of fixed values.
    #[serde(borrow, deserialize_with = "vec_scalar")]
    Fixed(Vec<Value<'s>>),
    /// A derived domain.
    #[serde(borrow)]
    Derived(ScaleDataRef<'s>),
}

impl<'s> From<Vec<Value<'s>>> for Domain<'s> {
    #[inline]
    fn from(value: Vec<Value<'s>>) -> Self {
        Self::Fixed(value)
    }
}

impl<'s> Domain<'s> {
    /// Generates and returns the domain.
    fn get(&self, node: &Node<'s, '_>) -> Vec<Value<'s>> {
        match self {
            // Since the only caller just ends up converting to an owned list,
            // there is no reason to borrow here, even though it is possible
            Domain::Fixed(values) => values.clone(),
            Domain::Derived(data_ref) => data_ref.get(node),
        }
    }
}

/// A domain value override. Used to override the minimum or maximum of a
/// domain. This probably only makes sense for quantitative scales.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum DomainValueOverride<'s> {
    /// Fixed value.
    Fixed(f64),
    /// Derived value.
    #[serde(borrow)]
    Derived(ScaleDataRef<'s>),
}

impl<'s> DomainValueOverride<'s> {
    /// Gets the override value. If the value is derived from a data set, take
    /// the `index`th item from the set.
    fn get(&self, node: &Node<'s, '_>, index: usize) -> Option<f64> {
        match self {
            DomainValueOverride::Fixed(n) => Some(*n),
            DomainValueOverride::Derived(data_ref) => {
                data_ref.get(node).get(index).map(ValueExt::to_f64)
            }
        }
    }
}

/// The kind of a scale.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum Kind {
    /// An ordinal scale.
    Ordinal {
        /// “Width-defined range”. Undocumented.
        #[serde(default)]
        band_size: Option<f64>,
        /// Undocumented. Used to apply spacing to the outer edges of the scale
        /// range. If unspecified, `padding` will be used.
        #[serde(default)]
        outer_padding: Option<f64>,
        /// Applies spacing among ordinal elements in the scale range. The
        /// actual effect depends on how the scale is configured. If the
        /// `points` parameter is true, the padding value is interpreted as a
        /// multiple of the spacing between points. A reasonable value is `1.0`,
        /// such that the first and last point will be offset from the minimum
        /// and maximum value by half the distance between points. Otherwise,
        /// padding is typically in the range `0..=1` and corresponds to the
        /// fraction of space in the range interval to allocate to padding.
        /// A value of `0.5` means that the range band width will be equal to
        /// the padding width.
        #[serde(default)]
        padding: Option<f64>,
        /// If true, distributes the ordinal values over a quantitative range at
        /// uniformly spaced points. The spacing of the points can be adjusted
        /// using the `padding` property. Otherwise, the ordinal scale will
        /// construct evenly-spaced bands, rather than points.
        #[serde(default)]
        points: bool,
    },
    /// A time scale.
    #[serde(alias = "utc")]
    Time {
        /// If true, values that exceed the data domain are clamped to either
        /// the minimum or maximum range value.
        #[serde(default)]
        clamp: bool,
        /// If specified, modifies the scale domain to use a more human-friendly
        /// value range.
        #[serde(default)]
        nice: Option<NiceTime>,
    },
    /// A quantitative scale.
    #[serde(untagged)]
    Quantitative {
        /// The kind of quantitative scale.
        #[serde(flatten)]
        kind: QuantitativeScale,
        /// If true, values that exceed the data domain are clamped to either
        /// the minimum or maximum range value. (Only for linear, log, and
        /// sqrt.)
        #[serde(default)]
        clamp: bool,
        /// If true, modifies the scale domain to use a more human-friendly
        /// number range (e.g., 7 instead of 6.96). (Only for linear, log, and
        /// sqrt.)
        #[serde(default)]
        nice: bool,
        /// If true, ensures that a zero baseline value is included in the scale
        /// domain.
        #[serde(default)]
        zero: Option<bool>,
    },
}

/// A rounding mode for time intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum NiceTime {
    /// Round to nearest second.
    Second,
    /// Round to nearest minute.
    Minute,
    /// Round to nearest hour.
    Hour,
    /// Round to nearest day.
    Day,
    /// Round to nearest week.
    Week,
    /// Round to nearest month.
    Month,
    /// Round to nearest year.
    Year,
}

impl From<NiceTime> for TimeInterval {
    fn from(value: NiceTime) -> Self {
        match value {
            NiceTime::Second => TimeInterval::Second(1),
            NiceTime::Minute => TimeInterval::Minute(1),
            NiceTime::Hour => TimeInterval::Hour(1),
            NiceTime::Day => TimeInterval::Day(1),
            NiceTime::Week => TimeInterval::Week(1),
            NiceTime::Month => TimeInterval::Month(1),
            NiceTime::Year => TimeInterval::Year(1),
        }
    }
}

/// A kind of quantitative scale.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum QuantitativeScale {
    /// No transform.
    Linear,
    /// Apply a logarithmic transform of 10 to the input domain value.
    Log,
    /// Apply an exponential transform to the input domain value.
    Pow {
        /// The exponent of the transform. Defaults to 1.
        #[serde(default)]
        exponent: Option<f64>,
    },
    /// Apply an exponential transform of 0.5 to the input domain value.
    Sqrt,
    /// Maps a sampled input domain to a discrete range.
    Quantile,
    /// Divides a continuous input domain into uniform segments based on the
    /// number of values in the output range.
    Quantize,
    /// Similar to quantize, except maps arbitrary subsets of the domain to
    /// discrete values in the range.
    Threshold,
}

impl QuantitativeScale {
    /// Returns an iterator that generates approximately `count` evenly
    /// distributed steps for the given range.
    fn ticks<'s>(self, min: f64, max: f64, count: f64) -> Option<impl Iterator<Item = Value<'s>>> {
        match self {
            Self::Linear | Self::Pow { .. } | Self::Sqrt | Self::Quantize => {
                Some(Either::Left(ticks(min, max, count).map(Value::from)))
            }
            Self::Log => Some(Either::Right(log_ticks(min, max, count).map(Value::from))),
            Self::Quantile | Self::Threshold => None,
        }
    }

    /// Returns true if this kind of scale can calculate ticks.
    fn can_ticks(self) -> bool {
        !matches!(self, Self::Quantile | Self::Threshold)
    }
}

/// A scale range (output range).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Range<'s> {
    /// `[0, width]`, where `width` is the width of the nearest parent mark
    /// rectangle (or the root data rectangle, if there is no parent).
    Width,
    /// `[0, height]`, where `height` is the height of the nearest parent mark
    /// rectangle (or the root data rectangle, if there is no parent).
    Height,
    /// Predefined set of [10 colour strings](Self::CATEGORY_10).
    Category10,
    /// Predefined set of [20 colour strings](Self::CATEGORY_20).
    Category20,
    /// Predefined set of [20 colour strings](Self::CATEGORY_20_B).
    Category20b,
    /// Predefined set of [20 colour strings](Self::CATEGORY_20_C).
    Category20c,
    /// Predefined set of [shape strings](Self::SHAPES).
    Shapes,
    /// List of fixed values.
    #[serde(borrow, untagged, deserialize_with = "vec_scalar")]
    Fixed(Vec<Value<'s>>),
    /// List of derived values for an ordinal scale.
    #[serde(borrow, untagged)]
    Derived(ScaleDataRef<'s>),
}

impl<'s> Range<'s> {
    /// Predefined category 10 colours from D3.
    const CATEGORY_10: &'static [Value<'static>] = &[
        Value::Str(Cow::Borrowed("#1f77b4")),
        Value::Str(Cow::Borrowed("#ff7f0e")),
        Value::Str(Cow::Borrowed("#2ca02c")),
        Value::Str(Cow::Borrowed("#d62728")),
        Value::Str(Cow::Borrowed("#9467bd")),
        Value::Str(Cow::Borrowed("#8c564b")),
        Value::Str(Cow::Borrowed("#e377c2")),
        Value::Str(Cow::Borrowed("#7f7f7f")),
        Value::Str(Cow::Borrowed("#bcbd22")),
        Value::Str(Cow::Borrowed("#17becf")),
    ];

    /// Predefined category 20 colours from D3.
    const CATEGORY_20: &'static [Value<'static>] = &[
        Value::Str(Cow::Borrowed("#1f77b4")),
        Value::Str(Cow::Borrowed("#aec7e8")),
        Value::Str(Cow::Borrowed("#ff7f0e")),
        Value::Str(Cow::Borrowed("#ffbb78")),
        Value::Str(Cow::Borrowed("#2ca02c")),
        Value::Str(Cow::Borrowed("#98df8a")),
        Value::Str(Cow::Borrowed("#d62728")),
        Value::Str(Cow::Borrowed("#ff9896")),
        Value::Str(Cow::Borrowed("#9467bd")),
        Value::Str(Cow::Borrowed("#c5b0d5")),
        Value::Str(Cow::Borrowed("#8c564b")),
        Value::Str(Cow::Borrowed("#c49c94")),
        Value::Str(Cow::Borrowed("#e377c2")),
        Value::Str(Cow::Borrowed("#f7b6d2")),
        Value::Str(Cow::Borrowed("#7f7f7f")),
        Value::Str(Cow::Borrowed("#c7c7c7")),
        Value::Str(Cow::Borrowed("#bcbd22")),
        Value::Str(Cow::Borrowed("#dbdb8d")),
        Value::Str(Cow::Borrowed("#17becf")),
        Value::Str(Cow::Borrowed("#9edae5")),
    ];

    /// Predefined category 20 colours (variant B) from D3.
    const CATEGORY_20_B: &'static [Value<'static>] = &[
        Value::Str(Cow::Borrowed("#393b79")),
        Value::Str(Cow::Borrowed("#5254a3")),
        Value::Str(Cow::Borrowed("#6b6ecf")),
        Value::Str(Cow::Borrowed("#9c9ede")),
        Value::Str(Cow::Borrowed("#637939")),
        Value::Str(Cow::Borrowed("#8ca252")),
        Value::Str(Cow::Borrowed("#b5cf6b")),
        Value::Str(Cow::Borrowed("#cedb9c")),
        Value::Str(Cow::Borrowed("#8c6d31")),
        Value::Str(Cow::Borrowed("#bd9e39")),
        Value::Str(Cow::Borrowed("#e7ba52")),
        Value::Str(Cow::Borrowed("#e7cb94")),
        Value::Str(Cow::Borrowed("#843c39")),
        Value::Str(Cow::Borrowed("#ad494a")),
        Value::Str(Cow::Borrowed("#d6616b")),
        Value::Str(Cow::Borrowed("#e7969c")),
        Value::Str(Cow::Borrowed("#7b4173")),
        Value::Str(Cow::Borrowed("#a55194")),
        Value::Str(Cow::Borrowed("#ce6dbd")),
        Value::Str(Cow::Borrowed("#de9ed6")),
    ];

    /// Predefined category 20 colours (variant C) from D3.
    const CATEGORY_20_C: &'static [Value<'static>] = &[
        Value::Str(Cow::Borrowed("#3182bd")),
        Value::Str(Cow::Borrowed("#6baed6")),
        Value::Str(Cow::Borrowed("#9ecae1")),
        Value::Str(Cow::Borrowed("#c6dbef")),
        Value::Str(Cow::Borrowed("#e6550d")),
        Value::Str(Cow::Borrowed("#fd8d3c")),
        Value::Str(Cow::Borrowed("#fdae6b")),
        Value::Str(Cow::Borrowed("#fdd0a2")),
        Value::Str(Cow::Borrowed("#31a354")),
        Value::Str(Cow::Borrowed("#74c476")),
        Value::Str(Cow::Borrowed("#a1d99b")),
        Value::Str(Cow::Borrowed("#c7e9c0")),
        Value::Str(Cow::Borrowed("#756bb1")),
        Value::Str(Cow::Borrowed("#9e9ac8")),
        Value::Str(Cow::Borrowed("#bcbddc")),
        Value::Str(Cow::Borrowed("#dadaeb")),
        Value::Str(Cow::Borrowed("#636363")),
        Value::Str(Cow::Borrowed("#969696")),
        Value::Str(Cow::Borrowed("#bdbdbd")),
        Value::Str(Cow::Borrowed("#d9d9d9")),
    ];

    /// Predefined shapes.
    const SHAPES: &'static [Value<'static>] = &[
        Value::Str(Cow::Borrowed("circle")),
        Value::Str(Cow::Borrowed("cross")),
        Value::Str(Cow::Borrowed("diamond")),
        Value::Str(Cow::Borrowed("square")),
        Value::Str(Cow::Borrowed("triangle-down")),
        Value::Str(Cow::Borrowed("triangle-up")),
    ];

    /// Gets the range as a list of values.
    fn get(&self, node: &Node<'s, '_>) -> Cow<'s, [Value<'s>]> {
        match self {
            Range::Width => Cow::Owned(vec![Value::from(0.0), Value::from(node.width())]),
            Range::Height => Cow::Owned(vec![Value::from(0.0), Value::from(node.height())]),
            Range::Shapes => Cow::Borrowed(Self::SHAPES),
            Range::Category10 => Cow::Borrowed(Self::CATEGORY_10),
            Range::Category20 => Cow::Borrowed(Self::CATEGORY_20),
            Range::Category20b => Cow::Borrowed(Self::CATEGORY_20_B),
            Range::Category20c => Cow::Borrowed(Self::CATEGORY_20_C),
            // TODO: Self-referential struct to avoid clones.
            Range::Fixed(values) => Cow::Owned(values.clone()),
            Range::Derived(data_ref) => Cow::Owned(data_ref.get(node)),
        }
    }
}

impl<'s> From<Vec<Value<'s>>> for Range<'s> {
    #[inline]
    fn from(value: Vec<Value<'s>>) -> Self {
        Self::Fixed(value)
    }
}

/// A boolean reference.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum Reverse<'s> {
    /// Literal value.
    Value(bool),
    /// A reference to a value in the current group’s data object.
    Field {
        /// The name of the field.
        #[serde(borrow)]
        field: Cow<'s, str>,
    },
}

impl Default for Reverse<'_> {
    fn default() -> Self {
        Self::Value(false)
    }
}

/// A reference to a data source or list of data sources for deriving data.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ScaleDataRef<'s> {
    /// A single data source.
    Data {
        /// The source data set name.
        ///
        /// If not defined, takes the source field(s) from the data set of the
        /// parent mark.
        ///
        /// In the Vega JSON schema this was defined as being a string or an
        /// object with a `fields` property, but this latter thing is clearly an
        /// error. Vega code uses this field only as a key lookup and the
        /// documentation says that `fields` is for the *[`Scale::domain`]*
        /// property.
        #[serde(borrow, default)]
        data: Option<Cow<'s, str>>,
        /// The source field(s) from the data set.
        #[serde(borrow)]
        field: ScaleDataRefField<'s>,
        /// The sort operation for the derived data.
        #[serde(borrow, default)]
        sort: ScaleDataRefSort<'s>,
    },
    /// Multiple data sources.
    Fields {
        /// The list of source data sets.
        #[serde(borrow)]
        fields: Vec<ScaleDataRefPart<'s>>,
        /// The sort operation for the derived data.
        #[serde(borrow, default)]
        sort: ScaleDataRefSort<'s>,
    },
}

impl<'s> ScaleDataRef<'s> {
    /// Gets the derived data.
    fn get(&self, node: &Node<'s, '_>) -> Vec<Value<'s>> {
        let (parts, sort) = match self {
            Self::Data { data, field, sort } => (
                Either::Left(core::iter::once((data.as_deref(), field, None))),
                sort,
            ),
            Self::Fields { fields, sort } => (
                Either::Right(fields.iter().map(|source| {
                    (
                        source.data.as_deref(),
                        &source.field,
                        source.sort.as_deref(),
                    )
                })),
                sort,
            ),
        };

        let mut rows = IndexMap::<_, Accumulator<'s, '_>>::new();
        for (data_key, fields, sort_key) in parts {
            // If the data key is missing, this means to implicitly use the data
            // from the parent mark.
            let data = if let Some(name) = data_key {
                let Some(data) = node.data_values(name) else {
                    continue;
                };
                data
            } else {
                node.faceted_data()
            };

            for item in data {
                let sorter = if let ScaleDataRefSort::Aggregate { field, op } = sort
                    && let sort_key = sort_key.unwrap_or(field)
                    && let Some(value) = get_nested_value(item, sort_key)
                {
                    Some((op, value))
                } else {
                    None
                };

                for field in fields.iter(node) {
                    let Some(value) = get_nested_value(item, field) else {
                        continue;
                    };

                    if let Some((op, agg)) = sorter {
                        let acc = rows.entry(value).or_default();
                        op.apply(acc, agg);
                        acc.commit();
                    } else {
                        rows.entry(value).or_default();
                    }
                }
            }
        }

        match sort {
            ScaleDataRefSort::Natural(sort) => {
                let mut values = rows.into_keys().cloned().collect::<Vec<_>>();
                if *sort {
                    values.sort_unstable_by(ValueExt::fuzzy_total_cmp);
                }
                values
            }
            ScaleDataRefSort::Aggregate { op, .. } => {
                let mut values = rows
                    .into_iter()
                    .map(|(key, acc)| (key, op.finish(&acc)))
                    .collect::<Vec<_>>();
                values.sort_unstable_by(|(_, a), (_, b)| a.fuzzy_total_cmp(b));
                values
                    .into_iter()
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>()
            }
        }
    }
}

/// One of several data sources from which a domain or range is synthesised.
#[derive(Debug, serde::Deserialize)]
struct ScaleDataRefPart<'s> {
    /// The source data set name.
    ///
    /// If not defined, takes the source field(s) from the data set of the
    /// parent mark.
    ///
    /// In the Vega JSON schema this was defined as being a string or an object
    /// with a `fields` property, but this is incorrect. Vega uses this field
    /// only as a key for a data set lookup.
    #[serde(borrow, default)]
    data: Option<Cow<'s, str>>,
    /// The source field(s) from the data set.
    #[serde(borrow)]
    field: ScaleDataRefField<'s>,
    /// The name of the field to group by for sorting, overriding the sort field
    /// given in the parent [`DomainRefSort::Aggregate::field`].
    ///
    /// In the Vega JSON schema this was defined as being a string or a sort
    /// object, but this is incorrect. Vega uses this field only as a field name
    /// override for the parent aggregation field name.
    #[serde(borrow)]
    sort: Option<Cow<'s, str>>,
}

/// A field name or set of field names to use to create a derived data set.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum ScaleDataRefField<'s> {
    /// A single field name.
    #[serde(borrow)]
    Single(Cow<'s, str>),
    /// Multiple field names.
    #[serde(borrow)]
    Multiple(Vec<Cow<'s, str>>),
    /// An indirect field lookup. This key will be used to look up a value in
    /// the enclosing group’s data object, and then that value will be used as
    /// the key to look up the actual data.
    #[serde(borrow)]
    Parent(ScaleDataRefParent<'s>),
    /// Multiple indirect lookups.
    #[serde(borrow)]
    MultipleParent(Vec<ScaleDataRefParent<'s>>),
}

impl<'s> ScaleDataRefField<'s> {
    /// Gets the list of field names.
    fn iter<'b>(
        &'b self,
        node: &'b Node<'s, '_>,
    ) -> impl Iterator<Item = &'b Cow<'s, str>> + Clone {
        let slice = match self {
            Self::Single(field) => core::slice::from_ref(field),
            Self::Multiple(fields) => fields,
            Self::Parent(field) => get_nested_value(node.item, &field.parent)
                .and_then(ValueExt::as_cow)
                .map_or(<_>::default(), core::slice::from_ref),
            Self::MultipleParent(fields) => {
                return Either::Right(fields.iter().filter_map(|field| {
                    get_nested_value(node.item, &field.parent).and_then(ValueExt::as_cow)
                }));
            }
        };
        Either::Left(slice.iter())
    }
}

/// An indirect data field name.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ScaleDataRefParent<'s> {
    /// The data field name.
    #[serde(borrow)]
    parent: Cow<'s, str>,
}

/// Sorting options for a derived data set.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum ScaleDataRefSort<'s> {
    /// If true, sort values based on natural order (numeric or lexicographic,
    /// depending on the data type).
    Natural(bool),
    /// Sort values using an aggregate operation over a specified field.
    Aggregate {
        /// The field name to aggregate over.
        #[serde(borrow)]
        field: Cow<'s, str>,
        /// The operation.
        op: AggregateOp,
    },
}

impl Default for ScaleDataRefSort<'_> {
    fn default() -> Self {
        Self::Natural(false)
    }
}

// SPDX-SnippetBegin
// SPDX-License-Identifier: ISC
// SPDX-SnippetComment: Adapted from d3 3.5.17 by Mike Bostock
/// Generates an iterator for logarithmic ticks using base 10.
fn log_ticks(min: f64, max: f64, count: f64) -> impl Iterator<Item = f64> {
    const BASE: u32 = 10;

    #[inline]
    fn pows(n: f64) -> f64 {
        f64::from(BASE).powf(n)
    }

    #[inline]
    fn powi(n: i64) -> f64 {
        f64::from(BASE).powi(n.try_into().unwrap())
    }

    let reverse = max < min;
    let (min, max) = (min.min(max), max.max(min));

    let log_min = min.log10();
    let log_max = max.log10();

    let mut out = vec![];

    if log_max - log_min < count {
        // Clippy: Truncation is desirable as this is a to-int conversion.
        #[allow(clippy::cast_possible_truncation)]
        let (i_min, i_max) = (log_min.floor() as i64, log_max.ceil() as i64);
        if min > 0.0 {
            for i in i_min..=i_max {
                for k in 1..BASE {
                    let k = f64::from(k);
                    let value = if i < 0 { k / powi(-i) } else { k * powi(i) };
                    if value < min {
                        continue;
                    }
                    if value > max {
                        break;
                    }
                    out.push(value);
                }
            }
        } else {
            for i in i_min..=i_max {
                for k in (1..BASE).rev() {
                    let k = f64::from(k);
                    let value = if i > 0 { k / powi(-i) } else { k * powi(i) };
                    if value < min {
                        continue;
                    }
                    if value > max {
                        break;
                    }
                    out.push(value);
                }
            }
        }

        // Clippy: If there are ever >=2**52 ticks, something sure happened.
        #[allow(clippy::cast_precision_loss)]
        if out.len() as f64 * 2.0 < count {
            out = ticks(min, max, count).collect();
        }
    } else {
        out = ticks(log_min, log_max, count.min(log_max - log_min))
            .map(pows)
            .collect();
    }

    if reverse {
        out.reverse();
    }

    out.into_iter()
}

/// Rounds the given minimum and maximum to more human-friendly values.
fn make_nice(first: f64, last: f64) -> (f64, f64) {
    const COUNT: f64 = 10.0;

    let mut nice_first = first.min(last);
    let mut nice_last = last.max(first);

    let mut last_step = None;
    for _ in 0..10 {
        let step = tick_increment(nice_first, nice_last, COUNT);
        if Some(step) == last_step {
            return (nice_first, nice_last);
        } else if step > 0.0 {
            nice_first = (nice_first / step).floor() * step;
            nice_last = (nice_last / step).ceil() * step;
        } else if step < 0.0 {
            nice_first = (nice_first * step).ceil() / step;
            nice_last = (nice_last * step).floor() / step;
        } else {
            break;
        }
        last_step = Some(step);
    }

    (first, last)
}

/// Rounds the given minimum and maximum to more human-friendly values.
// Clippy: Numbers converted to Date in JS are truncated, ECMAScript 2026
// §§21.4.2.1, 21.4.1.31, and the maximum Date range is ±8.65e15, so precision
// loss is impossible/irrelevant.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn make_nice_time(first: f64, last: f64, nice: NiceTime) -> (f64, f64) {
    let start_time = DateTime::from_f64(first.min(last), false);
    let stop_time = DateTime::from_f64(last.max(first), false);
    let interval = TimeInterval::from(nice);
    let start = interval.floor(start_time).unix_timestamp() as f64 * 1_000.0;
    let stop = interval.ceil(stop_time).unix_timestamp() as f64 * 1_000.0;
    (start, stop)
}
// SPDX-SnippetEnd

/// Overrides a range value.
#[inline]
fn range_override<'s, G, M>(
    range: &mut Cow<'_, [Value<'s>]>,
    value: Option<&Value<'s>>,
    get: G,
    get_mut: M,
) where
    G: for<'a> FnOnce(&'a [Value<'s>]) -> Option<&'a Value<'s>>,
    M: for<'a> FnOnce(&'a mut [Value<'s>]) -> Option<&'a mut Value<'s>>,
{
    if let Some(value) = value
        && get(range) != Some(value)
    {
        let range = range.to_mut();
        if let Some(slot) = get_mut(range) {
            *slot = value.clone();
        } else {
            range.push(value.clone());
        }
    }
}
