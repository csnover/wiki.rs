//! Code for handling MediaWiki images.

use super::{
    Result, StackFrame, State, StripMarker, Surrogate as _,
    document::{Document, DocumentSink},
    tags::{self, LinkKind, LinkKindOptions},
    transform::Sink,
};
use crate::transform::tokenise;
use core::fmt::Write as _;
use libmisc::CowExt as _;
use libwikitext_common::{
    db::FileMetadata,
    make_url,
    title::{Namespace, Title},
    url::Url,
    url_decode,
};
use libwikitext_parse::{Argument, Spanned, Token, borrow_fastest};
use std::borrow::Cow;
use uncased::UncasedStr;

/// Image dimensions.
type Dims = (u32, u32);

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

/// An image link strategy.
#[derive(Clone, Debug)]
pub(super) enum Link<'a> {
    /// Use the image title as the link.
    Inherit,
    /// Use no link.
    None,
    /// Use a custom link.
    Custom(LinkKind<'a>),
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
    /// The caption, in attribute form.
    caption_attr: Option<Cow<'a, str>>,
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
    /// The language to use when rendering an SVG with `<switch>` options
    /// varying on a `systemLanguage` attribute.
    lang: Option<Cow<'a, str>>,
    /// The target URL for an image link. This can be either a bare external URL
    /// or a bare article title.
    ///
    /// The output behaviour is different depending on whether there was no
    /// link (then it is a link to the file), empty link (then it is no link),
    /// or internal link (then a title attribute appears).
    link: Link<'a>,
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
            caption_attr: <_>::default(),
            class: <_>::default(),
            controls: true,
            end: <_>::default(),
            frame: <_>::default(),
            height: <_>::default(),
            lang: <_>::default(),
            link: Link::Inherit,
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

    /// The image alt text, or `None` if there is no alt text.
    fn alt(&self) -> Option<&str> {
        self.alt.as_deref().or({
            if matches!(self.frame, Some(FrameKind::Thumb(_) | FrameKind::Frame)) {
                None
            } else {
                self.caption_attr.as_deref()
            }
        })
    }

    /// Returns true if this image should have a `figcaption`.
    #[inline]
    fn is_captioned(&self) -> bool {
        self.align.is_some() || matches!(self.frame, Some(FrameKind::Thumb(_) | FrameKind::Frame))
    }

    /// Returns true if this image was not given any explicit dimensions.
    #[inline]
    fn is_default_size(&self) -> bool {
        self.width.is_none() && !self.is_unscaled()
    }

    /// Returns true if this image should link to an external URL.
    #[inline]
    fn is_external_link(&self) -> bool {
        matches!(self.link, Link::Custom(LinkKind::External(..)))
    }

    /// Returns true if this image is a “file description”. Whatever that is.
    #[inline]
    fn is_file_description(&self) -> bool {
        !matches!(self.link, Link::None | Link::Custom(_))
            && !matches!(self.frame, Some(FrameKind::Thumb(Some(_))))
    }

    /// Returns true if this image should link to the target.
    #[inline]
    fn is_link(&self) -> bool {
        !matches!(self.link, Link::None)
    }

    /// Returns true if this image should use thumbnail sizes and source sets.
    #[inline]
    fn is_thumb(&self) -> bool {
        matches!(
            self.frame,
            Some(FrameKind::Thumb(None) | FrameKind::Frameless)
        )
    }

    /// Returns true if this image should be displayed without scaling.
    #[inline]
    fn is_unscaled(&self) -> bool {
        matches!(
            self.frame,
            Some(FrameKind::Thumb(Some(..)) | FrameKind::Frame)
        )
    }

    /// Returns the link target for this image, or `None` if there is no link
    /// target.
    fn link(&self) -> Option<Cow<'_, LinkKind<'_>>> {
        match &self.link {
            Link::Inherit => Some(Cow::Owned(LinkKind::Internal(self.title.clone()))),
            Link::None => None,
            Link::Custom(link) => Some(Cow::Borrowed(link)),
        }
    }

    /// Returns the tooltip text for the image link, or `None` if there should
    /// not be a tooltip.
    fn link_title(&self) -> Option<&str> {
        if let Some(FrameKind::Thumb(None) | FrameKind::Frame) = &self.frame {
            None
        } else if let Some(FrameKind::Thumb(Some(_))) = &self.frame {
            match &self.link {
                Link::Custom(LinkKind::Internal(title)) => Some(title.prefixed_text()),
                Link::Inherit => Some(self.title.prefixed_text()),
                _ => None,
            }
        } else if let Link::Custom(LinkKind::Internal(title)) = &self.link {
            self.caption_attr.as_deref().or(Some(title.prefixed_text()))
        } else {
            self.caption_attr.as_deref()
        }
    }

    /// The title of the file to use for the thumbnail.
    fn thumb(&self) -> &Title {
        if let Some(FrameKind::Thumb(Some(thumb))) = &self.frame {
            thumb
        } else {
            &self.title
        }
    }
}

