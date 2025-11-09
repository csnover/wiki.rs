//! MediaWiki Scribunto Lua localisation support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::{common::format_date, php::format_number};
use std::cell::Cell;
use time::UtcDateTime;

/// The localisation support library.
#[derive(gc_arena::Collect)]
#[collect(require_static)]
pub(crate) struct LanguageLibrary {
    /// The “current” time.
    date: Cell<UtcDateTime>,
}

impl LanguageLibrary {
    /// Sets the date which shall be considered the current time.
    pub(crate) fn set_date(&self, date: UtcDateTime) {
        self.date.set(date);
    }

    mw_unimplemented! {
        caseFold = case_fold,
        convertGrammar = convert_grammar,
        formatDuration = format_duration,
        gender = gender,
        getFallbacksFor = get_fallbacks_for,
        isSupportedLanguage = is_supported_language,
        isValidCode = is_valid_code,
        isValidBuiltInCode = is_valid_built_in_code,
        toBcp47Code = to_bcp47_code,
    }

    /// Chooses the correct plural form for the number `n` from the given list
    /// of possible `forms` for the language with the given BCP 47 `code`.
    fn convert_plural<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, n, forms): (VmString<'gc>, i64, Table<'gc>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::warn!("stub: mw.language.convertPlural({_code:?}, {n:?}, {args:?})");
        Ok(if let value @ Value::String(_) = forms.get_value(ctx, n) {
            value
        } else {
            forms.get_value(ctx, forms.length() - 1)
        })
    }

    /// Returns the name of the language matching the given BCP 47 `code`. If
    /// `in_language` is provided, the name is localised to that language;
    /// otherwise, the native name of the language is used.
    fn fetch_language_name<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, _in_language): (VmString<'gc>, Option<VmString<'gc>>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::trace!("stub: fetchLanguageName({code:?}, {in_language:?})");
        Ok("English".into_value(ctx))
    }

    /// Returns a table of `String(BCP 47 code): String(language name)`. If
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
        (_in_language, _include): (Option<VmString<'gc>>, Option<VmString<'gc>>),
    ) -> Result<Table<'gc>, VmError<'gc>> {
        // log::trace!("stub: fetchLanguageNames({in_language:?}, {include:?})");
        Ok(table! {
            using ctx;

            en = "English".into_value(ctx),
        })
    }

    /// Formats a date according to the locale given in `code`. If `local` is
    /// true, the output date is converted to the local time zone; otherwise, it
    /// is given in UTC.
    fn format_date<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, format, date, local): (
            VmString<'gc>,
            VmString<'gc>,
            Option<VmString<'gc>>,
            Option<bool>,
        ),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        Ok(format_date(
            &self.date.get().into(),
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
        (_code, n, options): (VmString<'gc>, f64, Option<Table<'gc>>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        // log::trace!("formatNum({code:?}, {n:?}, {options:?})");

        let no_separators = if let Some(options) = options {
            options.get_value(ctx, "noCommafy").to_bool()
        } else {
            false
        };

        Ok(ctx.intern(format_number(n, no_separators).as_bytes()))
    }

    /// Returns the default BCP 47 code for the wiki.
    fn get_cont_lang_code<'gc>(
        &self,
        ctx: Context<'gc>,
        (): (),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::warn!("stub: getContLangCode()");
        Ok("en".into_value(ctx))
    }

    /// Splits a duration, in seconds, into a table of larger time intervals.
    fn get_duration_intervals<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, mut seconds, chosen_intervals): (VmString<'gc>, f64, Option<Table<'gc>>),
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

    /// Returns true if the given string is a BCP 47 code known to MediaWiki.
    fn is_known_language_tag<'gc>(
        &self,
        _: Context<'_>,
        lang: VmString<'gc>,
    ) -> Result<bool, VmError<'gc>> {
        log::warn!("stub: mw.language.isKnownLanguageTag({lang:?})");
        Ok(true)
    }

    /// Returns true if the language with the given BCP 47 code is written
    /// right-to-left.
    fn is_rtl<'gc>(&self, _: Context<'gc>, _code: VmString<'gc>) -> Result<bool, VmError<'gc>> {
        // log::trace!("stub: isRTL()");
        Ok(false)
    }

    /// Converts a string to lowercase according to the rules of the given
    /// language.
    fn lc<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        Ok(ctx.intern(text.to_str()?.to_lowercase().as_bytes()))
    }

    /// Converts the first letter of a string to lowercase according to the
    /// rules of the given language.
    fn lcfirst<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        let mut text = text.to_str()?.chars();
        Ok(if let Some(first) = text.next() {
            ctx.intern(format!("{}{}", first.to_lowercase(), text.as_str()).as_bytes())
        } else {
            VmString::from_static(&ctx, "")
        })
    }

    /// Parses a number formatted according to the rules of the language given
    /// in `code` back into a machine-readable number.
    fn parse_formatted_number<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, value): (VmString<'_>, Value<'gc>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::trace!("stub: mw.language.parseFormattedNumber({value:?})");
        // One might think that this would return `Value::Number` but actually
        // it is supposed to return strings…
        Ok(match value {
            Value::Integer(_) | Value::Number(_) => value.into_string(ctx).unwrap().into_value(ctx),
            Value::String(s) if s == "NaN" => "NAN".into_value(ctx),
            Value::String(s) if s == "∞" => "INF".into_value(ctx),
            Value::String(s) if s == "-∞" || s == "\u{2212}∞" => "-INF".into_value(ctx),
            Value::String(s) => s.to_str()?.replace(',', "").into_value(ctx),
            _ => Value::Nil,
        })
    }

    /// Converts a string to uppercase according to the rules of the given
    /// language.
    fn uc<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        Ok(ctx.intern(text.to_str()?.to_uppercase().as_bytes()))
    }

    /// Converts the first letter of a string to uppercase according to the
    /// rules of the given language.
    fn ucfirst<'gc>(
        &self,
        ctx: Context<'gc>,
        (_code, text): (VmString<'gc>, VmString<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        let mut text = text.to_str()?.chars();
        Ok(if let Some(first) = text.next() {
            ctx.intern(format!("{}{}", first.to_uppercase(), text.as_str()).as_bytes())
        } else {
            VmString::from_static(&ctx, "")
        })
    }
}

impl Default for LanguageLibrary {
    fn default() -> Self {
        Self {
            date: Cell::new(UtcDateTime::UNIX_EPOCH),
        }
    }
}

impl MwInterface for LanguageLibrary {
    const NAME: &str = "mw.language";
    const CODE: &[u8] = include_bytes!("./modules/mw.language.lua");

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

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(Table::new(&ctx))
    }
}
