//! MediaWiki Scribunto Lua text support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::wikitext::{MARKER_PREFIX, MARKER_SUFFIX};
use piccolo::StashedTable;
use regex::bytes::{Regex, RegexBuilder};
use std::{borrow::Cow, cell::RefCell, sync::LazyLock};

/// The text support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub struct TextLibrary {
    /// Cached HTML entity translation table.
    entity_table: RefCell<Option<StashedTable>>,
}

impl TextLibrary {
    /// Returns a translation table for converting HTML entities to character
    /// literals.
    fn get_entity_table<'gc>(&self, ctx: Context<'gc>, (): ()) -> Result<Table<'gc>, VmError<'gc>> {
        // log::trace!("stub: mw_text.getEntityTable()");
        if self.entity_table.borrow().is_none() {
            let table = Table::new(&ctx);
            for (name, value) in html_escape::NAMED_ENTITIES {
                table.set(ctx, format!("&{};", name.escape_ascii()), value)?;
            }
            *self.entity_table.borrow_mut() = Some(ctx.stash(table));
        }

        let table = ctx.fetch(self.entity_table.borrow().as_ref().unwrap());
        Ok(table)
    }

    /// Converts a JSON string into a Lua value.
    fn json_decode<'gc>(
        &self,
        ctx: Context<'gc>,
        (value, _flags): (VmString<'_>, Value<'_>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        let ser = piccolo_util::serde::ser::Serializer::new(ctx, <_>::default());
        let mut deser = serde_json::Deserializer::from_slice(value.as_bytes());
        Ok(serde_transcode::transcode(&mut deser, ser)?)
    }

    /// Converts a Lua value into a JSON string.
    fn json_encode<'gc>(
        &self,
        ctx: Context<'gc>,
        (value, _flags): (Value<'_>, Value<'_>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        let deser = piccolo_util::serde::de::Deserializer::from_value(value);
        let mut ser = serde_json::Serializer::new(vec![]);
        serde_transcode::transcode(deser, &mut ser)?;
        Ok(ctx.intern(&ser.into_inner()))
    }

    /// Removes all strip markers from the text.
    fn kill_markers<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'gc>,
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        static RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(&format!("{MARKER_PREFIX}([^\x7f<>&'\"]+){MARKER_SUFFIX}")).unwrap()
        });

        Ok(match RE.replace_all(text.as_bytes(), b"") {
            Cow::Borrowed(_) => text,
            Cow::Owned(text) => ctx.intern(&text),
        })
    }

    /// Replaces strip markers inside `<nowiki>` tags with their original text?
    fn unstrip<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'gc>,
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        // log::warn!("stub: mw.text.unstrip({s:?})");
        // stripState.killMarkers(stripState.unstripNoWiki(text))
        self.kill_markers(ctx, self.unstrip_no_wiki(ctx, (text, Some(true)))?)
    }

    /// Replaces strip markers inside `<nowiki>` tags with their original text
    /// and removes the tags?
    fn unstrip_no_wiki<'gc>(
        &self,
        ctx: Context<'gc>,
        (text, get_orig_text_when_preprocessing): (VmString<'gc>, Option<bool>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        // log::warn!("stub: mw.text.unstripNoWiki({s:?}, {get_orig_text_when_preprocessing:?})");

        if get_orig_text_when_preprocessing == Some(true) {
            // stripState.unstripNoWiki(s)
            Ok(text)
        } else {
            static RE: LazyLock<Regex> = LazyLock::new(|| {
                RegexBuilder::new("</?nowiki[^>]*>")
                    .case_insensitive(true)
                    .build()
                    .unwrap()
            });
            // stripState.replaceNoWikis(|s| {
            //     let s = RE.replace_all(s, "");
            //     if s.is_empty() {
            //         ""
            //     } else {
            //         CoreTagHooks::nowiki(s, [], parser)[0]
            //     }
            // })
            Ok(match RE.replace_all(text.as_bytes(), b"") {
                Cow::Borrowed(_) => text,
                Cow::Owned(text) => ctx.intern(&text),
            })
        }
    }
}

impl MwInterface for TextLibrary {
    const NAME: &str = "mw.text";
    const CODE: &[u8] = include_bytes!("./modules/mw.text.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            getEntityTable = get_entity_table,
            jsonDecode = json_decode,
            jsonEncode = json_encode,
            killMarkers = kill_markers,
            unstrip = unstrip,
            unstripNoWiki = unstrip_no_wiki,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            and = " and ",
            comma = ", ",
            ellipsis = "…",
            nowiki_protocols = Table::new(&ctx),
        })
    }
}
