//! Parser function implementations.
//!
//! <https://www.mediawiki.org/wiki/Help:Extension:ParserFunctions>

#![expect(
    clippy::unnecessary_wraps,
    reason = "implementing an interface invisible to clippy"
)]

use super::{
    Document, Error, PluginResult, PluginState, Result, State,
    expand_templates::ExpandMode,
    extension_tags, preprocess_frame,
    stack::{IndexedArgs, KeyCacheKvs, Kv, StackFrame},
    surrogate::Surrogate as _,
    template::call_module,
};
use ::time::{Month, UtcDateTime};
use core::{
    fmt::{self, Write as _},
    iter,
    str::FromStr,
};
use either::Either;
use libmisc::CowExt as _;
use libphp_rs::{floatval, fuzzy_cmp, strtr};
use libwikitext_common::{
    AnchorEncodeMode, anchor_encode,
    config::Configuration,
    db::{Article, DatabaseProvider as _, Error as DatabaseError},
    decode_html, format_date_mediawiki, format_message, format_number, format_raw_message,
    make_url, parse_formatted_number,
    title::{Namespace, Title},
    url_encode,
};
use libwikitext_common_gpl::expr;
use libwikitext_parse::{FileMap, Span, strip};
use locale_rs::Locale;
use regex::Regex;
use std::{
    borrow::Cow,
    sync::{Arc, LazyLock},
};

/// A trait for plugins to add new parser functions.
pub trait PluginParserFn {
    /// Invokes the new parser function.
    ///
    /// # Errors
    ///
    /// * the parser call fails
    fn call(
        &self,
        out: &mut String,
        state: &mut PluginState<'_, '_, '_, '_>,
        args: PluginFnArgs<'_, '_, '_>,
    ) -> PluginResult;
}

/// An opaque arguments object for plugin calls.
pub struct PluginFnArgs<'args, 'call, 'sp>(&'call IndexedArgs<'args, 'call, 'sp>);

impl PluginFnArgs<'_, '_, '_> {
    /// Gets the name of the callee.
    #[inline]
    #[must_use]
    pub fn callee(&self) -> &str {
        self.0.callee
    }

    /// Evaluates an entire k-v pair at the given index as a single value.
    ///
    /// The returned value will include any leading and trailing whitespace
    /// present in the original text.
    ///
    /// # Errors
    ///
    /// * parsing or rendering fails
    #[inline]
    pub fn eval(
        &self,
        state: &mut PluginState<'_, '_, '_, '_>,
        index: usize,
    ) -> PluginResult<Option<Cow<'_, str>>> {
        self.0.eval(state.0, index).map_err(Into::into)
    }

    /// Roughly equivalent to `recursiveTagParseFully`.
    ///
    /// # Errors
    ///
    /// * parsing or rendering fails
    #[inline]
    pub fn eval_as_document(
        &self,
        state: &mut PluginState<'_, '_, '_, '_>,
        index: usize,
    ) -> PluginResult<Option<Cow<'_, str>>> {
        self.eval_as_impl(state, index, false)
    }

    /// Roughly equivalent to `recursiveTagParse`.
    ///
    /// # Errors
    ///
    /// * parsing or rendering fails
    #[inline]
    pub fn eval_as_fragment(
        &self,
        state: &mut PluginState<'_, '_, '_, '_>,
        index: usize,
    ) -> PluginResult<Option<Cow<'_, str>>> {
        self.eval_as_impl(state, index, true)
    }

    /// Preprocesses an argument, then renders the preprocessed content as HTML.
    fn eval_as_impl(
        &self,
        state: &mut PluginState<'_, '_, '_, '_>,
        index: usize,
        fragment: bool,
    ) -> PluginResult<Option<Cow<'_, str>>> {
        let Some(source) = self.0.eval(state.0, index)? else {
            return Ok(None);
        };
        let sp = self.0.sp.clone_with_source(FileMap::new(&source));
        let root = state.0.statics.parser.parse_no_expansion(&sp.source)?;
        let mut out = Document::new(fragment);
        out.adopt_output(state.0, &sp, &root)?;
        Ok(Some(out.finish(state.0).into()))
    }

    /// Returns true if there are no arguments.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of arguments.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// The function signature of a parser function.
type ParserFn = fn(&mut String, &mut State<'_, '_, '_>, &IndexedArgs<'_, '_, '_>) -> Result;

mod cond {
    //! Flow control parser functions.

    use super::*;

    /// `{{#expr: expression}}`
    pub fn expr(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(expr) = arguments.eval(state, 0)?.map(trim) {
            let result = expr::do_expression(&expr);
            // log::trace!("#expr: '{expr}' = {result:?}");

            // 'Template:Minor planet' sends garbage into an expression and
            // relies on this just not matching a switch key.
            // TODO: See the note on `fn if_error`.
            match result {
                Ok(Some(result)) => write!(out, "{result}")?,
                Ok(None) => {}
                Err(err) => write!(
                    out,
                    r#"<span class="error">{}</span>"#,
                    html_escape::encode_text(&err.to_string())
                )?,
            }
        }

        Ok(())
    }

