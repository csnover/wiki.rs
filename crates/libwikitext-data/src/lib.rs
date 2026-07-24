//! Static data required to render Wikitext.

mod config;

pub use config::CONFIG;
use libwikitext_common::Dictionary;
use std::sync::LazyLock;

/// A set of defined date formatting strings for a locale.
pub struct DateFormats<'a> {
    /// The format for date and time.
    pub both: Option<&'a str>,
    /// The format for date only.
    pub date: Option<&'a str>,
    /// The format for year and month only.
    pub month_only: Option<&'a str>,
    /// The format for a ‘pretty’ date.
    pub pretty: Option<&'a str>,
    /// The format for time only.
    pub time: Option<&'a str>,
}

/// Locale-specific date formats from MediaWiki.
// TODO: Actually get all of these, including the alternate forms, and the
// inheritances.
pub static DATE_FORMATS: phf::Map<&str, DateFormats<'static>> = phf::phf_map! {
    "en" => DateFormats {
        both: Some("H:i, j F Y"),
        date: Some("j F Y"),
        month_only: Some("F Y"),
        pretty: Some("j F"),
        time: Some("H:i"),
    },
    "ja" => DateFormats {
        both: Some("Y年n月j日 (D) H:i"),
        date: Some("Y年n月j日 (D)"),
        month_only: None,
        pretty: None,
        time: Some("H:i"),
    }
};

/// The English i18n dictionary from MediaWiki.
// TODO: Actually get all dictionaries.
pub static MESSAGES: LazyLock<Dictionary<'_>> = LazyLock::new(|| {
    serde_json::from_str::<Dictionary<'static>>(include_str!("../i18n/en.json"))
        .unwrap()
        .merge(serde_json::from_str(include_str!("../i18n/en-nontranslatable.json")).unwrap())
        .merge(serde_json::from_str(include_str!("../i18n/parser-functions/en.json")).unwrap())
});
