//! Static data required to render Wikitext.

mod config;

pub use config::CONFIG;
use libwikitext_common::Dictionary;
use std::sync::LazyLock;

/// The English i18n dictionary from MediaWiki.
pub static MESSAGES: LazyLock<Dictionary<'_>> = LazyLock::new(|| {
    serde_json::from_str::<Dictionary<'static>>(include_str!("../../res/i18n/en.json"))
        .unwrap()
        .merge(
            serde_json::from_str(include_str!("../../res/i18n/en-nontranslatable.json")).unwrap(),
        )
});