/// A [`DocumentSink`] which strips all tags from the input and normalises all
/// whitespace to a single space character.
#[derive(Debug)]
struct ParseAttr {
    /// The output.
    acc: String,
    /// If true, filtering out input.
    filtering: bool,
    /// If true, the next output should be preceded by a space character.
    needs_ws: bool,
}

impl DocumentSink for ParseAttr {
    const LIST_ITEMS: bool = true;
    const UNSTRIP_MARKERS: bool = true;

    type Args = ();

    fn new((): Self::Args) -> Self
    where
        Self: Sized,
    {
        Self {
            acc: <_>::default(),
            filtering: <_>::default(),
            needs_ws: <_>::default(),
        }
    }

    fn set_in_caption(&mut self, _: bool) {}
    fn set_in_list(&mut self, _: bool) {}
}

impl Sink for ParseAttr {
    #[inline]
    fn comment_end(&mut self) {}

    #[inline]
    fn comment_start(&mut self) {}

    #[inline]
    fn entity(&mut self, value: char, _: &str) {
        if self.filtering {
            return;
        }
        if self.needs_ws {
            self.acc.push(' ');
            self.needs_ws = false;
        }
        self.acc.push(value);
    }

    #[inline]
    fn finish(self) -> String {
        self.acc
    }

    #[inline]
    fn new_line(&mut self) {
        if !self.filtering {
            self.needs_ws = !self.acc.is_empty();
        }
    }

    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if self.filtering {
            return;
        }

