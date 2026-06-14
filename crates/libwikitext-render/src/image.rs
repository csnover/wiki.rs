//! Code for handling MediaWiki images.

use super::{
    Result, StackFrame, State, StripMarkers, Surrogate as _,
    document::Document,
    emitters::Sink,
    tags::{self, LinkKind},
};
use libmisc::{CowExt as _, to_ascii_lower};
use libwikitext_common::{
    db::DatabaseProvider as _,
    make_url, normalize_whitespace,
    title::{Namespace, Title},
    url::Url,
    url_decode,
};
use libwikitext_parse::{Argument, Spanned, Token};
use std::borrow::Cow;

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
#[expect(clippy::struct_excessive_bools, reason = "that is how many there are!")]
#[derive(Clone, Debug)]
pub(super) struct Options<'a> {
    /// Horizontal image alignment. One of 'left', 'right', 'center', or 'none'.
    pub(super) align: Option<&'static str>,
    /// The alternate text for an image.
    alt: Option<Cow<'a, str>>,
    /// Render the image with a… border??? (lol).
    border: bool,
    /// The caption for the media. This will be rendered below the image in
    /// 'thumb' or 'frame' format, and otherwise as a tooltip.
    caption: Option<&'a [Spanned<Token>]>,
    /// A list of CSS class names to apply to the image container.
    class: Option<Cow<'a, str>>,
    /// If `true`, show controls on image… videos and audio.
    controls: bool,
    /// The playback end time for a video… er… image.
    end: Option<Cow<'a, str>>,
    /// The intended format of the image.
    pub(super) frame: Option<FrameKind>,
    /// The height override for the image.
    pub(super) height: Option<u32>,
    /// The kind of the media.
    kind: MediaKind,
    /// The language to use when rendering an SVG with `<switch>` options
    /// varying on a `systemLanguage` attribute.
    lang: Option<Cow<'a, str>>,
    /// The target URL for an image link. This can be either a bare external URL
    /// or a bare article title.
    link: Option<LinkKind<'a>>,
    /// The title attribute to apply to the image link.
    link_title: Option<Cow<'a, str>>,
    /// Whether the media should be looped continuously when played.
    r#loop: bool,
    /// Whether to use PNG instead of JPEG thumbnails from TIFF files.
    lossy: Option<bool>,
    /// Whether the audio of an, uh, *image*, should be muted.
    muted: bool,
    /// The page number to extract and render from a DJVU or PDF image.
    page: Option<i32>,
    /// The playback start time for a video… er… image.
    start: Option<Cow<'a, str>>,
    /// The target title for the image.
    title: Title,
    /// The timestamp to extract and render as a still from a video file.
    thumbtime: Option<Cow<'a, str>>,
    /// “Resizes an image to a multiple of the user’s thumbnail size
    /// preferences”. This will probably never be implemented, but it will be
    /// recorded.
    upright: Option<f64>,
    /// The vertical alignment of an image.
    valign: Option<&'static str>,
    /// The width override for the image.
    width: Option<u32>,
}

impl Options<'_> {
    /// Creates a new `Options` with the given `title`.
    fn new(title: Title) -> Self {
        Self {
            align: <_>::default(),
            alt: <_>::default(),
            border: <_>::default(),
            caption: <_>::default(),
            class: <_>::default(),
            controls: true,
            end: <_>::default(),
            frame: <_>::default(),
            height: <_>::default(),
            kind: <_>::default(),
            lang: <_>::default(),
            link: <_>::default(),
            link_title: <_>::default(),
            r#loop: <_>::default(),
            lossy: <_>::default(),
            muted: <_>::default(),
            page: <_>::default(),
            start: <_>::default(),
            title,
            thumbtime: <_>::default(),
            upright: <_>::default(),
            valign: <_>::default(),
            width: <_>::default(),
        }
    }
}

/// An image rendering strategy.
#[derive(Clone, Debug)]
pub(super) enum FrameKind {
    /// Show the image with a frame. Is a frame a border? Who could say.
    Frame,
    /// Show the image with no frame.
    Frameless,
    /// Show the image as a thumbnail with a border and a caption underneath the
    /// image, using the given optional `Title` for the actual thumbnail image.
    Thumb(Option<Title>),
}

