//! MediaWiki `ParserFunctions` extension Lua support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-ParserFunctions>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::expr::do_expression;

/// MediaWiki `ParserFunctions` extension.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct LuaLibrary;

impl LuaLibrary {
    /// Evaluates a mathematical expression and returns the result as a number.
    fn expr<'gc>(&self, _: Context<'gc>, expr: VmString<'gc>) -> Result<f64, VmError<'gc>> {
        Ok(do_expression(expr.to_str()?)?.unwrap_or(0.0))
    }
}

impl MwInterface for LuaLibrary {
    const NAME: &str = "mw.ext.ParserFunctions";
    const CODE: &[u8] = include_bytes!("./modules/ext/mw.ext.ParserFunctions.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            expr = expr,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(Table::new(&ctx))
    }
}
