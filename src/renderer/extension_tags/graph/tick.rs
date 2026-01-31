//! Utility functions for calculating ticks from ranges.

// SPDX-License-Identifier: ISC
// Adapted from d3-time 3.1 and d3-scale 4.0 by Mike Bostock

use super::{EPSILON, TimeExt as _};
use crate::php::DateTime;
use core::cmp::Ordering;
use time::{Date, Duration, Month};

/// A time interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TimeInterval {
    /// N milliseconds.
    Millisecond(i64),
    /// N seconds.
    Second(i64),
    /// N minutes.
    Minute(i64),
    /// N hours.
    Hour(i64),
    /// N days.
    Day(i64),
    /// N weeks.
    Week(i64),
    /// N months.
    Month(i64),
    /// N years.
    Year(i64),
}

impl TimeInterval {
    /// Rounds up to the nearest N unit.
    pub fn ceil(self, date: DateTime) -> DateTime {
        self.floor(self.step(self.floor(date - Duration::milliseconds(1))))
    }

    /// Rounds down to the nearest `N.max(1)` unit.
    pub fn floor(self, date: DateTime) -> DateTime {
        fn trunc<F>(date: DateTime, value: i64, interval: i64, f: F) -> DateTime
        where
            F: FnOnce(i64) -> Duration,
        {
            date - f(value % interval.max(1))
        }

        match self {
            TimeInterval::Millisecond(ms) => {
                trunc(date, date.millisecond().into(), ms, Duration::milliseconds)
                    .truncate_to_millisecond()
            }
            TimeInterval::Second(s) => {
                trunc(date, date.second().into(), s, Duration::seconds).truncate_to_second()
            }
            TimeInterval::Minute(m) => {
                trunc(date, date.minute().into(), m, Duration::minutes).truncate_to_minute()
            }
            TimeInterval::Hour(h) => {
                trunc(date, date.hour().into(), h, Duration::hours).truncate_to_hour()
            }
            TimeInterval::Day(d) => {
                trunc(date, (date.day() - 1).into(), d, Duration::days).truncate_to_day()
            }
            TimeInterval::Week(w) => {
                // This is weird because in D3 a “week” interval floors to
                // Sunday but the step is seven days. There is no unit which
                // makes sense for “floor N days of the week”. Luckily(?) the
                // nice tick step intervals only use 1 week at a time.
                debug_assert!(w == -1 || w == 1);
                (date - Duration::days(-i64::from(date.weekday().number_days_from_sunday())))
                    .truncate_to_day()
            }
            TimeInterval::Month(m) => {
                // Again, D3 “month” floors to the first day of the month but
                // the steps are months. Because months are not fixed sizes,
                // the values are replaced instead of using a calculated
                // duration
                debug_assert!((-12..=12).contains(&m));
                let month = date.month() as i64;
                // Clippy: The input value and modulo are both always >= 1.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let month = date.month().nth_prev(((month - 1) % m.max(1)) as u8);
                date.replace_day(1)
                    .unwrap()
                    .replace_month(month)
                    .unwrap()
                    .truncate_to_day()
            }
            TimeInterval::Year(y) => {
                let year = i64::from(date.year());
                let year = year - year % y;
                // Clippy: The year value comes from an i32.
                #[allow(clippy::cast_possible_truncation)]
                date.replace_day(1)
                    .unwrap()
                    .replace_month(Month::January)
                    .unwrap()
                    .replace_year(year as i32)
                    .unwrap()
                    .truncate_to_day()
            }
        }
    }

