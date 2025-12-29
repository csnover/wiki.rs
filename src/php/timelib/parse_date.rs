//! A parser for PHP `strtotime` format date strings.

// This code is adapted from timelib <https://github.com/derickr/timelib>.
// The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-Copyright-Text: Copyright (c) 2015-2023 Derick Rethans
// SPDX-Copyright-Text: Copyright (c) 2018 MongoDB, Inc.

use super::{
    DateTimeBuilder, Hour24, Keyword, Month, Special, TimelibDate, TimelibTime, Timezone, Weekday,
    WeekdayBehavior, Weekdays,
};
use std::borrow::Cow;
use time::Date;

/// A parser error.
pub(super) type PegError = peg::error::ParseError<peg::str::LineCol>;

/// The result of a parse.
pub(super) struct ParseResult<'a> {
    /// A builder containing the raw values from the parsed string.
    pub(super) builder: DateTimeBuilder<'a>,
    /// A list of errors that occurred during parsing. Note that this will
    /// always be zero or one errors, except when running unit tests.
    pub(super) errors: Vec<PegError>,
}

/// Tries to parse a string into a date time builder.
pub(super) fn parse(mut text: &str) -> ParseResult<'_> {
    let mut builder = DateTimeBuilder::default();
    let mut errors = Vec::new();
    loop {
        match strtotime::parse(text, &mut builder) {
            Ok(Some((next_state, rest))) => {
                builder = next_state;
                text = rest;
            }
            Ok(None) => break,
            // The timelib unit tests require continuing to try to parse after
            // failure, but PHP throws if there is any error, so no need to
            // bother having more than one error if we are not under test since
            // the only thing we care about at runtime is the public PHP API
            // behaviour
            Err(error) => {
                if cfg!(test) {
                    text = &text[1..];
                    errors.push(error);
                } else {
                    errors.push(error);
                    break;
                }
            }
        }
    }

    ParseResult { builder, errors }
}

