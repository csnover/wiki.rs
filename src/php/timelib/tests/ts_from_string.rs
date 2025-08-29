// This code is adapted from timelib <https://github.com/derickr/timelib>.
// The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-Copyright-Text: Copyright (c) 2015-2023 Derick Rethans
// SPDX-Copyright-Text: Copyright (c) 2018 MongoDB, Inc.

use super::super::{Timezone, parse_date::parse};

#[track_caller]
fn run_test(expected: i64, source: &str, reference_date: &str, reference_tz: &str) {
    let t = parse(source).builder;
    let mut now = parse(reference_date).builder;
    if now.offset.is_none() && !reference_tz.is_empty() {
        now.offset = Some(Timezone::Named(reference_tz.into()));
    }
    let dt = t.build(Some(now)).unwrap();
    let actual = dt.unix_timestamp();
    assert_eq!(expected, actual);
}

macro_rules! make_test {
    ($fn_name:ident, $expected:literal, $source:literal, $reference_date:literal, $reference_tz:literal) => {
        #[test]
        fn $fn_name() {
            run_test($expected, $source, $reference_date, $reference_tz);
        }
    };
}

/* from bug24910.ts */
make_test!(
    bug24910_00,
    1_076_824_799,
    "2004-04-07 00:00:00 -2 months +7 days +23 hours +59 minutes +59 seconds",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_01,
    1_076_824_800,
    "2004-04-07 00:00:00 -2 months +7 days +23 hours +59 minutes +60 seconds",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_02,
    1_076_824_801,
    "2004-04-07 00:00:00 -2 months +7 days +23 hours +59 minutes +61 seconds",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_03,
    1_079_503_200,
    "2004-04-07 00:00:00 -21 days",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_04,
    1_080_367_200,
    "2004-04-07 00:00:00 11 days ago",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_05,
    1_080_460_800,
    "2004-04-07 00:00:00 -10 day +2 hours",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_06,
    1_081_227_600,
    "2004-04-07 00:00:00 -1 day",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_07,
    1_081_314_000,
    "2004-04-07 00:00:00",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_08,
    1_081_317_600,
    "2004-04-07 00:00:00 +1 hour",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_09,
    1_081_321_200,
    "2004-04-07 00:00:00 +2 hour",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_10,
    1_081_400_400,
    "2004-04-07 00:00:00 +1 day",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_11,
    1_081_400_400,
    "2004-04-07 00:00:00 1 day",
    "",
    "America/Chicago"
); // Bug #27780
make_test!(
    bug24910_12,
    1_083_128_400,
    "2004-04-07 00:00:00 +21 days",
    "",
    "America/Chicago"
); // Bug #27780

make_test!(bug24910_13, 1_080_432_000, "2004-03-28 00:00:00", "", "GMT");
make_test!(
    bug24910_14,
    1_080_428_400,
    "2004-03-28 00:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_15,
    1_080_432_000,
    "2004-03-28 01:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_16,
    1_080_435_540,
    "2004-03-28 01:59:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_17,
    1_080_435_600,
    "2004-03-28 02:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_18,
    1_080_435_660,
    "2004-03-28 02:01:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_19,
    1_080_435_600,
    "2004-03-28 03:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_20,
    1_080_435_660,
    "2004-03-28 03:01:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_21,
    1_080_428_400,
    "2004-04-07 00:00:00 -10 day",
    "",
    "Europe/Amsterdam"
); // Bug #27780
make_test!(
    bug24910_22,
    1_080_432_000,
    "2004-04-07 00:00:00 -10 day +1 hour",
    "",
    "Europe/Amsterdam"
); // Bug #27780
make_test!(
    bug24910_23,
    1_080_435_600,
    "2004-04-07 00:00:00 -10 day +2 hours",
    "",
    "Europe/Amsterdam"
); // Bug #27780

make_test!(
    bug24910_24,
    1_130_626_800,
    "2005-10-30 01:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_25,
    1_130_634_000,
    "2005-10-30 02:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_26,
    1_130_637_600,
    "2005-10-30 03:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    bug24910_27,
    1_130_641_200,
    "2005-10-30 04:00:00",
    "",
    "Europe/Amsterdam"
);

make_test!(
    bug24910_28,
    1_081_288_800,
    "2004-04-07 00:00:00",
    "",
    "Asia/Jerusalem"
);
make_test!(
    bug24910_29,
    1_081_292_400,
    "2004-04-07 00:00:00 +1 hour",
    "",
    "Asia/Jerusalem"
);

/* from bug28024.ts */
make_test!(
    bug28024_00,
    1_072_976_400,
    "17:00 2004-01-01",
    "",
    "Europe/London"
); // Bug #28024

/* from bug30190.ts */
make_test!(bug30190_00, 946_684_800, "2000-01-01", "00:00:00 GMT", "");
make_test!(bug30190_01, 946_598_400, "2000-01-00", "00:00:00 GMT", "");
make_test!(bug30190_02, 943_920_000, "2000-00-00", "00:00:00 GMT", "");
make_test!(
    bug30190_03,
    -62_167_219_200,
    "0000-01-01",
    "00:00:00 GMT",
    ""
);
make_test!(
    bug30190_04,
    -62_167_305_600,
    "0000-01-00",
    "00:00:00 GMT",
    ""
);
make_test!(
    bug30190_05,
    -62_169_984_000,
    "0000-00-00",
    "00:00:00 GMT",
    ""
);

/* from bug30532.ts */
make_test!(
    bug30532_00,
    1_099_195_200,
    "2004-10-31 +0 hours",
    "00:00:00",
    "America/New_York"
);
make_test!(
    bug30532_01,
    1_099_198_800,
    "2004-10-31 +1 hours",
    "00:00:00",
    "America/New_York"
);
make_test!(
    bug30532_02,
    1_099_206_000,
    "2004-10-31 +2 hours",
    "00:00:00",
    "America/New_York"
);
make_test!(
    bug30532_03,
    1_099_195_200,
    "+0 hours",
    "2004-10-31 00:00:00",
    "America/New_York"
);
make_test!(
    bug30532_04,
    1_099_198_800,
    "+1 hours",
    "2004-10-31 00:00:00",
    "America/New_York"
);
make_test!(
    bug30532_05,
    1_099_206_000,
    "+2 hours",
    "2004-10-31 00:00:00",
    "America/New_York"
);

/* from bug32086.ts */
make_test!(
    bug32086_00,
    1_099_278_000,
    "2004-11-01 00:00",
    "",
    "America/Sao_Paulo"
); // Bug #32086
make_test!(
    bug32086_01,
    1_099_360_800,
    "2004-11-01 23:00",
    "",
    "America/Sao_Paulo"
); // Bug #32086
make_test!(
    bug32086_02,
    1_099_364_400,
    "2004-11-01 00:00 +1 day",
    "",
    "America/Sao_Paulo"
); // Bug #32086
make_test!(
    bug32086_03,
    1_099_364_400,
    "2004-11-02 00:00",
    "",
    "America/Sao_Paulo"
); // Doesn't really exist
make_test!(
    bug32086_04,
    1_099_364_400,
    "2004-11-02 01:00",
    "",
    "America/Sao_Paulo"
); // Bug #32086
make_test!(
    bug32086_05,
    1_108_778_400,
    "2005-02-19 00:00",
    "",
    "America/Sao_Paulo"
); // Bug #32086
make_test!(
    bug32086_06,
    1_108_868_400,
    "2005-02-19 00:00 +1 day",
    "",
    "America/Sao_Paulo"
); // Bug #32086
make_test!(
    bug32086_07,
    1_108_868_400,
    "2005-02-20 00:00",
    "",
    "America/Sao_Paulo"
); // Bug #32086