        match marker {
            StripMarker::General(s) => tokenise(self, s),
            StripMarker::NoWiki(s) => self.text(s),
            _ => {}
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {}

    #[inline]
    fn tag_attribute_start(&mut self, _: &str) {}

    #[inline]
    fn tag_end(&mut self, name: &str) {
        if self.filtering {
            self.filtering = false;
        } else if BLOCK.contains(name.into()) {
            self.needs_ws = !self.acc.is_empty();
        }
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        if BLOCK.contains(name.into()) {
            self.needs_ws = !self.acc.is_empty();
        }
        self.filtering = true;
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        if name != UncasedStr::new("script") && name != UncasedStr::new("style") {
            self.filtering = false;
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.filtering {
            return;
        }

        self.needs_ws |=
            !self.acc.is_empty() && text.starts_with(|c: char| c.is_ascii_whitespace());
        for part in text.split_ascii_whitespace() {
            if self.needs_ws {
                self.acc.push(' ');
            }
            self.acc += part;
            self.needs_ws = true;
        }
        self.needs_ws = !self.acc.is_empty() && text.ends_with(|c: char| c.is_ascii_whitespace());
    }
}

/// Calculates the desired width and height for the image with the native
/// dimensions `native_width` and `native_height` for the preferred `width`
/// and optionally preferred `height`.
fn calc_image_dims(width: u32, height: Option<u32>, (native_width, native_height): Dims) -> Dims {
    if native_width == 0 || native_height == 0 {
        return (0, 0);
    }

    let width = height.map_or(width, |height| {
        let prefer_height = width * native_height > height * native_width;
        if prefer_height {
            let best_width = (native_width * height).div_ceil(native_height);
            if div_round(best_width * native_height, native_width) > height {
                native_width * height / native_height
            } else {
                best_width
            }
        } else {
            width
        }
    });

    (width, div_round(native_height * width, native_width))
}

/// Returns the default thumbnail size.
#[inline]
fn default_thumb_limit(state: &State<'_, '_, '_>) -> u32 {
    state
        .statics
        .db
        .config()
        .thumb_limits
        .first()
        .copied()
        .unwrap_or(180)
}

/// Divides `n` by `d`, rounding half toward positive infinity.
#[inline]
fn div_round(n: u32, d: u32) -> u32 {
    let q = n / d;
    let r = n % d;
    q + u32::from(r << 1 >= d)
}

/// Returns `true` if the string appears to be an absolute URL.
// TODO: This is a hack for the test suite. At the least it should be using
// `config.protocols`.
#[inline]
fn is_absolute_url(str: &str) -> bool {
    str.starts_with("http://") || str.starts_with("https://")
}

/// Creates a URL to a title in the [media namespace].
///
/// [media namespace]: libwikitext_common::title::Namespace::MEDIA
pub(super) fn make_media_url(base_uri: &Url, media_path: &str, text: &str) -> String {
    use std::io::Write as _;
    let mut prefix = [b'0'; 2];
    let b = md5::compute(text.as_bytes())[0];
    let _ = write!(&mut prefix[..], "{b:02x}");
    // SAFETY: Guaranteed to be ASCII.
    let prefix = unsafe { str::from_utf8_unchecked(&prefix[..]) };
    let path = format_args!("{media_path}/{}/{}/{text}", &prefix[..1], &prefix);
    if is_absolute_url(media_path) {
        path.to_string()
    } else {
        make_url(base_uri, None, path, None, None)
    }
}

/// Generates HTML `src` and `srcset` attribute values for an image with the
/// given `max_width` and `width`, base filename `base_name`, and original scale
/// URL `src`.
fn make_srcset(
    state: &mut State<'_, '_, '_>,
    base_width: u32,
    preferred_width: u32,
    height: Option<u32>,
    native_dims @ (max_width, _): Dims,
    base_name: &str,
    src: &str,
) -> (String, Option<String>) {
    let thumb_src = make_media_url(
        &state.statics.base_uri,
        &format!("{}/thumb", state.statics.paths.media),
        base_name,
    );
    let mut srcset = String::new();
    for (mult, mult_name) in [(15, "1.5"), (20, "2")] {
        // To match the rounding of the original parser it is necessary to
        // recalculate the image width for each multiple rather than doing the
        // fast and easy thing of multiplying the first calculated base image
        // width
        let (size, _) = calc_image_dims(
            preferred_width * mult,
            height.map(|h| h * mult),
            native_dims,
        );
        let size = div_round(size, 10);
        if !srcset.is_empty() {
            srcset += ", ";
        }

        if size >= max_width {
            let _ = write!(srcset, "{src} {mult_name}x");
            break;
        }

        let _ = write!(srcset, "{thumb_src}/{size}px-{base_name} {mult_name}x");
    }

    let src = format!("{thumb_src}/{base_width}px-{base_name}");
    (src, (!srcset.is_empty()).then_some(srcset))
}

/// Parses [`Options`] from a media node.
pub(super) fn media_options<'s>(
    state: &mut State<'_, '_, '_>,
    sp: &'s StackFrame<'s>,
    title: Title,
    arguments: &'s [Spanned<Argument>],
) -> Result<Options<'s>> {
    let mut options = Options::new(title);

