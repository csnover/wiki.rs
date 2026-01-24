//! Plain HTML rendering functions.

use super::{Error, Result, StackFrame, State, WriteSurrogate, image};
use crate::{
    common::{anchor_encode, decode_html, title_decode},
    config::CONFIG,
    title::{Namespace, Title},
    wikitext::{Argument, FileMap, Span, Spanned, Token, builder::token},
};
use axum::http::Uri;
use std::borrow::Cow;

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
    let target = sp.eval(state, target)?;
    let target = title_decode(&target);

    let title = Title::new(&target, None);
    match title.namespace().id {
        Namespace::CATEGORY if !target.starts_with(':') => {
            state.globals.categories.insert(title.key().to_string());
            if let Some(trail) = trail {
                out.adopt_generated(state, sp, None, trail)?;
            }
        }
        Namespace::FILE if !target.starts_with(':') => {
            image::render_media(out, state, sp, title, content)?;
            if let Some(trail) = trail {
                out.adopt_generated(state, sp, None, trail)?;
            }
        }
        _ => {
            render_internal_link(out, state, sp, &target, content, trail, title)?;
        }
    }
    Ok(())
}

/// Renders an internal link.
fn render_internal_link<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    target: &str,
    content: &[Spanned<Argument>],
    trail: Option<&str>,
    title: Title,
) -> Result<(), Error> {
    if title.fragment().is_empty() && sp.root().name == title {
        render_runtime(out, state, sp, |_, source| {
            token!(
                source,
                Token::StartTag {
                    name: token!(source, Span { "a" }),
                    attributes: token![source, [ "class" => "mw-selflink selflink" ]].into(),
                    self_closing: false
                }
            )
        })?;
    } else {
        render_start_link(out, state, sp, &LinkKind::Internal(title))?;
    }

    if content.is_empty() {
        out.adopt_generated(
            state,
            sp,
            None,
            &decode_html(target.trim_start_matches(':')),
        )?;
    } else {
        render_single_attribute(out, state, sp, content)?;
    }
    if let Some(trail) = trail {
        out.adopt_generated(state, sp, None, trail)?;
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
    let query = if let LinkKind::Internal(title) = link
        && title.interwiki().is_none()
        && !state.statics.db.contains(title)
    {
        Some("mode=edit&redlink=1")
    } else {
        None
    };
    let href = link.to_string(&state.statics.base_uri, query);

    render_runtime(out, state, sp, |_, source| {
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
    render_runtime(out, state, sp, |_, source| {
        token!(
            source,
            Token::EndTag {
                name: token!(source, Span { "a" }),
            }
        )
    })
}

/// A kind of link to render.
#[derive(Clone, Debug)]
pub(super) enum LinkKind<'a> {
    /// An external link.
    External(Cow<'a, str>),
    /// An internal link.
    Internal(Title),
}

impl LinkKind<'_> {
    /// Converts the link to a URI-encoded string suitable for use in an HTML
    /// `href` attribute.
    pub fn to_string(&self, base_uri: &Uri, query: Option<&str>) -> String {
        match self {
            LinkKind::External(url) => {
                // TODO: Hack together some URL parsing good enough that there is an
                // actual way to check that the origin is the same
                if url.starts_with('/') {
                    url.to_string()
                } else {
                    format!("{}/external/{url}", base_uri.path())
                }
            }
            LinkKind::Internal(title) => {
                if let Some(iw) = title
                    .interwiki()
                    .and_then(|iw| CONFIG.interwiki_map.get(&iw.to_ascii_lowercase()))
                {
                    format!(
                        "{}/external/{}",
                        base_uri.path(),
                        iw.replace("$1", &title.partial_url().to_string())
                    )
                } else if title.text().is_empty() {
                    format!("#{}", anchor_encode(title.fragment()))
                } else {
                    let mut link = format!("{}/article/{}", base_uri.path(), title.partial_url());
                    if let Some(query) = query {
                        link.push('?');
                        link += query;
                    }
                    if !title.fragment().is_empty() {
                        link.push('#');
                        link += &anchor_encode(title.fragment());
                    }
                    link
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

/// Renders a runtime-generated token.
pub(super) fn render_runtime<
    W: WriteSurrogate + ?Sized,
    F: FnOnce(&mut State<'_>, &mut String) -> Spanned<Token>,
>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    f: F,
) -> Result {
    let source = &mut String::new();
    let token = f(state, source);
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

/// Phrasing content, per the HTML5 specification, including obsolete elements
/// allowed by MediaWiki.
pub(super) static PHRASING_TAGS: phf::Set<&str> = phf::phf_set! {
    "a", "abbr", "area", "audio", "b", "bdi", "bdo", "big", "br", "button",
    "canvas", "cite", "code", "data", "datalist", "del", "dfn", "em", "embed",
    "font", "i", "iframe", "img", "input", "ins", "kbd", "label", "link", "map",
    "mark", "math", "meta", "meter", "noscript", "object", "output", "picture",
    "progress", "q", "rb", "rp", "rt", "rtc", "ruby", "s", "samp", "script",
    "selectedcontent", "slot", "small", "span", "strike", "strong", "sub",
    "sup", "svg", "template", "textarea", "time", "u", "var", "video", "wbr"
};
