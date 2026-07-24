//! PHP compatible functions and types.

use core::fmt::Write as _;
use std::{borrow::Cow, io::Write as _};
pub use time::{Date, Duration, Month, Time, UtcOffset, Weekday};
use time::{
    OffsetDateTime,
    format_description::well_known::{Rfc2822, iso8601},
};
pub use timelib_rs::Error as DateTimeParseError;
use timelib_rs::Timezone;
pub use tz::{LocalTimeType, TimeZoneRef};

/// Any time error.
#[derive(Debug, thiserror::Error)]
pub enum DateTimeError {
    /// An error occurred when formatting.
    #[error(transparent)]
    Format(#[from] DateTimeFormatError),
    /// An error occurred when parsing.
    #[error(transparent)]
    Parse(#[from] DateTimeParseError),
}

/// Time formatting error.
#[derive(Debug, thiserror::Error)]
pub enum DateTimeFormatError {
    /// An error occurred when formatting.
    #[error(transparent)]
    Format(#[from] time::error::Format),
    /// An error occurred when trying to write to a byte buffer.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// An error occurred when trying to write to a string.
    #[error(transparent)]
    Write(#[from] core::fmt::Error),
}

/// A provider for localising date components.
pub trait DateTimeLocalizer: Clone + Copy {
    /// The output display type for [`Self::month_abbr`].
    type AbbrMonthOutput: core::fmt::Display;
    /// The output display type for [`Self::weekday_abbr`].
    type AbbrWeekdayOutput: core::fmt::Display;
    /// The output display type for [`Self::month_full`].
    type FullMonthOutput: core::fmt::Display;
    /// The output display type for [`Self::weekday_full`].
    type FullWeekdayOutput: core::fmt::Display;

    /// Returns the abbreviated localised name for the given `month`.
    fn month_abbr(&self, month: Month) -> Self::AbbrMonthOutput;
    /// Returns the localised name for the given `month`.
    fn month_full(&self, month: Month) -> Self::FullMonthOutput;
    /// Returns the abbreviated localised name for the given `month`.
    fn weekday_abbr(&self, day: Weekday) -> Self::AbbrWeekdayOutput;
    /// Returns the localised name for the given `weekday`.
    fn weekday_full(&self, day: Weekday) -> Self::FullWeekdayOutput;
}

/// A default date component localiser.
#[derive(Clone, Copy, Debug)]
pub struct DefaultDateTimeLocalizer;
impl DateTimeLocalizer for DefaultDateTimeLocalizer {
    type AbbrMonthOutput = String;
    type AbbrWeekdayOutput = String;
    type FullMonthOutput = Month;
    type FullWeekdayOutput = Weekday;

    fn month_abbr(&self, month: Month) -> Self::AbbrMonthOutput {
        format!("{month:.3}")
    }

    fn month_full(&self, month: Month) -> Self::FullMonthOutput {
        month
    }

    fn weekday_abbr(&self, day: Weekday) -> Self::AbbrWeekdayOutput {
        format!("{day:.3}")
    }

    fn weekday_full(&self, day: Weekday) -> Self::FullWeekdayOutput {
        day
    }
}

/// A time zone.
#[derive(Clone, Copy, Debug)]
pub enum DateTimeZone {
    /// Local time zone.
    Alias(LocalTimeType),
    /// IANA time zone.
    Named(&'static str, TimeZoneRef<'static>),
    /// Offset from UTC.
    Offset(UtcOffset),
}

impl<'a> From<&'a DateTimeZone> for Timezone<'a> {
    fn from(value: &'a DateTimeZone) -> Self {
        match value {
            DateTimeZone::Offset(offset) => Timezone::Offset(offset.whole_seconds()),
            DateTimeZone::Alias(alias) => Timezone::Alias(alias.time_zone_designation().into()),
            DateTimeZone::Named(name, _) => Timezone::Named(name),
        }
    }
}

impl TryFrom<Timezone<'_>> for DateTimeZone {
    type Error = DateTimeParseError;

    fn try_from(value: Timezone<'_>) -> Result<Self, Self::Error> {
        Ok(match &value {
            Timezone::Offset(seconds) => {
                let offset = UtcOffset::from_whole_seconds(*seconds)?;
                DateTimeZone::Offset(offset)
            }
            tz @ Timezone::Alias(alias) => {
                let offset = tz.offset() + if tz.is_dst() { 3600 } else { 0 };
                let local = LocalTimeType::new(offset, tz.is_dst(), Some(alias.as_bytes()))
                    .map_err(tz::Error::from)?;
                DateTimeZone::Alias(local)
            }
            Timezone::Named(name) => {
                let zone = tzdb_data::find_tz(name.as_bytes())
                    .ok_or(tz::Error::Tz(tz::TzError::NoAvailableLocalTimeType))?;
                DateTimeZone::Named(name, *zone)
            }
        })
    }
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
    ///
    /// # Errors
    ///
    /// * The local system time zone cannot be determined
    pub fn local() -> Result<Self, DateTimeError> {
        Ok(Self::Offset(
            UtcOffset::current_local_offset().map_err(DateTimeParseError::from)?,
        ))
    }
}

/// A time with associated time zone.
#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    /// The time.
    inner: OffsetDateTime,
    /// The time zone.
    tz: DateTimeZone,
}

impl DateTime {
    /// Midnight, 1 January, 1970 (UTC).
    pub const UNIX_EPOCH: Self = Self {
        inner: OffsetDateTime::UNIX_EPOCH,
        tz: DateTimeZone::UTC,
    };

