//! Template rendering types and functions.

use super::{
    Error, Result, State,
    expand_templates::ExpandMode,
    parser_fns::call_parser_fn,
    resolve_redirects,
    stack::{KeyCacheKvs, Kv, StackFrame},
    tags::{render_runtime, render_runtime_list},
};
use crate::{
    LoadMode,
    common::make_url,
    config::CONFIG,
    lua::run_vm,
    renderer::{WriteSurrogate, expand_templates::ExpandTemplates},
    title::{Namespace, Title},
    wikitext::{
        Argument, FileMap, MARKER_PREFIX, MARKER_SUFFIX, Span, Spanned, Token, builder::token,
    },
};
use regex::Regex;
use std::{borrow::Cow, pin::pin, rc::Rc, sync::LazyLock, time::Instant};

/// Calls a Lua function.
pub(super) fn call_module<W: WriteSurrogate + ?Sized>(
    out: &mut W,
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
            let sp = sp.clone_with_source(FileMap::new(&result));
            let tree = state.statics.parser.parse_no_expansion(&sp.source)?;
            // eprintln!("{result}\n\n{:#?}", crate::wikitext::inspect(&sp.source, &tree.root));
            out.adopt_output(state, &sp, &tree)?;
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
            render_runtime_list(out, state, &sp, |_, source| {
                token![source, [
                    Token::StartTag {
                        name: token!(source, Span { "span" }),
                        attributes: token![source, [ "class" => "error" ]].into(),
                        self_closing: false,
                    },
                    Token::Text { root_error.to_string() },
                    Token::EndTag { name: token!(source, Span { "span" }) },
                ]]
                .into()
            })?;
        }
    }

    Ok(())
}

/// Renders a parameter.
pub(super) fn render_parameter<W: WriteSurrogate + ?Sized>(
    out: &mut W,
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

    if !sp.expand_raw(out, state, key.trim_ascii())? {
        if let Some(default) = default {
            out.adopt_tokens(state, sp, default)?;
        } else {
            out.adopt_text(state, sp, span, &sp.source[span.into_range()])?;
        }
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
pub(super) fn render_template<'tt, W: WriteSurrogate>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &'tt StackFrame<'_>,
    bounds: Span,
    target: &'tt [Spanned<Token>],
    arguments: &'tt [Spanned<Argument>],
) -> Result {
    // eprintln!("render_template {sp:?} {:?}", inspect(&sp.source, target));

    if state.load_mode == LoadMode::Base {
        return render_fallback(out, state, sp);
    }

    let Target {
        has_colon,
        first,
        rest,
        callee,
    } = split_target(state, sp, target)?;
    // eprintln!("{callee} / {first:?} / {rest:?}");

    // Technically, MediaWiki has some magic words that are case-sensitive
    // and others which are case-insensitive. In practice, this does not
    // seem to matter, and can just treat everything as lowercase. MW uses
    // Unicode-aware functions for this, although everything seems to be ASCII.
    let callee_lower = callee.to_lowercase();

    let use_function_hook = is_function_call(arguments.is_empty(), has_colon, &callee_lower);

    // TODO: There is some undocumented stuff to remove 'msgnw', 'msg', or 'raw'
    // magic words from the callee and then to change some options before
    // continuing.

    // At least 'Template:Color' builds HTML elements in pieces in a way
    // where it is impossible to parse them correctly before template
    // expansion is completed, which is very annoying because it requires
    // this buffering and double-parsing where it would not otherwise be
    // necessary, and makes extension tags very hard to deal with because
    // they must be able to emit tags which are not serialisable in Wikitext
    // (`<math>`, etc.).
    let mut evaluator = ExpandTemplates::new(ExpandMode::Include);

    if use_function_hook {
        // It is important to actually not pass a zeroth argument is there is
        // not one because this changes the behaviour of variable get/set
        let first =
            has_colon.then(|| Kv::Partial(first.as_ref().into_iter().chain(rest.iter()).collect()));

        let arguments = first
            .into_iter()
            .chain(arguments.iter().map(Kv::Argument))
            .collect::<Vec<_>>();

        call_parser_fn(
            &mut evaluator,
            state,
            sp,
            Some(bounds),
            &callee_lower,
            &arguments,
        )?;
    } else {
        let arguments = arguments.iter().map(Kv::Argument).collect::<Vec<_>>();

        let callee = sp.eval(state, target)?;
        call_template(
            &mut evaluator,
            state,
            sp,
            &Kv::Partial(target.iter().collect()),
            callee.trim_ascii(),
            &arguments,
        )?;
    }

    let mut partial = evaluator.finish();

    // “T2529: if the template begins with a table or block-level
    //  element, it should be treated as beginning a new line.
    //  This behavior is somewhat controversial.”
    if partial.starts_with("{|") || partial.starts_with([':', ';', '#', '*']) {
        partial.insert(0, '\n');
    }

    // Many templates do not provide adequate (or any) hooks for overriding
    // styles. (See styles.css; everything using `[data-wiki-rs]` is a template
    // with missing or inadequate hooks of its own).
    // TODO: This is related to `tag_blocks` (grep Git history). Do this, except
    // more like how `tag_blocks` worked, no disgusting hacks.
    {
        // The hack in MW that is used to detect errors from random HTML-ish
        // strings expects to see `<{tag} class="error"`. So, the disgusting
        // hack *here* has to make sure to not cause *that* hack to break by
        // injecting the attribute in the most convenient place. Unfortunately,
        // only one of these hacks can ever be replaced by something less stupid
        // later.
        static DISGUSTING_HACK: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(&format!(
                r#"^(?:\s|{MARKER_PREFIX}\d+{MARKER_SUFFIX})*<([\w-]+)[^>]*?(?: data-wiki-rs="([^"]+)")?>"#
            ))
            .unwrap()
        });

        if !use_function_hook
            && let Some(hax) = DISGUSTING_HACK.captures(&partial)
            && let tag_name = hax.get(1).unwrap().as_str()
            && crate::wikitext::HTML5_TAGS.contains(&tag_name.to_ascii_lowercase())
        {
            // TODO: This should account for redirections.
            let class_name = Title::new(&callee, Namespace::find_by_id(Namespace::TEMPLATE))
                .key()
                .to_ascii_lowercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-");

            let (prefix, extra, suffix) = if let Some(existing) = hax.get(2) {
                (
                    existing.start(),
                    format!("{} {class_name}", existing.as_str()),
                    existing.end(),
                )
            } else {
                (
                    hax.get_match().end() - 1,
                    format!(r#" data-wiki-rs="{class_name}""#),
                    hax.get_match().end() - 1,
                )
            };

            partial = String::from(&partial[..prefix]) + &extra + &partial[suffix..];
        }
    }

    let root = state.statics.parser.parse_no_expansion(&partial)?;
    let sp = sp.clone_with_source(FileMap::new(&partial));
    // eprintln!("{partial}\n\n{:#?}", crate::wikitext::inspect(&sp.source, &root.root));
    out.adopt_output(state, &sp, &root)
}

