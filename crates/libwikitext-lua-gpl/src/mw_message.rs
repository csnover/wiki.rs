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
#[derive(gc_arena::Collect)]
#[collect(require_static)]
pub struct MessageLibrary<'dict, Db> {
    /// A reference to the message dictionary for the requested locale.
    messages: Cell<Option<&'dict Messages<'dict, Db>>>,
}

impl<Db> Default for MessageLibrary<'_, Db> {
    fn default() -> Self {
        Self {
            messages: <_>::default(),
        }
    }
}

impl<'dict, Db> MessageLibrary<'dict, Db> {
    /// Returns the message dictionary.
    ///
    /// # Panics
    ///
    /// * The dictionary is not set
    #[inline]
    fn messages(&self) -> &'dict Messages<'dict, Db> {
        self.messages.get().unwrap()
    }

    /// Sets the message dictionary.
    #[inline]
    pub fn set_messages(&self, messages: &'dict Messages<'dict, Db>) {
        self.messages.set(Some(messages));
    }
}

impl<'dict, Db> MessageLibrary<'dict, Db>
where
    Db: DatabaseProvider,
    for<'gc> VmError<'gc>: From<Db::Error>,
{
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
            Some(Cow::Borrowed(s.to_str()?))
        } else {
            let (keys, lang, use_db) = message_options(ctx, data)?;
            let mut message = None;
            for key in keys {
                message = self.messages().get(key, lang, use_db)?;
                if message.is_some() {
                    break;
                }
            }
            message
        };
        let message = message.as_deref();

        Ok(match what.to_str()? {
            "exists" => message.is_some(),
            "isBlank" => message.is_none_or(str::is_empty),
            "disabled" => message.is_none_or(|message| message.is_empty() || message == "-"),
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

        let replacer = |key: &str| {
            let key = key.parse::<i64>().unwrap();
            Ok(params
                .get::<_, VmString<'_>>(ctx, key)
                .ok()
                .and_then(|s| s.to_str().ok())
                .map(Cow::Borrowed))
        };

        let message = if let Ok(message) = data.get::<_, VmString<'_>>(ctx, "rawMessage") {
            format_raw_message(message.to_str()?, replacer)?
        } else {
            let (keys, lang, use_db) = message_options(ctx, data)?;
            format_message(self.messages(), lang, use_db, keys, replacer)?
        };

        Ok(ctx.intern(message.as_bytes()))
    }
}

/// Extracts message formatting options from a table.
fn message_options<'gc>(
    ctx: Context<'gc>,
    data: Table<'gc>,
) -> Result<(impl Iterator<Item = &'gc str>, Option<&'gc str>, bool), VmError<'gc>> {
    let keys = data
        .get::<_, Table<'_>>(ctx, "keys")
        .unwrap_or_else(|_| Table::new(&ctx))
        .iter()
        .filter_map(move |(_, value)| value.into_string(ctx).and_then(|s| s.to_str().ok()));

    let lang = data
        .get::<_, Option<VmString<'_>>>(ctx, "lang")?
        .map(VmString::to_str)
        .transpose()?;

    let use_db = data.get::<_, bool>(ctx, "useDB").unwrap_or(false);

    Ok((keys, lang, use_db))
}

impl<'dict: 'static, Db> MwInterface for MessageLibrary<'dict, Db>
where
    Db: DatabaseProvider,
    for<'gc> VmError<'gc>: From<Db::Error>,
{
    const CODE: &'static [u8] = include_bytes!("./modules/mw.message.lua");
    const NAME: &'static str = "mw.message";

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            plain = plain,
            check = check,
        }
    }

    fn setup<'gc, SetupDb: DatabaseProvider>(
        &self,
        messages: &Messages<'_, SetupDb>,
        ctx: Context<'gc>,
    ) -> Result<Table<'gc>, RuntimeError> {
        let lang = VmString::from_static(&ctx, messages.db().config().language);
        Ok(table! {
            using ctx;
            lang = lang,
        })
    }
}