    /// Creates a new [`DateTime`] from numeric date parts.
    ///
    /// The `month` and `day` parts are 1-indexed.
    ///
    /// Parts with values that are outside the range of the given time part will
    /// overflow into the next largest time part.
    ///
    /// # Errors
    ///
    /// * `offset` cannot be converted to an offset
    /// * The calculated date is out of range of [`time::Date`]
    #[expect(
        clippy::too_many_arguments,
        reason = "adding an args struct would mostly just be busywork"
    )]
    #[inline]
    pub fn from_parts(
        year: i64,
        month: Option<i64>,
        day: Option<i64>,
        hour: Option<i64>,
        minute: Option<i64>,
        second: Option<i64>,
        micros: Option<i64>,
        offset: Option<&DateTimeZone>,
    ) -> Result<Self, DateTimeError> {
        timelib_rs::DateTime::from_parts(
            year,
            month,
            day,
            hour,
            minute,
            second,
            micros,
            offset.map(Timezone::from),
        )
        .and_then(|dt| {
            Ok(Self {
                inner: dt.date,
                tz: dt.tz.try_into()?,
            })
        })
        .map_err(Into::into)
    }

    /// Creates a new `DateTime` object from a Unix timestamp.
    ///
    /// # Errors
    ///
    /// * `timestamp` is out of range of [`time::Date`]
    pub fn from_unix_timestamp(timestamp: i64) -> Result<Self, DateTimeError> {
        Ok(Self {
            inner: OffsetDateTime::from_unix_timestamp(timestamp)
                .map_err(DateTimeParseError::from)?,
            tz: DateTimeZone::UTC,
        })
    }

    /// Creates a new `DateTime` object from a
    /// [PHP date format string](https://www.php.net/manual/en/datetime.formats.php).
    ///
    /// # Errors
    ///
    /// * `text` is not a valid time string
    /// * `default_tz` is `Some` and cannot be converted to an offset
    /// * `now` is `None` and the local time cannot be determined
    /// * `now` is `Some` and out of range of the date type
    /// * The parsed date is out of range of [`time::Date`]
    pub fn new(
        text: &str,
        default_tz: Option<&DateTimeZone>,
        now: Option<&DateTime>,
    ) -> Result<Self, DateTimeError> {
        timelib_rs::DateTime::new(
            text,
            default_tz.map(Into::into),
            now.map(|now| now.unix_timestamp()),
        )
        .and_then(|dt| {
            Ok(Self {
                inner: dt.date,
                tz: dt.tz.try_into()?,
            })
        })
        .map_err(Into::into)
    }

    /// Creates a new `DateTime` object for the current time, in local time.
    ///
    /// # Errors
    ///
    /// * The system time cannot be determined
    pub fn now() -> Result<Self, DateTimeError> {
        let inner = OffsetDateTime::now_local().map_err(DateTimeParseError::from)?;
        let tz = DateTimeZone::Offset(inner.offset());
        Ok(Self { inner, tz })
    }

    /// Computes `self + duration`, saturating value on overflow.
    #[inline]
    #[must_use]
    pub fn saturating_add(self, duration: Duration) -> Self {
        Self {
            inner: self.inner.saturating_add(duration),
            tz: self.tz,
        }
    }

    /// Computes `self - duration`, saturating value on overflow.
    #[inline]
    #[must_use]
    pub fn saturating_sub(self, duration: Duration) -> Self {
        Self {
            inner: self.inner.saturating_sub(duration),
            tz: self.tz,
        }
    }

    /// Gets the [`Date`].
    #[inline]
    #[must_use]
    pub fn date(self) -> Date {
        self.inner.date()
    }

    /// Gets the day (`1..=31`) of the date.
    #[inline]
    #[must_use]
    pub fn day(self) -> u8 {
        self.inner.day()
    }

    /// Formats a time according to the
    /// [MediaWiki extended time format](https://www.mediawiki.org/wiki/Special:MyLanguage/Help:Extension:ParserFunctions#time).
    ///
    /// # Errors
    ///
    /// * A write to the output buffer fails
    pub fn format(
        &self,
        format: &str,
        localizer: impl DateTimeLocalizer,
    ) -> Result<String, DateTimeFormatError> {
        fn write_offset_hm(out: &mut String, offset: UtcOffset, sep: &str) -> core::fmt::Result {
            write!(
                out,
                "{:+03}{sep}{:02}",
                offset.whole_hours(),
                offset.minutes_past_hour().abs()
            )
        }

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
                'D' => write!(out, "{}", localizer.weekday_abbr(d.weekday()))?,
                'j' => write!(out, "{}", d.day())?,
                'l' => write!(out, "{}", localizer.weekday_full(d.weekday()))?,
                'F' => write!(out, "{}", localizer.month_full(d.month()))?,
                'm' => write!(out, "{:02}", u8::from(d.month()))?,
                'M' => write!(out, "{}", localizer.month_abbr(d.month()))?,
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
                    const DATE_TIME_NO_NANOS: u128 = iso8601::Config::DEFAULT
                        .set_time_precision(iso8601::TimePrecision::Second {
                            decimal_digits: None,
                        })
                        .set_formatted_components(iso8601::FormattedComponents::DateTime)
                        .encode();
                    out += &d.format(&iso8601::Iso8601::<DATE_TIME_NO_NANOS>)?;
                    write_offset_hm(&mut out, d.offset(), ":")?;
                }
                'r' => out += &d.format(&Rfc2822)?,
                'e' => out += &self.time_zone_designation(),
                'O' => write_offset_hm(&mut out, d.offset(), "")?,
                'P' => write_offset_hm(&mut out, d.offset(), ":")?,
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

    /// Gets the hour (`0..=23`) of the date.
    #[inline]
    #[must_use]
    pub fn hour(self) -> u8 {
        self.inner.hour()
    }

    /// Projects this time into a different time zone. (In other words, the same
    /// time instant as seen from another time zone.)
    ///
    /// # Errors
    ///
    /// * The time zone offset cannot be determined
    pub fn into_offset(mut self, tz: DateTimeZone) -> Result<Self, DateTimeError> {
        self.inner = self.inner.to_offset(self.tz_to_offset(tz)?);
        self.tz = tz;
        Ok(self)
    }

    /// Returns true if the currently represented time is in daylight saving
    /// time.
    ///
    /// # Panics
    ///
    /// * The local time type for a named time zone cannot be found
    #[must_use]
    pub fn is_dst(&self) -> bool {
        match self.tz {
            DateTimeZone::Offset(_) => false,
            DateTimeZone::Alias(alias) => alias.is_dst(),
            DateTimeZone::Named(_, time_zone_ref) => time_zone_ref
                .find_local_time_type(self.unix_timestamp())
                .expect("local time type")
                .is_dst(),
        }
    }

    /// Gets the ISO week (`1..=53`) of the date.
    #[inline]
    #[must_use]
    pub fn iso_week(self) -> u8 {
        self.inner.iso_week()
    }

    /// Gets the millisecond (`0..=999`) of the date.
    #[inline]
    #[must_use]
    pub fn millisecond(self) -> u16 {
        self.inner.millisecond()
    }

    /// Gets the minute (`0..=59`) of the date.
    #[inline]
    #[must_use]
    pub fn minute(self) -> u8 {
        self.inner.minute()
    }

    /// Gets the week number (`0..=53`) where week 1 begins on the first Monday.
    #[inline]
    #[must_use]
    pub fn monday_based_week(self) -> u8 {
        self.inner.monday_based_week()
    }

    /// Gets the month of the date.
    #[inline]
    #[must_use]
    pub fn month(self) -> Month {
        self.inner.month()
    }

    /// Gets the offset of the date from UTC.
    #[inline]
    #[must_use]
    pub fn offset(self) -> UtcOffset {
        self.inner.offset()
    }

    /// Gets the day (`1..=366`) of the year.
    #[inline]
    #[must_use]
    pub fn ordinal(self) -> u16 {
        self.inner.ordinal()
    }

    /// Replace the date, which is assumed to be in the stored offset. The time
    /// and offset components are unchanged.
    #[inline]
    #[must_use]
    pub fn replace_date(self, date: Date) -> Self {
        Self {
            inner: self.inner.replace_date(date),
            tz: self.tz,
        }
    }

    /// Replace the day of the month.
    ///
    /// # Errors
    ///
    /// * `day` is out of range of `self.month()` (e.g. Feb 28 → Feb 31)
    pub fn replace_day(self, day: u8) -> Result<Self, DateTimeError> {
        self.inner
            .replace_day(day)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Replace the hour of the day.
    ///
    /// # Errors
    ///
    /// * `hour` is out of range
    pub fn replace_hour(self, hour: u8) -> Result<Self, DateTimeError> {
        self.inner
            .replace_hour(hour)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Replace the millisecond of the second.
    ///
    /// # Errors
    ///
    /// * `millisecond` is out of range
    pub fn replace_millisecond(self, millisecond: u16) -> Result<Self, DateTimeError> {
        self.inner
            .replace_millisecond(millisecond)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Replace the minute of the hour.
    ///
    /// # Errors
    ///
    /// * `minute` is out of range
    pub fn replace_minute(self, minute: u8) -> Result<Self, DateTimeError> {
        self.inner
            .replace_minute(minute)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Replace the month of the year.
    ///
    /// # Errors
    ///
    /// * `self.day()` is out of range of `month` (e.g. Jan 31 → Feb 31)
    pub fn replace_month(self, month: Month) -> Result<Self, DateTimeError> {
        self.inner
            .replace_month(month)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Replace the ordinal day of the year.
    ///
    /// # Errors
    ///
    /// * `ordinal` is out of range (e.g. 367)
    pub fn replace_ordinal(self, ordinal: u16) -> Result<Self, DateTimeError> {
        self.inner
            .replace_ordinal(ordinal)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Replace the second of the minute.
    ///
    /// # Errors
    ///
    /// * `second` is out of range
    pub fn replace_second(self, second: u8) -> Result<Self, DateTimeError> {
        self.inner
            .replace_second(second)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Replace the time, which is assumed to be in the stored offset. The date
    /// and offset components are unchanged.
    #[inline]
    #[must_use]
    pub fn replace_time(self, time: Time) -> Self {
        Self {
            inner: self.inner.replace_time(time),
            tz: self.tz,
        }
    }

    /// Replace the month of the year.
    ///
    /// # Errors
    ///
    /// * `self.day()` in `self.month()` is out of range in `year`
    ///   (e.g. Feb 29 2000 → Feb 29 2001)
    pub fn replace_year(self, year: i32) -> Result<Self, DateTimeError> {
        self.inner
            .replace_year(year)
            .map(|inner| Self { inner, tz: self.tz })
            .map_err(|err| DateTimeError::Parse(err.into()))
    }

    /// Computes `self + duration`, saturating value on overflow.
    #[inline]
    #[must_use]
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.inner
            .checked_add(duration)
            .map(|inner| Self { inner, tz: self.tz })
    }

    /// Computes `self - duration`, saturating value on overflow.
    #[inline]
    #[must_use]
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        self.inner
            .checked_sub(duration)
            .map(|inner| Self { inner, tz: self.tz })
    }

    /// Gets the second (`0..=59`) of the date.
    #[inline]
    #[must_use]
    pub fn second(self) -> u8 {
        self.inner.second()
    }

    /// Gets the week number (`0..=53`) where week 1 begins on the first Sunday.
    #[inline]
    #[must_use]
    pub fn sunday_based_week(self) -> u8 {
        self.inner.sunday_based_week()
    }

    /// Gets the [`Time`].
    #[inline]
    #[must_use]
    pub fn time(self) -> Time {
        self.inner.time()
    }

    /// Gets the time zone.
    #[inline]
    #[must_use]
    pub fn time_zone(&self) -> &DateTimeZone {
        &self.tz
    }

    /// Gets the string representation of the current time zone.
    #[must_use]
    pub fn time_zone_designation(&self) -> Cow<'_, str> {
        match &self.tz {
            DateTimeZone::Offset(offset) => Cow::Owned(offset.to_string()),
            DateTimeZone::Alias(alias) => Cow::Borrowed(alias.time_zone_designation()),
            DateTimeZone::Named(name, _) => Cow::Borrowed(name),
        }
    }

    /// Gets the year, month, and day.
    #[inline]
    #[must_use]
    pub fn to_calendar_date(self) -> (i32, Month, u8) {
        self.inner.to_calendar_date()
    }

    /// Gets the Julian day for the date, ignoring the time part.
    #[inline]
    #[must_use]
    pub fn to_julian_day(self) -> i32 {
        self.inner.to_julian_day()
    }

    /// Gets the ISO 8601 year, week number (`1..=53`), and weekday.
    #[inline]
    #[must_use]
    pub fn to_iso_week_date(self) -> (i32, u8, Weekday) {
        self.inner.to_iso_week_date()
    }

    /// Gets the primitive offset date time.
    #[inline]
    #[must_use]
    pub fn to_offset_time(self) -> OffsetDateTime {
        self.inner
    }

    /// Truncate to the start of the day, setting the time to midnight.
    #[inline]
    #[must_use]
    pub fn truncate_to_day(self) -> Self {
        Self {
            inner: self.inner.truncate_to_day(),
            tz: self.tz,
        }
    }

    /// Truncate to the hour, setting the minute, second, and subsecond
    /// components to zero.
    #[inline]
    #[must_use]
    pub fn truncate_to_hour(self) -> Self {
        Self {
            inner: self.inner.truncate_to_hour(),
            tz: self.tz,
        }
    }

    /// Truncate to the millisecond, setting the microsecond and nanosecond
    /// components to zero.
    #[inline]
    #[must_use]
    pub fn truncate_to_millisecond(self) -> Self {
        Self {
            inner: self.inner.truncate_to_millisecond(),
            tz: self.tz,
        }
    }

    /// Truncate to the minute, setting the second and subsecond components to
    /// zero.
    #[inline]
    #[must_use]
    pub fn truncate_to_minute(self) -> Self {
        Self {
            inner: self.inner.truncate_to_minute(),
            tz: self.tz,
        }
    }

    /// Truncate to the second, setting the subsecond components to zero.
    #[inline]
    #[must_use]
    pub fn truncate_to_second(self) -> Self {
        Self {
            inner: self.inner.truncate_to_second(),
            tz: self.tz,
        }
    }

    /// Converts a timezone to an offset for this time.
    fn tz_to_offset(self, tz: DateTimeZone) -> Result<UtcOffset, DateTimeError> {
        Ok(match tz {
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
        })
    }

    /// Gets the Unix timestamp.
    #[inline]
    #[must_use]
    pub fn unix_timestamp(self) -> i64 {
        self.inner.unix_timestamp()
    }

    /// Gets the Unix timestamp in nanoseconds.
    #[inline]
    #[must_use]
    pub fn unix_timestamp_nanos(self) -> i128 {
        self.inner.unix_timestamp_nanos()
    }

    /// Gets the weekday of the date.
    #[inline]
    #[must_use]
    pub fn weekday(self) -> Weekday {
        self.inner.weekday()
    }

    /// Gets the year of the date.
    #[inline]
    #[must_use]
    pub fn year(self) -> i32 {
        self.inner.year()
    }
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

impl core::ops::Sub for DateTime {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        self.inner - rhs.inner
    }
}

impl core::ops::Add<Duration> for DateTime {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self {
            inner: self.inner + rhs,
            tz: self.tz,
        }
    }
}

impl core::ops::Sub<Duration> for DateTime {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self {
            inner: self.inner - rhs,
            tz: self.tz,
        }
    }
}

/// Parses a string as a number similar to [`floatval`](https://php.net/floatval)
/// but returning an error if there is no number instead of returning 0.0.
///
/// # Errors
///
/// * `n` does not start with a number
pub fn floatval(n: &str) -> Result<(f64, &str), core::num::ParseFloatError> {
    #[inline]
    fn is_ascii_digit_or_sign(b: Option<u8>) -> bool {
        b.is_some_and(|b| matches!(b, b'-' | b'+') || b.is_ascii_digit())
    }

    let mut seen_e = None;
    let mut seen_dec = false;
    let mut is_numeric = |pos: usize, b: u8, next: Option<u8>| {
        // TODO: Really this requires two-token look-ahead since it could be
        // e±<invalid> but this is not important enough to waste much time on
        if matches!(b, b'e' | b'E') && seen_e.is_none() && is_ascii_digit_or_sign(next) {
            seen_dec = true;
            seen_e = Some(pos);
            true
        } else if b == b'.' && !seen_dec {
            seen_dec = true;
            true
        } else if matches!(b, b'+' | b'-') {
            pos == 0 || seen_e.is_some_and(|e| pos == e + 1)
        } else {
            b.is_ascii_digit()
        }
    };

    let mut end = 0;
    let bytes = n.as_bytes();
    while end != bytes.len() {
        if is_numeric(end, bytes[end], bytes.get(end + 1).copied()) {
            end += 1;
        } else {
            break;
        }
    }

    n[..end].parse().map(|value| (value, &n[end..]))
}

/// Formats a date using a
/// [glibc](https://www.man7.org/linux/man-pages/man3/strftime.3.html)
/// `strftime` formatting string.
///
/// # Errors
///
/// * A write to the output buffer fails
pub fn format_date_strftime(
    time: DateTime,
    format: impl IntoIterator<Item = u8>,
    localizer: impl DateTimeLocalizer,
) -> Result<Vec<u8>, DateTimeFormatError> {
    let mut format = format.into_iter();
    let mut out = Vec::<u8>::new();
    while let Some(b) = format.next() {
        if b != b'%' {
            out.push(b);
            continue;
        }

        match format.next() {
            Some(b'a') => write!(out, "{:.3}", time.weekday()),
            Some(b'A') => write!(out, "{}", time.weekday()),
            Some(b'b' | b'h') => write!(out, "{:.3}", time.month()),
            Some(b'B') => write!(out, "{}", time.month()),
            Some(b'c') => write!(out, "{}", time.format("r", localizer)?),
            Some(b'C') => write!(out, "{}", time.year() / 100),
            Some(b'd') => write!(out, "{:02}", time.day()),
            Some(b'D') => write!(
                out,
                "{:02}/{:02}/{:02}",
                u8::from(time.month()),
                time.day(),
                time.year()
            ),
            Some(b'e') => write!(out, "{:>2}", time.day()),
            Some(b'F') => write!(
                out,
                "{:04}-{:02}-{:02}",
                time.year(),
                u8::from(time.month()),
                time.day()
            ),
            Some(b'G') => {
                let (year, week, _) = time.to_iso_week_date();
                write!(out, "{year:04}-{week:02}")
            }
            Some(b'g') => {
                let (year, week, _) = time.to_iso_week_date();
                write!(out, "{:02}-{week:02}", year % 100)
            }
            Some(b'H') => write!(out, "{:02}", time.hour()),
            Some(b'I') => {
                write!(out, "{:02}", {
                    let h = time.hour() % 12;
                    if h == 0 { 12 } else { h }
                })
            }
            Some(b'j') => write!(out, "{}", time.ordinal()),
            Some(b'k') => write!(out, "{:>2}", time.hour()),
            Some(b'l') => {
                write!(out, "{:>2}", {
                    let h = time.hour() % 12;
                    if h == 0 { 12 } else { h }
                })
            }
            Some(b'm') => write!(out, "{:02}", u8::from(time.month())),
            Some(b'M') => write!(out, "{:02}", time.minute()),
            Some(b'n') => writeln!(out),
            Some(b'p') => write!(out, "{}M", if time.hour() < 12 { 'A' } else { 'P' }),
            Some(b'P') => write!(out, "{}m", if time.hour() < 12 { 'a' } else { 'p' }),
            Some(b'r') => write!(out, "{}.m.", if time.hour() < 12 { 'a' } else { 'p' }),
            Some(b'R') => write!(out, "{:02}:{:02}", time.hour(), time.minute()),
            Some(b's') => write!(out, "{}", time.unix_timestamp()),
            Some(b'S') => write!(out, "{:02}", time.second()),
            Some(b't') => write!(out, "\t"),
            Some(b'T') => write!(
                out,
                "{:02}:{:02}:{:02}",
                time.hour(),
                time.minute(),
                time.second()
            ),
            Some(b'u') => write!(out, "{}", time.weekday().number_from_monday()),
            Some(b'U') => write!(out, "{:02}", time.sunday_based_week()),
            Some(b'V') => write!(out, "{:02}", time.iso_week()),
            Some(b'W') => write!(out, "{:02}", time.monday_based_week()),
            Some(b'x' | b'X') => todo!(),
            Some(b'y') => write!(out, "{:02}", time.year() % 100),
            Some(b'Y') => write!(out, "{}", time.year()),
            Some(b'z') => write!(
                out,
                "{:+02}{:02}",
                time.offset().whole_hours(),
                time.offset().minutes_past_hour().abs()
            ),
            Some(b'Z') => write!(out, "{}", time.time_zone_designation()),
            Some(b'%') | None => write!(out, "%"),
            Some(c) => write!(out, "%{c}"),
        }?;
    }
    Ok(out)
}

/// Performs a fuzzy comparison of two string values
/// [like PHP](https://www.php.net/manual/en/language.types.numeric-strings.php).
#[expect(clippy::float_cmp, reason = "matches upstream behaviour")]
#[must_use]
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

/// Parses a string as a number similar to [`intval`](https://php.net/intval)
/// but returning an error if there is no number instead of returning 0.
///
/// The default for a `None` `radix` is 10. To detect a base from a string
/// prefix, use `Some(0)`.
///
/// # Errors
///
/// * `n` does not start with a number
pub fn intval(n: &str, radix: Option<u32>) -> Result<(i64, &str), core::num::ParseIntError> {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

    #[inline]
    fn is_ascii_digit_or_pos(b: Option<u8>) -> bool {
        b.is_some_and(|b| b == b'+' || b.is_ascii_digit())
    }

    let (n, radix) = if radix.is_none() {
        (n, 10)
    } else if let Some(radix) = radix
        && radix != 0
    {
        (n, radix)
    } else if let Some(n) = n.strip_prefix('0') {
        if let Some(n) = n.strip_prefix(['x', 'X']) {
            (n, 16)
        } else if let Some(n) = n.strip_prefix(['b', 'B']) {
            (n, 2)
        } else {
            (n, 8)
        }
    } else {
        (n, 10)
    };

    let mut seen_e = None;
    let mut is_numeric = |pos: usize, b: u8, next: Option<u8>| {
        if radix == 10
            && matches!(b, b'e' | b'E')
            && seen_e.is_none()
            && is_ascii_digit_or_pos(next)
        {
            seen_e = Some(pos);
            true
        } else if matches!(b, b'-' | b'+') {
            pos == 0
        } else {
            ALPHABET[..radix as usize].contains(&b.to_ascii_lowercase())
        }
    };

    let mut end = 0;
    let bytes = n.as_bytes();
    while end != bytes.len() {
        if is_numeric(end, bytes[end], bytes.get(end + 1).copied()) {
            end += 1;
        } else {
            break;
        }
    }

    if let Some(e) = seen_e {
        // Rust `from_str_radix` does not support this notation
        let lhs = n[..e].parse::<i64>()?;
        let rhs = n[e + 1..end].parse::<u32>()?;
        Ok(lhs * 10_i64.pow(rhs))
    } else {
        i64::from_str_radix(&n[..end], radix)
    }
    .map(|value| (value, &n[end..]))
}

/// Encodes the `input` similar to
/// [`rawurlencode`](https://www.php.net/rawurlencode).
#[inline]
#[must_use]
pub fn raw_url_encode(input: &str) -> percent_encoding::PercentEncode<'_> {
    raw_url_encode_bytes(input.as_bytes())
}

