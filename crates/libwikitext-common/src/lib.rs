//! Common Wikitext types and functions.

pub mod config;
pub mod db;
pub mod lru_limiter;
pub mod title;
pub mod url;

use core::{fmt::Write as _, iter};
use db::DatabaseProvider;
use fixed_decimal::{FloatPrecision, ParseError};
use html_escape::NAMED_ENTITIES;
use icu_datetime::provider::{
    names::{DatetimeNamesMonthGregorianV1, DatetimeNamesWeekdayV1, MonthNames},
    semantic_skeletons::marker_attrs::{ABBR_STANDALONE, WIDE_STANDALONE},
};
use icu_decimal::{DecimalFormatter, input::Decimal, options::GroupingStrategy};
use icu_locale::Locale;
use icu_provider::DataIdentifierBorrowed;
use libmisc::{CowExt as _, to_ascii_lower, to_ascii_upper};
use libphp_rs::{DateTime, DateTimeError, DateTimeZone, strtr, ucfirst};
use regex::Regex;
use std::{borrow::Cow, collections::HashMap, sync::LazyLock};
use uncased::UncasedStr;

/// A date formatting error.
#[derive(Debug, thiserror::Error)]
pub enum FormatDateError {
    /// There was something wrong with the date or formatting string.
    #[error(transparent)]
    DateTime(#[from] DateTimeError),
    /// The language code was not recognised as a valid locale.
    #[error(transparent)]
    Locale(#[from] icu_locale::ParseError),
    /// ICU4X did not like getting the data for the localiser.
    #[error(transparent)]
    Localizer(#[from] icu_provider::DataError),
}

/// A message formatting error.
#[derive(Debug, thiserror::Error)]
pub enum FormatMessageError<Db, Cb> {
    /// The database goofed.
    #[error(transparent)]
    Database(Db),
    /// The user callback goofed.
    #[error(transparent)]
    User(Cb),
}

/// A number formatting error.
#[derive(Debug, thiserror::Error)]
pub enum FormatNumberError<Db> {
    /// The database goofed.
    #[error(transparent)]
    Database(Db),
    /// A decimal string was not decimal enough.
    #[error(transparent)]
    Decimal(#[from] ParseError),
    /// ICU4X was sad about retrieving data.
    #[error(transparent)]
    IcuData(#[from] icu_provider::DataError),
    /// ICU4X was sad about parsing a locale name.
    #[error(transparent)]
    IcuLocale(#[from] icu_locale::ParseError),
}

/// An i18n message library.
pub struct Messages<'a, Db> {
    /// The database from which extra messages can be retrieved.
    db: Db,
    /// The map from a MediaWiki language code to a precompiled dictionary.
    dictionaries: HashMap<&'a str, &'a Dictionary<'a>>,
}

impl<'a, Db> Messages<'a, Db> {
    /// Creates a new `Messages` with the given `db` and `dictionaries`.
    pub fn new(db: Db, dictionaries: impl Into<HashMap<&'a str, &'a Dictionary<'a>>>) -> Self {
        Self {
            db,
            dictionaries: dictionaries.into(),
        }
    }

    /// Returns a reference to the extra messages database.
    // TODO: This is only used to simplify the tracking category API, which is
    // silly.
    pub fn db(&self) -> &Db {
        &self.db
    }
}

impl<'a, Db> Messages<'a, Db>
where
    Db: DatabaseProvider,
{
    /// Returns a reference to the first message that exists with a
    /// corresponding key from `keys` for the given MediaWiki language code
    /// `lang`, or for the default language if `lang` is `None`.
    ///
    /// # Errors
    ///
    /// * the database broke
    pub fn find_or_default<I, R>(
        &self,
        keys: I,
        lang: Option<&str>,
        use_db: bool,
    ) -> Result<Cow<'a, str>, Db::Error>
    where
        I: IntoIterator<Item = R>,
        R: AsRef<str> + Default,
    {
        let mut last = R::default();
        for key in keys {
            if let Some(message) = self.get_raw(key.as_ref(), lang, use_db)? {
                return Ok(message);
            }
            last = key;
        }

        let last = html_escape::encode_text(last.as_ref());
        let last = strtr(&last, &[("\u{0338}", "&#x338;")]);
        Ok(format!("⧼{last}⧽").into())
    }

    /// Finds the first valid message from the list of keys given in `keys` and
    /// returns that message, formatted using `cb` to replace any `$N`
    /// placeholders. If `cb` returns `None`, no replacement occurs.
    ///
    /// If the message is not found, returns a not-found string.
    ///
    /// # Errors
    ///
    /// * `callback` returns an error
    pub fn format_message<'c, F, I, R, E>(
        &self,
        lang: Option<&str>,
        use_db: bool,
        keys: I,
        callback: F,
    ) -> Result<Cow<'a, str>, FormatMessageError<Db::Error, E>>
    where
        F: FnMut(&str) -> Result<Option<Cow<'c, str>>, E>,
        I: IntoIterator<Item = R>,
        R: AsRef<str> + Default,
    {
        let message = self
            .find_or_default(keys, lang, use_db)
            .map_err(FormatMessageError::Database)?;
        message
            .try_map(|message| format_raw_message(message, callback))
            .map_err(FormatMessageError::User)
    }

    /// Formats a number similar to [`number_format`](https://php.net/number_format).
    ///
    /// # Errors
    ///
    /// * `messages` returns an error
    pub fn format_number(
        &self,
        lang: Option<&str>,
        n: f64,
        no_separators: bool,
    ) -> Result<Cow<'_, str>, FormatNumberError<Db::Error>> {
        Ok(match n {
            f64::INFINITY => Cow::Borrowed("∞"),
            f64::NEG_INFINITY => Cow::Borrowed("\u{2212}∞"),
            n if n.is_nan() => self
                .find_or_default(["formatnum-nan"], lang, true)
                .map_err(FormatNumberError::Database)?,
            n => {
                let lang = lang.unwrap_or_else(|| self.db().config().language);
                let locale = lang_to_bcp47::<true>(lang).parse::<Locale>()?;
                let grouping = if no_separators {
                    GroupingStrategy::Never
                } else {
                    <_>::default()
                };
                let fmt = DecimalFormatter::try_new(locale.into(), grouping.into())?;
                let v = Decimal::try_from_f64(n, FloatPrecision::RoundTrip)
                    .map_err(|_| ParseError::Limit)?;
                Cow::Owned(fmt.format(&v).to_string())
            }
        })
    }

    /// Returns a reference to the message with the corresponding `key` for the
    /// given MediaWiki language code `lang`, or for the default language if
    /// `lang` is `None`.
    ///
    /// # Errors
    ///
    /// * the database broke
    pub fn get_raw(
        &self,
        key: &str,
        lang: Option<&str>,
        use_db: bool,
    ) -> Result<Option<Cow<'a, str>>, Db::Error> {
        let content_language = self.db.config().language;
        let lang = lang.unwrap_or(content_language);
        // TODO: Get and use lang fallbacks configuration from config.

        if use_db {
            let db_key = to_upper_first(key);
            let db_key = if lang == content_language {
                db_key
            } else {
                Cow::Owned(format!("{db_key}/{lang}"))
            };

            if let message @ Some(_) =
                db::fetch(&self.db, &db_key, Some(title::Namespace::MEDIAWIKI))?
                    .map(|article| Cow::Owned(article.body().to_owned()))
            {
                return Ok(message);
            }
        }

        Ok(if let Some(dict) = self.dictionaries.get(lang) {
            let key = strtr(key, &[(" ", "_")]).map(to_lower_first);
            dict.messages
                .get(key.as_ref())
                .map(|v| Cow::Borrowed(v.as_ref()))
        } else {
            None
        })
    }
}

/// An i18n message dictionary.
#[derive(serde::Deserialize)]
pub struct Dictionary<'a> {
    /// Dictionary metadata.
    #[serde(rename = "@metadata")]
    _metadata: serde::de::IgnoredAny,
    /// Dictionary messages.
    #[serde(borrow, flatten)]
    messages: HashMap<&'a str, Cow<'a, str>>,
}

impl Dictionary<'_> {
    /// Merges `other` into `self`.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        self.messages.extend(other.messages);
        self
    }
}

