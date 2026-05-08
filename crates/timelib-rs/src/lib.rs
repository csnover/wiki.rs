//! PHP `strtotime` compatible time parsing library.

use time::{Month, OffsetDateTime, Weekday};
pub use timezone::Timezone;

mod parse_date;
#[cfg(test)]
mod tests;
mod timezone;
mod to_unixtime;

/// A parsed date and time.
#[derive(Clone, Debug)]
pub struct DateTime<'a> {
    /// The parsed date.
    pub date: OffsetDateTime,
    /// The parsed time zone.
    pub tz: Timezone<'a>,
}

impl<'a> DateTime<'a> {
    /// Parses a date string into a [`DateTime`].
    ///
    /// If no time zone is given in the date string, `default_tz` will be used
    /// to fill it in. If `default_tz` is `None`, the machine’s local time zone
    /// will be used.
    ///
    /// If date and time components are missing from the date string, `now` will
    /// be used to fill them in. If `now` is `None`, the machine’s wall time
    /// will be used.
    ///
    /// # Errors
    ///
    /// * `text` is not a valid time string
    /// * `default_tz` is `Some` and cannot be converted to an offset
    /// * `now` is `None` and the local time cannot be determined
    /// * `now` is `Some` and out of range of the date type
    /// * The parsed date is out of range of [`time::Date`]
    pub fn new(
        text: &'a str,
        default_tz: Option<Timezone<'a>>,
        now: Option<i64>,
    ) -> Result<Self, Error> {
        let parse_date::ParseResult {
            builder: state,
            errors,
        } = parse_date::parse(text);

        if let Some(error) = errors.into_iter().next() {
            return Err(Error::Parse(error));
        }

        let now = if let Some(now) = now {
            OffsetDateTime::from_unix_timestamp(now)?
        } else {
            OffsetDateTime::now_local()?
        };

        let tz = state
            .offset
            .clone()
            .or(default_tz)
            .unwrap_or_else(|| Timezone::Offset(now.offset().whole_seconds()));

        let other = DateTimeBuilder {
            date: (now.year(), now.month(), now.day()).into(),
            time: (
                Hour24(now.hour()),
                now.minute(),
                now.second(),
                now.microsecond(),
            )
                .into(),
            offset: Some(tz.clone()),
            ..Default::default()
        };

        state.build(Some(other))
    }

    /// Calculates a Unix timestamp from numeric date parts. If the parts are
    /// out-of-range, they will overflow into the next date component.
    ///
    /// Any `None` parts will be filled in with midnight on the first day of the
    /// year at UTC.
    ///
    /// # Errors
    ///
    /// * `offset` cannot be converted to an offset
    /// * The calculated date is out of range of [`time::Date`]
    #[expect(
        clippy::too_many_arguments,
        reason = "adding an args struct would mostly just be busywork"
    )]
    pub fn from_parts(
        year: i64,
        month: Option<i64>,
        day: Option<i64>,
        hour: Option<i64>,
        minute: Option<i64>,
        second: Option<i64>,
        micros: Option<i64>,
        offset: Option<Timezone<'a>>,
    ) -> Result<Self, Error> {
        DateTimeBuilder {
            date: TimelibDate {
                year: Some(year),
                month: month.or(Some(1)),
                day: day.or(Some(1)),
            },
            time: TimelibTime {
                hour: hour.or(Some(0)),
                minute: minute.or(Some(0)),
                second: second.or(Some(0)),
                micros: micros.or(Some(0)),
            },
            offset,
            ..Default::default()
        }
        .build(None)
    }
}

/// A time builder error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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

/// An date specifier without range constraints. During processing, out-of-range
/// values will overflow or underflow into the previous/next component.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TimelibDate {
    /// The year.
    year: Option<i64>,
    /// The month, 0-indexed.
    month: Option<i64>,
    /// The day, 0-indexed.
    day: Option<i64>,
}

/// An time specifier without range constraints. During processing, out-of-range
/// values will overflow or underflow into the previous/next component.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TimelibTime {
    /// The hour.
    hour: Option<i64>,
    /// The minute.
    minute: Option<i64>,
    /// The second.
    second: Option<i64>,
    /// The microseconds.
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
