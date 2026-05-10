//! Static data required to render Wikitext.

mod config;

pub use config::CONFIG;
use std::sync::LazyLock;

/// The English i18n dictionary from MediaWiki.
pub static MESSAGES: LazyLock<serde_json_borrow::Value<'_>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../res/i18n/en.json")).unwrap());
