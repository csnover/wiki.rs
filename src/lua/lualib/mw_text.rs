//! MediaWiki Scribunto Lua text support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::{
    lua::{HostCall, UnstripMode},
    php::strtr,
    renderer::{State, StripMarker, StripMarkers},
};
use piccolo::{ExternError, Stack, StashedString, StashedTable, UserData};
use std::{borrow::Cow, cell::RefCell};

/// The text support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(crate) struct TextLibrary {
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
        Ok(match StripMarkers::kill(text.to_str()?) {
            Cow::Borrowed(_) => text,
            Cow::Owned(text) => ctx.intern(text.as_bytes()),
        })
    }

    /// Replaces stripped `<nowiki>` tags with their original text and removes
    /// all other strip markers.
    fn unstrip<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let text = stack.consume::<VmString<'gc>>(ctx)?;
        // log::trace!("mw.text.unstrip({text:?})");
        stack.replace(
            ctx,
            UserData::new_static(
                &ctx,
                HostCall::Unstrip {
                    text: ctx.stash(text),
                    mode: UnstripMode::Unstrip,
                },
            ),
        );
        Ok(CallbackReturn::Yield {
            to_thread: None,
            then: None,
        })
    }

    /// Replaces stripped `<nowiki>` tags with their original text and retains
    /// other strip markers, optionally escaping the returned `<nowiki>` text.
    fn unstrip_no_wiki<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let (text, get_orig_text_when_preprocessing) =
            stack.consume::<(VmString<'gc>, Option<bool>)>(ctx)?;
        // log::trace!("mw.text.unstripNoWiki({text:?}, {get_orig_text_when_preprocessing:?})");
        stack.replace(
            ctx,
            UserData::new_static(
                &ctx,
                HostCall::Unstrip {
                    text: ctx.stash(text),
                    mode: if get_orig_text_when_preprocessing == Some(true) {
                        UnstripMode::OrigText
                    } else {
                        UnstripMode::UnstripNoWiki
                    },
                },
            ),
        );
        Ok(CallbackReturn::Yield {
            to_thread: None,
            then: None,
        })
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
            ~unstrip = unstrip,
            ~unstripNoWiki = unstrip_no_wiki,
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

/// Replaces `<nowiki>` markers in the given `text`, optionally in an encoded
/// form, and optionally removing other markers.
///
/// This runs outside of the Lua VM to avoid having to wrap `StripMarkers` in
/// `Rc<RefCell>`.
pub(super) fn unstrip(
    state: &mut State<'_>,
    text: &StashedString,
    mode: UnstripMode,
) -> Result<StashedString, ExternError> {
    state.statics.vm.try_enter(|ctx| {
        let text = ctx.fetch(text);

        let result = match mode {
            UnstripMode::OrigText => {
                state
                    .strip_markers
                    .for_each_marker(text.to_str()?, |marker| {
                        if let StripMarker::NoWiki(text) = marker {
                            Some(Cow::Borrowed(text))
                        } else {
                            None
                        }
                    })
            }
            UnstripMode::UnstripNoWiki => {
                state
                    .strip_markers
                    .for_each_marker(text.to_str()?, |marker| {
                        if let StripMarker::NoWiki(text) = marker {
                            // TODO: This is supposed to be a call to the
                            // `<nowiki>` extension tag; this should be doing
                            // the equivalent thing, but is it?
                            Some(strtr(
                                text,
                                &[
                                    ("-{", "-&#123;"),
                                    ("}-", "&#125;-"),
                                    ("<", "&lt;"),
                                    (">", "&gt;"),
                                ],
                            ))
                        } else {
                            None
                        }
                    })
            }
            UnstripMode::Unstrip => state
                .strip_markers
                .for_each_marker(text.to_str()?, |marker| {
                    Some(Cow::Borrowed(if let StripMarker::NoWiki(text) = marker {
                        text
                    } else {
                        ""
                    }))
                }),
        };

        Ok(ctx.stash(match result {
            Cow::Borrowed(_) => text,
            Cow::Owned(text) => ctx.intern(text.as_bytes()),
        }))
    })
}
