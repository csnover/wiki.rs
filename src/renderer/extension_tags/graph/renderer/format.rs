//! D3-style string formatter.

// SPDX-SnippetBegin
// SPDX-License-Identifier: BSD-3-clause
// Adapted from Vega 2 by Trifacta, Inc., Univerity of Washington Interactive
// Data Lab

use super::super::{
    EPSILON, ScaleNode, TimeExt as _, axis::Format, data::ValueExt, scale::NiceTime,
    tick::TimeInterval,
};
use crate::{common::format_date_strftime, php::DateTime};
use core::num::{ParseIntError, TryFromIntError};
use regex::Regex;
use serde_json_borrow::Value;
use std::{borrow::Cow, sync::LazyLock};

/// The result type for formatting operations.
pub(crate) type Result<T, E = Error> = core::result::Result<T, E>;

/// A formatting error.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Bad align character.
    #[error("invalid align value '{}'", _0.escape_ascii())]
    Align(u8),
    /// Integer parsing error.
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    /// Integer range error.
    #[error(transparent)]
    TryFromInt(#[from] TryFromIntError),
    /// Regex match failed.
    #[error("does not match format spec: {0}")]
    Regex(String),
    /// Bad sign character.
    #[error("invalid sign value '{}'", _0.escape_ascii())]
    Sign(u8),
    /// Bad symbol character.
    #[error("invalid symbol value '{}'", _0.escape_ascii())]
    Symbol(u8),
}

/// A dynamic text formatter.
pub(super) type Formatter<'b, 's> = dyn Fn(&Value<'s>) -> Cow<'s, str> + 'b;

/// Creates a text formatter for the given `scale` with an optional non-default
/// `format_kind` and `format` string.
pub(super) fn make_formatter<'b, 's: 'b>(
    scale: ScaleNode<'s, '_>,
    format_kind: Option<Format>,
    format: &'b str,
    count: usize,
) -> Result<Box<Formatter<'b, 's>>> {
    let format_kind = format_kind.unwrap_or_else(|| {
        if scale.is_ordinal() {
            Format::String
        } else if scale.is_time() {
            Format::Time
        } else {
            Format::Number
        }
    });

    Ok(match format_kind {
        Format::Time | Format::Utc => {
            Box::new(move |value| {
                let utc = matches!(format_kind, Format::Utc);
                let date = DateTime::from_f64(value.to_f64(), utc);
                let format = if format.is_empty() {
                    auto_time_format(date)
                } else {
                    format
                };
                let date =
                    format_date_strftime(date, format.bytes()).expect("infallible date formatting");
                // SAFETY: The format string came from UTF-8.
                Cow::Owned(unsafe { String::from_utf8_unchecked(date) })
            })
        }
        Format::String => Box::new(ValueExt::to_string),
        Format::Number => {
            #[expect(
                clippy::cast_precision_loss,
                reason = "if there are ever ≥2**53 items, something sure happened"
            )]
            let count = count as f64;
            let number_formatter = auto_number_format(format, scale.input_range(), count)?;
            // TODO: This is supposed to do special things for log scales.
            Box::new(move |value| Cow::Owned(number_formatter.format(value.to_f64())))
        }
    })
}

/// Calculates a number formatting string that fits the given numeric range.
fn auto_number_format(
    format: &str,
    (min, max): (f64, f64),
    count: f64,
) -> Result<NumberFormatter<'static>> {
    const E10: f64 = 7.071_067_811_865_475_5 /* 50.0_f64.sqrt() */;
    const E5: f64 = 3.162_277_660_168_379_5 /* 10.0_f64.sqrt() */;
    const E2: f64 = core::f64::consts::SQRT_2;

    let (min, max) = (min.min(max), max.max(min));
    let span = max - min;
    let (span, count) = if span == 0.0 {
        (
            if min != 0.0 {
                min
            } else if max != 0.0 {
                max
            } else {
                1.0
            },
            1.0,
        )
    } else {
        (span, count)
    };
    let step = (span / count).log10().floor();
    let step = 10.0_f64.powf(step);
    let error = span / count / step;

    let step = if error >= E10 {
        step * 10.0
    } else if error >= E5 {
        step * 5.0
    } else if error >= E2 {
        step * 2.0
    } else {
        step
    };

    let (min, max) = (
        (min / step).ceil() * step,
        (max / step).floor() * step + step / 2.0,
    );

    let format = if format.is_empty() { ",f" } else { format };

    let mut spec = format.parse::<Specifier>()?;
    let max = min.abs().max(max.abs());
    match spec.kind {
        Kind::Default => {
            if spec.precision.is_none() {
                // TODO: case 's' or 'r' without precision are supposed to do
                // different things but they got smooshed together by the
                // parser and maybe this matters!!!
                spec.precision = Some(precision_prefix(step, max));
            }
        }
        Kind::Exponent | Kind::General => {
            if spec.precision.is_none() {
                let e = u8::from(spec.kind == Kind::Exponent);
                spec.precision = Some(precision_round(step, max) - e);
            }
        }
        Kind::Fixed => {
            if spec.precision.is_none() {
                let p = u8::from(spec.suffix == Some("%")) * 2;
                spec.precision = Some(precision_fixed(step).saturating_sub(p));
            }
        }
        _ => {}
    }

    Ok(NumberFormatter {
        locale: &Locale::EN_US,
        spec,
    })
}

/// Calculates a time formatting string for a date according to how distant the
/// date is from a set of intervals.
fn auto_time_format(date: DateTime) -> &'static str {
    fn is_nice(date: DateTime, nice: NiceTime) -> bool {
        TimeInterval::from(nice).floor(date) < date
    }

    if is_nice(date, NiceTime::Second) {
        ".%L"
    } else if is_nice(date, NiceTime::Minute) {
        ":%S"
    } else if is_nice(date, NiceTime::Hour) {
        "%I:%M"
    } else if is_nice(date, NiceTime::Day) {
        "%I %p"
    } else if is_nice(date, NiceTime::Month) {
        if is_nice(date, NiceTime::Week) {
            "%a %d"
        } else {
            "%b %d"
        }
    } else if is_nice(date, NiceTime::Year) {
        "%B"
    } else {
        "%Y"
    }
}

// SPDX-SnippetEnd

// SPDX-SnippetBegin
// SPDX-License-Identifier: ISC
// Adapted from d3 3.5.17 by Mike Bostock

/// Fill alignment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Align {
    /// Forces the field to be left-aligned within the available space.
    Left,
    /// Forces the field to be centered within the available space.
    Center,
    /// Forces the field to be right-aligned within the available space.
    #[default]
    Right,
    /// Like [`Self::Right`], but with any sign and symbol to the left of any
    /// padding.
    Symbol,
}

