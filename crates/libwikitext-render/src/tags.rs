//! Plain HTML rendering functions.

use super::{Error, Paths, Result, StackFrame, State, Surrogate};
use http::Uri;
use libmisc::CowExt as _;
use libwikitext_common::{
    AnchorEncodeMode, anchor_encode,
    db::DatabaseProvider as _,
    decode_html, make_url,
    title::{Namespace, Title},
    url_encode_sanitized,
};
use libwikitext_parse::{Argument, FileMap, Span, Spanned, Token, builder::token};
use serde_json_borrow::Value;
use std::borrow::Cow;

/// Renders an external web site link.
pub(super) fn render_external_link<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    target: &[Spanned<Token>],
    content: &[Spanned<Token>],
    auto_link: bool,
) -> Result {
    // TODO: Handle “external” links that just come back to the wiki. Right now
    // it is annoying to try to do this because `http::Uri` does not conform to
    // RFC 3986 so it mixes up authority and path when the scheme is missing,
    // but adding a whole new dependency just for this one case is too much.
    let link = LinkKind::External(
        sp.eval(state, target)?,
        if auto_link {
            ExternalLinkKind::Free
        } else if content.is_empty() {
            ExternalLinkKind::Autonumber
        } else {
            ExternalLinkKind::Text
        },
    );
    render_start_link(out, state, sp, &link)?;
    if content.is_empty() {
        let ordinal = &mut state.globals.external_link_ordinal;
        *ordinal += 1;
        let text = format!("[{ordinal}]");
        out.adopt_generated(state, sp, None, &text)?;
    } else {
        out.adopt_tokens(state, sp, content)?;
    }
    render_end_link(out, state, sp)
}

/// Renders an internal link.
pub(super) fn render_internal_link<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    target: &str,
    content: &[Spanned<Argument>],
    trail: Option<&str>,
    title: Title,
) -> Result<(), Error> {
    if state.globals.title == title {
        if let Some(fragment) = title.fragment() {
            render_runtime(out, state, sp, |_, source| {
                token!(
                    source,
                    Token::StartTag {
                        name: token!(source, Span { "a" }),
                        attributes: token![source, [
                            "class" => "mw-selflink-fragment",
                            "href" => &format!("#{}", anchor_encode(fragment, AnchorEncodeMode::Html5)),
                        ]].into(),
                        self_closing: false
                    }
                )
            })?;
        } else {
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
        }
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
pub(super) fn render_start_link<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    link: &LinkKind<'_>,
) -> Result {
    let (missing, query) = if let LinkKind::Internal(title) = link
        && title.interwiki().is_none()
        && !title.key().is_empty()
        && !state.statics.db.contains(title)
    {
        (
            true,
            (title.namespace().id != Namespace::SPECIAL).then_some("action=edit&redlink=1"),
        )
    } else {
        (false, None)
    };

    let options = LinkKindOptions {
        base_uri: &state.statics.base_uri,
        interwiki_map: &state.statics.db.config().interwiki_map,
        paths: &state.statics.paths,
    };
    let href = link.to_string(&options, query);

    let href = if missing {
        href.split_once('#').map_or(href.as_str(), |(lhs, _)| lhs)
    } else {
        &href
    };

    render_runtime(out, state, sp, |_, source| {
        token!(
            source,
            Token::StartTag {
                name: token!(source, Span { "a" }),
                attributes: {
                    match link {
                        LinkKind::External(_, kind) => token![source, [
                            "rel" => "nofollow",
                            "class" => kind.css(),
                            "href" => &href
                        ]]
                        .into(),
                        LinkKind::Internal(title) => {
                            let mut args = token![source, ["href" => &href]].to_vec();
                            if missing {
                                args.push(token![source, Argument { "class" => "new" }]);
                                if let Some(message) =
                                    state.messages.get("red-link-title").and_then(Value::as_str)
                                {
                                    args.push(token![source, Argument {
                                        "title" => message.replace("$1", title.key())
                                    }]);
                                }
                            } else if !title.prefixed_text().is_empty() {
                                args.push(token![source, Argument {
                                    "title" => title.prefixed_text()
                                }]);
                            }
                            args
                        }
                    }
                },
                self_closing: false
            }
        )
    })
}

/// Renders an `</a>` tag. This is only suitable for use with a `Document`.
pub(super) fn render_end_link<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
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

/// Static option data for [`LinkKind::to_string`].
pub(crate) struct LinkKindOptions<'a> {
    /// The base URI for an internal link.
    pub base_uri: &'a Uri,
    /// The map of interwiki prefixes.
    pub interwiki_map: &'a phf::Map<&'static str, &'static str>,
    /// Link URI paths.
    pub paths: &'a Paths,
}