peg::parser! {grammar strtotime() for str {
    rule i(lit: &'static str)
        = quiet!{
            input:$([_]*<{lit.chars().count()}>)
            {? if input.eq_ignore_ascii_case(lit) { Ok(()) } else { Err(lit) } }
        } / expected!(lit)
    rule with_slice<T>(r: rule<T>) -> (T, &'input str)
        = value:&r() input:$(r()) { (value, input) }
    rule eof()   = quiet!{![_]} / expected!("end of input")
    rule any()   = quiet!{[_]} / expected!("any character")
    rule upper() = quiet!{['A'..='Z']} / expected!("uppercase letter")
    rule lower() = quiet!{['a'..='z']} / expected!("lowercase letter")
    rule alpha() = quiet!{upper() / lower()} / expected!("letter")
    rule digit() = quiet!{['0'..='9']} / expected!("digit")
    rule digitnz() = quiet!{['1'..='9']} / expected!("non-zero digit")
    rule sign() -> char = quiet!{['+'|'-']} / expected!("sign")

    rule _ = quiet!{[' '|'\t'|'\u{00a0}'|'\u{202f}']+}
    rule frac() -> u32
        = quiet!{"." f:$(digit()+) { to_micros(f) }}
        / expected!("time fraction")

    rule hour24lz() -> Hour24
        = quiet!{
            h:$(['0'|'1'] digit() / "2" ['0'..='4'])
            { Hour24(h.parse().unwrap()) }
        } / expected!("24-hour hour with leading zero")
    #[cache]
    rule hour24() -> Hour24
        = quiet!{
            h:$(hour24lz() / digit())
            { Hour24(h.parse().unwrap()) }
        } / expected!("24-hour hour")
    #[cache]
    rule hour12() -> Hour12
        = quiet!{
            h:$("1" ['0'..='2'] / "0" digitnz() / digitnz())
            { Hour12(h.parse().unwrap()) }
        } / expected!("12-hour hour")
    #[cache]
    rule minutelz() -> u8
        = quiet!{
            m:$(['0'..='5'] digit())
            { m.parse().unwrap() }
        } / expected!("minute with leading zero")
    rule minute() -> u8 = quiet!{
            m:$(minutelz() / digit())
            { m.parse().unwrap() }
        } / expected!("minute")
    rule second() -> u8
        = quiet!{
            minute() / "60" { 60 }
        } / expected!("second")
    #[cache]
    rule secondlz() -> u8
        = quiet!{
            minutelz() / "60" { 60 }
        } / expected!("second with leading zero")

    rule meridian() -> Period
        = quiet!{
            p:(['A'|'a'] { Period::Am } / ['P'|'p'] { Period::Pm }) "."? ['M'|'m'] "."? (eof() / ['\t'|' '])
            { p }
        } / expected!("meridian")

    rule tz() -> Timezone<'input>
        = quiet!{
            n:$(upper() lower()+ (['_'|'/'|'-'] alpha()+)+)
            {? static_tz(n).map(Timezone::Named) }
            / "("? n:$(alpha()*<1,6>) ")"?
            {
                let value = if n.as_bytes().iter().all(u8::is_ascii_uppercase) {
                    Cow::Borrowed(n)
                } else {
                    Cow::Owned(n.to_ascii_uppercase())
                };
                Timezone::Alias(value)
            }
        } / expected!("named time zone")

    rule tzcorrection() -> Timezone<'static>
        = quiet!{
            "GMT"? s:sign()
            o:(h:hour24lz() ":"? i:minutelz() ":"? s:secondlz() { (h.0, i, s) }
                // timelib allows insane corrections like +42, which is an
                // ambiguous parse. The only upstream test for this terminates
                // at eof but presumably any transition applies
                / h:$(digit() digit()) !(":" / digit() / _) { (h.parse().unwrap(), 0, 0) }
                / h:hour24() ":"? i:minute()? { (h.0, i.unwrap_or(0), 0) })
            {
                let signum = if s == '-' { -1 } else { 1 };
                let (h, i, s) = o;
                Timezone::Offset(signum * (i32::from(h) * 3600 + i32::from(i) * 60 + i32::from(s)))
            }
        } / expected!("offset time zone")

    rule daysuf() = quiet!{"st" / "nd" / "rd" / "th"}
    rule daylz() -> u8
        = quiet!{
            d:$(['0'..='2'] digit() / "3" ['0'|'1'])
            { d.parse().unwrap() }
        } / expected!("day of month with leading zero")
    #[cache]
    rule day() -> u8
        = quiet!{
            d:$(daylz() / digit()) daysuf()?
            { d.parse().unwrap() }
        } / expected!("day of month")

    rule monthlz() -> u8
        = quiet!{
            m:$("0" digit() / "1" ['0'..='2'])
            { m.parse::<u8>().unwrap() }
        } / expected!("month with leading zero")
    rule month() -> u8
        = quiet!{
            m:$(monthlz() / digit())
            { m.parse::<u8>().unwrap() }
        } / expected!("month")

    #[cache]
    rule year() -> i32
        = quiet!{
            y:$(digit()*<1,4>)
            {
                let year = y.parse::<i32>().unwrap();
                year + if y.len() == 4 {
                    0
                } else if year < 70 {
                    2000
                } else {
                    1900
                }
            }
        } / expected!("year")
    rule year2() -> i32
        = quiet!{
            y:$(digit()*<2,2>)
            {
                let y = y.parse::<i32>().unwrap();
                y + if y < 70 { 2000 } else { 1900 }
            }
        } / expected!("2-digit year")
    #[cache]
    rule year4() -> i32
        = quiet!{
            y:$(digit()*<4,4>)
            { y.parse().unwrap() }
        } / expected!("4-digit year")
    rule year4withsign() -> i32
        = quiet!{
            y:$(sign()? digit()*<4,4>)
            { y.parse().unwrap() }
        } / expected!("4-digit year with optional sign")
    rule yearx() -> i64
        = quiet!{
            y:$(sign() digit()*<5,19>)
            { y.parse().unwrap() }
        } / expected!("extended year")

    rule dayofyear() -> u16
        = quiet!{
            doy:$(
                "00" digitnz()
                / "0" digitnz() digit()
                / ['1'|'2'] digit() digit()
                / "3" ['0'..='5'] digit()
                / "36" ['0'..='6']
            )
            { doy.parse().unwrap() }
        } / expected!("day of year (0-366)")

    rule weekofyear() -> u8
        = quiet!{
            woy:$(
                "0" digitnz()
                / ['1'..='4'] digit()
                / "5" ['1'..='3']
            )
            { woy.parse().unwrap() }
        } / expected!("week of year (0-53)")

    rule dayfull() -> Weekday
        = i("sunday")    { Weekday::Sunday }
        / i("monday")    { Weekday::Monday }
        / i("tuesday")   { Weekday::Tuesday }
        / i("wednesday") { Weekday::Wednesday }
        / i("thursday")  { Weekday::Thursday }
        / i("friday")    { Weekday::Friday }
        / i("saturday")  { Weekday::Saturday }
    rule dayfulls() -> Weekday = d:dayfull() "s"? { d }
    rule dayabbr() -> Weekday
        = i("sun") { Weekday::Sunday }
        / i("mon") { Weekday::Monday }
        / i("tue") { Weekday::Tuesday }
        / i("wed") { Weekday::Wednesday }
        / i("thu") { Weekday::Thursday }
        / i("fri") { Weekday::Friday }
        / i("sat") { Weekday::Saturday }
    rule dayspecial() -> Weekdays
        = i("weekday") i("s")? { Weekdays::All }
    rule daytext() -> Weekdays
        = d:(dayfulls() / dayabbr()) { Weekdays::Weekday(d) }
        / dayspecial()

    rule monthfull() -> Month
        = quiet!{
            i("january")   { Month::January }
            / i("february")  { Month::February }
            / i("march")     { Month::March }
            / i("april")     { Month::April }
            / i("may")       { Month::May }
            / i("june")      { Month::June }
            / i("july")      { Month::July }
            / i("august")    { Month::August }
            / i("september") { Month::September }
            / i("october")   { Month::October }
            / i("november")  { Month::November }
            / i("december")  { Month::December }
        } / expected!("full month name")
    rule monthabbr() -> Month
        = quiet!{
            i("jan")  { Month::January }
            / i("feb")  { Month::February }
            / i("mar")  { Month::March }
            / i("apr")  { Month::April }
            / i("may")  { Month::May }
            / i("jun")  { Month::June }
            / i("jul")  { Month::July }
            / i("aug")  { Month::August }
            / i("sep") i("t")? { Month::September }
            / i("oct")  { Month::October }
            / i("nov")  { Month::November }
            / i("dec")  { Month::December }
        } / expected!("abbreviated month name")
    rule monthroman() -> Month
        = quiet!{
            "III"  { Month::March }
            / "II"   { Month::February }
            / "IV"   { Month::April }
            / "IX"   { Month::September }
            / "I"    { Month::January }
            / "VIII" { Month::August }
            / "VII"  { Month::July }
            / "VI"   { Month::June }
            / "V"    { Month::May }
            / "XII"  { Month::December }
            / "XI"   { Month::November }
            / "X"    { Month::October }
        } / expected!("roman numeral month name")
    #[cache]
    rule monthtext() -> Month = monthfull() / monthabbr() / monthroman()

    rule timesep() = quiet!{[':'|'.']} / expected!("time separator")

    rule timetiny12() -> TimelibTime
        = h:hour12() _? p:meridian()
        { (p.to_24h(h), Some(0), Some(0)).into() }
    rule timeshort12() -> TimelibTime
        = h:hour12() timesep() i:minutelz() _? p:meridian()
        { (p.to_24h(h), Some(i), Some(0)).into() }
    rule timelong12() -> TimelibTime
        = h:hour12() timesep() i:minute() timesep() s:secondlz() _? p:meridian()
        { (p.to_24h(h), Some(i), Some(s)).into() }

    rule timetiny24() -> TimelibTime
        = i("t") h:hour24()
        { (h, Some(0), Some(0), None).into() }
    rule timeshort24() -> TimelibTime
        = i("t")? h:hour24() timesep() i:minute()
        { (h, Some(i), Some(0), None).into() }
    rule timelong24() -> TimelibTime
        = i("t")? h:hour24() timesep() i:minute() timesep()
            // timelong24 and pointeddate2 parses are ambiguous. Disambiguate by
            // rejecting longer matches that are not valid seconds, since
            // timelong24 is preferred over pointeddate2 for other ambiguous
            // parses
            !("6" digitnz() / ['7'..='9'] digit()) s:second()
        { (h, Some(i), Some(s), None).into() }
    rule iso8601long() -> TimelibTime
        = i("t")? h:hour24() timesep() i:minute() timesep() s:second() f:frac()
        { (h, Some(i), Some(s), Some(f)).into() }

    rule iso8601normtz() -> (TimelibTime, Timezone<'input>)
        = h:hour24() ":" i:minute() ":" s:secondlz() n:frac() _? z:(tzcorrection() / tz())
        { ((h, i, s, n).into(), z) }

    rule gnunocolon() -> TimelibTime
        = i("t")? h:hour24lz() i:minutelz()
        { (h, Some(i), Some(0)).into() }
    rule iso8601nocolon() -> TimelibTime
        = i("t")? h:hour24lz() i:minutelz() s:secondlz()
        { (h, i, s).into() }

    rule dh() = quiet!{['.'|'-']} / expected!("dot or hyphen")
    rule dt() = quiet!{['.'|'\t']} / expected!("dot or tab")
    rule dht() = quiet!{dt() / "-"} / expected!("dot, hyphen, or tab")
    rule dhts() = quiet!{dht() / " "} / expected!("dot, hyphen, or whitespace")
    rule dtssuffix()
        = quiet!{[','|'.'|'s'|'t'|'n'|'d'|'r'|'h'|'\t'|' ']}
        / expected!(r#"comma, dot, "s", "t", "n", "d", "r", "h", or whitespace"#)

    rule american() -> TimelibDate
        = m:month() "/" d:day() y:("/" y:year() { y })?
        { (y, m, d).into() }
    rule iso8601dateslash() -> TimelibDate
        = y:year4() "/" m:monthlz() "/" d:daylz() "/"?
        { (y, m, d).into() }
    rule dateslash() -> TimelibDate
        = y:year4() "/" m:month() "/" d:day()
        { (y, m, d).into() }
    rule iso8601date4() -> TimelibDate
        = y:year4withsign() "-" m:monthlz() "-" d:daylz()
        { (y, m, d).into() }
    rule iso8601date2() -> TimelibDate
        = y:year2() "-" m:monthlz() "-" d:daylz()
        { (y, m, d).into() }
    rule iso8601datex() -> TimelibDate
        = y:yearx() "-" m:monthlz() "-" d:daylz()
        { (y, m, d).into() }
    rule gnudateshorter() -> TimelibDate
        = y:year4() "-" m:month()
        { (y, m, 1_u8).into() }
    rule gnudateshort() -> TimelibDate
        = y:year() "-" m:month() "-" d:day()
        { (y, m, d).into() }
    rule pointeddate4() -> TimelibDate
        = d:day() dht() m:month() dh() y:year4()
        { (y, m, d).into() }
    rule pointeddate2() -> TimelibDate
        = d:day() dt() m:month() "." y:year2()
        { (y, m, d).into() }
    rule datefull() -> TimelibDate
        = d:day() dhts()* m:monthtext() dhts()* y:year()
        { (y, m, d).into() }
    rule datenoday() -> TimelibDate
        = m:monthtext() dhts()* y:year4()
        { (y, m, 1).into() }
    rule datenodayrev() -> TimelibDate
        = y:year4() dhts()* m:monthtext()
        { (y, m, 1).into() }
    rule datetextual() -> TimelibDate
        = m:monthtext() dhts()* d:day() dtssuffix()+ y:year()
        { (y, m, d).into() }
    rule datenoyear() -> TimelibDate
        = m:monthtext() dhts()* d:day() (dtssuffix()+ / eof())
        { (None, m, d).into() }
    rule datenoyearrev() -> TimelibDate
        = d:day() dhts()* m:monthtext()
        { (None, m, d).into() }
    rule datenocolon() -> TimelibDate
        = y:year4() m:monthlz() d:daylz()
        { (y, m, d).into() }

    rule soap() -> TimelibDateTime<'input>
        = y:year4() "-" m:monthlz() "-" d:daylz() "T"
          h:hour24lz() ":" i:minutelz() ":" s:secondlz() f:frac()
          z:tzcorrection()?
        { TimelibDateTime {
            date: (y, m, d).into(),
            time: (h, i, s, f).into(),
            offset: z
        } }
    rule xmlrpc() -> TimelibDateTime<'input>
        = y:year4() m:monthlz() d:daylz() "T" h:hour24() ":" i:minutelz() ":" s:secondlz()
        { TimelibDateTime {
            date: (y, m, d).into(),
            time: (h, i, s).into(),
            offset: None
        } }
    rule xmlrpcnocolon() -> TimelibDateTime<'input>
        = y:year4() m:monthlz() d:daylz() i("t") h:hour24() i:minutelz() s:secondlz()
        { TimelibDateTime {
            date: (y, m, d).into(),
            time: (h, i, s).into(),
            offset: None
        } }
    rule wddx() -> TimelibDateTime<'input>
        = y:year4() "-" m:month() "-" d:day() "T" h:hour24() ":" i:minute() ":" s:second()
        { TimelibDateTime {
            date: (y, m, d).into(),
            time: (h, i, s).into(),
            offset: None,
        } }
    rule pgydotd() -> TimelibDate
        = y:year4() dh()? doy:dayofyear()
        { (y, 1, doy).into() }
    rule pgtextshort() -> TimelibDate
        = m:monthabbr() "-" d:daylz() "-" y:year()
        { (y, m, d).into() }
    rule pgtextreverse() -> TimelibDate
        = y:year() "-" m:monthabbr() "-" d:daylz()
        { (y, m, d).into() }
    rule mssqltime() -> TimelibTime
        = h:hour12() ":" i:minutelz() ":" s:secondlz() [':'|'.'] f:$(digit()+) p:meridian()
        { (p.to_24h(h), i, s, to_micros(f)).into() }
    rule isoweekday() -> (i32, u8, u8)
        = y:year4() "-"? "W" woy:weekofyear() date:("-"? date:$(['0'..='7']) { date })?
        { (y, woy, date.map_or(1, |date| date.parse().unwrap())) }
    rule exif() -> TimelibDateTime<'input>
        = y:year4() ":" m:monthlz() ":" d:daylz() " " h:hour24lz() ":" i:minutelz() ":" s:secondlz()
        { TimelibDateTime {
            date: (y, m, d).into(),
            time: (h, i, s).into(),
            offset: None
        } }

    rule dayof() -> Keyword
        = kw:(i("first") { Keyword::FirstDay } / i("last") { Keyword::LastDay }) i(" day of")
        { kw }

    rule backof() -> TimelibTime
        = front_of:(i("back") { false } / i("front") { true }) i(" of ")
            h:hour24() p:(_? p:meridian() { p })?
        {
            let hour = i64::from(p.map_or(h, |p| p.to_24h(Hour12(h.0)))) - i64::from(front_of);
            (hour, 45, 0).into()
        }

    rule clf() -> TimelibDateTime<'input>
        = d:day() "/" m:monthabbr() "/" y:year4()
          ":" h:hour24lz() ":" i:minutelz() ":" s:secondlz() _ z:tzcorrection()
        { TimelibDateTime {
            date: (y, m, d).into(),
            time: (h, i, s).into(),
            offset: Some(z),
        } }

    rule timestamp() -> (i64, Option<i64>)
        = "@" ts:$("-"? digit()+) us:("." us:$(digit()*<0,6>) { us })?
        {?
            let signum = if matches!(ts.bytes().next(), Some(b'-')) { -1 } else { 1 };
            Ok((
                ts.parse::<i64>().map_err(|_| "in-range number")?,
                us.map(|us| i64::from(to_micros(us)) * signum)
            ))
        }

    rule dateshortwithtime() -> TimelibDateTime<'input>
        = date:datenoyear()
          time_tz:(
            t:timelong12()      { (t, None) }
            / t:timeshort12()   { (t, None) }
            / t:iso8601normtz() { (t.0, Some(t.1)) }
            / t:timelong24()    { (t, None) }
            / t:timeshort24()   { (t, None) }
          )
        {
            let (time, offset) = time_tz;
            TimelibDateTime { date, time, offset }
        }

    rule reltextnumber() -> i8
        = i("first")    { 1 }
        / i("second")   { 2 }
        / i("third")    { 3 }
        / i("fourth")   { 4 }
        / i("fifth")    { 5 }
        / i("sixth")    { 6 }
        / i("seventh")  { 7 }
        / i("eight") i("h")? { 8 }
        / i("ninth")    { 9 }
        / i("tenth")    { 10 }
        / i("eleventh") { 11 }
        / i("twelfth")  { 12 }

    rule reltexttext() -> i8
        = i("next")                   { 1 }
        / (i("last") / i("previous")) { -1 }
        / i("this")                   { 0 }

    rule reltextunit() -> RelativeUnit
        = quiet!{
            i("ms")                                { RelativeUnit::Microsecond(1000) }
            / i("µs")                                { RelativeUnit::Microsecond(1) }
            / u:(
                u:( (i("msec") / i("millisecond"))   { RelativeUnit::Microsecond(1000) }
                    / (i("µsec")
                        / i("microsecond")
                        / i("usec"))                 { RelativeUnit::Microsecond(1) }
                    / (i("sec") i("ond")?)           { RelativeUnit::Second(1) }
                    / (i("min") i("ute")?)           { RelativeUnit::Minute(1) }
                    / i("hour")                      { RelativeUnit::Hour(1) }
                    / i("day")                       { RelativeUnit::Day(1) }
                    / (i("fort") i("h")? i("night")) { RelativeUnit::Day(14) }
                    / i("month")                     { RelativeUnit::Month(1) }
                    / i("year")                      { RelativeUnit::Year(1) }
                ) i("s")? { u }
            ) { u }
            / i("weeks")                             { RelativeUnit::Day(7) }
            / d:daytext()                            { RelativeUnit::Weekday(1, d) }
        } / expected!("time unit")

    rule relnumber() -> i64
        = s:sign()* [' '|'\t']* n:$(digit()*<1,13>)
        {
            let negs = s.into_iter().filter(|s| *s == '-').count() % 2;
            n.parse::<i64>().unwrap() * if negs != 0 { -1 } else { 1 }
        }

    rule relative() -> RelativeUnit
        = n:relnumber() _? u:(reltextunit() / i("week") { RelativeUnit::Day(7) })
        { u * n }

    rule relativetext() -> (RelativeUnit, WeekdayBehavior)
        = r:(reltextnumber() / reltexttext()) _ u:reltextunit()
        { (u * r.into(), if r == 0 {
                WeekdayBehavior::CountCurrentDay
            } else {
                WeekdayBehavior::IgnoreCurrentDay
            }
        ) }

    rule relativetextweek() -> RelativeUnit
        = r:reltexttext() _ i("week")
        { RelativeUnit::Day(7) * r.into() }

    rule weekdayof() -> (i8, Weekday)
        = r:(reltextnumber() / reltexttext()) _ d:(dayfulls() / dayabbr()) _ i("of")
        { (r, d) }

    rule main(state: &mut DateTimeBuilder<'input>) -> DateTimeBuilder<'input>
        = i("yesterday") {
            let mut state = state.clone();
            state.unhave_time();
            state.relative.d = -1;
            state
        }
        / i("now") {
            state.clone()
        }
        / i("noon") {
            let mut state = state.clone();
            state.have_time = true;
            state.time = (Hour24(12), 0, 0).into();
            state
        }
        / (i("midnight") / i("today")) {
            let mut state = state.clone();
            state.unhave_time();
            state
        }
        / i("tomorrow") {
            let mut state = state.clone();
            state.unhave_time();
            state.relative.d = 1;
            state
        }
        / ts:timestamp() {?
            let (s, us) = ts;
            let mut state = state.clone();
            state.have_date()?;
            state.have_time()?;
            state.have_tz()?;
            state.date = (1970_i32, 1_u8, 1_u8).into();
            state.time = (0, 0, 0).into();
            state.relative.s += s;
            state.offset = Some(Timezone::Offset(0));
            if let Some(us) = us {
                state.relative.us = us;
            }
            Ok(state)
        }
        / kw:dayof() {
            let mut state = state.clone();
            state.unhave_time();
            state.relative.first_last_day_of = Some(kw);
            state
        }

        / time:backof()
        {
            let mut state = state.clone();
            state.have_time = true;
            state.time = time;
            state
        }

        / r:weekdayof() {?
            let mut state = state.clone();
            let (nth, weekday) = r;

            // TODO: This is wacky. If RelativeUnit were simply stored directly,
            // the logic later would be very simple.
            state.set_relative(RelativeUnit::Weekday(nth.into(), Weekdays::Weekday(weekday)), false)?;
            state.relative.special = Some(if nth > 0 {
                Special::NthDayOfWeekInMonth
            } else {
                Special::LastDayOfWeekInMonth
            });
            // This information would be encoded by the RelativeUnit.
            state.relative.weekday_behavior = if nth == -1 {
                WeekdayBehavior::IgnoreCurrentDay
            } else {
                WeekdayBehavior::CountCurrentDay
            };
            Ok(state)
        }
        / date_time:(clf() / soap() / xmlrpc() / xmlrpcnocolon() / wddx() / exif()
            / dateshortwithtime())
        {?
            let mut state = state.clone();
            state.have_time()?;
            state.have_date()?;
            state.date = date_time.date;
            state.time = date_time.time;
            state.offset = date_time.offset;
            Ok(state)
        }
        / time:(mssqltime() / timelong12() / timeshort12() / timetiny12())
        {?
            let mut state = state.clone();
            state.have_time()?;
            state.time = time;
            Ok(state)
        }
        / date:(american() / iso8601date4() / iso8601dateslash() / dateslash()
            / pointeddate4() / iso8601date2() / iso8601datex() / datenocolon() / pgydotd()
            / gnudateshort() / gnudateshorter()
            / datefull()
            / datetextual() / pgtextreverse() / pgtextshort()
            / datenoyear() / datenoday()
            / datenodayrev() / datenoyearrev())
        {?
            let mut state = state.clone();
            state.have_date()?;
            state.date = date;
            Ok(state)
        }
        / date:isoweekday()
        {?
            let (year, woy, date) = date;
            let mut state = state.clone();
            state.have_date()?;
            state.date = (year, 1_u8, 1_u8).into();
            let (year, woy, date) = match (date, woy) {
                (0, 0) => (year - 1, time::util::weeks_in_year(year - 1), Weekday::Sunday),
                (0, _) => (year, woy - 1, Weekday::Sunday),
                (_, _) => (year, woy, Weekday::Monday.nth_next(date - 1))
            };
            let (dy, dd) = Date::from_iso_week_date(year, woy, date).unwrap().to_ordinal_date();
            state.relative.d = (dd - 1).into();
            if dy != year {
                state.relative.d -= i64::from(time::util::days_in_year(dy));
            }
            Ok(state)
        }
        / time:(iso8601long() / iso8601nocolon() / timelong24())
        {?
            let mut state = state.clone();
            state.have_time()?;
            state.time = time;
            Ok(state)
        }
        / date:pointeddate2()
        {?
            let mut state = state.clone();
            state.have_date()?;
            state.date = date;
            Ok(state)
        }
        / time:(timeshort24() / timetiny24())
        {?
            let mut state = state.clone();
            state.have_time()?;
            state.time = time;
            Ok(state)
        }
        / time:gnunocolon() {?
            let mut state = state.clone();
            state.have_time()?;
            state.time = time;
            Ok(state)
        }
        // This will match `+tztz` but should be ignored if there is already a
        // year. (ts_from_string::strange_04)
        / year:year4() {
            let mut state = state.clone();
            state.date.year.get_or_insert(year.into());
            state
        }
        / i("ago") {
            let mut state = state.clone();
            state.relative.y = -state.relative.y;
            state.relative.m = -state.relative.m;
            state.relative.d = -state.relative.d;
            state.relative.h = -state.relative.h;
            state.relative.i = -state.relative.i;
            state.relative.s = -state.relative.s;
            state.relative.weekday = state.relative.weekday.map(|w| match w {
                Weekdays::Weekday(weekday) => {
                    Weekdays::Ago(weekday)
                }
                Weekdays::Ago(weekday) => {
                    Weekdays::Weekday(weekday)
                }
                Weekdays::All => Weekdays::All,
            });
            if let Some(Special::WeekdayCount(amount)) = &mut state.relative.special {
                *amount = -*amount;
            }
            state
        }
        // TODO: {weekday} or just "weekday". So this is just like 0th {weekday}
        // except maybe the string is "this week ... {weekday}".
        / r:daytext() {
            let mut state = state.clone();
            state.unhave_time();
            state.relative.weekday = Some(r);
            if state.relative.weekday_behavior != WeekdayBehavior::RelativeTextWeek {
                state.relative.weekday_behavior = WeekdayBehavior::CountCurrentDay;
            }
            state
        }
        // TODO: {this/last/next} {unit}. weekday behaviour is coded by this/last/next.
        / r:relativetext() {?
            let (r, behavior) = r;
            let mut state = state.clone();
            state.set_relative(r, false)?;
            if matches!(r, RelativeUnit::Weekday(..)) {
                state.relative.weekday_behavior = behavior;
            }
            Ok(state)
        }
        // TODO: {this/last/next} "week". weekday behaviour is coded specially
        // by the "week" keyword.
        / r:relativetextweek() {?
            let mut state = state.clone();
            state.set_relative(r, false)?;
            state.relative.weekday_behavior = WeekdayBehavior::RelativeTextWeek;
            if state.relative.weekday.is_none() {
                state.relative.weekday = Some(Weekdays::Weekday(Weekday::Monday));
            }
            Ok(state)
        }
        // TODO: {#} {unit}. weekday behaviour is overridden because only
        // "this" is IgnoreCurrentDay and "this" never happens in this rule, but
        // again that flag is only relevant for a weekday unit
        / r:relative() {?
            let mut state = state.clone();
            state.set_relative(r, true)?;
            state.relative.weekday_behavior = WeekdayBehavior::CountCurrentDay;
            Ok(state)
        }
        / month:(monthfull() / monthabbr()) {?
            let mut state = state.clone();
            state.have_date()?;
            state.date.month = Some(u8::from(month).into());
            Ok(state)
        }
        / tz:(tzcorrection() / tz()) {?
            let mut state = state.clone();
            state.have_tz()?;
            state.offset = Some(tz);
            Ok(state)
        }
        / (['.'|','|'\n'] / _) { state.clone() }

        pub rule parse(state: &mut DateTimeBuilder<'input>) -> Option<(DateTimeBuilder<'input>, &'input str)>
            = eof() { None }
            / next_state:main(state) rest:$([_]*) { Some((next_state, rest)) }
}}

/// Newtype for 12-hour hours, to avoid type confusion.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Hour12(u8);

/// Specifier for time period.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Period {
    /// Morning.
    Am,
    /// Afternoon.
    Pm,
}

impl Period {
    /// Converts a 12-hour in this time period into a 24-hour hour.
    fn to_24h(self, hour: Hour12) -> Hour24 {
        Hour24(match self {
            Period::Am if hour.0 == 12 => 0,
            Period::Pm if hour.0 != 12 => hour.0 + 12,
            _ => hour.0,
        })
    }
}

/// Specifier for a relative unit of time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RelativeUnit {
    /// Microseconds.
    Microsecond(i64),
    /// Seconds.
    Second(i64),
    /// Minutes.
    Minute(i64),
    /// Hours.
    Hour(i64),
    /// Days.
    Day(i64),
    /// Months.
    Month(i64),
    /// Years.
    Year(i64),
    /// The same as Days(7) + a Weekday, but with extra work.
    Weekday(i64, Weekdays),
}

impl core::ops::Mul<i64> for RelativeUnit {
    type Output = RelativeUnit;

    fn mul(self, rhs: i64) -> Self::Output {
        match self {
            RelativeUnit::Microsecond(lhs) => RelativeUnit::Microsecond(lhs * rhs),
            RelativeUnit::Second(lhs) => RelativeUnit::Second(lhs * rhs),
            RelativeUnit::Minute(lhs) => RelativeUnit::Minute(lhs * rhs),
            RelativeUnit::Hour(lhs) => RelativeUnit::Hour(lhs * rhs),
            RelativeUnit::Day(lhs) => RelativeUnit::Day(lhs * rhs),
            RelativeUnit::Month(lhs) => RelativeUnit::Month(lhs * rhs),
            RelativeUnit::Year(lhs) => RelativeUnit::Year(lhs * rhs),
            RelativeUnit::Weekday(lhs, weekday) => RelativeUnit::Weekday(lhs * rhs, weekday),
        }
    }
}

/// A combined date, time, and time zone. Used to keep the grammar tidier.
#[derive(Clone, Debug, Default)]
struct TimelibDateTime<'a> {
    /// The date.
    date: TimelibDate,
    /// The time.
    time: TimelibTime,
    /// The time zone.
    offset: Option<Timezone<'a>>,
}

impl DateTimeBuilder<'_> {
    /// Marks the builder as having a valid date specification.
    fn have_date(&mut self) -> Result<(), &'static str> {
        if self.have_date {
            Err("double date specification")
        } else {
            self.have_date = true;
            Ok(())
        }
    }

    /// Marks the builder as having a valid time specification.
    fn have_time(&mut self) -> Result<(), &'static str> {
        if self.have_time {
            Err("double time specification")
        } else {
            self.have_time = true;
            Ok(())
        }
    }

    /// Marks the builder as having a valid time zone specification.
    fn have_tz(&mut self) -> Result<(), &'static str> {
        if self.have_zone {
            Err("double timezone specification")
        } else {
            self.have_zone = true;
            Ok(())
        }
    }

    /// Appends a relative unit of time to the builder.
    fn set_relative(&mut self, rel: RelativeUnit, keep_time: bool) -> Result<(), &'static str> {
        match rel {
            RelativeUnit::Microsecond(us) => add_checked(&mut self.relative.us, us)?,
            RelativeUnit::Second(s) => add_checked(&mut self.relative.s, s)?,
            RelativeUnit::Minute(i) => add_checked(&mut self.relative.i, i)?,
            RelativeUnit::Hour(h) => add_checked(&mut self.relative.h, h)?,
            RelativeUnit::Day(d) => add_checked(&mut self.relative.d, d)?,
            RelativeUnit::Month(m) => add_checked(&mut self.relative.m, m)?,
            RelativeUnit::Year(y) => add_checked(&mut self.relative.y, y)?,
            RelativeUnit::Weekday(n, weekdays) => {
                if !keep_time {
                    self.unhave_time();
                }
                match weekdays {
                    Weekdays::Weekday(_) | Weekdays::Ago(_) => {
                        // n = -1 => "last"; n = 0 => "this"; n > 0 => Nth.
                        // If today is Monday, "first tuesday" is tomorrow, not
                        // 7 days from tomorrow
                        self.relative.d += if n.is_positive() { n - 1 } else { n } * 7;
                        self.relative.weekday = Some(weekdays);
                    }
                    Weekdays::All => {
                        self.relative.special = Some(Special::WeekdayCount(n));
                    }
                }
            }
        }
        Ok(())
    }

    /// Removes a time specification from the builder.
    fn unhave_time(&mut self) {
        self.have_time = false;
        self.time = TimelibTime {
            hour: Some(0),
            minute: Some(0),
            second: Some(0),
            micros: Some(0),
        };
    }
}