/// An anchor encoding algorithm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnchorEncodeMode {
    /// The HTML5 anchor encoding algorithm.
    Html5,
    /// The legacy anchor encoding algorithm.
    Legacy,
}

/// Converts a BCP-47 code to its corresponding MediaWiki language code.
#[must_use]
pub fn bcp47_to_lang(code: &str) -> Cow<'_, str> {
    let code = NON_STANDARD_BCP47_CODES
        .get(code.into())
        .copied()
        .unwrap_or(code);
    DEPRECATED_LANGUAGE_CODES
        .get(code.into())
        .copied()
        .map_or(to_ascii_lower(code), Cow::Borrowed)
}

/// Decodes HTML entities according to the Wikitext rules.
pub fn decode_html(text: &str) -> Cow<'_, str> {
    // Someone working on MW decided that if an entity decodes to something
    // which is invalid in either HTML *or* XML that it is going to nuke it even
    // though ENTITIES EXIST IN PART TO ENABLE THESE CHARACTERS TO BE ENCODED!
    // Not replacing them in this way causes problems in edge cases like, of all
    // outdated and useless things, the CSS sanitiser.
    fn break_perfectly_valid_characters(c: char) -> char {
        if matches!(c, '\t' | '\n' | ' '..='\x7e'
            | '\u{00a0}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}'
        ) {
            c
        } else {
            char::REPLACEMENT_CHARACTER
        }
    }

    const MAX_LEN: usize = {
        let mut max = 0;
        let mut entities = NAMED_ENTITIES.as_slice();
        while let [(name, _), rest @ ..] = entities {
            if name.len() > max {
                max = name.len();
            }
            entities = rest;
        }

        if "רלמ".len() > max {
            max = "רלמ".len();
        }

        if "رلم".len() > max {
            max = "رلم".len();
        }

        max + b";".len()
    };

    let bytes = text.as_bytes();
    let entity_ranges = memchr::memchr_iter(b'&', bytes).filter_map(|start| {
        let next = start + "&".len();
        memchr::memchr(b';', &bytes[next..(next + MAX_LEN).min(bytes.len())])
            .map(|len| start..(next + len + b";".len()))
    });

    let mut flushed = 0;
    let mut out = String::new();
    for range in entity_ranges {
        let mut char = [0; 4];
        let name = &text[range.start + 1..range.end - 1];
        let value = if let Some(name) = name.strip_prefix('#') {
            if let Some(name) = name.strip_prefix(|c: char| matches!(c, 'X' | 'x')) {
                u32::from_str_radix(name, 16)
            } else {
                name.parse::<u32>()
            }
            .ok()
            .and_then(char::from_u32)
            .map(|c| &*break_perfectly_valid_characters(c).encode_utf8(&mut char))
        } else {
            NAMED_ENTITIES
                .binary_search_by(|(t_name, _)| t_name.cmp(&name.as_bytes()))
                .map_or_else(
                    |_| (name == "רלמ" || name == "رلم").then_some("\u{200f}"),
                    |index| Some(NAMED_ENTITIES[index].1),
                )
        };
        if let Some(value) = value {
            out += &text[flushed..range.start];
            out += value;
            flushed = range.end;
        }
    }

    if flushed != 0 {
        out += &text[flushed..];
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// Escapes all non-HTML Wikitext control sequences.
#[must_use]
pub fn escape(text: &str) -> Cow<'_, str> {
    static REPLS: &[(&str, &str)] = &[
        ("ISBN", "&#73;SBN"),
        ("PMID", "&#80;MID"),
        ("RFC", "&#82;FC"),
        ("＿", "&#xFF3F;"), // 3 bytes in UTF-8
        ("__", "&#95;_"),
        ("\'\'", "&#39;&#39;"),
        ("!", "&#33;"),
        (":", "&#58;"),
        (";", "&#59;"),
        ("[", "&#91;"),
        ("]", "&#93;"),
        ("{", "&#123;"),
        ("|", "&#124;"),
        ("}", "&#125;"),
    ];

    strtr(text, REPLS)
}