    /// Steps the given date by `self`.
    fn step(self, date: DateTime) -> DateTime {
        match self {
            TimeInterval::Month(interval) => {
                // The time crate does not currently have a mechanism for doing
                // month or year addition, and just changing the year/month will
                // fail if the day ends up being invalid. So your intrepid
                // programmer must do this cursed calculation manually here,
                // which is very unfortunate since the time crate *does* have
                // overflow arithmetic stuff internally that is just not
                // exposed.

                // Overflow the month into the year by including it in the
                // interval and then dividing. We are blessed that years
                // (currently) always have the same number of months.
                let interval = interval + i64::from(date.month() as u8);
                let year = date.year() + i32::try_from(interval / 12).unwrap();
                // Clippy: The value is mod 12 so is guaranteed to fit, and sign
                // checked so guaranteed to be positive.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let mut month = Month::try_from(if interval < 0 {
                    12 + (interval % 12)
                } else {
                    interval % 12
                } as u8)
                .unwrap();
                let mut day = date.day();
                let max_days = month.length(year);
                if date.day() > max_days {
                    // Thankfully because December has the most days of any
                    // month (at the moment anyway) this will never happen at
                    // the end of the year so only the month rollover needs to
                    // be considered
                    day -= max_days;
                    month = month.next();
                }
                date.replace_date(Date::from_calendar_date(year, month, day).unwrap())
            }
            TimeInterval::Year(interval) => {
                let year = date.year() + i32::try_from(interval).unwrap();
                // Leap days are very dangerous days.
                if date.month() == Month::February
                    && date.day() == 29
                    && !time::util::is_leap_year(year)
                {
                    date.replace_date(Date::from_calendar_date(year, Month::March, 1).unwrap())
                } else {
                    date.replace_year(year).unwrap()
                }
            }
            fixed => date + fixed.into(),
        }
    }
}

impl From<TimeInterval> for Duration {
    fn from(value: TimeInterval) -> Self {
        match value {
            TimeInterval::Millisecond(ms) => Duration::milliseconds(ms),
            TimeInterval::Second(s) => Duration::seconds(s),
            TimeInterval::Minute(m) => Duration::minutes(m),
            TimeInterval::Hour(h) => Duration::hours(h),
            TimeInterval::Day(d) => Duration::days(d),
            TimeInterval::Week(w) => Duration::weeks(w),
            TimeInterval::Month(m) => Duration::days(m * 30),
            TimeInterval::Year(y) => Duration::days(y * 365),
        }
    }
}

impl From<Duration> for TimeInterval {
    fn from(value: Duration) -> Self {
        if let Ok(ms) = i64::try_from(value.whole_milliseconds()) {
            Self::Millisecond(ms)
        } else {
            Self::Second(value.whole_seconds())
        }
    }
}

impl Ord for TimeInterval {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        Duration::from(*self).cmp(&Duration::from(*other))
    }
}

impl PartialOrd for TimeInterval {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl core::ops::Neg for TimeInterval {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            TimeInterval::Millisecond(ms) => TimeInterval::Millisecond(ms),
            TimeInterval::Second(s) => TimeInterval::Second(s),
            TimeInterval::Minute(m) => TimeInterval::Minute(m),
            TimeInterval::Hour(h) => TimeInterval::Hour(h),
            TimeInterval::Day(d) => TimeInterval::Day(d),
            TimeInterval::Week(w) => TimeInterval::Week(w),
            TimeInterval::Month(m) => TimeInterval::Month(m),
            TimeInterval::Year(y) => TimeInterval::Year(y),
        }
    }
}

/// Returns an iterator that generates approximately `count` evenly distributed
/// steps for the given range.
///
/// This is a newer version of the D3 algorithm which does not exactly match
/// the output of the D3 3.x algorithm used by Vega 2, but this is probably
/// fine.
pub(super) fn ticks(start: f64, stop: f64, count: f64) -> impl Iterator<Item = f64> {
    let (reverse, i1, i2, inc, n) = if count.partial_cmp(&0.0) != Some(Ordering::Greater) {
        <_>::default()
    } else if (start - stop).abs() < EPSILON {
        (false, start, stop, 1.0, 1)
    } else {
        let reverse = stop < start;
        let (i1, i2, inc) = if reverse {
            tick_spec(stop, start, count)
        } else {
            tick_spec(start, stop, count)
        };

        if i2 >= i1 {
            // Clippy: The input values are already integral by rounding and
            // guaranteed to be ordered such that i2 >= i1.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = (i2 - i1) as u32;
            (reverse, i1, i2, inc, n + 1)
        } else {
            <_>::default()
        }
    };

    (0..n).map(move |i| {
        let i = f64::from(i);
        match (reverse, inc < 0.0) {
            (true, true) => (i2 - i) / -inc,
            (true, false) => (i2 - i) * inc,
            (false, true) => (i1 + i) / -inc,
            (false, false) => (i1 + i) * inc,
        }
    })
}

/// Returns the distance between ticks for the given range and tick count.
pub(super) fn tick_increment(start: f64, stop: f64, count: f64) -> f64 {
    tick_spec(start, stop, count).2
}