impl FrameKind {
    /// Returns the Resource Description Framework type for an image.
    fn rdfa_kind(&self) -> &str {
        match self {
            Self::Frame => "mw:File/Frame",
            Self::Frameless => "mw:File/Frameless",
            Self::Thumb(_) => "mw:File/Thumb",
        }
    }
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
) -> Result<Options<'s>> {
    let mut options = Options::new(title);
    options.kind = if let Some((_, ext)) = options.title.key().rsplit_once('.') {
        // TODO: Get from config. API has siprops "fileextensions".
        match to_ascii_lower(ext).as_ref() {
            "mid" | "ogg" | "oga" | "flac" | "opus" | "wav" | "mp3" | "midi" => MediaKind::Audio,
            "ogv" | "webm" | "mpg" | "mpeg" => MediaKind::Video,
            _ => MediaKind::Image,
        }
    } else {
        MediaKind::Image
    };

    for argument in arguments {
        let value = sp
            .eval(state, argument.combined())?
            .map_ref(str::trim_ascii);
        let config = state.statics.db.config();
        let strip_markers = &state.strip_markers;
        let Some((value, arg)) = config.magic_word_matches(&value) else {
            options.caption = Some(argument.combined());
            continue;
        };

        if let Some(arg) = arg.map(str::trim_ascii) {
            if value.contains(&"img_alt") {
                options.alt = Some(to_attr(strip_markers, arg).into());
            } else if value.contains(&"img_class") {
                options.class = Some(to_attr(strip_markers, arg).into());
            } else if value.contains(&"img_lang") {
                options.lang = Some(arg.to_owned().into());
            } else if value.contains(&"img_link") {
                let arg = to_attr(strip_markers, arg);
                // TODO: This is supposed to do the equivalent of the `url_term`
                options.link = if config.protocols_pattern.is_match(&arg) {
                    Some(LinkKind::External(arg.into(), tags::ExternalLinkKind::Text))
                } else {
                    Title::new(config, &url_decode(&arg), None)
                        .ok()
                        .map(LinkKind::Internal)
                };
            } else if value.contains(&"img_lossy") {
                options.lossy = Some(arg != "false");
            } else if value.contains(&"img_manualthumb") {
                let title = Title::new(
                    config,
                    &url_decode(&to_attr(strip_markers, arg)),
                    Some(Namespace::FILE),
                )?;

                options.frame = Some(FrameKind::Thumb(Some(title)));
            } else if value.contains(&"img_page") {
                options.page = Some(arg.parse::<i32>().unwrap_or(1));
            } else if value.contains(&"img_upright") {
                options.upright = Some(arg.parse::<f64>().unwrap_or(1.0));
            } else if value.contains(&"img_width") {
                let (w, h) = arg.split_once('x').unwrap_or((arg, ""));
                if let Ok(w) = w.parse::<u32>() {
                    options.width = Some(w);
                }
                if let Ok(h) = h.parse::<u32>() {
                    options.height = Some(h);
                }
            } else if value.contains(&"timedmedia_disablecontrols") {
                options.controls = false;
            } else if value.contains(&"timedmedia_endtime") {
                options.end = Some(arg.to_owned().into());
            } else if value.contains(&"timedmedia_starttime") {
                options.start = Some(arg.to_owned().into());
            } else if value.contains(&"timedmedia_thumbtime") {
                options.thumbtime = Some(arg.to_owned().into());
            } else {
                log::warn!("unexpected magic word {value:?}");
            }
        } else if value.contains(&"img_border") {
            options.border = true;
        } else if value.contains(&"img_center") {
            options.align = Some("center");
        } else if value.contains(&"img_left") {
            options.align = Some("left");
        } else if value.contains(&"img_none") {
            options.align = Some("none");
        } else if value.contains(&"img_right") {
            options.align = Some("right");
        } else if value.contains(&"img_baseline") {
            options.valign = Some("baseline");
        } else if value.contains(&"img_middle") {
            options.valign = Some("middle");
        } else if value.contains(&"img_sub") {
            options.valign = Some("sub");
        } else if value.contains(&"img_super") {
            options.valign = Some("super");
        } else if value.contains(&"img_text_bottom") {
            options.valign = Some("text-bottom");
        } else if value.contains(&"img_text_top") {
            options.valign = Some("text-top");
        } else if value.contains(&"img_top") {
            options.valign = Some("top");
        } else if value.contains(&"img_framed") {
            options.frame = Some(FrameKind::Frame);
        } else if value.contains(&"img_frameless") {
            options.frame = Some(FrameKind::Frameless);
        } else if value.contains(&"img_thumbnail") {
            options.frame = Some(FrameKind::Thumb(None));
        } else if value.contains(&"img_upright") {
            options.upright = Some(0.75);
        } else if value.contains(&"timedmedia_muted") {
            options.muted = true;
        } else if value.contains(&"timedmedia_loop") {
            options.r#loop = true;
        } else {
            options.caption = Some(argument.combined());
        }
    }

    if let Some(caption) = options.caption
        && matches!(options.frame, None | Some(FrameKind::Frameless))
    {
        let value = sp.eval(state, caption)?;
        let value = to_attr(&state.strip_markers, &value);
        if options.alt.is_none() {
            options.alt = Some(value.clone().into());
        }
        options.link_title = Some(value.into());
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
    let options = media_options(state, sp, title, arguments)?;
    render_media_with_options(out, state, sp, &options)
}