/* from bug32270.ts */
make_test!(
    bug32270_00,
    -2_145_888_000,
    "01/01/1902 00:00:00",
    "",
    "America/Los_Angeles"
); // Bug #32270
make_test!(
    bug32270_01,
    -631_123_200,
    "01/01/1950 00:00:00",
    "",
    "America/Los_Angeles"
); // Bug #32270
make_test!(
    bug32270_02,
    946_713_600,
    "Sat 01 Jan 2000 08:00:00 AM GMT",
    "",
    ""
); // Bug #32270
make_test!(bug32270_03, 946_713_600, "01/01/2000 08:00:00 GMT", "", ""); // Bug #32270
make_test!(
    bug32270_04,
    946_713_600,
    "01/01/2000 00:00:00",
    "",
    "America/Los_Angeles"
); // Bug #32270
make_test!(
    bug32270_05,
    946_713_600,
    "01/01/2000 00:00:00 PST",
    "",
    "America/Los_Angeles"
); // Bug #32270

/* from bug32555.ts */
make_test!(
    bug32555_00,
    1_112_427_000,
    "2005-04-02 02:30",
    "",
    "America/New_York"
);
make_test!(
    bug32555_01,
    1_112_427_000,
    "2005-04-02 02:30 now",
    "",
    "America/New_York"
);
make_test!(
    bug32555_02,
    1_112_418_000,
    "2005-04-02 02:30 today",
    "",
    "America/New_York"
);
make_test!(
    bug32555_03,
    1_112_504_400,
    "2005-04-02 02:30 tomorrow",
    "",
    "America/New_York"
);

/* from bug32588.ts */
make_test!(
    bug32588_00,
    1_112_400_000,
    "last saturday",
    "2005/04/05/08:15:48 GMT",
    ""
); // Bug #32588
make_test!(
    bug32588_01,
    1_112_400_000,
    "0 secs",
    "2005/04/02/00:00:00 GMT",
    ""
); // Bug #32588

make_test!(
    bug32588_02,
    1_112_486_400,
    "last sunday",
    "2005/04/05/08:15:48 GMT",
    ""
); // Bug #32588
make_test!(
    bug32588_03,
    1_112_486_400,
    "0 secs",
    "2005/04/03/00:00:00 GMT",
    ""
); // Bug #32588

make_test!(
    bug32588_04,
    1_112_572_800,
    "last monday",
    "2005/04/05/08:15:48 GMT",
    ""
); // Bug #32588
make_test!(
    bug32588_05,
    1_112_572_800,
    "0 secs",
    "2005/04/04/00:00:00 GMT",
    ""
); // Bug #32588

make_test!(
    bug32588_06,
    1_112_688_948,
    "0 secs",
    "2005/04/05/08:15:48 GMT",
    ""
); // Bug #32588
make_test!(
    bug32588_07,
    1_112_659_200,
    "0 secs",
    "2005/04/05/00:00:00 GMT",
    ""
); // Bug #32588

/* from bug33056.ts */
make_test!(bug33056_00, 1_116_406_800, "20050518t090000Z", "", ""); // Bug #33056
make_test!(bug33056_01, 1_116_407_554, "20050518t091234Z", "", ""); // Bug #33056
make_test!(bug33056_02, 1_116_443_554, "20050518t191234Z", "", ""); // Bug #33056
make_test!(
    bug33056_03,
    1_116_403_200,
    "20050518t090000",
    "",
    "Europe/London"
); // Bug #33056
make_test!(
    bug33056_04,
    1_116_403_954,
    "20050518t091234",
    "",
    "Europe/London"
); // Bug #33056
make_test!(
    bug33056_05,
    1_116_439_954,
    "20050518t191234",
    "",
    "Europe/London"
); // Bug #33056

/* from bug34874.ts */
make_test!(bug34874_00, 1_130_284_800, "", "2005-10-26 00:00", "UTC");
make_test!(
    bug34874_01,
    1_130_284_800,
    "first wednesday",
    "2005-10-19 15:05",
    "UTC"
);
make_test!(
    bug34874_02,
    1_130_284_800,
    "next wednesday",
    "2005-10-19 15:05",
    "UTC"
);
make_test!(
    bug34874_03,
    1_129_680_000,
    "wednesday",
    "2005-10-19 15:05",
    "UTC"
);
make_test!(
    bug34874_04,
    1_129_680_000,
    "this wednesday",
    "2005-10-19 15:05",
    "UTC"
);
make_test!(bug34874_05, 1_129_680_000, "", "2005-10-19 00:00", "UTC");
make_test!(bug34874_06, 1_129_734_300, "", "2005-10-19 15:05", "UTC");

make_test!(
    bug34874_07,
    1_130_198_400,
    "tuesday",
    "2005-10-19 15:05",
    "UTC"
);
make_test!(
    bug34874_08,
    1_129_680_000,
    "wednesday",
    "2005-10-19 15:05",
    "UTC"
);
make_test!(
    bug34874_09,
    1_129_766_400,
    "thursday",
    "2005-10-19 15:05",
    "UTC"
);

/* from bug37017.ts */
make_test!(
    bug37017_00,
    1_147_453_201,
    "2006-05-12 13:00:01 America/New_York",
    "",
    ""
);
make_test!(
    bug37017_01,
    1_147_453_200,
    "2006-05-12 13:00:00 America/New_York",
    "",
    ""
);
make_test!(
    bug37017_02,
    1_147_453_199,
    "2006-05-12 12:59:59 America/New_York",
    "",
    ""
);
make_test!(
    bug37017_03,
    1_147_438_799,
    "2006-05-12 12:59:59 GMT",
    "",
    ""
);

/* from bug40290.ts */
make_test!(
    bug40290_00,
    1_170_159_900,
    "Tue, 30 Jan 2007 12:27:00 +0002",
    "",
    "Pacific/Auckland"
); // Bug #40290
make_test!(
    bug40290_01,
    1_170_159_960,
    "Tue, 30 Jan 2007 12:27:00 +0001",
    "",
    "Pacific/Auckland"
); // Bug #40290
make_test!(
    bug40290_02,
    1_170_160_020,
    "Tue, 30 Jan 2007 12:27:00 +0000",
    "",
    "Pacific/Auckland"
); // Bug #40290
make_test!(
    bug40290_03,
    1_170_160_080,
    "Tue, 30 Jan 2007 12:27:00 -0001",
    "",
    "Pacific/Auckland"
); // Bug #40290
make_test!(
    bug40290_04,
    1_170_160_140,
    "Tue, 30 Jan 2007 12:27:00 -0002",
    "",
    "Pacific/Auckland"
); // Bug #40290

/* from bug41709.ts */
make_test!(bug41709_00, 946_684_800, "01.01.2000", "00:00:00 GMT", "");
make_test!(bug41709_01, 946_598_400, "00.01.2000", "00:00:00 GMT", "");
make_test!(bug41709_02, 943_920_000, "00.00.2000", "00:00:00 GMT", "");
make_test!(
    bug41709_03,
    -62_167_219_200,
    "01.01.0000",
    "00:00:00 GMT",
    ""
);
make_test!(
    bug41709_04,
    -62_167_305_600,
    "00.01.0000",
    "00:00:00 GMT",
    ""
);
make_test!(
    bug41709_05,
    -62_169_984_000,
    "00.00.0000",
    "00:00:00 GMT",
    ""
);