    for argument in arguments {
        let raw = sp
            .eval(state, argument.combined())?
            .map_ref(str::trim_ascii);
        let result = state.statics.db.config().magic_word_matches(raw);
        if let Ok((value, arg)) = result {
            if let Some(arg) = arg.map(|arg| arg.map_ref(str::trim_ascii)) {
                option_arg(state, sp, &mut options, argument, value, arg)?;
            } else {
                option_flag(state, sp, &mut options, argument, value)?;
            }
        } else {
            option_caption(state, sp, &mut options, argument)?;
        }
    }

    Ok(options)
}

/// Gets the HTML tag associated with a kind of media.
#[inline]
fn media_tag_name(meta: FileMetadata) -> &'static str {
    match meta {
        FileMetadata::Audio => "audio",
        FileMetadata::Image { .. } => "img",
        FileMetadata::Video { .. } => "video",
    }
}

/// Set an option on `options` from the given parameterised `arg` which matched
/// one of the magic words given in `value`.
fn option_arg<'s>(
    state: &mut State<'_, '_, '_>,
    sp: &'s StackFrame<'s>,
    options: &mut Options<'s>,
    raw_value: &'s Spanned<Argument>,
    key: &[&str],
    arg: Cow<'s, str>,
) -> Result {
    if key.contains(&"img_alt") {
        options.alt = Some(to_attr(state, sp, raw_value.value())?);
    } else if key.contains(&"img_class") {
        options.class = Some(to_attr(state, sp, raw_value.value())?);
    } else if key.contains(&"img_lang") {
        if icu_locale::LanguageIdentifier::try_from_str(&arg).is_ok() {
            options.lang = Some(arg);
        } else {
            option_caption(state, sp, options, raw_value)?;
        }
    } else if key.contains(&"img_link") {
        let arg = to_attr(state, sp, raw_value.value())?;
        let config = &state.statics.db.config();
        options.link = if arg.is_empty() {
            Link::None
        } else if config.protocols_pattern.is_match(&arg) {
            // TODO: This is supposed to match like the `url_term` grammar
            // rule
            Link::Custom(LinkKind::External(arg, tags::ExternalLinkKind::Text))
        } else {
            Title::new(config, &url_decode(&arg), None)
                .map_or(Link::None, |title| Link::Custom(LinkKind::Internal(title)))
        };
    } else if key.contains(&"img_lossy") {
        options.lossy = Some(arg != "false");
    } else if key.contains(&"img_manualthumb") {
        if options.frame.is_none() {
            let title = to_attr(state, sp, raw_value.value())?;
            let title = Title::new(state.statics.db.config(), &title, Some(Namespace::FILE))?;
            options.frame = Some(FrameKind::Thumb(Some(title)));
        }
    } else if key.contains(&"img_page") {
        options.page = Some(arg.parse::<i32>().unwrap_or(1));
    } else if key.contains(&"img_upright") {
        options.upright = Some(arg.parse::<f64>().unwrap_or(1.0));
    } else if key.contains(&"img_width") {
        if arg.ends_with("px") {
            state
                .globals
                .categories
                .tracking(&state.statics.messages, "double-px-category")?;
        }

        let (width, height) = parse_dims(&arg);
        if width.is_some() {
            options.width = width;
        }
        if height.is_some() {
            options.height = height;
        }
    } else if key.contains(&"timedmedia_disablecontrols") {
        options.controls = false;
    } else if key.contains(&"timedmedia_endtime") {
        options.end = Some(arg);
    } else if key.contains(&"timedmedia_starttime") {
        options.start = Some(arg);
    } else if key.contains(&"timedmedia_thumbtime") {
        options.thumbtime = Some(arg);
    } else {
        log::warn!("unexpected magic word {key:?}");
    }
    Ok(())
}

/// Sets the caption on `options` from the given `argument`.
fn option_caption<'s>(
    state: &mut State<'_, '_, '_>,
    sp: &'s StackFrame<'s>,
    options: &mut Options<'s>,
    argument: &'s Spanned<Argument>,
) -> Result {
    options.caption = Some(argument.combined());
    options.caption_attr = Some(to_attr(state, sp, argument.combined())?);
    Ok(())
}

