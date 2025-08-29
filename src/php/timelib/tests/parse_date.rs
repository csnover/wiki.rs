// This code is adapted from timelib <https://github.com/derickr/timelib>.
// The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-Copyright-Text: Copyright (c) 2015-2023 Derick Rethans
// SPDX-Copyright-Text: Copyright (c) 2018 MongoDB, Inc.

use super::super::{
    DateTimeBuilder, Keyword, Special, Timezone, Weekday, WeekdayBehavior, Weekdays,
    parse_date::PegError,
};
use std::borrow::Cow;

#[track_caller]
fn test_parse(text: &str) -> TestResult<'_> {
    let result = super::super::parse_date::parse(text);
    let mut result = TestResult {
        state: TestWrapper {
            inner: result.builder,
            dst: false,
        },
        errors: result.errors,
    };
    if let Some(tz) = &result.state.offset {
        result.state.dst = tz.is_dst();
    }
    result
}

struct TestResult<'a> {
    state: TestWrapper<'a>,
    errors: Vec<PegError>,
}

struct TestWrapper<'a> {
    inner: DateTimeBuilder<'a>,
    dst: bool,
}

impl<'a> core::ops::Deref for TestWrapper<'a> {
    type Target = DateTimeBuilder<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[test]
fn american_00() {
    let t = test_parse("9/11").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(11), t.date.day);
}

#[test]
fn american_01() {
    let t = test_parse("09/11").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(11), t.date.day);
}