/// The kind of an external link.
#[derive(Clone, Copy, Debug)]
pub(super) enum ExternalLinkKind {
    /// An explicit external link with no content.
    Autonumber,
    /// An autolink.
    Free,
    /// An explicit external link with text content.
    Text,
}

impl ExternalLinkKind {
    /// The CSS class for this kind of external link.
    #[inline]
    fn css(self) -> &'static str {
        match self {
            Self::Autonumber => "external autonumber",
            Self::Free => "external free",
            Self::Text => "external text",
        }
    }
}

/// A kind of link to render.
#[derive(Clone, Debug)]
pub(super) enum LinkKind<'a> {
    /// An external link.
    External(Cow<'a, str>, ExternalLinkKind),
    /// An internal link.
    Internal(Title),
}

impl LinkKind<'_> {
    /// Converts the link to a URI-encoded string suitable for use in an HTML
    /// `href` attribute.
    pub fn to_string(&self, options: &LinkKindOptions<'_>, query: Option<&str>) -> String {
        match self {
            LinkKind::External(url, _) => {
                let url = decode_html(url).map(url_encode_sanitized);

                // TODO: Hack together some URL parsing good enough that there
                // is an actual way to check that the origin is the same
                if let Some(external) = options.paths.external
                    && (!url.starts_with('/') || url.starts_with("//"))
                {
                    make_url(
                        options.base_uri,
                        None,
                        format_args!("{external}/{url}"),
                        None,
                        None,
                    )
                } else {
                    url.to_string()
                }
            }
            LinkKind::Internal(title) => {
                if let Some(iw) = title
                    .interwiki()
                    .and_then(|iw| options.interwiki_map.get(&iw.to_ascii_lowercase()))
                {
                    let url = iw.replace("$1", &title.partial_url());
                    if let Some(external) = options.paths.external {
                        make_url(
                            options.base_uri,
                            None,
                            format_args!("{external}/{url}"),
                            None,
                            title.fragment(),
                        )
                    } else if let Some(fragment) = title.fragment()
                        && !fragment.is_empty()
                    {
                        format!("{url}#{}", anchor_encode(fragment, AnchorEncodeMode::Html5))
                    } else {
                        url
                    }
                } else if title.prefixed_text().is_empty() {
                    format!(
                        "#{}",
                        anchor_encode(
                            title.fragment().unwrap_or_default(),
                            AnchorEncodeMode::Html5
                        )
                    )
                } else {
                    make_url(
                        options.base_uri,
                        None,
                        format_args!("{}/{}", options.paths.article, title.partial_url()),
                        query,
                        title.fragment(),
                    )
                }
            }
        }
    }
}

/// Serialises values which are structured like
/// `{argument}{delimiter}{argument}...`.
pub(super) fn render_single_attribute<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
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
    W: Surrogate<Error> + ?Sized,
    F: FnOnce(&mut State<'_, '_, '_>, &mut String) -> Spanned<Token>,
>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    f: F,
) -> Result {
    let source = &mut String::new();
    let token = f(state, source);
    out.adopt_token(state, &sp.clone_with_source(FileMap::new(source)), &token)
}

/// Renders runtime-generated tokens.
pub(super) fn render_runtime_list<
    W: Surrogate<Error> + ?Sized,
    F: FnOnce(&mut State<'_, '_, '_>, &mut String) -> Vec<Spanned<Token>>,
>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    f: F,
) -> Result {
    let source = &mut String::new();
    let tokens = f(state, source);
    out.adopt_tokens(state, &sp.clone_with_source(FileMap::new(source)), &tokens)
}

/// Phrasing content, per the HTML5 specification, including obsolete elements
/// allowed by MediaWiki.
pub(super) const PHRASING_TAGS: phf::Set<&str> = phf::phf_set! {
    "a", "abbr", "area", "audio", "b", "bdi", "bdo", "big", "br", "button",
    "canvas", "cite", "code", "data", "datalist", "del", "dfn", "em", "embed",
    "font", "i", "iframe", "img", "input", "ins", "kbd", "label", "link", "map",
    "mark", "math", "meta", "meter", "noscript", "object", "output", "picture",
    "progress", "q", "rb", "rp", "rt", "rtc", "ruby", "s", "samp", "script",
    "selectedcontent", "slot", "small", "span", "strike", "strong", "sub",
    "sup", "svg", "template", "textarea", "time", "u", "var", "video", "wbr"
};
