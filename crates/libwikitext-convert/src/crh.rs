//! Language conversion for the Crimean Tatar language.

use icu_locale::{Locale, locale};
use std::borrow::Cow;

/// A language conversion function for the Crimean Tatar language.
pub fn convert<'a>(text: &'a str, to: &Locale) -> Cow<'a, str> {
    if *to == CRH_CYRL {
        log::warn!("TODO: crh-Cyrl");
        Cow::Borrowed(text)
    } else if *to == CRH_LATN {
        log::warn!("TODO: crh-Latn");
        Cow::Borrowed(text)
    } else {
        Cow::Borrowed(text)
    }
}

/// Crimean Tatar Cyrillic locale.
const CRH_CYRL: Locale = locale!("crh-cyrl");

/// Crimean Tatar Latin locale.
const CRH_LATN: Locale = locale!("crh-latn");