#[test]
fn american_02() {
    let t = test_parse("12/22/69").state;
    assert_eq!(Some(2069), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn american_03() {
    let t = test_parse("12/22/70").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn american_04() {
    let t = test_parse("12/22/78").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn american_05() {
    let t = test_parse("12/22/1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn american_06() {
    let t = test_parse("12/22/2078").state;
    assert_eq!(Some(2078), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn bug37017_00() {
    let t = test_parse("2006-05-12 12:59:59 America/New_York").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(59), t.time.second);
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/New_York")))
    );
}

#[test]
fn bug37017_01() {
    let t = test_parse("2006-05-12 13:00:00 America/New_York").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/New_York")))
    );
}

#[test]
fn bug37017_02() {
    let t = test_parse("2006-05-12 13:00:01 America/New_York").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(1), t.time.second);
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/New_York")))
    );
}

#[test]
fn bug37017_03() {
    let t = test_parse("2006-05-12 12:59:59 GMT").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(59), t.time.second);
}

#[test]
fn bug41523_00() {
    let t = test_parse("0000-00-00").state;
    assert_eq!(Some(0), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_01() {
    let t = test_parse("0001-00-00").state;
    assert_eq!(Some(1), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_02() {
    let t = test_parse("0002-00-00").state;
    assert_eq!(Some(2), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_03() {
    let t = test_parse("0003-00-00").state;
    assert_eq!(Some(3), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_04() {
    let t = test_parse("000-00-00").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_05() {
    let t = test_parse("001-00-00").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_06() {
    let t = test_parse("002-00-00").state;
    assert_eq!(Some(2002), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_07() {
    let t = test_parse("003-00-00").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_08() {
    let t = test_parse("00-00-00").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_09() {
    let t = test_parse("01-00-00").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_10() {
    let t = test_parse("02-00-00").state;
    assert_eq!(Some(2002), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41523_11() {
    let t = test_parse("03-00-00").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(0), t.date.month);
    assert_eq!(Some(0), t.date.day);
}

#[test]
fn bug41842_00() {
    let t = test_parse("-0001-06-28").state;
    assert_eq!(Some(-1), t.date.year);
    assert_eq!(Some(6), t.date.month);
    assert_eq!(Some(28), t.date.day);
}

#[test]
fn bug41842_01() {
    let t = test_parse("-2007-06-28").state;
    assert_eq!(Some(-2007), t.date.year);
    assert_eq!(Some(6), t.date.month);
    assert_eq!(Some(28), t.date.day);
}

#[test]
fn bug41964_00() {
    let _ = test_parse("Ask the experts").state;
}

#[test]
fn bug41964_01() {
    let t = test_parse("A").state;
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("A"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn bug41964_02() {
    let t = test_parse("A Revolution in Development").state;
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("A"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn bug44426_00() {
    let t = test_parse("Aug 27 2007 12:00:00:000AM").state;
    assert_eq!(Some(2007), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(27), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug44426_01() {
    let t = test_parse("Aug 27 2007 12:00:00.000AM").state;
    assert_eq!(Some(2007), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(27), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug44426_02() {
    let t = test_parse("Aug 27 2007 12:00:00:000").state;
    assert_eq!(Some(2007), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(27), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug44426_03() {
    let t = test_parse("Aug 27 2007 12:00:00.000").state;
    assert_eq!(Some(2007), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(27), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug44426_04() {
    let t = test_parse("Aug 27 2007 12:00:00AM").state;
    assert_eq!(Some(2007), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(27), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug44426_05() {
    let t = test_parse("Aug 27 2007").state;
    assert_eq!(Some(2007), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(27), t.date.day);
}

#[test]
fn bug44426_06() {
    let t = test_parse("Aug 27 2007 12:00AM").state;
    assert_eq!(Some(2007), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(27), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug50392_00() {
    let t = test_parse("2010-03-06 16:07:25").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
}

#[test]
fn bug50392_01() {
    let t = test_parse("2010-03-06 16:07:25.1").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(100_000), t.time.micros);
}

#[test]
fn bug50392_02() {
    let t = test_parse("2010-03-06 16:07:25.12").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(120_000), t.time.micros);
}

#[test]
fn bug50392_03() {
    let t = test_parse("2010-03-06 16:07:25.123").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(123_000), t.time.micros);
}

#[test]
fn bug50392_04() {
    let t = test_parse("2010-03-06 16:07:25.1234").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(123_400), t.time.micros);
}

#[test]
fn bug50392_05() {
    let t = test_parse("2010-03-06 16:07:25.12345").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
}

#[test]
fn bug50392_06() {
    let t = test_parse("2010-03-06 16:07:25.123456").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(123_456), t.time.micros);
}

#[test]
fn bug50392_07() {
    let t = test_parse("2010-03-06 16:07:25.1234567").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(123_456), t.time.micros);
}

#[test]
fn bug50392_08() {
    let t = test_parse("2010-03-06 16:07:25.12345678").state;
    assert_eq!(Some(2010), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(6), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(7), t.time.minute);
    assert_eq!(Some(25), t.time.second);
    assert_eq!(Some(123_456), t.time.micros);
}

#[test]
fn bug51096_00() {
    let t = test_parse("first day").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(None, t.time.hour);
    assert_eq!(None, t.time.minute);
    assert_eq!(None, t.time.second);
}

#[test]
fn bug51096_01() {
    let t = test_parse("last day").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(None, t.time.hour);
    assert_eq!(None, t.time.minute);
    assert_eq!(None, t.time.second);
}

#[test]
fn bug51096_02() {
    let t = test_parse("next month").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(1, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(None, t.time.hour);
    assert_eq!(None, t.time.minute);
    assert_eq!(None, t.time.second);
}

#[test]
fn bug51096_03() {
    let t = test_parse("first day next month").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(1, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(None, t.time.hour);
    assert_eq!(None, t.time.minute);
    assert_eq!(None, t.time.second);
}

#[test]
fn bug51096_04() {
    let t = test_parse("first day of next month").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(1, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Keyword::FirstDay), t.relative.first_last_day_of);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug51096_05() {
    let t = test_parse("last day next month").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(1, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(None, t.time.hour);
    assert_eq!(None, t.time.minute);
    assert_eq!(None, t.time.second);
}

#[test]
fn bug51096_06() {
    let t = test_parse("last day of next month").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(1, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Keyword::LastDay), t.relative.first_last_day_of);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bug54597_00() {
    let t = test_parse("January 0099").state;
    assert_eq!(Some(99), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug54597_01() {
    let t = test_parse("January 1, 0099").state;
    assert_eq!(Some(99), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug54597_02() {
    let t = test_parse("0099-1").state;
    assert_eq!(Some(99), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug54597_03() {
    let t = test_parse("0099-January").state;
    assert_eq!(Some(99), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug54597_04() {
    let t = test_parse("0099-Jan").state;
    assert_eq!(Some(99), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug54597_05() {
    let t = test_parse("January 1099").state;
    assert_eq!(Some(1099), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug54597_06() {
    let t = test_parse("January 1, 1299").state;
    assert_eq!(Some(1299), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug54597_07() {
    let t = test_parse("1599-1").state;
    assert_eq!(Some(1599), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn bug63470_00() {
    let t = test_parse("2015-07-12 00:00 this week").state;
    assert_eq!(Some(2015), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_01() {
    let t = test_parse("2015-07-12 00:00 sunday this week").state;
    assert_eq!(Some(2015), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_02() {
    let t = test_parse("2015-07-12 00:00 this week sunday").state;
    assert_eq!(Some(2015), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_03() {
    let t = test_parse("2015-07-12 00:00 sunday").state;
    assert_eq!(Some(2015), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_04() {
    let t = test_parse("2008-04-25 00:00 this week tuesday").state;
    assert_eq!(Some(2008), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Tuesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_05() {
    let t = test_parse("2008-04-25 00:00 this week sunday").state;
    assert_eq!(Some(2008), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_06() {
    let t = test_parse("Sun 2017-01-01 00:00 saturday this week").state;
    assert_eq!(Some(2017), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_07() {
    let t = test_parse("Mon 2017-01-02 00:00 saturday this week").state;
    assert_eq!(Some(2017), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_08() {
    let t = test_parse("Tue 2017-01-03 00:00 saturday this week").state;
    assert_eq!(Some(2017), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(3), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug63470_09() {
    let t = test_parse("Sun 2017-01-02 00:00 saturday this week").state;
    assert_eq!(Some(2017), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn bug74819_00() {
    let t = test_parse("I06.00am 0").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(6), t.date.day);
}

#[test]
fn bugs_00() {
    let t = test_parse("04/05/06 0045").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(5), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bugs_01() {
    let t = test_parse("17:00 2004-01-03").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(3), t.date.day);
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bugs_02() {
    let t = test_parse("2004-03-10 16:33:17.11403+01").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(10), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(33), t.time.minute);
    assert_eq!(Some(17), t.time.second);
    assert_eq!(Some(114_030), t.time.micros);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn bugs_03() {
    let t = test_parse("2004-03-10 16:33:17+01").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(10), t.date.day);
    assert_eq!(Some(16), t.time.hour);
    assert_eq!(Some(33), t.time.minute);
    assert_eq!(Some(17), t.time.second);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn bugs_04() {
    let t = test_parse("Sun, 21 Dec 2003 20:38:33 +0000 GMT").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(21), t.date.day);
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(38), t.time.minute);
    assert_eq!(Some(33), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn bugs_05() {
    let t = test_parse("2003-11-19 08:00:00 T").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(11), t.date.month);
    assert_eq!(Some(19), t.date.day);
    assert_eq!(Some(8), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("T"))), t.offset);
}

#[test]
fn bugs_06() {
    let t = test_parse("01-MAY-1982 00:00:00").state;
    assert_eq!(Some(1982), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bugs_07() {
    let t = test_parse("2040-06-12T04:32:12").state;
    assert_eq!(Some(2040), t.date.year);
    assert_eq!(Some(6), t.date.month);
    assert_eq!(Some(12), t.date.day);
    assert_eq!(Some(4), t.time.hour);
    assert_eq!(Some(32), t.time.minute);
    assert_eq!(Some(12), t.time.second);
}

#[test]
fn bugs_08() {
    let t = test_parse("july 14th").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(14), t.date.day);
}

#[test]
fn bugs_09() {
    let t = test_parse("july 14tH").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(14), t.date.day);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("H"))), t.offset);
    assert_eq!(28800, t.offset.as_ref().unwrap().offset());
}

#[test]
fn bugs_10() {
    let t = test_parse("11Oct").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(11), t.date.day);
}

#[test]
fn bugs_11() {
    let t = test_parse("11 Oct").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(11), t.date.day);
}

#[test]
fn bugs_12() {
    let t = test_parse("2005/04/05/08:15:48 last saturday").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(5), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn bugs_13() {
    let t = test_parse("2005/04/05/08:15:48 last sunday").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(5), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn bugs_14() {
    let t = test_parse("2005/04/05/08:15:48 last monday").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(5), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn bugs_15() {
    let t = test_parse("2004-04-07 00:00:00 CET -10 day +1 hour").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(7), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-10, t.relative.d);
    assert_eq!(1, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn bugs_16() {
    let t = test_parse("Jan14, 2004").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(14), t.date.day);
}

#[test]
fn bugs_17() {
    let t = test_parse("Jan 14, 2004").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(14), t.date.day);
}

#[test]
fn bugs_18() {
    let t = test_parse("Jan.14, 2004").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(14), t.date.day);
}

#[test]
fn bugs_19() {
    let t = test_parse("1999-10-13").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(13), t.date.day);
}

#[test]
fn bugs_20() {
    let t = test_parse("Oct 13  1999").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(13), t.date.day);
}

#[test]
fn bugs_21() {
    let t = test_parse("2000-01-19").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(19), t.date.day);
}

#[test]
fn bugs_22() {
    let t = test_parse("Jan 19  2000").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(19), t.date.day);
}

#[test]
fn bugs_23() {
    let t = test_parse("2001-12-21").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(21), t.date.day);
}

#[test]
fn bugs_24() {
    let t = test_parse("Dec 21  2001").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(21), t.date.day);
}

#[test]
fn bugs_25() {
    let t = test_parse("2001-12-21 12:16").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(21), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(16), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bugs_26() {
    let t = test_parse("Dec 21 2001 12:16").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(21), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(16), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bugs_27() {
    let t = test_parse("Dec 21  12:16").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(21), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(16), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn bugs_28() {
    let t = test_parse("2001-10-22 21:19:58").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(19), t.time.minute);
    assert_eq!(Some(58), t.time.second);
}

#[test]
fn bugs_29() {
    let t = test_parse("2001-10-22 21:19:58-02").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(19), t.time.minute);
    assert_eq!(Some(58), t.time.second);
    assert_eq!(Some(Timezone::Offset(-7200)), t.offset);
}

#[test]
fn bugs_30() {
    let t = test_parse("2001-10-22 21:19:58-0213").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(19), t.time.minute);
    assert_eq!(Some(58), t.time.second);
    assert_eq!(Some(Timezone::Offset(-7980)), t.offset);
}

#[test]
fn bugs_31() {
    let t = test_parse("2001-10-22 21:19:58+02").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(19), t.time.minute);
    assert_eq!(Some(58), t.time.second);
    assert_eq!(Some(Timezone::Offset(7200)), t.offset);
}

#[test]
fn bugs_32() {
    let t = test_parse("2001-10-22 21:19:58+0213").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(19), t.time.minute);
    assert_eq!(Some(58), t.time.second);
    assert_eq!(Some(Timezone::Offset(7980)), t.offset);
}

#[test]
fn bugs_33() {
    let t = test_parse("2001-10-22T21:20:58-03:40").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(20), t.time.minute);
    assert_eq!(Some(58), t.time.second);
    assert_eq!(Some(Timezone::Offset(-13200)), t.offset);
}

#[test]
fn bugs_34() {
    let t = test_parse("2001-10-22T211958-2").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(19), t.time.minute);
    assert_eq!(Some(58), t.time.second);
    assert_eq!(Some(Timezone::Offset(-7200)), t.offset);
}

#[test]
fn bugs_35() {
    let t = test_parse("20011022T211958+0213").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(19), t.time.minute);
    assert_eq!(Some(58), t.time.second);
    assert_eq!(Some(Timezone::Offset(7980)), t.offset);
}

#[test]
fn bugs_36() {
    let t = test_parse("20011022T21:20+0215").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(20), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Offset(8100)), t.offset);
}

#[test]
fn bugs_37() {
    let t = test_parse("1997W011").state;
    assert_eq!(Some(1997), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn bugs_38() {
    let t = test_parse("2004W101T05:00+0").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(5), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(60, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn bugs_39() {
    assert!(!test_parse("nextyear").errors.is_empty());
}

#[test]
fn bugs_40() {
    let t = test_parse("next year").state;
    assert_eq!(1, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn combined_00() {
    let t = test_parse("Sat, 24 Apr 2004 21:48:40 +0200").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(24), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(48), t.time.minute);
    assert_eq!(Some(40), t.time.second);
    assert_eq!(Some(Timezone::Offset(7200)), t.offset);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn combined_01() {
    let t = test_parse("Sun Apr 25 01:05:41 CEST 2004").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn combined_02() {
    let t = test_parse("Sun Apr 18 18:36:57 2004").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(18), t.date.day);
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(36), t.time.minute);
    assert_eq!(Some(57), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn combined_03() {
    let t = test_parse("Sat, 24 Apr 2004	21:48:40	+0200").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(24), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(48), t.time.minute);
    assert_eq!(Some(40), t.time.second);
    assert_eq!(Some(Timezone::Offset(7200)), t.offset);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn combined_04() {
    let t = test_parse("20040425010541 CEST").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn combined_05() {
    let t = test_parse("20040425010541").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(41), t.time.second);
}

#[test]
fn combined_06() {
    let t = test_parse("19980717T14:08:55").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(17), t.date.day);
    assert_eq!(Some(14), t.time.hour);
    assert_eq!(Some(8), t.time.minute);
    assert_eq!(Some(55), t.time.second);
}

#[test]
fn combined_07() {
    let t = test_parse("10/Oct/2000:13:55:36 -0700").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(10), t.date.day);
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(55), t.time.minute);
    assert_eq!(Some(36), t.time.second);
    assert_eq!(Some(Timezone::Offset(-25200)), t.offset);
}

#[test]
fn combined_08() {
    let t = test_parse("2001-11-29T13:20:01.123").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(11), t.date.month);
    assert_eq!(Some(29), t.date.day);
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(20), t.time.minute);
    assert_eq!(Some(1), t.time.second);
    assert_eq!(Some(123_000), t.time.micros);
}

#[test]
fn combined_09() {
    let t = test_parse("2001-11-29T13:20:01.123-05:00").state;
    assert_eq!(Some(2001), t.date.year);
    assert_eq!(Some(11), t.date.month);
    assert_eq!(Some(29), t.date.day);
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(20), t.time.minute);
    assert_eq!(Some(1), t.time.second);
    assert_eq!(Some(123_000), t.time.micros);
    assert_eq!(Some(Timezone::Offset(-18000)), t.offset);
}

#[test]
fn combined_10() {
    let t = test_parse("Fri Aug 20 11:59:59 1993 GMT").state;
    assert_eq!(Some(1993), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(20), t.date.day);
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(59), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Friday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn combined_11() {
    let t = test_parse("Fri Aug 20 11:59:59 1993 UTC").state;
    assert_eq!(Some(1993), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(20), t.date.day);
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(59), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
    assert_eq!(Some(Weekdays::Weekday(Weekday::Friday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn combined_12() {
    let t = test_parse("Fri	Aug	20	 11:59:59	 1993	UTC").state;
    assert_eq!(Some(1993), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(20), t.date.day);
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(59), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
    assert_eq!(Some(Weekdays::Weekday(Weekday::Friday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn combined_13() {
    let t = test_parse("May 18th 5:05 UTC").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(18), t.date.day);
    assert_eq!(Some(5), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
}

#[test]
fn combined_14() {
    let t = test_parse("May 18th 5:05pm UTC").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(18), t.date.day);
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
}

#[test]
fn combined_15() {
    let t = test_parse("May 18th 5:05 pm UTC").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(18), t.date.day);
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
}

#[test]
fn combined_16() {
    let t = test_parse("May 18th 5:05am UTC").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(18), t.date.day);
    assert_eq!(Some(5), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
}

#[test]
fn combined_17() {
    let t = test_parse("May 18th 5:05 am UTC").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(18), t.date.day);
    assert_eq!(Some(5), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
}

#[test]
fn combined_18() {
    let t = test_parse("May 18th 2006 5:05pm UTC").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(18), t.date.day);
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("UTC"))), t.offset);
    assert_eq!(0, t.offset.as_ref().unwrap().offset());
}

#[test]
fn common_00() {
    assert!(test_parse("now").errors.is_empty());
}

#[test]
fn common_01() {
    assert!(test_parse("NOW").errors.is_empty());
}

#[test]
fn common_02() {
    assert!(test_parse("noW").errors.is_empty());
}

#[test]
fn common_03() {
    let t = test_parse("today").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_04() {
    let t = test_parse("midnight").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_05() {
    let t = test_parse("noon").state;
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_06() {
    let t = test_parse("tomorrow").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_07() {
    let t = test_parse("yesterday 08:15pm").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(15), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_08() {
    let t = test_parse("yesterday midnight").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_09() {
    let t = test_parse("tomorrow 18:00").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_10() {
    let t = test_parse("tomorrow noon").state;
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_11() {
    let t = test_parse("TODAY").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_12() {
    let t = test_parse("MIDNIGHT").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_13() {
    let t = test_parse("NOON").state;
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_14() {
    let t = test_parse("TOMORROW").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_15() {
    let t = test_parse("YESTERDAY 08:15pm").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(15), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_16() {
    let t = test_parse("YESTERDAY MIDNIGHT").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_17() {
    let t = test_parse("TOMORROW 18:00").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_18() {
    let t = test_parse("TOMORROW NOON").state;
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_19() {
    let t = test_parse("ToDaY").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_20() {
    let t = test_parse("mIdNiGhT").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_21() {
    let t = test_parse("NooN").state;
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn common_22() {
    let t = test_parse("ToMoRRoW").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_23() {
    let t = test_parse("yEstErdAY 08:15pm").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(15), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_24() {
    let t = test_parse("yEsTeRdAY mIdNiGht").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_25() {
    let t = test_parse("toMOrrOW 18:00").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_26() {
    let t = test_parse("TOmoRRow nOOn").state;
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn common_27() {
    let t = test_parse("TOmoRRow	nOOn").state;
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(1, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn datefull_00() {
    let t = test_parse("22 dec 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_01() {
    let t = test_parse("22-dec-78").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_02() {
    let t = test_parse("22 Dec 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_03() {
    let t = test_parse("22DEC78").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_04() {
    let t = test_parse("22 december 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_05() {
    let t = test_parse("22-december-78").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_06() {
    let t = test_parse("22 December 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_07() {
    let t = test_parse("22DECEMBER78").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_08() {
    let t = test_parse("22	dec	1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_09() {
    let t = test_parse("22	Dec	1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_10() {
    let t = test_parse("22	december	1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datefull_11() {
    let t = test_parse("22	December	1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datenocolon_00() {
    let t = test_parse("19781222").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datenoday_00() {
    let t = test_parse("Oct 2003").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn datenoday_01() {
    let t = test_parse("20 October 2003").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(20), t.date.day);
}

#[test]
fn datenoday_02() {
    let t = test_parse("Oct 03").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(3), t.date.day);
}

#[test]
fn datenoday_03() {
    let t = test_parse("Oct 2003 2045").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn datenoday_04() {
    let t = test_parse("Oct 2003 20:45").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn datenoday_05() {
    let t = test_parse("Oct 2003 20:45:37").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(37), t.time.second);
}

#[test]
fn datenoday_06() {
    let t = test_parse("20 October 2003 00:00 CEST").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(20), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn datenoday_07() {
    let t = test_parse("Oct 03 21:46m").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(3), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(46), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("M"))), t.offset);
    assert_eq!(43200, t.offset.as_ref().unwrap().offset());
}

#[test]
fn datenoday_08() {
    let t = test_parse("Oct	2003	20:45").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn datenoday_09() {
    let t = test_parse("Oct	03	21:46m").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(3), t.date.day);
    assert_eq!(Some(21), t.time.hour);
    assert_eq!(Some(46), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("M"))), t.offset);
    assert_eq!(43200, t.offset.as_ref().unwrap().offset());
}

#[test]
fn date_00() {
    let t = test_parse("31.01.2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(31), t.date.day);
}

#[test]
fn date_01() {
    let t = test_parse("32.01.2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn date_02() {
    let t = test_parse("28.01.2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(28), t.date.day);
}

#[test]
fn date_03() {
    let t = test_parse("29.01.2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(29), t.date.day);
}

#[test]
fn date_04() {
    let t = test_parse("30.01.2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(30), t.date.day);
}

#[test]
fn date_05() {
    let t = test_parse("31.01.2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(31), t.date.day);
}

#[test]
fn date_06() {
    let t = test_parse("32.01.2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn date_07() {
    let t = test_parse("31-01-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(31), t.date.day);
}

#[test]
fn date_08() {
    let t = test_parse("32-01-2006").state;
    assert_eq!(Some(2032), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(20), t.date.day);
}

#[test]
fn date_09() {
    let t = test_parse("28-01-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(28), t.date.day);
}

#[test]
fn date_10() {
    let t = test_parse("29-01-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(29), t.date.day);
}

#[test]
fn date_11() {
    let t = test_parse("30-01-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(30), t.date.day);
}

#[test]
fn date_12() {
    let t = test_parse("31-01-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(31), t.date.day);
}

#[test]
fn date_13() {
    let t = test_parse("32-01-2006").state;
    assert_eq!(Some(2032), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(20), t.date.day);
}

#[test]
fn date_14() {
    let t = test_parse("29-02-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(2), t.date.month);
    assert_eq!(Some(29), t.date.day);
}

#[test]
fn date_15() {
    let t = test_parse("30-02-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(2), t.date.month);
    assert_eq!(Some(30), t.date.day);
}

#[test]
fn date_16() {
    let t = test_parse("31-02-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(2), t.date.month);
    assert_eq!(Some(31), t.date.day);
}

#[test]
fn date_17() {
    let t = test_parse("01-01-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
}

#[test]
fn date_18() {
    let t = test_parse("31-12-2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(31), t.date.day);
}

#[test]
fn date_19() {
    let t = test_parse("31-13-2006").state;
    assert_eq!(Some(Timezone::Offset(-46800)), t.offset);
}

#[test]
fn date_20() {
    let t = test_parse("11/10/2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(11), t.date.month);
    assert_eq!(Some(10), t.date.day);
}

#[test]
fn date_21() {
    let t = test_parse("12/10/2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(10), t.date.day);
}

#[test]
fn date_22() {
    let t = test_parse("13/10/2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(10), t.date.day);
}

#[test]
fn date_23() {
    let t = test_parse("14/10/2006").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(10), t.date.day);
}

#[test]
fn dateroman_00() {
    let t = test_parse("22 I 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_01() {
    let t = test_parse("22. II 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(2), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_02() {
    let t = test_parse("22 III. 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(3), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_03() {
    let t = test_parse("22- IV- 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_04() {
    let t = test_parse("22 -V -1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_05() {
    let t = test_parse("22-VI-1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(6), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_06() {
    let t = test_parse("22.VII.1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_07() {
    let t = test_parse("22 VIII 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_08() {
    let t = test_parse("22 IX 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_09() {
    let t = test_parse("22 X 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_10() {
    let t = test_parse("22 XI 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(11), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateroman_11() {
    let t = test_parse("22	XII	1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn dateslash_00() {
    let t = test_parse("2005/8/12").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(8), t.date.month);
    assert_eq!(Some(12), t.date.day);
}

#[test]
fn dateslash_01() {
    let t = test_parse("2005/01/02").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn dateslash_02() {
    let t = test_parse("2005/01/2").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn dateslash_03() {
    let t = test_parse("2005/1/02").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn dateslash_04() {
    let t = test_parse("2005/1/2").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn datetextual_00() {
    let t = test_parse("December 22, 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_01() {
    let t = test_parse("DECEMBER 22nd 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_02() {
    let t = test_parse("December 22. 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_03() {
    let t = test_parse("December 22 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_04() {
    let t = test_parse("Dec 22, 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_05() {
    let t = test_parse("DEC 22nd 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_06() {
    let t = test_parse("Dec 22. 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_07() {
    let t = test_parse("Dec 22 1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_08() {
    let t = test_parse("December 22").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_09() {
    let t = test_parse("Dec 22").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_10() {
    let t = test_parse("DEC 22nd").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_11() {
    let t = test_parse("December	22	1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn datetextual_12() {
    let t = test_parse("DEC	22nd").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn frontof_00() {
    let t = test_parse("frONt of 0 0").state;
    assert_eq!(Some(-1), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn frontof_01() {
    let t = test_parse("frONt of 4pm").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn frontof_02() {
    let t = test_parse("frONt of 4 pm").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(45), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601date_00() {
    let t = test_parse("1978-12-22").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn iso8601date_01() {
    let t = test_parse("0078-12-22").state;
    assert_eq!(Some(78), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn iso8601date_02() {
    let t = test_parse("078-12-22").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn iso8601date_03() {
    let t = test_parse("78-12-22").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn iso8601date_04() {
    let t = test_parse("4-4-25").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
}

#[test]
fn iso8601date_05() {
    let t = test_parse("69-4-25").state;
    assert_eq!(Some(2069), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
}

#[test]
fn iso8601date_06() {
    let t = test_parse("70-4-25").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(25), t.date.day);
}

#[test]
fn iso8601date_07() {
    let t = test_parse("1978/12/22").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn iso8601date_08() {
    let t = test_parse("1978/02/02").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(2), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn iso8601date_09() {
    let t = test_parse("1978/12/02").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn iso8601date_10() {
    let t = test_parse("1978/02/22").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(2), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn iso8601long_00() {
    let t = test_parse("01:00:03.12345").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
}

#[test]
fn iso8601long_01() {
    let t = test_parse("13:03:12.45678").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(456_780), t.time.micros);
}

#[test]
fn iso8601longtz_00() {
    let t = test_parse("01:00:03.12345 CET").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601longtz_01() {
    let t = test_parse("13:03:12.45678 CEST").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(456_780), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601longtz_02() {
    let t = test_parse("15:57:41.0GMT").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
}

#[test]
fn iso8601longtz_03() {
    let t = test_parse("15:57:41.0 pdt").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("PDT"))), t.offset);
    assert_eq!(-28800, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601longtz_04() {
    let t = test_parse("23:41:00.0Z").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(41), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("Z"))), t.offset);
}

#[test]
fn iso8601longtz_05() {
    let t = test_parse("23:41:00.0 k").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(41), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("K"))), t.offset);
    assert_eq!(36000, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601longtz_06() {
    let t = test_parse("04:05:07.789cast").state;
    assert_eq!(Some(4), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(7), t.time.second);
    assert_eq!(Some(789_000), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CAST"))), t.offset);
    assert_eq!(34200, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601longtz_07() {
    let t = test_parse("01:00:03.12345  +1").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601longtz_08() {
    let t = test_parse("13:03:12.45678 +0100").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(456_780), t.time.micros);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn iso8601longtz_09() {
    let t = test_parse("15:57:41.0-0").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
}

#[test]
fn iso8601longtz_10() {
    let t = test_parse("15:57:41.0-8").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(Some(Timezone::Offset(-28800)), t.offset);
}

#[test]
fn iso8601longtz_11() {
    let t = test_parse("23:41:00.0 -0000").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(41), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601longtz_12() {
    let t = test_parse("04:05:07.789 +0930").state;
    assert_eq!(Some(4), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(7), t.time.second);
    assert_eq!(Some(789_000), t.time.micros);
    assert_eq!(Some(Timezone::Offset(34200)), t.offset);
}

#[test]
fn iso8601longtz_13() {
    let t = test_parse("01:00:03.12345 (CET)").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601longtz_14() {
    let t = test_parse("13:03:12.45678 (CEST)").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(456_780), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601longtz_15() {
    let t = test_parse("(CET) 01:00:03.12345").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601longtz_16() {
    let t = test_parse("(CEST) 13:03:12.45678").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(456_780), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601longtz_17() {
    let t = test_parse("13:03:12.45678	(CEST)").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(456_780), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601longtz_18() {
    let t = test_parse("(CEST)	13:03:12.45678").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(456_780), t.time.micros);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601nocolon_00() {
    let t = test_parse("2314").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601nocolon_01() {
    let t = test_parse("2314 2314").state;
    assert_eq!(Some(2314), t.date.year);
    assert_eq!(None, t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601nocolon_02() {
    let t = test_parse("2314 PST").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("PST"))), t.offset);
    assert_eq!(-28800, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601nocolon_03() {
    let t = test_parse("231431 CEST").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(31), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601nocolon_04() {
    let t = test_parse("231431 CET").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(31), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601nocolon_05() {
    let t = test_parse("231431").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(31), t.time.second);
}

#[test]
fn iso8601nocolon_06() {
    let t = test_parse("231431 2314").state;
    assert_eq!(Some(2314), t.date.year);
    assert_eq!(None, t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(31), t.time.second);
}

#[test]
fn iso8601nocolon_07() {
    let t = test_parse("2314 231431").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601nocolon_08() {
    let t = test_parse("2314	2314").state;
    assert_eq!(Some(2314), t.date.year);
    assert_eq!(None, t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601nocolon_09() {
    let t = test_parse("2314	PST").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("PST"))), t.offset);
    assert_eq!(-28800, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601nocolon_10() {
    let t = test_parse("231431	CEST").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(31), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601nocolon_11() {
    let t = test_parse("231431	CET").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(31), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601nocolon_12() {
    let t = test_parse("231431	2314").state;
    assert_eq!(Some(2314), t.date.year);
    assert_eq!(None, t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(31), t.time.second);
}

#[test]
fn iso8601nocolon_13() {
    let t = test_parse("2314	231431").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601normtz_00() {
    let t = test_parse("01:00:03 CET").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601normtz_01() {
    let t = test_parse("13:03:12 CEST").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601normtz_02() {
    let t = test_parse("15:57:41GMT").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
}

#[test]
fn iso8601normtz_03() {
    let t = test_parse("15:57:41 pdt").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("PDT"))), t.offset);
    assert_eq!(-28800, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601normtz_04() {
    let t = test_parse("23:41:02Y").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(41), t.time.minute);
    assert_eq!(Some(2), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("Y"))), t.offset);
    assert_eq!(-43200, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601normtz_05() {
    let t = test_parse("04:05:07cast").state;
    assert_eq!(Some(4), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(7), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CAST"))), t.offset);
    assert_eq!(34200, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601normtz_06() {
    let t = test_parse("01:00:03  +1").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn iso8601normtz_07() {
    let t = test_parse("13:03:12 +0100").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn iso8601normtz_08() {
    let t = test_parse("15:57:41-0").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
}

#[test]
fn iso8601normtz_09() {
    let t = test_parse("15:57:41-8").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(Some(Timezone::Offset(-28800)), t.offset);
}

#[test]
fn iso8601normtz_10() {
    let t = test_parse("23:41:01 -0000").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(41), t.time.minute);
    assert_eq!(Some(1), t.time.second);
}

#[test]
fn iso8601normtz_11() {
    let t = test_parse("04:05:07 +0930").state;
    assert_eq!(Some(4), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(7), t.time.second);
    assert_eq!(Some(Timezone::Offset(34200)), t.offset);
}

#[test]
fn iso8601normtz_12() {
    let t = test_parse("13:03:12	CEST").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601normtz_13() {
    let t = test_parse("15:57:41	pdt").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("PDT"))), t.offset);
    assert_eq!(-28800, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601normtz_14() {
    let t = test_parse("01:00:03		+1").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn iso8601normtz_15() {
    let t = test_parse("13:03:12	+0100").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn iso8601shorttz_00() {
    let t = test_parse("01:00 CET").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CET"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601shorttz_01() {
    let t = test_parse("13:03 CEST").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601shorttz_02() {
    let t = test_parse("15:57GMT").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601shorttz_03() {
    let t = test_parse("15:57 pdt").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("PDT"))), t.offset);
    assert_eq!(-28800, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
}

#[test]
fn iso8601shorttz_04() {
    let t = test_parse("23:41F").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(41), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("F"))), t.offset);
    assert_eq!(21600, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601shorttz_05() {
    let t = test_parse("04:05cast").state;
    assert_eq!(Some(4), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CAST"))), t.offset);
    assert_eq!(34200, t.offset.as_ref().unwrap().offset());
}

#[test]
fn iso8601shorttz_06() {
    let t = test_parse("01:00  +1").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn iso8601shorttz_07() {
    let t = test_parse("13:03 +0100").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn iso8601shorttz_08() {
    let t = test_parse("15:57-0").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601shorttz_09() {
    let t = test_parse("15:57-8").state;
    assert_eq!(Some(15), t.time.hour);
    assert_eq!(Some(57), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Offset(-28800)), t.offset);
}

#[test]
fn iso8601shorttz_10() {
    let t = test_parse("23:41 -0000").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(41), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn iso8601shorttz_11() {
    let t = test_parse("04:05 +0930").state;
    assert_eq!(Some(4), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Offset(34200)), t.offset);
}

#[test]
fn last_day_of_00() {
    let t = test_parse("last saturday of feb 2008").state;
    assert_eq!(Some(2008), t.date.year);
    assert_eq!(Some(2), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
    assert_eq!(Some(Special::LastDayOfWeekInMonth), t.relative.special);
}

#[test]
fn last_day_of_01() {
    let t = test_parse("last tue of 2008-11").state;
    assert_eq!(Some(2008), t.date.year);
    assert_eq!(Some(11), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Tuesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
    assert_eq!(Some(Special::LastDayOfWeekInMonth), t.relative.special);
}

#[test]
fn last_day_of_02() {
    let t = test_parse("last sunday of sept").state;
    assert_eq!(None, t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
    assert_eq!(Some(Special::LastDayOfWeekInMonth), t.relative.special);
}

#[test]
fn last_day_of_03() {
    let t = test_parse("last saturday of this month").state;
    assert_eq!(None, t.date.year);
    assert_eq!(None, t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
    assert_eq!(Some(Special::LastDayOfWeekInMonth), t.relative.special);
}

#[test]
fn last_day_of_04() {
    let t = test_parse("last thursday of last month").state;
    assert_eq!(None, t.date.year);
    assert_eq!(None, t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(-1, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Thursday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
    assert_eq!(Some(Special::LastDayOfWeekInMonth), t.relative.special);
}

#[test]
fn last_day_of_05() {
    let t = test_parse("last wed of fourth month").state;
    assert_eq!(None, t.date.year);
    assert_eq!(None, t.date.month);
    assert_eq!(None, t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(4, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Wednesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
    assert_eq!(Some(Special::LastDayOfWeekInMonth), t.relative.special);
}

#[test]
fn microsecond_00() {
    let t = test_parse("+1 ms").state;
    assert_eq!(1000, t.relative.us);
}

#[test]
fn microsecond_01() {
    let t = test_parse("+3 msec").state;
    assert_eq!(3000, t.relative.us);
}

#[test]
fn microsecond_02() {
    let t = test_parse("+4 msecs").state;
    assert_eq!(4000, t.relative.us);
}

#[test]
fn microsecond_03() {
    let t = test_parse("+5 millisecond").state;
    assert_eq!(5000, t.relative.us);
}

#[test]
fn microsecond_04() {
    let t = test_parse("+6 milliseconds").state;
    assert_eq!(6000, t.relative.us);
}

#[test]
fn microsecond_05() {
    let t = test_parse("+1 µs").state;
    assert_eq!(1, t.relative.us);
}

#[test]
fn microsecond_06() {
    let t = test_parse("+3 usec").state;
    assert_eq!(3, t.relative.us);
}

#[test]
fn microsecond_07() {
    let t = test_parse("+4 usecs").state;
    assert_eq!(4, t.relative.us);
}

#[test]
fn microsecond_08() {
    let t = test_parse("+5 µsec").state;
    assert_eq!(5, t.relative.us);
}

#[test]
fn microsecond_09() {
    let t = test_parse("+6 µsecs").state;
    assert_eq!(6, t.relative.us);
}

#[test]
fn microsecond_10() {
    let t = test_parse("+7 microsecond").state;
    assert_eq!(7, t.relative.us);
}

#[test]
fn microsecond_11() {
    let t = test_parse("+8 microseconds").state;
    assert_eq!(8, t.relative.us);
}

#[test]
fn mysql_00() {
    let t = test_parse("19970523091528").state;
    assert_eq!(Some(1997), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(23), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(15), t.time.minute);
    assert_eq!(Some(28), t.time.second);
}

#[test]
fn mysql_01() {
    let t = test_parse("20001231185859").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(31), t.date.day);
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(58), t.time.minute);
    assert_eq!(Some(59), t.time.second);
}

#[test]
fn mysql_02() {
    let t = test_parse("20500410101010").state;
    assert_eq!(Some(2050), t.date.year);
    assert_eq!(Some(4), t.date.month);
    assert_eq!(Some(10), t.date.day);
    assert_eq!(Some(10), t.time.hour);
    assert_eq!(Some(10), t.time.minute);
    assert_eq!(Some(10), t.time.second);
}

#[test]
fn mysql_03() {
    let t = test_parse("20050620091407").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(6), t.date.month);
    assert_eq!(Some(20), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(14), t.time.minute);
    assert_eq!(Some(7), t.time.second);
}

#[test]
fn pgsql_00() {
    let t = test_parse("January 8, 1999").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_01() {
    let t = test_parse("January	8,	1999").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_02() {
    let t = test_parse("1999-01-08").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_03() {
    let t = test_parse("1/8/1999").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_04() {
    let t = test_parse("1/18/1999").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(18), t.date.day);
}

#[test]
fn pgsql_05() {
    let t = test_parse("01/02/03").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(2), t.date.day);
}

#[test]
fn pgsql_06() {
    let t = test_parse("1999-Jan-08").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_07() {
    let t = test_parse("Jan-08-1999").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_08() {
    let t = test_parse("08-Jan-1999").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_09() {
    let t = test_parse("99-Jan-08").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_10() {
    let t = test_parse("08-Jan-99").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_11() {
    let t = test_parse("Jan-08-99").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_12() {
    let t = test_parse("19990108").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_13() {
    let t = test_parse("1999.008").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_14() {
    let t = test_parse("1999.038").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(38), t.date.day);
}

#[test]
fn pgsql_15() {
    let t = test_parse("1999.238").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(238), t.date.day);
}

#[test]
fn pgsql_16() {
    let t = test_parse("1999.366").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(366), t.date.day);
}

#[test]
fn pgsql_17() {
    let t = test_parse("1999008").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_18() {
    let t = test_parse("1999038").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(38), t.date.day);
}

#[test]
fn pgsql_19() {
    let t = test_parse("1999238").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(238), t.date.day);
}

#[test]
fn pgsql_20() {
    let t = test_parse("1999366").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(366), t.date.day);
}

#[test]
fn pgsql_21() {
    let t = test_parse("1999-008").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(8), t.date.day);
}

#[test]
fn pgsql_22() {
    let t = test_parse("1999-038").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(38), t.date.day);
}

#[test]
fn pgsql_23() {
    let t = test_parse("1999-238").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(238), t.date.day);
}

#[test]
fn pgsql_24() {
    let t = test_parse("1999-366").state;
    assert_eq!(Some(1999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(366), t.date.day);
}

#[test]
fn pointeddate_00() {
    let t = test_parse("22.12.1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn pointeddate_01() {
    let t = test_parse("22.7.1978").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn pointeddate_02() {
    let t = test_parse("22.12.78").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn pointeddate_03() {
    let t = test_parse("22.7.78").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(22), t.date.day);
}

#[test]
fn relative_00() {
    let t = test_parse("2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_01() {
    let t = test_parse("+2 sec").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_02() {
    let t = test_parse("-2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_03() {
    let t = test_parse("++2 sec").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_04() {
    let t = test_parse("+-2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_05() {
    let t = test_parse("-+2 sec").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_06() {
    let t = test_parse("--2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_07() {
    let t = test_parse("+++2 sec").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_08() {
    let t = test_parse("++-2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_09() {
    let t = test_parse("+-+2 sec").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_10() {
    let t = test_parse("+--2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_11() {
    let t = test_parse("-++2 sec").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_12() {
    let t = test_parse("-+-2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_13() {
    let t = test_parse("--+2 sec").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_14() {
    let t = test_parse("---2 secs").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_15() {
    let t = test_parse("+2 sec ago").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_16() {
    let t = test_parse("2 secs ago").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-2, t.relative.s);
}

#[test]
fn relative_17() {
    assert!(test_parse("0 second").errors.is_empty());
}

#[test]
fn relative_18() {
    let t = test_parse("first second").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(1, t.relative.s);
}

#[test]
fn relative_19() {
    let t = test_parse("next second").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(1, t.relative.s);
}

#[test]
fn relative_20() {
    let t = test_parse("second second").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(2, t.relative.s);
}

#[test]
fn relative_21() {
    let t = test_parse("third second").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(3, t.relative.s);
}

#[test]
fn relative_22() {
    let t = test_parse("-3 seconds").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-3, t.relative.s);
}

#[test]
fn relative_23() {
    let t = test_parse("+2 days").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_24() {
    let t = test_parse("+2 days ago").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_25() {
    let t = test_parse("-2 days").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_26() {
    let t = test_parse("-3 fortnight").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-42, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_27() {
    let t = test_parse("+12 weeks").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(84, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_28() {
    let t = test_parse("- 3 seconds").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(-3, t.relative.s);
}

#[test]
fn relative_29() {
    let t = test_parse("+ 2 days").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_30() {
    let t = test_parse("+ 2 days ago").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_31() {
    let t = test_parse("- 2 days").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_32() {
    let t = test_parse("- 3 fortnight").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-42, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_33() {
    let t = test_parse("+ 12 weeks").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(84, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_34() {
    let t = test_parse("- 2	days").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-2, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_35() {
    let t = test_parse("-	3 fortnight").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-42, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_36() {
    let t = test_parse("+	12	weeks").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(84, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_37() {
    let t = test_parse("6 month 2004-05-05 12:15:23 CEST").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(5), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(15), t.time.minute);
    assert_eq!(Some(23), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
    assert_eq!(0, t.relative.y);
    assert_eq!(6, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_38() {
    let t = test_parse("2004-05-05 12:15:23 CEST 6 months").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(5), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(15), t.time.minute);
    assert_eq!(Some(23), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
    assert_eq!(0, t.relative.y);
    assert_eq!(6, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_39() {
    let t = test_parse("2004-05-05 12:15:23 CEST 6 months ago").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(5), t.date.day);
    assert_eq!(Some(12), t.time.hour);
    assert_eq!(Some(15), t.time.minute);
    assert_eq!(Some(23), t.time.second);
    assert_eq!(Some(Timezone::Alias(Cow::Borrowed("CEST"))), t.offset);
    assert_eq!(3600, t.offset.as_ref().unwrap().offset());
    assert!(t.dst);
    assert_eq!(0, t.relative.y);
    assert_eq!(-6, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_40() {
    let t = test_parse("6 months ago 4 days").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(-6, t.relative.m);
    assert_eq!(4, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_41() {
    let t = test_parse("first month").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(1, t.relative.m);
    assert_eq!(0, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn relative_42() {
    let t = test_parse("saturday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_43() {
    let t = test_parse("saturday ago").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Ago(Weekday::Saturday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_44() {
    let t = test_parse("this saturday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_45() {
    let t = test_parse("this saturday ago").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Ago(Weekday::Saturday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_46() {
    let t = test_parse("last saturday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_47() {
    let t = test_parse("last saturday ago").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Ago(Weekday::Saturday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_48() {
    let t = test_parse("first saturday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_49() {
    let t = test_parse("first saturday ago").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Ago(Weekday::Saturday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_50() {
    let t = test_parse("next saturday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_51() {
    let t = test_parse("next saturday ago").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Ago(Weekday::Saturday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_52() {
    let t = test_parse("third saturday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(14, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_53() {
    let t = test_parse("third saturday ago").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-14, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Ago(Weekday::Saturday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_54() {
    let t = test_parse("previous saturday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::IgnoreCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_55() {
    let t = test_parse("this weekday").state;
    assert_eq!(Some(Special::WeekdayCount(0)), t.relative.special);
}

#[test]
fn relative_56() {
    let t = test_parse("last weekday").state;
    assert_eq!(Some(Special::WeekdayCount(-1)), t.relative.special);
}

#[test]
fn relative_57() {
    let t = test_parse("next weekday").state;
    assert_eq!(Some(Special::WeekdayCount(1)), t.relative.special);
}

#[test]
fn relative_58() {
    let t = test_parse("8 weekdays ago").state;
    assert_eq!(Some(Special::WeekdayCount(-8)), t.relative.special);
    assert_eq!(None, t.time.hour);
    assert_eq!(None, t.time.minute);
    assert_eq!(None, t.time.second);
}

#[test]
fn relative_59() {
    let t = test_parse("Sun, 21 Dec 2003 20:38:33 +0000 GMT").state;
    assert_eq!(Some(2003), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(21), t.date.day);
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(38), t.time.minute);
    assert_eq!(Some(33), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn relative_60() {
    let t = test_parse("Mon, 08 May 2006 13:06:44 -0400 +30 days").state;
    assert_eq!(Some(2006), t.date.year);
    assert_eq!(Some(5), t.date.month);
    assert_eq!(Some(8), t.date.day);
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(6), t.time.minute);
    assert_eq!(Some(44), t.time.second);
    assert_eq!(Some(Timezone::Offset(-14400)), t.offset);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(30, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::CountCurrentDay,
        t.relative.weekday_behavior
    );
}

#[test]
fn special_00() {
    let t = test_parse("1998-9-15T09:05:32+4:0").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
    assert_eq!(Some(Timezone::Offset(14400)), t.offset);
}

#[test]
fn special_01() {
    let t = test_parse("1998-09-15T09:05:32+04:00").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
    assert_eq!(Some(Timezone::Offset(14400)), t.offset);
}

#[test]
fn special_02() {
    let t = test_parse("1998-09-15T09:05:32.912+04:00").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
    assert_eq!(Some(912_000), t.time.micros);
    assert_eq!(Some(Timezone::Offset(14400)), t.offset);
}

#[test]
fn special_03() {
    let t = test_parse("1998-09-15T09:05:32").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
}

#[test]
fn special_04() {
    let t = test_parse("19980915T09:05:32").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
}

#[test]
fn special_05() {
    let t = test_parse("19980915t090532").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
}

#[test]
fn special_06() {
    let t = test_parse("1998-09-15T09:05:32+4:9").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
    assert_eq!(Some(Timezone::Offset(14940)), t.offset);
}

#[test]
fn special_07() {
    let t = test_parse("1998-9-15T09:05:32+4:30").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
    assert_eq!(Some(Timezone::Offset(16200)), t.offset);
}

#[test]
fn special_08() {
    let t = test_parse("1998-09-15T09:05:32+04:9").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
    assert_eq!(Some(Timezone::Offset(14940)), t.offset);
}

#[test]
fn special_09() {
    let t = test_parse("1998-9-15T09:05:32+04:30").state;
    assert_eq!(Some(1998), t.date.year);
    assert_eq!(Some(9), t.date.month);
    assert_eq!(Some(15), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(5), t.time.minute);
    assert_eq!(Some(32), t.time.second);
    assert_eq!(Some(Timezone::Offset(16200)), t.offset);
}

#[test]
fn timelong12_00() {
    let t = test_parse("01:00:03am").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
}

#[test]
fn timelong12_01() {
    let t = test_parse("01:03:12pm").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
}

#[test]
fn timelong12_02() {
    let t = test_parse("12:31:13 A.M.").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(31), t.time.minute);
    assert_eq!(Some(13), t.time.second);
}

#[test]
fn timelong12_03() {
    let t = test_parse("08:13:14 P.M.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(13), t.time.minute);
    assert_eq!(Some(14), t.time.second);
}

#[test]
fn timelong12_04() {
    let t = test_parse("11:59:15 AM").state;
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(15), t.time.second);
}

#[test]
fn timelong12_05() {
    let t = test_parse("06:12:16 PM").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(12), t.time.minute);
    assert_eq!(Some(16), t.time.second);
}

#[test]
fn timelong12_06() {
    let t = test_parse("07:08:17 am").state;
    assert_eq!(Some(7), t.time.hour);
    assert_eq!(Some(8), t.time.minute);
    assert_eq!(Some(17), t.time.second);
}

#[test]
fn timelong12_07() {
    let t = test_parse("08:09:18 p.m.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(9), t.time.minute);
    assert_eq!(Some(18), t.time.second);
}

#[test]
fn timelong12_08() {
    let t = test_parse("01.00.03am").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
}

#[test]
fn timelong12_09() {
    let t = test_parse("01.03.12pm").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
}

#[test]
fn timelong12_10() {
    let t = test_parse("12.31.13 A.M.").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(31), t.time.minute);
    assert_eq!(Some(13), t.time.second);
}

#[test]
fn timelong12_11() {
    let t = test_parse("08.13.14 P.M.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(13), t.time.minute);
    assert_eq!(Some(14), t.time.second);
}

#[test]
fn timelong12_12() {
    let t = test_parse("11.59.15 AM").state;
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(15), t.time.second);
}

#[test]
fn timelong12_13() {
    let t = test_parse("06.12.16 PM").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(12), t.time.minute);
    assert_eq!(Some(16), t.time.second);
}

#[test]
fn timelong12_14() {
    let t = test_parse("07.08.17 am").state;
    assert_eq!(Some(7), t.time.hour);
    assert_eq!(Some(8), t.time.minute);
    assert_eq!(Some(17), t.time.second);
}

#[test]
fn timelong12_15() {
    let t = test_parse("08.09.18 p.m.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(9), t.time.minute);
    assert_eq!(Some(18), t.time.second);
}

#[test]
fn timelong12_16() {
    let t = test_parse("07.08.17	am").state;
    assert_eq!(Some(7), t.time.hour);
    assert_eq!(Some(8), t.time.minute);
    assert_eq!(Some(17), t.time.second);
}

#[test]
fn timelong12_17() {
    let t = test_parse("08.09.18	p.m.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(9), t.time.minute);
    assert_eq!(Some(18), t.time.second);
}

#[test]
fn timelong24_00() {
    let t = test_parse("01:00:03").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
}

#[test]
fn timelong24_01() {
    let t = test_parse("13:03:12").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
}

#[test]
fn timelong24_02() {
    let t = test_parse("24:03:12").state;
    assert_eq!(Some(24), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
}

#[test]
fn timelong24_03() {
    let t = test_parse("01.00.03").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
}

#[test]
fn timelong24_04() {
    let t = test_parse("13.03.12").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
}

#[test]
fn timelong24_05() {
    let t = test_parse("24.03.12").state;
    assert_eq!(Some(24), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(12), t.time.second);
}

#[test]
fn timeshort12_00() {
    let t = test_parse("01:00am").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_01() {
    let t = test_parse("01:03pm").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_02() {
    let t = test_parse("12:31 A.M.").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(31), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_03() {
    let t = test_parse("08:13 P.M.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(13), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_04() {
    let t = test_parse("11:59 AM").state;
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_05() {
    let t = test_parse("06:12 PM").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(12), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_06() {
    let t = test_parse("07:08 am").state;
    assert_eq!(Some(7), t.time.hour);
    assert_eq!(Some(8), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_07() {
    let t = test_parse("08:09 p.m.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(9), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_08() {
    let t = test_parse("01.00am").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_09() {
    let t = test_parse("01.03pm").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_10() {
    let t = test_parse("12.31 A.M.").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(31), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_11() {
    let t = test_parse("08.13 P.M.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(13), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_12() {
    let t = test_parse("11.59 AM").state;
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(59), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_13() {
    let t = test_parse("06.12 PM").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(12), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_14() {
    let t = test_parse("07.08 am").state;
    assert_eq!(Some(7), t.time.hour);
    assert_eq!(Some(8), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_15() {
    let t = test_parse("08.09 p.m.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(9), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_16() {
    let t = test_parse("07.08	am").state;
    assert_eq!(Some(7), t.time.hour);
    assert_eq!(Some(8), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort12_17() {
    let t = test_parse("08.09	p.m.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(9), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort24_00() {
    let t = test_parse("01:00").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort24_01() {
    let t = test_parse("13:03").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort24_02() {
    let t = test_parse("01.00").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timeshort24_03() {
    let t = test_parse("13.03").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(3), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timestamp_00() {
    let t = test_parse("@1508765076.3").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(300_000, t.relative.us);
}

#[test]
fn timestamp_01() {
    let t = test_parse("@1508765076.34").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(340_000, t.relative.us);
}

#[test]
fn timestamp_02() {
    let t = test_parse("@1508765076.347").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(347_000, t.relative.us);
}

#[test]
fn timestamp_03() {
    let t = test_parse("@1508765076.3479").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(347_900, t.relative.us);
}

#[test]
fn timestamp_04() {
    let t = test_parse("@1508765076.34795").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(347_950, t.relative.us);
}

#[test]
fn timestamp_05() {
    let t = test_parse("@1508765076.347958").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(347_958, t.relative.us);
}

#[test]
fn timestamp_06() {
    let t = test_parse("@1508765076.003").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(3000, t.relative.us);
}

#[test]
fn timestamp_07() {
    let t = test_parse("@1508765076.0003").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(300, t.relative.us);
}

#[test]
fn timestamp_08() {
    let t = test_parse("@1508765076.00003").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(30, t.relative.us);
}

#[test]
fn timestamp_09() {
    let t = test_parse("@1508765076.000003").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(1_508_765_076, t.relative.s);
    assert_eq!(3, t.relative.us);
}

#[test]
fn php_gh_7758() {
    let t = test_parse("@-0.4").state;
    assert_eq!(Some(1970), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.s);
    assert_eq!(-400_000, t.relative.us);
}

#[test]
fn timetiny12_00() {
    let t = test_parse("01am").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_01() {
    let t = test_parse("01pm").state;
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_02() {
    let t = test_parse("12 A.M.").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_03() {
    let t = test_parse("08 P.M.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_04() {
    let t = test_parse("11 AM").state;
    assert_eq!(Some(11), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_05() {
    let t = test_parse("06 PM").state;
    assert_eq!(Some(18), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_06() {
    let t = test_parse("07 am").state;
    assert_eq!(Some(7), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_07() {
    let t = test_parse("08 p.m.").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_08() {
    let t = test_parse("09	am").state;
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny12_09() {
    let t = test_parse("10	p.m.").state;
    assert_eq!(Some(22), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn tzcorrection_00() {
    let t = test_parse("+4:30").state;
    assert_eq!(Some(Timezone::Offset(16200)), t.offset);
}

#[test]
fn tzcorrection_01() {
    let t = test_parse("+4").state;
    assert_eq!(Some(Timezone::Offset(14400)), t.offset);
}

#[test]
fn tzcorrection_02() {
    let t = test_parse("+1").state;
    assert_eq!(Some(Timezone::Offset(3600)), t.offset);
}

#[test]
fn tzcorrection_03() {
    let t = test_parse("+14").state;
    assert_eq!(Some(Timezone::Offset(50400)), t.offset);
}

#[test]
fn tzcorrection_04() {
    let t = test_parse("+42").state;
    assert_eq!(Some(Timezone::Offset(151_200)), t.offset);
}

#[test]
fn tzcorrection_05() {
    let t = test_parse("+4:0").state;
    assert_eq!(Some(Timezone::Offset(14400)), t.offset);
}

#[test]
fn tzcorrection_06() {
    let t = test_parse("+4:01").state;
    assert_eq!(Some(Timezone::Offset(14460)), t.offset);
}

#[test]
fn tzcorrection_07() {
    let t = test_parse("+4:30").state;
    assert_eq!(Some(Timezone::Offset(16200)), t.offset);
}

#[test]
fn tzcorrection_08() {
    let t = test_parse("+401").state;
    assert_eq!(Some(Timezone::Offset(14460)), t.offset);
}

#[test]
fn tzcorrection_09() {
    let t = test_parse("+402").state;
    assert_eq!(Some(Timezone::Offset(14520)), t.offset);
}

#[test]
fn tzcorrection_10() {
    let t = test_parse("+430").state;
    assert_eq!(Some(Timezone::Offset(16200)), t.offset);
}

#[test]
fn tzcorrection_11() {
    let t = test_parse("+0430").state;
    assert_eq!(Some(Timezone::Offset(16200)), t.offset);
}

#[test]
fn tzcorrection_12() {
    let t = test_parse("+04:30").state;
    assert_eq!(Some(Timezone::Offset(16200)), t.offset);
}

#[test]
fn tzcorrection_13() {
    let t = test_parse("+04:9").state;
    assert_eq!(Some(Timezone::Offset(14940)), t.offset);
}

#[test]
fn tzcorrection_14() {
    let t = test_parse("+04:09").state;
    assert_eq!(Some(Timezone::Offset(14940)), t.offset);
}

#[test]
fn tzcorrection_15() {
    let t = test_parse("+040915").state;
    assert_eq!(Some(Timezone::Offset(14955)), t.offset);
}

#[test]
fn tzcorrection_16() {
    let t = test_parse("-040916").state;
    assert_eq!(Some(Timezone::Offset(-14956)), t.offset);
}

#[test]
fn tzcorrection_17() {
    let t = test_parse("+04:09:15").state;
    assert_eq!(Some(Timezone::Offset(14955)), t.offset);
}

#[test]
fn tzcorrection_18() {
    let t = test_parse("-04:09:25").state;
    assert_eq!(Some(Timezone::Offset(-14965)), t.offset);
}

#[test]
fn tz_identifier_00() {
    let t = test_parse("01:00:03.12345 Europe/Amsterdam").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("Europe/Amsterdam")))
    );
}

#[test]
fn tz_identifier_01() {
    let t = test_parse("01:00:03.12345 America/Indiana/Knox").state;
    assert_eq!(Some(1), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(3), t.time.second);
    assert_eq!(Some(123_450), t.time.micros);
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/Indiana/Knox")))
    );
}

#[test]
fn tz_identifier_02() {
    let t = test_parse("2005-07-14 22:30:41 America/Los_Angeles").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(14), t.date.day);
    assert_eq!(Some(22), t.time.hour);
    assert_eq!(Some(30), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/Los_Angeles")))
    );
}

#[test]
fn tz_identifier_03() {
    let t = test_parse("2005-07-14	22:30:41	America/Los_Angeles").state;
    assert_eq!(Some(2005), t.date.year);
    assert_eq!(Some(7), t.date.month);
    assert_eq!(Some(14), t.date.day);
    assert_eq!(Some(22), t.time.hour);
    assert_eq!(Some(30), t.time.minute);
    assert_eq!(Some(41), t.time.second);
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/Los_Angeles")))
    );
}

#[test]
fn tz_identifier_04() {
    let t = test_parse("Africa/Dar_es_Salaam").state;
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("Africa/Dar_es_Salaam")))
    );
}

#[test]
fn tz_identifier_05() {
    let t = test_parse("Africa/Porto-Novo").state;
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("Africa/Porto-Novo")))
    );
}

#[test]
fn tz_identifier_06() {
    let t = test_parse("America/Blanc-Sablon").state;
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/Blanc-Sablon")))
    );
}

#[test]
fn tz_identifier_07() {
    let t = test_parse("America/Port-au-Prince").state;
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/Port-au-Prince")))
    );
}

#[test]
fn tz_identifier_08() {
    let t = test_parse("America/Port_of_Spain").state;
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("America/Port_of_Spain")))
    );
}

#[test]
fn tz_identifier_09() {
    let t = test_parse("Antarctica/DumontDUrville").state;
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("Antarctica/DumontDUrville")))
    );
}

#[test]
fn tz_identifier_10() {
    let t = test_parse("Antarctica/McMurdo").state;
    assert_eq!(
        t.offset,
        Some(Timezone::Named(Cow::Borrowed("Antarctica/McMurdo")))
    );
}

#[test]
fn weeknr_00() {
    let t = test_parse("1995W051").state;
    assert_eq!(Some(1995), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(29, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn weeknr_01() {
    let t = test_parse("2004W30").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(200, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn weeknr_02() {
    let t = test_parse("1995-W051").state;
    assert_eq!(Some(1995), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(29, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn weeknr_03() {
    let t = test_parse("2004-W30").state;
    assert_eq!(Some(2004), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(200, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn weeknr_04() {
    let t = test_parse("1995W05-1").state;
    assert_eq!(Some(1995), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(29, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn weeknr_05() {
    let t = test_parse("1995-W05-1").state;
    assert_eq!(Some(1995), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(29, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
}

#[test]
fn week_00() {
    let t = test_parse("this week").state;
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_01() {
    let t = test_parse("this week monday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_02() {
    let t = test_parse("this week tuesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Tuesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_03() {
    let t = test_parse("this week wednesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Wednesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_04() {
    let t = test_parse("thursday this week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Thursday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_05() {
    let t = test_parse("friday this week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Friday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_06() {
    let t = test_parse("saturday this week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_07() {
    let t = test_parse("sunday this week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_08() {
    let t = test_parse("last week").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_09() {
    let t = test_parse("last week monday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_10() {
    let t = test_parse("last week tuesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Tuesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_11() {
    let t = test_parse("last week wednesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Wednesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_12() {
    let t = test_parse("thursday last week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Thursday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_13() {
    let t = test_parse("friday last week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Friday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_14() {
    let t = test_parse("saturday last week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_15() {
    let t = test_parse("sunday last week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_16() {
    let t = test_parse("previous week").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_17() {
    let t = test_parse("previous week monday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_18() {
    let t = test_parse("previous week tuesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Tuesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_19() {
    let t = test_parse("previous week wednesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Wednesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_20() {
    let t = test_parse("thursday previous week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Thursday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_21() {
    let t = test_parse("friday previous week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Friday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_22() {
    let t = test_parse("saturday previous week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_23() {
    let t = test_parse("sunday previous week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(-7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_24() {
    let t = test_parse("next week").state;
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_25() {
    let t = test_parse("next week monday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Monday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_26() {
    let t = test_parse("next week tuesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Tuesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_27() {
    let t = test_parse("next week wednesday").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Wednesday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_28() {
    let t = test_parse("thursday next week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Thursday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_29() {
    let t = test_parse("friday next week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Friday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_30() {
    let t = test_parse("saturday next week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(
        Some(Weekdays::Weekday(Weekday::Saturday)),
        t.relative.weekday
    );
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn week_31() {
    let t = test_parse("sunday next week").state;
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(0, t.relative.y);
    assert_eq!(0, t.relative.m);
    assert_eq!(7, t.relative.d);
    assert_eq!(0, t.relative.h);
    assert_eq!(0, t.relative.i);
    assert_eq!(0, t.relative.s);
    assert_eq!(Some(Weekdays::Weekday(Weekday::Sunday)), t.relative.weekday);
    assert_eq!(
        WeekdayBehavior::RelativeTextWeek,
        t.relative.weekday_behavior
    );
}

#[test]
fn year_long_00() {
    let t = test_parse("+10000-01-01T00:00:00").state;
    assert_eq!(Some(10000), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_01() {
    let t = test_parse("+99999-01-01T00:00:00").state;
    assert_eq!(Some(99999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_02() {
    let t = test_parse("+100000-01-01T00:00:00").state;
    assert_eq!(Some(100_000), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_03() {
    let t = test_parse("+4294967296-01-01T00:00:00").state;
    assert_eq!(Some(4_294_967_296), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_04() {
    let t = test_parse("+9223372036854775807-01-01T00:00:00").state;
    assert_eq!(Some(9_223_372_036_854_775_807), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_05() {
    let t = test_parse("-10000-01-01T00:00:00").state;
    assert_eq!(Some(-10000), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_06() {
    let t = test_parse("-99999-01-01T00:00:00").state;
    assert_eq!(Some(-99999), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_07() {
    let t = test_parse("-100000-01-01T00:00:00").state;
    assert_eq!(Some(-100_000), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_08() {
    let t = test_parse("-4294967296-01-01T00:00:00").state;
    assert_eq!(Some(-4_294_967_296), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn year_long_09() {
    let t = test_parse("-9223372036854775807-01-01T00:00:00").state;
    assert_eq!(Some(-9_223_372_036_854_775_807), t.date.year);
    assert_eq!(Some(1), t.date.month);
    assert_eq!(Some(1), t.date.day);
    assert_eq!(Some(0), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny24_00() {
    let t = test_parse("1978-12-22T23").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny24_01() {
    let t = test_parse("T9").state;
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny24_02() {
    let t = test_parse("T23Z").state;
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny24_03() {
    let t = test_parse("1978-12-22T9").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny24_04() {
    let t = test_parse("1978-12-22T23Z").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(22), t.date.day);
    assert_eq!(Some(23), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn timetiny24_05() {
    let t = test_parse("1978-12-03T09-03").state;
    assert_eq!(Some(1978), t.date.year);
    assert_eq!(Some(12), t.date.month);
    assert_eq!(Some(3), t.date.day);
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Offset(-10800)), t.offset);
}

#[test]
fn timetiny24_06() {
    let t = test_parse("T09-03").state;
    assert_eq!(Some(9), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
    assert_eq!(Some(Timezone::Offset(-10800)), t.offset);
}

// Clippy: the wrapping is the whole point
#[allow(clippy::cast_possible_wrap)]
#[test]
fn gh_124a() {
    let t = test_parse("@-9223372036854775808").state;
    assert_eq!(0x8000_0000_0000_0000_u64 as i64, t.relative.s);
}

#[test]
fn ozfuzz_27360() {
    let t = test_parse("@10000000000000000000 2SEC");
    assert!(!t.errors.is_empty());
    // assert_eq!(errors->error_messages[0].error_code, TIMELIB_ERR_NUMBER_OUT_OF_RANGE);
}

#[test]
fn ozfuzz_33011() {
    let t = test_parse("@21641666666666669708sun");
    assert!(!t.errors.is_empty());
    // assert_eq!(errors->error_messages[0].error_code, TIMELIB_ERR_NUMBER_OUT_OF_RANGE);
}

#[test]
fn ozfuzz_55330() {
    let t = test_parse("@-25666666666666663653");
    assert!(!t.errors.is_empty());
    // assert_eq!(errors->error_messages[0].error_code, TIMELIB_ERR_NUMBER_OUT_OF_RANGE);
}

#[test]
fn icu_nnbsp_timetiny12() {
    let t = test_parse("8\u{202f}pm").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(0), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn icu_nnbsp_timeshort12_01() {
    let t = test_parse("8:43\u{202f}pm").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(43), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn icu_nnbsp_timeshort12_02() {
    let t = test_parse("8:43\u{202f}\u{202f}pm").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(43), t.time.minute);
    assert_eq!(Some(0), t.time.second);
}

#[test]
fn icu_nnbsp_timelong12() {
    let t = test_parse("8:43.43\u{202f}pm").state;
    assert_eq!(Some(20), t.time.hour);
    assert_eq!(Some(43), t.time.minute);
    assert_eq!(Some(43), t.time.second);
}

#[test]
fn icu_nnbsp_iso8601normtz_00() {
    let t = test_parse("T17:21:49GMT+0230").state;
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(21), t.time.minute);
    assert_eq!(Some(49), t.time.second);
    assert_eq!(Some(Timezone::Offset(9000)), t.offset);
}

#[test]
fn icu_nnbsp_iso8601normtz_01() {
    let t = test_parse("T17:21:49\u{202f}GMT+0230").state;
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(21), t.time.minute);
    assert_eq!(Some(49), t.time.second);
    assert_eq!(Some(Timezone::Offset(9000)), t.offset);
}

#[test]
fn icu_nnbsp_iso8601normtz_02() {
    let t = test_parse("T17:21:49\u{202f}\u{202f}GMT+0230").state;
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(21), t.time.minute);
    assert_eq!(Some(49), t.time.second);
    assert_eq!(Some(Timezone::Offset(9000)), t.offset);
}

#[test]
fn icu_nnbsp_iso8601normtz_03() {
    let t = test_parse("T17:21:49\u{00a0}GMT+0230").state;
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(21), t.time.minute);
    assert_eq!(Some(49), t.time.second);
    assert_eq!(Some(Timezone::Offset(9000)), t.offset);
}

#[test]
fn icu_nnbsp_iso8601normtz_04() {
    let t = test_parse("T17:21:49\u{202f}\u{00a0}GMT+0230").state;
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(21), t.time.minute);
    assert_eq!(Some(49), t.time.second);
    assert_eq!(Some(Timezone::Offset(9000)), t.offset);
}

#[test]
fn icu_nnbsp_iso8601normtz_05() {
    let t = test_parse("T17:21:49\u{00a0}\u{202f}GMT+0230").state;
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(21), t.time.minute);
    assert_eq!(Some(49), t.time.second);
    assert_eq!(Some(Timezone::Offset(9000)), t.offset);
}

#[test]
fn icu_nnbsp_iso8601normtz_06() {
    let t = test_parse("T17:21:49\u{00a0}\u{00a0}GMT+0230").state;
    assert_eq!(Some(17), t.time.hour);
    assert_eq!(Some(21), t.time.minute);
    assert_eq!(Some(49), t.time.second);
    assert_eq!(Some(Timezone::Offset(9000)), t.offset);
}

#[test]
fn icu_nnbsp_clf_01() {
    let t = test_parse("10/Oct/2000:13:55:36\u{202f}-0230").state;
    assert_eq!(Some(2000), t.date.year);
    assert_eq!(Some(10), t.date.month);
    assert_eq!(Some(10), t.date.day);
    assert_eq!(Some(13), t.time.hour);
    assert_eq!(Some(55), t.time.minute);
    assert_eq!(Some(36), t.time.second);
    assert_eq!(Some(Timezone::Offset(-9000)), t.offset);
}

#[test]
fn cf1() {
    let _ = test_parse("@9223372036854775807 9sec");
}
