//! MediaWiki Scribunto Lua localisation support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use core::cell::{Cell, RefCell};
use gc_arena::Rootable;
use libmisc::{to_lower, to_upper};
use libphp_rs::strval;
use libwikitext_common::{
    FormatNumberError, Messages, bcp47_to_lang, db::DatabaseProvider, format_date_mediawiki,
    parse_formatted_number, to_lower_first, to_upper_first,
};
use libwikitext_lua::WallTime;
use piccolo::StashedString;

/// The localisation support library.
// TODO: Actually support all the languages.
#[derive(gc_arena::Collect)]
#[collect(require_static)]
pub struct LanguageLibrary<'dict, Db> {
    /// The content language code.
    ///
    /// This is held separately because this information is requested indirectly
    /// by the module setup function so must be available before `messages`
    /// is set.
    // TODO: Try to get rid of this without making the config permanently
    // 'static.
    content_language_code: RefCell<Option<StashedString>>,
    /// A reference to the message dictionary for the current locale.
    messages: Cell<Option<&'dict Messages<'dict, Db>>>,
}

impl<Db> Default for LanguageLibrary<'_, Db> {
    #[inline]
    fn default() -> Self {
        Self {
            content_language_code: <_>::default(),
            messages: <_>::default(),
        }
    }
}

impl<'dict, Db> LanguageLibrary<'dict, Db> {
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

impl<Db> LanguageLibrary<'_, Db>
where
    Db: DatabaseProvider,
{
    mw_unimplemented! {
        caseFold = case_fold,
        convertGrammar = convert_grammar,
        formatDuration = format_duration,
        gender = gender,
        getFallbacksFor = get_fallbacks_for,
        isSupportedLanguage = is_supported_language,
        isValidBuiltInCode = is_valid_built_in_code,
        isValidCode = is_valid_code,
        toBcp47Code = to_bcp47_code,
    }

    /// Chooses the correct plural form for the number `n` from the given list
    /// of possible `forms` for the language with the given language `code`.
    fn convert_plural<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, n, forms): (VmString<'gc>, i64, Table<'gc>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        log::warn!("stub: mw.language.convertPlural({code:?}, {n:?}, {forms:?})");
        Ok(if let value @ Value::String(_) = forms.get_value(ctx, n) {
            value
        } else {
            forms.get_value(ctx, forms.length())
        })
    }