impl TryFrom<u8> for Align {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            b'<' => Self::Left,
            b'^' => Self::Center,
            b'>' => Self::Right,
            b'=' => Self::Symbol,
            _ => Err(Error::Align(value))?,
        })
    }
}

/// A localised number formatter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NumberFormatter<'a> {
    /// The locale of the formatter.
    locale: &'a Locale<'a>,
    /// The specifier of the formatter.
    spec: Specifier,
}

impl NumberFormatter<'_> {
    /// Creates a new number formatter for the default en-US locale and given
    /// specifier string.
    pub fn new(spec: &str) -> Result<Self> {
        Ok(Self {
            locale: &Locale::EN_US,
            spec: spec.parse()?,
        })
    }

    /// Formats the given value.
    pub fn format(&self, value: f64) -> String {
        self.spec.fmt(value, self.locale)
    }
}

/// Output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    /// Binary notation, rounded to integer. (`b`)
    Binary,
    /// Character data, for a string of text. (`c`)
    Character,
    /// Decimal notation, rounded to integer. (`d`)
    Decimal,
    /// Decimal notation with fractional part.
    Default,
    /// Exponential notation. (`e`)
    Exponent,
    /// Fixed point notation. (`f`)
    Fixed,
    /// Either decimal or exponent notation, rounded to significant digits. (`g`)
    General,
    /// Hexadecimal notation, using lower-case letters, rounded to integer. (`x`)
    HexLower,
    /// Hexadecimal notation, using upper-case letters, rounded to integer. (`X`)
    HexUpper,
    /// Octal notation, rounded to integer. (`o`)
    Octal,
    /// Decimal notation, rounded to significant digits. (`r`)
    Round,
}

impl Kind {
    /// Converts the given number `n` to a string with optional precision `p`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "truncation is desirable here"
    )]
    fn fmt(self, n: f64, p: Option<u8>) -> String {
        if n == f64::INFINITY {
            return "Infinity".into();
        } else if n == f64::NEG_INFINITY {
            return "-Infinity".into();
        }
        match self {
            Kind::Binary => format!("{n:b}", n = n as i64),
            Kind::Character =>
            {
                #[expect(clippy::cast_sign_loss, reason = "matches ES2026 §22.1.2.1")]
                char::from_u32((n as u16).into())
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
                    .to_string()
            }
            Kind::Decimal => {
                if n >= 1e21 {
                    fix_positive_exponent(format!("{n:e}"))
                } else {
                    format!("{n}", n = n as i64)
                }
            }
            Kind::Default => {
                if n >= 1e21 {
                    fix_positive_exponent(format!("{n:e}"))
                } else {
                    format!("{n}")
                }
            }
            Kind::Exponent => fix_positive_exponent(if let Some(p) = p {
                format!("{n:.p$e}", p = p.into())
            } else {
                format!("{n:e}")
            }),
            Kind::Fixed => format!("{n:.p$}", p = p.unwrap_or(0).into()),
            Kind::General => {
                // If there is no precision, the spec should have gone to
                // `Kind::Default`, since in ECMAScript, `Number#toPrecision`
                // called with an `undefined` precision is just an indirect call
                // to `ToString`.
                let p = i16::from(p.unwrap());
                let n_digits = exponent(n);

                let e = 10.0_f64.powi(i32::from(p - n_digits - 1));
                let n = (n * e).round() / e;
                if p <= n_digits {
                    let dec = n / 10.0_f64.powi(n_digits.into());
                    let sign = if n_digits < 0 { "-" } else { "+" };
                    let exp = n_digits.abs();
                    #[expect(clippy::cast_sign_loss, reason = "value comes from u8")]
                    let p = p as usize - 1;
                    format!("{dec:.p$}e{sign}{exp}")
                } else {
                    let p = e.log10();
                    assert!(
                        p.signum() > 0.0 && p <= f64::from(i16::MAX),
                        "impossible precision"
                    );
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "previous line asserts range"
                    )]
                    let p = p as usize;
                    format!("{n:.p$}")
                }
            }
            Kind::HexLower => format!("{n:x}", n = n as i64),
            Kind::HexUpper => format!("{n:X}", n = n as i64),
            Kind::Octal => format!("{n:o}", n = n as i64),
            Kind::Round => {
                let p = p.unwrap_or(0);
                let n = round(n, format_precision(n, p));
                let p = format_precision(n * (1.0 + 1e-15), p).clamp(0, 20);
                #[expect(clippy::cast_sign_loss, reason = "value is clamped to unsigned range")]
                let p = p as usize;
                format!("{n:.p$}")
            }
        }
    }

    /// Returns true if this kind is decimal.
    fn is_decimal(self) -> bool {
        matches!(
            self,
            Self::Decimal
                | Self::Exponent
                | Self::Fixed
                | Self::General
                | Self::Round
                | Self::Default
        )
    }

    /// Returns true if this kind expects an integer value.
    fn integer(self) -> bool {
        matches!(
            self,
            Self::Binary
                | Self::Character
                | Self::Decimal
                | Self::HexLower
                | Self::HexUpper
                | Self::Octal
        )
    }

    /// Returns the prefix string for this kind.
    fn prefix(self) -> &'static str {
        match self {
            Kind::Binary => "0b",
            Kind::HexLower | Kind::HexUpper => "0x",
            Kind::Octal => "0o",
            _ => "",
        }
    }
}

/// Massages the Rust exponent output to match the expected ECMAScript format.
fn fix_positive_exponent(mut f: String) -> String {
    if let Some(e) = f.rfind('e') {
        let e = e + 1;
        if !f[e..].starts_with('-') {
            f.insert(e, '+');
        }
    }
    f
}

/// A locale for number formatting.
#[derive(Clone, Copy, Debug)]
pub(super) struct Locale<'a> {
    /// Currency prefix and suffix.
    currency: Option<(&'a str, &'a str)>,
    /// Decimal separator.
    decimal: Option<&'a str>,
    /// Minus sign.
    minus: &'a str,
    /// Not-a-number string.
    nan: &'a str,
    /// Number of digits per formatted number group.
    grouping: Option<&'a [usize]>,
    /// Grouping separator.
    thousands: Option<&'a str>,
}