/// Checked integer addition.
#[inline]
fn add_checked(lhs: &mut i64, rhs: i64) -> Result<(), &'static str> {
    *lhs = lhs.checked_add(rhs).ok_or("non-overflowing number")?;
    Ok(())
}

/// Replaces a parsed named time zone with a static string from the timezone
/// database.
fn static_tz(parsed: &str) -> Result<&'static str, &'static str> {
    tzdb_data::TZ_NAMES
        .binary_search_by(|name| {
            name.bytes()
                .map(|b| b.to_ascii_lowercase())
                .cmp(parsed.bytes().map(|b| b.to_ascii_lowercase()))
        })
        .map(|index| tzdb_data::TZ_NAMES[index])
        .map_err(|_| "valid timezone name")
}

/// Converts the decimal part of a numeric string into a microseconds integer.
// Clippy: Always in range 0..6
#[allow(clippy::cast_possible_truncation)]
#[inline]
fn to_micros(s: &str) -> u32 {
    const USEC_DIGITS: usize = 6;
    if s.is_empty() {
        0
    } else {
        let limit = USEC_DIGITS.min(s.len());
        let s = &s[0..limit];
        s.parse::<u32>().unwrap() * 10_u32.pow((USEC_DIGITS - limit) as u32)
    }
}

// All of these conversions exist just to keep the parser grammar tidier.
impl From<(Option<i32>, u8, u8)> for TimelibDate {
    #[inline]
    fn from((year, month, day): (Option<i32>, u8, u8)) -> Self {
        Self {
            year: year.map(Into::into),
            month: Some(month.into()),
            day: Some(day.into()),
        }
    }
}