/* for bug51934 */
make_test!(
    bug51934_00,
    1_272_853_080,
    "4 sundays ago",
    "2010-05-27 19:18",
    "America/Los_Angeles"
);

/* from bug63470.ts */
make_test!(
    bug63470_00,
    1_435_536_000,
    "this week",
    "2015-07-04 00:00",
    "UTC"
);
make_test!(
    bug63470_01,
    1_435_536_000,
    "this week",
    "2015-07-05 00:00",
    "UTC"
); // Sunday
make_test!(
    bug63470_02,
    1_436_140_800,
    "this week",
    "2015-07-06 00:00",
    "UTC"
);
make_test!(
    bug63470_03,
    1_436_140_800,
    "this week",
    "2015-07-11 00:00",
    "UTC"
);
make_test!(
    bug63470_04,
    1_436_140_800,
    "this week",
    "2015-07-12 00:00",
    "UTC"
); // Sunday
make_test!(
    bug63470_05,
    1_436_745_600,
    "this week",
    "2015-07-13 00:00",
    "UTC"
);

make_test!(bug63470_06, 1_209_254_400, "", "2008-04-27 00:00", "UTC"); // Thursday
make_test!(
    bug63470_07,
    1_209_254_400,
    "this week sunday",
    "2008-04-25 00:00",
    "UTC"
); // Thursday

make_test!(bug63470_08, 1_208_822_400, "", "2008-04-22 00:00", "UTC"); // Thursday
make_test!(
    bug63470_09,
    1_208_822_400,
    "this week tuesday",
    "2008-04-25 00:00",
    "UTC"
); // Thursday

make_test!(bug63470_10, 1_482_710_400, "", "2016-12-26 00:00", "UTC");
make_test!(
    bug63470_11,
    1_482_710_400,
    "monday this week",
    "2017-01-01 00:00",
    "UTC"
);
make_test!(bug63470_12, 1_483_315_200, "", "2017-01-02 00:00", "UTC");
make_test!(
    bug63470_13,
    1_483_315_200,
    "monday this week",
    "2017-01-02 00:00",
    "UTC"
);
make_test!(
    bug63470_14,
    1_483_315_200,
    "monday this week",
    "2017-01-03 00:00",
    "UTC"
);
make_test!(
    bug63470_15,
    1_483_315_200,
    "monday this week",
    "2017-01-04 00:00",
    "UTC"
);
make_test!(
    bug63470_16,
    1_483_315_200,
    "monday this week",
    "2017-01-05 00:00",
    "UTC"
);
make_test!(
    bug63470_17,
    1_483_315_200,
    "monday this week",
    "2017-01-06 00:00",
    "UTC"
);
make_test!(
    bug63470_18,
    1_483_315_200,
    "monday this week",
    "2017-01-07 00:00",
    "UTC"
);
make_test!(
    bug63470_19,
    1_483_315_200,
    "monday this week",
    "2017-01-08 00:00",
    "UTC"
);
make_test!(bug63470_20, 1_483_920_000, "", "2017-01-09 00:00", "UTC");
make_test!(
    bug63470_21,
    1_483_920_000,
    "monday this week",
    "2017-01-09 00:00",
    "UTC"
);

make_test!(bug63470_22, 1_483_056_000, "", "2016-12-30 00:00", "UTC");
make_test!(
    bug63470_23,
    1_483_056_000,
    "friday this week",
    "2017-01-01 00:00",
    "UTC"
);
make_test!(bug63470_24, 1_483_660_800, "", "2017-01-06 00:00", "UTC");
make_test!(
    bug63470_25,
    1_483_660_800,
    "friday this week",
    "2017-01-02 00:00",
    "UTC"
);
make_test!(
    bug63470_26,
    1_483_660_800,
    "friday this week",
    "2017-01-03 00:00",
    "UTC"
);
make_test!(
    bug63470_27,
    1_483_660_800,
    "friday this week",
    "2017-01-04 00:00",
    "UTC"
);
make_test!(
    bug63470_28,
    1_483_660_800,
    "friday this week",
    "2017-01-05 00:00",
    "UTC"
);
make_test!(
    bug63470_29,
    1_483_660_800,
    "friday this week",
    "2017-01-06 00:00",
    "UTC"
);
make_test!(
    bug63470_30,
    1_483_660_800,
    "friday this week",
    "2017-01-07 00:00",
    "UTC"
);
make_test!(
    bug63470_31,
    1_483_660_800,
    "friday this week",
    "2017-01-08 00:00",
    "UTC"
);
make_test!(bug63470_32, 1_484_265_600, "", "2017-01-13 00:00", "UTC");
make_test!(
    bug63470_33,
    1_484_265_600,
    "friday this week",
    "2017-01-09 00:00",
    "UTC"
);

make_test!(bug63470_34, 1_483_142_400, "", "2016-12-31 00:00", "UTC");
make_test!(
    bug63470_35,
    1_483_142_400,
    "saturday this week",
    "2017-01-01 00:00",
    "UTC"
);
make_test!(bug63470_36, 1_483_747_200, "", "2017-01-07 00:00", "UTC");
make_test!(
    bug63470_37,
    1_483_747_200,
    "saturday this week",
    "2017-01-02 00:00",
    "UTC"
);
make_test!(
    bug63470_38,
    1_483_747_200,
    "saturday this week",
    "2017-01-03 00:00",
    "UTC"
);
make_test!(
    bug63470_39,
    1_483_747_200,
    "saturday this week",
    "2017-01-04 00:00",
    "UTC"
);
make_test!(
    bug63470_40,
    1_483_747_200,
    "saturday this week",
    "2017-01-05 00:00",
    "UTC"
);
make_test!(
    bug63470_41,
    1_483_747_200,
    "saturday this week",
    "2017-01-06 00:00",
    "UTC"
);
make_test!(
    bug63470_42,
    1_483_747_200,
    "saturday this week",
    "2017-01-07 00:00",
    "UTC"
);
make_test!(
    bug63470_43,
    1_483_747_200,
    "saturday this week",
    "2017-01-08 00:00",
    "UTC"
);
make_test!(bug63470_44, 1_484_352_000, "", "2017-01-14 00:00", "UTC");
make_test!(
    bug63470_45,
    1_484_352_000,
    "saturday this week",
    "2017-01-09 00:00",
    "UTC"
);

make_test!(bug63470_46, 1_483_228_800, "", "2017-01-01 00:00", "UTC");
make_test!(
    bug63470_47,
    1_483_228_800,
    "sunday this week",
    "2017-01-01 00:00",
    "UTC"
);
make_test!(bug63470_48, 1_483_833_600, "", "2017-01-08 00:00", "UTC");
make_test!(
    bug63470_49,
    1_483_833_600,
    "sunday this week",
    "2017-01-02 00:00",
    "UTC"
);
make_test!(
    bug63470_50,
    1_483_833_600,
    "sunday this week",
    "2017-01-03 00:00",
    "UTC"
);
make_test!(
    bug63470_51,
    1_483_833_600,
    "sunday this week",
    "2017-01-04 00:00",
    "UTC"
);
make_test!(
    bug63470_52,
    1_483_833_600,
    "sunday this week",
    "2017-01-05 00:00",
    "UTC"
);
make_test!(
    bug63470_53,
    1_483_833_600,
    "sunday this week",
    "2017-01-06 00:00",
    "UTC"
);
make_test!(
    bug63470_54,
    1_483_833_600,
    "sunday this week",
    "2017-01-07 00:00",
    "UTC"
);
make_test!(
    bug63470_55,
    1_483_833_600,
    "sunday this week",
    "2017-01-08 00:00",
    "UTC"
);
make_test!(bug63470_56, 1_484_438_400, "", "2017-01-15 00:00", "UTC");
make_test!(
    bug63470_57,
    1_484_438_400,
    "sunday this week",
    "2017-01-09 00:00",
    "UTC"
);