/// Returns the signed distance between ticks for the given range and tick
/// count.
pub(super) fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let reverse = stop < start;
    let inc = if reverse {
        tick_increment(stop, start, count)
    } else {
        tick_increment(start, stop, count)
    };
    let inc = if inc < 0.0 { 1.0 / -inc } else { inc };
    if reverse { -inc } else { inc }
}

/// Returns an iterator that generates approximately `count` evenly distributed
/// steps for the given time range.
pub(super) fn time_ticks(start: f64, stop: f64, count: f64) -> impl Iterator<Item = f64> {
    const STEPS: &[TimeInterval] = &[
        TimeInterval::Second(1),
        TimeInterval::Second(5),
        TimeInterval::Second(15),
        TimeInterval::Second(30),
        TimeInterval::Minute(1),
        TimeInterval::Minute(5),
        TimeInterval::Minute(15),
        TimeInterval::Minute(30),
        TimeInterval::Hour(1),
        TimeInterval::Hour(3),
        TimeInterval::Hour(6),
        TimeInterval::Hour(12),
        TimeInterval::Day(1),
        TimeInterval::Day(2),
        TimeInterval::Week(1),
        TimeInterval::Month(1),
        TimeInterval::Month(3),
        TimeInterval::Year(1),
    ];

    let start_time = DateTime::from_f64(start, false);
    let stop_time = DateTime::from_f64(stop, false);

    let duration = stop_time - start_time;

    let target_interval = duration.abs() / count;
    let best = STEPS
        .binary_search(&TimeInterval::from(target_interval))
        .unwrap_or_else(|index| index);

    let sign = if duration.is_negative() { -1 } else { 1 };
    let interval = if best == 0 {
        // Clippy: The value is guaranteed to be >=1 so will always advance even
        // if truncated, is >0 so floor is the same as trunc, and these values
        // in D3 are floored.
        #[allow(clippy::cast_possible_truncation)]
        let interval = tick_step(start, stop, count).max(1.0) as i64;
        TimeInterval::Millisecond(sign * interval)
    } else if best == STEPS.len() {
        const MS_PER_YEAR: f64 = 1_000.0 * 60.0 * 60.0 * 24.0 * 365.0;
        // Clippy: The value is floored so will never have a fractional part.
        #[allow(clippy::cast_possible_truncation)]
        let interval = tick_step(start / MS_PER_YEAR, stop / MS_PER_YEAR, count).floor() as i64;
        TimeInterval::Year(interval)
    } else {
        let to_floor = target_interval / Duration::from(STEPS[best - 1]);
        let to_ceil = Duration::from(STEPS[best]) / target_interval;
        let interval = STEPS[best - usize::from(to_floor < to_ceil)];
        if duration.is_negative() {
            -interval
        } else {
            interval
        }
    };

    let mut tick = interval.ceil(start_time);
    let stop = interval.floor(stop_time);
    core::iter::from_fn(move || {
        (tick <= stop).then(|| {
            let value = tick.to_f64();
            tick = interval.step(tick);
            value
        })
    })
}

/// Calculates the integral range and step size for the given range and count.
pub(super) fn tick_spec(first: f64, last: f64, count: f64) -> (f64, f64, f64) {
    const E10: f64 = 7.071_067_811_865_475_5 /* 50.0_f64.sqrt() */;
    const E5: f64 = 3.162_277_660_168_379_5 /* 10.0_f64.sqrt() */;
    const E2: f64 = core::f64::consts::SQRT_2;

    let step = (last - first) / count.max(1.0);
    let power = step.log10().floor();
    let error = step / 10.0_f64.powf(power);
    let factor = if error >= E10 {
        10.0
    } else if error >= E5 {
        5.0
    } else if error >= E2 {
        2.0
    } else {
        1.0
    };

    let (mut i1, mut i2, mut inc);
    if power < 0.0 {
        inc = 10.0_f64.powf(-power) / factor;
        i1 = (first * inc).round();
        i2 = (last * inc).round();
        if i1 / inc < first {
            i1 += 1.0;
        }
        if i2 / inc > last {
            i2 -= 1.0;
        }
        inc = -inc;
    } else {
        inc = 10.0_f64.powf(power) * factor;
        i1 = (first / inc).round();
        i2 = (last / inc).round();
        if i1 * inc < first {
            i1 += 1.0;
        }
        if i2 * inc > last {
            i2 -= 1.0;
        }
    }

    if i2 < i1 && (0.5..2.0).contains(&count) {
        tick_spec(first, last, count * 2.0)
    } else {
        (i1, i2, inc)
    }
}
