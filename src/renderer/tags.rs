//! Plain HTML rendering functions.

use super::{Error, Result, StackFrame, State, WriteSurrogate, image, trim::TrimLink};
use crate::{
    common::anchor_encode,
    renderer::Surrogate,
    title::{Namespace, Title},
    wikitext::{Argument, FileMap, Span, Spanned, Token, builder::token},
};
use axum::http::Uri;
use either::Either;
use std::borrow::Cow;

/// Renders an HTML attribute.
pub(super) fn render_attribute<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    name: Option<Either<&str, &[Spanned<Token>]>>,
    value: Either<&str, &[Spanned<Token>]>,
) -> Result {
    // At least 'Template:Skip to top and bottom' contains invalid HTML
    // where an attribute is missing a close quote, and this is error
    // corrected differently in HTML5 versus the MW parser, so it is
    // necessary to handle the key and value parts separately and always
    // make sure the value is quoted or most of the page content ends up
    // in the attribute.
    if let Some(name) = name {
        render_either(out, state, sp, name)?;
        if !value.either(str::is_empty, <[_]>::is_empty) {
            out.write_str("=\"")?;
            render_either(out, state, sp, value)?;
            out.write_str("\"")?;
        }
    } else {
        render_either(out, state, sp, value)?;
    }
    Ok(())
}

/// Renders a possibly generated attribute subpart.
fn render_either<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    value: Either<&str, &[Spanned<Token>]>,
) -> Result {
    match value {
        Either::Left(s) => out.adopt_generated(state, sp, None, s),
        Either::Right(t) => out.adopt_tokens(state, sp, t),
    }
}

/// Renders an external web site link.
pub(super) fn render_external_link<W: WriteSurrogate>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    target: &[Spanned<Token>],
    content: &[Spanned<Token>],
) -> Result {
    // TODO: Handle “external” links that just come back to the wiki. Right now
    // it is annoying to try to do this because `http::Uri` does not conform to
    // RFC 3986 so it mixes up authority and path when the scheme is missing,
    // but adding a whole new dependency just for this one case is too much.
    let link = LinkKind::External(sp.eval(state, target)?);
    render_start_link(out, state, sp, &link)?;
    if content.is_empty() {
        let ordinal = &mut state.globals.external_link_ordinal;
        *ordinal += 1;
        write!(out, "[{ordinal}]")?;
    } else {
        out.adopt_tokens(state, sp, content)?;
    }
    render_end_link(out, state, sp)
}

/// Renders a wikilink.
pub(super) fn render_wikilink<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    target: &[Spanned<Token>],
    content: &[Spanned<Argument>],
    trail: Option<&str>,
) -> Result<(), Error> {
    let target_text = sp.eval(state, target)?;
    let title = Title::new(&target_text, None);
    match title.namespace().id {
        Namespace::FILE => {
            image::render_image(out, state, sp, title, content)?;
            if let Some(trail) = trail {
                write!(out, "{trail}")?;
            }
        }
        Namespace::CATEGORY => {
            state.globals.categories.insert(target_text.to_string());
            if let Some(trail) = trail {
                write!(out, "{trail}")?;
            }
        }
        _ => {
            render_internal_link(out, state, sp, target, content, trail, title)?;
        }
    }
    Ok(())
}

/// Renders an internal link.
fn render_internal_link<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    target: &[Spanned<Token>],
    content: &[Spanned<Argument>],
    trail: Option<&str>,
    title: Title,
) -> Result<(), Error> {
    render_start_link(out, state, sp, &LinkKind::Internal(title))?;
    if content.is_empty() {
        TrimLink::new(out).adopt_tokens(state, sp, target)?;
    } else {
        render_single_attribute(out, state, sp, content)?;
    }
    if let Some(trail) = trail {
        out.write_str(trail)?;
    }
    render_end_link(out, state, sp)
}

/// Renders an anchor for a link.
pub(super) fn render_start_link<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    link: &LinkKind<'_>,
) -> Result {
    let href = link.to_string(&state.statics.base_uri);

    render_runtime(out, state, sp, |source| {
        token!(
            source,
            Token::StartTag {
                name: token!(source, Span { "a" }),
                attributes: token![source, [ "href" => &href ]].into(),
                self_closing: false
            }
        )
    })
}

/// Renders an `</a>` tag. This is only suitable for use with a `Document`.
pub(super) fn render_end_link<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
) -> Result {
    render_runtime(out, state, sp, |source| {
        token!(
            source,
            Token::EndTag {
                name: token!(source, Span { "a" }),
            }
        )
    })
}

/// A kind of link to render.
#[derive(Debug)]
pub(super) enum LinkKind<'a> {
    /// An external link.
    External(Cow<'a, str>),
    /// An internal link.
    Internal(Title),
}

impl LinkKind<'_> {
    /// Converts the link to a URI-encoded string suitable for use in an HTML
    /// `href` attribute.
    pub fn to_string(&self, base_uri: &Uri) -> String {
        match self {
            LinkKind::External(url) => {
                // TODO: Hack together some URL parsing good enough that there is an
                // actual way to check that the origin is the same
                if url.starts_with('/') {
                    html_escape::encode_double_quoted_attribute(url).to_string()
                } else {
                    format!(
                        "{}/external/{}",
                        base_uri.path(),
                        html_escape::encode_double_quoted_attribute(url)
                    )
                }
            }
            LinkKind::Internal(title) => {
                if title.text().is_empty() {
                    format!("#{}", anchor_encode(title.fragment()))
                } else {
                    format!("{}/article/{}", base_uri.path(), title.partial_url())
                }
            }
        }
    }
}

/// Serialises values which are structured like
/// `{argument}{delimiter}{argument}...`.
pub(super) fn render_single_attribute<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    attributes: &[Spanned<Argument>],
) -> Result {
    if let Some(first) = attributes.first() {
        out.adopt_tokens(state, sp, &first.content)?;
    }
    for attrs in attributes.windows(2) {
        let (prev, curr) = (&attrs[0], &attrs[1]);
        let span = Span::new(prev.span.end, curr.span.start);
        out.adopt_text(state, sp, span, &sp.source[span.into_range()])?;
        out.adopt_tokens(state, sp, &curr.content)?;
    }
    Ok(())
}

/// Renders an HTML start tag node.
pub(super) fn render_start_tag<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    name: &str,
    attributes: &[Spanned<Argument>],
    self_closing: bool,
) -> Result {
    write!(out, "<{name}")?;
    for attr in attributes {
        out.write_char(' ')?;
        out.adopt_attribute(
            state,
            sp,
            attr.name().map(Either::Right),
            Either::Right(attr.value()),
        )?;
    }
    if self_closing {
        out.write_char('/')?;
    }
    out.write_char('>')?;
    Ok(())
}

/// Renders a runtime-generated token.
pub(super) fn render_runtime<
    W: WriteSurrogate + ?Sized,
    F: FnOnce(&mut String) -> Spanned<Token>,
>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    f: F,
) -> Result {
    let source = &mut String::new();
    let token = f(source);
    out.adopt_token(state, &sp.clone_with_source(FileMap::new(source)), &token)
}

/// Renders runtime-generated tokens.
pub(super) fn render_runtime_list<
    W: WriteSurrogate + ?Sized,
    F: FnOnce(&mut State<'_>, &mut String) -> Vec<Spanned<Token>>,
>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    f: F,
) -> Result {
    let source = &mut String::new();
    let tokens = f(state, source);
    out.adopt_tokens(state, &sp.clone_with_source(FileMap::new(source)), &tokens)
}