    /// `{{#if: condition | consequent (!condition.trim().is_empty()) | alternate }}`
    pub fn r#if(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // Article 'Television' has `{{As of|June 2021}}` which is not
        // a valid input for that template, but the template makes it
        // work because it is an error nested inside of an `#if`.
        // TODO: Emit errors here as warnings.
        let lhs_is_empty = match arguments.eval(state, 0) {
            Ok(Some(value)) => decode_trim(value).is_empty(),
            Ok(None) => true,
            Err(err) => {
                log::warn!("#if: error suppressed: {err}");
                false
            }
        };
        let index = 1 + usize::from(lhs_is_empty);
        // log::trace!("#if: '{lhs}'? {}", index == 0);
        if let Some(value) = arguments.eval(state, index)?.map(trim) {
            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{#ifeq: lhs | rhs | consequent (lhs == rhs) | alternate }}`
    pub fn if_eq(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let lhs = arguments.eval(state, 0)?.map_or("".into(), decode_trim);
        let rhs = arguments.eval(state, 1)?.map_or("".into(), decode_trim);
        let is_eq = fuzzy_cmp(&lhs, &rhs);
        // log::trace!("#ifeq: '{lhs:?}' == '{rhs:?}'? {is_eq}");
        if let Some(value) = arguments.eval(state, 2 + usize::from(!is_eq))?.map(trim) {
            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{#iferror: condition | consequent (error) | alternate (no error) }}`
    pub fn if_error(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        static I_AM_BAD: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"<(?:strong|span|p|div)\s(?:[^\s>]*\s+)*?class="(?:[^"\s>]*\s+)*?error(?:\s[^">]*)?""#).unwrap()
        });

        let lhs = arguments.eval(state, 0);
        let is_error = match lhs {
            // It is probably still necessary to do the string check because
            // some script or template might emit an error handwritten in this
            // way
            Ok(Some(lhs)) => I_AM_BAD.is_match(&lhs),
            Ok(None) => false,
            Err(_) => true,
        };

        if is_error {
            if let Some(value) = arguments.eval(state, 1)?.map(trim) {
                write!(out, "{value}")?;
            }
        } else if let Some(value) = arguments.eval(state, 2)?.map(trim) {
            write!(out, "{value}")?;
        } else if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{#ifexpr: expression | consequent (expression != 0.0) | alternate }}`
    pub fn if_expr(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let expr = arguments.eval(state, 0)?;
        // 'Template:Date' sends garbage values to `#ifexpr` without an
        // `#iferror` guard to capture the errors.
        match on_error_resume_next(expr::do_expression(expr.as_deref().unwrap_or_default())) {
            Ok(result) => {
                // log::trace!("#ifexpr: {expr:?} = {result:?}");
                let index = 1 + usize::from(result.unwrap_or(0.0) == 0.0);
                if let Some(value) = arguments.eval(state, index)?.map(trim) {
                    write!(out, "{value}")?;
                }
            }
            Err(err) => write!(out, "{err}")?,
        }

        Ok(())
    }

    /// `{{#switch: match | case [| case ...] = value | default }}`
    pub fn switch(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        const DEFAULT: &str = "default";

        let lhs = arguments.eval(state, 0)?.map_or("".into(), decode_trim);
        let mut found = false;
        let mut consequent = None;

        let len = arguments.len();
        for (index, arg) in arguments.iter().enumerate().skip(1) {
            // log::trace!("#switch: arg '{:?}'", &arg.value);

            // If the case is in the form `k=v` then it is a new case,
            // otherwise we must record whether the case matched and
            // continue processing until a `k=v` is encountered to know
            // the consequent
            let (rhs, is_kv) = if let Some(name) = arg.name(state, arguments.sp)? {
                (name, true)
            } else {
                (arg.eval(state, arguments.sp)?, false)
            };
            let rhs = decode_trim(rhs);

            // Default value can either be a bare final parameter or it
            // can be `#default = value`
            if magic_matches(state, DEFAULT, &rhs) && is_kv {
                consequent = Some(arg);
            }

            if !found {
                found = fuzzy_cmp(&lhs, &rhs);
                // log::trace!("#switch: '{lhs}' == '{rhs}'? {found}");
            }

            if found && is_kv {
                consequent = Some(arg);
                break;
            }

            // If the case is the last one, there was no `#default`, and it
            // is not a `k=v`, then it is the default value
            if index + 1 == len && consequent.is_none() && !is_kv {
                consequent = Some(arg);
                break;
            }
        }

        if let Some(consequent) = consequent {
            let value = consequent.value(state, arguments.sp).map(trim)?;
            write!(out, "{value}")?;
        }

        Ok(())
    }
}

mod ext {
    //! Tag parser functions.

    use super::*;

    /// `{{#tag: tag_name | content [| attribute [= value] ...] }}`
    pub fn extension_tag(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let (Some(name), Some(body)) = (arguments.eval(state, 0)?, arguments.eval(state, 1)?) {
            let name = strip::kill(&name);
            // Extension tags may contain non-ASCII characters
            let name = name.trim_ascii().to_lowercase();
            match extension_tags::render_extension_tag(
                state,
                arguments.sp,
                arguments.span,
                &name,
                &extension_tags::InArgs::ParserFn(&arguments.arguments[2..]),
                Some(&body),
                false,
            )? {
                Some(Either::Left(marker)) => {
                    state.strip_markers.push(out, &name, marker);
                }
                Some(Either::Right(raw)) => {
                    write!(out, "{raw}")?;
                }
                None => {}
            }
        }
        Ok(())
    }

    /// `{{#coordinates: latitude | longitude [| primary][| GeoHack parameters][| extra parameters] }}`
    pub fn geodata_coordinates(
        _: &mut String,
        _: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // This normally converts and validates coordinates, then stashes them
        // in “a database”. But we have no database, so unless this is the data
        // which 'Module:Mapframe|wikidataCoords' tries and fails to find, there
        // is no point in doing anything with this. TODO: Is it?
        Ok(())
    }

    /// `{{#invoke: module | function [| argument [= value] ...] }}`
    pub fn invoke(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        call_module(out, state, arguments.sp, &arguments.arguments)
    }

    /// `{{#property: name [| from = Qid] }}`
    pub fn wikibase_property(
        _: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(name) = arguments.eval(state, 0)? {
            let id = arguments.get(state, "from")?;
            log::warn!("stub: #property({name}, {id:?})");
        }
        Ok(())
    }
}

mod page {
    //! Page information parser functions.

    use super::*;

    /// `{{BASEPAGENAME}}`
    pub fn base_page_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        page_name_impl(out, state, arguments, Title::base_text, Title::base_uri)
    }

    /// `{{FULLPAGENAME}}`
    pub fn full_page_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        page_name_impl(
            out,
            state,
            arguments,
            Title::prefixed_text,
            Title::prefixed_url,
        )
    }

    /// `{{PAGEID[: title] }}`
    pub fn page_id(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(id) = get_article(state, arguments)?.map(|article| article.id()) {
            write!(out, "{id}")?;
        }
        Ok(())
    }

    /// `{{PAGENAME}}`
    pub fn page_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        page_name_impl(out, state, arguments, Title::text, Title::text_url)
    }

    /// Common implementation for all `{{XXXPAGENAME}}` functions.
    #[inline]
    fn page_name_impl<FnT, FnU>(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        IndexedArgs { callee, .. }: &IndexedArgs<'_, '_, '_>,
        text: FnT,
        uri: FnU,
    ) -> Result
    where
        FnT: FnOnce(&Title) -> &str,
        FnU: FnOnce(&Title) -> Cow<'_, str>,
    {
        let title = &state.globals.title;
        let as_uri = callee.ends_with("ee");
        let part = if as_uri {
            uri(title)
        } else {
            text(title).into()
        };
        write!(
            out,
            "{}",
            libwikitext_parse_gpl::escape_all(state.statics.db.config(), &part)
        )?;
        Ok(())
    }

    /// `{{PAGESIZE}}`
    #[expect(
        clippy::cast_precision_loss,
        reason = "≥2**53 is not an addressable amount of memory"
    )]
    pub fn page_size(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(page_size) = get_article(state, arguments)?.map(|article| article.body().len())
        {
            let no_separators = arguments
                .eval(state, 1)?
                .is_some_and(|arg| magic_matches(state, RAW_SUFFIX, &arg));
            write!(out, "{}", format_number(page_size as f64, no_separators))?;
        }
        Ok(())
    }

