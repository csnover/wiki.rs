//! Template rendering types and functions.

use super::{
    Error, Result, State, StripMarker, StripMarkers,
    expand_templates::{ExpandMode, ExpandTemplates},
    parser_fns::call_parser_fn,
    resolve_redirects,
    stack::{KeyCacheKvs, Kv, StackFrame},
    surrogate::Surrogate,
};
use crate::{
    LoadMode,
    common::{make_url, title_decode},
    config::CONFIG,
    db::PrefetchPriority,
    lua::run_vm,
    title::{Namespace, Title},
    wikitext::{Argument, FileMap, MARKER_PREFIX, MARKER_SUFFIX, Span, Spanned, Token},
};
use core::fmt::{self, Write as _};
use memchr::memmem::FinderRev;
use std::{
    borrow::Cow,
    pin::pin,
    sync::{Arc, LazyLock},
    time::Instant,
};

/// Templates that need to be spruced up a bit, but don’t have any hooks of
/// their own for styling.
///
/// It is necessary to use a whitelist instead of just marking everything
/// because:
///
/// 1. 'Module:Citation/CS1' is unbearable and complains visibly if a strip
///    marker exists inside of any of its parameters instead of just ignoring or
///    stripping them itself (is it any wonder it runs so slowly?); and
/// 2. More importantly, 'Module:Infobox' (in `fixChildBoxes`) does that thing
///    that script writers love to do and uses pattern matching expressions to
///    find HTML tags in strings, and expects that those strings will be in a
///    very specific format without any changes to whitespace, strip markers,
///    etc., since that causes its anchored pattern match to fail. Probably
///    other modules do this too, but Infobox was the first one where the brain
///    damage was terminal for the patient. This is noticeable on at least
///    'Template:Nutritionalvalue'.
///
/// On the other hand, 'Module:Documentation' is also an ouroboros where
/// stripping all the wiki.rs markers from `mw.getExpandedArgument(s)` to avoid
/// breaking scripts generally means that *its* output ends up losing style
/// hooks that are necessary to not be ugly. So, whitelist it is!
static TACKY_TEMPLATES: phf::Set<&str> = phf::phf_set! {
    "Template:Ahnentafel",
    "Template:Article for improvement banner",
    "Template:Bar percent",
    "Template:Chart top",
    "Template:Climate chart",
    "Template:Climate chart/celsius column",
    "Template:Climate chart/celsius column i",
    "Template:Climate chart/fahrenheit column",
    "Template:Climate chart/fahrenheit column i",
    "Template:Football kit",
    "Template:Fossil range/bar",
    "Template:Historical populations",
    "Template:Largest cities",
    "Template:Markup",
    "Template:Phanerozoic 220px",
    "Template:Tree chart",
    "Template:Tree chart/start",
    "Template:Weather box",
};

