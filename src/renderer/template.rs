//! Template rendering types and functions.

use super::{
    Error, Result, State, StripMarker,
    expand_templates::{ExpandMode, ExpandTemplates},
    parser_fns::call_parser_fn,
    resolve_redirects,
    stack::{KeyCacheKvs, Kv, StackFrame},
    surrogate::Surrogate,
    tags,
};
use crate::{
    LoadMode,
    common::make_url,
    config::CONFIG,
    lua::run_vm,
    title::{Namespace, Title},
    wikitext::{Argument, FileMap, Span, Spanned, Token},
};
use core::fmt::{self, Write as _};
use std::{borrow::Cow, pin::pin, rc::Rc, time::Instant};

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

    let code = match state.statics.db.get(callee.key()) {
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
        //
        // If you are here because you saw some attempt (or many, many
        // attempts) to load a template named like `{{{1}}}`, this could be
        // *intentional*! Pour a drink and go read
        // [`crate::db::CacheableArticle`].
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
) -> Result {
    // eprintln!("render_template {sp:?} {:?}", inspect(&sp.source, target));

    if state.load_mode == LoadMode::Base {
        return render_fallback(out, state, sp);
    }

    let mut first = None;
    let target = split_target(state, sp, &mut first, target, arguments)?;

    // TODO: There is some undocumented stuff to remove 'msgnw', 'msg', or 'raw'
    // magic words from the callee and then to change some options before
    // continuing.

    // At least 'Template:Color' builds HTML elements in pieces in a way
    // where it is impossible to parse them correctly before template
    // expansion is completed, which is very annoying because it requires
    // this buffering and double-parsing where it would not otherwise be
    // necessary.
    let mut partial = String::new();

    let wrapper_key = match target {
        Target::ParserFn { callee, arguments } => {
            call_parser_fn(&mut partial, state, sp, Some(bounds), &callee, &arguments)?;
            None
        }

        Target::Template {
            callee,
            target,
            arguments,
        } => {
            call_template(&mut partial, state, sp, &target, callee.clone(), &arguments)?;

            // TODO: 'Template:Infobox' breaks when it is fed recursively into
            // itself because it gets confused by the extra unexpected strip
            // markers in its `fixChildBoxes` function and emits invalid table
            // markup. Probably other things do this too but infobox is
            // especially brain damaged. This is noticeable on
            // 'Template:Nutritionalvalue', among other pages.
            contains_blocks(&partial).then(|| {
                callee
                    .key()
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect::<String>()
            })
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

    Ok(())
}

/// Returns whether a partial template string appears to contain some subset of
/// block-level elements.
///
/// It is necessary to not just inject source markers into the output
/// indiscriminately for every template because some template expansions are
/// plain text interpolations which will break because other templates and
/// modules don’t expect to see tags or strip markers in those positions. For
/// example, 'Template:Speciesbox' interpolates a template expansion into the
/// target for another template expansion, and having extra strip markers in
/// there will just break it.
///
/// It is not possible without doing a bunch of extra work to check for every
/// possible kind of element that would ideally be taged because characters
/// which are valid to start Wikitext elements are often also intended to be
/// used as part of some text expression where injecting anything into the
/// output would break the output (e.g. `*` starting a template which is
/// expanded as part of an arithmetic expression).
fn contains_blocks(partial: &str) -> bool {
    if partial.starts_with("{|") {
        return true;
    }

    let mut iter = partial.chars();
    while let Some(c) = iter.next() {
        if (c == '<' && starts_with_block_tag_name(iter.as_str()))
            || (c == '\n' && iter.as_str().starts_with("{|"))
        {
            return true;
        }
    }
    false
}

/// Returns whether the given string starts with an interesting tag name.
///
/// This is necessary because 'Module:Citation/CS1' is unbearable and complains
/// visibly if a strip marker exists inside of any of its parameters instead of
/// just ignoring or stripping them itself. (Is it any wonder it runs so
/// slowly?) Since it is only really necessary to attach extra styling markers
/// to a subset of block-level tags, avoid generating the template source
/// markers in this case.
///
/// Eventually it will turn out that source tracking has to occur totally out of
/// band using a code map, and then the problem will be solved forever, but why
/// do work when you can pretend like it is unnecessary, lol.
fn starts_with_block_tag_name(tag_name: &str) -> bool {
    let mut max = "blockquote".len() + 1;
    while !tag_name.is_char_boundary(max) {
        max -= 1;
    }
    let Some((tag_name, _)) =
        tag_name[..max].split_once(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
    else {
        return false;
    };

    !tags::PHRASING_TAGS.contains(&tag_name.to_ascii_lowercase())
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
        /// The template to expand.
        callee: Title,
        /// The raw template name.
        target: Kv<'tt>,
        /// The arguments to the template.
        arguments: Vec<Kv<'tt>>,
    },
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

        if let Some((lhs, rhs)) = text.split_once(':') {
            callee += lhs;
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

    let callee = callee.trim_ascii();
    let callee_lower = callee.to_lowercase();

    // eprintln!("{callee} / {first:?} / {rest:?}");

    Ok(
        if is_function_call(arguments.is_empty(), has_colon, &callee_lower) {
            // It is important to actually not pass a zeroth argument is there is
            // not one because this changes the behaviour of variable get/set
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
            let callee = Title::new(
                &sp.eval(state, target)?,
                Namespace::find_by_id(Namespace::TEMPLATE),
            );
            let target = Kv::Partial(target.iter().collect());
            let arguments = arguments.iter().map(Kv::Argument).collect::<Vec<_>>();
            Target::Template {
                callee,
                target,
                arguments,
            }
        },
    )
}

/// Transcludes a template.
pub(crate) fn call_template(
    out: &mut String,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    target: &Kv<'_>,
    callee: Title,
    arguments: &[Kv<'_>],
) -> Result {
    let Ok(template) = state.statics.db.get(callee.key()) else {
        log::warn!("No template found for '{callee}'");

        // 'Template:Date and time templates' contains a
        // `{{<nowiki/>{{template call which returns wikilink}}<nowiki/>}}`.
        // TODO: It is not totally clear which thing is intended to trigger this
        // fallback mode. It is valid to create a target by interpolating with a
        // template call, so is it the existence of a extension tag (in which
        // case the parser should not parse those inside a template target)? Is
        // it because the *expanded* target contains non-text tokens? It
        // *cannot* actually be due to the database lookup failing because
        // *that* is supposed to fall back to a wikilink to edit the template.
        // Therefore, this is not a totally correct fallback, and more work
        // needs to be done elsewhere.
        write!(out, "{{{{{}", target.eval(state, sp)?)?;
        for argument in arguments {
            write!(out, "|{}", argument.eval(state, sp)?)?;
        }
        out.write_str("}}")?;

        return Ok(());
    };

    let Ok(template) = resolve_redirects(&state.statics.db, template) else {
        log::warn!("Template redirects failed for {callee}");
        return Ok(());
    };

    let now = Instant::now();
    let mut expansion = ExpandTemplates::new(ExpandMode::Include);

    // TODO: What is supposed to happen if there was a redirect? Is the callee
    // name supposed to change?
    let sp = sp.chain(callee, FileMap::new(&template.body), arguments)?;
    // For now, just assume that the cache will always be big enough and unwrap
    let root = Rc::clone(
        state
            .statics
            .template_cache
            .get_or_insert_fallible(template.id, || {
                state.statics.parser.parse(&sp.source, true).map(Rc::new)
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
    Ok(())
}

/// Returns true if the given template target is a variable or parser function.
pub(super) fn is_function_call(empty_arguments: bool, has_colon: bool, callee_lower: &str) -> bool {
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
            Target::Template { callee, .. } => state.statics.db.prefetch(callee.key()),
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
                        state.statics.db.prefetch(title.key());
                    }
                } else if callee == "#invoke" || callee == "invoke" {
                    let target = arguments[0].eval(state, sp)?;
                    let target = target.trim_ascii();
                    let title = Title::new(target, Namespace::find_by_id(Namespace::MODULE));
                    state.statics.db.prefetch(title.key());
                }
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
