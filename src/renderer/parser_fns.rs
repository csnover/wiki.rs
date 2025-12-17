//! Parser function implementations.
//!
//! <https://www.mediawiki.org/wiki/Help:Extension:ParserFunctions>

// Clippy: Functions signatures all conform to a specific API; inline modules
// are clearer with wildcard import.
#![allow(clippy::unnecessary_wraps, clippy::wildcard_imports)]

use super::{
    Error, Result, State, StripMarkers, extension_tags,
    stack::{IndexedArgs, KeyCacheKvs, Kv, StackFrame},
    template::{call_module, call_template, is_function_call},
};
use crate::{
    common::{anchor_encode, decode_html, format_date, make_url, url_encode},
    config::CONFIG,
    expr,
    php::{format_number, fuzzy_cmp, parse_number},
    title::{Namespace, Title},
    wikitext::Span,
};
use core::{
    fmt::{self, Write as _},
    iter,
};
use either::Either;
use regex::Regex;
use std::{borrow::Cow, sync::LazyLock};

/// The function signature of a parser function.
type ParserFn = fn(&mut String, &mut State<'_>, &IndexedArgs<'_, '_, '_>) -> Result;

mod cond {
    //! Flow control parser functions.

    use super::*;

    /// `{{#expr: expression}}`
    pub fn expr(
        out: &mut String,
        state: &mut State<'_>,
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
        state: &mut State<'_>,
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
        state: &mut State<'_>,
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
        state: &mut State<'_>,
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
        state: &mut State<'_>,
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
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
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
            if rhs == "#default" && is_kv {
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
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let (Some(name), Some(body)) = (arguments.eval(state, 0)?, arguments.eval(state, 1)?) {
            let name = StripMarkers::kill(&name);
            let name = name.trim_ascii().to_ascii_lowercase();
            match extension_tags::render_extension_tag(
                state,
                arguments.sp,
                arguments.span,
                &name,
                &extension_tags::InArgs::ParserFn(&arguments.arguments[2..]),
                Some(&body),
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
        _: &mut State<'_>,
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
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        call_module(out, state, arguments.sp, &arguments.arguments)
    }

    /// `{{#property: name [| from = Qid] }}`
    pub fn wikibase_property(
        _: &mut String,
        state: &mut State<'_>,
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
        _: &mut State<'_>,
        IndexedArgs { sp, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", sp.root().name.base_text())?;
        Ok(())
    }

    /// `{{FULLPAGENAME}}`
    pub fn full_page_name(
        out: &mut String,
        _: &mut State<'_>,
        IndexedArgs { sp, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", sp.root().name.key())?;
        Ok(())
    }

    /// `{{PAGENAME}}`
    pub fn page_name(
        out: &mut String,
        _: &mut State<'_>,
        IndexedArgs { sp, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", sp.root().name.text())?;
        Ok(())
    }

    /// `{{PROTECTIONEXPIRY[: action [| pagename]] }}`
    pub fn protection_expiry(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // From <https://www.mediawiki.org/wiki/Manual:Checking_for_page_existence/PROTECTIONEXPIRY_method>:
        //
        // “The {{PROTECTIONEXPIRY}} parser function can be used to check
        //  whether a page exists. It returns `infinity` if the page exists and
        //  is not protected, the actual expiry time if it is protected, and the
        //  empty string if it doesn't exist.”
        let exists = arguments
            .eval(state, 1)?
            .map(trim)
            .is_none_or(|page_name| state.statics.db.contains(&page_name));
        if exists {
            write!(out, "infinity")?;
        }
        Ok(())
    }

    /// `{{[gettable variable name]}}`
    pub fn page_var(
        out: &mut String,
        state: &mut State<'_>,
        IndexedArgs { callee, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // TODO: Technically the value might be a tree
        if let Some(value) = state.globals.variables.get(*callee) {
            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{REVISIONID}}`
    pub fn revision_id(out: &mut String, _: &mut State<'_>, _: &IndexedArgs<'_, '_, '_>) -> Result {
        // TODO: For the purposes of debugging, it might be worthwhile to
        // make this a toggle which can be empty string instead, since MW
        // modules will emit more warnings in that case
        out.write_char('-')?;
        Ok(())
    }

    /// `{{ROOTPAGENAME}}`
    pub fn root_page_name(
        out: &mut String,
        _: &mut State<'_>,
        IndexedArgs { sp, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", sp.root().name.root_text())?;
        Ok(())
    }

    /// `{{[settable variable name]: value [| option ...]}}`
    pub fn set_page_var(
        _: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)? {
            state
                .globals
                .variables
                .insert(arguments.callee.to_string(), value.to_string());
        }

        Ok(())
    }

    /// `{{SUBPAGENAME}}`
    pub fn sub_page_name(
        out: &mut String,
        _: &mut State<'_>,
        IndexedArgs { sp, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", sp.root().name.subpage_text())?;
        Ok(())
    }

    /// `{{ARTICLEPAGENAME}}` or `{{SUBJECTPAGENAME}}`
    pub fn subject_page_name(
        out: &mut String,
        _: &mut State<'_>,
        IndexedArgs { sp, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let title = &sp.root().name;
        if let Some(subject) = title.namespace().subject() {
            write!(out, "{}:{}", subject.name, title.text())?;
        }
        Ok(())
    }

    /// `{{TALKPAGENAME}}`
    pub fn talk_page_name(
        out: &mut String,
        _: &mut State<'_>,
        IndexedArgs { sp, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let title = &sp.root().name;
        if let Some(talk) = title.namespace().talk() {
            write!(out, "{}:{}", talk.name, title.text())?;
        }
        Ok(())
    }
}

mod site {
    //! Site information parser functions.

    use super::*;

    /// `{{NUMBEROFPAGES[:flag] }}`
    // Clippy: If Wikipedia ever has more than 2**52 articles, the
    // singularity will have occurred and our new AI overlords can adjust
    // this to avoid a slight inaccuracy in output
    #[allow(clippy::cast_precision_loss)]
    pub fn number_of_pages(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let no_separators = arguments.eval(state, 0)?.map(trim).as_deref() == Some("R");
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
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if !arguments.is_empty() {
            let no_separators = arguments.eval(state, 1)?.map(trim).as_deref() == Some("R");
            write!(out, "{}", format_number(1.0, no_separators))?;
        }

        Ok(())
    }
}

mod string {
    //! String manipulation functions.

    use super::*;

    /// `{{anchorencode: text }}`
    pub fn anchor_encode(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(text) = arguments.eval(state, 0)?.map(trim) {
            let text = StripMarkers::kill(&text);
            write!(out, "{}", super::anchor_encode(&text))?;
        }

        Ok(())
    }

    /// `{{formatnum: number [|flag] }}`
    pub fn format_number(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(n) = arguments.eval(state, 0)?.map(trim)
            && !n.is_empty()
        {
            let no_separators = if let Some(flag) = arguments.eval(state, 1)? {
                // TODO: Deal with flags in a generic way
                if !flag.is_empty() && flag != "R" {
                    log::warn!("formatnum: unsupported flag {flag}");
                }
                flag == "R"
            } else {
                false
            };
            // log::trace!("formatnum:{n:?}");

            write!(
                out,
                "{}",
                StripMarkers::for_each_non_marker(&n, |mut s| {
                    // MW used this unpleasant regex along with a callback:
                    // '(-(?=[\d\.]))?(\d+|(?=\.\d))(\.\d*)?([Ee][-+]?\d+)?'
                    // which is not really any different than just trying every
                    // position and seeing if it succeeds to parse as a float,
                    // except slower
                    let mut out = String::new();
                    while !s.is_empty() {
                        if let Ok((n, rest)) = parse_number(s) {
                            out += &super::format_number(n, no_separators);
                            s = rest;
                        } else {
                            let c = s.chars().next().unwrap();
                            out.push(c);
                            s = &s[c.len_utf8()..];
                        }
                    }
                    Some(out.into())
                })
            )?;
        }

        Ok(())
    }

    /// `{{int: message name }}`
    pub fn interface_message(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            write!(out, "{value}")?;
        }
        Ok(())
    }

    /// `{{lc: string }}`
    pub fn lc(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            write!(
                out,
                "{}",
                StripMarkers::for_each_non_marker(&value, |value| {
                    Some(value.to_lowercase().into())
                })
            )?;
        }
        Ok(())
    }

    /// `{{lcfirst: string }}`
    pub fn lc_first(
        out: &mut String,
        state: &mut State<'_>,
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

    /// `{{padleft: string | length [| padding value] }}`
    pub fn pad_left(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let (Some(value), Some(len)) = (
            arguments.eval(state, 0)?.map(trim),
            arguments.eval(state, 1)?.map(trim),
        ) {
            let len = len.parse::<usize>().unwrap_or(0);
            if value.len() < len {
                let pad = arguments.eval(state, 2)?.map_or(Cow::Borrowed("0"), trim);
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
            write!(out, "{value}")?;
        }
        Ok(())
    }

    /// `{{plural: number [| [number = ] variant ...] }}`
    pub fn plural(
        out: &mut String,
        state: &mut State<'_>,
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
        state: &mut State<'_>,
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
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            write!(
                out,
                "{}",
                StripMarkers::for_each_non_marker(&value, |value| {
                    Some(value.to_uppercase().into())
                })
            )?;
        }
        Ok(())
    }

    /// `{{ucfirst: string }}`
    pub fn uc_first(
        out: &mut String,
        state: &mut State<'_>,
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
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            write!(out, "{}", super::url_encode(&value))?;
        }
        Ok(())
    }
}

mod subst {
    //! Substitution pseudo-functions.

    use super::*;

    /// `{{safesubst: template name [| ...] }}`
    ///
    /// Normally, template expressions are saved in the Wikitext and expanded
    /// at the time a page is rendered; `subst` and `safesubst` both cause
    /// the expression to be expanded at save time instead.
    ///
    /// `subst` does not expand recursively, so if a template *evaluates* to
    /// `{{subst:foo}}` (i.e. contains a horror like `{{{{{|subst:}}}foo}}`
    /// which prevents it from being expanded at the template’s own save
    /// time), the expanded text in the caller will be `{{subst:foo}}`.
    ///
    /// `safesubst` *does* expand recursively, so if a template evaluates to
    /// `{{safesubst:foo}}` (again by e.g. `{{{{{|safesubst:}}}foo}}`), the
    /// text in the caller will be the same as if it were written as
    /// `{{foo}}`.
    ///
    /// Parent          | Child                      | Output
    /// ----------------+----------------------------+-----------------------
    ///   At render time:
    /// ---------------------------------------------------------------------
    /// `{{foo}}`       | `{{bar}}`                  | content of bar
    /// `{{foo}}`       | `{{{{{|subst:}}}bar}}`     | `{{subst:bar}}`
    /// `{{foo}}`       | `{{{{{|safesubst:}}}bar}}` | content of bar
    /// ----------------+----------------------------+-----------------------
    ///   At save time:
    /// ----------------+----------------------------+-----------------------
    /// `{{subst:foo}}` | `{{bar}}`                  | `{{bar}}`
    /// `{{subst:foo}}` | `{{{{{|subst:}}}bar}}`     | content of bar
    /// `{{subst:foo}}` | `{{{{{|safesubst:}}}bar}}` | content of bar
    pub fn safesubst(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(target) = arguments.eval(state, 0)? {
            let target = target.trim_ascii();
            let (callee, rest) = target
                .split_once(':')
                .map_or((target, None), |(callee, rest)| (callee, Some(rest)));
            let callee_lower = callee.to_lowercase();

            let use_function_hook =
                is_function_call(arguments.len() == 1, rest.is_some(), &callee_lower);

            if use_function_hook {
                // TODO: There has got to be a better way to do this. Maybe the
                // first argument is just always passed into `call_parser_fn`
                // separately since it is the source of all of these problems.
                let args = rest
                    .map(Kv::Borrowed)
                    .into_iter()
                    .chain(arguments.arguments[1..].iter().cloned())
                    .collect::<Vec<_>>();

                call_parser_fn(
                    out,
                    state,
                    arguments.sp,
                    arguments.span,
                    &callee_lower,
                    &args,
                )?;
            } else {
                call_template(
                    out,
                    state,
                    arguments.sp,
                    Title::new(target, Namespace::find_by_id(Namespace::TEMPLATE)),
                    &arguments.arguments[1..],
                )?;
            }
        }

        Ok(())
    }

    /// `{{subst: template name [| ...] }}`
    ///
    /// Since wiki.rs is never in save mode, this will always just emit the
    /// original text.
    pub fn subst(
        out: &mut String,
        _: &mut State<'_>,
        IndexedArgs { sp, span, .. }: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(span) = span {
            write!(out, "{}", &sp.source[span.into_range()])?;
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
        state: &mut State<'_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let time = &state.statics.base_time;
        write!(out, "{:02}:{:02}", time.hour(), time.minute())?;
        Ok(())
    }

    /// `{{LOCALDAY}}` or `{{CURRENTDAY}}`
    pub fn day(out: &mut String, state: &mut State<'_>, _: &IndexedArgs<'_, '_, '_>) -> Result {
        write!(out, "{}", state.statics.base_time.day())?;
        Ok(())
    }

    /// `{{LOCALDAY2}}` or `{{CURRENTDAY2}}`
    pub fn day_lz(out: &mut String, state: &mut State<'_>, _: &IndexedArgs<'_, '_, '_>) -> Result {
        write!(out, "{:02}", state.statics.base_time.day())?;
        Ok(())
    }

    /// `{{LOCALDAYNAME}}` or `{{CURRENTDAYNAME}}`
    pub fn day_name(
        out: &mut String,
        state: &mut State<'_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.base_time.weekday())?;
        Ok(())
    }

    /// `{{LOCALDOW}}` or `{{CURRENTDOW}}`
    pub fn day_of_week(
        out: &mut String,
        state: &mut State<'_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(
            out,
            "{}",
            state.statics.base_time.weekday().number_days_from_sunday()
        )?;
        Ok(())
    }

    /// `{{LOCALHOUR}}` or `{{CURRENTHOUR}}`
    pub fn hour(out: &mut String, state: &mut State<'_>, _: &IndexedArgs<'_, '_, '_>) -> Result {
        write!(out, "{:02}", state.statics.base_time.hour())?;
        Ok(())
    }

    /// `{{LOCALMONTH1}}` or `{{CURRENTMONTH1}}`
    pub fn month(out: &mut String, state: &mut State<'_>, _: &IndexedArgs<'_, '_, '_>) -> Result {
        write!(out, "{}", u8::from(state.statics.base_time.month()))?;
        Ok(())
    }

    /// `{{LOCALMONTHABBREV}}` or `{{CURRENTMONTHABBREV}}`
    pub fn month_abbr(
        out: &mut String,
        state: &mut State<'_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{:.3}", state.statics.base_time.month())?;
        Ok(())
    }

    /// `{{LOCALMONTH}}` or `{{LOCALMONTH2}}}}` or `{{CURRENTMONTH}}` or
    /// `{{CURRENTMONTH2}}`
    // "localmonth" | "localmonth2" | "currentmonth" | "currentmonth2" => {
    pub fn month_lz(
        out: &mut String,
        state: &mut State<'_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{:02}", u8::from(state.statics.base_time.month()))?;
        Ok(())
    }

    /// `{{LOCALMONTHNAME}}` or `{{LOCALMONTHNAMEGEN}}}}` or
    /// `{{CURRENTMONTHNAME}}` or `{{CURRENTMONTHNAMEGEN}}`
    pub fn month_name(
        out: &mut String,
        state: &mut State<'_>,
        _: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        write!(out, "{}", state.statics.base_time.month())?;
        Ok(())
    }

    /// `{{#time: format [| time [| language code [| local ]]] }}`
    pub fn time(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(format) = arguments.eval(state, 0)?.map(trim) {
            let date = arguments.eval(state, 1)?.map(trim);
            let local = arguments
                .eval(state, 3)?
                .map(trim)
                .is_some_and(|local| local.trim_ascii() == "local");

            // 'Template:Date' sends garbage values to `#time` without an
            // `#iferror` guard to capture the errors.
            match on_error_resume_next(format_date(
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
        state: &mut State<'_>,
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
    pub fn week(out: &mut String, state: &mut State<'_>, _: &IndexedArgs<'_, '_, '_>) -> Result {
        write!(out, "{}", state.statics.base_time.iso_week())?;
        Ok(())
    }

    /// `{{LOCALYEAR}}` or `{{CURRENTYEAR}}`
    pub fn year(out: &mut String, state: &mut State<'_>, _: &IndexedArgs<'_, '_, '_>) -> Result {
        write!(out, "{}", state.statics.base_time.year())?;
        Ok(())
    }
}

mod title {
    //! Article title functions.

    use super::*;

    /// `{{fullurl: title [| query string] }}`
    pub fn full_url(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            let url = make_url(None, &state.statics.base_uri, &value, None, false)?;
            write!(out, "{url}")?;
            if let Some(query) = arguments.eval(state, 1)?.map(trim) {
                write!(out, "?{query}")?;
            }
        }

        Ok(())
    }

    /// `{{#ifexist: title | consequent (exists) | alternate }}`
    pub fn if_exist(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        // log::trace!("#ifexist: '{value:?}'");
        let exists = arguments
            .eval(state, 0)?
            .map(trim)
            .is_some_and(|value| state.statics.db.contains(Title::new(&value, None).key()));
        if let Some(value) = arguments.eval(state, 1 + usize::from(!exists))?.map(trim) {
            write!(out, "{value}")?;
        }

        Ok(())
    }

    /// `{{NAMESPACE[:title] }}` or `{{NAMESPACENUMBER[:title] }}` or
    /// `{{SUBJECTSPACE[:title] }}` or `{{ARTICLESPACE[:title] }}` or
    /// `{{TALKSPACE[:title] }}`
    pub fn namespace(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let ns = if let Some(value) = arguments.eval(state, 0)?.map(trim) {
            Namespace::find_by_name(value.split_once(':').map_or("", |(ns, _)| ns))
        } else {
            Some(arguments.sp.root().name.namespace())
        };

        if let Some(ns) = ns {
            if arguments.callee == "namespace" {
                write!(out, "{}", ns.name)?;
            } else if arguments.callee == "articlespace" || arguments.callee == "subjectspace" {
                if let Some(ns) = ns.subject() {
                    write!(out, "{}", ns.name)?;
                }
            } else if arguments.callee == "talkspace" {
                if let Some(ns) = ns.talk() {
                    write!(out, "{}", ns.name)?;
                }
            } else {
                write!(out, "{}", ns.id)?;
            }
        }

        Ok(())
    }

    /// `{{ns: namespace name or id }}`
    pub fn namespace_by_name_or_id(
        out: &mut String,
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        let ns = arguments.eval(state, 0)?.map(trim).and_then(|value| {
            if let Ok(id) = value.parse::<i32>() {
                Namespace::find_by_id(id)
            } else {
                Namespace::find_by_name(&value)
            }
        });
        if let Some(ns) = ns {
            write!(out, "{}", ns.name)?;
        }

        Ok(())
    }

    /// `{{#lst:title | section [| replacement text] }}`
    pub fn transclude_except(
        _: &mut String,
        state: &mut State<'_>,
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
        state: &mut State<'_>,
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
        state: &mut State<'_>,
        arguments: &IndexedArgs<'_, '_, '_>,
    ) -> Result {
        log::warn!(
            "TODO: #lstx({:?}, {:?}",
            arguments.eval(state, 0)?,
            arguments.eval(state, 1)?
        );
        Ok(())
    }
}

/// Known parser functions.
static PARSER_FUNCTIONS: phf::Map<&'static str, ParserFn> = phf::phf_map! {
    "!" => |out: &mut String, _, _| { out.write_char('|')?; Ok(()) },
    "=" => |out: &mut String, _, _| { out.write_char('=')?; Ok(()) },

    "#expr" => cond::expr,
    "#if" => cond::r#if,
    "#ifeq" => cond::if_eq,
    "#iferror" => cond::if_error,
    "#ifexpr" => cond::if_expr,
    "#switch" => cond::switch,

    "#coordinates" => ext::geodata_coordinates,
    "#property" => ext::wikibase_property,
    "#tag" => ext::extension_tag,
    "tag" => ext::extension_tag,
    "#invoke" => ext::invoke,

    "articlepagename" => page::subject_page_name,
    "basepagename" => page::base_page_name,
    "defaultsort" => page::set_page_var,
    "displaytitle" => page::set_page_var,
    "fullpagename" => page::full_page_name,
    "getshortdesc" => page::page_var,
    "pagename" => page::page_name,
    "protectionexpiry" => page::protection_expiry,
    "revisionid" => page::revision_id,
    "rootpagename" => page::root_page_name,
    "shortdesc" => page::set_page_var,
    "subjectpagename" => page::subject_page_name,
    "subpagename" => page::sub_page_name,
    "talkpagename" => page::talk_page_name,

    "numberofpages" => site::number_of_pages,
    "pagesincategory" => site::pages_in_category,

    "anchorencode" => string::anchor_encode,
    "formatnum" => string::format_number,
    "int" => string::interface_message,
    "lc" => string::lc,
    "lcfirst" => string::lc_first,
    "padleft" => string::pad_left,
    "plural" => string::plural,
    "#plural" => string::plural,
    "#titleparts" => string::title_parts,
    "uc" => string::uc,
    "ucfirst" => string::uc_first,
    "urlencode" => string::url_encode,

    "safesubst" => subst::safesubst,
    "subst" => subst::subst,

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
    "currenttime" => time::clock_time,
    "currenttimestamp" => time::timestamp,
    "currentweek" => time::week,
    "currentyear" => time::year,
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
    "localtime" => time::clock_time,
    "localtimestamp" => time::timestamp,
    "localweek" => time::week,
    "localyear" => time::year,
    "#time" => time::time,

    "articlespace" => title::namespace,
    "subjectspace" => title::namespace,
    "fullurl" => title::full_url,
    "#ifexist" => title::if_exist,
    "#lst" => title::transclude_section,
    "#lsth" => title::transclude_heading,
    "#lstx" => title::transclude_except,
    "namespace" => title::namespace,
    "namespacenumber" => title::namespace,
    "ns" => title::namespace_by_name_or_id,
    "talkspace" => title::namespace,
};

/// Renders a parser function.
#[allow(clippy::too_many_lines)]
pub fn call_parser_fn(
    out: &mut String,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    bounds: Option<Span>,
    callee: &str,
    arguments: &[Kv<'_>],
) -> Result<(), Error> {
    let args = IndexedArgs {
        sp,
        callee,
        arguments: KeyCacheKvs::new(arguments),
        span: bounds,
    };
    if let Some(parser_fn) = PARSER_FUNCTIONS.get(callee) {
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
    } else if CONFIG.variables.contains(callee) {
        if let Some(value) = args.eval(state, 0)? {
            // log::trace!("Setting {callee} to {value}");
            state
                .globals
                .variables
                .insert(callee.to_string(), value.to_string());
        } else if let Some(value) = state.globals.variables.get(callee) {
            write!(out, "{value}")?;
        }
        Ok(())
    } else {
        log::warn!("TODO: {callee}()");
        Ok(())
    }
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

/// Decodes HTML entities and trims ASCII whitespace from the value.
fn decode_trim(value: Cow<'_, str>) -> Cow<'_, str> {
    match value {
        // This ugliness seems to be necessary to maintain the original lifetime
        // and satisfy borrowck
        Cow::Borrowed(value) => match decode_html(value) {
            Cow::Borrowed(value) => Cow::Borrowed(value.trim_ascii()),
            Cow::Owned(value) => Cow::Owned(value.trim_ascii().to_string()),
        },
        Cow::Owned(value) => Cow::Owned(decode_html(&value).trim_ascii().to_string()),
    }
}

/// Trims ASCII whitespace from the value.
///
/// All registered parser functions that did not use the `SFH_OBJECT_ARGS` flag
/// would receive all arguments pre-expanded and implicitly trimmed. We do not
/// have such a flag concept, so these parser functions must trim their own
/// strings.
fn trim(value: Cow<'_, str>) -> Cow<'_, str> {
    match value {
        Cow::Borrowed(value) => Cow::Borrowed(value.trim_ascii()),
        Cow::Owned(value) => Cow::Owned(value.trim_ascii().to_string()),
    }
}
