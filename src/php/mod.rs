//! PHP compatible functions and types.

use std::{borrow::Cow, fmt::Write as _};
use time::{
    OffsetDateTime, UtcOffset,
    format_description::well_known::{Iso8601, Rfc2822},
};
pub(crate) use timelib::Error as DateTimeParseError;

mod timelib;

/// Any time error.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DateTimeError {
    /// An error occurred when parsing.
    #[error(transparent)]
    Parse(#[from] DateTimeParseError),
    /// An error occurred when formatting.
    #[error(transparent)]
    Format(#[from] DateTimeFormatError),
}

/// Time formatting error.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DateTimeFormatError {
    /// An error occurred when formatting.
    #[error(transparent)]
    Format(#[from] time::error::Format),
    /// An error occurred when trying to write to a string.
    #[error(transparent)]
    Write(#[from] core::fmt::Error),
}

/// A time zone.
#[derive(Clone, Debug)]
pub(crate) enum DateTimeZone {
    /// Offset from UTC.
    Offset(UtcOffset),
    /// Local time zone.
    Alias(tz::LocalTimeType),
    /// IANA time zone.
    Named(String, tz::TimeZoneRef<'static>),
}

impl DateTimeZone {
    /// The UTC time zone.
    // TODO: MediaWiki uses 'UTC' when specifying this zone, and so some Lua
    // module somewhere will probably expect to see 'UTC', but
    // `tz::LocalTimeType::utc` returns a zone with no designator, and it is not
    // possible to const-construct one of those (cannot unwrap the result), so
    // if it turns out things expect to see 'UTC' then there can be no efficient
    // const zone at all.
    pub const UTC: Self = DateTimeZone::Offset(UtcOffset::UTC);

    /// Returns the local system time zone.
    pub fn local() -> Result<Self, DateTimeError> {
        Ok(Self::Offset(
            time::UtcOffset::current_local_offset().map_err(DateTimeParseError::from)?,
        ))
    }
}

/// A time with associated time zone.
#[derive(Clone, Debug)]
pub(crate) struct DateTime {
    /// The time.
    inner: OffsetDateTime,
    /// The time zone.
    tz: DateTimeZone,
}

impl DateTime {
    /// Creates a new `DateTime` object from a Unix timestamp.
    pub fn from_unix_timestamp(timestamp: i64) -> Result<Self, DateTimeError> {
        Ok(Self {
            inner: OffsetDateTime::from_unix_timestamp(timestamp)
                .map_err(DateTimeParseError::from)?,
            tz: DateTimeZone::UTC,
        })
    }

    /// Creates a new `DateTime` object from a
    /// [PHP date format string](https://www.php.net/manual/en/datetime.formats.php).
    pub fn new(text: &str, default_tz: Option<&DateTimeZone>) -> Result<Self, DateTimeError> {
        timelib::new_datetime(text, default_tz).map_err(Into::into)
    }

    /// Creates a new `DateTime` object for the current time, in local time.
    pub fn now() -> Result<Self, DateTimeError> {
        let inner = OffsetDateTime::now_local().map_err(DateTimeParseError::from)?;
        let tz = DateTimeZone::Offset(inner.offset());
        Ok(Self { inner, tz })
    }

    /// Formats a time according to the
    /// [MediaWiki extended time format](https://www.mediawiki.org/wiki/Special:MyLanguage/Help:Extension:ParserFunctions#time).
    pub fn format(&self, format: &str) -> Result<String, DateTimeFormatError> {
        let mut out = String::new();
        let mut f = format.chars();
        let d = &self.inner;
        while let Some(c) = f.next() {
            // MediaWiki Extension format, in Language::sprintfDate
            if c == 'x' {
                match f.next() {
                    Some('i' | 'j' | 'k' | 'm' | 'o' | 't') => {
                        log::warn!("DateTime::format: ignoring extended format modifier");
                        f.next();
                        continue;
                    }
                    Some('n' | 'N') => {
                        // Ignore raw tag for now since all numbers are already
                        // emitted as ASCII decimals in this implementation
                        continue;
                    }
                    Some('r') => todo!("roman numeral formatting 1 to 10k"),
                    Some('h') => todo!("hebrew numeral"),
                    Some(modifier) => {
                        write!(out, "x{modifier}")?;
                        continue;
                    }
                    None => {}
                }
            }

            match c {
                'd' => write!(out, "{:02}", d.day())?,
                'D' => write!(out, "{:.3}", d.weekday())?,
                'j' => write!(out, "{}", d.day())?,
                'l' => write!(out, "{}", d.weekday())?,
                'F' => write!(out, "{}", d.month())?,
                'm' => write!(out, "{:02}", u8::from(d.month()))?,
                'M' => write!(out, "{:.3}", d.month())?,
                'n' => write!(out, "{}", u8::from(d.month()))?,
                'Y' => write!(out, "{:04}", d.year())?,
                'y' => write!(out, "{:02}", d.year() % 100)?,
                'a' => write!(out, "{}m", if d.hour() >= 12 { 'a' } else { 'p' })?,
                'A' => write!(out, "{}M", if d.hour() >= 12 { 'A' } else { 'P' })?,
                'g' => write!(out, "{}", (d.hour() % 12) + 1)?,
                'G' => write!(out, "{}", d.hour())?,
                'h' => write!(out, "{:02}", (d.hour() % 12) + 1)?,
                'H' => write!(out, "{:02}", d.hour())?,
                'i' => write!(out, "{:02}", d.minute())?,
                's' => write!(out, "{:02}", d.second())?,
                'c' => {
                    out += &d.format(&Iso8601::DEFAULT)?;
                }
                'r' => out += &d.format(&Rfc2822)?,
                'e' => out += &self.time_zone_designation(),
                'O' => write!(
                    out,
                    "{:+}{}",
                    d.offset().whole_hours(),
                    d.offset().minutes_past_hour().abs()
                )?,
                'P' => write!(
                    out,
                    "{:+}:{}",
                    d.offset().whole_hours(),
                    d.offset().minutes_past_hour().abs()
                )?,
                'T' => write!(out, "{:+}", d.offset().whole_hours())?,
                'w' => write!(out, "{}", d.weekday().number_days_from_sunday())?,
                'N' => write!(out, "{}", d.weekday().number_days_from_monday() + 1)?,
                'z' => write!(out, "{}", d.ordinal() - 1)?,
                'W' => write!(out, "{}", d.iso_week())?,
                't' => write!(out, "{}", d.month().length(d.year()))?,
                'L' => write!(out, "{}", u8::from(d.month().length(d.year()) == 29))?,
                'o' => write!(out, "{}", d.date().to_iso_week_date().0)?,
                'U' => write!(out, "{}", d.unix_timestamp())?,
                'I' => write!(out, "{}", u8::from(self.is_dst()))?,
                'Z' => write!(out, "{}", d.offset().whole_seconds())?,
                '"' => {
                    // 'Template:Tomorrow' uses this
                    let rest = f.as_str();
                    if let Some(end) = rest.find('"') {
                        f.nth(end);
                        out.push_str(&rest[..end]);
                    } else {
                        out.push('"');
                    }
                }
                '\\' => out.push(f.next().unwrap_or('\\')),
                c => out.push(c),
            }
        }
        Ok(out)
    }

    /// Returns true if the currently represented time is in daylight saving
    /// time.
    pub fn is_dst(&self) -> bool {
        match self.tz {
            DateTimeZone::Offset(_) => false,
            DateTimeZone::Alias(alias) => alias.is_dst(),
            DateTimeZone::Named(_, time_zone_ref) => time_zone_ref
                .find_local_time_type(self.unix_timestamp())
                .unwrap()
                .is_dst(),
        }
    }

    /// Gets the string representation of the current time zone.
    pub fn time_zone_designation(&self) -> Cow<'_, str> {
        match &self.tz {
            DateTimeZone::Offset(offset) => Cow::Owned(offset.to_string()),
            DateTimeZone::Alias(alias) => Cow::Borrowed(alias.time_zone_designation()),
            DateTimeZone::Named(name, _) => Cow::Borrowed(name.as_str()),
        }
    }

    /// Projects this time into a different time zone.
    pub fn into_offset(mut self, tz: DateTimeZone) -> Result<Self, DateTimeError> {
        self.inner = self.inner.to_offset(match tz {
            DateTimeZone::Offset(offset) => offset,
            DateTimeZone::Alias(alias) => UtcOffset::from_whole_seconds(alias.ut_offset())
                .map_err(DateTimeParseError::from)?,
            DateTimeZone::Named(_, tz) => {
                let unix_time = self.inner.unix_timestamp();
                let local = tz
                    .find_local_time_type(unix_time)
                    .map_err(|err| DateTimeParseError::Timezone(err.into()))?;
                UtcOffset::from_whole_seconds(local.ut_offset())
                    .map_err(DateTimeParseError::from)?
            }
        });

        self.tz = tz;
        Ok(self)
    }

    // Allow Deref to avoid rote, but hide the timezone functions so they are
    // not called accidentally
    #[allow(dead_code, clippy::missing_docs_in_private_items, clippy::unused_self)]
    fn checked_to_offset(&self) {}
    #[allow(dead_code, clippy::missing_docs_in_private_items, clippy::unused_self)]
    fn replace_offset(&self) {}
    #[allow(dead_code, clippy::missing_docs_in_private_items, clippy::unused_self)]
    fn to_offset(&self) {}
}

impl From<time::UtcDateTime> for DateTime {
    fn from(time: time::UtcDateTime) -> Self {
        Self {
            inner: time.to_offset(UtcOffset::UTC),
            tz: DateTimeZone::UTC,
        }
    }
}

impl Eq for DateTime {}

impl Ord for DateTime {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl PartialEq for DateTime {
    // It should be good enough to say two `OffsetDateTime` match since it will
    // compensate for the offset.
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl PartialOrd for DateTime {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::ops::Deref for DateTime {
    type Target = OffsetDateTime;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Parses a string as a number similar to [`floatval`](https://php.net/floatval)
/// but returning an error if there is no number instead of returning 0.0.
pub fn floatval(n: &str) -> Result<(f64, &str), core::num::ParseFloatError> {
    let end = n
        .as_bytes()
        .iter()
        .position(|b| !b.is_ascii_digit() && !matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
        .unwrap_or(n.len());
    n[..end].parse().map(|value| (value, &n[end..]))
}

/// Performs a fuzzy comparison of two string values
/// [like PHP](https://www.php.net/manual/en/language.types.numeric-strings.php).
#[allow(clippy::float_cmp)]
pub fn fuzzy_cmp(lhs: &str, rhs: &str) -> bool {
    let lhs = lhs.trim_ascii();
    let rhs = rhs.trim_ascii();
    if let (Ok(lhs), Ok(rhs)) = (lhs.parse::<i64>(), rhs.parse::<i64>()) {
        lhs == rhs
    } else if let (Ok(lhs), Ok(rhs)) = (lhs.parse::<f64>(), rhs.parse::<f64>()) {
        lhs == rhs
    } else {
        lhs == rhs
    }
}

/// Finds and replaces substrings in the input like [`strtr`](https://php.net/strtr).
/// To avoid extra temporary allocation, `replacements` should be ordered from
/// longest to shortest match.
pub fn strtr<'a>(input: &'a str, replacements: &[(&str, &str)]) -> Cow<'a, str> {
    let replacements = if replacements.is_sorted_by(|(a, _), (b, _)| a.len() >= b.len()) {
        Cow::Borrowed(replacements)
    } else {
        let mut replacements = Vec::from(replacements);
        replacements.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()));
        Cow::Owned(replacements)
    };

    let mut iter = input.char_indices();
    let mut out = String::new();
    let mut flushed = 0;
    'next: while iter.offset() != input.len() {
        for (find, replace) in replacements.iter() {
            if iter.as_str().starts_with(find) {
                out += &input[flushed..iter.offset()];
                out += *replace;
                flushed = iter.offset() + find.len();
                for _ in 0..find.len() {
                    iter.next();
                }
                continue 'next;
            }
        }
        iter.next();
    }

    if flushed == 0 {
        Cow::Borrowed(input)
    } else {
        out += &input[flushed..];
        Cow::Owned(out)
    }
}

/// Casts a float to a string similar to [`strval`](https://www.php.net/strval).
pub fn strval(n: f64) -> String {
    match n {
        f64::INFINITY => return "INF".into(),
        f64::NEG_INFINITY => return "-INF".into(),
        n if n.is_nan() => return "NAN".into(),
        _ => {}
    }

    // In PHP, this is configurable by the `precision` ini value; MW does not
    // appear to really think about it
    let len = 14_usize;

    // Clippy: Truncation and sign loss is deliberate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = n.abs() as u64;
    let (len, exp) = if whole == 0 {
        (Some(len), 0)
    } else {
        let exp = whole.ilog10() as usize;
        (14_usize.checked_sub(exp + 1), exp)
    };
    if let Some(len) = len {
        let mut s = format!("{n:.len$}");
        let b = s.as_bytes();
        let end = b
            .iter()
            .rposition(|c| *c != b'0')
            .map_or(b.len(), |e| e + usize::from(b[e] != b'.'));
        s.truncate(end);
        s
    } else {
        // Clippy: The number is always positive and in range.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        {
            format!("{:.13}E+{exp}", n / 10.0_f64.powi(exp as i32))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_cmp() {
        assert!(fuzzy_cmp("0", "0.0"));
        assert!(fuzzy_cmp("0", "0.0"));
        assert!(fuzzy_cmp("  +0 ", " -0. "));
        assert!(fuzzy_cmp("00", "0"));
        assert!(fuzzy_cmp("01", "1"));
        assert!(fuzzy_cmp("1", "1.0"));
        assert!(fuzzy_cmp("-1", "-1.0"));
        assert!(fuzzy_cmp("1e2", "100"));
        assert!(fuzzy_cmp("1e+2", "100"));
        assert!(fuzzy_cmp("4503599627370496.0", "4503599627370496.5"));
        assert!(fuzzy_cmp("4611686018427387904.0", "4611686018427387905"));
        assert!(!fuzzy_cmp("4611686018427387904", "4611686018427387905"));
        assert!(!fuzzy_cmp("0", "false"));
        assert!(!fuzzy_cmp("1", "true"));
        assert!(!fuzzy_cmp("0", "1"));
        assert!(!fuzzy_cmp("0.0", "1.0"));
    }

    #[test]
    fn test_floatval() {
        assert_eq!(floatval("122.34343The"), Ok((122.34343, "The")));
        assert_eq!(floatval("1,200"), Ok((1.0, ",200")));
    }

    #[test]
    fn test_strtr() {
        let input = "hello, world!";

        // longest first
        assert_eq!(
            strtr(input, &[("ll", "lol"), ("hello", "goodbye")]),
            Cow::<str>::Owned(String::from("goodbye, world!"))
        );

        // do not match already matched
        assert_eq!(
            strtr(input, &[("hello", "world"), ("world", "universe")]),
            Cow::<str>::Owned(String::from("world, universe!"))
        );

        // return original if no match
        assert_eq!(
            strtr(input, &[("foo", "bar")]),
            Cow::Borrowed("hello, world!")
        );
    }

    #[test]
    fn test_strval() {
        assert_eq!(strval(f64::INFINITY), "INF");
        assert_eq!(strval(f64::NEG_INFINITY), "-INF");
        assert_eq!(strval(f64::NAN), "NAN");
        assert_eq!(strval(0.0), "0");
        assert_eq!(strval(0.1 + 0.2), "0.3");
        assert_eq!(strval(1.123_456_789_012_34), "1.1234567890123");
        assert_eq!(strval(1.123_456_789_012_345), "1.1234567890123");
        assert_eq!(strval(0.123_456_789_012_34), "0.12345678901234");
        assert_eq!(strval(0.123_456_789_012_345), "0.12345678901234");
        assert_eq!(strval(12_345_678_901_234.0), "12345678901234");
        assert_eq!(strval(123_456_789_012_340.0), "1.2345678901234E+14");
        // TODO: Fix this if it ever matters
        // assert_eq!(strval(123_456_789_012_345.0), "1.2345678901234E+14");
        assert_eq!(strval(123_456_789_012_346.0), "1.2345678901235E+14");
    }
}