/// Encodes the `input` similar to
/// [`rawurlencode`](https://www.php.net/rawurlencode).
#[inline]
#[must_use]
pub fn raw_url_encode_bytes(input: &[u8]) -> percent_encoding::PercentEncode<'_> {
    const ALPHABET: percent_encoding::AsciiSet = URL_ENCODE_ALPHABET.add(b' ').remove(b'~');
    percent_encoding::percent_encode(input, &ALPHABET)
}

/// Finds and replaces substrings in the input like [`strtr`](https://php.net/strtr).
/// To avoid extra temporary allocation, `replacements` should be ordered from
/// longest to shortest match.
// TODO: Use a trie or some other structure for the replacements list which is
// more efficient and does not require manual care to be sorted.
#[must_use]
pub fn strtr<'a>(input: &'a str, replacements: &[(&str, &str)]) -> Cow<'a, str> {
    let replacements = if replacements.is_sorted_by(|(a, _), (b, _)| a.len() >= b.len()) {
        Cow::Borrowed(replacements)
    } else {
        let mut replacements = Vec::from(replacements);
        replacements.sort_by_key(|(needle, _)| core::cmp::Reverse(needle.len()));
        Cow::Owned(replacements)
    };

    let mut out = String::new();
    let mut offset = 0;
    let mut flushed = 0;
    'next: while offset != input.len() {
        for (find, replace) in replacements.iter() {
            if input[offset..].starts_with(find) {
                out += &input[flushed..offset];
                out += *replace;
                offset += find.len();
                flushed = offset;
                continue 'next;
            }
        }
        offset = input.ceil_char_boundary(offset + 1);
    }

    if flushed == 0 {
        Cow::Borrowed(input)
    } else {
        out += &input[flushed..];
        Cow::Owned(out)
    }
}