/* from bug73294.ts */
make_test!(bug73294_00, -122_110_502_400, "-1900-06-22", "", "UTC");
make_test!(bug73294_01, -122_615_337_600, "-1916-06-22", "", "UTC");

/* from first_transition.ts */
make_test!(
    first_transition_00,
    -2_695_022_427,
    "1884-08-06 06:39:33",
    "",
    "America/Los_Angeles"
);
make_test!(
    first_transition_01,
    -2_190_187_227,
    "1900-08-06 06:39:33",
    "",
    "America/Los_Angeles"
);
make_test!(
    first_transition_02,
    -2_158_651_227,
    "1901-08-06 06:39:33",
    "",
    "America/Los_Angeles"
);
make_test!(
    first_transition_03,
    -2_127_115_227,
    "1902-08-06 06:39:33",
    "",
    "America/Los_Angeles"
);
make_test!(
    first_transition_04,
    -1_637_832_027,
    "1918-02-06 06:39:33",
    "",
    "America/Los_Angeles"
);
make_test!(
    first_transition_05,
    -1_622_197_227,
    "1918-08-06 06:39:33",
    "",
    "America/Los_Angeles"
);

/* from full.ts */
make_test!(full_00, 1_126_396_800, "9/11", "2005-09-11 00:00:00", ""); // We have no timezone at all -> GMT
make_test!(
    full_01,
    1_126_396_800,
    "9/11",
    "2005-09-11 00:00:00 GMT",
    ""
); // The filler specified a timezone -> GMT
make_test!(full_02, 1_126_396_800, "9/11", "2005-09-11 00:00:00", "GMT"); // Global timezone is set -> GMT
make_test!(
    full_03,
    1_126_396_800,
    "9/11 GMT",
    "2005-09-11 00:00:00",
    ""
); // String to be parsed contains timezone -> GMT

make_test!(
    full_04,
    1_126_393_200,
    "9/11",
    "2005-09-11 00:00:00 CET",
    ""
); // Timezone specified -> CET, no DST (GMT+1)
make_test!(
    full_05,
    1_126_389_600,
    "9/11",
    "2005-09-11 00:00:00 CEST",
    ""
); // Timezone specified -> CEST, with DST (GMT+2)

make_test!(
    full_06,
    1_126_393_200,
    "9/11 CET",
    "2005-09-11 00:00:00",
    ""
); // Timezone specified -> CET, no DST (GMT+1)
make_test!(
    full_07,
    1_126_389_600,
    "9/11 CEST",
    "2005-09-11 00:00:00",
    ""
); // Timezone specified -> CEST, with DST (GMT+2)

make_test!(
    full_08,
    1_126_389_600,
    "9/11",
    "2005-09-11 00:00:00",
    "Europe/Amsterdam"
); // Zone identifier specified -> use it (CEST, GMT+2)
make_test!(
    full_09,
    1_126_393_200,
    "9/11",
    "2005-09-11 00:00:00 CET",
    "Europe/Amsterdam"
); // Timezone specified (wrong) and Zone ID specified -> adjust (GMT+1)
make_test!(
    full_10,
    1_126_389_600,
    "9/11",
    "2005-09-11 00:00:00 CEST",
    "Europe/Amsterdam"
); // Timezone specified and Zone ID specified -> (GMT+2)

make_test!(
    full_11,
    1_105_401_600,
    "1/11",
    "2005-01-11 00:00:00 GMT",
    ""
); // Timezone specified -> GMT
make_test!(
    full_12,
    1_105_398_000,
    "1/11",
    "2005-01-11 00:00:00 CET",
    ""
); // Timezone specified -> CET
make_test!(
    full_13,
    1_105_394_400,
    "1/11",
    "2005-01-11 00:00:00 CEST",
    ""
); // Timezone specified -> CEST (doesn't actually exist)

make_test!(
    full_14,
    1_105_398_000,
    "1/11",
    "2005-01-11 00:00:00",
    "Europe/Amsterdam"
); // Zone identifier specified -> use it (CST, GMT+1)
make_test!(
    full_15,
    1_105_398_000,
    "1/11",
    "2005-01-11 00:00:00 CET",
    "Europe/Amsterdam"
); // Timezone specified and Zone ID specified -> adjust (GMT+1)
make_test!(
    full_16,
    1_105_394_400,
    "1/11",
    "2005-01-11 00:00:00 CEST",
    "Europe/Amsterdam"
); // Timezone specified (wrong) and Zone ID specified -> (GMT+2)

make_test!(full_17, 283_132_800, "1978-12-22", "00:00:00 GMT", "");
make_test!(full_18, 283_147_200, "1978-12-22", "00:00:00 EDT", "");
make_test!(full_19, 1_113_861_600, "2005-04-19", "", "Europe/Amsterdam");
make_test!(full_20, 1_113_886_800, "2005-04-19", "", "America/Chicago");
make_test!(full_21, 1_113_883_200, "2005-04-19", "", "America/New_York");
make_test!(
    full_22,
    1_113_883_200,
    "2005-04-19",
    "00:00:00",
    "America/New_York"
);
make_test!(
    full_23,
    1_113_918_120,
    "2005-04-19",
    "09:42:00",
    "America/New_York"
);

/* from last_day_of.ts */
make_test!(
    last_day_of_00,
    1_203_724_800,
    "last saturday of feb 2008",
    "2008-05-04 22:28:27",
    "UTC"
);
make_test!(
    last_day_of_01,
    1_227_571_200,
    "last tue of 2008-11",
    "2008-05-04 22:28:27",
    "UTC"
);
make_test!(
    last_day_of_02,
    1_222_560_000,
    "last sunday of sept",
    "2008-05-04 22:28:27",
    "UTC"
);
make_test!(
    last_day_of_03,
    1_212_192_000,
    "last saturday of this month",
    "2008-05-04 22:28:27",
    "UTC"
);
make_test!(
    last_day_of_04,
    1_208_995_200,
    "last thursday of last month",
    "2008-05-04 22:28:27",
    "UTC"
);
make_test!(
    last_day_of_05,
    1_222_214_400,
    "last wed of fourth month",
    "2008-05-04 22:28:27",
    "UTC"
);
make_test!(
    last_day_of_06,
    1_201_219_200,
    "last friday of next month",
    "2007-12-13",
    "UTC"
);

/* from month.ts */
make_test!(
    month_00,
    1_141_410_805,
    "march",
    "2006-05-03 18:33:25",
    "UTC"
);
make_test!(month_01, 1_159_900_405, "OCT", "2006-05-03 18:33:25", "UTC");
make_test!(
    month_02,
    1_157_308_405,
    "September",
    "2006-05-03 18:33:25",
    "UTC"
);
make_test!(
    month_03,
    1_165_170_805,
    "deCEMber",
    "2006-05-03 18:33:25",
    "UTC"
);

/* from relative.ts */
make_test!(
    relative_00,
    1_116_457_202,
    "+2 sec",
    "2005-05-18 23:00 GMT",
    "GMT"
);
make_test!(
    relative_01,
    1_116_457_198,
    "2 secs ago",
    "2005-05-18 23:00 GMT",
    "GMT"
);

