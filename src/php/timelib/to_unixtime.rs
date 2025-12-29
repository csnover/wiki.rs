//! Conversion code to construct [`DateTime`] from a [`DateTimeBuilder`].

// This code is adapted from timelib <https://github.com/derickr/timelib>.
// The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-Copyright-Text: Copyright (c) 2015-2023 Derick Rethans
// SPDX-Copyright-Text: Copyright (c) 2018 MongoDB, Inc.

use super::{
    super::{DateTime, DateTimeZone},
    DateTimeBuilder, Error, Keyword, Relatime, Special, Timezone, WeekdayBehavior, Weekdays,
};
use time::{Date, Duration, Month, Time, UtcOffset, Weekday};

impl<'a> DateTimeBuilder<'a> {
    /// Builds a [`DateTime`] object from this builder, using `other` to fill
    /// any fields which were not previously set.
    pub fn build(mut self, other: Option<Self>) -> Result<DateTime, Error> {
        if let Some(other) = other {
            self.fill_holes(other);
        }

        let (mut year, mut month, mut day) = (
            self.date.year.ok_or(Error::MissingData)?,
            self.date.month.ok_or(Error::MissingData)?,
            self.date.day.ok_or(Error::MissingData)?,
        );

        adjust_special_early(&self.relative, &mut year, &mut month, &mut day)?;

        if self.relative.weekday.is_some() {
            adjust_for_weekday(&self.relative, &mut year, &mut month, &mut day)?;
        }

        adjust_relative_date(&self.relative, &mut year, &mut month, &mut day)?;

        let month = Month::try_from(u8::try_from(month)?)?;
        let date = Date::from_calendar_date(year.try_into()?, month, day.try_into()?)?;

        let date = if let Some(Special::WeekdayCount(count)) = self.relative.special {
            adjust_special_weekday(date, count)?
        } else {
            date
        };

        let time = Time::from_hms_micro(
            self.time.hour.ok_or(Error::MissingData)?.try_into()?,
            self.time.minute.ok_or(Error::MissingData)?.try_into()?,
            self.time.second.ok_or(Error::MissingData)?.try_into()?,
            self.time.micros.ok_or(Error::MissingData)?.try_into()?,
        )?;

        let time_delta = Duration::new(
            self.relative.h * 3600 + self.relative.i * 60 + self.relative.s,
            (self.relative.us * 1_000).try_into()?,
        );

        let datetime = time::PrimitiveDateTime::new(date, time).saturating_add(time_delta);

        let (offset, tz) = match self.offset.as_ref() {
            Some(Timezone::Offset(offset)) => {
                let offset = UtcOffset::from_whole_seconds(*offset)?;
                (offset, DateTimeZone::Offset(offset))
            }
            Some(tz @ Timezone::Alias(alias)) => {
                // TODO: This is very dumb and is happening because the
                // parse_date tests expect the offset to exclude DST, so the
                // offset function call subtracts it
                let offset = tz.offset() + if tz.is_dst() { 3600 } else { 0 };
                let local = tz::LocalTimeType::new(offset, tz.is_dst(), Some(alias.as_bytes()))
                    .map_err(tz::Error::from)?;

                (
                    UtcOffset::from_whole_seconds(offset)?,
                    DateTimeZone::Alias(local),
                )
            }
            Some(Timezone::Named(name)) => {
                let tz = tzdb_data::find_tz(name.as_bytes())
                    .ok_or(tz::Error::Tz(tz::TzError::NoAvailableLocalTimeType))?;
                let times = tz::DateTime::find(
                    datetime.year(),
                    datetime.month().into(),
                    datetime.day(),
                    datetime.hour(),
                    datetime.minute(),
                    datetime.second(),
                    datetime.nanosecond(),
                    *tz,
                )
                .map_err(tz::Error::from)?;
                let local = if let Some(local_tz) = times.unique() {
                    *local_tz.local_time_type()
                } else if let (Some(earliest), Some(latest)) = (times.earliest(), times.latest()) {
                    let earliest = earliest.local_time_type();
                    let latest = latest.local_time_type();
                    if earliest.is_dst() && !latest.is_dst() && earliest.ut_offset().is_positive() {
                        *latest
                    } else {
                        *earliest
                    }
                } else {
                    return Err(tz::Error::from(tz::TzError::NoAvailableLocalTimeType))?;
                };
                (
                    UtcOffset::from_whole_seconds(local.ut_offset())?,
                    DateTimeZone::Named(name, *tz),
                )
            }
            None => (UtcOffset::UTC, DateTimeZone::UTC),
        };

        Ok(DateTime {
            inner: datetime.assume_offset(offset),
            tz,
        })
    }

