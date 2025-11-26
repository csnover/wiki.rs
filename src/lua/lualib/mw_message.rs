//! MediaWiki Scribunto Lua internationalisation support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use regex::bytes::Regex;
use std::sync::LazyLock;

/// The internationalisation support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct MessageLibrary;

impl MessageLibrary {
    /// Checks whether a messages or sequence of messages exist, are blank, or
    /// are disabled.
    ///
    /// The `data` argument is the same as in [`Self::plain`].
    ///
    /// The `what` argument can be one of:
    ///
    /// * 'exists': The message exists in some dictionary
    /// * 'isBlank': The message exists and is not blank
    /// * 'disabled': The message exists and is not blank or disabled ("-")
    fn check<'gc>(
        &self,
        _: Context<'gc>,
        (what, data): (VmString<'gc>, Table<'gc>),
    ) -> Result<bool, VmError<'gc>> {
        log::warn!("stub: mw.message.check({what:?}, {data:?})");
        Ok(true)
    }

    /// Interpolates a message with translation.
    ///
    /// Valid keys for `data` are 'rawMessage', 'keys', 'lang', 'useDB', and
    /// 'params'.
    ///
    /// If `data.rawMessage` is set, its value is treated as the string to
    /// interpolate (similar to GNU gettext).
    ///
    /// If `data.keys` is a sequence, the first valid and non-empty key is used.
    /// If none of the keys are acceptable, the last one is used.
    fn plain<'gc>(&self, ctx: Context<'gc>, data: Table<'gc>) -> Result<Value<'gc>, VmError<'gc>> {
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d+)").unwrap());

        let raw_message = data.get_value(ctx, "rawMessage");

        let Some(params) = data.get::<_, Table<'_>>(ctx, "params").ok() else {
            return Ok(raw_message);
        };

        // TODO: Figure out what to do about all the data that cannot be
        // retrieved from Wikimedia Commons which is required to render
        // correctly.
        if let Ok(keys) = data.get::<_, Table<'gc>>(ctx, "keys")
            && let Ok(first) = keys.get::<_, VmString<'gc>>(ctx, 1)
        {
            let hack = match first.to_str()?.to_ascii_lowercase().as_str() {
                "and" => Some("&#32;and"),
                "comma-separator" => Some(",&#32;"),
                "colon-separator" => Some(":"),
                "dot-separator" => Some("&nbsp;<b>·</b>&#32;"),
                "parentheses" => Some("($1)"),
                "word-separator" => Some("&#32;"),
                _ => None,
            };

            if let Some(hack) = hack {
                return Ok(hack.into_value(ctx));
            }
        }

        let Some(message) = raw_message.into_string(ctx) else {
            return Ok(params
                .iter()
                .next()
                .map_or_else(|| "".into_value(ctx), |(_, v)| v));
        };

        let result = RE.replace_all(&message, |caps: &regex::bytes::Captures<'_>| {
            let key = str::from_utf8(&caps[1]).unwrap().parse::<i64>().unwrap();
            match params.get::<_, VmString<'_>>(ctx, key) {
                Ok(param) => param.to_vec(),
                Err(_) => caps[1].to_vec(),
            }
        });

        Ok(ctx.intern(&result).into())
    }
}

impl MwInterface for MessageLibrary {
    const NAME: &str = "mw.message";
    const CODE: &[u8] = include_bytes!("./modules/mw.message.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            plain = plain,
            check = check,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            lang = "en",
        })
    }
}