make_test!(
    relative_02,
    1_116_630_000,
    "+2 days",
    "2005-05-18 23:00 GMT",
    "GMT"
);
make_test!(
    relative_03,
    1_116_630_000,
    "",
    "2005-05-20 23:00 GMT",
    "GMT"
); // should be the same

make_test!(
    relative_04,
    1_116_284_400,
    "+2 days ago",
    "2005-05-18 23:00 GMT",
    "GMT"
);

make_test!(
    relative_05,
    1_112_828_400,
    "-3 forthnight",
    "2005-05-18 23:00 GMT",
    "GMT"
);
make_test!(
    relative_06,
    1_112_828_400,
    "",
    "2005-04-06 23:00 GMT",
    "GMT"
);

make_test!(
    relative_07,
    1_123_714_800,
    "+12 weeks",
    "2005-05-18 23:00 GMT",
    "GMT"
);
make_test!(
    relative_08,
    1_123_714_800,
    "",
    "2005-08-10 23:00 GMT",
    "GMT"
);

make_test!(
    relative_09,
    1_128_938_400,
    "0 secs",
    "2005-10-10 12:00",
    "Europe/Amsterdam"
);
make_test!(
    relative_10,
    1_128_942_000,
    "0 secs",
    "2005-10-10 12:00 CET",
    "Europe/Amsterdam"
);
make_test!(
    relative_11,
    1_128_938_400,
    "0 secs",
    "2005-10-10 12:00 CEST",
    "Europe/Amsterdam"
);
make_test!(
    relative_12,
    1_131_620_400,
    "0 secs",
    "2005-11-10 12:00",
    "Europe/Amsterdam"
);
make_test!(
    relative_13,
    1_131_620_400,
    "0 secs",
    "2005-11-10 12:00 CET",
    "Europe/Amsterdam"
);
make_test!(
    relative_14,
    1_131_616_800,
    "0 secs",
    "2005-11-10 12:00 CEST",
    "Europe/Amsterdam"
);
make_test!(
    relative_15,
    1_131_620_400,
    "+31 days",
    "2005-10-10 12:00",
    "Europe/Amsterdam"
);

make_test!(
    relative_16,
    1_099_648_800,
    "6 month 2004-05-05 12:00:00 CEST",
    "",
    ""
);
make_test!(
    relative_17,
    1_099_648_800,
    "2004-11-05 12:00:00 CEST",
    "",
    ""
);
make_test!(
    relative_18,
    1_099_648_800,
    "2004-05-05 12:00:00 CEST 6 months",
    "",
    ""
);

make_test!(
    relative_19,
    1_099_648_800,
    "6 month 2004-05-05 12:00:00 CEST",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_20,
    1_099_648_800,
    "2004-11-05 12:00:00 CEST",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_21,
    1_099_648_800,
    "2004-05-05 12:00:00 CEST 6 months",
    "",
    "Europe/Amsterdam"
);

make_test!(
    relative_22,
    1_099_652_400,
    "6 month 2004-05-05 12:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_23,
    1_099_652_400,
    "2004-11-05 12:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_24,
    1_099_652_400,
    "2004-05-05 12:00:00 6 months",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_25,
    1_083_751_200,
    "2004-05-05 12:00:00",
    "",
    "Europe/Amsterdam"
);

make_test!(
    relative_26,
    1_068_027_323,
    "2003-11-05 12:15:23 CEST",
    "",
    ""
);
make_test!(
    relative_27,
    1_068_027_323,
    "2004-05-05 12:15:23 CEST 6 months ago",
    "",
    ""
);
make_test!(
    relative_28,
    1_068_372_923,
    "2003-11-09 12:15:23 CEST",
    "",
    ""
);
make_test!(
    relative_29,
    1_068_372_923,
    "2004-05-05 12:15:23 CEST 6 months ago 4 days",
    "",
    ""
);

make_test!(
    relative_30,
    1_145_570_400,
    "21-04-2006",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_31,
    1_145_570_400,
    "this weekday",
    "21-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_32,
    1_145_484_000,
    "last weekday",
    "21-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_33,
    1_145_570_400,
    "last weekday",
    "22-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_34,
    1_145_570_400,
    "last weekday",
    "23-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_35,
    1_145_397_600,
    "13 weekdays ago",
    "07-05-2006",
    "Europe/Amsterdam"
);

make_test!(
    relative_36,
    1_145_570_400,
    "21-04-2006",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_37,
    1_145_570_400,
    "this weekday",
    "21-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_38,
    1_145_829_600,
    "24-04-2006",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_39,
    1_145_829_600,
    "this weekday",
    "22-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_40,
    1_145_829_600,
    "this weekday",
    "23-04-2006",
    "Europe/Amsterdam"
);

make_test!(
    relative_41,
    1_145_829_600,
    "24-04-2006",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_42,
    1_145_829_600,
    "first weekday",
    "21-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_43,
    1_145_829_600,
    "first weekday",
    "22-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_44,
    1_145_829_600,
    "first weekday",
    "23-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_45,
    1_145_916_000,
    "25-04-2006",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_46,
    1_145_916_000,
    "first weekday",
    "24-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_47,
    1_146_002_400,
    "26-04-2006",
    "",
    "Europe/Amsterdam"
);
make_test!(
    relative_48,
    1_146_002_400,
    "8 weekday",
    "15-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_49,
    1_146_002_400,
    "eight weekday",
    "15-04-2006",
    "Europe/Amsterdam"
);
make_test!(
    relative_50,
    1_149_700_004,
    "Mon, 08 May 2006 13:06:44 -0400 +30 days",
    "",
    "Europe/Amsterdam"
);

/* from relative_weekday_1.ts */
make_test!(
    relative_weekday_1_00,
    1_216_598_400,
    "1 monday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_01,
    1_216_684_800,
    "1 tuesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_02,
    1_216_771_200,
    "1 wednesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_03,
    1_216_857_600,
    "1 thursday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_04,
    1_216_944_000,
    "1 friday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_05,
    1_217_030_400,
    "1 saturday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_06,
    1_217_116_800,
    "1 sunday",
    "2008-07-21 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_1_07,
    1_217_203_200,
    "1 monday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_08,
    1_216_684_800,
    "1 tuesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_09,
    1_216_771_200,
    "1 wednesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_10,
    1_216_857_600,
    "1 thursday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_11,
    1_216_944_000,
    "1 friday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_12,
    1_217_030_400,
    "1 saturday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_13,
    1_217_116_800,
    "1 sunday",
    "2008-07-22 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_1_14,
    1_217_203_200,
    "1 monday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_15,
    1_217_289_600,
    "1 tuesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_16,
    1_216_771_200,
    "1 wednesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_17,
    1_216_857_600,
    "1 thursday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_18,
    1_216_944_000,
    "1 friday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_19,
    1_217_030_400,
    "1 saturday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_20,
    1_217_116_800,
    "1 sunday",
    "2008-07-23 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_1_21,
    1_217_203_200,
    "1 monday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_22,
    1_217_289_600,
    "1 tuesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_23,
    1_217_376_000,
    "1 wednesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_24,
    1_216_857_600,
    "1 thursday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_25,
    1_216_944_000,
    "1 friday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_26,
    1_217_030_400,
    "1 saturday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_27,
    1_217_116_800,
    "1 sunday",
    "2008-07-24 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_1_28,
    1_217_203_200,
    "1 monday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_29,
    1_217_289_600,
    "1 tuesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_30,
    1_217_376_000,
    "1 wednesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_31,
    1_217_462_400,
    "1 thursday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_32,
    1_216_944_000,
    "1 friday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_33,
    1_217_030_400,
    "1 saturday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_34,
    1_217_116_800,
    "1 sunday",
    "2008-07-25 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_1_35,
    1_217_203_200,
    "1 monday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_36,
    1_217_289_600,
    "1 tuesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_37,
    1_217_376_000,
    "1 wednesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_38,
    1_217_462_400,
    "1 thursday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_39,
    1_217_548_800,
    "1 friday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_40,
    1_217_030_400,
    "1 saturday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_41,
    1_217_116_800,
    "1 sunday",
    "2008-07-26 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_1_42,
    1_217_203_200,
    "1 monday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_43,
    1_217_289_600,
    "1 tuesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_44,
    1_217_376_000,
    "1 wednesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_45,
    1_217_462_400,
    "1 thursday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_46,
    1_217_548_800,
    "1 friday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_47,
    1_217_635_200,
    "1 saturday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_1_48,
    1_217_116_800,
    "1 sunday",
    "2008-07-27 00:00 UTC",
    "UTC"
);