    /// `{{PROTECTIONEXPIRY[: action [| pagename]] }}`
    pub fn protection_expiry(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // From <https://www.mediawiki.org/wiki/Manual:Checking_for_page_existence/PROTECTIONEXPIRY_method>:
        //
        // “The {{PROTECTIONEXPIRY}} parser function can be used to check
        //  whether a page exists. It returns `infinity` if the page exists and
        //  is not protected, the actual expiry time if it is protected, and the
        //  empty string if it doesn't exist.”
        let exists = arguments.eval(state, 1)?.map(trim).is_none_or(|page_name| {
            let Ok(title) = Title::new(state.statics.db.config(), &page_name, None) else {
                // If creating the title fails then it falls back to the current
                // page, which of course exists
                return true;
            };
            state.statics.db.contains(&title)
        });
        if exists {
            write!(out, "infinity")?;
        }
        Ok(())
    }

    /// `{{[gettable variable name]}}`
    pub fn page_var(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        IndexedArgs { callee, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // TODO: Technically the value might be a tree
        if let Some(value) = state.globals.variables.get(*callee) {
            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{ARTICLEPAGENAME}}` or `{{SUBJECTPAGENAME}}` or `{{TALKPAGENAME}}`
    fn related_page_name<F>(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        IndexedArgs { callee, .. }: &IndexedArgs<'_, '_, '_>,
        related: F,
    ) -> Result
    where
        F: for<'a> FnOnce(&'a Title, &Configuration) -> Option<Cow<'a, Title>>,
    {
        let config = state.statics.db.config();
        let title = related(&state.globals.title, config);
        if let Some(title) = title {
            let as_uri = callee.ends_with("ee");
            let part = if as_uri {
                title.partial_url()
            } else {
                title.key().into()
            };
            write!(out, "{}", libwikitext_parse_gpl::escape_all(config, &part))?;
        }
        Ok(())
    }

    /// `{{REVISIONID[: title] }}`
    pub fn revision_id(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let revision_id = get_article(state, arguments)?.map(|article| article.revision_id());
        if let Some(revision_id) = revision_id {
            write!(out, "{revision_id}")?;
        }
        Ok(())
    }

    /// Common implementation for all `{{REVISIONXXX}}` time functions.
    #[inline]
    fn revision_time_impl<F>(
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
        write_time: F,
    ) -> Result
    where
        F: FnOnce(UtcDateTime) -> fmt::Result,
    {
        if let Some(time) =
            get_article(state, arguments)?.map(|article| article.revision_timestamp())
        {
            write_time(time)?;
        }
        Ok(())
    }

    /// `{{REVISIONDAY[: title] }}`
    pub fn revision_day(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        revision_time_impl(state, arguments, |time| write!(out, "{}", time.day()))
    }

    /// `{{REVISIONDAY2[: title] }}`
    pub fn revision_day_lz(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        revision_time_impl(state, arguments, |time| write!(out, "{:02}", time.day()))
    }

    /// `{{REVISIONMONTH1[: title] }}`
    pub fn revision_month(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        revision_time_impl(state, arguments, |time| {
            write!(out, "{}", u8::from(time.month()))
        })
    }

    /// `{{REVISIONMONTH[: title] }}`
    pub fn revision_month_lz(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        revision_time_impl(state, arguments, |time| {
            write!(out, "{:02}", u8::from(time.month()))
        })
    }

    /// `{{REVISIONTIMESTAMP[: title] }}`
    pub fn revision_timestamp(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        revision_time_impl(state, arguments, |time| {
            write!(
                out,
                "{}{:02}{:02}{:02}{:02}{:02}",
                time.year(),
                u8::from(time.month()),
                time.day(),
                time.hour(),
                time.minute(),
                time.second()
            )
        })
    }

    /// `{{REVISIONUSER[: title] }}`
    pub fn revision_user(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let revision = get_article(state, arguments)?;
        if let Some(author) = revision.as_ref().map(|article| article.revision_author()) {
            write!(out, "{author}")?;
        }
        Ok(())
    }

    /// `{{REVISIONYEAR[: title] }}`
    pub fn revision_year(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        revision_time_impl(state, arguments, |time| write!(out, "{}", time.year()))
    }

    /// `{{ROOTPAGENAME}}`
    pub fn root_page_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        page_name_impl(out, state, arguments, Title::root_text, Title::root_url)
    }

    /// `{{[settable variable name]: value [| option ...]}}`
    pub fn set_page_var(
        _: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)? {
            state
                .globals
                .variables
                .insert(arguments.callee.to_owned(), value.to_string());
        }

        Ok(())
    }

    /// `{{SUBPAGENAME}}`
    pub fn sub_page_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        page_name_impl(
            out,
            state,
            arguments,
            Title::subpage_text,
            Title::subpage_url,
        )
    }

    /// `{{ARTICLEPAGENAME}}` or `{{SUBJECTPAGENAME}}`
    pub fn subject_page_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        related_page_name(out, state, arguments, Title::subject)
    }

    /// `{{TALKPAGENAME}}`
    pub fn talk_page_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        related_page_name(out, state, arguments, Title::talk)
    }
}

mod site {
    //! Site information parser functions.

    use super::*;

    /// `{{CONTENTLANGUAGE}}` or `{{PAGELANGUAGE}}`
    pub fn content_language(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.db.config().language)?;
        Ok(())
    }

    /// `{{NUMBEROFFILES[:flag] }}`
    pub fn number_of_files(
        out: &mut String,
        _: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // The multistream bz2 database format includes no stats
        write!(out, "0")?;
        Ok(())
    }

    /// `{{NUMBEROFPAGES[:flag] }}`
    #[expect(
        clippy::cast_precision_loss,
        reason = "if there are ever ≥2**53 articles, the singularity will have occurred and our new AI overlords can adjust this to fix the slight statistical inaccuracy"
    )]
    pub fn number_of_pages(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let no_separators = arguments
            .eval(state, 0)?
            .is_some_and(|arg| magic_matches(state, RAW_SUFFIX, &arg));
        write!(
            out,
            "{}",
            format_number(state.statics.db.len() as f64, no_separators)
        )?;
        Ok(())
    }

    /// `{{PAGESINCATEGORY: category [|flag] }}`
    pub fn pages_in_category(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if !arguments.is_empty() {
            let no_separators = arguments
                .eval(state, 1)?
                .is_some_and(|arg| magic_matches(state, RAW_SUFFIX, &arg));
            write!(out, "{}", format_number(1.0, no_separators))?;
        }

        Ok(())
    }

    /// `{{SERVER}}`
    pub fn server(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(authority) = state.statics.base_uri.authority() {
            write!(out, "//{authority}")?;
        }
        Ok(())
    }

    /// `{{SERVERNAME}}`
    pub fn server_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(authority) = state.statics.base_uri.authority() {
            write!(out, "{authority}")?;
        }
        Ok(())
    }

    /// `{{SITENAME}}`
    pub fn site_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.db.name())?;
        Ok(())
    }
}

mod string {
    //! String manipulation functions.

    use super::*;

    /// `{{anchorencode: text }}`
    pub fn anchor_encode(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(text) = arguments.eval(state, 0)?.map(trim) {
            let text = strip::kill(&text);
            write!(
                out,
                "{}",
                super::anchor_encode(&text, AnchorEncodeMode::Html5)
            )?;
        }

        Ok(())
    }

    /// `{{formatnum: number [|flag [|flag]] }}`
    pub fn format_number(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        bitflags::bitflags! {
            /// Number formatting flags.
            pub struct Flags: u8 {
                /// Remove number formatting from a string.
                const REVERSE = 1;
                /// Format a number with no separators. For English, this is a
                /// no-op.
                const NO_SEPARATORS = 2;
                /// Return the original unformatted number if parsing the number
                /// causes a loss of precision.
                const LOSSLESS = 4;
            }
        }

        impl FromStr for Flags {
            type Err = anyhow::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(match s {
                    RAW_SUFFIX => Self::REVERSE,
                    NO_SEPARATORS => Self::NO_SEPARATORS,
                    LOSSLESS => Self::LOSSLESS,
                    _ => return Err(anyhow::anyhow!("unknown flag '{s}'")),
                })
            }
        }

        /// Formats a numeric string.
        fn format_part(flags: Flags) -> impl Fn(&str) -> Option<Cow<'_, str>> {
            move |mut s| {
                let no_separators = flags.contains(Flags::NO_SEPARATORS);
                let lossless = flags.contains(Flags::LOSSLESS);

                // MW used this unpleasant regex along with a callback:
                // '(-(?=[\d\.]))?(\d+|(?=\.\d))(\.\d*)?([Ee][-+]?\d+)?'
                // which is not really any different than just trying every
                // position and seeing if it succeeds to parse as a float,
                // except slower
                let mut out = String::new();
                while !s.is_empty() {
                    if let Ok((n, rest)) = floatval(s) {
                        let formatted = super::format_number(n, no_separators);
                        if lossless
                            && let original = &s[..s.len() - rest.len()]
                            && parse_formatted_number(&formatted) != original
                        {
                            out += original;
                        } else {
                            out += &formatted;
                        }
                        s = rest;
                    } else {
                        let c = s.chars().next().unwrap();
                        out.push(c);
                        s = &s[c.len_utf8()..];
                    }
                }
                Some(out.into())
            }
        }

        if let Some(n) = arguments.eval(state, 0)?.map(trim) {
            let flags = {
                let mut flags = Flags::empty();
                for index in 1..=2 {
                    if let Some(flag) = arguments.eval(state, index)?.and_then(|arg| {
                        magic_flag(state, &[LOSSLESS, NO_SEPARATORS, RAW_SUFFIX], &arg)
                    }) {
                        match Flags::from_str(flag) {
                            Ok(flag) => flags |= flag,
                            Err(err) => log::warn!("#formatnum: {err}"),
                        }
                    }
                }
                flags
            };

            let value = if flags.contains(Flags::REVERSE) {
                strip::for_each_non_marker(&n, |s| parse_formatted_number(s).owned())
            } else {
                strip::for_each_non_marker(&n, format_part(flags))
            };

            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{int: message name }}`
    pub fn interface_message(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(which) = arguments.eval(state, 0)?.map(trim) {
            // TODO: This is supposed to check for /lang first,
            // then fall back to non-lang.
            let title = Title::new(
                state.statics.db.config(),
                &libphp_rs::ucfirst(&which),
                Some(Namespace::MEDIAWIKI),
            );

            let message = match title.ok().map(|title| state.statics.db.get(&title)) {
                Some(Ok(article)) => {
                    // TODO: Is this supposed to follow redirects?
                    // TODO: This should probably identify the frame by the
                    // title instead of anonymous text.
                    let message =
                        preprocess_frame(state, arguments.sp, article.body(), ExpandMode::Normal)?;
                    format_raw_message(&message, |key| {
                        let index = key.parse::<usize>().unwrap();
                        arguments.eval(state, index)
                    })?
                    .into_owned()
                    .into()
                }
                None | Some(Err(DatabaseError::NotFound)) => {
                    format_message(state.messages, [which], |key| {
                        let index = key.parse::<usize>().unwrap();
                        arguments.eval(state, index)
                    })?
                }
                Some(Err(err)) => return Err(err)?,
            };
            write!(out, "{message}")?;
        }
        Ok(())
    }

    /// `{{lc: string }}`
    pub fn lc(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            write!(
                out,
                "{}",
                strip::for_each_non_marker(&value, |value| { Some(value.to_lowercase().into()) })
            )?;
        }
        Ok(())
    }

    /// `{{lcfirst: string }}`
    pub fn lc_first(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            let mut text = value.chars();
            if let Some(first) = text.next() {
                write!(out, "{}{}", first.to_lowercase(), text.as_str())?;
            }
        }
        Ok(())
    }

