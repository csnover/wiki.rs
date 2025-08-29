//! MediaWiki Scribunto Lua URI support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::{TitleLibrary, prelude::*};
use crate::{
    common::anchor_encode,
    wikitext::{Parser, helpers::TextContent, visit::Visitor as _},
};
use gc_arena::Rootable;
use std::cell::{Ref, RefCell};

/// The URI support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(crate) struct UriLibrary {
    /// The Wikitext parser.
    parser: RefCell<Option<Parser<'static>>>,
}

impl UriLibrary {
    /// Encodes the input string for use within a URL fragment-part.
    fn anchor_encode<'gc>(
        &self,
        ctx: Context<'gc>,
        s: VmString<'gc>,
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        let s = s.to_str()?;

        let parser = Ref::filter_map(self.parser.borrow(), Option::as_ref)
            .map_err(|_| "missing parser".into_value(ctx))?;
        let s = {
            let root = parser.parse_no_expansion(s)?;
            let mut extractor = TextContent::new(s, String::new());
            let _ = extractor.visit_output(&root);
            extractor.finish()
        };

        // log::trace!("stub: mw_uri.anchorEncode({s:?}) = {id}");
        Ok(ctx.intern(anchor_encode(&s).as_bytes()))
    }

    /// Gets the fully qualified canonical URL of an article with the given
    /// title text and optional query string.
    fn canonical_url<'gc>(
        &self,
        ctx: Context<'gc>,
        (page, query): (VmString<'gc>, Option<Value<'gc>>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        ctx.singleton::<Rootable![TitleLibrary]>().get_url(
            ctx,
            (
                page,
                VmString::from_static(&ctx, "canonicalUrl"),
                query,
                None,
            ),
        )
    }

    /// Gets the fully qualified protocol-relative URL of an article with the
    /// given title text and optional query string.
    fn full_url<'gc>(
        &self,
        ctx: Context<'gc>,
        (page, query): (VmString<'gc>, Option<Value<'gc>>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        ctx.singleton::<Rootable![TitleLibrary]>().get_url(
            ctx,
            (page, VmString::from_static(&ctx, "fullUrl"), query, None),
        )
    }

    /// Gets the path to an article with the given title text and optional query
    /// string.
    fn local_url<'gc>(
        &self,
        ctx: Context<'gc>,
        (page, query): (VmString<'gc>, Value<'gc>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        ctx.singleton::<Rootable![TitleLibrary]>().get_url(
            ctx,
            (
                page,
                VmString::from_static(&ctx, "localUrl"),
                Some(query),
                None,
            ),
        )
    }

    /// Sets the Wikitext parser.
    pub(crate) fn set_parser(&self, parser: Parser<'static>) {
        *self.parser.borrow_mut() = Some(parser);
    }
}

impl MwInterface for UriLibrary {
    const NAME: &str = "mw.uri";
    const CODE: &[u8] = include_bytes!("./modules/mw.uri.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            anchorEncode = anchor_encode,
            canonicalUrl = canonical_url,
            fullUrl = full_url,
            localUrl = local_url,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            // TODO: Should this be something?
            defaultUrl = Value::Nil,
        })
    }
}
