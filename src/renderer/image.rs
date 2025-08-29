//! Code for handling MediaWiki images.

use super::{
    Error, StackFrame, State, WriteSurrogate,
    tags::{self, LinkKind},
};
use crate::{
    config::CONFIG,
    title::Title,
    wikitext::{
        Argument, FileMap, Spanned, Token,
        builder::{tok_arg, token},
        helpers::TextContent,
        visit::Visitor as _,
    },
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::{borrow::Cow, collections::HashMap};

/// Options for rendering an image node.
#[derive(Debug, Default)]
pub(super) struct Options<'a> {
    /// Render the image with a border??? (lol).
    border: Option<()>,
    /// The intended format of the image. One of 'frameless', 'frame', 'framed',
    /// 'thumb', or 'thumbnail'.
    format: Option<Cow<'a, str>>,
    /// Horizontal image alignment. One of 'left', 'right', 'center', or 'none'.
    align: Option<Cow<'a, str>>,
    /// Arbitrary HTML attributes to apply to the `<img>`.
    attrs: HashMap<Cow<'a, str>, Cow<'a, str>>,
    /// “Resizes an image to a multiple of the user’s thumbnail size
    /// preferences”. This will probably never be implemented, but it will be
    /// recorded.
    upright: Option<f64>,
    /// The target URL for an image link. This can be either a bare external URL
    /// or a bare article title.
    link: Option<LinkKind<'a>>,
    /// The page number to extract and render from a DJVU or PDF image.
    page: Option<i32>,
    /// The timestamp to extract and render as a still from a video file.
    thumbtime: Option<Cow<'a, str>>,
    /// The playback start time for a video… er… image.
    start: Option<Cow<'a, str>>,
    /// Whether the audio of an, uh, *image*, should be muted.
    muted: Option<()>,
    /// Whether the media should be looped continuously when played.
    r#loop: Option<()>,
    /// Whether to use PNG instead of JPEG thumbnails from TIFF files.
    lossy: Option<bool>,
    /// The language to use when rendering an SVG with `<switch>` options
    /// varying on a `systemLanguage` attribute.
    lang: Option<Cow<'a, str>>,
    /// The caption of the image. This will be rendered below the image in
    /// 'thumb' or 'frame' format, and otherwise as a tooltip.
    caption: Option<&'a [Spanned<Token>]>,
}

/// Parses [`Options`] from an image node.
#[allow(clippy::too_many_lines)]
pub(super) fn image_options<'s>(
    state: &mut State<'_>,
    sp: &'s StackFrame<'_>,
    title: Title,
    arguments: &'s [Spanned<Argument>],
) -> Result<Options<'s>, Error> {
    let mut options = Options::default();

    options.attrs.insert(
        "src".into(),
        Cow::Owned(format!(
            "{}/images/{}",
            state.statics.base_uri.path(),
            utf8_percent_encode(title.text(), NON_ALPHANUMERIC)
        )),
    );

    options.link = Some(LinkKind::Internal(title));

    for argument in arguments {
        let value = match sp.eval(state, argument.value())? {
            Cow::Borrowed(v) => Cow::Borrowed(v.trim_ascii()),
            Cow::Owned(o) => Cow::Owned(o.trim_ascii().to_string()),
        };
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
                } else if CONFIG.protocols.iter().any(|proto| {
                    value
                        .get(..proto.len())
                        .is_some_and(|v| v.eq_ignore_ascii_case(proto))
                }) {
                    Some(LinkKind::External(value))
                } else {
                    Some(LinkKind::Internal(Title::new(&value, None)))
                };
            } else if name == "alt" {
                // “If there is a space character between alt and the equals
                // sign, the alt statement will be treated as a caption.”
                // This will happen because evaluating `argument.name` does
                // not strip whitespace so the key will not match.
                options.attrs.insert(name, value);
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
        } else {
            match &*value {
                "left" | "right" | "center" | "none" => {
                    options.align = Some(value);
                }
                "baseline" | "sub" | "super" | "top" | "text-top" | "middle" | "bottom"
                | "text-bottom" => {
                    options.attrs.insert("valign".into(), value);
                }
                "frameless" | "frame" | "framed" | "thumb" | "thumbnail" => {
                    options.format = Some(value);
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

    if !matches!(options.format.as_deref(), Some("thumb" | "frame"))
        && let Some(caption) = options.caption.take()
    {
        let mut extractor = TextContent::new(&sp.source, String::new());
        extractor.visit_tokens(caption)?;
        let title = extractor.finish();
        options
            .attrs
            .insert("title".into(), title.trim_ascii().to_string().into());
    }

    Ok(options)
}

/// Renders an image node.
pub(crate) fn render_image<W: WriteSurrogate + ?Sized>(
    out: &mut W,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    title: Title,
    arguments: &[Spanned<Argument>],
) -> Result<(), Error> {
    let options = image_options(state, sp, title, arguments)?;

    if options.caption.is_some() {
        tags::render_runtime(out, state, sp, |source| {
            token!(
                source,
                Token::StartTag {
                    name: token!(source, Span { "figure" }),
                    attributes: if let Some(align) = &options.align {
                        vec![tok_arg(source, "class", format!("mw-halign-{align}"))]
                    } else if let Some("thumb") = options.format.as_deref() {
                        vec![tok_arg(source, "class", "mw-halign-right")]
                    } else {
                        vec![]
                    },
                    self_closing: false,
                }
            )
        })?;
    }

    if let Some(link) = &options.link {
        tags::render_start_link(out, state, sp, link)?;
    }

    tags::render_runtime(out, state, sp, |source| {
        token!(
            source,
            Token::StartTag {
                name: token!(source, Span { "img" }),
                attributes: {
                    let mut attrs = options
                        .attrs
                        .iter()
                        .map(|(key, value)| tok_arg(source, key, value))
                        .collect::<Vec<_>>();
                    if options.caption.is_none()
                        && let Some(align) = &options.align
                    {
                        attrs.push(tok_arg(source, "align", align));
                    }
                    attrs
                },
                self_closing: true
            }
        )
    })?;

    if options.link.is_some() {
        tags::render_end_link(out, state, sp)?;
    }

    if let Some(body) = options.caption {
        tags::render_runtime(out, state, sp, |source| {
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