    /// Returns the name of the language matching the given MediaWiki language
    /// `code`. If `in_language` is provided, the name is localised to that
    /// language; otherwise, the native name of the language is used.
    fn fetch_language_name<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, in_language): (VmString<'gc>, Option<VmString<'gc>>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        let code = code.to_str()?;
        let in_language = in_language.map(VmString::to_str).transpose()?;

        Ok(VmString::from_static(
            &ctx,
            if let Some(lang) = self.messages().db().config().languages.get(code) {
                if let Some(to_code) = in_language {
                    if to_code != "en" {
                        log::warn!("What? There are languages beyond English?");
                    }
                    lang.name
                } else {
                    lang.autonym
                }
            } else {
                ""
            },
        ))
    }

    /// Returns a table of `String(language code): String(language name)`. If
    /// `in_language` is provided, the language names are localised to that
    /// language; otherwise, the native names of each language are used.
    ///
    /// If `include` is provided:
    ///
    /// * 'all': return all known languages;
    /// * 'mw': return languages enabled in MediaWiki;
    /// * 'mwfile': return enabled languages with message files in MediaWiki
    fn fetch_language_names<'gc>(
        &self,
        ctx: Context<'gc>,
        (in_language, include): (Option<VmString<'gc>>, Option<VmString<'gc>>),
    ) -> Result<Table<'gc>, VmError<'gc>> {
        let in_language = in_language.map(VmString::to_str).transpose()?;
        let include = include.map(VmString::to_str).transpose()?;

        let names = Table::new(&ctx);
        for (code, lang) in &self.messages().db().config().languages {
            let add = match include {
                Some("all") => true,
                // TODO: Add more languages, I guess
                Some("mwfile") => *code == "en",
                _ => lang.is_enabled,
            };
            if !add {
                continue;
            }

            let code = ctx.intern_static(code.as_bytes());
            let name = ctx.intern_static(
                in_language
                    .map_or(lang.autonym, |to_code| {
                        if to_code != "en" {
                            log::warn!("What? There are languages beyond English?");
                        }
                        lang.name
                    })
                    .as_bytes(),
            );

            names.set(ctx, code, name)?;
        }
        Ok(names)
    }

    /// Formats a date according to the locale given in `code`. If `local` is
    /// true, the output date is converted to the local time zone; otherwise, it
    /// is given in UTC.
    fn format_date<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, format, date, local): (
            VmString<'gc>,
            VmString<'gc>,
            Option<VmString<'gc>>,
            Option<bool>,
        ),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        if code != "en" {
            log::warn!("stub: mw.language.formatDate({code:?}, {format:?}, {date:?}), {local:?}");
        }
        Ok(format_date_mediawiki(
            &ctx.singleton::<Rootable![WallTime]>().get(),
            format.to_str()?,
            date.map(VmString::to_str).transpose()?,
            local == Some(true),
        )?
        .into_value(ctx))
    }

    /// Formats a number according to the rules of the locale given in `code`.
    fn format_num<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, n, options): (VmString<'gc>, f64, Option<Table<'gc>>),
    ) -> Result<VmString<'gc>, VmError<'gc>>
    where
        VmError<'gc>: From<FormatNumberError<Db::Error>>,
    {
        let no_separators = if let Some(options) = options {
            options.get_value(ctx, "noCommafy").to_bool()
        } else {
            false
        };

        Ok(ctx.intern(
            self.messages()
                .format_number(Some(code.to_str()?), n, no_separators)?
                .as_bytes(),
        ))
    }

    /// Returns the default language code for the wiki.
    fn get_cont_lang_code<'gc>(
        &self,
        ctx: Context<'gc>,
        (): (),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        Ok(ctx.fetch(self.content_language_code.borrow().as_ref().unwrap()))
    }

    /// Splits a duration, in seconds, into a table of larger time intervals.
    fn get_duration_intervals<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, mut seconds, chosen_intervals): (VmString<'gc>, f64, Option<Table<'gc>>),
    ) -> Result<Table<'gc>, VmError<'gc>> {
        const INTERVALS: &[&str] = &[
            "millennia",
            "centuries",
            "decades",
            "years",
            "months",
            "days",
            "hours",
            "minutes",
            "seconds",
        ];

        if code != "en" {
            log::warn!(
                "stub: mw.language.getDurationIntervals({code:?}, {seconds:?}, {chosen_intervals:?})"
            );
        }

        // TODO: :-(((((((
        let (intervals, smallest_key) = chosen_intervals.map_or_else(
            || {
                Ok::<_, VmError<'gc>>((
                    either::Left(
                        // `months` were not part of the original default
                        [
                            "millennia",
                            "centuries",
                            "decades",
                            "years",
                            "days",
                            "hours",
                            "minutes",
                            "seconds",
                        ]
                        .into_iter()
                        .map(Ok),
                    ),
                    "seconds",
                ))
            },
            |intervals| {
                let mut best = 0;

                for (_, value) in intervals {
                    if let Some(value) = value.into_string(ctx) {
                        let value = value.to_str()?;
                        if let Some(candidate) = INTERVALS[best + 1..].iter().enumerate().find_map(
                            |(index, interval)| (*interval == value).then_some(index + best + 1),
                        ) && candidate > best
                        {
                            best = candidate;
                        }
                    }
                }

                Ok((
                    either::Right(
                        intervals
                            .into_iter()
                            .filter_map(|(_, value)| value.into_string(ctx).map(VmString::to_str)),
                    ),
                    INTERVALS[best],
                ))
            },
        )?;

        let segments = Table::new(&ctx);
        for key in intervals {
            let key = key?;
            let epoch = match key {
                "millennia" => 1000.0 * 31_556_952.0,
                "centuries" => 100.0 * 31_556_952.0,
                "decades" => 10.0 * 31_556_952.0,
                // The average year is 365.2425 days (365 + (24 * 3 + 25) / 400)
                "years" => 31_556_952.0, // 365.2425 * 24 * 3600
                // To simplify, we consider a month to be 1/12 of a year
                "months" => 365.2425 * 24.0 * 3600.0 / 12.0,
                "days" => 24.0 * 3600.0,
                "hours" => 3600.0,
                "minutes" => 60.0,
                "seconds" => 1.0,
                _ => continue,
            };
            let value = (seconds / epoch).floor();
            if value > 0.0 || (key == smallest_key && segments.length() == 0) {
                seconds -= value * epoch;
                segments.set(ctx, ctx.intern(key.as_bytes()), value)?;
            }
        }

        // log::trace!("mw.language.getDurationIntervals(.., {seconds:?}, {chosen_intervals:?}) = {segments:?}");

        Ok(segments)
    }

    /// Returns true if the given string is a language code known to MediaWiki.
    fn is_known_language_tag<'gc>(
        &self,
        _: Context<'_>,
        code: VmString<'gc>,
    ) -> Result<bool, VmError<'gc>> {
        // log::trace!("mw.language.isKnownLanguageTag({code:?})");
        let code = bcp47_to_lang(code.to_str()?);
        Ok(self.messages().db().config().languages.contains_key(&code))
    }

    /// Returns true if the language with the given language code is written
    /// right-to-left.
    fn is_rtl<'gc>(&self, _: Context<'gc>, code: VmString<'gc>) -> Result<bool, VmError<'gc>> {
        Ok(self
            .messages()
            .db()
            .config()
            .languages
            .get(code.to_str()?)
            .is_some_and(|lang| lang.is_rtl))
    }

    /// Converts a string to lowercase according to the rules of the given
    /// language.
    fn lc<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        if code != "en" {
            log::warn!("stub: mw.language.lc({code:?}, {text:?})");
        }
        Ok(ctx.intern(to_lower(text.to_str()?).as_bytes()))
    }

    /// Converts the first letter of a string to lowercase according to the
    /// rules of the given language.
    fn lcfirst<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        if code != "en" {
            log::warn!("stub: mw.language.lcfirst({code:?}, {text:?})");
        }
        Ok(ctx.intern(to_lower_first(text.to_str()?).as_bytes()))
    }

    /// Parses a number formatted according to the rules of the language given
    /// in `code` back into a machine-readable number.
    fn parse_formatted_number<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, value): (VmString<'_>, Value<'gc>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        if code != "en" {
            log::warn!("stub: mw.language.parseFormattedNumber({value:?})");
        }
        // One might think that this would return `Value::Number` but actually
        // it is supposed to return strings…
        let s = match value {
            Value::Integer(i) => format!("{i}").into(),
            Value::Number(n) => strval(n).into(),
            Value::String(s) => parse_formatted_number(s.to_str()?),
            _ => return Ok(Value::Nil),
        };

        Ok(ctx.intern(s.as_bytes()).into())
    }

    /// Converts a string to uppercase according to the rules of the given
    /// language.
    fn uc<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        if code != "en" {
            log::warn!("stub: mw.language.uc({code:?}, {text:?})");
        }
        Ok(ctx.intern(to_upper(text.to_str()?).as_bytes()))
    }

    /// Converts the first letter of a string to uppercase according to the
    /// rules of the given language.
    fn ucfirst<'gc>(
        &self,
        ctx: Context<'gc>,
        (code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        if code != "en" {
            log::warn!("stub: mw.language.ucfirst({code:?}, {text:?})");
        }
        Ok(ctx.intern(to_upper_first(text.to_str()?).as_bytes()))
    }
}

