//! Code for handling MediaWiki images.

use super::{
    Error, Result, StackFrame, State, Surrogate,
    tags::{self, LinkKind},
    text_run,
};
use core::iter;
use libmisc::{CowExt as _, to_ascii_lower};
use libwikitext_common::{db::DatabaseProvider as _, make_url, title::Title};
use libwikitext_parse::{
    Argument, FileMap, Spanned, Token,
    builder::{tok_arg, token},
    helpers::TextContent,
    visit::Visitor as _,
};
use std::{borrow::Cow, collections::BTreeMap};

/// The kind of media.
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum MediaKind {
    /// Beeps and boops.
    Audio,
    /// Soul thievery.
    #[default]
    Image,
    /// Witchcraft, sometimes with added beeps and boops.
    Video,
}

impl MediaKind {
    /// The HTML tag associated with this kind of media.
    #[inline]
    fn tag_name(self) -> &'static str {
        match self {
            MediaKind::Audio => "audio",
            MediaKind::Image => "img",
            MediaKind::Video => "video",
        }
    }
}

/// Options for rendering a media node.
#[derive(Clone, Debug, Default)]
pub(super) struct Options<'a> {
    /// Horizontal image alignment. One of 'left', 'right', 'center', or 'none'.
    pub(super) align: Option<Cow<'a, str>>,
    /// Arbitrary HTML attributes to apply to the HTML tag.
    ///
    /// A `BTreeMap` is used instead of a `HashMap` for key stability across
    /// runs.
    pub(super) attrs: BTreeMap<Cow<'a, str>, Cow<'a, str>>,
    /// Render the image with a border??? (lol).
    border: Option<()>,
    /// The caption for the media. This will be rendered below the image in
    /// 'thumb' or 'frame' format, and otherwise as a tooltip.
    caption: Option<&'a [Spanned<Token>]>,
    /// The intended format of the image. One of 'frameless', 'frame', 'framed',
    /// 'thumb', or 'thumbnail'.
    pub(super) format: Option<Cow<'a, str>>,
    /// The kind of the media.
    kind: MediaKind,
    /// The language to use when rendering an SVG with `<switch>` options
    /// varying on a `systemLanguage` attribute.
    lang: Option<Cow<'a, str>>,
    /// The target URL for an image link. This can be either a bare external URL
    /// or a bare article title.
    link: Option<LinkKind<'a>>,
    /// Whether the media should be looped continuously when played.
    r#loop: Option<()>,
    /// Whether to use PNG instead of JPEG thumbnails from TIFF files.
    lossy: Option<bool>,
    /// Whether the audio of an, uh, *image*, should be muted.
    muted: Option<()>,
    /// The page number to extract and render from a DJVU or PDF image.
    page: Option<i32>,
    /// The playback start time for a video… er… image.
    start: Option<Cow<'a, str>>,
    /// The timestamp to extract and render as a still from a video file.
    thumbtime: Option<Cow<'a, str>>,
    /// “Resizes an image to a multiple of the user’s thumbnail size
    /// preferences”. This will probably never be implemented, but it will be
    /// recorded.
    upright: Option<f64>,
}

