//! Static data required to render Wikitext.

mod config;

pub use config::CONFIG;
use libwikitext_common::Messages;
use std::sync::LazyLock;

/// The English i18n dictionary from MediaWiki.
pub static MESSAGES: LazyLock<Messages<'_>> = LazyLock::new(|| {
    serde_json::from_str::<Messages<'static>>(include_str!("../../res/i18n/en.json"))
        .unwrap()
        .merge(
            serde_json::from_str(include_str!("../../res/i18n/en-nontranslatable.json")).unwrap(),
        )
});
