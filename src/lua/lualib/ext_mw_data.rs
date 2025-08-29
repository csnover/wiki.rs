//! MediaWiki `JsonConfig` extension Lua support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-JsonConfig>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;

/// MediaWiki `JsonConfig` extension.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct JCLuaLibrary;

impl JCLuaLibrary {
    /// Gets JSON that conforms to the MediaWiki Commons JSON schema from
    /// the MediaWiki Commons with the given title and language.
    ///
    /// Since Commons data is not included in dumps from other MW installations,
    /// this function can never work in an offline context, and always returns a
    /// stub.
    fn get<'gc>(
        &self,
        ctx: Context<'gc>,
        (title, lang): (VmString<'gc>, Option<VmString<'gc>>),
    ) -> Result<Table<'gc>, VmError<'gc>> {
        log::warn!("stub: JCLuaLibrary.get({title:?}, {lang:?})");
        Ok(table! {
            using ctx;

            data = Table::new(&ctx),
            schema = table! {
                using ctx;

                fields = Table::new(&ctx),
            }
        })
    }
}

impl MwInterface for JCLuaLibrary {
    const NAME: &str = "JCLuaLibrary";
    const CODE: &[u8] = include_bytes!("./modules/ext/JCLuaLibrary.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            get = get,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(Table::new(&ctx))
    }
}