/// Parses [`Options`] from a media node.
#[expect(
    clippy::too_many_lines,
    reason = "not enough value in splitting this into smaller units"
)]
// TODO: This needs to use `config.extra_words` instead of hard coding the
// keywords.
pub(super) fn media_options<'s>(
    state: &mut State<'_, '_, '_>,
    sp: &'s StackFrame<'_>,
    title: Title,
    arguments: &'s [Spanned<Argument>],
    mut options: Options<'s>,
) -> Result<Options<'s>> {
    options.kind = if let Some((_, ext)) = title.key().rsplit_once('.') {
        // TODO: Get from config. API has siprops "fileextensions".
        match to_ascii_lower(ext).as_ref() {
            "mid" | "ogg" | "oga" | "flac" | "opus" | "wav" | "mp3" | "midi" => MediaKind::Audio,
            "ogv" | "webm" | "mpg" | "mpeg" => MediaKind::Video,
            _ => MediaKind::Image,
        }
    } else {
        MediaKind::Image
    };

    let path = format_args!("{}/{}", state.statics.paths.media, title.text_url());
    options.attrs.insert(
        "src".into(),
        Cow::Owned(if is_absolute_url(state.statics.paths.media) {
            path.to_string()
        } else {
            make_url(&state.statics.base_uri, None, path, None, None)
        }),
    );

    options.link = Some(LinkKind::Internal(title));

    for argument in arguments {
        let value = sp.eval(state, argument.value())?.map_ref(str::trim_ascii);
        if let Some(name_node) = &argument.name() {
            let name = sp.eval(state, name_node)?;
            if name == "link" {
                // “If there is a space character between link and the
                // equals sign, the link statement will be treated as a
                // caption.” This will happen because evaluating
                // `argument.name` does not strip whitespace so the key will
                // not match.
                options.link = if value.is_empty() {
                    None
                } else if state.statics.db.config().protocols.iter().any(|proto| {
                    value
                        .get(..proto.len())
                        .is_some_and(|v| v.eq_ignore_ascii_case(proto))
                }) {
                    Some(LinkKind::External(value, tags::ExternalLinkKind::Text))
                } else {
                    Title::new(state.statics.db.config(), &value, None)
                        .ok()
                        .map(LinkKind::Internal)
                };
            } else if name == "alt" {
                // “If there is a space character between alt and the equals
                // sign, the alt statement will be treated as a caption.”
                // This will happen because evaluating `argument.name` does
                // not strip whitespace so the key will not match.
                options.attrs.insert(name, text_run(state, &value).into());
            } else {
                match name.trim_ascii() {
                    "upright" => {
                        options.upright = Some(value.parse::<f64>().unwrap_or(1.0));
                    }
                    "page" => {
                        options.page = Some(value.parse::<i32>().unwrap_or(1));
                    }
                    "thumbtime" => {
                        options.thumbtime = Some(value);
                    }
                    "start" => {
                        options.start = Some(value);
                    }
                    "lossy" => {
                        options.lossy = Some(value != "false");
                    }
                    "class" => {
                        if !value.is_empty() {
                            options.attrs.insert(name, value);
                        }
                    }
                    "lang" => {
                        options.lang = Some(value);
                    }
                    "border" => {
                        options.border = Some(());
                    }
                    _ => {
                        options.caption = Some(argument.combined());
                    }
                }
            }
        } else if value.ends_with("px") {
            let value = value.trim_end_matches("px").trim_ascii_end();
            let (w, h) = value.split_once('x').unwrap_or((value, ""));
            if let Ok(value) = w.parse::<i32>() {
                options
                    .attrs
                    .insert("width".into(), Cow::Owned(value.to_string()));
            }
            if let Ok(value) = h.parse::<i32>() {
                options
                    .attrs
                    .insert("height".into(), Cow::Owned(value.to_string()));
            }
        } else if let Some(value) = value.strip_prefix("upright ") {
            options.upright = Some(value.parse::<f64>().unwrap_or(1.0));
        } else {
            match value.as_ref() {
                "upright" => {
                    options.upright = Some(0.75);
                }
                "left" | "right" | "center" | "none" => {
                    options.align = Some(value);
                }
                "baseline" | "sub" | "super" | "top" | "text-top" | "middle" | "bottom"
                | "text-bottom" => {
                    options.attrs.insert("valign".into(), value);
                }
                "frameless" | "frame" | "thumb" => {
                    options.format = Some(value);
                }
                "framed" => {
                    options.format = Some("frame".into());
                }
                "thumbnail" => {
                    options.format = Some("thumb".into());
                }
                "muted" => {
                    options.muted = Some(());
                }
                "loop" => {
                    options.r#loop = Some(());
                }
                _ => {
                    options.caption = Some(argument.combined());
                }
            }
        }
    }

    if matches!(options.format.as_deref(), Some("thumb" | "frame")) {
        options.align.get_or_insert("right".into());
    } else if let Some(caption) = options.caption.take() {
        let mut extractor = TextContent::new(state.statics.db.config(), &sp.source, String::new());
        extractor.visit_tokens(caption)?;
        options.attrs.insert(
            "title".into(),
            text_run(state, extractor.finish().trim_ascii()).into(),
        );
    }

    Ok(options)
}

