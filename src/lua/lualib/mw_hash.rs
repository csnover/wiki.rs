//! MediaWiki Scribunto Lua cryptographic hash support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;

/// The cryptographic hash support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct HashLibrary;

impl HashLibrary {
    mw_unimplemented! {
        hashValue = hash_value,
        listAlgorithms = list_algorithms,
    }
}

impl MwInterface for HashLibrary {
    const NAME: &str = "mw.hash";
    const CODE: &[u8] = include_bytes!("./modules/mw.hash.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            hashValue = hash_value,
            listAlgorithms = list_algorithms,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(Table::new(&ctx))
    }
}