/// Renders a media tag using the given media options.
pub(super) fn render_media_with_options(
    out: &mut Document,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    options: &Options<'_>,
) -> Result {
    fn emit_class<S: Sink + ?Sized>(next: &mut S, emitted: &mut bool) {
        if *emitted {
            next.text(" ");
        } else {
            next.tag_attribute_start("class");
            *emitted = true;
        }
    }

    // For now, they are all missing.
    const MISSING_IMAGE: bool = true;

    let tag_name = if options.align.is_some() {
        "figure"
    } else {
        "span"
    };

    out.next.tag_start(tag_name);

    let mut emitted = false;
    if options.width.is_none()
        && !matches!(
            options.frame,
            Some(FrameKind::Frame | FrameKind::Thumb(Some(..)))
        )
    {
        emit_class(&mut out.next, &mut emitted);
        out.next.text("mw-default-size");
    }
    if let Some(align) = options.align {
        emit_class(&mut out.next, &mut emitted);
        out.next.text("mw-halign-");
        out.next.text(align);
    }
    if let Some(valign) = options.valign {
        emit_class(&mut out.next, &mut emitted);
        out.next.text("mw-valign-");
        out.next.text(valign);
    }
    if options.border {
        emit_class(&mut out.next, &mut emitted);
        out.next.text("mw-image-border");
    }
    if let Some(class) = &options.class {
        emit_class(&mut out.next, &mut emitted);
        out.next.text(class);
    }
    if emitted {
        out.next.tag_attribute_end("class");
    }

    out.next.tag_attribute_start("typeof");
    if MISSING_IMAGE {
        out.next.text("mw:Error ");
    }
    let rdfa_kind = options.frame.as_ref().map_or("mw:File", |f| f.rdfa_kind());
    out.next.text(rdfa_kind);
    out.next.tag_attribute_end("typeof");
    out.next.tag_start_end(tag_name);

    if MISSING_IMAGE {
        // TODO: This link is supposed to go to special page "Upload" and have
        // title.text() as title attr
        super::tags::render_start_link(
            &mut out.next,
            state,
            &LinkKind::Internal(options.title.clone()),
        );
        out.next.tag_start("span");
        out.next
            .tag_attribute_full("class", "mw-file-element mw-broken-media");
        out.next.tag_start_end("span");
        if let Some(alt) = &options.alt {
            out.next.text(alt);
        } else {
            out.next.text(options.title.prefixed_text());
        }
        out.next.tag_end("span");
        out.next.tag_end("a");
    } else {
        match options.kind {
            MediaKind::Audio | MediaKind::Video => {
                render_timed_media(&mut out.next, state, options);
            }
            MediaKind::Image => {
                render_image(&mut out.next, state, options);
            }
        }
    }

    if options.align.is_some() {
        out.next.tag_start_full("figcaption");
        if let Some(caption) = options.caption {
            out.adopt_tokens(state, sp, caption)?;
        }
        out.next.tag_end("figcaption");
    }

    out.next.tag_end(tag_name);

    Ok(())
}

/// Renders an image tag.
fn render_image<S: Sink + ?Sized>(
    out: &mut S,
    state: &mut State<'_, '_, '_>,
    options: &Options<'_>,
) {
    let wrapper = if options.link.is_some() { "a" } else { "span" };

    if let Some(link) = &options.link {
        // TODO: This is wrong because it also needs a title attribute
        tags::render_start_link(out, state, link);
    } else {
        out.tag_start(wrapper);
        if let Some(title_attr) = &options.link_title {
            out.tag_attribute_full("title", title_attr);
        }
        out.tag_start_end(wrapper);
    }

    out.tag_start(options.kind.tag_name());

    let thumb = if let Some(FrameKind::Thumb(Some(title))) = &options.frame {
        title
    } else {
        &options.title
    };

    out.tag_attribute_full(
        "src",
        &make_media_url(
            &state.statics.base_uri,
            state.statics.paths.media,
            &thumb.text_url(),
        ),
    );

    if let Some(alt) = &options.alt {
        out.tag_attribute_full("alt", alt);
    }

    out.tag_attribute_full("decoding", "async");

    out.tag_attribute_start("class");
    out.text("mw-file-element");
    if options.upright.is_some() {
        out.text(" mw-file-upright");
    }
    if let Some(class) = &options.class {
        out.text(" ");
        out.text(class);
    }
    out.tag_attribute_end("class");

    if options.upright.is_some() || options.valign.is_some() {
        out.tag_attribute_start("style");
        if let Some(upright) = &options.upright {
            out.text("--mw-file-upright:");
            out.text(&upright.to_string());
            out.text(";");
        }
        if let Some(valign) = &options.valign {
            out.text("--wiki-rs-vertical-align:");
            out.text(valign);
            out.text(";");
        }
        out.tag_attribute_end("style");
    }

    // This is a void tag so there is no `tag_end`
    out.tag_start_end(options.kind.tag_name());

    out.tag_end(wrapper);
}