/// Renders a media tag.
pub(super) fn render_media<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    title: Title,
    arguments: &[Spanned<Argument>],
) -> Result {
    let options = media_options(state, sp, title, arguments, <_>::default())?;
    render_media_with_options(out, state, sp, &options)
}

/// Renders a media tag using the given media options.
pub(super) fn render_media_with_options<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    options: &Options<'_>,
) -> Result {
    if options.caption.is_some() {
        tags::render_runtime(out, state, sp, |_, source| {
            token!(
                source,
                Token::StartTag {
                    name: token!(source, Span { "figure" }),
                    attributes: if let Some(align) = &options.align {
                        vec![tok_arg(source, "class", format!("mw-halign-{align}"))]
                    } else {
                        vec![]
                    },
                    self_closing: false,
                }
            )
        })?;
    }

    match options.kind {
        MediaKind::Audio | MediaKind::Video => {
            render_timed_media(out, state, sp, options)?;
        }
        MediaKind::Image => {
            render_image(out, state, sp, options)?;
        }
    }

    if let Some(body) = options.caption {
        tags::render_runtime(out, state, sp, |_, source| {
            token!(
                source,
                Token::StartTag {
                    name: token!(source, Span { "figcaption" }),
                    attributes: vec![],
                    self_closing: false
                }
            )
        })?;

        out.adopt_tokens(state, sp, body)?;

        let source = &mut String::new();
        let end = token!(
            source,
            [
                Token::EndTag {
                    name: token!(source, Span { "figcaption" })
                },
                Token::EndTag {
                    name: token!(source, Span { "figure" })
                }
            ]
        );

        out.adopt_tokens(state, &sp.clone_with_source(FileMap::new(source)), &end)?;
    }

    Ok(())
}

/// Renders an image tag.
fn render_image<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    options: &Options<'_>,
) -> Result {
    if let Some(link) = &options.link {
        tags::render_start_link(out, state, sp, link)?;
    }

    tags::render_runtime(out, state, sp, |_, source| {
        token!(
            source,
            Token::StartTag {
                name: token!(source, Span { options.kind.tag_name() }),
                attributes: {
                    alignment(source, options)
                        .chain(
                            options
                                .attrs
                                .iter()
                                .map(|(key, value)| tok_arg(source, key, value)),
                        )
                        .collect()
                },
                self_closing: true
            }
        )
    })?;

    if options.link.is_some() {
        tags::render_end_link(out, state, sp)?;
    }

    Ok(())
}

/// Renders an audio or video tag.
// TODO: This is even more bogus than the image tags; this does not even *use*
// most of the timed media options.
fn render_timed_media<W: Surrogate<Error> + ?Sized>(
    out: &mut W,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    options: &Options<'_>,
) -> Result {
    tags::render_runtime_list(out, state, sp, |_, source| {
        token![
            source,
            [
                Token::StartTag {
                    name: token!(source, Span { options.kind.tag_name() }),
                    attributes: {
                        iter::once(tok_arg(source, "controls", ""))
                            .chain(
                                options
                                    .attrs
                                    .iter()
                                    .map(|(key, value)| tok_arg(source, key, value)),
                            )
                            .collect()
                    },
                    self_closing: false
                },
                Token::EndTag {
                    name: token!(source, Span { options.kind.tag_name() })
                }
            ]
        ]
        .into()
    })
}

/// Generates an image tag horizontal alignment attribute iterator.
fn alignment(
    source: &mut String,
    options: &Options<'_>,
) -> impl Iterator<Item = Spanned<Argument>> + use<> {
    if options.caption.is_none()
        && let Some(align) = &options.align
    {
        Some(tok_arg(source, "align", align))
    } else {
        None
    }
    .into_iter()
}

/// Returns `true` if the string appears to be an absolute URL.
// TODO: This is a hack for the test suite. At the least it should be using
// `config.protocols`.
#[inline]
fn is_absolute_url(str: &str) -> bool {
    str.starts_with("http://") || str.starts_with("https://")
}