impl Locale<'_> {
    /// An en-US locale.
    pub const EN_US: Self = Self {
        currency: Some(("$", "")),
        decimal: Some("."),
        minus: "-",
        nan: "NaN",
        grouping: Some(&[3]),
        thousands: Some(","),
    };

    /// Groups numbers together according to locale-specific rules, optionally
    /// limited by `width`.
    fn format_group<'a>(&self, value: &'a str, width: Option<u8>) -> Cow<'a, str> {
        let (Some(grouping), Some(thousands)) = (self.grouping, self.thousands) else {
            return Cow::Borrowed(value);
        };

        let mut sizes = grouping
            .iter()
            .copied()
            .cycle()
            .take_while(|count| *count != 0);

        let width = width.map_or(usize::MAX, usize::from);

        let mut cursor = value.len();
        let mut len = 0;
        let mut groups = vec![];
        let sep_chars = thousands.chars().count();
        while cursor != 0
            && len <= width
            && let Some(mut chunk_size) = sizes.next()
        {
            let out_size = chunk_size + sep_chars;
            if len + out_size > width {
                chunk_size = width.saturating_sub(len).max(1);
            }
            let next = value[..cursor]
                .char_indices()
                .nth_back(chunk_size - 1)
                .map_or(0, |(index, _)| index);
            groups.push(&value[next..cursor]);
            cursor = next;
            len += out_size;
        }

        groups.reverse();

        Cow::Owned(groups.join(thousands))
    }
}

/// Sign display.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Sign {
    /// Nothing for zero or positive and a minus sign for negative.
    #[default]
    Minus,
    /// A plus sign for zero or positive and a minus sign for negative.
    Plus,
    /// A space for zero or positive and a minus sign for negative.
    Space,
}

impl Sign {
    /// The string representation of this sign for a positive number.
    fn as_str(self) -> &'static str {
        match self {
            Sign::Minus => "",
            Sign::Plus => "+",
            Sign::Space => " ",
        }
    }
}

impl TryFrom<u8> for Sign {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            b'-' => Self::Minus,
            b'+' => Self::Plus,
            b' ' => Self::Space,
            _ => Err(Error::Sign(value))?,
        })
    }
}

/// A format specifier.
#[derive(Clone, Copy, Debug)]
struct Specifier {
    /// The padding fill character. Defaults to ' '.
    fill: char,
    /// The output alignment.
    align: Align,
    /// The sign display.
    sign: Sign,
    /// The symbol display.
    symbol: Option<Symbol>,
    /// Minimum field width.
    width: Option<u8>,
    /// Use locale group separators.
    comma: bool,
    /// Depending on the type, the precision either indicates the number of
    /// digits that follow the decimal point (types `f` and `%`), or the number
    /// of significant digits.
    precision: Option<u8>,
    /// The output value scale.
    scale: Option<u8>,
    /// The output format.
    kind: Kind,
    /// Character suffix.
    suffix: Option<&'static str>,
}

impl Specifier {
    /// Formats the given `value` using the given `locale`.
    fn fmt(&self, value: f64, locale: &Locale<'_>) -> String {
        if self.kind.integer() && value.fract() > EPSILON {
            return <_>::default();
        }

        let (prefix, suffix) = match self.symbol {
            Some(Symbol::Prefix) => (self.kind.prefix(), self.suffix.unwrap_or_default()),
            Some(Symbol::Currency) => locale.currency.unwrap_or_default(),
            None => ("", self.suffix.unwrap_or_default()),
        };

        let (value, sign) = if value.signum() < 0.0 {
            (-value, locale.minus)
        } else {
            (value, self.sign.as_str())
        };

        let (value, suffix) = if let Some(scale) = self.scale {
            (value * f64::from(scale), Cow::Borrowed(suffix))
        } else {
            let unit = UnitFormatter::with_value(value, self.precision);
            let symbol = unit.symbol;
            let suffix = if suffix.is_empty() {
                Cow::Borrowed(symbol)
            } else {
                Cow::Owned(format!("{symbol}{suffix}"))
            };
            ((unit.scale)(value), suffix)
        };

        let value_str = if value.is_nan() {
            Cow::Borrowed(locale.nan)
        } else {
            Cow::Owned(self.kind.fmt(value, self.precision))
        };

        let (whole_part, fract_part) = if self.kind.is_decimal() {
            let end = value_str
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(value_str.len());
            let fract_part = if let Some(after) = value_str[end..].strip_prefix('.') {
                Cow::Owned(format!("{}{after}", locale.decimal.unwrap_or(".")))
            } else {
                Cow::Borrowed(&value_str[end..])
            };
            (&value_str[..end], fract_part)
        } else {
            (&*value_str, Cow::Borrowed(""))
        };

        // When the padding fill character is '0' it should be treated as part
        // of the number for the purpose of locale number grouping and sign
        // placement
        let group_after_pad = self.fill == '0' && self.comma;

        let whole_part = if self.comma && !group_after_pad && value.is_finite() {
            locale.format_group(whole_part, None)
        } else {
            Cow::Borrowed(whole_part)
        };

        #[rustfmt::skip]
        let formatted_len = prefix.chars().count()
            + whole_part.chars().count()
            + fract_part.chars().count()
            + suffix.chars().count()
            + if group_after_pad { 0 } else { sign.chars().count() };

        let padding = if let Some(width) = self.width
            && let Some(len) = usize::from(width).checked_sub(formatted_len)
        {
            core::iter::repeat_n(self.fill, len).collect::<String>()
        } else {
            <_>::default()
        };

        let whole_part = if group_after_pad && value.is_finite() {
            let value = format!("{padding}{whole_part}");
            let width = if padding.is_empty() {
                None
            } else {
                self.width.map(|width| {
                    let after_len = u8::try_from(fract_part.chars().count()).unwrap();
                    width.saturating_sub(after_len)
                })
            };
            locale.format_group(&value, width).into_owned().into()
        } else {
            whole_part
        };

        match self.align {
            Align::Left => {
                format!("{sign}{prefix}{whole_part}{fract_part}{suffix}{padding}")
            }
            Align::Right => {
                format!("{padding}{sign}{prefix}{whole_part}{fract_part}{suffix}")
            }
            Align::Center => {
                // The padding may have an odd number of characters so using the
                // same half twice would break
                let (left_pad, right_pad) = padding.split_at(padding.len().div_ceil(2));
                format!("{left_pad}{sign}{prefix}{whole_part}{fract_part}{suffix}{right_pad}")
            }
            Align::Symbol => {
                let padding = if group_after_pad { "" } else { &padding };
                format!("{sign}{prefix}{padding}{whole_part}{fract_part}{suffix}")
            }
        }
    }
}

impl core::str::FromStr for Specifier {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        static RE: LazyLock<Regex> = LazyLock::new(|| {
            regex::RegexBuilder::new(
                r"^(?:([^{])?([<>=^]))?([+\- ])?([$#])?(0)?(\d+)?(,)?(\.-?\d+)?([a-z%])?$",
            )
            .case_insensitive(true)
            .build()
            .unwrap()
        });