    /// Common implementation for all `{{#padXXX}}` functions.
    fn pad_impl<const LEFT: bool>(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let (Some(value), Some(len)) = (
            arguments.eval(state, 0)?.map(trim),
            arguments.eval(state, 1)?.map(trim),
        ) {
            if !LEFT {
                write!(out, "{value}")?;
            }
            let len = len.parse::<usize>().unwrap_or(0);
            if value.len() < len {
                let pad = arguments
                    .eval(state, 2)?
                    .map_or(Cow::Borrowed("0"), |pad| trim(pad).map(strip::kill));
                // log::trace!("padleft({value}, {len}, {pad})");
                if !pad.is_empty() {
                    for c in iter::repeat(&pad)
                        .flat_map(|pad| pad.chars())
                        .take(len - value.len())
                    {
                        out.write_char(c)?;
                    }
                }
            }
            if LEFT {
                write!(out, "{value}")?;
            }
        }
        Ok(())
    }

    /// `{{padleft: string | length [| padding value] }}`
    pub fn pad_left(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        pad_impl::<true>(out, state, arguments)
    }

    /// `{{padright: string | length [| padding value] }}`
    pub fn pad_right(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        pad_impl::<false>(out, state, arguments)
    }

    /// `{{plural: number [| [number = ] variant ...] }}`
    pub fn plural(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            let n = value
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse::<i32>()
                .unwrap_or(0)
                .abs();
            // log::trace!("#plural: {value} = {n}");
            let index = usize::from(n != 1);
            if let Some(value) = arguments.eval(state, 1 + index)?.map(trim) {
                write!(out, "{value}")?;
            }
        }