/* from relative_weekday_2.ts */
make_test!(
    relative_weekday_2_00,
    1_217_203_200,
    "+1 week monday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_01,
    1_217_289_600,
    "+1 week tuesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_02,
    1_217_376_000,
    "+1 week wednesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_03,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_04,
    1_217_548_800,
    "+1 week friday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_05,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_06,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-21 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_2_07,
    1_217_808_000,
    "+1 week monday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_08,
    1_217_289_600,
    "+1 week tuesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_09,
    1_217_376_000,
    "+1 week wednesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_10,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_11,
    1_217_548_800,
    "+1 week friday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_12,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_13,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-22 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_2_14,
    1_217_808_000,
    "+1 week monday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_15,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_16,
    1_217_376_000,
    "+1 week wednesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_17,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_18,
    1_217_548_800,
    "+1 week friday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_19,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_20,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-23 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_2_21,
    1_217_808_000,
    "+1 week monday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_22,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_23,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_24,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_25,
    1_217_548_800,
    "+1 week friday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_26,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_27,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-24 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_2_28,
    1_217_808_000,
    "+1 week monday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_29,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_30,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_31,
    1_218_067_200,
    "+1 week thursday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_32,
    1_217_548_800,
    "+1 week friday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_33,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_34,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-25 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_2_35,
    1_217_808_000,
    "+1 week monday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_36,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_37,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_38,
    1_218_067_200,
    "+1 week thursday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_39,
    1_218_153_600,
    "+1 week friday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_40,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_41,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-26 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_2_42,
    1_217_808_000,
    "+1 week monday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_43,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_44,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_45,
    1_218_067_200,
    "+1 week thursday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_46,
    1_218_153_600,
    "+1 week friday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_47,
    1_218_240_000,
    "+1 week saturday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_2_48,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-27 00:00 UTC",
    "UTC"
);

/* from relative_weekday_first.ts */
make_test!(
    relative_weekday_first_00,
    1_217_203_200,
    "first monday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_01,
    1_216_684_800,
    "first tuesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_02,
    1_216_771_200,
    "first wednesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_03,
    1_216_857_600,
    "first thursday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_04,
    1_216_944_000,
    "first friday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_05,
    1_217_030_400,
    "first saturday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_06,
    1_217_116_800,
    "first sunday",
    "2008-07-21 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_first_07,
    1_217_203_200,
    "first monday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_08,
    1_217_289_600,
    "first tuesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_09,
    1_216_771_200,
    "first wednesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_10,
    1_216_857_600,
    "first thursday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_11,
    1_216_944_000,
    "first friday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_12,
    1_217_030_400,
    "first saturday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_13,
    1_217_116_800,
    "first sunday",
    "2008-07-22 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_first_14,
    1_217_203_200,
    "first monday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_15,
    1_217_289_600,
    "first tuesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_16,
    1_217_376_000,
    "first wednesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_17,
    1_216_857_600,
    "first thursday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_18,
    1_216_944_000,
    "first friday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_19,
    1_217_030_400,
    "first saturday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_20,
    1_217_116_800,
    "first sunday",
    "2008-07-23 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_first_21,
    1_217_203_200,
    "first monday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_22,
    1_217_289_600,
    "first tuesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_23,
    1_217_376_000,
    "first wednesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_24,
    1_217_462_400,
    "first thursday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_25,
    1_216_944_000,
    "first friday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_26,
    1_217_030_400,
    "first saturday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_27,
    1_217_116_800,
    "first sunday",
    "2008-07-24 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_first_28,
    1_217_203_200,
    "first monday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_29,
    1_217_289_600,
    "first tuesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_30,
    1_217_376_000,
    "first wednesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_31,
    1_217_462_400,
    "first thursday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_32,
    1_217_548_800,
    "first friday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_33,
    1_217_030_400,
    "first saturday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_34,
    1_217_116_800,
    "first sunday",
    "2008-07-25 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_first_35,
    1_217_203_200,
    "first monday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_36,
    1_217_289_600,
    "first tuesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_37,
    1_217_376_000,
    "first wednesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_38,
    1_217_462_400,
    "first thursday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_39,
    1_217_548_800,
    "first friday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_40,
    1_217_635_200,
    "first saturday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_41,
    1_217_116_800,
    "first sunday",
    "2008-07-26 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_first_42,
    1_217_203_200,
    "first monday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_43,
    1_217_289_600,
    "first tuesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_44,
    1_217_376_000,
    "first wednesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_45,
    1_217_462_400,
    "first thursday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_46,
    1_217_548_800,
    "first friday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_47,
    1_217_635_200,
    "first saturday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_first_48,
    1_217_721_600,
    "first sunday",
    "2008-07-27 00:00 UTC",
    "UTC"
);

/* from relative_weekday_second.ts */
make_test!(
    relative_weekday_second_00,
    1_217_808_000,
    "second monday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_01,
    1_217_289_600,
    "second tuesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_02,
    1_217_376_000,
    "second wednesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_03,
    1_217_462_400,
    "second thursday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_04,
    1_217_548_800,
    "second friday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_05,
    1_217_635_200,
    "second saturday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_06,
    1_217_721_600,
    "second sunday",
    "2008-07-21 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_second_07,
    1_217_808_000,
    "second monday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_08,
    1_217_894_400,
    "second tuesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_09,
    1_217_376_000,
    "second wednesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_10,
    1_217_462_400,
    "second thursday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_11,
    1_217_548_800,
    "second friday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_12,
    1_217_635_200,
    "second saturday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_13,
    1_217_721_600,
    "second sunday",
    "2008-07-22 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_second_14,
    1_217_808_000,
    "second monday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_15,
    1_217_894_400,
    "second tuesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_16,
    1_217_980_800,
    "second wednesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_17,
    1_217_462_400,
    "second thursday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_18,
    1_217_548_800,
    "second friday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_19,
    1_217_635_200,
    "second saturday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_20,
    1_217_721_600,
    "second sunday",
    "2008-07-23 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_second_21,
    1_217_808_000,
    "second monday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_22,
    1_217_894_400,
    "second tuesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_23,
    1_217_980_800,
    "second wednesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_24,
    1_218_067_200,
    "second thursday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_25,
    1_217_548_800,
    "second friday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_26,
    1_217_635_200,
    "second saturday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_27,
    1_217_721_600,
    "second sunday",
    "2008-07-24 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_second_28,
    1_217_808_000,
    "second monday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_29,
    1_217_894_400,
    "second tuesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_30,
    1_217_980_800,
    "second wednesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_31,
    1_218_067_200,
    "second thursday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_32,
    1_218_153_600,
    "second friday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_33,
    1_217_635_200,
    "second saturday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_34,
    1_217_721_600,
    "second sunday",
    "2008-07-25 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_second_35,
    1_217_808_000,
    "second monday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_36,
    1_217_894_400,
    "second tuesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_37,
    1_217_980_800,
    "second wednesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_38,
    1_218_067_200,
    "second thursday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_39,
    1_218_153_600,
    "second friday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_40,
    1_218_240_000,
    "second saturday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_41,
    1_217_721_600,
    "second sunday",
    "2008-07-26 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_second_42,
    1_217_808_000,
    "second monday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_43,
    1_217_894_400,
    "second tuesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_44,
    1_217_980_800,
    "second wednesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_45,
    1_218_067_200,
    "second thursday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_46,
    1_218_153_600,
    "second friday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_47,
    1_218_240_000,
    "second saturday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_second_48,
    1_218_326_400,
    "second sunday",
    "2008-07-27 00:00 UTC",
    "UTC"
);