/// Calls a Lua function.
pub(super) fn call_module(
    out: &mut String,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    arguments: &KeyCacheKvs<'_, '_>,
) -> Result {
    if state.load_mode != LoadMode::Module {
        return render_fallback(out, state, sp);
    }

    let Some(callee) = arguments.eval(state, sp, 0)? else {
        log::warn!("tried to call #invoke with no module name");
        return Ok(());
    };

    let Some(fn_name) = arguments.eval(state, sp, 1)? else {
        return Err(Error::MissingFunctionName);
    };

    let callee = Title::new(&callee, Namespace::find_by_id(Namespace::MODULE));

    let code = match state.statics.db.get(&callee) {
        Ok(code) => resolve_redirects(&state.statics.db, code)?,
        Err(err) => {
            log::warn!("could not load module {callee}: {err}");
            sp.backtrace();
            return Ok(());
        }
    };

    // TODO: The source code in the frame has to be the one associated with the
    // `arguments`, not the module source code, for arguments lookups to work
    // correctly, which is bad.
    let sp = sp.chain(callee, sp.source.clone(), &arguments[2..])?;

    // log::trace!("Invoking {}|{}", &code.title, fn_name);
    let now = Instant::now();
    let result = run_vm(state, pin!(&sp), &code, &fn_name).map_err(|err| Error::Module {
        name: code.title.clone(),
        fn_name: fn_name.to_string(),
        err: Box::new(err.into()),
    });

    state
        .timing
        .entry(sp.name.key().to_string())
        .and_modify(|(count, duration)| {
            *count += 1;
            *duration += now.elapsed();
        })
        .or_insert_with(|| (1, now.elapsed()));

    // 'Module:Maplink' absolutely relies on Wikidata, with no error guards,
    // relying on MW just emitting HTML whenever an error occurs instead of
    // any structured error handling.
    match result {
        Ok(result) => {
            write!(out, "{result}")?;
        }
        Err(err) => {
            let root_error = {
                let mut err = &err as &dyn std::error::Error;
                while let Some(source) = err.source() {
                    err = source;
                }
                err
            };
            log::error!("{}: {err:#}", sp.name);
            write!(out, r#"<span class="error">{root_error}</span>"#)?;
        }
    }

    Ok(())
}

/// Renders a parameter.
pub(super) fn render_parameter(
    out: &mut ExpandTemplates,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    span: Span,
    name: &[Spanned<Token>],
    default: Option<&[Spanned<Token>]>,
) -> Result {
    if state.load_mode == LoadMode::Base {
        return render_fallback(out, state, sp);
    }

    let key = sp.eval(state, name)?;

    if let Some(value) = sp.expand(state, key.trim_ascii())? {
        write!(out, "{value}")?;
    } else if let Some(default) = default {
        out.adopt_tokens(state, sp, default)?;
    } else {
        // This cannot simply adopt the whole text of the parameter as-is
        // because if the parameter contained inclusion control tags, e.g.
        // `{{{1<noinclude>|default</noinclude>}}}` then the wrong result
        // will be emitted.
        let fragment = Span::new(span.start, span.start + 3);
        out.adopt_text(state, sp, fragment, &sp.source[fragment.into_range()])?;
        for token in name {
            out.adopt_text(state, sp, token.span, &sp.source[token.span.into_range()])?;
        }
        if let Some(default) = default {
            let first = default
                .first()
                .map_or(span.end - 3, |first| first.span.start);
            let fragment = Span::new(first - 1, first);
            out.adopt_text(state, sp, fragment, &sp.source[fragment.into_range()])?;
            for token in default {
                out.adopt_text(state, sp, token.span, &sp.source[token.span.into_range()])?;
            }
        }
        let fragment = Span::new(span.end - 3, span.end);
        out.adopt_text(state, sp, fragment, &sp.source[fragment.into_range()])?;
    }

    Ok(())
}

/// Renders a template.
///
/// Whilst the documentation at
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Manual:Magic_words#How_magic_words_work>
/// makes claims about the order of operations of magic words, they are
/// actually processed thus:
///
/// 1. Is it a subst, and the parser is configured to retain the output
///    as literal text? If so, emit as text and do no more work.
/// 2. If no args, try to match as a variable, and expand the variable
///    if so.
/// 3. Change configuration settings based on special symbols 'msgnw',
///    'msg', and 'raw'.
/// 4. Is there a ':' in the name? If so, assume it is a parser function,
///    try to call the parser function, and allow processing to continue
///    if the parser function does not match.
/// 5. Check if it is a template, and if so, process the template, with
///    stack recursion limits.
/// 6. Query a database.
/// 7. Emit as text.
pub(super) fn render_template<'tt>(
    out: &mut String,
    state: &mut State<'_>,
    sp: &'tt StackFrame<'_>,
    bounds: Span,
    target: &'tt [Spanned<Token>],
    arguments: &'tt [Spanned<Argument>],
    line_start: bool,
) -> Result<bool> {
    // eprintln!("render_template {sp:?} {:?}", inspect(&sp.source, target));

    if state.load_mode == LoadMode::Base {
        render_fallback(out, state, sp)?;
        return Ok(true);
    }

    // TODO: There is some undocumented stuff to remove 'msgnw', 'msg', or 'raw'
    // magic words from the callee and then to change some options before
    // continuing.

    // At least 'Template:Color' builds HTML elements in pieces in a way
    // where it is impossible to parse them correctly before template
    // expansion is completed, which is very annoying because it requires
    // this buffering and double-parsing where it would not otherwise be
    // necessary.
    let mut partial = String::new();

    let mut first = None;
    let wrapper_key = match split_target(state, sp, &mut first, target, arguments)? {
        Target::ParserFn { callee, arguments } => {
            call_parser_fn(&mut partial, state, sp, Some(bounds), &callee, &arguments)?;
            None
        }

        Target::Template { arguments, callee } => {
            call_template(&mut partial, state, sp, callee.clone(), &arguments)?
        }

        Target::Text => {
            return Ok(false);
        }
    };

    // “T2529: if the template begins with a table or block-level
    //  element, it should be treated as beginning a new line.
    //  This behavior is somewhat controversial.”
    let needs_newline =
        !line_start && (partial.starts_with("{|") || partial.starts_with([':', ';', '#', '*']));

    if let Some(key) = wrapper_key {
        // It is necessary to inject strip markers rather than extension tags
        // or else the start-of-line rules break
        state
            .strip_markers
            .push(out, "wiki-rs", StripMarker::WikiRsSourceStart(key.clone()));
        if needs_newline {
            writeln!(out)?;
        }
        write!(out, "{partial}")?;
        state
            .strip_markers
            .push(out, "wiki-rs", StripMarker::WikiRsSourceEnd(key));
    } else {
        if needs_newline {
            writeln!(out)?;
        }
        write!(out, "{partial}")?;
    }

    Ok(true)
}