/// Sets an option on `options` from the given flaglike `argument` which matched
/// one of the magic words given in `value`.
fn option_flag<'s>(
    state: &mut State<'_, '_, '_>,
    sp: &'s StackFrame<'s>,
    options: &mut Options<'s>,
    argument: &'s Spanned<Argument>,
    value: &[&str],
) -> Result {
    if value.contains(&"img_border") {
        options.border = true;
    } else if value.contains(&"img_center") {
        options.align.get_or_insert("center");
    } else if value.contains(&"img_left") {
        options.align.get_or_insert("left");
    } else if value.contains(&"img_none") {
        options.align.get_or_insert("none");
    } else if value.contains(&"img_right") {
        options.align.get_or_insert("right");
    } else if value.contains(&"img_baseline") {
        options.valign.get_or_insert("baseline");
    } else if value.contains(&"img_bottom") {
        options.valign.get_or_insert("bottom");
    } else if value.contains(&"img_middle") {
        options.valign.get_or_insert("middle");
    } else if value.contains(&"img_sub") {
        options.valign.get_or_insert("sub");
    } else if value.contains(&"img_super") {
        options.valign.get_or_insert("super");
    } else if value.contains(&"img_text_bottom") {
        options.valign.get_or_insert("text-bottom");
    } else if value.contains(&"img_text_top") {
        options.valign.get_or_insert("text-top");
    } else if value.contains(&"img_top") {
        options.valign.get_or_insert("top");
    } else if value.contains(&"img_framed") {
        options.frame.get_or_insert(FrameKind::Frame);
    } else if value.contains(&"img_frameless") {
        options.frame.get_or_insert(FrameKind::Frameless);
    } else if value.contains(&"img_thumbnail") {
        options.frame.get_or_insert(FrameKind::Thumb(None));
    } else if value.contains(&"img_upright") {
        options.upright = Some(0.75);
    } else if value.contains(&"timedmedia_muted") {
        options.muted = true;
    } else if value.contains(&"timedmedia_loop") {
        options.r#loop = true;
    } else {
        // Maybe some other non-image magic word alias got in there
        option_caption(state, sp, options, argument)?;
    }
    Ok(())
}

/// Parses an `img_width` option into width and height.
pub(super) fn parse_dims(arg: &str) -> (Option<u32>, Option<u32>) {
    let arg = arg.strip_suffix("px").unwrap_or(arg);
    let (w, h) = arg.split_once('x').unwrap_or((arg, ""));
    (w.parse::<u32>().ok(), h.parse::<u32>().ok())
}