        let matches = RE.captures(s).ok_or(Error::Regex(s.to_owned()))?;

        let zero = matches.get(5).is_some();
        let fill = if zero {
            '0'
        } else {
            matches
                .get(1)
                .map_or(' ', |m| m.as_str().chars().next().unwrap())
        };

        let align = if zero {
            Align::Symbol
        } else {
            Align::try_from(matches.get(2).map_or(b'>', |m| m.as_str().as_bytes()[0]))?
        };
        let sign = Sign::try_from(matches.get(3).map_or(b'-', |m| m.as_str().as_bytes()[0]))?;
        let symbol = matches
            .get(4)
            .map(|m| Symbol::try_from(m.as_str().as_bytes()[0]))
            .transpose()?;
        let width = matches
            .get(6)
            .map(|m| m.as_str().parse::<u8>())
            .transpose()?;

        let mut comma = matches.get(7).is_some();
        let mut precision = matches
            .get(8)
            .map(|m| m.as_str()[1..].parse::<i16>())
            .transpose()?
            .map(|p| u8::try_from(p.max(0)))
            .transpose()?;
        let mut scale = Some(1);
        let mut suffix = None;

        let kind = match matches.get(9).map_or(b'\0', |m| m.as_str().as_bytes()[0]) {
            kind @ (b'%' | b'p') => {
                scale = Some(100);
                suffix = Some("%");
                if kind == b'%' {
                    Kind::Fixed
                } else if precision.is_none() {
                    Kind::Default
                } else {
                    Kind::Round
                }
            }
            b'b' => Kind::Binary,
            b'c' => Kind::Character,
            b'd' => Kind::Decimal,
            b'e' => Kind::Exponent,
            b'f' => Kind::Fixed,
            b'g' if precision.is_some() => Kind::General,
            b'n' => {
                comma = true;
                if precision.is_some() {
                    Kind::General
                } else {
                    Kind::Default
                }
            }
            b'o' => Kind::Octal,
            b'r' if precision.is_none() => Kind::Default,
            b'r' => Kind::Round,
            b's' => {
                scale = None;
                if precision.is_none() {
                    Kind::Default
                } else {
                    Kind::Round
                }
            }
            b'x' => Kind::HexLower,
            b'X' => Kind::HexUpper,
            _ => Kind::Default,
        };

        if let Some(precision) = &mut precision {
            if kind == Kind::General {
                *precision = (*precision).clamp(1, 21);
            } else if matches!(kind, Kind::Exponent | Kind::Fixed) {
                *precision = (*precision).clamp(0, 20);
            }
        }

        Ok(Self {
            fill,
            align,
            sign,
            symbol,
            width,
            comma,
            precision,
            scale,
            kind,
            suffix,
        })
    }
}

/// Symbol display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Symbol {
    /// Apply currency symbols per the locale definition.
    Currency,
    /// For binary, octal, or hexadecimal notation, prefix by `0b`, `0o`, or
    /// `0x`, respectively.
    Prefix,
}

impl TryFrom<u8> for Symbol {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            b'$' => Self::Currency,
            b'#' => Self::Prefix,
            _ => Err(Error::Symbol(value))?,
        })
    }
}

/// An SI unit formatter.
struct UnitFormatter {
    /// The scaling function for the value.
    scale: fn(f64) -> f64,
    /// The symbolic suffix for the scaled value.
    symbol: &'static str,
}

impl UnitFormatter {
    /// Gets an SI unit formatter for the given value and optional precision.
    fn with_value(value: f64, precision: Option<u8>) -> &'static Self {
        macro_rules! scales {
            ($($symbol:literal => $mult:literal),* $(,)?) => {
                &[
                    $(UnitFormatter {
                        scale: |d: f64| {
                            // Using division unconditionally causes floating
                            // point errors which show up in the output. Pray
                            // to the optimising compiler.
                            const MULT: f64 = $mult.abs();
                            if $mult > 0.0 { d / MULT } else { d * MULT }
                        },
                        symbol: $symbol,
                    },)*
                ]
            }
        }

        const SCALES: &[UnitFormatter] = scales! {
            "y" => -1e24_f64,
            "z" => -1e21_f64,
            "a" => -1e18_f64,
            "f" => -1e15_f64,
            "p" => -1e12_f64,
            "n" => -1e9_f64,
            "µ" => -1e6_f64,
            "m" => -1e3_f64,
            "" => 1.0_f64,
            "k" => 1e3_f64,
            "M" => 1e6_f64,
            "G" => 1e9_f64,
            "T" => 1e12_f64,
            "P" => 1e15_f64,
            "E" => 1e18_f64,
            "Z" => 1e21_f64,
            "Y" => 1e24_f64,
        };

        let index = if value == 0.0 {
            0
        } else {
            let value = value.abs();
            let value = if let Some(precision) = precision
                && precision != 0
            {
                round(value, format_precision(value, precision))
            } else {
                value
            };
            // Truncation must happen after floor because the value may be
            // negative, which has a different behaviour to truncation.
            #[expect(clippy::cast_possible_truncation, reason = "intended behaviour")]
            (((1e-12 + value.log10()) / 3.0).floor() as isize).clamp(-8, 8)
        };

        &SCALES[8_usize.strict_add_signed(index)]
    }
}

/// Computes the decimal exponent of the specified number `x`.
#[expect(clippy::cast_possible_truncation, reason = "the range is -324..=308")]
fn exponent(x: f64) -> i16 {
    if x == 0.0 {
        0
    } else {
        x.abs().log10().floor() as i16
    }
}

/// Calculates the final precision for the given value and precision.
#[inline]
fn format_precision(value: f64, precision: u8) -> i16 {
    debug_assert!(value.signum() > 0.0);
    // This formula is almost the same as a log10, except that it occasionally
    // exhibits different behaviour. For example, 0.0̅1 does something different.
    // This is required to pass tests.
    #[expect(clippy::cast_possible_truncation, reason = "the range is -324..=308")]
    let weird_whole_digits = if value == 0.0 {
        1
    } else {
        (value.ln() / core::f64::consts::LN_10).ceil() as i16
    };
    i16::from(precision) - weird_whole_digits
}

/// Returns a suggested decimal precision for fixed point notation given the
/// specified numeric `step` value.
#[inline]
fn precision_fixed(step: f64) -> u8 {
    (-exponent(step.abs())).max(0).try_into().unwrap()
}

/// Returns a suggested decimal precision for use with [`UnitFormatter`] given
/// the specified numeric `step` and reference `value`.
#[inline]
fn precision_prefix(step: f64, value: f64) -> u8 {
    // This could stay in i16 and be truncated except that the exponent might be
    // negative and needs to round down instead of toward zero.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "guaranteed to be in range as `exponent` returns i16"
    )]
    let value = (f64::from(exponent(value)) / 3.0).floor() as i16;
    (value.clamp(-8, 8) * 3 - exponent(step.abs()))
        .max(0)
        .try_into()
        .unwrap()
}

