//! MediaWiki Scribunto Lua HTML support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::wikitext::{MARKER_PREFIX, MARKER_SUFFIX};

/// The HTML support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct HtmlLibrary;

impl MwInterface for HtmlLibrary {
    const NAME: &str = "mw.html";
    const CODE: &[u8] = include_bytes!("./modules/mw.html.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        Table::new(&ctx)
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            uniqPrefix = MARKER_PREFIX,
            uniqSuffix = MARKER_SUFFIX
        })
    }
}
