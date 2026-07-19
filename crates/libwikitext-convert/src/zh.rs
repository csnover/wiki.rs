//! Language conversion for the Chinese language.

use daachorse::{Match, charwise::iter::LeftmostFindIterator};
use icu_locale::Locale;
use std::{borrow::Cow, sync::LazyLock};

/// A language conversion function for the Chinese language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to == ZH_CN || *to == ZH_MY || *to == ZH_SG {
        convert_fast(text, &ZH_TO_CN)
    } else if *to == GAN_HANS || *to == ZH_HANS {
        convert_fast(text, &ZH_TO_HANS)
    } else if *to == GAN_HANT || *to == ZH_HANT {
        convert_fast(text, &ZH_TO_HANT)
    } else if *to == ZH_HK || *to == ZH_MO {
        convert_fast(text, &ZH_TO_HK)
    } else if *to == ZH_TW {
        convert_fast(text, &ZH_TO_TW)
    } else {
        Cow::Borrowed(text)
    }
}

/// Converts text with possible substitutions.
fn convert_fast<'a>(text: &'a str, table: &Table) -> Cow<'a, str> {
    let mut iter = table.leftmost_find_iter(text);
    if let Some(capture) = iter.next() {
        Cow::Owned(convert_slow(text, capture, iter))
    } else {
        Cow::Borrowed(text)
    }
}

/// Converts text with substitutions.
fn convert_slow(
    text: &str,
    first: Match<&str>,
    iter: LeftmostFindIterator<'_, &str, &str>,
) -> String {
    let mut out = String::from(&text[..first.start()]);
    out += first.value();
    let mut last = first.end();
    for capture in iter {
        out += &text[last..capture.start()];
        out += capture.value();
        last = capture.end();
    }
    out += &text[last..];
    out
}

/// The type of the aho-corasick string scanner.
type Table = daachorse::CharwiseDoubleArrayAhoCorasick<&'static str>;

/// Generates aho-corasick string scanners for raw tables with the given
/// `$ident`s.
macro_rules! tables {
    ($($ident:ident),* $(,)?) => {
        $(pub static $ident: LazyLock<Table> = LazyLock::new(|| {
            daachorse::CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(daachorse::MatchKind::LeftmostLongest)
                .build_with_values(raw::$ident.iter().copied())
                .unwrap()
            });
        )*
    }
}

/// Gan Chinese Simplified locale.
const GAN_HANS: Locale = icu_locale::locale!("gan-hans");

/// Gan Chinese Traditional locale.
const GAN_HANT: Locale = icu_locale::locale!("gan-hant");

/// Chinese Mainland locale.
const ZH_CN: Locale = icu_locale::locale!("zh-cn");

/// Chinese Simplified locale.
const ZH_HANS: Locale = icu_locale::locale!("zh-hans");

/// Chinese Traditional locale.
const ZH_HANT: Locale = icu_locale::locale!("zh-hant");

/// Chinese Hong Kong locale.
const ZH_HK: Locale = icu_locale::locale!("zh-hk");

/// Chinese Macao locale.
const ZH_MO: Locale = icu_locale::locale!("zh-mo");

/// Chinese Malaysia locale.
const ZH_MY: Locale = icu_locale::locale!("zh-my");

/// Chinese Singapore locale.
const ZH_SG: Locale = icu_locale::locale!("zh-sg");

/// Chinese Taiwan locale.
const ZH_TW: Locale = icu_locale::locale!("zh-tw");

tables!(ZH_TO_CN, ZH_TO_HANT, ZH_TO_HANS, ZH_TO_HK, ZH_TO_TW);

/// Raw substitution tables.
mod raw {
    include!(concat!(env!("OUT_DIR"), "/libwikitext_convert_zhtables.rs"));
}