/// Encodes section heading text into a format suitable for use as a URL anchor.
///
/// This is equivalent to `escapeIdForAttribute` (`escapeIdInternal`).
#[must_use]
pub fn escape_id(s: &str, mode: AnchorEncodeMode) -> Cow<'_, str> {
    let id = &s[..s.floor_char_boundary(1024)];
    match mode {
        AnchorEncodeMode::Html5 => strtr(
            id,
            &[
                ("\t", "_"),
                ("\n", "_"),
                ("\x0c", "_"),
                ("\r", "_"),
                (" ", "_"),
            ],
        ),
        AnchorEncodeMode::Legacy => {
            const ALPHABET: percent_encoding::AsciiSet =
                libphp_rs::URL_ENCODE_ALPHABET.remove(b':');
            Cow::from(percent_encoding::utf8_percent_encode(id, &ALPHABET))
                .map(|id| strtr(id, &[(" ", "_"), ("%", ".")]))
        }
    }
}

/// Encodes section heading text into a format suitable for use as a URL anchor.
///
/// This is equivalent to `escapeIdForLink` (`escapeIdInternalUrl`).
#[must_use]
pub fn escape_id_url(s: &str, mode: AnchorEncodeMode) -> Cow<'_, str> {
    let id = escape_id(s, mode);
    if mode == AnchorEncodeMode::Html5 {
        static RE_DOUBLE_ENCODE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new("%([0-9a-fA-F]{2})").unwrap());
        id.map(|id| RE_DOUBLE_ENCODE.replace_all(id, "%25$1"))
    } else {
        id
    }
}

/// Escapes all Wikitext and HTML control sequences.
#[must_use]
pub fn escape_all(text: &str) -> Cow<'_, str> {
    static REPLS: &[(&str, &str)] = &[
        ("ISBN", "&#73;SBN"),
        ("PMID", "&#80;MID"),
        ("RFC", "&#82;FC"),
        ("＿", "&#xFF3F;"), // 3 bytes in UTF-8
        ("\'\'", "&#39;&#39;"),
        ("__", "&#95;_"),
        ("\"", "&quot;"),
        ("!", "&#33;"),
        ("&", "&amp;"),
        (":", "&#58;"),
        (";", "&#59;"),
        ("<", "&lt;"),
        ("=", "&#61;"),
        (">", "&gt;"),
        ("[", "&#91;"),
        ("]", "&#93;"),
        ("{", "&#123;"),
        ("|", "&#124;"),
        ("}", "&#125;"),
    ];

    strtr(text, REPLS)
}

