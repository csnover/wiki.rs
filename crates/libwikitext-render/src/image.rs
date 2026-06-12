//! Code for handling MediaWiki images.

use super::{
    Result, StackFrame, State, Surrogate as _,
    document::Document,
    emitters::Sink,
    tags::{self, LinkKind},
};
use libmisc::{CowExt as _, to_ascii_lower};
use libwikitext_common::{db::DatabaseProvider as _, make_url, title::Title, url::Url};
use libwikitext_parse::{Argument, Spanned, Token, helpers::TextContent, visit::Visitor as _};
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

    options.attrs.insert(
        "src".into(),
        make_media_url(
            &state.statics.base_uri,
            state.statics.paths.media,
            &title.text_url(),
        )
        .into(),
    );

    options.link = Some(LinkKind::Internal(title));

    for argument in arguments {
        let value = sp
            .eval(state, argument.combined())?
            .map_ref(str::trim_ascii);
        let config = state.statics.db.config();
        let Some((value, arg)) = config.magic_word_matches(&value) else {
            options.caption = Some(argument.combined());
            continue;
        };

        if let Some(arg) = arg.map(str::trim_ascii) {
            if value.contains(&"img_alt") {
                options.attrs.insert("alt".into(), arg.to_owned().into());
            } else if value.contains(&"img_class") {
                if !arg.is_empty() {
                    options.attrs.insert("class".into(), arg.to_owned().into());
                }
            } else if value.contains(&"img_lang") {
                options.lang = Some(arg.to_owned().into());
            } else if value.contains(&"img_link") {
                options.link = if config.protocols_pattern.is_match(arg) {
                    Some(LinkKind::External(
                        arg.to_owned().into(),
                        tags::ExternalLinkKind::Text,
                    ))
                } else {
                    Title::new(config, arg, None).ok().map(LinkKind::Internal)
                };
            } else if value.contains(&"img_lossy") {
                options.lossy = Some(arg != "false");
            } else if value.contains(&"img_page") {
                options.page = Some(arg.parse::<i32>().unwrap_or(1));
            } else if value.contains(&"img_upright") {
                options.upright = Some(arg.parse::<f64>().unwrap_or(1.0));
            } else if value.contains(&"img_width") {
                let (w, h) = arg.split_once('x').unwrap_or((arg, ""));
                if w.as_bytes().iter().all(u8::is_ascii_digit) {
                    options.attrs.insert("width".into(), w.to_owned().into());
                }
                if h.as_bytes().iter().all(u8::is_ascii_digit) {
                    options.attrs.insert("height".into(), h.to_owned().into());
                }
            } else if value.contains(&"timedmedia_disablecontrols") {
                log::warn!("TODO: timedmedia_disablecontrols");
            } else if value.contains(&"timedmedia_endtime") {
                log::warn!("TODO: timedmedia_endtime");
            } else if value.contains(&"timedmedia_starttime") {
                options.start = Some(arg.to_owned().into());
            } else if value.contains(&"timedmedia_thumbtime") {
                options.thumbtime = Some(arg.to_owned().into());
            } else {
                log::warn!("unexpected magic word {value:?}");
            }
        } else if value.contains(&"img_border") {
            options.border = Some(());
        } else if value.contains(&"img_center") {
            options.align = Some("center".into());
        } else if value.contains(&"img_left") {
            options.align = Some("left".into());
        } else if value.contains(&"img_none") {
            options.align = Some("none".into());
        } else if value.contains(&"img_right") {
            options.align = Some("right".into());
        } else if value.contains(&"img_baseline") {
            options.attrs.insert("valign".into(), "baseline".into());
        } else if value.contains(&"img_middle") {
            options.attrs.insert("valign".into(), "middle".into());
        } else if value.contains(&"img_sub") {
            options.attrs.insert("valign".into(), "sub".into());
        } else if value.contains(&"img_super") {
            options.attrs.insert("valign".into(), "super".into());
        } else if value.contains(&"img_text_bottom") {
            options.attrs.insert("valign".into(), "text-bottom".into());
        } else if value.contains(&"img_text_top") {
            options.attrs.insert("valign".into(), "text-top".into());
        } else if value.contains(&"img_top") {
            options.attrs.insert("valign".into(), "top".into());
        } else if value.contains(&"img_framed") {
            options.format = Some("frame".into());
        } else if value.contains(&"img_frameless") {
            options.format = Some("frameless".into());
        } else if value.contains(&"img_thumbnail") {
            options.format = Some("thumb".into());
        } else if value.contains(&"img_upright") {
            options.upright = Some(0.75);
        } else if value.contains(&"timedmedia_muted") {
            options.muted = Some(());
        } else if value.contains(&"timedmedia_loop") {
            options.r#loop = Some(());
        } else {
            options.caption = Some(argument.combined());
        }
    }

    if matches!(options.format.as_deref(), Some("thumb" | "frame")) {
        options.align.get_or_insert("right".into());
    } else if let Some(caption) = options.caption.take() {
        let mut extractor = TextContent::new(
            state.statics.db.config(),
            state.globals.title.namespace().is_talk(),
            &sp.source,
            String::new(),
        );
        extractor.visit_tokens(caption)?;
        options.attrs.insert(
            "title".into(),
            extractor.finish().trim_ascii().to_owned().into(),
        );
    }

    Ok(options)
}