        Ok(())
    }

    /// `{{#titleparts: title [| len [| start]] }}`
    pub fn title_parts(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let page_name = arguments.eval(state, 0)?.unwrap_or_default();
        let page_name = decode_html(&page_name);
        let return_count = arguments
            .eval(state, 1)?
            .map_or(0, |len| len.trim().parse::<i32>().unwrap_or(0));
        let start_at = arguments
            .eval(state, 2)?
            .map_or(1, |len| len.trim().parse::<i32>().unwrap_or(1));

        let title = page_name.split('/');
        let (return_count, start_at) = if return_count < 0 || start_at < 0 {
            let count = i32::try_from(title.clone().count()).unwrap();

            let return_count = usize::try_from(if return_count < 0 {
                count + return_count
            } else {
                return_count
            })
            .unwrap();

            let start_at = usize::try_from(if start_at < 0 {
                count + start_at
            } else {
                start_at
            })
            .unwrap();

            (return_count, start_at)
        } else {
            (
                usize::try_from(return_count).unwrap(),
                usize::try_from(start_at).unwrap(),
            )
        };

        // `#[feature(iter_intersperse)]` any day now
        // TODO: This needs to entity-encode output. (Or the `fmt::Write`
        // interface needs to guarantee it and nothing shall use that to write
        // HTML.)
        for (index, part) in title.skip(start_at - 1).take(return_count).enumerate() {
            if index != 0 {
                out.write_char('/')?;
            }
            write!(out, "{part}")?;
        }

        Ok(())
    }

    /// `{{uc: string }}`
    pub fn uc(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            write!(
                out,
                "{}",
                strip::for_each_non_marker(&value, |value| { Some(value.to_uppercase().into()) })
            )?;
        }
        Ok(())
    }

    /// `{{ucfirst: string }}`
    pub fn uc_first(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            let mut text = value.chars();
            if let Some(first) = text.next() {
                write!(out, "{}{}", first.to_uppercase(), text.as_str())?;
            }
        }
        Ok(())
    }

    /// `{{urlencode: string }}`
    pub fn url_encode(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        const URL_PATH: &str = "url_path";
        const URL_WIKI: &str = "url_wiki";

        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            let value = strip::kill(&value);
            match arguments
                .eval(state, 1)?
                .and_then(|arg| magic_flag(state, &[URL_PATH, URL_WIKI], &arg))
            {
                Some(URL_PATH) => {
                    write!(out, "{}", libphp_rs::raw_url_encode(&value))?;
                }
                Some(URL_WIKI) => {
                    write!(
                        out,
                        "{}",
                        libwikitext_common::url_encode(&strtr(&value, &[(" ", "_")]))
                    )?;
                }
                _ => {
                    write!(out, "{}", libphp_rs::url_encode(&value))?;
                }
            }
        }
        Ok(())
    }
}

// TODO: All the 'current' times should be UTC, and 'local' in the local time,
// and they should be relative to the database dump time.
mod time {
    //! Date and time functions.

    use super::*;

    /// `{{LOCALTIME}}` or `{{CURRENTTIME}}`
    pub fn clock_time(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let time = &state.statics.base_time;
        write!(out, "{:02}:{:02}", time.hour(), time.minute())?;
        Ok(())
    }