/// Template target information.
struct Target<'tt> {
    /// The extracted callee.
    callee: String,
    /// The first argument of a parser function.
    first: Option<Spanned<Token>>,
    /// The rest of the tokens.
    rest: &'tt [Spanned<Token>],
    /// Whether the target split on a colon.
    has_colon: bool,
}

/// Splits a template target in the form `callee:arg` into parts without
/// evaluating the `arg` part.
fn split_target<'tt>(
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    target: &'tt [Spanned<Token>],
) -> Result<Target<'tt>, Error> {
    let mut callee = String::new();
    let mut rest = target.iter();
    let mut has_colon = false;
    let mut first = None;
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
                first = Some(Spanned {
                    node: Token::Generated(rhs.to_string()),
                    span: part.span,
                });
            }
            break;
        }

        callee += &text;
    }
    let rest = rest.as_slice();
    let callee = callee.trim_ascii().to_string();
    Ok(Target {
        callee,
        first,
        rest,
        has_colon,
    })
}

/// Transcludes a template.
pub fn call_template<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    target: &Kv<'_>,
    callee: &str,
    arguments: &[Kv<'_>],
) -> Result {
    // log::trace!("{} is wanting {callee}", sp.name);
    let callee = Title::new(callee, Namespace::find_by_id(Namespace::TEMPLATE));

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
        out.write_str("{{")?;
        target.eval_into(out, state, sp)?;
        for argument in arguments {
            out.write_str("|")?;
            argument.eval_into(out, state, sp)?;
        }
        out.write_str("}}")?;

        return Ok(());
    };

    let Ok(template) = resolve_redirects(&state.statics.db, template) else {
        log::warn!("Template redirects failed for {callee}");
        return Ok(());
    };

    let now = Instant::now();

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

    out.adopt_output(state, &sp, &root)?;

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
pub(super) fn render_fallback<W: WriteSurrogate + ?Sized>(
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

    // TODO: This actually needs to be an extension tag strip marker since
    // templates can be in any arbitrary position, including inside attributes,
    // so this results in an invalid tree.
    render_runtime(out, state, sp, |_, source| {
        token!(
            source,
            Token::ExternalLink {
                target: token!(source, [Token::Text { href }]).into(),
                content: token![source, [
                    Token::StartTag {
                        name: token!(source, Span { "span" }),
                        attributes: token![source, [ "class" => "wiki-rs-incomplete" ]].into(),
                        self_closing: false,
                    },
                    Token::Text { "Run scripts" },
                    Token::EndTag { name: token!(source, Span { "span" }) }
                ]]
                .into()
            }
        )
    })
}