/// Escapes HTML inside a `<nowiki>` tag.
#[must_use]
pub fn escape_no_wiki(text: &str) -> Cow<'_, str> {
    strtr(
        text,
        &[
            ("-{", "-&#123;"),
            ("}-", "&#125;-"),
            ("<", "&lt;"),
            (">", "&gt;"),
        ],
    )
}

/// Formats a date according to the given `format` string.
///
/// The `format` string is a MediaWiki extended time formatting string.
///
/// The `date` string is a modified form of the PHP date string format where
/// a four-digit number is treated as a year instead of a time.
///
/// If `local` is true, the time will be treated as being in the system time
/// zone; otherwise, it will be treated as UTC.
///
/// The value given in `now` will be used as the current time if no `date` is
/// given.
///
/// # Errors
///
/// * `date` cannot be parsed as a date
/// * `local` is true and the local time zone cannot be determined
/// * A write to the output buffer fails
pub fn format_date_mediawiki(
    now: &DateTime,
    format: &str,
    date: Option<&str>,
    lang: &str,
    local_tz: bool,
) -> Result<String, FormatDateError> {
    let date = if let Some(date) = date {
        let date = if date.len() == 4 && date.chars().all(|c| c.is_ascii_digit()) {
            Cow::Owned(format!("00:00 {date}"))
        } else {
            date.into()
        };

        DateTime::new(&date, Some(&DateTimeZone::UTC), Some(now))?
    } else {
        *now
    };

    let tz = if local_tz {
        DateTimeZone::local()?
    } else {
        DateTimeZone::UTC
    };

    let locale = lang_to_bcp47::<true>(lang).parse::<icu_locale::Locale>()?;
    let localizer = IcuDateTimeLocalizer::try_from(&locale)?;

    date.into_offset(tz)?
        .format(format, localizer)
        .map_err(|err| FormatDateError::DateTime(err.into()))
}

/// A localiser that uses baked data from [`icu_datetime`].
#[derive(Clone, Copy, Debug)]
pub struct IcuDateTimeLocalizer<'a> {
    /// The list of abbreviated month names.
    month_abbr: &'a zerovec::VarZeroVec<'a, str>,
    /// The list of full month names.
    month_full: &'a zerovec::VarZeroVec<'a, str>,
    /// The list of abbreviated weekday names.
    weekday_abbr: &'a zerovec::VarZeroVec<'a, str>,
    /// The list of full weekday names.
    weekday_full: &'a zerovec::VarZeroVec<'a, str>,
}

impl IcuDateTimeLocalizer<'_> {
    /// Retrieves a list of month names for the given `locale` with the given
    /// `marker` variant.
    ///
    /// # Errors
    ///
    /// * no data exists for the given `locale` and `marker`
    pub fn months<'a>(
        locale: &'a Locale,
        marker: &icu_provider::DataMarkerAttributes,
    ) -> Result<&'a zerovec::VarZeroVec<'a, str>, icu_provider::DataError> {
        let data_locale = icu_locale::DataLocale::from(locale);
        let MonthNames::Linear(months) =
            icu_provider::DataProvider::<DatetimeNamesMonthGregorianV1>::load(
                &icu_datetime::provider::Baked,
                icu_provider::DataRequest {
                    id: DataIdentifierBorrowed::for_marker_attributes_and_locale(
                        marker,
                        &data_locale,
                    ),
                    metadata: <_>::default(),
                },
            )?
            .payload
            .get_static()
            .ok_or(icu_provider::DataErrorKind::Custom.with_str_context("expected baked data"))?
        else {
            unreachable!()
        };
        Ok(months)
    }

    /// Retrieves a list of weekday names for the given `locale` with the given
    /// `marker` variant.
    ///
    /// # Errors
    ///
    /// * no data exists for the given `locale` and `marker`
    pub fn weekdays<'a>(
        locale: &'a Locale,
        marker: &icu_provider::DataMarkerAttributes,
    ) -> Result<&'a zerovec::VarZeroVec<'a, str>, icu_provider::DataError> {
        let data_locale = icu_locale::DataLocale::from(locale);
        icu_provider::DataProvider::<DatetimeNamesWeekdayV1>::load(
            &icu_datetime::provider::Baked,
            icu_provider::DataRequest {
                id: DataIdentifierBorrowed::for_marker_attributes_and_locale(marker, &data_locale),
                metadata: <_>::default(),
            },
        )?
        .payload
        .get_static()
        .map(|days| &days.names)
        .ok_or(icu_provider::DataErrorKind::Custom.with_str_context("expected baked data"))
    }
}

impl<'a> TryFrom<&'a icu_locale::Locale> for IcuDateTimeLocalizer<'a> {
    type Error = icu_provider::DataError;

    fn try_from(locale: &'a icu_locale::Locale) -> Result<Self, Self::Error> {
        Ok(Self {
            month_abbr: Self::months(locale, ABBR_STANDALONE)?,
            month_full: Self::months(locale, WIDE_STANDALONE)?,
            weekday_abbr: Self::weekdays(locale, ABBR_STANDALONE)?,
            weekday_full: Self::weekdays(locale, WIDE_STANDALONE)?,
        })
    }
}