/// Renders an audio or video tag.
// TODO: This is even more bogus than the image tags; this does not even *use*
// most of the timed media options.
fn render_timed_media<S: Sink + ?Sized>(
    out: &mut S,
    state: &mut State<'_, '_, '_>,
    options: &Options<'_>,
) {
    out.tag_start(options.kind.tag_name());
    if options.controls {
        out.tag_attribute_full("controls", "");
    }
    if matches!(options.kind, MediaKind::Audio) {
        out.tag_attribute_full("height", "23");
        out.tag_attribute_full("width", &options.width.unwrap_or(300).min(35).to_string());
    } else {
        if let Some(height) = options.height {
            out.tag_attribute_full("height", &height.to_string());
        }
        if let Some(width) = options.width {
            out.tag_attribute_full("width", &width.to_string());
        }
    }
    if options.r#loop {
        out.tag_attribute_full("loop", "");
    }
    if options.muted {
        out.tag_attribute_full("muted", "");
    }
    if matches!(options.kind, MediaKind::Video)
        && let Some(FrameKind::Thumb(Some(title))) = &options.frame
    {
        let src = make_media_url(
            &state.statics.base_uri,
            state.statics.paths.media,
            &title.text_url(),
        );
        out.tag_attribute_full("poster", &src);
    }
    let src = make_media_url(
        &state.statics.base_uri,
        state.statics.paths.media,
        &options.title.text_url(),
    );
    out.tag_attribute_full("src", &src);
    out.tag_start_end(options.kind.tag_name());
    out.tag_end(options.kind.tag_name());
}

/// Converts an image argument into a form suitable for use in an HTML
/// attribute.
fn to_attr(strip_markers: &StripMarkers, arg: &str) -> String {
    use htmlparser::{ElementEnd, Token, Tokenizer};

    #[inline]
    fn spacelike(c: char) -> bool {
        c.is_ascii_whitespace()
    }

    const BLOCK: phf::Set<&str> = phf::phf_set! {
        "address", "article", "aside", "blockquote", "br", "canvas", "dd",
        "div", "dl", "dt", "fieldset", "figcaption", "figure", "footer", "form",
        "h1", "h2", "h3", "h4", "h5", "h6", "header", "hgroup", "hr", "li",
        "main", "nav","noscript", "ol", "output", "p", "pre", "section",
        "table", "td", "tfoot", "th", "tr", "ul", "video"
    };

    let arg = strip_markers.unstrip(arg);
    let mut out = String::new();
    let mut in_skip = 0;
    let mut is_block = false;
    let mut opened_skip = false;
    for token in Tokenizer::from(arg.as_ref()) {
        match token {
            Ok(Token::ElementStart { local, .. }) => {
                opened_skip = matches!(local.as_str(), "script" | "style");
                is_block = BLOCK.contains(local.as_str());
            }
            Ok(Token::ElementEnd { end, .. }) => match end {
                ElementEnd::Open => {
                    in_skip += u32::from(opened_skip);
                    if is_block {
                        out.push(' ');
                    }
                }
                ElementEnd::Close(.., local) => {
                    if matches!(local.as_str(), "script" | "style") {
                        in_skip = in_skip.saturating_sub(1);
                    } else if BLOCK.contains(local.as_str()) {
                        out.push(' ');
                    }
                }
                ElementEnd::Empty => {
                    if is_block {
                        out.push(' ');
                    }
                }
            },
            Ok(Token::Text { text } | Token::Cdata { text, .. }) if in_skip == 0 => {
                out += text.as_str();
            }
            _ => {}
        }
    }

    normalize_whitespace::<true>(&out, spacelike, spacelike).into_owned()
}

/// Returns `true` if the string appears to be an absolute URL.
// TODO: This is a hack for the test suite. At the least it should be using
// `config.protocols`.
#[inline]
fn is_absolute_url(str: &str) -> bool {
    str.starts_with("http://") || str.starts_with("https://")
}