/* from relative_weekday.ts */
make_test!(
    relative_weekday_00,
    1_216_598_400,
    "monday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_01,
    1_216_684_800,
    "tuesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_02,
    1_216_771_200,
    "wednesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_03,
    1_216_857_600,
    "thursday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_04,
    1_216_944_000,
    "friday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_05,
    1_217_030_400,
    "saturday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_06,
    1_217_116_800,
    "sunday",
    "2008-07-21 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_07,
    1_217_203_200,
    "monday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_08,
    1_216_684_800,
    "tuesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_09,
    1_216_771_200,
    "wednesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_10,
    1_216_857_600,
    "thursday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_11,
    1_216_944_000,
    "friday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_12,
    1_217_030_400,
    "saturday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_13,
    1_217_116_800,
    "sunday",
    "2008-07-22 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_14,
    1_217_203_200,
    "monday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_15,
    1_217_289_600,
    "tuesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_16,
    1_216_771_200,
    "wednesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_17,
    1_216_857_600,
    "thursday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_18,
    1_216_944_000,
    "friday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_19,
    1_217_030_400,
    "saturday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_20,
    1_217_116_800,
    "sunday",
    "2008-07-23 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_21,
    1_217_203_200,
    "monday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_22,
    1_217_289_600,
    "tuesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_23,
    1_217_376_000,
    "wednesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_24,
    1_216_857_600,
    "thursday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_25,
    1_216_944_000,
    "friday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_26,
    1_217_030_400,
    "saturday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_27,
    1_217_116_800,
    "sunday",
    "2008-07-24 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_28,
    1_217_203_200,
    "monday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_29,
    1_217_289_600,
    "tuesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_30,
    1_217_376_000,
    "wednesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_31,
    1_217_462_400,
    "thursday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_32,
    1_216_944_000,
    "friday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_33,
    1_217_030_400,
    "saturday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_34,
    1_217_116_800,
    "sunday",
    "2008-07-25 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_35,
    1_217_203_200,
    "monday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_36,
    1_217_289_600,
    "tuesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_37,
    1_217_376_000,
    "wednesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_38,
    1_217_462_400,
    "thursday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_39,
    1_217_548_800,
    "friday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_40,
    1_217_030_400,
    "saturday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_41,
    1_217_116_800,
    "sunday",
    "2008-07-26 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_42,
    1_217_203_200,
    "monday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_43,
    1_217_289_600,
    "tuesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_44,
    1_217_376_000,
    "wednesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_45,
    1_217_462_400,
    "thursday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_46,
    1_217_548_800,
    "friday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_47,
    1_217_635_200,
    "saturday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_48,
    1_217_116_800,
    "sunday",
    "2008-07-27 00:00 UTC",
    "UTC"
);

/* from relative_weekday_week1.ts */
make_test!(
    relative_weekday_week1_00,
    1_217_203_200,
    "+1 week monday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_01,
    1_217_289_600,
    "+1 week tuesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_02,
    1_217_376_000,
    "+1 week wednesday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_03,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_04,
    1_217_548_800,
    "+1 week friday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_05,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-21 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_06,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-21 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_week1_07,
    1_217_808_000,
    "+1 week monday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_08,
    1_217_289_600,
    "+1 week tuesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_09,
    1_217_376_000,
    "+1 week wednesday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_10,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_11,
    1_217_548_800,
    "+1 week friday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_12,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-22 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_13,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-22 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_week1_14,
    1_217_808_000,
    "+1 week monday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_15,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_16,
    1_217_376_000,
    "+1 week wednesday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_17,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_18,
    1_217_548_800,
    "+1 week friday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_19,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-23 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_20,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-23 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_week1_21,
    1_217_808_000,
    "+1 week monday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_22,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_23,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_24,
    1_217_462_400,
    "+1 week thursday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_25,
    1_217_548_800,
    "+1 week friday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_26,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-24 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_27,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-24 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_week1_28,
    1_217_808_000,
    "+1 week monday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_29,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_30,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_31,
    1_218_067_200,
    "+1 week thursday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_32,
    1_217_548_800,
    "+1 week friday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_33,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-25 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_34,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-25 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_week1_35,
    1_217_808_000,
    "+1 week monday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_36,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_37,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_38,
    1_218_067_200,
    "+1 week thursday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_39,
    1_218_153_600,
    "+1 week friday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_40,
    1_217_635_200,
    "+1 week saturday",
    "2008-07-26 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_41,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-26 00:00 UTC",
    "UTC"
);

make_test!(
    relative_weekday_week1_42,
    1_217_808_000,
    "+1 week monday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_43,
    1_217_894_400,
    "+1 week tuesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_44,
    1_217_980_800,
    "+1 week wednesday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_45,
    1_218_067_200,
    "+1 week thursday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_46,
    1_218_153_600,
    "+1 week friday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_47,
    1_218_240_000,
    "+1 week saturday",
    "2008-07-27 00:00 UTC",
    "UTC"
);
make_test!(
    relative_weekday_week1_48,
    1_217_721_600,
    "+1 week sunday",
    "2008-07-27 00:00 UTC",
    "UTC"
);

/* from strange.ts */
make_test!(
    strange_00,
    1_126_396_800,
    "+1126396800 secs",
    "1970-01-01 00:00:00 GMT",
    ""
);
make_test!(strange_01, 1_118_016_000, "2005-06-06 00:00:00", "", "");
make_test!(
    strange_02,
    1_126_396_800,
    "@1126396800",
    "1970-01-01 00:00:00 GMT",
    ""
);
make_test!(
    strange_03,
    -126_396_800,
    "@-126396800",
    "1970-01-01 00:00:00 GMT",
    ""
);
make_test!(
    strange_04,
    1_126_396_800,
    "@1126396800 +0200",
    "1970-01-01 00:00:00 GMT",
    ""
);
make_test!(
    strange_05,
    -126_396_800,
    "@-126396800 Europe/Oslo",
    "1970-01-01 00:00:00 GMT",
    ""
);
make_test!(strange_06, 0, "@0", "1970-01-01 00:00:00 GMT", "");
make_test!(
    strange_07,
    1_118_008_800,
    "2005-06-06 00:00:00 CEST",
    "",
    ""
);
make_test!(
    strange_08,
    1_118_008_800,
    "2005-06-06 00:00:00 +0200",
    "",
    ""
);
make_test!(
    strange_09,
    1_126_398_132,
    "@1126398132.712315",
    "1970-01-01 00:00:00 GMT",
    ""
);