impl<'db: 'static, Db> MwInterface for LanguageLibrary<'db, Db>
where
    Db: DatabaseProvider,
    Db::Error: core::error::Error + Send + Sync,
    for<'gc> VmError<'gc>: From<Db::Error>,
{
    const CODE: &'static [u8] = include_bytes!("./modules/mw.language.lua");
    const NAME: &'static str = "mw.language";

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            caseFold = case_fold,
            convertGrammar = convert_grammar,
            convertPlural = convert_plural,
            fetchLanguageName = fetch_language_name,
            fetchLanguageNames = fetch_language_names,
            formatDate = format_date,
            formatDuration = format_duration,
            formatNum = format_num,
            gender = gender,
            getContLangCode = get_cont_lang_code,
            getDurationIntervals = get_duration_intervals,
            getFallbacksFor = get_fallbacks_for,
            isKnownLanguageTag = is_known_language_tag,
            isRTL = is_rtl,
            isSupportedLanguage = is_supported_language,
            isValidBuiltInCode = is_valid_built_in_code,
            isValidCode = is_valid_code,
            lc = lc,
            lcfirst = lcfirst,
            parseFormattedNumber = parse_formatted_number,
            toBcp47Code = to_bcp47_code,
            uc = uc,
            ucfirst = ucfirst,
        }
    }

    fn setup<'gc, SetupDb: DatabaseProvider>(
        &self,
        messages: &Messages<'_, SetupDb>,
        ctx: Context<'gc>,
    ) -> Result<Table<'gc>, RuntimeError> {
        let lang = VmString::from_static(&ctx, messages.db().config().language);
        *self.content_language_code.borrow_mut() = Some(ctx.stash(lang));
        Ok(Table::new(&ctx))
    }
}
