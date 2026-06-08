//! MediaWiki Scribunto Lua internationalisation support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use core::cell::Cell;
use libwikitext_common::{Messages, db::DatabaseProvider, format_message, format_raw_message};
use std::borrow::Cow;

/// The internationalisation support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub struct MessageLibrary<'dict> {
    /// A reference to the message dictionary for the requested locale.
    messages: Cell<Option<&'dict Messages<'dict>>>,
}

impl<'dict> MessageLibrary<'dict> {
    /// Returns the message dictionary.
    ///
    /// # Panics
    ///
    /// * The dictionary is not set
    #[inline]
    fn messages(&self) -> &'dict Messages<'dict> {
        self.messages.get().unwrap()
    }

    /// Sets the message dictionary.
    #[inline]
    pub fn set_messages(&self, messages: &'dict Messages<'dict>) {
        self.messages.set(Some(messages));
    }
}

impl<'dict> MessageLibrary<'dict> {
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
        ctx: Context<'gc>,
        (what, data): (VmString<'gc>, Table<'gc>),
    ) -> Result<bool, VmError<'gc>> {
        let message = if let Ok(s) = data.get::<_, VmString<'_>>(ctx, "rawMessage") {
            Some(s.as_bytes())
        } else if let Ok(keys) = data.get::<_, Table<'_>>(ctx, "keys") {
            keys.iter().find_map(|(_, key)| {
                key.into_string(ctx)
                    .and_then(|key| key.to_str().ok())
                    .and_then(|key| self.messages().get(key))
                    .map(str::as_bytes)
            })
        } else {
            None
        };

        Ok(match what.to_str()? {
            "exists" => message.is_some(),
            "isBlank" => message.is_none_or(<[u8]>::is_empty),
            "disabled" => message.is_none_or(|message| message.is_empty() || message == b"-"),
            _ => return Err("invalid what for 'messageCheck'".into_value(ctx))?,
        })
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
    fn plain<'gc>(&self, ctx: Context<'gc>, data: Table<'gc>) -> Result<VmString<'gc>, VmError<'gc>>
    where
        'dict: 'gc,
    {
        let params = data
            .get::<_, Table<'_>>(ctx, "params")
            .unwrap_or_else(|_| Table::new(&ctx));

        let message = if let Ok(message) = data.get::<_, VmString<'_>>(ctx, "rawMessage") {
            format_raw_message(message.to_str()?, make_replacer(ctx, params))?
        } else {
            let keys = data
                .get::<_, Table<'_>>(ctx, "keys")
                .unwrap_or_else(|_| Table::new(&ctx))
                .iter()
                .filter_map(|(_, value)| value.into_string(ctx).and_then(|s| s.to_str().ok()));

            format_message(self.messages(), keys, make_replacer(ctx, params))?
        };

        Ok(ctx.intern(message.as_bytes()))
    }
}

/// Creates a message placeholder callback to retrieve values from the given
/// table.
fn make_replacer<'gc>(
    ctx: Context<'gc>,
    params: Table<'gc>,
) -> impl Fn(&str) -> Result<Option<Cow<'gc, str>>, core::convert::Infallible> {
    move |key| {
        let key = key.parse::<i64>().unwrap();
        Ok(params
            .get::<_, VmString<'gc>>(ctx, key)
            .ok()
            .and_then(|s| s.to_str().ok())
            .map(Cow::Borrowed))
    }
}

impl<'dict: 'static> MwInterface for MessageLibrary<'dict> {
    const CODE: &'static [u8] = include_bytes!("./modules/mw.message.lua");
    const NAME: &'static str = "mw.message";

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            plain = plain,
            check = check,
        }
    }

    fn setup<'gc, Db: DatabaseProvider>(
        &self,
        _: &Db,
        ctx: Context<'gc>,
    ) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            // TODO: Get this information from the dictionary.
            lang = "en",
        })
    }
}