/// Renders an image tag to `out` using `state` with the given `options` and
/// native dimensions `native_height` and `native_width`.
fn render_image<S: Sink + ?Sized>(
    out: &mut S,
    state: &mut State<'_, '_, '_>,
    options: &Options<'_>,
    native_dims: Dims,
    scalable: bool,
) {
    let wrapper = if options.is_link() { "a" } else { "span" };
    let native_width = native_dims.0;
    let preferred_width = options.width.map_or_else(
        || {
            if options.is_thumb() {
                default_thumb_limit(state).min(native_width)
            } else {
                native_width
            }
        },
        |width| {
            if scalable || options.frame.is_none() {
                width
            } else {
                width.min(native_width)
            }
        },
    );
    let (width, height) = if options.is_unscaled() {
        native_dims
    } else {
        calc_image_dims(preferred_width, options.height, native_dims)
    };

    out.tag_start(wrapper);
    if let Some(link) = options.link() {
        let href = resource_url(state, &link);
        let href = if matches!(options.link, Link::Inherit) {
            href.split_once('#').map_or(href.as_str(), |(href, _)| href)
        } else {
            &href
        };
        out.tag_attribute_full("href", href);
        if options.is_external_link() {
            out.tag_attribute_full("rel", "nofollow");
        } else if options.is_file_description() {
            out.tag_attribute_full("class", "mw-file-description");
        }
    }
    if let Some(title_attr) = &options.link_title() {
        out.tag_attribute_full("title", title_attr);
    }
    out.tag_start_end(wrapper);

    out.tag_start("img");

    let thumb = options.thumb();
    let base_name = thumb.text_url();
    let src = make_media_url(
        &state.statics.base_uri,
        state.statics.paths.media,
        &base_name,
    );
    let (src, srcset) = if !scalable && width < native_width {
        make_srcset(
            state,
            width,
            preferred_width,
            options.height,
            native_dims,
            &base_name,
            &src,
        )
    } else {
        (src, None)
    };

    if let Some(alt) = &options.alt() {
        out.tag_attribute_full("alt", alt);
    }
    if matches!(options.frame, Some(FrameKind::Thumb(Some(_)))) {
        let href = resource_url(state, &LinkKind::Internal(options.title.clone()));
        out.tag_attribute_full("resource", &href);
    }
    out.tag_attribute_full("src", &src);
    out.tag_attribute_full("decoding", "async");
    out.tag_attribute_full("width", &width.to_string());
    out.tag_attribute_full("height", &height.to_string());

    out.tag_attribute_start("class");
    out.text("mw-file-element");
    if options.upright.is_some() {
        out.text(" mw-file-upright");
    }
    out.tag_attribute_end("class");

    if let Some(srcset) = srcset {
        out.tag_attribute_full("srcset", &srcset);
    }

    if let Some(upright) = &options.upright {
        out.tag_attribute_start("style");
        out.text("--mw-file-upright:");
        out.text(&upright.to_string());
        out.text(";");
        out.tag_attribute_end("style");
    }

    // This is a void tag so there is no `tag_end`
    out.tag_start_end("img");

    out.tag_end(wrapper);
}

/// Renders a media tag.
pub(super) fn render_media<S: DocumentSink>(
    out: &mut Document<S>,
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    title: Title,
    arguments: &[Spanned<Argument>],
) -> Result {
    let options = media_options(state, sp, title, arguments)?;
    render_media_with_options(out, state, sp, &options)
}