impl From<(i64, u8, u8)> for TimelibDate {
    #[inline]
    fn from((year, month, day): (i64, u8, u8)) -> Self {
        Self {
            year: Some(year),
            month: Some(month.into()),
            day: Some(day.into()),
        }
    }
}

impl From<(i32, u8, u8)> for TimelibDate {
    #[inline]
    fn from((year, month, day): (i32, u8, u8)) -> Self {
        Self {
            year: Some(year.into()),
            month: Some(month.into()),
            day: Some(day.into()),
        }
    }
}

impl From<(i32, u8, u16)> for TimelibDate {
    #[inline]
    fn from((year, month, day): (i32, u8, u16)) -> Self {
        Self {
            year: Some(year.into()),
            month: Some(month.into()),
            day: Some(day.into()),
        }
    }
}

impl From<(Option<i32>, Month, u8)> for TimelibDate {
    #[inline]
    fn from((year, month, day): (Option<i32>, Month, u8)) -> Self {
        Self {
            year: year.map(Into::into),
            month: Some(u8::from(month).into()),
            day: Some(day.into()),
        }
    }
}

impl From<(i32, Month, u8)> for TimelibDate {
    #[inline]
    fn from((year, month, day): (i32, Month, u8)) -> Self {
        Self {
            year: Some(year.into()),
            month: Some(u8::from(month).into()),
            day: Some(day.into()),
        }
    }
}

