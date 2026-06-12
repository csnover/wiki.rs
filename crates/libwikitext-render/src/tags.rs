//! Plain HTML rendering functions.

use super::{
    Error, Paths, Result, StackFrame, State, Surrogate, document::Document, emitters::Sink,
    image::make_media_url,
};
use libmisc::CowExt as _;
use libwikitext_common::{
    AnchorEncodeMode, anchor_encode,
    db::DatabaseProvider as _,
    decode_html, make_url,
    title::{Namespace, Title},
    url::Url,
    url_encode_sanitized,
};
use libwikitext_parse::{Argument, Span, Spanned, Token};
use std::borrow::Cow;

/// Renders an external web site link.
pub(super) fn render_external_link(
    out: &mut Document,
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
    render_start_link(&mut out.next, state, &link);
    if content.is_empty() {
        let ordinal = &mut state.globals.external_link_ordinal;
        *ordinal += 1;
        let text = format!("[{ordinal}]");
        out.adopt_generated(state, sp, None, &text)?;
    } else {
        out.adopt_tokens(state, sp, content)?;
    }
    out.next.tag_end("a");
    Ok(())
}

/// Renders an internal link.
#[expect(clippy::too_many_arguments, reason = "this is how many there are")]
pub(super) fn render_internal_link(
    out: &mut Document,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    target: &str,
    prefix: Option<&str>,
    content: &[Spanned<Argument>],
    trail: Option<&str>,
    title: Title,
) -> Result<(), Error> {
    if state.globals.title == title {
        out.next.tag_start("a");
        if let Some(fragment) = title.fragment() {
            out.next.tag_attribute_full("class", "mw-selflink-fragment");
            out.next.tag_attribute_full(
                "href",
                &format!("#{}", anchor_encode(fragment, AnchorEncodeMode::Html5)),
            );
        } else {
            out.next.tag_attribute_full("class", "mw-selflink selflink");
        }
        out.next.tag_start_end("a");
    } else {
        render_start_link(&mut out.next, state, &LinkKind::Internal(title));
    }

    if let Some(prefix) = prefix {
        out.adopt_generated(state, sp, None, prefix)?;
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

    out.next.tag_end("a");
    Ok(())
}

/// Renders an anchor for a link.
pub(super) fn render_start_link<W: Sink + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    link: &LinkKind<'_>,
) {
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

    out.tag_start("a");
    // Stupid redundancies and reordering are to avoid having to do a bunch of
    // contorting in the unit test runner, not because this is smart or makes
    // any sense or has any purpose
    match link {
        LinkKind::External(_, kind) => {
            out.tag_attribute_full("rel", "nofollow");
            out.tag_attribute_full("class", kind.css());
            out.tag_attribute_full("href", href);
        }
        LinkKind::Internal(title) => {
            out.tag_attribute_full("href", href);
            if missing {
                out.tag_attribute_full("class", "new");
                if let Some(message) = state.messages.get("red-link-title") {
                    out.tag_attribute_full("title", &message.replace("$1", title.key()));
                }
            } else if !title.prefixed_text().is_empty() {
                if title.interwiki().is_some() {
                    out.tag_attribute_full("class", "extiw");
                }
                out.tag_attribute_full("title", title.prefixed_text());
            }
        }
    }
    out.tag_start_end("a");
}

/// Static option data for [`LinkKind::to_string`].
pub(crate) struct LinkKindOptions<'a> {
    /// The base URI for an internal link.
    pub base_uri: &'a Url,
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
    /// A PubMed magic link.
    MagicPmid,
    /// An RFC magic link.
    MagicRfc,
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
            Self::MagicPmid => "external mw-magiclink-pmid",
            Self::MagicRfc => "external mw-magiclink-rfc",
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

/// Cleans up a URL authority part according to the MediaWiki rules.
fn clean_url(mut url: Url) -> Url {
    fn valid_idn(c: char) -> bool {
        pub const PRECIS_IGNORABLE: &[(char, char)] = &[
            ('\u{00AD}', '\u{00AD}'),
            ('\u{034F}', '\u{034F}'),
            ('\u{061C}', '\u{061C}'),
            ('\u{115F}', '\u{1160}'),
            ('\u{17B4}', '\u{17B5}'),
            ('\u{180B}', '\u{180D}'),
            ('\u{180E}', '\u{180E}'),
            ('\u{200B}', '\u{200F}'),
            ('\u{202A}', '\u{202E}'),
            ('\u{2060}', '\u{2064}'),
            ('\u{2065}', '\u{2065}'),
            ('\u{2066}', '\u{206F}'),
            ('\u{3164}', '\u{3164}'),
            ('\u{FE00}', '\u{FE0F}'),
            ('\u{FEFF}', '\u{FEFF}'),
            ('\u{FFA0}', '\u{FFA0}'),
            ('\u{FFF0}', '\u{FFF8}'),
            ('\u{1BCA0}', '\u{1BCA3}'),
            ('\u{1D173}', '\u{1D17A}'),
            ('\u{E0000}', '\u{E0000}'),
            ('\u{E0001}', '\u{E0001}'),
            ('\u{E0002}', '\u{E001F}'),
            ('\u{E0020}', '\u{E007F}'),
            ('\u{E0080}', '\u{E00FF}'),
            ('\u{E0100}', '\u{E01EF}'),
            ('\u{E01F0}', '\u{E0FFF}'),
        ];
        for (first, last) in PRECIS_IGNORABLE {
            if c >= *first && c <= *last {
                return false;
            }
        }
        !c.is_whitespace()
    }

    const IPV6_START: &str = "//%5B";
    const AUTH: &str = "//";
    const IPV6_END: &str = "%5D";

    if let Some(authority) = url.authority() {
        let mut out = String::new();
        let mut flushed = 0;
        for (index, c) in authority.char_indices().filter(|(_, c)| !valid_idn(*c)) {
            out += &authority[flushed..index];
            flushed = index + c.len_utf8();
        }
        let mut authority = if flushed != 0 {
            out += &authority[flushed..];
            Cow::Owned(out)
        } else {
            Cow::Borrowed(authority)
        };

        // This is decoding the brackets in IPv6/IPvFuture hosts
        if authority.starts_with(IPV6_START)
            && let Some(end) = authority.find(IPV6_END)
        {
            let authority = authority.to_mut();
            authority.replace_range(end..end + IPV6_END.len(), "]");
            authority.replace_range(AUTH.len()..IPV6_START.len(), "[");
        }

        if let Cow::Owned(authority) = authority {
            url.set_authority(&authority);
        }
    }

    url
}

impl LinkKind<'_> {
    /// Converts the link to a URI-encoded string suitable for use in an HTML
    /// `href` attribute.
    pub fn to_string(&self, options: &LinkKindOptions<'_>, query: Option<&str>) -> String {
        match self {
            LinkKind::External(url, _) => {
                let url = Url::lax(&decode_html(url).map(url_encode_sanitized))
                    .map(clean_url)
                    .unwrap();

                // TODO: Hack together some URL parsing good enough that there
                // is an actual way to check that the origin is the same
                if let Some(external) = options.paths.external
                    && url.is_absolute()
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
                } else if title.namespace().id == Namespace::MEDIA {
                    make_media_url(options.base_uri, options.paths.media, &title.text_url())
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