    /// Fills undefined time parts in `self` with values from `other`.
    fn fill_holes(&mut self, other: DateTimeBuilder<'a>) {
        if self.time.micros.is_none() {
            self.time.micros = if self.date == <_>::default()
                && self.time == <_>::default()
                && other.time.micros.is_some()
            {
                other.time.micros
            } else {
                Some(0)
            };
        }

        self.date.year.get_or_insert(other.date.year.unwrap_or(0));
        self.date.month.get_or_insert(other.date.month.unwrap_or(0));
        self.date.day.get_or_insert(other.date.day.unwrap_or(0));
        self.time.hour.get_or_insert(other.time.hour.unwrap_or(0));
        self.time
            .minute
            .get_or_insert(other.time.minute.unwrap_or(0));
        self.time
            .second
            .get_or_insert(other.time.second.unwrap_or(0));

        if self.offset.is_none()
            && let Some(tz) = other.offset
        {
            self.offset.replace(tz);
        }
    }
}

/// Adjusts the out-params `year`, `month`, and `day` according to any weekday
/// given in `relative.weekday` and `relative.weekday_behavior`.
fn adjust_for_weekday(
    relative: &Relatime,
    year: &mut i64,
    month: &mut i64,
    day: &mut i64,
) -> Result<(), Error> {
    let current_dow = i64::from(
        Date::from_calendar_date(
            i32::try_from(*year)?,
            Month::try_from(u8::try_from(*month)?)?,
            u8::try_from(*day)?,
        )?
        .weekday()
        .number_days_from_sunday(),
    );

    let weekday = match relative.weekday {
        Some(Weekdays::Weekday(weekday)) => i64::from(weekday.number_days_from_sunday()),
        Some(Weekdays::Ago(weekday)) => match -i64::from(weekday.number_days_from_sunday()) {
            0 => -7,
            value => value,
        },
        Some(Weekdays::All) | None => panic!("this should be impossible"),
    };

    match relative.weekday_behavior {
        WeekdayBehavior::IgnoreCurrentDay | WeekdayBehavior::CountCurrentDay => {
            if weekday.is_negative() {
                *day -= 7 - (weekday.abs() - current_dow);
            } else {
                let behavior = -(relative.weekday_behavior as i64);
                let mut delta = weekday - current_dow;
                if (relative.d < 0 && delta < 0) || (relative.d >= 0 && delta <= behavior) {
                    delta += 7;
                }
                *day += delta;
            }
        }
        WeekdayBehavior::RelativeTextWeek => {
            let adjust = if current_dow == 0 && weekday != 0 {
                // To make "this week" work, where the current DOW is a "sunday"
                weekday - 7
            } else if current_dow != 0 && weekday == 0 {
                // To make "sunday this week" work, where the current DOW is not
                // a "sunday"
                7
            } else {
                weekday
            };

            *day += adjust - current_dow;
        }
    }
    norm(year, month, day)
}

/// Adjusts the out-params `year`, `month`, and `day` according to any relative
/// year, month, day, and first/last day of month given in `relative`.
fn adjust_relative_date(
    relative: &Relatime,
    year: &mut i64,
    month: &mut i64,
    day: &mut i64,
) -> Result<(), Error> {
    *year += relative.y;
    if !matches!(
        relative.special,
        Some(Special::LastDayOfWeekInMonth | Special::NthDayOfWeekInMonth)
    ) {
        *month += relative.m;
    }
    *day += relative.d;

    match relative.first_last_day_of {
        Some(Keyword::FirstDay) => {
            *day = 1;
        }
        Some(Keyword::LastDay) => {
            *day = 0;
            *month += 1;
        }
        None => {}
    }
    norm(year, month, day)
}

