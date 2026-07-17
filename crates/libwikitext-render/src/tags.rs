//! Plain HTML rendering functions.

use super::{
    Error, Paths, Result, StackFrame, State, Surrogate,
    document::{Document, DocumentSink},
    image::make_media_url,
    transform::Sink,
};
use core::convert::Infallible;
use libmisc::CowExt as _;
use libphp_rs::strtr;
use libwikitext_common::{
    AnchorEncodeMode,
    config::{Configuration, ImageHotlinking, SpecialPages},
    db::DatabaseProvider as _,
    decode_html, make_url,
    title::{Namespace, Title},
    url::Url,
    url_encode_sanitized,
};
use libwikitext_parse::{Argument, Span, Spanned, Token};
use regex::Regex;
use std::{borrow::Cow, sync::LazyLock};

/// Renders an external web site link.
pub(super) fn render_external_link<S: DocumentSink>(
    out: &mut Document<S>,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    target: &[Spanned<Token>],
    content: &[Spanned<Token>],
    auto_link: bool,
) -> Result {
    let target = sp.eval(state, target)?;

    if should_render_hotlink(state.statics.db.config(), &target) {
        render_hotlink(&mut out.next, state, &target);
        return Ok(());
    }

    // TODO: Handle “external” links that just come back to the wiki. Right now
    // it is annoying to try to do this because `http::Uri` does not conform to
    // RFC 3986 so it mixes up authority and path when the scheme is missing,
    // but adding a whole new dependency just for this one case is too much.
    let link = LinkKind::External(
        Cow::Borrowed(target.as_ref()),
        if auto_link {
            ExternalLinkKind::Free
        } else if content.is_empty() {
            ExternalLinkKind::Autonumber
        } else {
            ExternalLinkKind::Text
        },
    );
    render_start_link(&mut out.next, state, &link, false);
    if auto_link {
        out.next.text(decode_raw_url(&target).as_str());
    } else if content.is_empty() {
        let ordinal = &mut state.globals.external_link_ordinal;
        *ordinal += 1;
        out.next.text(&format!("[{ordinal}]"));
    } else {
        let target = sp.eval(state, content)?;
        if should_render_hotlink(state.statics.db.config(), &target) {
            render_hotlink(&mut out.next, state, &target);
        } else {
            out.adopt_tokens(state, sp, content)?;
        }
    }
    out.next.tag_end("a");
    Ok(())
}

/// Renders a hotlinked image.
fn render_hotlink<S: Sink + ?Sized>(out: &mut S, state: &mut State<'_, '_, '_>, target: &str) {
    let url = if let Some(external) = state.statics.paths.external {
        Cow::Owned(make_url(
            &state.statics.base_uri,
            None,
            format_args!("{external}/{}", url_encode_sanitized(target)),
            None,
            None,
        ))
    } else {
        Cow::Borrowed(target)
    };

    let (_, alt) = target.rsplit_once('/').unwrap();

    out.tag_start("img");
    out.tag_attribute_full("src", &url);
    out.tag_attribute_full("alt", alt);
    out.tag_start_end("img");
}

/// Returns true if the given `target` should be emitted as a hotlinked image.
fn should_render_hotlink(config: &Configuration, target: &str) -> bool {
    static RE_IMAGEY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^https?://.+\.(?i:avif|gif|jpe?g|png|svg|webp)$").unwrap());

    match config.image_hotlinking {
        ImageHotlinking::Disabled => false,
        ImageHotlinking::Whitelist { config, message } => {
            if !RE_IMAGEY.is_match(target) {
                false
            } else if message {
                log::warn!("TODO: external_image_whitelist");
                false
            } else {
                config.iter().any(|prefix| target.starts_with(prefix))
            }
        }
        ImageHotlinking::Enabled => RE_IMAGEY.is_match(target),
    }
}

/// Renders an internal link.
#[expect(clippy::too_many_arguments, reason = "this is how many there are")]
pub(super) fn render_internal_link<S: DocumentSink>(
    out: &mut Document<S>,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    target: &str,
    prefix: &[Spanned<Token>],
    content: &[Spanned<Argument>],
    trail: &[Spanned<Token>],
    title: Title,
) -> Result<(), Error> {
    if state.globals.title == title {
        out.next.tag_start("a");
        if let Some(fragment) = title.fragment_url(AnchorEncodeMode::Html5) {
            out.next.tag_attribute_full("class", "mw-selflink-fragment");
            out.next.tag_attribute_full("href", &format!("#{fragment}"));
        } else {
            out.next.tag_attribute_full("class", "mw-selflink selflink");
        }
        out.next.tag_start_end("a");
    } else {
        render_start_link(
            &mut out.next,
            state,
            &LinkKind::Internal(title, <_>::default()),
            false,
        );
    }

    out.adopt_tokens(state, sp, prefix)?;
    if content.is_empty() {
        out.next.text(&decode_html(target.trim_start_matches(':')));
    } else {
        render_single_attribute(out, state, sp, content)?;
    }
    out.adopt_tokens(state, sp, trail)?;
    out.next.tag_end("a");
    Ok(())
}