impl<'a> libphp_rs::DateTimeLocalizer for IcuDateTimeLocalizer<'a> {
    type AbbrMonthOutput = &'a str;
    type AbbrWeekdayOutput = &'a str;
    type FullMonthOutput = &'a str;
    type FullWeekdayOutput = &'a str;

    fn month_abbr(&self, month: time::Month) -> Self::AbbrMonthOutput {
        &self.month_abbr[usize::from(month as u8 - 1)]
    }

    fn month_full(&self, month: time::Month) -> Self::FullMonthOutput {
        &self.month_full[usize::from(month as u8 - 1)]
    }

    fn weekday_abbr(&self, day: time::Weekday) -> Self::AbbrWeekdayOutput {
        &self.weekday_abbr[usize::from(day.number_days_from_sunday())]
    }

    fn weekday_full(&self, day: time::Weekday) -> Self::FullWeekdayOutput {
        &self.weekday_full[usize::from(day.number_days_from_sunday())]
    }
}

/// Formats a message, using `callback` to replace any `$N` placeholders in the
/// message. If `callback` returns `None`, no replacement occurs.
///
/// # Errors
///
/// * `callback` returns an error
pub fn format_raw_message<'a, E, F>(message: &str, mut callback: F) -> Result<Cow<'_, str>, E>
where
    F: FnMut(&str) -> Result<Option<Cow<'a, str>>, E>,
{
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d+)").unwrap());

    let mut out = String::new();
    let mut flushed = 0;
    for capture in RE.captures_iter(message) {
        let (_, [key]) = capture.extract();
        if let Some(value) = callback(key)? {
            let range = capture.get_match().range();
            out += &message[flushed..range.start];
            out += &value;
            flushed = range.end;
        }
    }

    Ok(if flushed == 0 {
        Cow::Borrowed(message)
    } else {
        out += &message[flushed..];
        Cow::Owned(out)
    })
}

/// Converts a MediaWiki language code to its corresponding BCP-47 code.
///
/// If `USE_NON_STANDARD` is false and `code` is a non-standard code, an empty
/// string is returned.
#[must_use]
pub fn lang_to_bcp47<const USE_NON_STANDARD: bool>(code: &str) -> Cow<'_, str> {
    let code = DEPRECATED_LANGUAGE_CODES
        .get(code.into())
        .copied()
        .unwrap_or(code);
    let code = NON_STANDARD_LANGUAGE_CODES
        .get(code.into())
        .map_or(code, |code| {
            // The only place where `USE_NON_STANDARD = false` is used is a place
            // where the MediaWiki API gives MediaWiki language codes, including
            // non-standard codes, and the caller does not want to match those non-
            // standard ones, even if the input is itself a non-standard MediaWiki
            // code.
            if USE_NON_STANDARD { code } else { "" }
        });
    std_lang_to_bcp47(code)
}

/// Converts a standard, non-deprecated MediaWiki language code to its
/// corresponding BCP-47 code.
fn std_lang_to_bcp47(code: &str) -> Cow<'_, str> {
    let mut out = String::new();
    let mut flushed = 0;
    let mut last = 0;
    let mut after_x = false;
    let iter = code
        .match_indices('-')
        .map(|(index, _)| index)
        .chain(iter::once(code.len()));
    for index in iter {
        let text = &code[last..index];
        let text = if !after_x && last != 0 && text.len() == 2 {
            to_ascii_upper(text)
        } else if !after_x && last != 0 && text.len() == 4 {
            ucfirst(text)
        } else {
            to_ascii_lower(text)
        };
        after_x = text == "x";
        if let Cow::Owned(text) = text {
            out += &code[flushed..last];
            out += &text;
            flushed = index;
        }
        last = index + 1;
    }

    if flushed == 0 {
        Cow::Borrowed(code)
    } else {
        out += &code[flushed..];
        Cow::Owned(out)
    }
}

/// Creates a URL for the given title using the given protocol, base URI, path,
/// and query string.
///
/// To create a protocol-relative URL, pass `Some("")` to `proto`.
///
/// The `query` and `fragment` arguments should *not* include any `?` or `#`
/// prefix.
///
/// The value passed to `fragment` will be anchor-encoded.
///
/// # Panics
///
/// * A write to the output buffer fails
pub fn make_url<P: core::fmt::Display>(
    base_uri: &url::Url,
    proto: Option<&str>,
    path: P,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let authority = base_uri.authority().unwrap_or("//localhost");
    let base_path = base_uri.path().trim_start_matches('/');

    let mut url = String::new();
    if let Some(proto) = proto {
        write!(url, "{proto}{authority}").unwrap();
    }
    if !base_path.is_empty() {
        write!(url, "/{base_path}").unwrap();
    }
    write!(url, "/{path}").unwrap();
    if let Some(query) = query {
        write!(url, "?{query}").unwrap();
    }
    if let Some(fragment) = fragment
        && !fragment.is_empty()
    {
        write!(url, "#{}", escape_id_url(fragment, AnchorEncodeMode::Html5)).unwrap();
    }
    url
}