/// Renders a media tag using the given media options.
pub(super) fn render_media_with_options<S: DocumentSink>(
    out: &mut Document<S>,
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

    let file = state.statics.db.metadata(&options.title)?;
    let thumb = state.statics.db.metadata(options.thumb())?.or(file);

    let tag_name = if options.is_captioned() {
        "figure"
    } else {
        "span"
    };

    out.next.tag_start(tag_name);

    let mut emitted = false;
    if options.is_default_size() {
        emit_class(&mut out.next, &mut emitted);
        out.next.text("mw-default-size");
    }
    if let Some(align) = options.align {
        emit_class(&mut out.next, &mut emitted);
        out.next.text("mw-halign-");
        out.next.text(align);
    } else if let Some(valign) = options.valign {
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
    if thumb.is_none() {
        out.next.text("mw:Error ");
    }
    let rdfa_kind = options.frame.as_ref().map_or("mw:File", |f| f.rdfa_kind());
    out.next.text(rdfa_kind);
    out.next.tag_attribute_end("typeof");
    out.next.tag_start_end(tag_name);

    if let Some(file) = file {
        match file {
            FileMetadata::Audio | FileMetadata::Video { .. } => {
                render_timed_media(&mut out.next, state, options, file);
            }
            FileMetadata::Image { .. } => {
                // TODO: Image with video thumb must use thumb of video
                let (width, height, scalable) = match thumb {
                    Some(FileMetadata::Image {
                        height,
                        scalable,
                        width,
                    }) => (width, height, scalable),
                    Some(FileMetadata::Video { height, width }) => (width, height, false),
                    _ => panic!("should have an image or video thumb"),
                };
                render_image(&mut out.next, state, options, (width, height), scalable);
            }
        }
    } else {
        super::tags::render_start_link(
            &mut out.next,
            state,
            &LinkKind::Internal(options.title.clone()),
            true,
        );
        out.next.tag_start("span");
        out.next
            .tag_attribute_full("class", "mw-file-element mw-broken-media");
        out.next.tag_start_end("span");
        if let Some(alt) = &options.alt() {
            out.next.text(alt);
        } else {
            out.next.text(options.title.prefixed_text());
        }
        out.next.tag_end("span");
        out.next.tag_end("a");
    }

    if options.is_captioned() {
        out.next.tag_start_full("figcaption");
        if let Some(caption) = options.caption {
            out.adopt_tokens(state, sp, caption)?;
        }
        out.next.tag_end("figcaption");
    }

    out.next.tag_end(tag_name);

    Ok(())
}

/// Renders an audio or video tag.
// TODO: This is even more bogus than the image tags; this does not even *use*
// most of the timed media options.
fn render_timed_media<S: Sink + ?Sized>(
    out: &mut S,
    state: &mut State<'_, '_, '_>,
    options: &Options<'_>,
    media: FileMetadata,
) {
    out.tag_start(media_tag_name(media));
    if options.controls {
        out.tag_attribute_full("controls", "");
    }
    if matches!(media, FileMetadata::Audio) {
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
    if matches!(media, FileMetadata::Video { .. })
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
    out.tag_start_end(media_tag_name(media));
    out.tag_end(media_tag_name(media));
}

/// Creates a resource URL for the image target.
fn resource_url(state: &mut State<'_, '_, '_>, link: &LinkKind<'_>) -> String {
    link.to_string(
        &LinkKindOptions {
            base_uri: &state.statics.base_uri,
            interwiki_map: &state.statics.db.config().interwiki_map,
            paths: &state.statics.paths,
        },
        None,
    )
}

/// Converts an image argument into a form suitable for use in an HTML
/// attribute.
fn to_attr<'a>(
    state: &mut State<'_, '_, '_>,
    sp: &'a StackFrame<'_>,
    arg: &'a [Spanned<Token>],
) -> Result<Cow<'a, str>> {
    Ok(if let Some(text) = borrow_fastest(&sp.source, arg) {
        Cow::Borrowed(text)
    } else {
        let mut document = Document::<ParseAttr>::new(());
        document.adopt_tokens(state, sp, arg)?;
        Cow::Owned(document.finish())
    })
}

/// Elements which should be replaced by whitespace in an image attribute.
const BLOCK: phf::Set<&UncasedStr> = phf::phf_set! {
    UncasedStr::new("address"), UncasedStr::new("article"), UncasedStr::new("aside"),
    UncasedStr::new("blockquote"), UncasedStr::new("br"),
    UncasedStr::new("canvas"),
    UncasedStr::new("dd"), UncasedStr::new("div"), UncasedStr::new("dl"), UncasedStr::new("dt"),
    UncasedStr::new("fieldset"), UncasedStr::new("figcaption"), UncasedStr::new("figure"), UncasedStr::new("footer"), UncasedStr::new("form"),
    UncasedStr::new("h1"), UncasedStr::new("h2"), UncasedStr::new("h3"), UncasedStr::new("h4"), UncasedStr::new("h5"), UncasedStr::new("h6"), UncasedStr::new("header"),
    UncasedStr::new("hgroup"), UncasedStr::new("hr"),
    UncasedStr::new("li"),
    UncasedStr::new("main"),
    UncasedStr::new("nav"), UncasedStr::new("noscript"),
    UncasedStr::new("ol"), UncasedStr::new("output"),
    UncasedStr::new("p"), UncasedStr::new("pre"),
    UncasedStr::new("section"),
    UncasedStr::new("table"), UncasedStr::new("td"), UncasedStr::new("tfoot"), UncasedStr::new("th"), UncasedStr::new("tr"),
    UncasedStr::new("ul"),
    UncasedStr::new("video")
};