/// Renders an anchor for a link.
pub(super) fn render_start_link<W: Sink + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    link: &LinkKind<'_>,
    for_image: bool,
) {
    let (missing, query) = if let Some(title) = link.title()
        && !title.exists(&state.statics.db)
        && state
            .statics
            .db
            .config()
            .special_pages
            .alias(title.text())
            .is_none()
    {
        let query = (!title.is_in_namespace(Namespace::SPECIAL)).then_some("action=edit&redlink=1");
        (true, query)
    } else {
        <_>::default()
    };

    let options = LinkKindOptions {
        base_uri: &state.statics.base_uri,
        interwiki_map: &state.statics.db.config().interwiki_map,
        paths: &state.statics.paths,
    };

    // Links to the Media namespace in image `link` attributes are not supposed
    // to link directly to the resource, only direct links are supposed to do
    // this, so this check must happen here instead of in `LinkKind::to_string`
    let href = if let Some(title) = link.title()
        && title.is_in_namespace(Namespace::MEDIA)
    {
        make_media_url(options.base_uri, options.paths.media, &title.text_url())
    } else {
        link.to_string(&options, query)
    };

    let href = if missing {
        if let Some(title) = link.title()
            && for_image
        {
            let config = state.statics.db.config();
            let special = config
                .special_pages
                .canonical_title(config, SpecialPages::UPLOAD, None)
                .expect("configured special pages are valid");
            Cow::Owned(
                LinkKind::Internal(special, <_>::default())
                    .to_string(&options, Some(&format!("wpDestFile={}", title.text_url()))),
            )
        } else {
            Cow::Borrowed(href.split_once('#').map_or(href.as_str(), |(lhs, _)| lhs))
        }
    } else {
        Cow::Borrowed(href.as_str())
    };

    out.tag_start("a");
    // Stupid redundancies and reordering are to avoid having to do a bunch of
    // contorting in the unit test runner, not because this is smart or makes
    // any sense or has any purpose
    match link {
        LinkKind::External(_, kind) => {
            out.tag_attribute_full("rel", "nofollow");
            out.tag_attribute_full("class", kind.css());
            out.tag_attribute_full("href", &href);
        }
        LinkKind::Internal(title, kind) => {
            out.tag_attribute_full("href", &href);
            if missing {
                out.tag_attribute_full("class", "new");
                if for_image {
                    out.tag_attribute_full("title", title.key());
                } else if let Ok(title) =
                    state
                        .statics
                        .messages
                        .format_message(None, true, ["red-link-title"], |key| {
                            Ok::<_, Infallible>((key == "1").then(|| title.key().into()))
                        })
                {
                    out.tag_attribute_full("title", &title);
                }
            } else if matches!(kind, InternalLinkKind::MagicIsbn) {
                out.tag_attribute_full("class", "internal mw-magiclink-isbn");
            } else if title.is_in_namespace(Namespace::MEDIA) {
                out.tag_attribute_full("class", "internal");
                out.tag_attribute_full("title", title.text());
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

/// The kind of an internal link.
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum InternalLinkKind {
    /// A regular title link.
    #[default]
    Normal,
    /// An ISBN magic link.
    MagicIsbn,
}

/// A kind of link to render.
#[derive(Clone, Debug)]
pub(super) enum LinkKind<'a> {
    /// An external link.
    External(Cow<'a, str>, ExternalLinkKind),
    /// An internal link.
    Internal(Title, InternalLinkKind),
}

impl LinkKind<'_> {
    /// Creates a new `LinkKind` for the given `title`.
    #[inline]
    pub fn from_title(title: Title) -> Self {
        Self::Internal(title, <_>::default())
    }

    /// Returns the internal [`Title`] of this link, if one exists.
    pub fn title(&self) -> Option<&Title> {
        match self {
            Self::External(..) => None,
            Self::Internal(title, _) => Some(title),
        }
    }

    /// Converts the link to a URI-encoded string suitable for use in an HTML
    /// `href` attribute.
    pub fn to_string(&self, options: &LinkKindOptions<'_>, query: Option<&str>) -> String {
        match self {
            Self::External(url, _) => {
                // TODO: Should this decoding really be done here and not at the
                // site where the LinkKind is created?
                let url = decode_raw_url(url);

                if let Some(external) = options.paths.external
                    && url.authority() != options.base_uri.authority()
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
            Self::Internal(title, _) => {
                if let Some(iw) = title
                    .interwiki()
                    .and_then(|iw| options.interwiki_map.get(&iw.to_ascii_lowercase()))
                {
                    let url = strtr(iw, &[("$1", &title.partial_url())]);
                    if let Some(external) = options.paths.external {
                        make_url(
                            options.base_uri,
                            None,
                            format_args!("{external}/{url}"),
                            None,
                            title.fragment(),
                        )
                    } else if let Some(fragment) = title.fragment_url(AnchorEncodeMode::Html5)
                        && !fragment.is_empty()
                    {
                        format!("{url}#{fragment}")
                    } else {
                        url.into_owned()
                    }
                } else if title.prefixed_text().is_empty() {
                    format!(
                        "#{}",
                        title
                            .fragment_url(AnchorEncodeMode::Html5)
                            .unwrap_or_default(),
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

/// Decodes a raw URL string into a cleaned `Url` object.
fn decode_raw_url(url: &str) -> Url {
    Url::lax(&decode_html(url).map(url_encode_sanitized))
        .map(clean_url)
        .unwrap()
}

/// Serialises values which are structured like
/// `{argument}{delimiter}{argument}...`.
///
/// This function can only be used on inputs without untokenised inclusion
/// control tags since otherwise if those tags are used in the interstitial
/// positions they will end up exposed in the output.
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