/// Strips formatting characters from a numeric string.
#[must_use]
pub fn parse_formatted_number(s: &str) -> Cow<'_, str> {
    match s {
        "NaN" => "NAN".into(),
        "∞" => "INF".into(),
        "-∞" | "\u{2212}∞" => "-INF".into(),
        s => strtr(s, &[("\u{2212}", "-"), (",", "")]),
    }
}

/// Normalises `text` by converting runs of whitespace that might exist in
/// a normalised [`Title`](title::Title) to a single space character and
/// trimming. This is like `Sanitizer::normalizeSectionNameWhitespace`.
#[must_use]
pub fn normalize_section_name(text: &str) -> Cow<'_, str> {
    #[inline]
    fn spacelike(c: char) -> bool {
        matches!(c, ' ' | '_')
    }
    normalize_whitespace::<true>(text, spacelike, spacelike)
}

/// A generic function for normalising text by converting runs of whitespace to
/// a single space character and trimming the end and (optionally) start.
///
/// * `trimmable` should return true if the character `c` should be trimmed from
///   the start and end of `text`.
/// * `spacelike` should return true if the character `c` should be part of a
///   run of whitespace that gets collapsed to a single space character.
pub fn normalize_whitespace<const LTRIM: bool>(
    text: &str,
    mut trimmable: impl FnMut(char) -> bool,
    mut spacelike: impl FnMut(char) -> bool,
) -> Cow<'_, str> {
    let mut out = String::new();
    let mut flushed = 0;
    let mut iter = text.char_indices().peekable();

    while let Some((index, c)) = iter.next() {
        // Peek to avoid switching to owned-mode when encountering a single
        // space
        if trimmable(c) && (c != ' ' || matches!(iter.peek(), Some((_, c)) if trimmable(*c))) {
            // Non-space whitespace are converted to space, and runs of
            // whitespace are collapsed into a single character
            while iter.next_if(|(_, c)| trimmable(*c)).is_some() {}

            // This acts like `trim`, not emitting a space at the start
            // (`index == 0`) or end (`peek().is_none()`) of the text.
            if let Some((next_index, _)) = iter.peek() {
                out += &text[flushed..index];
                flushed = *next_index;
                // Bidi markers get stripped because “Sometimes they slip
                // into cut-n-pasted page titles”
                if (!LTRIM || index != 0) && spacelike(c) {
                    out.push(' ');
                }
            }
        }
    }

    if flushed == 0 {
        Cow::Borrowed(if LTRIM {
            text.trim_matches(trimmable)
        } else {
            text.trim_end_matches(trimmable)
        })
    } else {
        out += text[flushed..].trim_end_matches(trimmable);
        Cow::Owned(out)
    }
}

/// Converts an iterator of strings into a regular expression alternates
/// subexpression.
#[must_use]
pub fn regex_switch<I>(items: I) -> String
where
    I: Iterator,
    I::Item: AsRef<str>,
{
    let mut out = String::new();
    for item in items {
        if !out.is_empty() {
            out.push('|');
        }
        out += &regex::escape(item.as_ref());
    }
    out
}

/// Decodes a possibly URL-encoded title from a Wikitext link target.
#[must_use]
pub fn title_decode(target: &str) -> Cow<'_, str> {
    url_decode(target).borrowed_or_else(|target| {
        strtr(&target, &[("<", "&lt;"), (">", "&gt;")])
            .owned()
            .unwrap_or(target)
    })
}

/// Converts the first letter in the string to Unicode lowercase, avoiding
/// allocation if it is already lowercase.
#[must_use]
pub fn to_lower_first(input: &str) -> Cow<'_, str> {
    let mut chars = input.chars();
    if let Some(first) = chars.next()
        && !first.is_lowercase()
    {
        Cow::Owned(format!("{}{}", first.to_lowercase(), chars.as_str()))
    } else {
        Cow::Borrowed(input)
    }
}

/// Converts the first letter in the string to Unicode uppercase, avoiding
/// allocation if it is already uppercase.
#[must_use]
pub fn to_upper_first(input: &str) -> Cow<'_, str> {
    let mut chars = input.chars();
    if let Some(first) = chars.next()
        && !first.is_uppercase()
    {
        Cow::Owned(format!("{}{}", first.to_uppercase(), chars.as_str()))
    } else {
        Cow::Borrowed(input)
    }
}

/// Percent-decodes a URL part.
#[inline]
#[must_use]
pub fn url_decode(input: &str) -> Cow<'_, str> {
    percent_encoding::percent_decode_str(input).decode_utf8_lossy()
}

/// Percent-encodes a URL part using the MediaWiki rules.
///
/// This is equivalent to `wfUrlencode`.
#[inline]
#[must_use]
pub fn url_encode(input: &str) -> Cow<'_, str> {
    libphp_rs::url_encode_alphabet(input, &WM_ALPHABET)
}

