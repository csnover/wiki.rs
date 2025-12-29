//! PHP `strtotime` compatible time parsing library.

use time::{Month, Weekday};
use timezone::Timezone;

mod parse_date;
#[cfg(test)]
mod tests;
mod timezone;
mod to_unixtime;

/// A time builder error.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Invalid input string.
    #[error("invalid input: {0}")]
    Parse(#[from] parse_date::PegError),

    /// The computer may be experiencing a space-time paradox, because it is
    /// claiming to be in a local time zone with an offset that is, as far as
    /// our best scientists are aware, is impossible to experience on Earth.
    #[error(transparent)]
    WhenEvenIsHere(#[from] time::error::IndeterminateOffset),

    /// A time component was out of range of a data unit.
    /// (This should be a [`time::error::ComponentRange`] error, but the `time`
    /// crate does not currently allow consumers to build their own, nor to even
    /// get access to all the fields to upconvert them to a single wrapper
    /// type.)
    #[error("integer conversion error: {0}")]
    DataRange(#[from] core::num::TryFromIntError),

    /// A time component was out of range of a time unit.
    #[error(transparent)]
    ComponentRange(#[from] time::error::ComponentRange),

    /// There were so many weekdays between then and now that they could not fit
    /// in a [`time::Duration`].
    #[error("weekdays out of range")]
    WeekdaysRange,

    /// An invalid time zone specifier was used.
    #[error("invalid time zone: {0}")]
    Timezone(#[from] tz::Error),

    /// [`DateTimeBuilder::build`] was called without ensuring all the fields
    /// were filled.
    #[error("incomplete time data")]
    MissingData,
}

/// Creates a new [`DateTime`](super::DateTime) from a PHP format date string.
// TODO: There is an unpleasant smearing of API boundaries here. The intent was
// to avoid leaking anything from timelib out, but the real boundary should be
// at DateTime, and anything it needs to do what it needs should be exposed from
// timelib (which should just be DateTimeBuilder and Timezone).
pub(super) fn new_datetime(
    text: &str,
    default_tz: Option<&super::DateTimeZone>,
    now: Option<&super::DateTime>,
) -> Result<super::DateTime, Error> {
    let state = match parse_date::parse(text) {
        parse_date::ParseResult {
            builder: state,
            errors,
        } if errors.is_empty() => state,
        parse_date::ParseResult { errors, .. } => {
            return Err(Error::Parse(errors.into_iter().next().unwrap()));
        }
    };

    let now = if let Some(now) = now {
        time::OffsetDateTime::from_unix_timestamp(now.unix_timestamp())?
    } else {
        time::OffsetDateTime::now_local()?
    };

    let offset = Some(if let Some(default_tz) = default_tz {
        match default_tz {
            super::DateTimeZone::Offset(offset) => Timezone::Offset(offset.whole_seconds()),
            super::DateTimeZone::Alias(alias) => {
                Timezone::Alias(alias.time_zone_designation().into())
            }
            super::DateTimeZone::Named(name, _) => Timezone::Named(name),
        }
    } else {
        Timezone::Offset(now.offset().whole_seconds())
    });

    let other = DateTimeBuilder {
        date: (now.year(), now.month(), now.day()).into(),
        time: (
            Hour24(now.hour()),
            now.minute(),
            now.second(),
            now.microsecond(),
        )
            .into(),
        offset,
        ..Default::default()
    };

    state.build(Some(other))
}

/// A time builder.
#[derive(Clone, Debug, Default)]
struct DateTimeBuilder<'a> {
    /// An absolute date.
    date: TimelibDate,
    /// An absolute time.
    time: TimelibTime,
    /// A time zone.
    offset: Option<Timezone<'a>>,
    /// Relative adjustments to the absolute time.
    relative: Relatime,
    /// A parser-specific state flag to avoid double-parsing of dates.
    have_date: bool,
    /// A parser-specific state flag to avoid double-parsing of times.
    have_time: bool,
    /// A parser-specific state flag to avoid double-parsing of time zones.
    have_zone: bool,
}

/// A 24-hour time specifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Hour24(u8);

impl From<Hour24> for i64 {
    fn from(value: Hour24) -> Self {
        value.0.into()
    }
}

/// Specifier for a “day of” expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Keyword {
    /// First day of.
    FirstDay,
    /// Last day of.
    LastDay,
}

/// Specifier for a relative time.
#[derive(Clone, Copy, Debug, Default)]
struct Relatime {
    /// Difference in years.
    y: i64,
    /// Difference in months.
    m: i64,
    /// Difference in days.
    d: i64,
    /// Difference in hours.
    h: i64,
    /// Difference in minutes.
    i: i64,
    /// Difference in seconds.
    s: i64,
    /// Difference in microseconds.
    us: i64,
    /// If specified, relative to the given weekday.
    weekday: Option<Weekdays>,
    /// The weekday behaviour, if a weekday is specified.
    weekday_behavior: WeekdayBehavior,
    /// If specified, relative to the first or last day of another unit of time.
    first_last_day_of: Option<Keyword>,
    /// If specified, relative to a month.
    special: Option<Special>,
}

/// Specifier for a date relative to a month.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Special {
    /// The first day of the week.
    NthDayOfWeekInMonth,
    /// The last day of the week.
    LastDayOfWeekInMonth,
    /// A number of weekdays.
    WeekdayCount(i64),
}

/// An unconstrained date specifier.
// Clippy: Come on, now.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TimelibDate {
    year: Option<i64>,
    month: Option<i64>,
    day: Option<i64>,
}

/// An unconstrained time specifier.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(clippy::missing_docs_in_private_items)]
struct TimelibTime {
    hour: Option<i64>,
    minute: Option<i64>,
    second: Option<i64>,
    micros: Option<i64>,
}

/// Specifier for how to resolve a weekday.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum WeekdayBehavior {
    /// Ignore the day of the absolute time part even if it matches.
    #[default]
    IgnoreCurrentDay = 0,
    /// Include the day of the absolute time part if it matches.
    CountCurrentDay = 1,
    /// Resolve the day relative to this/next/last week.
    RelativeTextWeek = 2,
}

/// Specifier for a date on a weekday.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Weekdays {
    /// The next given weekday on or after the absolute time part.
    Weekday(Weekday),
    /// The last given weekday before the absolute time part.
    Ago(Weekday),
    /// Any weekday on or after the absolute time part.
    All,
}