impl From<(i64, u8, u8)> for TimelibTime {
    #[inline]
    fn from((hour, minute, second): (i64, u8, u8)) -> Self {
        Self {
            hour: Some(hour),
            minute: Some(minute.into()),
            second: Some(second.into()),
            micros: None,
        }
    }
}

impl From<(Hour24, u8, u8)> for TimelibTime {
    #[inline]
    fn from((hour, minute, second): (Hour24, u8, u8)) -> Self {
        Self {
            hour: Some(hour.0.into()),
            minute: Some(minute.into()),
            second: Some(second.into()),
            micros: None,
        }
    }
}

impl From<(Hour24, u8, u8, u32)> for TimelibTime {
    #[inline]
    fn from((hour, minute, second, micros): (Hour24, u8, u8, u32)) -> Self {
        Self {
            hour: Some(hour.0.into()),
            minute: Some(minute.into()),
            second: Some(second.into()),
            micros: Some(micros.into()),
        }
    }
}

impl From<(Hour24, u8, u8, Option<u32>)> for TimelibTime {
    #[inline]
    fn from((hour, minute, second, micros): (Hour24, u8, u8, Option<u32>)) -> Self {
        Self {
            hour: Some(hour.0.into()),
            minute: Some(minute.into()),
            second: Some(second.into()),
            micros: micros.map(Into::into),
        }
    }
}

impl From<(Hour24, Option<u8>, Option<u8>)> for TimelibTime {
    #[inline]
    fn from((hour, minute, second): (Hour24, Option<u8>, Option<u8>)) -> Self {
        Self {
            hour: Some(hour.0.into()),
            minute: minute.map(Into::into),
            second: second.map(Into::into),
            micros: None,
        }
    }
}

impl From<(Hour24, Option<u8>, Option<u8>, Option<u32>)> for TimelibTime {
    #[inline]
    fn from((hour, minute, second, micros): (Hour24, Option<u8>, Option<u8>, Option<u32>)) -> Self {
        Self {
            hour: Some(hour.0.into()),
            minute: minute.map(Into::into),
            second: second.map(Into::into),
            micros: micros.map(Into::into),
        }
    }
}