/// Percent-encodes a URL part.
#[inline]
#[must_use]
pub fn url_encode_bytes(input: &[u8]) -> Cow<'_, str> {
    libphp_rs::url_encode_bytes_alphabet(input, &WM_ALPHABET)
}

/// Percent-encodes a URL part following the MediaWiki rules for sanitized URLs,
/// which only encodes a subset of ASCII characters.
#[must_use]
pub fn url_encode_sanitized(input: &str) -> Cow<'_, str> {
    let mut out = String::new();
    let mut flushed = 0;
    for (cursor, b) in input.bytes().enumerate() {
        if matches!(
            b,
            b'\x00'..=b'\x1f' | b'"' | b'<' | b'>' | b'[' | b']' | b'|' | b'\x7f'
        ) {
            out += &input[flushed..cursor];
            out += percent_encoding::percent_encode_byte(b);
            flushed = cursor + 1;
        } else if b == b' ' {
            out += &input[flushed..cursor];
            out.push('+');
            flushed = cursor + 1;
        }
    }

    if flushed == 0 {
        Cow::Borrowed(input)
    } else {
        out += &input[flushed..];
        Cow::Owned(out)
    }
}

/// A map from deprecated MediaWiki language codes to their non-deprecated
/// replacements.
pub const DEPRECATED_LANGUAGE_CODES: phf::Map<&UncasedStr, &str> = phf::phf_map! {
    UncasedStr::new("als") => "gsw",
    UncasedStr::new("bat-smg") => "sgs",
    UncasedStr::new("be-x-old") => "be-tarask",
    UncasedStr::new("fiu-vro") => "vro",
    UncasedStr::new("roa-rup") => "rup",
    UncasedStr::new("zh-classical") => "lzh",
    UncasedStr::new("zh-min-nan") => "nan",
    UncasedStr::new("zh-yue") => "yue",
};

/// Creates maps between irregular MediaWiki language codes and their BCP-47
/// equivalents.
macro_rules! non_standard_lang_codes {
    ($($from:literal => $to:literal),* $(,)?) => {
        /// A map from an irregular language code to its BCP-47 equivalent.
        const NON_STANDARD_LANGUAGE_CODES: phf::Map<&UncasedStr, &str> = phf::phf_map! {
            $(UncasedStr::new($from) => $to),*
        };

        /// A map from a BCP-47 code to its irregular MediaWiki language code
        /// equivalent.
        const NON_STANDARD_BCP47_CODES: phf::Map<&UncasedStr, &str> = phf::phf_map! {
            $(UncasedStr::new($to) => $from),*
        };
    };
}

non_standard_lang_codes! {
    "cbk-zam" => "cbk",
    "de-formal" => "de-x-formal",
    "eml" => "egl",
    "en-rtl" => "en-x-rtl",
    "es-formal" => "es-x-formal",
    "hu-formal" => "hu-x-formal",
    "map-bms" => "jv-x-bms",
    "mo" => "ro-Cyrl-MD",
    "nrm" => "nrf",
    "nl-informal" => "nl-x-informal",
    "roa-tara" => "nap-x-tara",
    "simple" => "en-simple",
    "sr-ec" => "sr-Cyrl",
    "sr-el" => "sr-Latn",
    "crh-ro" => "crh-Latn-RO",
    "kk-cn" => "kk-Arab-CN",
    "kk-tr" => "kk-Latn-TR",
    "zh-cn" => "zh-Hans-CN",
    "zh-sg" => "zh-Hans-SG",
    "zh-my" => "zh-Hans-MY",
    "zh-tw" => "zh-Hant-TW",
    "zh-hk" => "zh-Hant-HK",
    "zh-mo" => "zh-Hant-MO",
}