    /// `{{LOCALDAY}}` or `{{CURRENTDAY}}`
    pub fn day(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.base_time.day())?;
        Ok(())
    }

    /// `{{LOCALDAY2}}` or `{{CURRENTDAY2}}`
    pub fn day_lz(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{:02}", state.statics.base_time.day())?;
        Ok(())
    }

    /// `{{LOCALDAYNAME}}` or `{{CURRENTDAYNAME}}`
    pub fn day_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.base_time.weekday())?;
        Ok(())
    }

    /// `{{LOCALDOW}}` or `{{CURRENTDOW}}`
    pub fn day_of_week(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(
            out,
            "{}",
            state.statics.base_time.weekday().number_days_from_sunday()
        )?;
        Ok(())
    }

    /// `{{#formatdate:date[| format] }}`
    pub fn format_date(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        #[derive(Clone, Copy)]
        enum Year {
            None,
            Dmy(i16),
            Iso(i16),
            Mdy(i16),
            Ymd(i16),
        }

        impl core::fmt::Display for Year {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                struct NamedYear(i16);
                impl core::fmt::Display for NamedYear {
                    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                        if self.0 <= 0 {
                            write!(f, "{} BC", self.0.abs() + 1)
                        } else {
                            write!(f, "{}", self.0)
                        }
                    }
                }
                match *self {
                    Self::None => Ok(()),
                    Self::Dmy(y) => write!(f, " {}", NamedYear(y)),
                    Self::Iso(y) => write!(f, "{y:00$}-", 4 + usize::from(y < 0)),
                    Self::Mdy(y) => write!(f, ", {}", NamedYear(y)),
                    Self::Ymd(y) => write!(f, "{}, ", NamedYear(y)),
                }
            }
        }

        if let Some(date) = arguments.eval(state, 0)?.map(trim) {
            // TODO: Use global locale.
            let locale = &Locale::en;
            if let Ok((y, m, d)) = simple_date::date(&date, locale) {
                let m = u8::from(m);
                let m_named = locale.months_wide()[usize::from(m - 1)];
                let y_iso = y.map_or(Year::None, Year::Iso);
                let iso = format_args!("{y_iso}{m:02}-{d:02}");

                write!(out, r#"<span class="mw-formatted-date" title="{iso}">"#)?;

                match arguments.eval(state, 1)?.map(trim).as_deref() {
                    Some("dmy") => write!(out, "{d} {m_named}{}", y.map_or(Year::None, Year::Dmy))?,
                    Some("mdy" | "default") => {
                        write!(out, "{m_named} {d}{}", y.map_or(Year::None, Year::Mdy))?;
                    }
                    Some("ymd") => write!(out, "{}{m_named} {d}", y.map_or(Year::None, Year::Ymd))?,
                    Some("ISO 8601") => write!(out, "{iso}")?,
                    _ => write!(out, "{date}")?,
                }
                write!(out, "</span>")?;
            } else {
                write!(out, "{date}")?;
            }
        }

        Ok(())
    }

    /// `{{LOCALHOUR}}` or `{{CURRENTHOUR}}`
    pub fn hour(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{:02}", state.statics.base_time.hour())?;
        Ok(())
    }

    /// `{{LOCALMONTH1}}` or `{{CURRENTMONTH1}}`
    pub fn month(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", u8::from(state.statics.base_time.month()))?;
        Ok(())
    }

    /// `{{LOCALMONTHABBREV}}` or `{{CURRENTMONTHABBREV}}`
    pub fn month_abbr(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{:.3}", state.statics.base_time.month())?;
        Ok(())
    }

    /// `{{LOCALMONTH}}` or `{{LOCALMONTH2}}}}` or `{{CURRENTMONTH}}` or
    /// `{{CURRENTMONTH2}}`
    pub fn month_lz(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{:02}", u8::from(state.statics.base_time.month()))?;
        Ok(())
    }

    /// `{{LOCALMONTHNAME}}` or `{{LOCALMONTHNAMEGEN}}}}` or
    /// `{{CURRENTMONTHNAME}}` or `{{CURRENTMONTHNAMEGEN}}`
    pub fn month_name(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.base_time.month())?;
        Ok(())
    }

    /// `{{#time: format [| time [| language code [| local ]]] }}`
    pub fn time(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(format) = arguments.eval(state, 0)?.map(trim) {
            let date = arguments.eval(state, 1)?.map(trim);
            let local = arguments
                .eval(state, 3)?
                .map(trim)
                .is_some_and(|local| !local.trim_ascii().is_empty());

            // 'Template:Date' sends garbage values to `#time` without an
            // `#iferror` guard to capture the errors.
            match on_error_resume_next(format_date_mediawiki(
                &state.statics.base_time,
                &format,
                date.as_deref(),
                local,
            )) {
                Ok(result) => {
                    write!(out, "{result}")?;
                }
                Err(err) => write!(out, "{err}")?,
            }
        }
        Ok(())
    }

    /// `{{LOCALTIMESTAMP}}` or `{{CURRENTTIMESTAMP}}`
    pub fn timestamp(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let time = &state.statics.base_time;
        write!(
            out,
            "{}{:02}{:02}{:02}{:02}{:02}",
            time.year(),
            u8::from(time.month()),
            time.day(),
            time.hour(),
            time.minute(),
            time.second()
        )?;
        Ok(())
    }

    /// `{{LOCALWEEK}}` or `{{CURRENTWEEK}}`
    pub fn week(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.base_time.iso_week())?;
        Ok(())
    }

    /// `{{LOCALYEAR}}` or `{{CURRENTYEAR}}`
    pub fn year(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.base_time.year())?;
        Ok(())
    }

    // MediaWiki used some incoherent algorithm for formatting dates that tried
    // running every possible regular expression against the input and then the
    // replaced text, with some extra remappings for some reason, in order to
    // generate the output. But because the input is restricted to ISO 8601 or
    // named months, there is only ambiguity year vs day for years 1-31, and
    // otherwise the grammar is extremely simple.
    peg::parser! {grammar simple_date(locale: &Locale) for str {
        pub rule date() -> (Option<i16>, Month, u8)
        = y:iso_year() "-" m:iso_month() "-" d:iso_day() { (Some(y), m, d) }
        / md:month_day() y:(year_space() y:year() { y })? { (y, md.0, md.1) }
        / y:(y:year() year_space() { y })? md:month_day() { (y, md.0, md.1) }

        rule month_day() -> (Month, u8)
        = m:month() space() d:day() { (m, d) }
        / d:day() space() m:month() { (m, d) }

        rule month() -> Month
        = #{|input, pos| {
            let months = locale
                .months_wide()
                .iter()
                .chain(locale.months_abbreviated())
                .enumerate();

            for (n, month) in months {
                let input = input.get(pos..pos + month.len());
                if input.is_some_and(|input| {
                    input.to_lowercase() == month.to_lowercase()
                }) {
                    #[expect(clippy::cast_possible_truncation, reason = "guaranteed range")]
                    return peg::RuleResult::Matched(
                        pos + month.len(),
                        Month::January.nth_next(n as u8 % 12)
                    );
                }
            }
            peg::RuleResult::Failed
        }}

        rule year() -> i16
        = y:$(digit()*<1,4>) sign:(space() ['B'|'b'] ['C'|'c'])?
        { y.parse::<i16>().unwrap() - i16::from(sign.is_some()) }

        rule iso_year() -> i16
        = y:$("-"? digit()*<4,4>)
        { y.parse().unwrap() }

        rule iso_month() -> Month
        = m:$("1" ['0'..='2'] / "0" ['1'..='9'])
        { Month::December.nth_next(m.parse().unwrap()) }

        rule iso_day() -> u8
        = d:$("3" ['0'|'1'] / ['1'|'2'] ['0'..='9'] / "0" ['1'..='9'])
        { d.parse().unwrap() }

        rule day() -> u8
        = d:$("3" ['0'|'1'] / ['1'|'2'] ['0'..='9'] / "0"? ['1'..='9'])
        { d.parse().unwrap() }

        rule year_space()
        = " "* "," " "*
        / " "+

        rule space()
        = [' '|'_']

        rule digit()
        = ['0'..='9']
    }}
}

mod title {
    //! Article title functions.

    use super::*;

    /// `{{canonicalurl: title [| query string] }}`
    pub fn canonical_url(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        url_impl(out, state, arguments, |uri| uri.scheme_str().or(Some("")))
    }

    /// `{{filepath: title [| 'nowiki'/size [| size/'nowiki']] }}`
    pub fn file_path(
        _: &mut String,
        _: &mut State<'_, '_, '_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // Normally this would look up a file, optionally picking one based on
        // the given size hint, but since no files are included in the database
        // dump, this can just always return nothing.
        Ok(())
    }

    /// `{{fullurl: title [| query string] }}`
    pub fn full_url(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        url_impl(out, state, arguments, |_| Some(""))
    }