/// Casts a float to a string similar to [`strval`](https://www.php.net/strval).
#[must_use]
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

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "intended behaviour"
    )]
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
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "value is known to be positive and in range"
        )]
        {
            format!("{:.13}E+{exp}", n / 10.0_f64.powi(exp as i32))
        }
    }
}

/// Uppercases the first letter of the given `text` if it is ASCII, similar to
/// [`ucfirst`](https://www.php.net/ucfirst).
#[inline]
#[must_use]
pub fn ucfirst(text: &str) -> Cow<'_, str> {
    let mut iter = text.chars();
    if let Some(first) = iter.next()
        && let upper = first.to_ascii_uppercase()
        && upper != first
    {
        Cow::Owned(format!("{upper}{}", iter.as_str()))
    } else {
        Cow::Borrowed(text)
    }
}

/// Encodes the `input` similar to [`urlencode`](https://www.php.net/urlencode).
#[inline]
#[must_use]
pub fn url_encode(input: &str) -> Cow<'_, str> {
    url_encode_alphabet(input, &URL_ENCODE_ALPHABET)
}

/// Encodes the `input` similar to [`urlencode`](https://www.php.net/urlencode)
/// using a custom alphabet.
#[inline]
#[must_use]
pub fn url_encode_alphabet<'a>(
    input: &'a str,
    alphabet: &'static percent_encoding::AsciiSet,
) -> Cow<'a, str> {
    url_encode_bytes_alphabet(input.as_bytes(), alphabet)
}