/// Returns a suggested decimal precision for format types that round to
/// significant digits given the specified numeric `step` and `max` values.
#[inline]
fn precision_round(step: f64, max: f64) -> u8 {
    let step = step.abs();
    let max = max.abs() - step;
    ((exponent(max) - exponent(step)).max(0) + 1)
        .try_into()
        .unwrap()
}

/// Rounds the given number at the given precision.
#[inline]
fn round(value: f64, precision: i16) -> f64 {
    if precision != 0 {
        let p = 10_f64.powi(precision.into());
        (value * p).round() / p
    } else {
        value.round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precision_fixed() {
        assert_eq!(precision_fixed(8.9), 0);
        assert_eq!(precision_fixed(1.1), 0);
        assert_eq!(precision_fixed(0.89), 1);
        assert_eq!(precision_fixed(0.11), 1);
        assert_eq!(precision_fixed(0.089), 2);
        assert_eq!(precision_fixed(0.011), 2);
    }

    #[test]
    fn test_precision_prefix_same_unit() {
        for i in (-24..24).step_by(3) {
            for j in i..i + 3 {
                assert_eq!(precision_prefix(10_f64.powi(i), 10_f64.powi(j)), 0);
            }
        }
    }

    #[test]
    fn test_precision_prefix_fract() {
        for i in (-24..24).step_by(3) {
            for j in i - 4..i {
                assert_eq!(
                    i32::from(precision_prefix(10_f64.powi(j), 10_f64.powi(i))),
                    i - j,
                    "i = {i}, j = {j}"
                );
            }
        }
    }

    #[test]
    fn test_precision_prefix_extremis() {
        assert_eq!(precision_prefix(1e-24, 1e-24), 0); // 1y
        assert_eq!(precision_prefix(1e-25, 1e-25), 1); // 0.1y
        assert_eq!(precision_prefix(1e-26, 1e-26), 2); // 0.01y
        assert_eq!(precision_prefix(1e-27, 1e-27), 3); // 0.001y
        assert_eq!(precision_prefix(1e-28, 1e-28), 4); // 0.0001y
        assert_eq!(precision_prefix(1e24, 1e24), 0); // 1Y
        assert_eq!(precision_prefix(1e24, 1e25), 0); // 10Y
        assert_eq!(precision_prefix(1e24, 1e26), 0); // 100Y
        assert_eq!(precision_prefix(1e24, 1e27), 0); // 1000Y
        assert_eq!(precision_prefix(1e23, 1e27), 1); // 1000.0Y
    }

    #[test]
    fn test_precision_round() {
        assert_eq!(precision_round(0.1, 1.1), 2); // "1.0", "1.1"
        assert_eq!(precision_round(0.01, 0.99), 2); // "0.98", "0.99"
        assert_eq!(precision_round(0.01, 1.00), 2); // "0.99", "1.0"
        assert_eq!(precision_round(0.01, 1.01), 3); // "1.00", "1.01"
    }

    #[track_caller]
    fn format(spec: &str, value: f64) -> String {
        NumberFormatter::new(spec).unwrap().format(value)
    }

    #[test]
    fn zero_fill() {
        let f = NumberFormatter::new("08d").unwrap();
        assert_eq!(f.format(0.0), "00000000");
        assert_eq!(f.format(42.0), "00000042");
        assert_eq!(f.format(42_000_000.0), "42000000");
        assert_eq!(f.format(420_000_000.0), "420000000");
        assert_eq!(f.format(-4.0), "-0000004");
        assert_eq!(f.format(-42.0), "-0000042");
        assert_eq!(f.format(-4_200_000.0), "-4200000");
        assert_eq!(f.format(-42_000_000.0), "-42000000");
    }
    #[test]
    fn space_fill() {
        let f = NumberFormatter::new("8d").unwrap();
        assert_eq!(f.format(0.0), "       0");
        assert_eq!(f.format(42.0), "      42");
        assert_eq!(f.format(42_000_000.0), "42000000");
        assert_eq!(f.format(420_000_000.0), "420000000");
        assert_eq!(f.format(-4.0), "      -4");
        assert_eq!(f.format(-42.0), "     -42");
        assert_eq!(f.format(-4_200_000.0), "-4200000");
        assert_eq!(f.format(-42_000_000.0), "-42000000");
    }

    #[test]
    fn fixed() {
        assert_eq!(format(".1f", 0.49), "0.5");
        assert_eq!(format(".2f", 0.449), "0.45");
        assert_eq!(format(".3f", 0.4449), "0.445");
        assert_eq!(format(".5f", 0.444_449), "0.44445");
        assert_eq!(format(".1f", 100.0), "100.0");
        assert_eq!(format(".2f", 100.0), "100.00");
        assert_eq!(format(".3f", 100.0), "100.000");
        assert_eq!(format(".5f", 100.0), "100.00000");
    }

    #[test]
    fn general() {
        assert_eq!(format(".1g", 0.049), "0.05");
        assert_eq!(format(".1g", 0.49), "0.5");
        assert_eq!(format(".2g", 0.449), "0.45");
        assert_eq!(format(".3g", 0.4449), "0.445");
        assert_eq!(format(".5g", 0.444_449), "0.44445");
        assert_eq!(format(".1g", 100.0), "1e+2");
        assert_eq!(format(".2g", 100.0), "1.0e+2");
        assert_eq!(format(".3g", 100.0), "100");
        assert_eq!(format(".5g", 100.0), "100.00");
        assert_eq!(format(".5g", 100.2), "100.20");
        assert_eq!(format(".2g", 0.002), "0.0020");
    }

    #[test]
    fn exponent() {
        let f = NumberFormatter::new("e").unwrap();
        assert_eq!(f.format(0.0), "0e+0");
        assert_eq!(f.format(42.0), "4.2e+1");
        assert_eq!(f.format(42_000_000.0), "4.2e+7");
        assert_eq!(f.format(420_000_000.0), "4.2e+8");
        assert_eq!(f.format(-4.0), "-4e+0");
        assert_eq!(f.format(-42.0), "-4.2e+1");
        assert_eq!(f.format(-4_200_000.0), "-4.2e+6");
        assert_eq!(f.format(-42_000_000.0), "-4.2e+7");
        assert_eq!(format(".0e", 42.0), "4e+1");
        assert_eq!(format(".3e", 42.0), "4.200e+1");
    }

    #[test]
    fn si() {
        let f = NumberFormatter::new("s").unwrap();
        assert_eq!(f.format(0.0), "0");
        assert_eq!(f.format(1.0), "1");
        assert_eq!(f.format(10.0), "10");
        assert_eq!(f.format(100.0), "100");
        assert_eq!(f.format(999.5), "999.5");
        assert_eq!(f.format(999_500.0), "999.5k");
        assert_eq!(f.format(1000.0), "1k");
        assert_eq!(f.format(1400.0), "1.4k");
        assert_eq!(f.format(1500.5), "1.5005k");
        assert_eq!(f.format(0.000_001), "1µ");
    }

    #[test]
    fn si_round() {
        let f = NumberFormatter::new(".3s").unwrap();
        assert_eq!(f.format(0.0), "0.00");
        assert_eq!(f.format(1.0), "1.00");
        assert_eq!(f.format(10.0), "10.0");
        assert_eq!(f.format(100.0), "100");
        assert_eq!(f.format(999.5), "1.00k");
        assert_eq!(f.format(999_500.0), "1.00M");
        assert_eq!(f.format(1000.0), "1.00k");
        assert_eq!(f.format(1500.5), "1.50k");
        assert_eq!(f.format(145_500_000.0), "146M");
        assert_eq!(f.format(145_999_999.999_999_34), "146M");
        assert_eq!(f.format(1e26), "100Y");
        assert_eq!(f.format(0.000_001), "1.00µ");
        assert_eq!(f.format(0.009_995), "10.0m");
        let f = NumberFormatter::new(".4s").unwrap();
        assert_eq!(f.format(999.5), "999.5");
        assert_eq!(f.format(999_500.0), "999.5k");
        assert_eq!(f.format(0.009_995), "9.995m");
    }

    #[test]
    fn si_symbol() {
        let f = NumberFormatter::new("$.3s").unwrap();
        assert_eq!(f.format(0.0), "$0.00");
        assert_eq!(f.format(1.0), "$1.00");
        assert_eq!(f.format(10.0), "$10.0");
        assert_eq!(f.format(100.0), "$100");
        assert_eq!(f.format(999.5), "$1.00k");
        assert_eq!(f.format(999_500.0), "$1.00M");
        assert_eq!(f.format(1000.0), "$1.00k");
        assert_eq!(f.format(1500.5), "$1.50k");
        assert_eq!(f.format(145_500_000.0), "$146M");
        assert_eq!(f.format(145_999_999.999_999_34), "$146M");
        assert_eq!(f.format(1e26), "$100Y");
        assert_eq!(f.format(0.000_001), "$1.00µ");
        assert_eq!(f.format(0.009_995), "$10.0m");
        let f = NumberFormatter::new("$.4s").unwrap();
        assert_eq!(f.format(999.5), "$999.5");
        assert_eq!(f.format(999_500.0), "$999.5k");
        assert_eq!(f.format(0.009_995), "$9.995m");
    }

    #[test]
    fn si_range() {
        let f = NumberFormatter::new("s").unwrap();

        assert_eq!(
            [1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1e-0, 1e1, 1e2, 1e3, 1e4, 1e5].map(|n| f.format(n)),
            [
                "10µ", "100µ", "1m", "10m", "100m", "1", "10", "100", "1k", "10k", "100k"
            ]
        );

        let f = NumberFormatter::new(".4s").unwrap();
        assert_eq!(
            [1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1e-0, 1e1, 1e2, 1e3, 1e4, 1e5].map(|n| f.format(n)),
            [
                "10.00µ", "100.0µ", "1.000m", "10.00m", "100.0m", "1.000", "10.00", "100.0",
                "1.000k", "10.00k", "100.0k"
            ]
        );
    }

    #[test]
    fn currency() {
        let f = NumberFormatter::new("$").unwrap();
        assert_eq!(f.format(0.0), "$0");
        assert_eq!(f.format(0.042), "$0.042");
        assert_eq!(f.format(0.42), "$0.42");
        assert_eq!(f.format(4.2), "$4.2");
        assert_eq!(f.format(-0.042), "-$0.042");
        assert_eq!(f.format(-0.42), "-$0.42");
        assert_eq!(f.format(-4.2), "-$4.2");
    }

    #[test]
    fn currency_group_sign() {
        let f = NumberFormatter::new("+$,.2f").unwrap();
        assert_eq!(f.format(0.0), "+$0.00");
        assert_eq!(f.format(0.429), "+$0.43");
        assert_eq!(f.format(-0.429), "-$0.43");
        assert_eq!(f.format(-1.0), "-$1.00");
        assert_eq!(f.format(1e4), "+$10,000.00");
    }

    #[test]
    fn currency_si() {
        let f = NumberFormatter::new("$.2s").unwrap();
        assert_eq!(f.format(0.0), "$0.0");
        assert_eq!(f.format(2.5e5), "$250k");
        assert_eq!(f.format(-2.5e8), "-$250M");
        assert_eq!(f.format(2.5e11), "$250G");
    }

    #[test]
    fn percent() {
        let f = NumberFormatter::new("%").unwrap();
        assert_eq!(f.format(0.0), "0%");
        assert_eq!(f.format(0.042), "4%");
        assert_eq!(f.format(0.42), "42%");
        assert_eq!(f.format(4.2), "420%");
        assert_eq!(f.format(-0.042), "-4%");
        assert_eq!(f.format(-0.42), "-42%");
        assert_eq!(f.format(-4.2), "-420%");
    }

    #[test]
    fn percent_round_sign() {
        let f = NumberFormatter::new("+.2p").unwrap();
        assert_eq!(f.format(0.00123), "+0.12%");
        assert_eq!(f.format(0.0123), "+1.2%");
        assert_eq!(f.format(0.123), "+12%");
        assert_eq!(f.format(1.23), "+120%");
        assert_eq!(f.format(-0.00123), "-0.12%");
        assert_eq!(f.format(-0.0123), "-1.2%");
        assert_eq!(f.format(-0.123), "-12%");
        assert_eq!(f.format(-1.23), "-120%");
    }

    #[test]
    fn round_sig() {
        assert_eq!(format(".2r", 0.0), "0.0");
        assert_eq!(format(".1r", 0.049), "0.05");
        assert_eq!(format(".1r", -0.049), "-0.05");
        assert_eq!(format(".1r", 0.49), "0.5");
        assert_eq!(format(".1r", -0.49), "-0.5");
        assert_eq!(format(".2r", 0.449), "0.45");
        assert_eq!(format(".3r", 0.4449), "0.445");
        assert_eq!(format(".3r", 1.00), "1.00");
        assert_eq!(format(".3r", 0.9995), "1.00");
        assert_eq!(format(".5r", 0.444_449), "0.44445");
        assert_eq!(format("r", 123.45), "123.45");
        assert_eq!(format(".1r", 123.45), "100");
        assert_eq!(format(".2r", 123.45), "120");
        assert_eq!(format(".3r", 123.45), "123");
        assert_eq!(format(".4r", 123.45), "123.5");
        assert_eq!(format(".5r", 123.45), "123.45");
        assert_eq!(format(".6r", 123.45), "123.450");
        assert_eq!(format(".1r", 0.9), "0.9");
        assert_eq!(format(".1r", 0.09), "0.09");
        assert_eq!(format(".1r", 0.949), "0.9");
        assert_eq!(format(".1r", 0.0949), "0.09");
        assert_eq!(format(".10r", 0.999_999_999_9), "0.9999999999");
        assert_eq!(format(".15r", 0.999_999_999_999_999), "0.999999999999999");
    }

    #[test]
    fn round_small() {
        let f = NumberFormatter::new(".2r").unwrap();
        assert_eq!(f.format(1e-22), "0.00000000000000000000");
    }

    #[test]
    fn group() {
        let f = NumberFormatter::new(",d").unwrap();
        assert_eq!(f.format(0.0), "0");
        assert_eq!(f.format(42.0), "42");
        assert_eq!(f.format(42_000_000.0), "42,000,000");
        assert_eq!(f.format(420_000_000.0), "420,000,000");
        assert_eq!(f.format(-4.0), "-4");
        assert_eq!(f.format(-42.0), "-42");
        assert_eq!(f.format(-4_200_000.0), "-4,200,000");
        assert_eq!(f.format(-42_000_000.0), "-42,000,000");
        assert_eq!(f.format(1e21), "1e+21");
    }

    #[test]
    fn group_zero() {
        assert_eq!(format("01,d", 0.0), "0");
        assert_eq!(format("01,d", 0.0), "0");
        assert_eq!(format("02,d", 0.0), "00");
        assert_eq!(format("03,d", 0.0), "000");
        assert_eq!(format("04,d", 0.0), "0,000");
        assert_eq!(format("05,d", 0.0), "0,000");
        assert_eq!(format("06,d", 0.0), "00,000");
        assert_eq!(format("08,d", 0.0), "0,000,000");
        assert_eq!(format("013,d", 0.0), "0,000,000,000");
        assert_eq!(format("021,d", 0.0), "0,000,000,000,000,000");
        assert_eq!(format("013,d", -42_000_000.0), "-0,042,000,000");
        assert_eq!(format("012,d", 1e21), "0,000,001e+21");
        assert_eq!(format("013,d", 1e21), "0,000,001e+21");
        assert_eq!(format("014,d", 1e21), "00,000,001e+21");
        assert_eq!(format("015,d", 1e21), "000,000,001e+21");
    }

    #[test]
    fn group_zero_overflow() {
        assert_eq!(format("01,d", 1.0), "1");
        assert_eq!(format("01,d", 1.0), "1");
        assert_eq!(format("02,d", 12.0), "12");
        assert_eq!(format("03,d", 123.0), "123");
        assert_eq!(format("05,d", 12345.0), "12,345");
        assert_eq!(format("08,d", 12_345_678.0), "12,345,678");
        assert_eq!(format("013,d", 1_234_567_890_123.0), "1,234,567,890,123");
    }

    #[test]
    fn group_space() {
        assert_eq!(format("1,d", 0.0), "0");
        assert_eq!(format("1,d", 0.0), "0");
        assert_eq!(format("2,d", 0.0), " 0");
        assert_eq!(format("3,d", 0.0), "  0");
        assert_eq!(format("5,d", 0.0), "    0");
        assert_eq!(format("8,d", 0.0), "       0");
        assert_eq!(format("13,d", 0.0), "            0");
        assert_eq!(format("21,d", 0.0), "                    0");
    }

    #[test]
    fn group_space_overflow() {
        assert_eq!(format("1,d", 1.0), "1");
        assert_eq!(format("1,d", 1.0), "1");
        assert_eq!(format("2,d", 12.0), "12");
        assert_eq!(format("3,d", 123.0), "123");
        assert_eq!(format("5,d", 12345.0), "12,345");
        assert_eq!(format("8,d", 12_345_678.0), "12,345,678");
        assert_eq!(format("13,d", 1_234_567_890_123.0), "1,234,567,890,123");
    }

    #[test]
    fn group_general() {
        let f = NumberFormatter::new(",g").unwrap();
        assert_eq!(f.format(0.0), "0");
        assert_eq!(f.format(42.0), "42");
        assert_eq!(f.format(42_000_000.0), "42,000,000");
        assert_eq!(f.format(420_000_000.0), "420,000,000");
        assert_eq!(f.format(-4.0), "-4");
        assert_eq!(f.format(-42.0), "-42");
        assert_eq!(f.format(-4_200_000.00), "-4,200,000");
        assert_eq!(f.format(-42_000_000.0), "-42,000,000");
    }

    #[test]
    fn group_space_round() {
        assert_eq!(format("10,.1f", 123_456.49), " 123,456.5");
        assert_eq!(format("10,.2f", 1_234_567.449), "1,234,567.45");
        assert_eq!(format("10,.3f", 12_345_678.444_9), "12,345,678.445");
        assert_eq!(format("10,.5f", 123_456_789.444_449), "123,456,789.44445");
        assert_eq!(format("10,.1f", 123_456.0), " 123,456.0");
        assert_eq!(format("10,.2f", 1_234_567.0), "1,234,567.00");
        assert_eq!(format("10,.3f", 12_345_678.0), "12,345,678.000");
        assert_eq!(format("10,.5f", 123_456_789.0), "123,456,789.00000");
    }

    #[test]
    fn fixed_int() {
        assert_eq!(format("f", 42.0), "42");
    }
    #[test]
    fn int_float() {
        assert_eq!(format("d", 4.2), "");
    }

    #[test]
    fn char() {
        assert_eq!(format("c", 9731.0), "☃");
    }

    #[test]
    fn binary() {
        assert_eq!(format("b", 10.0), "1010");
        assert_eq!(format("#b", 10.0), "0b1010");
    }

    #[test]
    fn octal() {
        assert_eq!(format("o", 10.0), "12");
        assert_eq!(format("#o", 10.0), "0o12");
    }

    #[test]
    fn hex() {
        assert_eq!(format("x", 3_735_928_559.0), "deadbeef");
        assert_eq!(format("#x", 3_735_928_559.0), "0xdeadbeef");
        assert_eq!(format("X", 3_735_928_559.0), "DEADBEEF");
        assert_eq!(format("#X", 3_735_928_559.0), "0xDEADBEEF");
    }

    #[test]
    fn fill_prefix() {
        assert_eq!(format("#20x", 3_735_928_559.0), "          0xdeadbeef");
    }

    #[test]
    fn align_left() {
        assert_eq!(format("<1,d", 0.0), "0");
        assert_eq!(format("<1,d", 0.0), "0");
        assert_eq!(format("<2,d", 0.0), "0 ");
        assert_eq!(format("<3,d", 0.0), "0  ");
        assert_eq!(format("<5,d", 0.0), "0    ");
        assert_eq!(format("<8,d", 0.0), "0       ");
        assert_eq!(format("<13,d", 0.0), "0            ");
        assert_eq!(format("<21,d", 0.0), "0                    ");
    }

    #[test]
    fn align_right() {
        assert_eq!(format(">1,d", 0.0), "0");
        assert_eq!(format(">1,d", 0.0), "0");
        assert_eq!(format(">2,d", 0.0), " 0");
        assert_eq!(format(">3,d", 0.0), "  0");
        assert_eq!(format(">5,d", 0.0), "    0");
        assert_eq!(format(">8,d", 0.0), "       0");
        assert_eq!(format(">13,d", 0.0), "            0");
        assert_eq!(format(">21,d", 0.0), "                    0");
        assert_eq!(format(">21,d", 1000.0), "                1,000");
        assert_eq!(format(">21,d", 1e21), "                1e+21");
    }

    #[test]
    fn align_center() {
        assert_eq!(format("^1,d", 0.0), "0");
        assert_eq!(format("^1,d", 0.0), "0");
        assert_eq!(format("^2,d", 0.0), " 0");
        assert_eq!(format("^3,d", 0.0), " 0 ");
        assert_eq!(format("^5,d", 0.0), "  0  ");
        assert_eq!(format("^8,d", 0.0), "    0   ");
        assert_eq!(format("^13,d", 0.0), "      0      ");
        assert_eq!(format("^21,d", 0.0), "          0          ");
        assert_eq!(format("^21,d", 1000.0), "        1,000        ");
        assert_eq!(format("^21,d", 1e21), "        1e+21        ");
    }

    #[test]
    fn pad_after_sign() {
        assert_eq!(format("=+1,d", 0.0), "+0");
        assert_eq!(format("=+1,d", 0.0), "+0");
        assert_eq!(format("=+2,d", 0.0), "+0");
        assert_eq!(format("=+3,d", 0.0), "+ 0");
        assert_eq!(format("=+5,d", 0.0), "+   0");
        assert_eq!(format("=+8,d", 0.0), "+      0");
        assert_eq!(format("=+13,d", 0.0), "+           0");
        assert_eq!(format("=+21,d", 0.0), "+                   0");
        assert_eq!(format("=+21,d", 1e21), "+               1e+21");
    }

    #[test]
    fn pad_after_sign_currency() {
        assert_eq!(format("=+$1,d", 0.0), "+$0");
        assert_eq!(format("=+$1,d", 0.0), "+$0");
        assert_eq!(format("=+$2,d", 0.0), "+$0");
        assert_eq!(format("=+$3,d", 0.0), "+$0");
        assert_eq!(format("=+$5,d", 0.0), "+$  0");
        assert_eq!(format("=+$8,d", 0.0), "+$     0");
        assert_eq!(format("=+$13,d", 0.0), "+$          0");
        assert_eq!(format("=+$21,d", 0.0), "+$                  0");
        assert_eq!(format("=+$21,d", 1e21), "+$              1e+21");
    }

    #[test]
    fn space_positive() {
        assert_eq!(format(" 1,d", -1.0), "-1");
        assert_eq!(format(" 1,d", 0.0), " 0");
        assert_eq!(format(" 2,d", 0.0), " 0");
        assert_eq!(format(" 3,d", 0.0), "  0");
        assert_eq!(format(" 5,d", 0.0), "    0");
        assert_eq!(format(" 8,d", 0.0), "       0");
        assert_eq!(format(" 13,d", 0.0), "            0");
        assert_eq!(format(" 21,d", 0.0), "                    0");
        assert_eq!(format(" 21,d", 1e21), "                1e+21");
    }

    #[test]
    fn sign_only_neg() {
        assert_eq!(format("-1,d", -1.0), "-1");
        assert_eq!(format("-1,d", 0.0), "0");
        assert_eq!(format("-2,d", 0.0), " 0");
        assert_eq!(format("-3,d", 0.0), "  0");
        assert_eq!(format("-5,d", 0.0), "    0");
        assert_eq!(format("-8,d", 0.0), "       0");
        assert_eq!(format("-13,d", 0.0), "            0");
        assert_eq!(format("-21,d", 0.0), "                    0");
    }

    #[test]
    fn negative_zero() {
        assert_eq!(format("1d", -0.0), "-0");
        assert_eq!(format("1f", -0.0), "-0");
    }

    #[test]
    fn n_alias_comma_g() {
        let f = NumberFormatter::new("n").unwrap();
        assert_eq!(f.format(0.0042), "0.0042");
        assert_eq!(f.format(0.42), "0.42");
        assert_eq!(f.format(0.0), "0");
        assert_eq!(f.format(42.0), "42");
        assert_eq!(f.format(42_000_000.0), "42,000,000");
        assert_eq!(f.format(420_000_000.0), "420,000,000");
        assert_eq!(f.format(-4.0), "-4");
        assert_eq!(f.format(-42.0), "-42");
        assert_eq!(f.format(-4_200_000.0), "-4,200,000");
        assert_eq!(f.format(-42_000_000.0), "-42,000,000");
        assert_eq!(f.format(1e21), "1e+21");
    }

    #[test]
    fn n_zero() {
        // assert_eq!(format("01n", 0.0), "0");
        // assert_eq!(format("01n", 0.0), "0");
        // assert_eq!(format("02n", 0.0), "00");
        // assert_eq!(format("03n", 0.0), "000");
        // assert_eq!(format("05n", 0.0), "0,000");
        assert_eq!(format("08n", 0.0), "0,000,000");
        assert_eq!(format("013n", 0.0), "0,000,000,000");
        assert_eq!(format("021n", 0.0), "0,000,000,000,000,000");
        assert_eq!(format("013n", -42_000_000.0), "-0,042,000,000");
    }

    #[test]
    fn stupid_precisions() {
        assert_eq!(format(".30f", 0.0), "0.00000000000000000000");
        assert_eq!(format(".0g", 1.0), "1");
        assert_eq!(format(",.-1f", 12345.0), "12,345");
        assert_eq!(format("+,.-1%", 123.45), "+12,345%");
    }
}

// SPDX-SnippetEnd