/// Template target information.
enum Target<'tt> {
    /// The target is a parser function (or a variable, which is implemented in
    /// wiki.rs using parser functions).
    ParserFn {
        /// The parser function to invoke.
        callee: String,
        /// The arguments to the function.
        arguments: Vec<Kv<'tt>>,
    },
    /// The target is a template.
    Template {
        /// The arguments to the template.
        arguments: Vec<Kv<'tt>>,
        /// The template to expand.
        callee: Title,
    },
    /// The target looked like a template, but after further consideration, this
    /// is actually just plain text.
    ///
    /// The Wikitext grammar parser cannot disambiguate on its own whether a
    /// template call containing an extension tag is valid or not because
    /// extension tags are valid in the first argument to a parser function, but
    /// it is impossible to know whether this is a parser function call until
    /// any inner templates are expanded, since it is legal in most towns for
    /// `{{{{Trenchcoat}}<nowiki/>}}` to become `{{#threechildren:<nowiki/>}}`
    /// once the inner template is expanded.
    ///
    /// 'Template:Date and time templates' contains a
    /// `{{<nowiki/>{{template call which returns wikilink}}<nowiki/>}}` which
    /// triggers this path.
    Text,
}

/// Splits a template target in the form `callee:arg` into parts without
/// evaluating the `arg` part.
fn split_target<'tt>(
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    first: &'tt mut Option<Spanned<Token>>,
    target: &'tt [Spanned<Token>],
    arguments: &'tt [Spanned<Argument>],
) -> Result<Target<'tt>, Error> {
    let mut callee = String::new();
    let mut rest = target.iter();
    let mut has_colon = false;
    for part in rest.by_ref() {
        // It is not good enough to just look for text nodes because there are
        // insane but legal constructions like `{{ {{#if:1|#if:}} 1|y|n }}`
        // (evaluates to "y").
        let text = if let Spanned {
            span,
            node: Token::Text,
        } = part
        {
            Cow::Borrowed(&sp.source[span.into_range()])
        } else if let Spanned {
            node: Token::Generated(text),
            ..
        } = part
        {
            Cow::Borrowed(text.as_str())
        } else {
            sp.eval(state, core::slice::from_ref(part))?
        };

        if let Some((lhs, mut rhs)) = text.split_once(':') {
            callee += lhs;

            // Normally, template expressions are saved in the Wikitext and
            // expanded at the time a page is rendered; `subst` and `safesubst`
            // both cause the expression to be expanded at save time instead.
            //
            // `subst` does not expand recursively, so if a template *evaluates*
            // to `{{subst:foo}}` (i.e. contains a horror like
            // `{{{{{|subst:}}}foo}}` which prevents it from being expanded at
            // the template’s own save time), the expanded text in the caller
            // will be `{{subst:foo}}`.
            //
            // `safesubst` *does* expand recursively, so if a template evaluates
            // to `{{safesubst:foo}}` (again by e.g.
            // `{{{{{|safesubst:}}}foo}}`), the text in the caller will be the
            // same as if it were written as `{{foo}}`.
            //
            // Parent          | Child                      | Output
            // ----------------+----------------------------+-------------------
            //   At render time:
            // -----------------------------------------------------------------
            // `{{foo}}`       | `{{bar}}`                  | content of bar
            // `{{foo}}`       | `{{{{{|subst:}}}bar}}`     | `{{subst:bar}}`
            // `{{foo}}`       | `{{{{{|safesubst:}}}bar}}` | content of bar
            // ----------------+----------------------------+-------------------
            //   At save time:
            // ----------------+----------------------------+-------------------
            // `{{subst:foo}}` | `{{bar}}`                  | `{{bar}}`
            // `{{subst:foo}}` | `{{{{{|subst:}}}bar}}`     | content of bar
            // `{{subst:foo}}` | `{{{{{|safesubst:}}}bar}}` | content of bar
            let trimmed = callee.trim_ascii();
            if trimmed.eq_ignore_ascii_case("subst") {
                // Since wiki.rs is never in save mode, subst will always just
                // emit the original text
                return Ok(Target::Text);
            } else if trimmed.eq_ignore_ascii_case("safesubst") {
                callee.clear();

                if let Some((lhs, rest)) = rhs.split_once(':') {
                    // `safesubst:foo:...`
                    callee += lhs;
                    rhs = rest;
                } else {
                    // `safesubst:...`
                    callee += rhs;
                    continue;
                }
            }

            has_colon = true;
            if !rhs.is_empty() {
                *first = Some(Spanned {
                    node: Token::Generated(rhs.to_string()),
                    span: part.span,
                });
            }
            break;
        }

        callee += &text;
    }
    let rest = rest.as_slice();

    let callee_lower = callee.trim_ascii().to_lowercase();

    // eprintln!("{callee_lower} / {first:?} / {rest:?}");

    Ok(
        if is_function_call(arguments.is_empty(), has_colon, &callee_lower) {
            // It is important to actually not pass a zeroth argument if there
            // is not one because this changes behaviour (e.g. `{{VAR}}` gets
            // `VAR`; `{{VAR:}}` calls `VAR` with an empty string)
            let first = has_colon
                .then(|| Kv::Partial(first.as_ref().into_iter().chain(rest.iter()).collect()));

            let arguments = first
                .into_iter()
                .chain(arguments.iter().map(Kv::Argument))
                .collect::<Vec<_>>();

            Target::ParserFn {
                callee: callee_lower,
                arguments,
            }
        } else {
            #[rustfmt::skip]
            if let Some(Spanned { node: Token::Generated(first), .. }) = first {
                callee.push(':');
                callee += first;
            };
            callee += &sp.eval(state, rest)?;
            let callee = callee.trim_ascii();
            if Title::is_valid(callee) {
                let callee = Title::new(callee, Namespace::find_by_id(Namespace::TEMPLATE));
                let arguments = arguments.iter().map(Kv::Argument).collect::<Vec<_>>();
                Target::Template { callee, arguments }
            } else {
                Target::Text
            }
        },
    )
}