/// The alphabet of characters to percent-encode when encoding URLs.
///
/// This is a combination of “all non-alphanumeric characters except `-_.`” from
/// PHP’s `urlencode`, and then MediaWiki also excludes `!$()*,/:;@~`.
const WM_ALPHABET: percent_encoding::AsciiSet = libphp_rs::URL_ENCODE_ALPHABET
    .remove(b'!')
    .remove(b'$')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b',')
    .remove(b'/')
    .remove(b':')
    .remove(b';')
    .remove(b'@')
    .remove(b'~');

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_html() {
        assert_eq!(
            decode_html("hello & world"),
            Cow::Borrowed("hello & world"),
            "non-entity should remain as-is"
        );
        assert_eq!(
            decode_html("hello&nbsp;world"),
            Cow::Owned::<str>(String::from("hello\u{00a0}world")),
            "entity should decode"
        );
        assert_eq!(
            decode_html("hello&oops;world"),
            Cow::Borrowed("hello&oops;world"),
            "invalid entity should remain as-is"
        );
        assert_eq!(
            decode_html("hello&;world"),
            Cow::Borrowed("hello&;world"),
            "invalid empty entity should remain as-is"
        );
        assert_eq!(
            decode_html("hello&nbsp world"),
            Cow::Borrowed("hello&nbsp world"),
            "html5 entity termination rules should not be used"
        );
        assert_eq!(
            decode_html("hello&רלמ;world"),
            Cow::Borrowed("hello\u{200f}world"),
            "special Hebrew RTL entity should decode"
        );
        assert_eq!(
            decode_html("hello&رلم;world"),
            Cow::Borrowed("hello\u{200f}world"),
            "special Arabic RTL entity should decode"
        );
        assert_eq!(
            decode_html("hello&#42;world"),
            Cow::Borrowed("hello*world"),
            "decimal entity should decode"
        );
        assert_eq!(
            decode_html("hello&#x42;world"),
            Cow::Borrowed("helloBworld"),
            "hexadecimal entity should decode"
        );
        assert_eq!(
            decode_html("hello&&nbsp;world"),
            Cow::Owned::<str>(String::from("hello&\u{00a0}world")),
            "incomplete entity should not interfere with later entity"
        );
    }

    #[test]
    fn test_lang_to_bcp47() {
        const TESTS: &[(&str, &str)] = &[
            ("en-ca-x-ca", "en-CA-x-ca"),
            ("sgn-be-fr", "sgn-BE-FR"),
            ("az-latn-x-latn", "az-Latn-x-latn"),
            ("sr-Latn-RS", "sr-Latn-RS"),
            ("az-arab-ir", "az-Arab-IR"),
            ("sl-nedis", "sl-nedis"),
            ("de-ch-1996", "de-CH-1996"),
            (
                "en-latn-gb-boont-r-extended-sequence-x-private",
                "en-Latn-GB-boont-r-extended-sequence-x-private",
            ),
            ("DE", "de"),
            ("fR", "fr"),
            ("ja", "ja"),
            ("zh-hans", "zh-Hans"),
            ("sr-cyrl", "sr-Cyrl"),
            ("sr-latn", "sr-Latn"),
            ("zh-cmn-hans-cn", "zh-cmn-Hans-CN"),
            ("cmn-hans-cn", "cmn-Hans-CN"),
            ("zh-yue-hk", "zh-yue-HK"),
            ("yue-hk", "yue-HK"),
            ("zh-hans-cn", "zh-Hans-CN"),
            ("sr-latn-RS", "sr-Latn-RS"),
            ("sl-rozaj", "sl-rozaj"),
            ("sl-rozaj-biske", "sl-rozaj-biske"),
            ("sl-nedis", "sl-nedis"),
            ("de-ch-1901", "de-CH-1901"),
            ("sl-it-nedis", "sl-IT-nedis"),
            ("hy-latn-it-arevela", "hy-Latn-IT-arevela"),
            ("de-de", "de-DE"),
            ("en-us", "en-US"),
            ("es-419", "es-419"),
            ("de-ch-x-phonebk", "de-CH-x-phonebk"),
            ("az-arab-x-aze-derbend", "az-Arab-x-aze-derbend"),
            ("x-whatever", "x-whatever"),
            ("qaa-qaaa-qm-x-southern", "qaa-Qaaa-QM-x-southern"),
            ("de-qaaa", "de-Qaaa"),
            ("sr-latn-qm", "sr-Latn-QM"),
            ("sr-qaaa-rs", "sr-Qaaa-RS"),
            ("en-us-u-islamcal", "en-US-u-islamcal"),
            ("zh-cn-a-myext-x-private", "zh-CN-a-myext-x-private"),
            ("en-a-myext-b-another", "en-a-myext-b-another"),
            ("als", "gsw"),
            ("bat-smg", "sgs"),
            ("be-x-old", "be-tarask"),
            ("fiu-vro", "vro"),
            ("roa-rup", "rup"),
            ("zh-classical", "lzh"),
            ("zh-min-nan", "nan"),
            ("zh-yue", "yue"),
            ("cbk-zam", "cbk"),
            ("de-formal", "de-x-formal"),
            ("eml", "egl"),
            ("en-rtl", "en-x-rtl"),
            ("es-formal", "es-x-formal"),
            ("hu-formal", "hu-x-formal"),
            ("kk-Arab", "kk-Arab"),
            ("kk-Cyrl", "kk-Cyrl"),
            ("kk-Latn", "kk-Latn"),
            ("map-bms", "jv-x-bms"),
            ("mo", "ro-Cyrl-MD"),
            ("nrm", "nrf"),
            ("nl-informal", "nl-x-informal"),
            ("roa-tara", "nap-x-tara"),
            ("simple", "en-simple"),
            ("sr-ec", "sr-Cyrl"),
            ("sr-el", "sr-Latn"),
            ("zh-cn", "zh-Hans-CN"),
            ("zh-sg", "zh-Hans-SG"),
            ("zh-my", "zh-Hans-MY"),
            ("zh-tw", "zh-Hant-TW"),
            ("zh-hk", "zh-Hant-HK"),
            ("zh-mo", "zh-Hant-MO"),
            ("zh-hans", "zh-Hans"),
            ("zh-hant", "zh-Hant"),
        ];

        for (code, expected) in TESTS {
            assert_eq!(lang_to_bcp47::<true>(code), *expected);
        }
    }
}