    /// `{{#ifexist: title | consequent (exists) | alternate }}`
    pub fn if_exist(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // log::trace!("#ifexist: '{value:?}'");
        let exists = arguments.eval(state, 0)?.map(trim).is_some_and(|value| {
            let Ok(title) = Title::new(state.statics.db.config(), &value, None) else {
                return false;
            };
            state.statics.db.contains(&title)
        });
        if let Some(value) = arguments.eval(state, 1 + usize::from(!exists))?.map(trim) {
            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{localurl: title [| query string] }}`
    pub fn local_url(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        url_impl(out, state, arguments, |_| None)
    }

    /// `{{NAMESPACE[:title] }}`
    pub fn namespace(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        namespace_impl(out, state, arguments, |ns, _| Some(ns))
    }

    /// `{{ns: namespace name or id }}`
    pub fn namespace_by_name_or_id(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let ns = arguments.eval(state, 0)?.map(trim).and_then(|value| {
            if let Ok(id) = value.parse::<i32>() {
                Namespace::find_by_id(state.statics.db.config(), id)
            } else {
                Namespace::find_by_name(state.statics.db.config(), &value)
            }
        });

        if let Some(ns) = ns {
            if arguments.callee.ends_with('e') {
                write!(out, "{}", url_encode(&strtr(ns.name, &[(" ", "_")])))?;
            } else {
                write!(out, "{}", ns.name)?;
            }
        }

        Ok(())
    }

    /// `{{NAMESPACE[:title] }}` or `{{NAMESPACENUMBER[:title] }}` or
    /// `{{SUBJECTSPACE[:title] }}` or `{{ARTICLESPACE[:title] }}` or
    /// `{{TALKSPACE[:title] }}`
    #[inline]
    fn namespace_impl<'a, F>(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments @ IndexedArgs { callee, .. }: &IndexedArgs<'_, '_, '_>,
        f: F,
    ) -> Result
    where
        F: FnOnce(&'a Namespace, &Configuration) -> Option<&'a Namespace>,
    {
        let ns = if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            let Ok(title) = Title::new(state.statics.db.config(), &value, None) else {
                return Ok(());
            };
            title.namespace()
        } else {
            state.globals.title.namespace()
        };

        if *callee == "namespacenumber" {
            write!(out, "{}", ns.id)?;
        } else if let Some(ns) = f(ns, state.statics.db.config()) {
            let as_uri = callee.ends_with("ee");
            if as_uri {
                write!(out, "{}", url_encode(&strtr(ns.name, &[(" ", "_")])))?;
            } else {
                write!(out, "{}", strtr(ns.name, &[("_", " ")]))?;
            }
        }

        Ok(())
    }

    /// `{{SUBJECTSPACE[:title] }}` or `{{ARTICLESPACE[:title] }}`
    pub fn subject_space(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        namespace_impl(out, state, arguments, Namespace::subject)
    }

    /// `{{TALKSPACE[:title] }}`
    pub fn talk_space(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        namespace_impl(out, state, arguments, Namespace::talk)
    }

    /// `{{#lst:title | section [| replacement text] }}`
    pub fn transclude_except(
        _: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        log::warn!(
            "TODO: #lst({:?}, {:?}",
            arguments.eval(state, 0)?,
            arguments.eval(state, 1)?
        );
        Ok(())
    }

    /// `{{#lsth:title | section [| replacement text] }}`
    pub fn transclude_heading(
        _: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        log::warn!(
            "TODO: #lsth({:?}, {:?}",
            arguments.eval(state, 0)?,
            arguments.eval(state, 1)?
        );
        Ok(())
    }

    /// `{{#lst:title | section [| end section] }}`
    pub fn transclude_section(
        _: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        log::warn!(
            "TODO: #lstx({:?}, {:?}",
            arguments.eval(state, 0)?,
            arguments.eval(state, 1)?
        );
        Ok(())
    }

    /// Common implementation for all URL generation functions.
    #[inline]
    fn url_impl(
        out: &mut String,
        state: &mut State<'_, '_, '_>,
        arguments: &IndexedArgs<'_, '_, '_>,
        scheme: impl FnOnce(&http::Uri) -> Option<&str>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim)
            && let Ok(title) = Title::new(state.statics.db.config(), &value, None)
        {
            let query = arguments.eval(state, 1)?.map(trim);
            let url = make_url(
                &state.statics.base_uri,
                scheme(&state.statics.base_uri),
                format_args!("{}/{}", state.statics.paths.article, title.partial_url()),
                query.as_deref(),
                title.fragment(),
            );
            write!(out, "{url}")?;
        }

        Ok(())
    }
}

/// Known parser functions.
static PARSER_FUNCTIONS: phf::Map<&'static str, ParserFn> = phf::phf_map! {
    "!" => |out: &mut String, _, _| { out.write_char('|')?; Ok(()) },
    "=" => |out: &mut String, _, _| { out.write_char('=')?; Ok(()) },

    "expr" => cond::expr,
    "if" => cond::r#if,
    "ifeq" => cond::if_eq,
    "iferror" => cond::if_error,
    "ifexpr" => cond::if_expr,
    "switch" => cond::switch,

    "coordinates" => ext::geodata_coordinates,
    "invoke" => ext::invoke,
    "property" => ext::wikibase_property,
    "tag" => ext::extension_tag,

    "basepagename" => page::base_page_name,
    "basepagenamee" => page::base_page_name,
    "defaultsort" => page::set_page_var,
    "displaytitle" => page::set_page_var,
    "fullpagename" => page::full_page_name,
    "fullpagenamee" => page::full_page_name,
    "getshortdesc" => page::page_var,
    "pageid" => page::page_id,
    // TODO: This information does not appear to be recorded in the database,
    // and does not actually seem to be page-specific but rather user-specific
    // (and then test things can override it)?
    "pagelanguage" => site::content_language,
    "pagename" => page::page_name,
    "pagenamee" => page::page_name,
    "pagesize" => page::page_size,
    "protectionexpiry" => page::protection_expiry,
    "revisionday" => page::revision_day,
    "revisionday2" => page::revision_day_lz,
    "revisionid" => page::revision_id,
    "revisionmonth" => page::revision_month_lz,
    "revisionmonth1" => page::revision_month,
    "revisionsize" => page::page_size,
    "revisiontimestamp" => page::revision_timestamp,
    "revisionuser" => page::revision_user,
    "revisionyear" => page::revision_year,
    "rootpagename" => page::root_page_name,
    "rootpagenamee" => page::root_page_name,
    "shortdesc" => page::set_page_var,
    "subjectpagename" => page::subject_page_name,
    "subjectpagenamee" => page::subject_page_name,
    "subpagename" => page::sub_page_name,
    "subpagenamee" => page::sub_page_name,
    "talkpagename" => page::talk_page_name,
    "talkpagenamee" => page::talk_page_name,

    "contentlanguage" => site::content_language,
    "numberoffiles" => site::number_of_files,
    "numberofpages" => site::number_of_pages,
    "pagesincategory" => site::pages_in_category,
    "server" => site::server,
    "servername" => site::server_name,
    "sitename" => site::site_name,
    "userlanguage" => site::content_language,

    "anchorencode" => string::anchor_encode,
    "formatnum" => string::format_number,
    "int" => string::interface_message,
    "lc" => string::lc,
    "lcfirst" => string::lc_first,
    "padleft" => string::pad_left,
    "padright" => string::pad_right,
    "plural" => string::plural,
    "titleparts" => string::title_parts,
    "uc" => string::uc,
    "ucfirst" => string::uc_first,
    "urlencode" => string::url_encode,

    "currentday" => time::day,
    "currentday2" => time::day_lz,
    "currentdayname" => time::day_name,
    "currentdow" => time::day_of_week,
    "currenthour" => time::hour,
    "currentmonth" => time::month_lz,
    "currentmonth1" => time::month,
    "currentmonth2" => time::month_lz,
    "currentmonthabbrev" => time::month_abbr,
    "currentmonthname" => time::month_name,
    "currentmonthnamegen" => time::month_name,
    "currenttime" => time::clock_time,
    "currenttimestamp" => time::timestamp,
    "currentweek" => time::week,
    "currentyear" => time::year,
    "formatdate" => time::format_date,
    "localday" => time::day,
    "localday2" => time::day_lz,
    "localdayname" => time::day_name,
    "localdow" => time::day_of_week,
    "localhour" => time::hour,
    "localmonth" => time::month_lz,
    "localmonth1" => time::month,
    "localmonth2" => time::month_lz,
    "localmonthabbrev" => time::month_abbr,
    "localmonthname" => time::month_name,
    "localmonthnamegen" => time::month_name,
    "localtime" => time::clock_time,
    "localtimestamp" => time::timestamp,
    "localweek" => time::week,
    "localyear" => time::year,
    "time" => time::time,

    "canonicalurl" => title::canonical_url,
    "canonicalurle" => title::canonical_url,
    "filepath" => title::file_path,
    "fullurl" => title::full_url,
    "fullurle" => title::full_url,
    "ifexist" => title::if_exist,
    "localurl" => title::local_url,
    "localurle" => title::local_url,
    "lst" => title::transclude_section,
    "lsth" => title::transclude_heading,
    "lstx" => title::transclude_except,
    "namespace" => title::namespace,
    "namespacee" => title::namespace,
    "namespacenumber" => title::namespace,
    "ns" => title::namespace_by_name_or_id,
    "nse" => title::namespace_by_name_or_id,
    "subjectspace" => title::subject_space,
    "subjectspacee" => title::subject_space,
    "talkspace" => title::talk_space,
    "talkspacee" => title::talk_space,
};

/// Flag for lossless number formatting.
const LOSSLESS: &str = "lossless";
/// Flag for number output without delimiters.
const NO_SEPARATORS: &str = "nocommafysuffix";
/// Flag for raw number output.
const RAW_SUFFIX: &str = "rawsuffix";

/// Renders a parser function.
pub fn call_parser_fn(
    out: &mut String,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    bounds: Option<Span>,
    callee: &str,
    arguments: &[Kv<'_>],
) -> Result<(), Error> {
    let args = IndexedArgs {
        arguments: KeyCacheKvs::new(arguments),
        callee,
        sp,
        span: bounds,
    };
    if let Some(parser_fn) = state.statics.parser_fns.get(callee) {
        parser_fn
            .call(out, &mut PluginState(state), PluginFnArgs(&args))
            .map_err(Error::Plugin)?;
        Ok(())
    } else if let Some(parser_fn) = PARSER_FUNCTIONS.get(callee) {
        parser_fn(out, state, &args).map_err(|err| {
            if let Some(bounds) = bounds {
                Error::Node {
                    frame: sp.name.to_string() + "$" + callee,
                    start: sp.source.find_line_col(bounds.start),
                    err: Box::new(err),
                }
            } else {
                err
            }
        })
    } else if let Some(callee) = state.statics.db.config().variables.get(callee).copied() {
        if let Some(value) = args.eval(state, 0)? {
            // log::trace!("Setting {callee} to {value}");
            state
                .globals
                .variables
                .insert((*callee).to_string(), value.to_string());
        } else if let Some(value) = state.globals.variables.get(callee) {
            write!(out, "{value}")?;
        }
        Ok(())
    } else {
        log::warn!("TODO: {callee}()");
        Ok(())
    }
}

/// Decodes HTML entities and trims ASCII whitespace from the value.
fn decode_trim(value: Cow<'_, str>) -> Cow<'_, str> {
    trim(value.map(decode_html))
}

/// Helper function for parser functions that operate either on the current page
/// or on a named page.
fn get_article(
    state: &mut State<'_, '_, '_>,
    arguments: &IndexedArgs<'_, '_, '_>,
) -> Result<Option<Arc<Article>>> {
    Ok(if let Some(title) = arguments.eval(state, 0)?.map(trim) {
        let title = Title::new(state.statics.db.config(), &title, None);
        match title.ok().map(|title| state.statics.db.get(&title)) {
            Some(Ok(article)) => Some(article),
            None | Some(Err(DatabaseError::NotFound)) => None,
            Some(Err(err)) => return Err(err.into()),
        }
    } else {
        state
            .statics
            .db
            .contains(&state.globals.title)
            .then(|| Arc::clone(&state.globals.article))
    })
}

/// Returns a function that tries to match the given `alias` to any of the
/// canonical representations given in `any_of`. Returns the matched canonical
/// representation, or `None` if the given `alias` did not match.
fn magic_flag(
    state: &State<'_, '_, '_>,
    any_of: &[&'static str],
    alias: &str,
) -> Option<&'static str> {
    let alias = alias.trim_ascii().to_lowercase();
    state
        .statics
        .db
        .config()
        .extra_words
        .get(&alias)
        .and_then(|canonical| {
            any_of
                .iter()
                .find(|candidate| canonical.contains(candidate))
                .copied()
        })
}

/// Returns a function that tries to convert the given `alias` to a canonical
/// representation, returning `true` if any of the possible representations is
/// `flag`.
fn magic_matches(state: &State<'_, '_, '_>, flag: &'static str, alias: &str) -> bool {
    let alias = alias.trim_ascii().to_lowercase();
    state
        .statics
        .db
        .config()
        .extra_words
        .get(&alias)
        .is_some_and(|canonical| canonical.contains(&flag))
}

/// Converts a `Result<T, E>` into a `Result<T, String>` to ignore errors like
/// it’s 1995.
fn on_error_resume_next<T, E: fmt::Display>(value: Result<T, E>) -> Result<T, String> {
    value.map_err(|err| {
        format!(
            r#"<span class="error">{}</span>"#,
            html_escape::encode_text(&err.to_string())
        )
    })
}

/// Trims ASCII whitespace from the value.
///
/// All registered parser functions that did not use the `SFH_OBJECT_ARGS` flag
/// would receive all arguments pre-expanded and implicitly trimmed. We do not
/// have such a flag concept, so these parser functions must trim their own
/// strings.
fn trim(value: Cow<'_, str>) -> Cow<'_, str> {
    value.map_ref(|value| value.trim_ascii())
}