/// Creates a URL to a title in the [media namespace].
///
/// [media namespace]: libwikitext_common::title::Namespace::MEDIA
pub(super) fn make_media_url(base_uri: &Url, media_path: &str, text: &str) -> String {
    let path = format_args!("{media_path}/{text}");
    if is_absolute_url(media_path) {
        path.to_string()
    } else {
        make_url(base_uri, None, path, None, None)
    }
}

/// Renders a media tag.
pub(super) fn render_media(
    out: &mut Document,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    title: Title,
    arguments: &[Spanned<Argument>],
) -> Result {
    let options = media_options(state, sp, title, arguments, <_>::default())?;
    render_media_with_options(out, state, sp, &options)
}

/// Renders a media tag using the given media options.
pub(super) fn render_media_with_options(
    out: &mut Document,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    options: &Options<'_>,
) -> Result {
    if options.caption.is_some() {
        out.next.tag_start("figure");
        if let Some(align) = &options.align {
            out.next
                .tag_attribute_full("class", &format!("mw-halign-{align}"));
        }
        out.next.tag_start_end("figure");
    }

    match options.kind {
        MediaKind::Audio | MediaKind::Video => {
            render_timed_media(&mut out.next, options);
        }
        MediaKind::Image => {
            render_image(&mut out.next, state, options);
        }
    }

    if let Some(body) = options.caption {
        out.next.tag_start_full("figcaption");
        out.adopt_tokens(state, sp, body)?;
        out.next.tag_end("figcaption");
        out.next.tag_end("figure");
    }

    Ok(())
}

/// Renders an image tag.
fn render_image<S: Sink + ?Sized>(
    out: &mut S,
    state: &mut State<'_, '_, '_>,
    options: &Options<'_>,
) {
    if let Some(link) = &options.link {
        tags::render_start_link(out, state, link);
    }

    out.tag_start(options.kind.tag_name());

    if options.caption.is_none()
        && let Some(align) = &options.align
    {
        out.tag_attribute_full("align", align);
    }

    for (k, v) in &options.attrs {
        out.tag_attribute_full(k, v);
    }

    // This is a void tag so there is no `tag_end`
    out.tag_start_end(options.kind.tag_name());

    if options.link.is_some() {
        out.tag_end("a");
    }
}

/// Renders an audio or video tag.
// TODO: This is even more bogus than the image tags; this does not even *use*
// most of the timed media options.
fn render_timed_media<S: Sink + ?Sized>(out: &mut S, options: &Options<'_>) {
    let tag_name = options.kind.tag_name();
    out.tag_start(tag_name);
    out.tag_attribute_full("controls", "");
    for (k, v) in &options.attrs {
        out.tag_attribute_full(k, v);
    }
    out.tag_start_end(tag_name);
    out.tag_end(tag_name);
}

/// Returns `true` if the string appears to be an absolute URL.
// TODO: This is a hack for the test suite. At the least it should be using
// `config.protocols`.
#[inline]
fn is_absolute_url(str: &str) -> bool {
    str.starts_with("http://") || str.starts_with("https://")
}