/// Transcludes a template.
pub(crate) fn call_template(
    out: &mut String,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    callee: Title,
    arguments: &[Kv<'_>],
) -> Result<Option<String>> {
    let Ok(template) = state.statics.db.get(&callee) else {
        log::warn!("No template found for '{callee}'");
        write!(out, "[[{}]]", callee.key())?;
        return Ok(None);
    };

    let Ok(template) = resolve_redirects(&state.statics.db, template) else {
        log::warn!("Template redirects failed for {callee}");
        return Ok(None);
    };

    let resolved_title = Title::new(&template.title, None);
    let resolved_key = resolved_title.key();
    let wrapper_key = TACKY_TEMPLATES.contains(resolved_key).then(|| {
        resolved_key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
    });

    log::trace!("Expanding {resolved_title}");

    let now = Instant::now();
    let mut expansion = ExpandTemplates::new(ExpandMode::Include);

    // TODO: What is supposed to happen if there was a redirect? Is the callee
    // name supposed to change?
    let sp = sp.chain(callee, FileMap::new(&template.body), arguments)?;
    // For now, just assume that the cache will always be big enough and unwrap
    let root = Arc::clone(
        state
            .statics
            .template_cache
            .write()?
            .get_or_insert_fallible(template.id, || {
                state.statics.parser.parse(&sp.source, true).map(Arc::new)
            })?
            .unwrap(),
    );

    expansion.adopt_output(state, &sp, &root)?;
    // TODO: Could just write directly to the out.
    write!(out, "{}", expansion.finish())?;

    state
        .timing
        .entry(sp.name.key().to_string())
        .and_modify(|(count, duration)| {
            *count += 1;
            *duration += now.elapsed();
        })
        .or_insert_with(|| (1, now.elapsed()));

    Ok(wrapper_key)
}

/// Returns true if the given template target is a variable or parser function.
fn is_function_call(empty_arguments: bool, has_colon: bool, callee_lower: &str) -> bool {
    // We can just assume that if it starts with a '#' then it is a parser
    // function since the way MediaWiki URLs work mean these cannot be
    // templates, and the list of function hooks from the MediaWiki API does
    // not actually include the hash.
    (empty_arguments && CONFIG.variables.contains(callee_lower))
        || callee_lower.starts_with('#')
        || (has_colon && CONFIG.function_hooks.contains(callee_lower))
        || callee_lower == "subst"
        || callee_lower == "safesubst"
}

/// Handles a template or parameter which is disabled due to the current
/// [`LoadMode`].
pub(super) fn render_fallback<W: fmt::Write + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
) -> Result {
    let href = make_url(
        None,
        &state.statics.base_uri,
        sp.root().name.full_text(),
        Some("mode=module"),
        false,
    )?;

    write!(
        out,
        r#"[{href} <span class="wiki-rs-incomplete">Run scripts</span>]"#
    )?;
    Ok(())
}