/// Encodes the `input` similar to [`urlencode`](https://www.php.net/urlencode).
#[inline]
#[must_use]
pub fn url_encode_bytes(input: &[u8]) -> Cow<'_, str> {
    url_encode_bytes_alphabet(input, &URL_ENCODE_ALPHABET)
}

/// Encodes the `input` similar to [`urlencode`](https://www.php.net/urlencode),
/// using a custom alphabet.
#[must_use]
pub fn url_encode_bytes_alphabet<'a>(
    input: &'a [u8],
    alphabet: &'static percent_encoding::AsciiSet,
) -> Cow<'a, str> {
    let mut flushed = 0;
    let mut out = String::new();
    for space in memchr::memchr_iter(b' ', input) {
        out.extend(percent_encoding::percent_encode(
            &input[flushed..space],
            alphabet,
        ));
        out.push('+');
        flushed = space + " ".len();
    }
    if flushed == 0 {
        Cow::from(percent_encoding::percent_encode(input, alphabet))
    } else {
        out.extend(percent_encoding::percent_encode(
            &input[flushed..],
            alphabet,
        ));
        Cow::Owned(out)
    }
}

/// The alphabet of characters to percent-encode when encoding URLs used by PHP
/// `urlencode`.
pub const URL_ENCODE_ALPHABET: percent_encoding::AsciiSet = percent_encoding::NON_ALPHANUMERIC
    .remove(b' ')
    .remove(b'-')
    .remove(b'_')
    .remove(b'.');

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
        assert_eq!(floatval("-122.34343The"), Ok((-122.34343, "The")));
        assert_eq!(floatval("-122-"), Ok((-122.0, "-")));
        assert_eq!(floatval("-122ee"), Ok((-122.0, "ee")));
        assert_eq!(floatval("1,200"), Ok((1.0, ",200")));
        assert_eq!(floatval("-1,200"), Ok((-1.0, ",200")));
    }

    #[test]
    fn test_intval() {
        assert_eq!(intval("122.34343The", None), Ok((122, ".34343The")));
        assert_eq!(intval("0x9,200", Some(0)), Ok((9, ",200")));
        assert_eq!(intval("0X9+200", Some(0)), Ok((9, "+200")));
        assert_eq!(intval("0b112", Some(0)), Ok((3, "2")));
        assert_eq!(intval("0B10", Some(0)), Ok((2, "")));
        assert_eq!(intval("077", Some(0)), Ok((63, "")));
        assert_eq!(intval("077", None), Ok((77, "")));
        assert_eq!(intval("1,200", None), Ok((1, ",200")));
        assert_eq!(intval("9e,200", None), Ok((9, "e,200")));
        assert_eq!(intval("-1,200", None), Ok((-1, ",200")));
        assert_eq!(intval("-1-,200", None), Ok((-1, "-,200")));
        assert_eq!(intval("1e5e1", None), Ok((100_000, "e1")));
        assert_eq!(intval("-1e5e1", None), Ok((-100_000, "e1")));
        assert_eq!(intval("-1e5-e1", None), Ok((-100_000, "-e1")));
        assert_eq!(intval("keklol", Some(21)), Ok((9134, "lol")));
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

        // do match unicode without skips
        assert_eq!(
            strtr("🤏🤏💦🤏", &[("🤏", "he")]),
            Cow::<str>::Owned(String::from("hehe💦he"))
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