/* from thisweek.ts */
make_test!(
    thisweek_00,
    1_116_547_200,
    "today",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_01,
    1_116_547_200,
    "00:00:00",
    "2005-05-20 00:00:00 GMT",
    ""
);

make_test!(thisweek_02, 1_116_590_400, "2005-05-20 noon", "", "");
make_test!(
    thisweek_03,
    1_116_590_400,
    "today noon",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_04,
    1_116_590_400,
    "12:00:00",
    "2005-05-20 00:00:00 GMT",
    ""
);

make_test!(
    thisweek_05,
    1_116_460_800,
    "yesterday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_06,
    1_116_460_800,
    "00:00:00",
    "2005-05-19 00:00:00 GMT",
    ""
);

make_test!(
    thisweek_07,
    1_116_201_600,
    "last monday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_08,
    1_116_288_000,
    "last tuesday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_09,
    1_116_374_400,
    "last wednesday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_10,
    1_116_460_800,
    "last thursday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_11,
    1_115_942_400,
    "last friday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_12,
    1_116_028_800,
    "last saturday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_13,
    1_116_115_200,
    "last sunday",
    "2005-05-20 21:08:14 GMT",
    ""
);

make_test!(
    thisweek_14,
    1_116_806_400,
    "next monday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_15,
    1_116_892_800,
    "next tuesday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_16,
    1_116_979_200,
    "next wednesday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_17,
    1_117_065_600,
    "next thursday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_18,
    1_117_152_000,
    "next friday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_19,
    1_116_633_600,
    "next saturday",
    "2005-05-20 21:08:14 GMT",
    ""
);
make_test!(
    thisweek_20,
    1_116_720_000,
    "next sunday",
    "2005-05-20 21:08:14 GMT",
    ""
);

/* from transition.ts */
make_test!(
    transition_00,
    1_206_835_200,
    "2008-03-30 01:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    transition_01,
    1_206_838_799,
    "2008-03-30 01:59:59",
    "",
    "Europe/Amsterdam"
);
make_test!(
    transition_02,
    1_206_838_800,
    "2008-03-30 02:00:00",
    "",
    "Europe/Amsterdam"
);
make_test!(
    transition_03,
    1_206_842_399,
    "2008-03-30 02:59:59",
    "",
    "Europe/Amsterdam"
);
make_test!(
    transition_04,
    1_206_838_800,
    "2008-03-30 03:00:00",
    "",
    "Europe/Amsterdam"
);

/* from weekdays.ts */
make_test!(
    weekdays_00,
    1_116_633_600,
    "this saturday",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_01,
    1_116_633_600,
    "00:00:00",
    "2005-05-21 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_02,
    1_116_028_800,
    "this saturday ago",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_03,
    1_116_028_800,
    "00:00:00",
    "2005-05-14 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_04,
    1_116_028_800,
    "last saturday",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_05,
    1_116_028_800,
    "00:00:00",
    "2005-05-14 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_06,
    1_116_633_600,
    "last saturday ago",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_07,
    1_116_633_600,
    "00:00:00",
    "2005-05-21 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_08,
    1_116_633_600,
    "first saturday",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_09,
    1_116_633_600,
    "00:00:00",
    "2005-05-21 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_10,
    1_116_028_800,
    "first saturday ago",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_11,
    1_116_028_800,
    "00:00:00",
    "2005-05-14 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_12,
    1_116_633_600,
    "next saturday",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_13,
    1_116_633_600,
    "00:00:00",
    "2005-05-21 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_14,
    1_116_028_800,
    "next saturday ago",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_15,
    1_116_028_800,
    "00:00:00",
    "2005-05-14 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_16,
    1_117_843_200,
    "third saturday",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_17,
    1_117_843_200,
    "00:00:00",
    "2005-06-04 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_18,
    1_114_819_200,
    "third saturday ago",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_19,
    1_114_819_200,
    "00:00:00",
    "2005-04-30 00:00:00 GMT",
    ""
);

make_test!(
    weekdays_20,
    1_116_028_800,
    "previous saturday",
    "2005-05-19 21:08:14 GMT",
    ""
);
make_test!(
    weekdays_21,
    1_116_028_800,
    "00:00:00",
    "2005-05-14 00:00:00 GMT",
    ""
);

/* from week.ts */
make_test!(
    week_00,
    1_208_728_800,
    "this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_01,
    1_208_728_800,
    "this week monday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_02,
    1_208_815_200,
    "this week tuesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_03,
    1_208_901_600,
    "this week wednesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_04,
    1_208_988_000,
    "this week thursday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_05,
    1_209_074_400,
    "this week friday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_06,
    1_209_160_800,
    "this week saturday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_07,
    1_209_247_200,
    "this week sunday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_08,
    1_208_728_800,
    "monday this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_09,
    1_208_815_200,
    "tuesday this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_10,
    1_208_901_600,
    "wednesday this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_11,
    1_208_988_000,
    "thursday this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_12,
    1_209_074_400,
    "friday this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_13,
    1_209_160_800,
    "saturday this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_14,
    1_209_247_200,
    "sunday this week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_15,
    1_208_124_000,
    "last week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_16,
    1_208_124_000,
    "last week monday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_17,
    1_208_210_400,
    "last week tuesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_18,
    1_208_296_800,
    "last week wednesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_19,
    1_208_383_200,
    "thursday last week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_20,
    1_208_469_600,
    "friday last week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_21,
    1_208_556_000,
    "saturday last week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_22,
    1_208_642_400,
    "sunday last week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_23,
    1_208_124_000,
    "previous week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_24,
    1_208_124_000,
    "previous week monday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_25,
    1_208_210_400,
    "previous week tuesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_26,
    1_208_296_800,
    "previous week wednesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_27,
    1_208_383_200,
    "thursday previous week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_28,
    1_208_469_600,
    "friday previous week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_29,
    1_208_556_000,
    "saturday previous week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_30,
    1_208_642_400,
    "sunday previous week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_31,
    1_209_333_600,
    "next week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_32,
    1_209_333_600,
    "next week monday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_33,
    1_209_420_000,
    "next week tuesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_34,
    1_209_506_400,
    "next week wednesday",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_35,
    1_209_592_800,
    "thursday next week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_36,
    1_209_679_200,
    "friday next week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_37,
    1_209_765_600,
    "saturday next week",
    "2008-04-25",
    "Europe/Oslo"
);
make_test!(
    week_38,
    1_209_852_000,
    "sunday next week",
    "2008-04-25",
    "Europe/Oslo"
);

make_test!(
    long_min_0,
    // -9_223_372_036_854_775_808,
    // time library default range is ±10k years
    -377_705_116_800,
    "@-9223372036854775808",
    "now",
    "UTC"
);
make_test!(
    long_min_1,
    // -9_223_372_036_854_775_000,
    // time library default range is ±10k years
    -377_705_116_800,
    "@-9223372036854775000",
    "now",
    "UTC"
);

make_test!(
    long_max_0,
    // 9_223_372_036_854_775_807,
    // time library default range is ±10k years
    253_402_300_799,
    "@9223372036854775807",
    "now",
    "UTC"
);
make_test!(
    long_max_1,
    // 9_223_372_036_854_775_000,
    // time library default range is ±10k years
    253_402_300_799,
    "@9223372036854775000",
    "now",
    "UTC"
);

make_test!(wiki_rs_1, 1_188_345_600, "2007-8-29", "", "");