/// Adjusts the out-params `year`, `month`, and `day` according to the values in
/// `relative.special` and `relative.first_last_day_of`.
fn adjust_special_early(
    relative: &Relatime,
    year: &mut i64,
    month: &mut i64,
    day: &mut i64,
) -> Result<(), Error> {
    match relative.special {
        Some(Special::NthDayOfWeekInMonth) => {
            *day = 1;
            *month += relative.m;
            norm(year, month, day)?;
        }
        Some(Special::LastDayOfWeekInMonth) => {
            *day = 1;
            *month += relative.m + 1;
            norm(year, month, day)?;
        }
        _ => {}
    }
    match relative.first_last_day_of {
        Some(Keyword::FirstDay) => {
            *day = 1;
        }
        Some(Keyword::LastDay) => {
            *day = 0;
            *month += 1;
            norm(year, month, day)?;
        }
        None => {}
    }
    Ok(())
}

/// Returns a new date that is `count` weekdays away from `date`.
fn adjust_special_weekday(date: Date, count: i64) -> Result<Date, Error> {
    let date = date
        .checked_add(Duration::weeks(count / 5))
        .ok_or(Error::WeekdaysRange)?;

    let days = count % 5;
    // Clippy: Guaranteed to be in range, number is mod 5.
    #[allow(clippy::cast_sign_loss)]
    if days == 0 {
        Ok(match date.weekday() {
            Weekday::Saturday | Weekday::Sunday => {
                if count.is_positive() {
                    date.prev_occurrence(Weekday::Friday)
                } else {
                    date.next_occurrence(Weekday::Monday)
                }
            }
            _ => date,
        })
    } else {
        let delta = if count.is_positive() {
            if date.weekday() == Weekday::Saturday {
                // We ended up on Saturday, but there's still work to do, so
                // move to Sunday and continue from there.
                days + 1
            } else if matches!(
                date.weekday().nth_next(days as u8),
                Weekday::Saturday | Weekday::Sunday
            ) {
                // We're on a weekday, but we're going past Friday, so skip
                // right over the weekend.
                days + 2
            } else {
                days
            }
        } else if date.weekday() == Weekday::Sunday {
            days - 1
        } else if matches!(date.weekday().nth_next(days as u8), Weekday::Sunday) {
            days - 2
        } else {
            days
        };

        date.checked_add(Duration::days(delta))
            .ok_or(Error::WeekdaysRange)
    }
}

/// Returns the number of days in the given year and month.
#[inline]
fn days_in_month(year: i64, month: i64) -> Result<i64, Error> {
    let month = Month::try_from(u8::try_from(month)?)?;
    Ok(i64::from(time::util::days_in_month(
        month,
        i32::try_from(year)?,
    )))
}

/// Decrements a month by 1 with rollover.
#[inline]
fn dec_month(year: &mut i64, month: &mut i64) {
    *month -= 1;
    if *month == 0 {
        *year -= 1;
        *month = 12;
    }
}

/// Increments a month by 1 with rollover.
#[inline]
fn inc_month(year: &mut i64, month: &mut i64) {
    *month += 1;
    if *month == 13 {
        *year += 1;
        *month = 1;
    }
}

/// Limits the out-param `value` to a range `start..start + len`, with any
/// overflow going to `overflow`.
#[inline]
fn limit_range(start: i64, len: i64, value: &mut i64, overflow: &mut i64) {
    if *value < start {
        // We calculate `value + 1` first as `start - *value - 1` can overflow
        // i64 if `value` is `i64::MIN`. `start` is 0 in this context, and
        // `0 - i64::MIN > i64::MAX`.
        *overflow -= (start - (*value + 1)) / len + 1;
        // TODO: According to the original code, this adds the extra `len`
        // separately as otherwise this can overflow i64 in situations where
        // `overflow` is near `i64::MIN`, but this does not make a lot of sense
        // given how it is not operating on `overflow`.
        *value += len * ((start - (*value + 1)) / len);
        *value += len;
    } else if *value >= start + len {
        *overflow += *value / len;
        *value -= len * (*value / len);
    }
}

/// Normalises a year, month, and day by overflowing or underflowing values into
/// the previous/next date component until each component is in a valid range
/// for the Gregorian calendar.
fn norm(year: &mut i64, month: &mut i64, day: &mut i64) -> Result<(), Error> {
    // 0001-00-00 is 0000-11-30 :-) :-)
    limit_range(1, 12, month, year);

    while *day <= 0 {
        dec_month(year, month);
        *day += days_in_month(*year, *month)?;
    }
    let mut days = days_in_month(*year, *month)?;
    while *day > days {
        inc_month(year, month);
        *day -= days;
        days = days_in_month(*year, *month)?;
    }

    Ok(())
}