/// Attempts to prefetch templates that are immediately resolvable to
/// allow for parallel decoding of database entries whilst the renderer is busy.
pub(crate) struct DbPrefetch;

impl Surrogate<Error> for DbPrefetch {
    fn adopt_autolink(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _target: &[Spanned<Token>],
        _content: &[Spanned<Token>],
    ) -> Result<(), Error> {
        Ok(())
    }

    fn adopt_external_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result<(), Error> {
        self.adopt_tokens(state, sp, target)?;
        self.adopt_tokens(state, sp, content)
    }

    fn adopt_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        _trail: Option<Spanned<&str>>,
    ) -> Result<(), Error> {
        self.adopt_tokens(state, sp, target)?;

        // Prefetching targets from the index is for redlinks.
        // TODO: Allow redlinks to be configurable, to make things faster, with
        // the downside that you might click on a dead link.
        if let [
            Spanned {
                node: Token::Text,
                span,
            },
        ] = target
        {
            let target = title_decode(&sp.source[span.into_range()]);
            state
                .statics
                .db
                .prefetch(Title::new(&target, None), PrefetchPriority::Low);
        }

        for argument in content {
            self.adopt_tokens(state, sp, &argument.content)?;
        }
        Ok(())
    }

    fn adopt_parameter(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        name: &[Spanned<Token>],
        default: Option<&[Spanned<Token>]>,
    ) -> Result<(), Error> {
        self.adopt_tokens(state, sp, name)?;
        if let Some(default) = default {
            self.adopt_tokens(state, sp, default)?;
        }
        Ok(())
    }

    fn adopt_redirect(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _target: &[Spanned<Token>],
        _content: &[Spanned<Argument>],
        _trail: Option<Spanned<&str>>,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn adopt_start_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
        attributes: &[Spanned<Argument>],
        _self_closing: bool,
    ) -> Result<(), Error> {
        for attribute in attributes {
            self.adopt_tokens(state, sp, &attribute.content)?;
        }
        Ok(())
    }

    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result<(), Error> {
        for attribute in attributes {
            self.adopt_tokens(state, sp, &attribute.content)?;
        }
        Ok(())
    }

    fn adopt_table_data(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result<(), Error> {
        for attribute in attributes {
            self.adopt_tokens(state, sp, &attribute.content)?;
        }
        Ok(())
    }

    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result<(), Error> {
        for attribute in attributes {
            self.adopt_tokens(state, sp, &attribute.content)?;
        }
        Ok(())
    }
    fn adopt_table_row(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result<(), Error> {
        for attribute in attributes {
            self.adopt_tokens(state, sp, &attribute.content)?;
        }
        Ok(())
    }
    fn adopt_table_start(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result<(), Error> {
        for attribute in attributes {
            self.adopt_tokens(state, sp, &attribute.content)?;
        }
        Ok(())
    }

    fn adopt_template(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
        arguments: &[Spanned<Argument>],
    ) -> Result<(), Error> {
        let mut first = None;
        match split_target(state, sp, &mut first, target, arguments)? {
            Target::Template { callee, .. } => {
                state.statics.db.prefetch(callee, PrefetchPriority::High);
            }
            Target::ParserFn { callee, arguments } => {
                if callee == "safesubst" {
                    let target = arguments[0].eval(state, sp)?;
                    let target = target.trim_ascii();
                    let (callee, rest) = target
                        .split_once(':')
                        .map_or((target, None), |(callee, rest)| (callee, Some(rest)));
                    let callee_lower = callee.to_lowercase();
                    if !is_function_call(arguments.len() == 1, rest.is_some(), &callee_lower) {
                        let title = Title::new(target, Namespace::find_by_id(Namespace::TEMPLATE));
                        state.statics.db.prefetch(title, PrefetchPriority::High);
                    }
                } else if callee == "#invoke" || callee == "invoke" {
                    let target = arguments[0].eval(state, sp)?;
                    let target = target.trim_ascii();
                    let title = Title::new(target, Namespace::find_by_id(Namespace::MODULE));
                    state.statics.db.prefetch(title, PrefetchPriority::High);
                }
            }
            Target::Text => {
                self.adopt_tokens(state, sp, target)?;
            }
        }
        if let Some(first) = first {
            self.adopt_token(state, sp, &first)?;
        }
        for argument in arguments {
            self.adopt_tokens(state, sp, &argument.content)?;
        }

        Ok(())
    }
}

/// “Strip newlines from the left hand context of Category links.
///  See T2087, T87753, T174639, T359886”
/// Since it is necessary to expand the target to know whether it is a
/// category link there is no good way to suppress the newline in the
/// Wikitext parser itself, and trying to do it in the renderer is a
/// fool’s errand with how the graf emitter works (it would require
/// buffering at least two tokens, or doing some crazy nonsense to try to
/// get the graf emitter to be able to undo part of what it just did).
/// Of course, it is a fool’s errand here too because empty strip markers
/// need to be retained for the sake of trimming parser functions but
/// need to be ignored when left-trimming categories.
pub(super) fn left_trim_category(
    out: &mut String,
    state: &mut State<'_>,
    title: &Title,
    at: usize,
) {
    if title.namespace().id == Namespace::CATEGORY {
        let mut truncated = &out[..at];

        // If the strip marker contains whitespace including a newline, and that
        // newline is the last one in the final concatenation, then we will not
        // truncate at the ‘correct’ place. Let’s at least be aware of it, but
        // also mostly pretend like it will never happen.
        #[cfg(debug_assertions)]
        let mut uh_oh_newline = None;

        // Scan backwards, looking for any sequence of ASCII whitespace mixed
        // with empty strip markers, until we stop retreating and learn to stick
        // up for ourselves
        'outer: loop {
            static PREFIX: LazyLock<FinderRev<'static>> =
                LazyLock::new(|| FinderRev::new(MARKER_PREFIX));

            while let Some(input) = truncated.strip_suffix(MARKER_SUFFIX)
                && let Some(start) = PREFIX.rfind(input)
            {
                let key = &input[start + MARKER_PREFIX.len()..];
                let marker = state.strip_markers.get(key).expect("trim key corruption");

                let is_empty = match marker {
                    StripMarker::NoWiki(s) => s.bytes().all(|c| {
                        #[cfg(debug_assertions)]
                        if c == b'\n' {
                            uh_oh_newline.replace(start);
                        }
                        c.is_ascii_whitespace()
                    }),
                    StripMarker::WikiRsSourceStart(_) | StripMarker::WikiRsSourceEnd(_) => true,
                    _ => false,
                };

                if is_empty {
                    truncated = &truncated[..start];
                } else {
                    break 'outer;
                }
            }

            let trimmed = truncated.trim_ascii_end();
            if trimmed.len() == truncated.len() {
                break;
            }
            truncated = trimmed;
        }

        // The regular expression used by MW was "\n\s*", so after
        // retreating before all of the whitespace, we must find some
        // courage and advance forward to the nearest newline
        let end = truncated.len();
        let end = memchr::memchr(b'\n', &out.as_bytes()[end..at]).map(|e| e + end);

        #[cfg(debug_assertions)]
        if let Some(pos) = uh_oh_newline
            && pos < end.unwrap_or(at)
        {
            panic!("uh-oh! the earliest newline was inside a strip marker");
        }

        if let Some(end) = end
            && let Cow::Owned(replacement) =
                StripMarkers::for_each_non_marker(&out[end..at], |_| Some("".into()))
        {
            out.replace_range(end..at, &replacement);
        }
    }
}
