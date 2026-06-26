//! HTML emitters for Wikitext fragments that require state management.

use super::{StripMarker, globals::Outline, tags::PHRASING_TAGS};
use core::{
    fmt,
    num::{NonZeroU8, NonZeroU32},
};
use html_escape::encode_double_quoted_attribute;
use html5gum::emitters::callback::{CallbackEmitter, CallbackEvent};
use indexmap::{IndexMap, IndexSet};
use libmisc::CowExt as _;
use libwikitext_common::{
    AnchorEncodeMode, decode_html, escape_id, normalize_section_name, title::normalize_fragment,
};
use libwikitext_parse::{HeadingLevel, TextStyle, VOID_TAGS};
use regex::Regex;
use std::{borrow::Cow, collections::HashSet, sync::LazyLock};
use uncased::{Uncased, UncasedStr};

/// An intermediate sink.
pub(super) trait Chain: Sink {
    /// The type of the next sink in the chain.
    type Next;

    /// Returns a reference to the next sink in the chain.
    fn next(&self) -> &Self::Next;

    /// Returns a mutable reference to the next sink in the chain.
    fn next_mut(&mut self) -> &mut Self::Next;
}

/// A back-propagating bookmarker of output positions. Used to inject additional
/// unstructured HTML without buffering.
pub(super) trait Markable {
    /// Frees the given `mark` for reuse. This is a performance optimisation.
    fn free_mark(&mut self, mark: Mark);

    /// Mark the current output position for later investigation.
    fn mark(&mut self) -> Mark;

    /// Runs the callback `f` with the resolved positions for the given `marks`
    /// and a mutable reference to the corresponding `MarkableString`.
    fn with_marks<const N: usize, F: FnOnce([Option<usize>; N], &mut MarkableString) -> T, T>(
        &mut self,
        marks: [&Mark; N],
        f: F,
    ) -> T;
}

impl<T> Markable for T
where
    T: Chain,
    T::Next: Markable,
{
    #[inline]
    fn free_mark(&mut self, mark: Mark) {
        self.next_mut().free_mark(mark);
    }

    #[inline]
    fn mark(&mut self) -> Mark {
        self.next_mut().mark()
    }

    #[inline]
    fn with_marks<const N: usize, F: FnOnce([Option<usize>; N], &mut MarkableString) -> U, U>(
        &mut self,
        marks: [&Mark; N],
        f: F,
    ) -> U {
        self.next_mut().with_marks(marks, f)
    }
}

/// A streaming node sink.
pub(super) trait Sink {
    /// Ends a comment.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                                   ^^^^
    /// ```
    fn comment_end(&mut self);

    /// Starts a comment.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                              ^^^^^
    /// ```
    fn comment_start(&mut self);

    /// A character entity.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                       ^^^^^^^
    /// ```
    fn entity(&mut self, value: char, raw: &str);

    /// Finish processing input.
    fn finish(self) -> String;

    /// A source newline.
    ///
    /// This is used for source-line-sensitive rules.
    fn new_line(&mut self);

    /// Writes strip marker content.
    fn strip_marker(&mut self, marker: &StripMarker);

    /// End a tag attribute with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                 ^
    /// ```
    fn tag_attribute_end(&mut self, name: &str);

    /// Emits a whole tag attribute with the given `name` and `value`.
    fn tag_attribute_full(&mut self, name: &str, value: &str) {
        self.tag_attribute_start(name);
        self.text(value);
        self.tag_attribute_end(name);
    }

    /// Start a tag attribute with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///     ^^^^^^^
    /// ```
    fn tag_attribute_start(&mut self, name: &str);

    /// Ends a node with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                                              ^^^^^^
    /// ```
    fn tag_end(&mut self, name: &str);

    /// Start a tag with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    /// ^^^^
    /// ```
    fn tag_start(&mut self, name: &str);

    /// Ends a start tag with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                  ^
    /// ```
    fn tag_start_end(&mut self, name: &str);

    /// Starts a tag with the given `name` and no attributes.
    fn tag_start_full(&mut self, name: &str) {
        self.tag_start(name);
        self.tag_start_end(name);
    }

    /// Text content.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///            ^^^^^  ^^^^            ^^^^^^^
    /// ```
    fn text(&mut self, text: &str);
}

/// Final accumulator for HTML.
///
/// This sink assumes that previous stages will have done all the necessary
/// work of ensuring that the DOM is as well-formed as it needs to be. This
/// sink should receive balanced tags and balanced attributes, and does nothing
/// other than concatenate calls into a string of HTML.
#[derive(Debug, Default)]
pub(super) struct Accumulator {
    /// The target string buffer.
    inner: MarkableString,
    /// If true, the accumulator has received a `tag_attribute_start` and is
    /// waiting for a `tag_attribute_end`.
    in_attr: bool,
}

impl Accumulator {
    /// Creates a new `Accumulator`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Extracts a string slice containing the entire accumulator.
    #[inline]
    fn as_str(&self) -> &str {
        &self.inner.inner
    }

    /// Returns the length of the accumulator string, in bytes.
    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Wikitext ≥2**32 is impossible"
    )]
    fn len(&self) -> u32 {
        self.inner.inner.len() as u32
    }

    /// Truncates the accumulator at `pos`.
    #[inline]
    fn truncate(&mut self, pos: u32) {
        self.inner.inner.truncate(pos as usize);
    }
}

impl Markable for Accumulator {
    #[inline]
    fn free_mark(&mut self, mark: Mark) {
        self.inner.free_mark(mark);
    }

    #[inline]
    fn mark(&mut self) -> Mark {
        self.inner.mark()
    }

    #[inline]
    fn with_marks<const N: usize, F: FnOnce([Option<usize>; N], &mut MarkableString) -> T, T>(
        &mut self,
        marks: [&Mark; N],
        f: F,
    ) -> T {
        let positions = marks.map(|mark| self.inner.restore_mark(mark));
        f(positions, &mut self.inner)
    }
}

impl Sink for Accumulator {
    #[inline]
    fn comment_end(&mut self) {
        self.inner.push_str("-->");
    }

    #[inline]
    fn comment_start(&mut self) {
        self.inner.push_str("<!--");
    }

    fn entity(&mut self, value: char, raw: &str) {
        if matches!(value, '<' | '>' | '&') || (self.in_attr && value == '"') {
            self.inner.push_str(raw);
        } else {
            self.inner.push(value);
        }
    }

    #[inline]
    fn finish(self) -> String {
        self.inner.into_inner()
    }

    #[inline]
    fn new_line(&mut self) {
        self.inner.push('\n');
    }

    #[inline]
    fn strip_marker(&mut self, _: &StripMarker) {
        panic!("strip markers should be decomposed before now");
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {
        self.inner.push('"');
        self.in_attr = false;
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.inner.push(' ');
        self.inner.push_str(name);
        // MediaWiki always emits double-quoted attributes, even if there is
        // no value, and since this is exposed in CSS, it matters
        self.inner.push_str(r#"=""#);
        self.in_attr = true;
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.inner.push_str("</");
        self.inner.push_str(name);
        self.inner.push_str(">");
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.inner.push('<');
        self.inner.push_str(name);
    }

    #[inline]
    fn tag_start_end(&mut self, _: &str) {
        self.inner.push('>');
    }

    #[inline]
    fn text(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '<' => self.inner.push_str("&lt;"),
                '"' if self.in_attr => self.inner.push_str("&quot;"),
                '&' => self.inner.push_str("&amp;"),
                c => self.inner.push(c),
            }
        }
    }
}

/// Deduplicates and filters invalid HTML attributes using the last-wins rule.
#[derive(Debug)]
pub(super) struct AttributeFilter<S: Sink + Markable> {
    /// The attribute accumulator.
    acc: IndexMap<Cow<'static, str>, String>,
    /// The list of allowed attribute names for the currently processing tag.
    allowed: &'static [&'static phf::Set<&'static str>],
    /// The output.
    next: S,
    /// The current state of the filter.
    state: FilterState,
}

/// The [`AttributeFilter`] state.
#[derive(Debug, Default, Eq, PartialEq)]
enum FilterState {
    /// Inactive.
    #[default]
    Idle,
    /// Buffering an attribute at the given index in [`AttributeFilter::acc`].
    Buffering(usize),
    /// Filtering an attribute.
    Filtering,
}

chainable!(AttributeFilter);

impl<S: Sink + Markable> AttributeFilter<S> {
    /// Creates a new `AttributeFilter` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            acc: <_>::default(),
            allowed: <_>::default(),
            next,
            state: <_>::default(),
        }
    }

    /// Fixes up a list of IDs.
    fn fixup_aria(next: &mut S, ids: &str) {
        let mut first = true;
        for id in ids.split_ascii_whitespace() {
            if first {
                first = false;
            } else {
                next.text(" ");
            }
            next.text(&escape_id(id, AnchorEncodeMode::Html5));
        }
    }

    /// Fixes up a `class` attribute.
    fn fixup_class(next: &mut S, classes: &str) {
        let mut seen = HashSet::new();
        for class in classes.split_ascii_whitespace() {
            if seen.insert(class) {
                if seen.len() != 1 {
                    next.text(" ");
                }
                next.text(class);
            }
        }
    }

    /// Fixes up a `style` attribute.
    fn fixup_style(next: &mut S, style: &str) {
        static RE_UHOH: LazyLock<Regex> = LazyLock::new(|| {
            const UHOH: &str = concat!(
                "expression",
                r"|accelerator\s*:",
                r"|-o-(?:link(?:-source)?|replace)\s*:",
                r"|(?:url|src|image|image-set)\s*\(",
                r"|attr\s*\([^)]+[\s,]+url"
            );
            Regex::new(UHOH).unwrap()
        });

        // TODO: Must decode CSS escapes.
        // TODO: `^\s*/\*[^*\\/]*\*/\s*$` should do nothing
        // TODO: Replace CSS comments by a single space character
        // TODO: Discard anything after "/*"

        if style.contains(
            |c| matches!(c, '\0'..='\x08' | '\x0b' | '\x0e'..='\x1f' | '\x7f' | char::REPLACEMENT_CHARACTER),
        ) {
            next.text("/* invalid control char */");
            return;
        } else if RE_UHOH.is_match(style) {
            next.text("/* insecure input */");
            return;
        }

        let mut style = style.trim_ascii();
        while !style.is_empty() {
            // 'Template:Table cell templates' contains a bunch of invalid
            // garbage. When this happens, just try skipping to the next
            // possibly valid declaration.
            if let Ok((decl, rest)) = barely_css::decl(style) {
                style = &style[rest..];
                if let Some((name, value)) = decl {
                    if name.starts_with("--") {
                        next.text(name);
                        next.text(":");
                        next.text(value);
                        next.text(";");
                    } else {
                        next.text("--mw-output-");
                        next.text(name);
                        next.text(":");
                        next.text(value);
                        next.text(";");
                    }
                }
            } else if let Some(rest) = style.find(';') {
                next.text(&style[..=rest]);
                style = &style[rest + 1..];
            } else {
                next.text(style);
                break;
            }
        }
    }

    /// Returns true if the given attribute `name` is allowed.
    fn is_allowed(&self, name: &str) -> Option<Cow<'static, str>> {
        if let Some(name) = self
            .allowed
            .iter()
            .find_map(|allowed| allowed.get_key(name))
        {
            Some(Cow::Borrowed(*name))
        } else {
            // Technically, these are supposed to be only allowed for things
            // which allow the common attributes, but HTML5 does not care, and
            // maybe our own data attributes want to go somewhere unexpected, so
            // it does not matter
            if let Some(suffix) = name.strip_prefix("data-") {
                (!suffix.starts_with("mw")
                    && !suffix.starts_with("ooui")
                    && !suffix.starts_with("parsoid")
                    && !suffix.contains([':', '=', ' ', '\t', '\r', '\n', '/', '>', '\0']))
                .then(|| name.to_owned().into())
            } else if let Some(suffix) = name.strip_prefix("xmlns:") {
                (!suffix.is_empty()).then(|| name.to_owned().into())
            } else {
                None
            }
        }
    }
}

impl<S: Sink + Markable> Sink for AttributeFilter<S> {
    #[inline]
    fn comment_end(&mut self) {
        if self.state == FilterState::Idle {
            self.next.comment_end();
        }
    }

    #[inline]
    fn comment_start(&mut self) {
        if self.state == FilterState::Idle {
            self.next.comment_start();
        }
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        match self.state {
            FilterState::Idle => self.next.entity(value, raw),
            FilterState::Buffering(index) => self.acc[index].push(value),
            FilterState::Filtering => {}
        }
    }

    #[inline]
    fn finish(self) -> String {
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.state == FilterState::Idle {
            self.next.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        if self.state == FilterState::Idle {
            self.next.strip_marker(marker);
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {
        self.state = FilterState::Idle;
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if let Some(name) = self.is_allowed(name) {
            let entry = self.acc.entry(name);
            let index = entry.index();
            entry.or_default().clear();
            self.state = FilterState::Buffering(index);
        } else {
            self.state = FilterState::Filtering;
        }
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.allowed = ALLOWED_ATTRS.get(name).copied().unwrap_or_default();
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.allowed = <_>::default();

        let has_itemscope = self.acc.contains_key("itemscope");
        for (name, value) in self.acc.drain(..) {
            if name == "tabindex" && value != "0" {
                continue;
            }
            // TODO: Figure out how to get a config in here without blowing up
            // borrowck
            // if matches!(name.as_ref(), "href" | "poster" | "src")
            //     && !self.config.protocols_pattern.is_match(&value) {
            //     continue;
            // }
            if name == "id" && value.is_empty() {
                continue;
            }
            if !has_itemscope && matches!(name.as_ref(), "itemtype" | "itemid" | "itemref") {
                continue;
            }

            self.next.tag_attribute_start(&name);
            match name.as_ref() {
                "aria-describedby" | "aria-flowto" | "aria-labelledby" | "aria-owns" => {
                    Self::fixup_aria(&mut self.next, &value);
                }
                "class" => Self::fixup_class(&mut self.next, &value),
                "id" => self.next.text(&escape_id(&value, AnchorEncodeMode::Html5)),
                "style" => Self::fixup_style(&mut self.next, &value),
                _ => self.next.text(&value),
            }
            self.next.tag_attribute_end(&name);
        }

        self.next.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        match self.state {
            FilterState::Idle => self.next.text(text),
            FilterState::Buffering(index) => self.acc[index] += text,
            FilterState::Filtering => {}
        }
    }
}

/// The allowed list of attributes for the given tags.
static ALLOWED_ATTRS: phf::Map<&str, &[&phf::Set<&str>]> = phf::phf_map! {
    "a" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "href", "rel", "rev" }
    ],
    "audio" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "controls", "height", "preload", "width" }
    ],
    "abbr" | "aside" | "b" | "bdi" | "bdo" | "big" | "center" | "cite" | "code"
    | "dd" | "dfn" | "dl" | "dt" | "em" | "figcaption" | "figure" | "i" | "kbd"
    | "mark" | "rb" | "rp" | "rt" | "rtc" | "ruby" | "s" | "samp" | "small"
    | "span" | "strike" | "strong" | "sub" | "sup" | "tbody" | "tfoot"
    | "thead" | "tt" | "u" | "var" | "wbr"
    => &[
        &COMMON_ATTRS
    ],
    "blockquote" | "q" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "cite" }
    ],
    "br" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "clear" }
    ],
    "caption" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "align" }
    ],
    "col" | "colgroup" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "span" }
    ],
    "data" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "value" }
    ],
    "font" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "color", "face", "size" }
    ],
    "hr" | "pre" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "width" }
    ],
    "img" => &[
        &COMMON_ATTRS,
        // For some reason, decoding is not in the Wikitext list, but it is
        // allowed
        &phf::phf_set! { "alt", "decoding", "height", "src", "srcset", "width" }
    ],
    "ins" | "del" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "cite", "datetime" }
    ],
    "li" => &[
        &COMMON_ATTRS, &phf::phf_set! { "type", "value" }
    ],
    "link" => &[
        &phf::phf_set! { "href", "itemprop", "title" }
    ],
    "math" => &[
        &phf::phf_set! { "class", "id", "style", "title" }
    ],
    "meta" => &[
        &phf::phf_set! { "content", "itemprop" }
    ],
    "ol" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "reversed", "start", "type" }
    ],
    "ul" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "type" }
    ],
    "source" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "src", "type" }
    ],
    "table" => &[
        &COMMON_ATTRS,
        &phf::phf_set! {
            "align", "bgcolor", "border", "cellpadding", "cellspacing", "frame",
            "rules", "summary", "width"
        }
    ],
    "td" | "th" => &[
        &COMMON_ATTRS,
        &phf::phf_set! {
            "abbr", "align", "axis", "bgcolor", "colspan", "headers", "height",
            "nowrap", "rowspan", "scope", "valign", "width"
        }
    ],
    "tr" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "align", "bgcolor", "valign" }
    ],
    "time" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "datetime" }
    ],
    "track" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "kind", "label", "src", "srclang", "type" }
    ],
    "video" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "controls", "height", "poster", "preload", "width" }
    ],
};

/// Common attributes allowed on most tags.
static COMMON_ATTRS: phf::Set<&str> = phf::phf_set! {
    "about",
    "aria-describedby",
    "aria-flowto",
    "aria-hidden",
    "aria-label",
    "aria-labelledby",
    "aria-level",
    "aria-owns",
    "class",
    "datatype",
    "dir",
    "id",
    "itemid",
    "itemprop",
    "itemref",
    "itemscope",
    "itemtype",
    "lang",
    "property",
    "resource",
    "role",
    "style",
    "tabindex",
    "title",
    "typeof",
};

/// An emitter debugger.
#[allow(
    clippy::allow_attributes,
    dead_code,
    reason = "this is debugging infrastructure"
)]
#[derive(Debug)]
pub(super) struct Debugger<S: Sink> {
    /// The output.
    next: S,
}

#[allow(
    clippy::allow_attributes,
    dead_code,
    reason = "this is debugging infrastructure"
)]
impl<S: Sink> Debugger<S> {
    /// Creates a new `Debugger` which emits to `next`.
    pub fn new(next: S) -> Self {
        Self { next }
    }
}

chainable!(Debugger);

#[expect(clippy::print_stderr, reason = "this is debugging infrastructure")]
impl<S: Sink> Sink for Debugger<S> {
    fn comment_end(&mut self) {
        eprint!("-->");
        self.next.comment_end();
    }

    fn comment_start(&mut self) {
        eprint!("<!--");
        self.next.comment_start();
    }

    fn entity(&mut self, value: char, raw: &str) {
        eprint!("{raw:?}");
        self.next.entity(value, raw);
    }

    fn finish(self) -> String {
        self.next.finish()
    }

    fn new_line(&mut self) {
        eprintln!();
        self.next.new_line();
    }

    fn strip_marker(&mut self, marker: &StripMarker) {
        eprint!("{marker:?}");
        self.next.strip_marker(marker);
    }

    fn tag_attribute_end(&mut self, name: &str) {
        eprint!("\"");
        self.next.tag_attribute_end(name);
    }

    fn tag_attribute_start(&mut self, name: &str) {
        eprint!(" {name}=\"");
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        eprint!("</{name}>");
        self.next.tag_end(name);
    }

    fn tag_start(&mut self, name: &str) {
        eprint!("<{name}");
        self.next.tag_start(name);
    }

    fn tag_start_end(&mut self, name: &str) {
        eprint!(">");
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        eprint!("{text:?}");
        self.next.text(text);
    }
}

/// A “list of active formatting elements”.
#[derive(Debug, Default)]
struct DomTreeFormattingList {
    /// The buffer for active formatting elements’ attributes. Since most
    /// formatting elements have no attributes, this should be small and rarely
    /// allocated.
    attributes: String,
    /// If true, currently buffering the attributes of a formatting element.
    buffering: bool,
    /// The active formatting elements.
    elements: Vec<DomTreeFormattingItem>,
    /// The index of the rightmost marker in [`Self::elements`].
    marker_index: Option<u8>,
}

impl DomTreeFormattingList {
    /// Marker in [`Self::attributes`] for the end of an attribute value.
    const END_OF_ATTR: char = '\0';
    /// Marker in [`Self::attributes`] for the end of an attribute list.
    const END_OF_ATTRS: char = '\x01';
    /// Marker in [`Self::attributes`] for the end of an attribute name.
    const END_OF_NAME: char = '=';

    /// Returns the index of the first item in [`Self::elements`] after the
    /// rightmost marker.
    #[inline]
    fn after_marker(&self) -> usize {
        self.marker_index.map_or(0, |index| usize::from(index) + 1)
    }

    /// Pushes `value` to the attributes buffer if a new formatting element is
    /// being buffered.
    #[inline]
    fn buffer_char(&mut self, value: char) {
        if self.buffering {
            self.attributes.push(value);
        }
    }

    /// Pushes `text` to the attributes buffer if a new formatting element is
    /// being buffered.
    #[inline]
    fn buffer_text(&mut self, text: &str) {
        if self.buffering {
            self.attributes += text;
        }
    }

    /// Truncates the “list of formatting elements” before the rightmost marker.
    #[inline]
    fn clear_to_marker(&mut self) {
        if let Some(index) = self.marker_index.take() {
            let index = usize::from(index);
            self.attributes
                .truncate(self.elements[index].attr_index.into());
            let marker = self.elements.drain(index..).next().map(|node| node.node);
            if let Some(TagNode::Marker(marker)) = marker {
                self.marker_index = marker.map(|index| u8::from(index) - 1);
            } else {
                panic!("a marker should always point to the next marker");
            }
        } else {
            self.attributes.clear();
            self.elements.clear();
        }
    }

    /// Returns true if `tag` exists in the “list of active formatting
    /// elements”.
    #[inline]
    fn contains(&self, tag: Tag) -> bool {
        self.elements.iter().any(|node| node.node == tag)
    }

    /// Finds the index of the rightmost item in [`Self::elements`] that matches
    /// the given predicate, ending at the rightmost marker.
    fn index(&self, mut predicate: impl FnMut(&TagNode) -> bool) -> Option<usize> {
        let min = self.after_marker();
        self.elements[min..]
            .iter()
            .rposition(|node| predicate(&node.node))
            .map(|index| min + index)
    }

    /// Iterates over all formatting elements in the given `range`, returning
    /// the tag and the list of attributes.
    fn iter(
        &self,
        range: core::ops::RangeFrom<usize>,
    ) -> impl Iterator<Item = (TagNode, impl Iterator<Item = (&str, &str)>)> {
        self.elements[range].iter().map(|node| {
            let mut attrs = &self.attributes[usize::from(node.attr_index)..];
            let attrs_iter = core::iter::from_fn(move || {
                if attrs.is_empty() || attrs.starts_with(Self::END_OF_ATTRS) {
                    None
                } else {
                    let (name, value) = attrs.split_once(Self::END_OF_NAME).unwrap();
                    let (value, rest) = value.split_once(Self::END_OF_ATTR).unwrap();
                    attrs = rest;
                    Some((name, value))
                }
            });
            (node.node, attrs_iter)
        })
    }

    /// Pushes a new tag to the “list of active formatting elements”, enabling
    /// attribute buffering.
    fn push(&mut self, tag: Tag) {
        // “If there are already three … remove the earliest”
        let min = self.after_marker();
        let mut iter = self.elements[min..]
            .iter()
            .enumerate()
            .filter_map(|(index, node)| (node.node == tag).then_some(index));
        let first = iter.next();
        if iter.count() == 2 {
            self.remove(min + first.unwrap());
        }

        self.elements.push(DomTreeFormattingItem {
            attr_index: self.attributes.len().try_into().unwrap(),
            node: tag.into(),
        });
        self.buffering = true;
    }

    /// Pushes a marker to the “list of active formatting elements”.
    fn push_marker(&mut self) {
        let node = TagNode::Marker(next_index(&mut self.marker_index, self.elements.len()));
        self.elements.push(DomTreeFormattingItem {
            attr_index: self.attributes.len().try_into().unwrap(),
            node,
        });
    }

    /// Removes a formatting item at the given `index`, correcting the marker
    /// pointer chain if needed.
    fn remove(&mut self, index: usize) {
        let old = self.elements.remove(index);
        if let Some(next) = &mut self.marker_index
            && usize::from(*next) > index
        {
            *next -= 1;
            let mut marker = usize::from(*next);
            while let TagNode::Marker(Some(next)) = &mut self.elements[marker].node
                && let old = u8::from(*next)
                && usize::from(old) > index
            {
                let new = old - 1;
                *next = NonZeroU8::new(new).unwrap();
                marker = new.into();
            }
        }
        if index == self.elements.len() {
            self.attributes.truncate(old.attr_index.into());
        }
    }

    /// Finds the position of the element with a tag matching the given
    /// `predicate`.
    #[inline]
    fn rfind(&self, predicate: impl Fn(&TagNode) -> bool) -> Option<usize> {
        self.elements.iter().rposition(|node| predicate(&node.node))
    }

    /// Finishes buffering an attribute.
    #[inline]
    fn tag_attribute_end(&mut self) {
        if self.buffering {
            self.attributes.push(Self::END_OF_ATTR);
        }
    }

    /// Starts buffering an attribute.
    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if self.buffering {
            self.attributes += name;
            self.attributes.push(Self::END_OF_NAME);
        }
    }

    /// Finishes buffering a formatting tag.
    #[inline]
    fn tag_start_end(&mut self) {
        if self.buffering {
            // A terminator is used so that if a formatting element is removed
            // from the middle of the list of formatting elements, it does not
            // require any work to fix up indexes or move the buffer around.
            // This is what we in the biz call premature optimisation.
            self.attributes.push(Self::END_OF_ATTRS);
            self.buffering = false;
        }
    }
}

/// An active formatting element.
#[derive(Clone, Copy, Debug)]
struct DomTreeFormattingItem {
    /// The index into [`DomTreeFormattingList::attributes`].
    attr_index: u16,
    /// The tag.
    node: TagNode,
}

/// Balances the DOM tree using the HTML5 tree construction algorithm(ish).
#[derive(Debug)]
pub(super) struct DomTree<S: Sink> {
    /// The set of tags not matching any known HTML5 tag.
    custom_tags: IndexSet<Uncased<'static>>,
    /// If true, filtering out an invalid start tag.
    filtering: bool,
    /// The index of the rightmost `<form>` element in [`Self::stack`].
    form_index: Option<u8>,
    /// The “list of active formatting elements”.
    format: DomTreeFormattingList,
    /// If true, filter out the next newline token.
    ignore_next_newline: bool,
    /// If true, currently in an HTML start tag.
    in_attr: bool,
    /// The current parser mode.
    mode: DomMode,
    /// The output.
    next: S,
    /// The index of the rightmost `<p>` in [`Self::stack`].
    p_index: Option<u8>,
    /// The stack of currently open nodes.
    stack: Vec<TagNode>,
}

/// Emit the tag to the next sink.
const EMIT: bool = true;

/// Discard the tag instead of emitting it.
const SUPPRESS: bool = false;

/// An HTML5 tree construction mode. Modes which are not salient to this
/// fragment parsing implementation are omitted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum DomMode {
    /// The “in body” insertion mode.
    #[default]
    Body,
    /// The “in caption” insertion mode.
    Caption,
    /// The “in cell” insertion mode.
    Cell,
    /// The “in column group” insertion mode.
    ColumnGroup,
    /// The “in row” insertion mode.
    Row,
    /// The “in table” insertion mode.
    Table,
    /// The “in table body” insertion mode.
    TableBody,
}

chainable!(DomTree);

impl<S: Sink> DomTree<S> {
    /// Creates a new `DomTree` which emits to `next`.
    #[inline]
    pub fn new(next: S) -> Self {
        Self {
            custom_tags: <_>::default(),
            filtering: <_>::default(),
            form_index: <_>::default(),
            format: <_>::default(),
            ignore_next_newline: <_>::default(),
            in_attr: <_>::default(),
            mode: <_>::default(),
            next,
            p_index: <_>::default(),
            stack: <_>::default(),
        }
    }

    /// Runs the “adoption agency algorithm”, either for a formatting end `tag`,
    /// or for a start `<nobr>`.
    fn adopt(&mut self, tag: Tag) {
        // TODO: This ends up being O(n) but could be O(1) if `self.format` had
        // a counter table.
        // 2.
        if let Some(e) = self
            .stack
            .pop_if(|node| *node == tag && !self.format.contains(tag))
        {
            // The top of the stack was the tag, which is a formatting tag, but
            // somehow there is no corresponding formatting tag in the list of
            // formatting tags?
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
            return;
        }

        // 3..4.2.
        for _ in 0..8 {
            // 4.3. `format_index` is the index of the corresponding start tag.
            let Some(format_index) = self.format.index(|node| *node == tag) else {
                // No corresponding formatting tag after the last marker means
                // this is either a rogue end tag which will be suppressed, or
                // the corresponding start node is in a scope outside the marker
                self.tag_end_default(tag);
                return;
            };

            // 4.5. Scope checked scan goes first because it will do less, so is
            // faster.
            // `stack_index` is the index of the start tag to be adopted in the
            // stack.
            let Some(stack_index) = self.index_in_scope(|node| *node == tag, Tag::is_general_scope)
            else {
                // If there is no corresponding start tag in scope, then
                // there is nothing to do right now

                // 4.4.
                if !self.stack.contains(&tag.into()) {
                    // Actually, there was no corresponding start tag in *any*
                    // scope, so the formatting tag must have been implicitly
                    // closed and goes now to the soylent factory, rip
                    self.format.remove(format_index);
                }

                return;
            };

            // 4.7. The “topmost node … lower in the stack than the formatting
            // element … in the special category”. And of course “lower” means
            // to the right, not lower index.
            let Some(max) = self.stack[stack_index + 1..]
                .iter()
                .position(|node| node.is_special())
            else {
                // 4.8. There wasn’t any “furthest block”, so this tag and all
                // of its children are getting closed. Any formatting elements
                // after this one will be reopened by `reformat` later.
                for e in self.stack.drain(stack_index..).rev() {
                    e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
                }
                self.format.remove(format_index);
                return;
            };

            // 4.9.
            // In the spec language, there would always be a common element
            // because there would always be a root `<html>`. Since this is an
            // insertion point, it can just point to the stack element index.
            // let common = stack_index;

            // 4.10.
            // let mut bookmark = format_index;

            // 4.11.
            let mut node_index = max;
            // let mut last_node_index = max;

            // 4.12..4.13.
            for inner in 1.. {
                if node_index == 0 {
                    log::warn!("Ran out of nodes");
                    return;
                }

                // 4.13.2.
                node_index -= 1;
                let node = self.stack[node_index];

                // 4.13.3.
                if node == tag {
                    break;
                }

                // 4.13.4.
                let mut format_index = self.format.rfind(|fmt_node| *fmt_node == node);

                if inner > 3
                    && let Some(index) = format_index.take()
                {
                    self.format.remove(index);
                }

                if let Some(_index) = format_index {
                    // 4.13.6. “[clone] `node` … [and] replace the entry … in
                    // [formatting and the stack with the clone] … and let
                    // `node` be the [clone]”

                    // 4.13.7. “move the bookmark … to be immediately after the
                    // new node”
                    // if last_node_index == max {
                    //     bookmark = index + 1;
                    // }
                } else {
                    // 4.13.5.
                    let e = self.stack.remove(node_index);
                    e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
                    continue;
                }

                // 4.13.8. “append `lastNode` to `node`”, which means nothing
                // for the first loop, only for the subsequent loops does this
                // cause a change? WHY IS THIS ALGORITHM SO BAD?
                // <b><a></b><c> <d>
                //           ^^^ ^^^ lastNode
                //          node
                log::warn!(concat!(
                    "TODO: Go back in time and insert tags such that",
                    " `last_node` is inside `node`"
                ));

                // 4.13.9.
                // last_node_index = node_index;
            }

            // 4.14. “insert `last_node` … at `common`”. Because `last_node` is
            // the node after the corresponding formatting start tag, this is
            // equivalent to doing something less insane TODO

            // Normally this would do content fostering if the mode had been
            // “in table” at the time of the insertion, but that is handled
            // separately, so this just does the normal insert
            // self.stack.insert(common, tag.into());

            // 4.15..4.17. These steps are all just injecting a single
            // formatting start tag in between the `max` and the children of
            // `max`
            // self.stack.insert(max, tag.into());

            // 4.18. This just shifts the position of the formatting element in
            // to reflect the new reality
            // if bookmark != format_index {
            //     self.format[format_index..bookmark].rotate_left(1);
            // }
        }
    }

    /// A marker for fostered content in the algorithm. Actual fostering is done
    /// more clearly and efficiently by the `TableFoster` sink, which can look
    /// at entire chunks of of interstitial content and then move it without any
    /// extra buffer allocations and without pessimising the whole thing.
    #[inline]
    const fn foster() {}

    /// Closes the nearest table cell element.
    fn close_cell(&mut self) {
        // The spec pops all implied end tags first to track errors, but this
        // implementation does not need to track errors
        self.pop_inclusive(|node| matches!(node.tag(), Some(Tag::Td | Tag::Th)));
        self.format.clear_to_marker();
        self.mode = DomMode::Row;
    }

    /// Closes the nearest `<p>` element “in button scope”, if one exists.
    #[inline]
    fn close_p(&mut self) {
        // The spec pops all implied end tags first to track errors, but
        // this implementation does not need to track errors
        self.pop_in_scope(|node| *node == Tag::P, Tag::is_button_scope);
    }

    /// Performs special fixups for nested `<a>` tags.
    fn fixup_anchor(&mut self, tag: Tag) {
        if self.format.index(|node| *node == tag).is_some() {
            self.adopt(tag);
            // “remove that element from the list of active formatting elements
            // and the stack of open elements if the adoption agency algorithm
            // didn’t already remove it (it might not have if the element is not
            // in table scope)” suggests some ability to identify the same
            // element in both stacks by identity after a mutation, which is not
            // possible here. the spec suggests in §13.3 that anchors are
            // allowed to nest in the case of fostering, until they are
            // serialised, then they are not. Since this is a serialiser it
            // should be the case that these things never nest.
            if let Some(index) = self.format.rfind(|node| *node == tag) {
                self.format.remove(index);
            }
            self.pop_in_scope(|node| *node == tag, |_| false);
        }
    }

    /// Pop all elements on the stack with implied end tags except for `except`.
    fn implied_end(&mut self, except: Option<Tag>) {
        while let Some(e) = self.stack.pop_if(|node| node.is_implied_close(except)) {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
    }

    /// Returns the index of an element matching the given `predicate` on the
    /// stack of open elements in the scope given by `scope`, or `None` if there
    /// is no such element.
    #[inline]
    fn in_scope(
        &self,
        predicate: impl FnMut(&TagNode) -> bool,
        scope: impl FnMut(Tag) -> bool,
    ) -> bool {
        self.index_in_scope(predicate, scope).is_some()
    }

    /// Returns the index of an element matching the given `predicate` on the
    /// stack of open elements in the scope given by `scope`, or `None` if there
    /// is no such element.
    fn index_in_scope(
        &self,
        mut predicate: impl FnMut(&TagNode) -> bool,
        mut scope: impl FnMut(Tag) -> bool,
    ) -> Option<usize> {
        for (index, node) in self.stack.iter().enumerate().rev() {
            #[rustfmt::skip]
            if predicate(node) {
                return Some(index);
            } else if let Some(tag) = node.tag() && scope(tag) {
                break;
            };
        }
        None
    }

    /// Closes all elements up to `predicate`.
    fn pop_exclusive(&mut self, mut predicate: impl FnMut(&mut TagNode) -> bool) {
        while let Some(e) = self.stack.pop_if(|node| !predicate(node)) {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
    }

    /// Closes all elements up to and including `predicate` if a match exists in
    /// the scope given by `scope`, returning `true` if elements were closed.
    fn pop_in_scope(
        &mut self,
        predicate: impl FnMut(&TagNode) -> bool,
        scope: impl FnMut(Tag) -> bool,
    ) -> bool {
        if let Some(index) = self.index_in_scope(predicate, scope) {
            for e in self.stack.drain(index..).rev() {
                e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
            }
            true
        } else {
            false
        }
    }

    /// Closes all elements up to and including `predicate`.
    fn pop_inclusive(&mut self, predicate: impl FnMut(&mut TagNode) -> bool) {
        self.pop_exclusive(predicate);
        if let Some(e) = self.stack.pop() {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
    }

    /// Closes the element at the end of the stack if it matches `predicate`,
    /// returning `true` if the element was closed.
    fn pop_one(&mut self, predicate: impl FnOnce(&mut TagNode) -> bool) -> bool {
        if let Some(e) = self.stack.pop_if(predicate) {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
            true
        } else {
            false
        }
    }

    /// Pushes an indexed `<p>` tag to the stack. This is an optimisation.
    fn push_p(&mut self) {
        let node = TagNode::P(next_index(&mut self.p_index, self.stack.len()));
        self.stack.push(node);
    }

    /// Reopens any formatting elements which were closed due to element
    /// splitting.
    fn reformat(&mut self) {
        // TODO: This is O(n^2), but could be made O(n) by having a tag count
        // table for the stack.
        let Some(first_missing) = self.format.index(|node| !self.stack.contains(node)) else {
            return;
        };

        for (tag, attrs) in self.format.iter(first_missing..) {
            let tag_name = tag.name(&self.custom_tags).expect("named tag");
            self.next.tag_start(tag_name.as_str());
            for (name, value) in attrs {
                self.next.tag_attribute_full(name, value);
            }
            self.next.tag_start_end(tag_name.as_str());
            self.stack.push(tag);
        }
    }

    /// Slowly recalculates the current insertion mode according to what
    /// elements are on the stack.
    fn reset_mode(&mut self) {
        let mode = self
            .stack
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, node)| match node.tag() {
                Some(Tag::Td | Tag::Th) if index != 0 => Some(DomMode::Cell),
                Some(Tag::Tr) => Some(DomMode::Row),
                Some(Tag::Tbody | Tag::Tfoot | Tag::Thead) => Some(DomMode::TableBody),
                Some(Tag::Caption) => Some(DomMode::Caption),
                Some(Tag::Table) => Some(DomMode::Table),
                _ => None,
            });

        self.mode = mode.unwrap_or(DomMode::Body);
    }

    /// Inserts a new end `tag` in the “in body” insertion mode.
    fn tag_end_body(&mut self, tag: Tag) {
        match tag {
            Tag::Address
            | Tag::Aside
            | Tag::Blockquote
            | Tag::Button
            | Tag::Center
            | Tag::Details
            | Tag::Div
            | Tag::Dl
            | Tag::Figcaption
            | Tag::Figure
            | Tag::Form
            | Tag::Ol
            | Tag::Pre
            | Tag::Select
            | Tag::Summary
            | Tag::Ul => {
                if tag == Tag::Form {
                    self.form_index = None;
                }
                if self.stack.iter().rfind(|node| **node == tag).is_some() {
                    self.implied_end(None);
                    // The spec pops all implied end tags first to track errors,
                    // but this implementation does not need to track errors
                    self.pop_inclusive(|node| *node == tag);
                }
            }
            Tag::Br => {
                if self.tag_start_body(tag) {
                    self.next.tag_start_full("br");
                }
            }
            Tag::Dd | Tag::Dt => {
                // The spec pops implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.pop_in_scope(|node| *node == tag, Tag::is_general_scope);
            }
            tag if tag.is_heading() => {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.pop_in_scope(|node| node.is_heading(), Tag::is_general_scope);
            }
            Tag::Li => {
                // The spec pops implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.pop_in_scope(|node| *node == tag, Tag::is_list_item_scope);
            }
            Tag::Object => {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                if self.pop_in_scope(|node| *node == tag, Tag::is_general_scope) {
                    self.format.clear_to_marker();
                }
            }
            Tag::P => {
                if !self.in_scope(|node| *node == tag, Tag::is_button_scope) {
                    self.tag_start_full("p");
                }
                self.close_p();
            }
            tag if tag.is_formatting() => {
                self.adopt(tag);
            }
            tag => self.tag_end_default(tag),
        }
    }

    /// Inserts a new end `tag` in the “in caption” insertion mode.
    fn tag_end_caption(&mut self, tag: Tag) {
        if matches!(tag, Tag::Caption | Tag::Table) {
            if self.pop_in_scope(|node| *node == Tag::Caption, Tag::is_table_scope) {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.format.clear_to_marker();
                self.mode = DomMode::Table;
                if tag == Tag::Table {
                    self.tag_end_table(tag);
                }
            }
        } else if !tag.is_table_item() {
            self.tag_end_body(tag);
        }
    }

    /// Inserts a new end `tag` in the “in cell” insertion mode.
    fn tag_end_cell(&mut self, tag: Tag) {
        if matches!(tag, Tag::Td | Tag::Th) {
            if self.pop_in_scope(|node| *node == tag, Tag::is_table_scope) {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.format.clear_to_marker();
                self.mode = DomMode::Row;
            }
        } else if tag.is_table_fosterable()
            && self.in_scope(|node| *node == tag, Tag::is_table_scope)
        {
            self.close_cell();
            self.tag_end_row(tag);
        } else if !tag.is_table_item() {
            self.tag_end_body(tag);
        }
    }

    /// Inserts a new end `tag` in the “in column group” insertion mode.
    fn tag_end_colgroup(&mut self, tag: Tag) {
        if tag != Tag::Col && self.pop_one(|node| *node == Tag::Colgroup) {
            self.mode = DomMode::Table;
            if tag != Tag::Colgroup {
                self.tag_end_table(tag);
            }
        }
    }

    /// The fallback implementation for inserting a new end `tag`.
    fn tag_end_default(&mut self, tag: Tag) {
        // The spec pops implied end tags first to track errors, but this
        // implementation does not need to track errors
        self.pop_in_scope(|node| *node == tag, Tag::is_special);
    }

    /// Inserts a new end `tag` in the “in row” insertion mode.
    fn tag_end_row(&mut self, tag: Tag) {
        if tag.is_table_fosterable() {
            if tag.is_table_body() && !self.in_scope(|node| *node == tag, Tag::is_table_scope) {
            } else if self.pop_in_scope(|node| *node == Tag::Tr, Tag::is_table_scope) {
                self.mode = DomMode::TableBody;
                if tag != Tag::Tr {
                    self.tag_end_table_body(tag);
                }
            }
        } else if !tag.is_table_item() {
            self.tag_end_table(tag);
        }
    }

    /// Inserts a new end `tag` in the “in table” insertion mode.
    fn tag_end_table(&mut self, tag: Tag) {
        if tag == Tag::Table {
            if self.pop_in_scope(|node| *node == Tag::Table, Tag::is_table_scope) {
                self.reset_mode();
            }
        } else if !tag.is_table_item() {
            Self::foster();
            self.tag_end_body(tag);
        }
    }

    /// Inserts a new end `tag` in the “in table body” insertion mode.
    fn tag_end_table_body(&mut self, tag: Tag) {
        if tag == Tag::Table || tag.is_table_body() {
            if self.pop_in_scope(|node| *node == tag, Tag::is_table_scope) {
                self.mode = DomMode::Table;
                if tag == Tag::Table {
                    self.reset_mode();
                }
            }
        } else if !tag.is_table_item() {
            self.tag_end_table(tag);
        }
    }

    /// Inserts a new start `tag` in the mode defined by [`Self::mode`].
    fn tag_start_any(&mut self, tag: Tag) -> bool {
        match self.mode {
            DomMode::Body => self.tag_start_body(tag),
            DomMode::Table => self.tag_start_table(tag),
            DomMode::Caption => self.tag_start_caption(tag),
            DomMode::ColumnGroup => self.tag_start_colgroup(tag),
            DomMode::TableBody => self.tag_start_table_body(tag),
            DomMode::Row => self.tag_start_row(tag),
            DomMode::Cell => self.tag_start_cell(tag),
        }
    }

    /// Inserts a new start `tag` in the “in body” insertion mode.
    #[expect(clippy::too_many_lines, reason = "complaints go to WHATWG")]
    fn tag_start_body(&mut self, tag: Tag) -> bool {
        match tag {
            tag if tag.is_head_item() => self.tag_start_head(tag),

            Tag::Br | Tag::Img | Tag::Wbr => {
                self.reformat();
                EMIT
            }
            Tag::Button => {
                // The spec pops implied end tags first, but this does not
                // seem to make sense since these would all be closed anyway
                // on the way to the button element and it is already a
                // parse error
                self.pop_in_scope(|node| *node == tag, Tag::is_general_scope);
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Dd | Tag::Dt | Tag::Li => {
                // The spec pops implied end tags first to track errors,
                // but this implementation does not need to track errors
                if tag == Tag::Li {
                    self.pop_in_scope(|node| *node == tag, Tag::is_list_special);
                } else {
                    self.pop_in_scope(|node| node.is_dl_item(), Tag::is_list_special);
                }
                self.close_p();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Form => {
                if self.form_index.is_none() {
                    self.form_index = Some(self.stack.len().try_into().unwrap());
                    self.close_p();
                    self.stack.push(tag.into());
                    EMIT
                } else {
                    SUPPRESS
                }
            }
            tag if tag.is_heading() => {
                self.close_p();
                self.pop_one(|node| node.is_heading());
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Hr => {
                self.close_p();
                // For `<hr>` in `<select>`
                if self.in_scope(|node| *node == Tag::Select, Tag::is_general_scope) {
                    self.implied_end(None);
                }
                EMIT
            }
            Tag::Iframe => {
                // For `<iframe>` the spec says to switch to “generic raw text
                // parsing algorithm” but this is not a tokeniser
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Input => {
                self.pop_in_scope(|node| *node == Tag::Select, Tag::is_general_scope);
                self.reformat();
                EMIT
            }
            Tag::Object => {
                self.reformat();
                self.format.push_marker();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Option | Tag::Optgroup => {
                if self.in_scope(|node| *node == Tag::Select, Tag::is_general_scope) {
                    let except = (tag == Tag::Option).then_some(Tag::Optgroup);
                    self.implied_end(except);
                } else {
                    self.pop_one(|node| *node == Tag::Option);
                }
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Pre | Tag::Textarea => {
                // For `<textarea>` the spec says to switch to RCDATA but this
                // is not a tokeniser
                if tag == Tag::Pre {
                    self.close_p();
                }
                self.ignore_next_newline = true;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Select if self.pop_in_scope(|node| *node == tag, Tag::is_general_scope) => {
                SUPPRESS
            }
            Tag::Select => {
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Table => {
                self.close_p();
                self.mode = DomMode::Table;
                self.stack.push(tag.into());
                EMIT
            }
            tag if tag.is_body_block() => {
                self.close_p();
                if tag == Tag::P {
                    self.push_p();
                } else {
                    self.stack.push(tag.into());
                }
                EMIT
            }
            tag if tag.is_formatting() => {
                if tag == Tag::A {
                    self.fixup_anchor(tag);
                } else if tag == Tag::Nobr
                    && self.in_scope(|node| *node == tag, Tag::is_general_scope)
                {
                    self.reformat();
                    self.adopt(tag);
                }
                self.reformat();
                self.format.push(tag);
                self.stack.push(tag.into());
                EMIT
            }
            tag if tag.is_ruby_item() => {
                if self.in_scope(|node| *node == Tag::Ruby, Tag::is_general_scope) {
                    let except = matches!(tag, Tag::Rp | Tag::Rt).then_some(Tag::Rtc);
                    self.implied_end(except);
                }
                self.stack.push(tag.into());
                EMIT
            }
            tag if tag.is_table_item() => SUPPRESS,
            _ => {
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
        }
    }

    /// Inserts a new start `tag` in the “in caption” insertion mode.
    fn tag_start_caption(&mut self, tag: Tag) -> bool {
        if tag.is_table_item() {
            if self.pop_in_scope(|node| *node == Tag::Caption, Tag::is_table_scope) {
                self.format.clear_to_marker();
                self.mode = DomMode::Table;
                self.tag_start_table(tag)
            } else {
                SUPPRESS
            }
        } else {
            self.tag_start_body(tag)
        }
    }

    /// Inserts a new start `tag` in the “in cell” insertion mode.
    fn tag_start_cell(&mut self, tag: Tag) -> bool {
        if tag.is_table_item() {
            self.close_cell();
            self.tag_start_row(tag)
        } else {
            self.tag_start_body(tag)
        }
    }

    /// Inserts a new start `tag` in the “in column group” insertion mode.
    fn tag_start_colgroup(&mut self, tag: Tag) -> bool {
        if tag == Tag::Col {
            EMIT
        } else if self.pop_one(|node| *node == Tag::Colgroup) {
            self.mode = DomMode::Table;
            self.tag_start_table(tag)
        } else {
            SUPPRESS
        }
    }

    /// Inserts a new start `tag` in the “in head” insertion mode.
    fn tag_start_head(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Basefont | Tag::Link | Tag::Meta => EMIT,
            Tag::Title => {
                // This is supposed to use the RCDATA element parsing algorithm,
                // but since the tokeniser has already done its thing, just
                // treat it like a normal whatever
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Style => {
                // This is supposed to use the generic raw text element parsing
                // algorithm, but since the tokeniser has already done its
                // thing, just treat it like a normal whatever
                self.stack.push(tag.into());
                EMIT
            }
            _ => panic!("should never get here"),
        }
    }

    /// Inserts a new start `tag` in the “in row” insertion mode.
    fn tag_start_row(&mut self, tag: Tag) -> bool {
        if matches!(tag, Tag::Td | Tag::Th) {
            self.pop_exclusive(|node| *node == Tag::Tr);
            self.mode = DomMode::Cell;
            self.format.push_marker();
            self.stack.push(tag.into());
            EMIT
        } else if tag.is_table_item() {
            if self.pop_in_scope(|node| *node == Tag::Tr, Tag::is_table_scope) {
                self.mode = DomMode::TableBody;
                self.tag_start_table_body(tag)
            } else {
                SUPPRESS
            }
        } else {
            self.tag_start_table(tag)
        }
    }

    /// Inserts a new start `tag` in the “in table” insertion mode.
    fn tag_start_table(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Caption => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = DomMode::Caption;
                self.format.push_marker();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Colgroup => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = DomMode::ColumnGroup;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Col => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.next.tag_start_full("colgroup");
                self.stack.push(Tag::Colgroup.into());
                self.mode = DomMode::ColumnGroup;
                self.tag_start_colgroup(tag)
            }
            Tag::Tbody | Tag::Tfoot | Tag::Thead => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = DomMode::TableBody;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Td | Tag::Th | Tag::Tr => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.stack.push(TagNode::ImplicitTbody);
                self.mode = DomMode::TableBody;
                self.tag_start_table_body(tag)
            }
            Tag::Table => {
                if self.pop_in_scope(|node| *node == Tag::Table, Tag::is_table_scope) {
                    self.reset_mode();
                    self.tag_start_any(tag)
                } else {
                    SUPPRESS
                }
            }
            Tag::Style => self.tag_start_head(tag),
            Tag::Input => {
                // The spec says that hidden inputs are not supposed to be
                // fostered but this is a needless complexity for this
                // implementation
                Self::foster();
                self.tag_start_body(tag)
            }
            Tag::Form => {
                // The spec says that form in a table is supposed to cause
                // the form pointer to be set, but then to not emit anything
                // to the output. For a serialiser, this just means to not
                // emit anything
                SUPPRESS
            }
            _ => {
                Self::foster();
                self.tag_start_body(tag)
            }
        }
    }

    /// Inserts a new start `tag` in the “in table body” insertion mode.
    fn tag_start_table_body(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Tr => {
                self.pop_exclusive(|node| node.is_table_body());
                self.mode = DomMode::Row;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Td | Tag::Th => {
                self.pop_exclusive(|node| node.is_table_body());
                self.mode = DomMode::Row;
                self.next.tag_start_full("tr");
                self.stack.push(Tag::Tr.into());
                self.tag_start_row(tag)
            }
            tag if tag.is_table_item() => {
                if self.in_scope(|node| node.is_table_body(), Tag::is_table_scope) {
                    self.pop_exclusive(|node| *node == Tag::Table);
                    self.mode = DomMode::Table;
                    self.tag_start_table(tag)
                } else {
                    SUPPRESS
                }
            }
            _ => self.tag_start_table(tag),
        }
    }
}

impl<S: Sink> Sink for DomTree<S> {
    #[inline]
    fn comment_end(&mut self) {
        if self.filtering {
            return;
        }

        self.next.comment_end();
        if !self.in_attr {
            self.ignore_next_newline = false;
        }
    }

    #[inline]
    fn comment_start(&mut self) {
        if self.filtering {
            return;
        }

        self.next.comment_start();
        if !self.in_attr {
            self.ignore_next_newline = false;
        }
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.format.buffer_char(value);
            self.next.entity(value, raw);
        } else {
            if matches!(self.mode, DomMode::Body | DomMode::Caption | DomMode::Cell) {
                self.reformat();
            }
            self.next.entity(value, raw);
            self.ignore_next_newline = false;
        }
    }

    #[inline]
    fn finish(mut self) -> String {
        for e in self.stack.drain(..).rev() {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.next.new_line();
        } else if self.ignore_next_newline {
            self.ignore_next_newline = false;
        } else {
            if matches!(self.mode, DomMode::Body | DomMode::Caption | DomMode::Cell) {
                self.reformat();
            }
            self.next.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        if self.filtering {
            return;
        }

        self.next.strip_marker(marker);
        if !self.in_attr {
            self.ignore_next_newline = false;
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if !self.filtering {
            self.format.tag_attribute_end();
            self.next.tag_attribute_end(name);
        }
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if !self.filtering {
            self.format.tag_attribute_start(name);
            self.next.tag_attribute_start(name);
        }
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.ignore_next_newline = false;

        let tag = Tag::new(name, &mut self.custom_tags);

        match self.mode {
            DomMode::Body => self.tag_end_body(tag),
            DomMode::Table => self.tag_end_table(tag),
            DomMode::Caption => self.tag_end_caption(tag),
            DomMode::ColumnGroup => self.tag_end_colgroup(tag),
            DomMode::TableBody => self.tag_end_table_body(tag),
            DomMode::Row => self.tag_end_row(tag),
            DomMode::Cell => self.tag_end_cell(tag),
        }
    }

    #[inline]
    fn tag_start(&mut self, mut name: &str) {
        self.ignore_next_newline = false;

        if name.eq_ignore_ascii_case("image") {
            name = "img";
        }

        let tag = Tag::new(name, &mut self.custom_tags);

        if self.tag_start_any(tag) {
            self.in_attr = true;
            self.next.tag_start(name);
        } else {
            self.filtering = true;
        }
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        if self.filtering {
            self.filtering = false;
        } else {
            self.in_attr = false;
            self.format.tag_start_end();
            self.next.tag_start_end(name);
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.format.buffer_text(text);
            self.next.text(text);
        } else {
            if matches!(self.mode, DomMode::Body | DomMode::Caption | DomMode::Cell) {
                self.reformat();
            }
            self.next.text(text);
            self.ignore_next_newline = false;
        }
    }
}

/// Takes a value from `index`, returning a niche-optimised `Option<NonZeroU8>`.
fn next_index(index: &mut Option<u8>, next: usize) -> Option<NonZeroU8> {
    index
        .replace(next.try_into().unwrap())
        .and_then(|n| NonZeroU8::new(n + 1))
}

/// Marks elements containing only whitespace.
#[derive(Debug)]
pub(super) struct EmptyTagger<S: Sink> {
    /// The tag name of a potentially empty element.
    last: Option<&'static str>,
    /// The output.
    next: S,
    /// The whitespace buffer for a potentially empty element.
    ws_buffer: String,
}

chainable!(EmptyTagger);

impl<S: Sink + Markable> EmptyTagger<S> {
    /// Creates a new `EmptyTagger` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            last: <_>::default(),
            next,
            ws_buffer: <_>::default(),
        }
    }

    /// Writes the buffered tag to the next sink.
    fn flush(&mut self) {
        if let Some(last) = self.last.take() {
            self.next.tag_start_full(last);
            let ws = self.ws_buffer.drain(..);
            flush_ws(&mut self.next, ws.as_str());
        }
    }
}

impl<S: Sink + Markable> Sink for EmptyTagger<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.flush();
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        debug_assert!(self.last.is_none());
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.last.is_some() {
            self.ws_buffer += "\n";
        } else {
            self.next.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        if !marker.is_empty() {
            self.flush();
        }
        self.next.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if let Some(last) = self.last.take() {
            self.next.tag_start(last);
            debug_assert!(self.ws_buffer.is_empty());
        }
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        if let Some(last) = self.last.take() {
            debug_assert_eq!(name, last);
            self.next.tag_start(last);
            self.next.tag_attribute_full("class", "mw-empty-elt");
            self.next.tag_start_end(last);
            self.next.text(self.ws_buffer.drain(..).as_str());
        }
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.flush();
        if let Some(name) = phf::phf_set!("p", "li", "tr").get_key(name) {
            self.last = Some(*name);
        } else {
            self.next.tag_start(name);
        }
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        if self.last.is_none() {
            self.next.tag_start_end(name);
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if text.bytes().any(|c| !c.is_ascii_whitespace()) {
            self.flush();
        }
        if self.last.is_some() {
            self.ws_buffer += text;
        } else {
            self.next.text(text);
        }
    }
}

/// Flushes runs of text tokens separated by newlines in `ws` to `next`.
#[inline]
fn flush_ws<S: Sink + ?Sized>(next: &mut S, mut ws: &str) {
    while let Some((text, rest)) = ws.split_once('\n') {
        next.text(text);
        next.new_line();
        ws = rest;
    }
    next.text(ws);
}

/// Implicit paragraphs (grafs) emitter. Implicit grafs may be runs of plain
/// text, which will be wrapped by `<p>`, or runs of plain text prefixed by a
/// single space, which will be wrapped by `<pre>`.
///
/// The processing rules, like everything in Wikitext, are absolutely insane
/// nonsense. Just look at this:
///
/// ```html
/// <div>a
/// <span>b
/// c
/// d</span></div>e
/// f
/// g
/// ```
///
/// is supposed to become:
///
/// ```html
/// <div>a
/// <p><span>b
/// c
/// </span></p> <!-- wtf is this, that is not where the `</span>` was?! -->
/// d</div><p>e
/// </p><p>f
/// g
/// </p>
/// ```
///
/// In MW, graf wrapping responsibilities are split between both
/// `Parser\BlockLevelPass` *and* `Tidy\RemexCompatMunger` (or, in Parsoid,
/// `DOM\Processors\PWrap`), presumably just to make it nearly impossible for
/// any one developer to understand how anything works.
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "should care, don’t care. hate this code"
)]
pub(super) struct GrafEmitter<S: Sink + Markable> {
    /// The next line buffer.
    buffer: PrettyText<Accumulator>,
    /// If true, the line contains an end tag which triggers a graf state
    /// transition.
    close_match: bool,
    /// The currently active graf.
    current: GrafState,
    /// If true, the document is currently inside a graf block.
    in_block: bool,
    /// If true, the document is currently inside an explicitly defined
    /// `<blockquote>`.
    in_blockquote: bool,
    /// If true, the document is currently inside an image caption.
    in_caption: bool,
    /// If true, the graf emitter is disabled and acts as a pass-through.
    in_list: bool,
    /// If true, the document is currently inside an explicitly defined `<pre>`.
    in_pre: bool,
    /// State tracking for lines that may contain only `<style>` and `<link>`
    /// tags.
    meta_line: GrafMetaLine,
    /// The output.
    next: S,
    /// If true, the line contains a start tag which triggers a graf state
    /// transition.
    open_match: bool,
    /// The next graf to emit.
    pending: GrafPendingState,
    /// If true, the line contains a `</pre>`.
    pre_close_match: bool,
    /// If true, the line contains a `<pre>`.
    pre_open_match: bool,
}

impl<S: Sink + Markable> GrafEmitter<S> {
    /// Creates a new `GrafEmitter` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            buffer: PrettyText::new(<_>::default()),
            close_match: <_>::default(),
            current: <_>::default(),
            in_block: <_>::default(),
            in_blockquote: <_>::default(),
            in_caption: <_>::default(),
            in_list: <_>::default(),
            in_pre: <_>::default(),
            meta_line: <_>::default(),
            next,
            open_match: <_>::default(),
            pending: <_>::default(),
            pre_close_match: <_>::default(),
            pre_open_match: <_>::default(),
        }
    }

    /// Emits the end of a graf to the output.
    fn close(&mut self, finishing: bool) {
        self.in_pre = false;
        match core::mem::take(&mut self.current) {
            GrafState::None => return,
            GrafState::Graf => self.next.tag_end("p"),
            GrafState::Pre => self.next.tag_end("pre"),
        }
        if !finishing {
            self.next.new_line();
        }
    }

    /// Finishes processing of a line of source text.
    fn end_line(&mut self, last_line: bool) {
        let mut start_at = 0;
        if self.open_match || self.close_match {
            self.pending = GrafPendingState::None;
            if !self.in_pre || self.pre_open_match {
                self.close(false);
            }
            if self.pre_open_match {
                self.in_pre = !self.pre_close_match;
            }
            self.in_block = !self.close_match;
        } else if !self.in_block && !self.in_pre {
            if self.is_pre_line() {
                if self.current != GrafState::Pre {
                    self.pending = GrafPendingState::None;
                    self.close(false);
                    self.next.tag_start_full("pre");
                    self.current = GrafState::Pre;
                }
                start_at = 1;
            } else if self.meta_line == GrafMetaLine::Yes {
                if self.pending != GrafPendingState::None {
                    self.close(false);
                    self.pending = GrafPendingState::None;
                }
            } else if self.is_empty_line() {
                if let Some(new_state) = self.pending.emit(&mut self.next) {
                    self.next.tag_start_full("br");
                    self.current = new_state;
                } else if self.current != GrafState::Graf {
                    self.close(false);
                    self.pending = GrafPendingState::Open;
                } else {
                    self.pending = GrafPendingState::Split;
                }
            } else if let Some(new_state) = self.pending.emit(&mut self.next) {
                self.current = new_state;
            } else if self.current != GrafState::Graf {
                self.close(false);
                self.next.tag_start_full("p");
                self.current = GrafState::Graf;
            }
        }

        let buffer = core::mem::take(self.buffer.next_mut());
        if self.pending == GrafPendingState::None {
            tokenise(&mut self.next, &buffer.as_str()[start_at..]);
            if !last_line || self.current != GrafState::None {
                self.next.new_line();
            }
        }

        self.pre_open_match = <_>::default();
        self.pre_close_match = <_>::default();
        self.open_match = <_>::default();
        self.close_match = <_>::default();
        self.meta_line = <_>::default();
    }

    /// Returns true if the currently buffered line is empty or contains only
    /// ASCII whitespace.
    #[inline]
    fn is_empty_line(&self) -> bool {
        self.buffer
            .next()
            .as_str()
            .bytes()
            .all(|b| b.is_ascii_whitespace())
    }

    /// Returns true if the currently buffered line should be treated like a
    /// preformatted line.
    #[inline]
    fn is_pre_line(&self) -> bool {
        !self.in_blockquote
            && self
                .buffer
                .next()
                .as_str()
                .strip_prefix(' ')
                .is_some_and(|text| {
                    self.current == GrafState::Pre || text.bytes().any(|b| !b.is_ascii_whitespace())
                })
    }

    /// Causes `GrafEmitter` to treat new line tokens as text. This is required
    /// for correct handling of image captions.
    #[inline]
    pub(super) fn set_in_caption(&mut self, in_caption: bool) {
        self.in_caption = in_caption;
    }

    /// Disables the `GrafEmitter`, causing it to pass through tokens. This is
    /// required for correct handling of lists.
    #[inline]
    pub(super) fn set_in_list(&mut self, in_list: bool) {
        self.pending = GrafPendingState::None;
        self.in_list = in_list;
        if in_list {
            self.close(false);
        }
    }
}

chainable!(GrafEmitter);

impl<S: Sink + Markable> Sink for GrafEmitter<S> {
    #[inline]
    fn comment_end(&mut self) {
        if self.in_list {
            self.next.comment_end();
        } else {
            self.buffer.comment_end();
        }
    }

    #[inline]
    fn comment_start(&mut self) {
        if self.in_list {
            self.next.comment_start();
        } else {
            self.buffer.comment_start();
        }
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.in_list {
            self.next.entity(value, raw);
        } else {
            self.meta_line = GrafMetaLine::No;
            self.buffer.entity(value, raw);
        }
    }

    fn finish(mut self) -> String {
        self.end_line(true);
        self.close(true);
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.in_caption {
            self.meta_line.update_text("\n");
            self.buffer.new_line();
        } else if self.in_list {
            self.next.new_line();
        } else {
            self.end_line(false);
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        if self.in_list {
            self.next.strip_marker(marker);
        } else {
            // General strip markers need to be unstripped before now
            debug_assert!(!matches!(marker, StripMarker::General(_)));
            if matches!(marker, StripMarker::NoWiki(_)) {
                self.meta_line = GrafMetaLine::No;
            }
            self.buffer.strip_marker(marker);
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if self.in_list {
            self.next.tag_attribute_end(name);
        } else {
            self.buffer.tag_attribute_end(name);
        }
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if self.in_list {
            self.next.tag_attribute_start(name);
        } else {
            self.buffer.tag_attribute_start(name);
        }
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        if self.in_list {
            self.next.tag_end(name);
        } else {
            self.open_match |= ALWAYS_TAG.contains(name) || ANTI_BLOCK_TAG.contains(name);
            self.close_match |= NEVER_TAG.contains(name) || BLOCK_TAG.contains(name);
            self.in_blockquote &= name != "blockquote";
            self.pre_close_match |= name == "pre";
            self.meta_line.update_tag_end(name);
            self.buffer.tag_end(name);
        }
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        if self.in_list {
            self.next.tag_start(name);
        } else {
            self.open_match |= ALWAYS_TAG.contains(name) || BLOCK_TAG.contains(name);
            self.close_match |= NEVER_TAG.contains(name) || ANTI_BLOCK_TAG.contains(name);
            self.pre_open_match |= name == "pre";
            self.in_blockquote |= name == "blockquote";
            self.meta_line.update_tag_start(name);
            self.buffer.tag_start(name);
        }
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        if self.in_list {
            self.next.tag_start_end(name);
        } else {
            self.buffer.tag_start_end(name);
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.in_list {
            self.next.text(text);
        } else {
            self.meta_line.update_text(text);
            self.buffer.text(text);
        }
    }
}

/// Graf meta line state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum GrafMetaLine {
    /// At the start of the line. The next thing must be a `<style>` or `<link>`
    /// to be a meta line.
    #[default]
    Start,
    /// Womp womp. Definitely not a meta line.
    No,
    /// Got a `<style>` tag. There must eventually be a `</style>`, or this is
    /// not a meta line.
    InStyle,
    /// Got a `<style>` or `<link>`. The next thing must be a `<style>` or
    /// `<link>` or ASCII whitespace or a newline to be a meta line.
    Yes,
}

impl GrafMetaLine {
    /// Update the state for an HTML end tag with the given `name`.
    fn update_tag_end(&mut self, name: &str) {
        if name != "style" || *self != Self::InStyle {
            *self = Self::No;
        } else {
            *self = Self::Yes;
        }
    }

    /// Update the state for an HTML start tag with the given `name`.
    fn update_tag_start(&mut self, name: &str) {
        if name == "style" {
            // `<style><style>` is stupid, but legal
            if *self != Self::No {
                *self = Self::InStyle;
            }
        } else if name == "link" {
            if *self == Self::Start {
                *self = Self::Yes;
            }
        } else {
            *self = Self::No;
        }
    }

    /// Update the state for the given `text`.
    fn update_text(&mut self, text: &str) {
        if *self == Self::Start
            || (*self == Self::Yes && text.bytes().any(|b| !b.is_ascii_whitespace()))
        {
            *self = Self::No;
        }
    }
}

/// Graf emitter pending output state.
///
/// This is used when the production of a line is ambiguous and cannot be
/// resolved until a subsequent line can offer disambiguation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum GrafPendingState {
    /// Emitting nothing.
    #[default]
    None,
    /// Maybe this line should be a graf.
    Open,
    /// Maybe this line should be a break between two grafs.
    Split,
}

impl GrafPendingState {
    /// Emits `self` to `next`, returning a new `GrafState` if something was
    /// emitted.
    fn emit<S: Sink + ?Sized>(&mut self, next: &mut S) -> Option<GrafState> {
        match self {
            Self::None => None,
            Self::Open => {
                next.tag_start_full("p");
                *self = Self::None;
                Some(GrafState::Graf)
            }
            Self::Split => {
                next.tag_end("p");
                next.tag_start_full("p");
                *self = Self::None;
                Some(GrafState::Graf)
            }
        }
    }
}

/// Graf emitter state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum GrafState {
    /// Emitting nothing.
    #[default]
    None,
    /// Emitting a normal graf (`<p>`).
    Graf,
    /// Emitting a preformatted graf (`<pre>`).
    Pre,
}

/// HTML tags which start a new block when they are encountered as either a
/// start or end tag.
static ALWAYS_TAG: phf::Set<&str> = phf::phf_set! {
    "caption", "dd", "dt", "li", "tr"
};

/// HTML tags which terminate a block when they are encountered as an end tag.
static ANTI_BLOCK_TAG: phf::Set<&str> = phf::phf_set! { "td", "th" };

/// HTML tags which start a new block when they are encountered as start tags.
static BLOCK_TAG: phf::Set<&str> = phf::phf_set! {
    "dl", "h1", "h2", "h3", "h4", "h5", "h6", "ol", "p", "pre", "table", "ul"
};

/// HTML tags which terminate a block when they are encountered as start or end
/// tags.
static NEVER_TAG: phf::Set<&str> = phf::phf_set! {
    "aside", "blockquote", "center", "div", "figure", "hr"
};

/// Tokenises the given `html` and sends it to `next`.
fn tokenise<S: Sink + ?Sized>(next: &mut S, html: &str) {
    let mut in_attr = None::<String>;
    let mut in_tag = None;
    let emitter = CallbackEmitter::new(|event: CallbackEvent<'_>, _: html5gum::Span<()>| {
        match event {
            CallbackEvent::OpenStartTag { name } => {
                // SAFETY: This data comes from a `&str`.
                let name = unsafe { str::from_utf8_unchecked(name) };
                next.tag_start(name);
                in_tag = Some(name.to_owned());
            }
            CallbackEvent::AttributeName { name } => {
                if let Some(name) = &in_attr {
                    next.tag_attribute_end(name);
                }
                // SAFETY: This data comes from a `&str`.
                let name = unsafe { str::from_utf8_unchecked(name) };
                next.tag_attribute_start(name);
                in_attr = Some(name.to_owned());
            }
            CallbackEvent::AttributeValue { value } | CallbackEvent::String { value } => {
                // SAFETY: This data comes from a `&str`.
                let value = unsafe { str::from_utf8_unchecked(value) };
                flush_ws(next, value);
            }
            CallbackEvent::CloseStartTag { self_closing } => {
                if let Some(name) = in_attr.take() {
                    next.tag_attribute_end(&name);
                }
                let name = in_tag.take().unwrap();
                next.tag_start_end(&name);
                if self_closing && !VOID_TAGS.contains(&name) {
                    next.tag_end(&name);
                }
            }
            CallbackEvent::EndTag { name } => {
                // SAFETY: This data comes from a `&str`.
                let name = unsafe { str::from_utf8_unchecked(name) };
                next.tag_end(name);
            }
            CallbackEvent::Comment { value } => {
                // SAFETY: This data comes from a `&str`.
                let value = unsafe { str::from_utf8_unchecked(value) };
                next.comment_start();
                next.text(value);
                next.comment_end();
            }
            CallbackEvent::Doctype { .. } => {}
            CallbackEvent::Error(error) => {
                log::warn!("Tokenizer error: {error}");
            }
        }

        None::<core::convert::Infallible>
    });
    html5gum::Tokenizer::new_with_emitter(html, emitter).finish();
}

/// List emitter.
#[derive(Debug, Default)]
pub(super) struct ListEmitter {
    /// Whether the last open definition list item was a term item.
    in_dt: bool,
    /// The stack of currently open list items.
    stack: Vec<ListKind>,
}
impl ListEmitter {
    /// Returns the length of the least common denominator of `self` and
    /// `bullets`.
    pub fn common(&self, bullets: &str) -> usize {
        Self::count_common(&self.stack, bullets.as_bytes(), |lhs, rhs| lhs == rhs)
    }

    /// Counts the least common denominator of `a` and `b` using a comparator
    /// `f`.
    fn count_common<F: Fn(ListKind, ListKind) -> bool>(a: &[ListKind], b: &[u8], f: F) -> usize {
        a.iter()
            .zip(b)
            .take_while(|(lhs, rhs)| f(**lhs, ListKind::from(**rhs)))
            .count()
    }

    /// Emits the difference between `self` and `bullets` that are not new
    /// bullets.
    pub fn emit_common<S: Sink + ?Sized>(&mut self, next: &mut S, bullets: &str) -> usize {
        let common_end = self.common(bullets);
        let old_len = self.stack.len();

        for item in self.stack.drain(common_end..).rev() {
            item.fixup(&mut self.in_dt, false).end(next, true);
        }

        let bullets = bullets.as_bytes();
        if common_end != 0 {
            let item = ListKind::from(bullets[common_end - 1]);
            if bullets.len() == common_end {
                // Transition between `<li>` and `<li>`, or `<dd>` to `<dt>`
                self.next_item(next, item);
            }
            if self.in_dt && item == ListKind::Detail {
                // Transition out of a `<dt>` and into a `<dd>`
                self.next_item(next, ListKind::Detail);
            }
        }

        if old_len != 0 && bullets.len() > common_end {
            next.new_line();
        }

        common_end
    }

    /// Emits a list item transition at the same depth as the previous list
    /// item.
    pub fn emit_last<S: Sink + ?Sized>(&mut self, next: &mut S, bullets: &str) {
        let item = ListKind::from(bullets.as_bytes()[bullets.len() - 1]);
        self.next_item(next, item);
    }

    /// Emits HTML to finish any incomplete list.
    pub fn finish<S: Sink + ?Sized>(&mut self, next: &mut S) {
        for item in self.stack.drain(..).rev() {
            item.fixup(&mut self.in_dt, false).end(next, true);
        }
    }

    /// Emits the next list item with the kind `item` to `next`.
    fn next_item<S: Sink + ?Sized>(&mut self, next: &mut S, item: ListKind) {
        item.fixup(&mut self.in_dt, true).end(next, false);
        next.new_line();
        item.start(next, false);
    }

    /// Pushes a list `item` to the stack and emits the associated HTML to
    /// `next`.
    pub fn push<S: Sink + ?Sized>(&mut self, next: &mut S, item: u8) {
        let mut item = ListKind::from(item);
        item.start(next, true);
        if item == ListKind::Term {
            self.in_dt = true;
            item = ListKind::Detail;
        }
        self.stack.push(item);
    }

    /// Returns true if `bullets` has all the same list item parents as `self`
    /// (`ol` to `ol`, `ul` to `ul`, `dt` or `dd` to `dl`).
    pub fn same(&mut self, bullets: &str) -> bool {
        let end = Self::count_common(&self.stack, bullets.as_bytes(), ListKind::same_parent);
        end != 0 && end == self.stack.len() && end == bullets.len()
    }
}

/// A list kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum ListKind {
    /// Definition list detail.
    ///
    /// ```wikitext
    /// ; Term : Detail
    ///        ^^^^^^^^
    /// : Definition detail
    /// ^^^^^^^^^^^^^^^^^^^
    /// ```
    Detail = b':',
    /// Ordered list.
    ///
    /// ```wikitext
    /// # Ordered list
    /// ```
    Ordered = b'#',
    /// Definition list term.
    ///
    /// ```wikitext
    /// ; Definition term
    /// ```
    Term = b';',
    /// Unordered list.
    ///
    /// ```wikitext
    /// * Unordered list
    /// ```
    Unordered = b'*',
}

impl ListKind {
    /// Emits HTML for the end of this kind of list item.
    fn end<S: Sink + ?Sized>(self, next: &mut S, end_of_list: bool) {
        match self {
            Self::Detail | Self::Term => {
                next.tag_end(self.tag_name());
                if end_of_list {
                    next.tag_end("dl");
                }
            }
            Self::Ordered | Self::Unordered => {
                next.tag_end("li");
                if end_of_list {
                    next.tag_end(self.tag_name());
                }
            }
        }
    }

    /// Applies cursed voodoo using `in_dt` to convert a definition list item
    /// to the correct subtype.
    fn fixup(self, in_dt: &mut bool, reset: bool) -> Self {
        if self.is_definition_list() {
            if core::mem::replace(in_dt, reset && self == Self::Term) {
                Self::Term
            } else {
                Self::Detail
            }
        } else {
            self
        }
    }

    /// Returns true if `self` is a definition list item.
    #[inline]
    fn is_definition_list(self) -> bool {
        matches!(self, Self::Term | Self::Detail)
    }

    /// Returns true if `self` has the same parent element as `other`.
    #[inline]
    fn same_parent(self, other: Self) -> bool {
        match self {
            Self::Ordered | Self::Unordered => self == other,
            Self::Term | Self::Detail => other.is_definition_list(),
        }
    }

    /// Emits HTML for the start of this kind of list item.
    fn start<S: Sink + ?Sized>(self, next: &mut S, start_of_list: bool) {
        match self {
            Self::Detail | Self::Term => {
                if start_of_list {
                    next.tag_start_full("dl");
                }
                next.tag_start_full(self.tag_name());
            }
            Self::Ordered | Self::Unordered => {
                if start_of_list {
                    next.tag_start_full(self.tag_name());
                }
                next.tag_start_full("li");
            }
        }
    }

    /// The HTML tag for this kind of list item.
    #[inline]
    pub(super) fn tag_name(self) -> &'static str {
        match self {
            ListKind::Ordered => "ol",
            ListKind::Unordered => "ul",
            ListKind::Term => "dt",
            ListKind::Detail => "dd",
        }
    }
}

impl From<u8> for ListKind {
    fn from(value: u8) -> Self {
        match value {
            b'*' => Self::Unordered,
            b'#' => Self::Ordered,
            b';' => Self::Term,
            b':' => Self::Detail,
            _ => unreachable!(),
        }
    }
}

/// A bookmarked position in a string.
#[derive(Debug)]
pub(super) struct Mark(u16);

#[cfg(debug_assertions)]
impl Drop for Mark {
    #[track_caller]
    fn drop(&mut self) {
        if !std::thread::panicking() && self.0 != MarkableString::NO_FREE {
            log::warn!("leaked {}", self.0);
        }
    }
}

/// A string wrapper where positions can be bookmarked and retrieved later. The
/// bookmarked positions are automatically adjusted in response to mutations to
/// the underlying string. To reduce memory use, the size of the underlying
/// string is limited to [`i32::MAX`] bytes, and there can be no more than
/// [`u16::MAX`]`- 1` bookmarks.
///
/// You may be asking yourself: boy, this sure seems janky. Well, that’s not a
/// question. But I understand what you mean. Obviously the ‘pure’ way to do
/// this is to have any of the earlier handlers buffer outputs until they have
/// everything they need. Very good, such computer science, much purity. But
/// it is more efficient (citation needed) to allow everything to flow down to
/// the single final String allocation instead of having a bunch of intermediate
/// buffers.
///
/// Now you might say something like, “well, you know, you could just use a bump
/// allocator and then all your buffers end up in a contiguous allocation and
/// also computers are fast and so it is like not much of a big deal”. And then
/// I would say, well, the code that injected stuff into strings was already
/// written that way, and this was easier than rewriting everything right now.
#[derive(Clone, Debug)]
pub(super) struct MarkableString {
    /// The underlying string buffer.
    inner: String,
    /// The next free mark index, or [`Self::NO_FREE`] if [`marks`](Self::marks)
    /// needs to be resized.
    next_free: u16,
    /// An packed unordered list of marked positions interleaved with a free
    /// list.
    marks: Vec<u32>,
}

impl MarkableString {
    /// Flag for marks that are actually free list entries.
    const FREE_BIT: u32 = 0x8000_0000;
    /// Sentinel value for marks which were invalidated by range deletion.
    const INVALID: u32 = i32::MAX as u32;
    /// Marker for the end of the free list.
    const NO_FREE: u16 = u16::MAX;

    /// Releases the given mark to the free pool.
    // TODO: It is bad that this has to be done manually, marks will leak!
    #[inline]
    pub fn free_mark(&mut self, mut mark: Mark) {
        self.marks[usize::from(mark.0)] = Self::FREE_BIT | u32::from(self.next_free);
        self.next_free = mark.0;
        if cfg!(debug_assertions) {
            mark.0 = Self::NO_FREE;
        }
    }

    /// Inserts a mark at the given position `pos`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "free list entries only want the low word"
    )]
    fn insert_mark(&mut self, pos: u32) -> Mark {
        if self.next_free == Self::NO_FREE {
            let mark = Mark(u16::try_from(self.marks.len()).unwrap());
            assert!(mark.0 < Self::NO_FREE, "too many marks");
            self.marks.push(pos);
            mark
        } else {
            let mark = Mark(self.next_free);
            let slot = &mut self.marks[usize::from(self.next_free)];
            debug_assert!(*slot & Self::FREE_BIT != 0);
            self.next_free = *slot as u16;
            *slot = pos;
            mark
        }
    }

    /// Returns the underlying `String`, consuming this object.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> String {
        self.inner
    }

    /// Returns a mutable iterator over all mark positions above `start`.
    fn iter_marks_mut(&mut self, start: u32) -> impl Iterator<Item = &'_ mut u32> {
        self.marks
            .iter_mut()
            .rev()
            .filter(move |&&mut pos| pos & Self::FREE_BIT == 0 && pos > start)
    }

    /// Returns a new mark corresponding to the current length of the string.
    #[must_use]
    pub fn mark(&mut self) -> Mark {
        let pos = u32::try_from(self.inner.len()).unwrap();
        self.insert_mark(pos)
    }

    /// Moves the string in `range` to `dest`.
    pub fn move_range<R: core::ops::RangeBounds<usize>>(&mut self, range: R, dest: usize) {
        // TODO: core::slice::range is unstable. rust-lang/rust#76393
        let start = match range.start_bound() {
            core::ops::Bound::Included(&start) => start,
            core::ops::Bound::Excluded(&start) => start + 1,
            core::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            core::ops::Bound::Included(&end) => end + 1,
            core::ops::Bound::Excluded(&end) => end,
            core::ops::Bound::Unbounded => self.inner.len(),
        };

        let (start, mid, end) = if dest < start {
            (dest, start, end)
        } else if dest < end {
            (start, end, dest + end - start)
        } else {
            (start, end - start, dest)
        };

        assert!(self.inner.is_char_boundary(start));
        assert!(self.inner.is_char_boundary(end));
        assert!(self.inner.is_char_boundary(dest));
        // SAFETY: All of start, end, and dest are asserted to be on character
        // boundaries.
        (unsafe { self.inner.as_bytes_mut() })[start..end].rotate_left(mid - start);

        let start = u32::try_from(start).unwrap();
        let mid = u32::try_from(mid).unwrap();
        let end = u32::try_from(end).unwrap();
        let delta_left = i32::try_from(end).unwrap().strict_sub_unsigned(mid);
        let delta_right = i32::try_from(start).unwrap().strict_sub_unsigned(mid);
        for pos in self.iter_marks_mut(start).filter(|pos| **pos <= end) {
            *pos = pos
                .checked_add_signed(if *pos > mid { delta_right } else { delta_left })
                .unwrap_or(Self::INVALID);
        }
    }

    /// Appends `ch` to the string.
    #[inline]
    pub fn push(&mut self, ch: char) {
        self.inner.push(ch);
    }

    /// Appends `string` to the string.
    #[inline]
    pub fn push_str(&mut self, string: &str) {
        self.inner.push_str(string);
    }

    /// Returns the byte position of the given `mark` in the string, or
    /// `None` if the bookmarked position was erased by [`Self::remove`] or
    /// [`Self::replace_range`].
    pub fn restore_mark(&self, mark: &Mark) -> Option<usize> {
        self.marks.get(usize::from(mark.0)).and_then(|&pos| {
            assert!(pos & Self::FREE_BIT == 0, "mark use-after-free");
            (pos != Self::INVALID).then_some(pos as usize)
        })
    }
}

impl Default for MarkableString {
    fn default() -> Self {
        Self {
            inner: <_>::default(),
            next_free: Self::NO_FREE,
            marks: <_>::default(),
        }
    }
}

impl<I> core::ops::Index<I> for MarkableString
where
    I: core::slice::SliceIndex<str>,
{
    type Output = <I as core::slice::SliceIndex<str>>::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        self.inner.index(index)
    }
}

impl fmt::Write for MarkableString {
    #[inline]
    fn write_char(&mut self, c: char) -> fmt::Result {
        self.inner.write_char(c)
    }

    #[inline]
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        self.inner.write_fmt(args)
    }

    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.inner.write_str(s)
    }
}

/// Collects an outline entry to be emitted to the table of contents.
///
/// Because MediaWiki allows both Wikitext headings and HTML headings to
/// contribute to the document outline, it is necessary to collect the outline
/// information by consuming HTML instead of Wikitext nodes. MediaWiki also
/// allows a subset of HTML to be injected to the outline.
#[derive(Debug)]
pub(super) struct OutlineEmitter<'a, S: Sink + Markable> {
    /// The buffer for the outline entry.
    buffer: Vec<OutlineEntry>,

    /// If true, processing a strip marker.
    ///
    /// The IDs of heading tags inside of strip markers are supposed to be fixed
    /// up, but they are not supposed to be go in the outline.
    in_strip_marker: bool,

    /// The output.
    next: S,

    /// The global outline.
    outline: &'a mut Outline,

    /// The emitter state.
    state: OutlineState,
}

impl<'a, S: Sink + Markable> OutlineEmitter<'a, S> {
    /// Creates a new `OutlineEmitter` chained to `next`.
    pub fn new(outline: &'a mut Outline, next: S) -> Self {
        Self {
            buffer: <_>::default(),
            in_strip_marker: <_>::default(),
            next,
            outline,
            state: <_>::default(),
        }
    }

    /// Adds the given `text` to the currently processing outline entry.
    fn add_text(&mut self, text: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.text(text);
            outline.entry_html.text(text);
            if self.state == OutlineState::Body
                && let OutlineId::Implicit(id) = &mut outline.id
            {
                id.push_str(text);
            } else if self.state == OutlineState::StartId
                && let OutlineId::Explicit(id) = &mut outline.id
            {
                id.push_str(text);
            }
        }
    }

    /// Saves the given `entry` to the global outline and emits the buffered
    /// HTML to the next sink.
    fn save_entry(&mut self, entry: OutlineEntry) {
        let id = match &entry.id {
            OutlineId::Implicit(id) => normalize_section_name(id).map(normalize_fragment),
            OutlineId::Explicit(id) => Cow::Borrowed(id.as_str()),
        };

        let html_id = escape_id(&id, AnchorEncodeMode::Html5);
        let legacy_id = {
            let id = escape_id(&id, AnchorEncodeMode::Legacy);
            (id != html_id).then(|| id.map(|id| self.outline.unique_id(id)))
        };
        let id = self.outline.unique_id(&html_id);

        if !self.in_strip_marker {
            let html = entry.entry_html.finish();
            self.outline.push(entry.level, html.trim_ascii(), &id);
        }

        let mut html = entry.document_html.finish();

        debug_assert!(entry.body_start > entry.id_start);

        if let Some(legacy_id) = legacy_id {
            html.insert_str(
                entry.body_start as usize,
                &format!(
                    r#"<span id="{}"></span>"#,
                    encode_double_quoted_attribute(&legacy_id)
                ),
            );
        }

        #[rustfmt::skip]
        if let Cow::Owned(id) = &id && let Some(end) = entry.id_end {
            html.replace_range(entry.id_start as usize..u32::from(end) as usize, id);
        } else if entry.id_end.is_none() {
            let id = format!(r#" id="{}""#, encode_double_quoted_attribute(&id));
            html.insert_str(entry.id_start as usize, &id);
        };

        tokenise(&mut self.next, &html);
    }
}

chainable!(OutlineEmitter<'a, S>);

impl<S: Sink + Markable> Sink for OutlineEmitter<'_, S> {
    fn comment_end(&mut self) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.comment_end();
            outline.entry_html.comment_end();
        } else {
            self.next.comment_end();
        }
    }

    fn comment_start(&mut self) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.comment_start();
            outline.entry_html.comment_start();
        } else {
            self.next.comment_start();
        }
    }

    fn entity(&mut self, value: char, raw: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.entity(value, raw);
            outline.entry_html.entity(value, raw);
            if self.state == OutlineState::Body
                && let OutlineId::Implicit(id) = &mut outline.id
            {
                id.push(value);
            } else if self.state == OutlineState::StartId
                && let OutlineId::Explicit(id) = &mut outline.id
            {
                id.push(value);
            }
        } else {
            self.next.entity(value, raw);
        }
    }

    #[inline]
    fn finish(self) -> String {
        debug_assert!(self.buffer.is_empty());
        self.next.finish()
    }

    fn new_line(&mut self) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.new_line();
            outline.entry_html.new_line();
        } else {
            self.next.new_line();
        }
    }

    fn strip_marker(&mut self, marker: &StripMarker) {
        match marker {
            // Even if this is not processing any heading, tokenising is needed
            // by subsequent sinks, so just do it here always
            StripMarker::General(html) => {
                self.in_strip_marker = true;
                tokenise(self, html);
                self.in_strip_marker = false;
            }
            StripMarker::NoWiki(text) => {
                if self.buffer.is_empty() {
                    self.next.strip_marker(marker);
                } else {
                    self.add_text(&decode_html(text));
                }
            }
            StripMarker::WikiRsSourceEnd(_) | StripMarker::WikiRsSourceStart(_) => {
                if let Some(outline) = self.buffer.last_mut() {
                    outline.document_html.strip_marker(marker);
                } else {
                    self.next.strip_marker(marker);
                }
            }
        }
    }

    fn tag_attribute_end(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.tag_attribute_end(name);
            outline.entry_html.tag_attribute_end(name);
            if self.state == OutlineState::StartId {
                outline.id_end = NonZeroU32::new(outline.document_html.len());
                self.state = OutlineState::Start;
            } else if self.state == OutlineState::StartAttr {
                self.state = OutlineState::Start;
            } else {
                self.state = OutlineState::Body;
            }
        } else {
            self.next.tag_attribute_end(name);
        }
    }

    fn tag_attribute_start(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            if self.state == OutlineState::Start {
                if name == "id" {
                    self.state = OutlineState::StartId;
                    outline.id = OutlineId::Explicit(<_>::default());
                    outline.id_start = outline.document_html.len();
                } else {
                    self.state = OutlineState::StartAttr;
                }
            } else {
                self.state = OutlineState::BodyAttr;
            }
            outline.document_html.tag_attribute_start(name);
            outline.entry_html.tag_attribute_start(name);
        } else {
            self.next.tag_attribute_start(name);
        }
    }

    fn tag_end(&mut self, name: &str) {
        if let Some(mut outline) = self.buffer.pop_if(|state| state.level.tag_name() == name) {
            outline.document_html.tag_end(name);
            self.save_entry(outline);
        } else if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.tag_end(name);
            outline.entry_html.tag_end(name);
        } else {
            self.next.tag_end(name);
        }
    }

    fn tag_start(&mut self, name: &str) {
        if let Ok(level) = name.parse() {
            let mut outline = OutlineEntry::new(level);
            outline.document_html.tag_start(name);
            self.buffer.push(outline);
            self.state = OutlineState::Start;
        } else if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.tag_start(name);
            outline.entry_html.tag_start(name);
        } else {
            self.next.tag_start(name);
        }
    }

    fn tag_start_end(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            if self.state == OutlineState::Start && outline.id_end.is_none() {
                outline.id_start = outline.document_html.len();
            }
            outline.document_html.tag_start_end(name);
            if self.state == OutlineState::Start {
                outline.body_start = outline.document_html.len();
                self.state = OutlineState::Body;
            } else {
                outline.entry_html.tag_start_end(name);
            }
        } else {
            self.next.tag_start_end(name);
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.buffer.is_empty() {
            self.next.text(text);
        } else {
            self.add_text(text);
        }
    }
}

/// An outline entry.
#[derive(Debug)]
struct OutlineEntry {
    /// The position immediately after the heading tag in the document HTML.
    body_start: u32,
    /// The accumulator for the heading tag to be emitted to the document
    /// output.
    document_html: Accumulator,
    /// The accumulator for the outline entry to be emitted to the outline.
    entry_html: OutlineEntryBody,
    /// The plain text anchor ID for the outline.
    id: OutlineId,
    /// The start position of the ID in the document HTML.
    id_start: u32,
    /// The end position of the ID in the document HTML. If `None`, the ID was
    /// implicit, and needs to be inserted.
    id_end: Option<NonZeroU32>,
    /// The level of the heading tag.
    level: HeadingLevel,
}

impl OutlineEntry {
    /// Creates a new `OutlineEntry`.
    fn new(level: HeadingLevel) -> Self {
        Self {
            body_start: <_>::default(),
            document_html: <_>::default(),
            entry_html: <_>::default(),
            id: <_>::default(),
            id_start: <_>::default(),
            id_end: <_>::default(),
            level,
        }
    }
}

/// Accumulates the HTML for an outline entry, filtering tags (but not their
/// contents) and most tag attributes.
#[derive(Debug)]
struct OutlineEntryBody {
    /// The last tag’s inner body position. Used to detect empty tags.
    body_pos: u32,
    /// The accumulator for the outline entry.
    buffer: PrettyText<Accumulator>,
    /// The filter counter.
    filtering: u8,
    /// If true, processing a `<span>` start tag.
    in_span: bool,
    /// The last tag’s outer start position. Used to truncate empty tags.
    start_pos: u32,
}

impl OutlineEntryBody {
    /// The list of tags allowed in outlines.
    ///
    /// This list comes from Parsoid `Wt2Html\DOM\Handlers\Headings`.
    const ALLOWED_TAGS: phf::Set<&str> = phf::phf_set! {
        "b", "bdi", "i", "q", "s", "span", "strike", "sub", "sup"
    };
}

impl Default for OutlineEntryBody {
    fn default() -> Self {
        Self {
            body_pos: <_>::default(),
            buffer: PrettyText::new(<_>::default()),
            filtering: <_>::default(),
            in_span: <_>::default(),
            start_pos: <_>::default(),
        }
    }
}

impl Sink for OutlineEntryBody {
    #[inline]
    fn comment_end(&mut self) {}

    #[inline]
    fn comment_start(&mut self) {}

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.filtering == 0 {
            self.buffer.entity(value, raw);
        }
    }

    #[inline]
    fn finish(self) -> String {
        self.buffer.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        // Because the legacy ID mode is sensitive to newline characters, this
        // has to be emitted, even though it is nonsensical in the normal
        // context of an outline entry
        if self.filtering == 0 {
            self.buffer.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, _: &StripMarker) {
        panic!("strip markers should not be sent here");
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if self.in_span && name == "dir" {
            self.buffer.tag_attribute_end(name);
        } else {
            self.filtering -= 1;
        }
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if self.in_span && name == "dir" {
            self.buffer.tag_attribute_start(name);
        } else {
            self.filtering += 1;
        }
    }

    fn tag_end(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            // If multiple nested tags are empty, the inner end tag will
            // truncate back to `start_pos` (`body_pos == len`), and then the
            // second one also needs to be suppressed (`body_pos > len`)
            if self.body_pos == self.buffer.next().len() {
                // Empty tags are filtered out
                self.buffer.next_mut().truncate(self.start_pos);
            } else if self.body_pos < self.buffer.next().len() {
                self.buffer.tag_end(name);
            }
        }
    }

    fn tag_start(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            let pos = self.buffer.next().len();
            // If multiple nested tags are empty, they should be all be removed,
            // not just the innermost one
            if pos != self.body_pos {
                self.start_pos = self.buffer.next().len();
            }
            self.buffer.tag_start(name);
            self.in_span = name == "span";
        } else {
            self.filtering += 1;
        }
    }

    fn tag_start_end(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            self.buffer.tag_start_end(name);
            self.body_pos = self.buffer.next().len();
        } else {
            self.filtering -= 1;
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.filtering == 0 {
            self.buffer.text(text);
        }
    }
}

/// An outline entry ID.
#[derive(Debug)]
enum OutlineId {
    /// The ID is generated implicitly from the body of the heading.
    Implicit(String),
    /// The ID is taken explicitly from an `id` attribute.
    Explicit(String),
}

impl Default for OutlineId {
    fn default() -> Self {
        Self::Implicit(<_>::default())
    }
}

/// The state of an `OutlineEmitter`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum OutlineState {
    /// In some other tag state.
    #[default]
    Body,
    /// In an attribute for some other tag.
    BodyAttr,
    /// In a new `<hN>` start tag.
    Start,
    /// In a new `<hN>` stat tag attribute.
    StartAttr,
    /// In the `id` attribute of a new `<hN>` start tag.
    StartId,
}

/// Text style emitter.
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum TextStyleEmitter {
    /// No current style.
    #[default]
    None,
    /// Current style is bold.
    B,
    /// Current style is italic nested in bold.
    BI,
    /// Current style is italic.
    I,
    /// Current style is bold nested in italic.
    IB,
}

impl TextStyleEmitter {
    /// Emits HTML to match the new state given by `style`.
    pub fn emit<S: Sink + ?Sized>(&mut self, next: &mut S, style: TextStyle) {
        // Because I don’t care and we aren’t buffering tags, this does not
        // bother with the pedantic attempt to avoid extra formatting tags by
        // recording the position of a None -> BoldItalic transition and then
        // only emitting once the next tag shows up so that it is known whether
        // the order should be BI or IB. Instead we just emit BI and suffer the
        // consequences of emitting a whole extra tag later if it should’ve been
        // IB (which, technically, because the HTML5 spec has defined rules
        // about fixing mismatched tags, it does not even really matter if they
        // are emitted in order).
        *self = match style {
            TextStyle::Bold(..) => match self {
                Self::B => {
                    next.tag_end("b");
                    Self::None
                }
                Self::BI => {
                    next.tag_end("i");
                    next.tag_end("b");
                    next.tag_start_full("i");
                    Self::I
                }
                Self::None => {
                    next.tag_start_full("b");
                    Self::B
                }
                Self::I => {
                    next.tag_start_full("b");
                    Self::IB
                }
                Self::IB => {
                    next.tag_end("b");
                    Self::I
                }
            },
            TextStyle::BoldItalic => match self {
                Self::None => {
                    next.tag_start_full("i");
                    next.tag_start_full("b");
                    Self::IB
                }
                Self::B => {
                    next.tag_end("b");
                    next.tag_start_full("i");
                    Self::I
                }
                Self::BI => {
                    next.tag_end("i");
                    next.tag_end("b");
                    Self::None
                }
                Self::I => {
                    next.tag_end("i");
                    next.tag_start_full("b");
                    Self::B
                }
                Self::IB => {
                    next.tag_end("b");
                    next.tag_end("i");
                    Self::None
                }
            },
            TextStyle::Italic => match self {
                Self::None => {
                    next.tag_start_full("i");
                    Self::I
                }
                Self::B => {
                    next.tag_start_full("i");
                    Self::BI
                }
                Self::BI => {
                    next.tag_end("i");
                    Self::B
                }
                Self::I => {
                    next.tag_end("i");
                    Self::None
                }
                Self::IB => {
                    next.tag_end("b");
                    next.tag_end("i");
                    next.tag_start_full("b");
                    Self::B
                }
            },
        };
    }

    /// Emits HTML to finish any incomplete style.
    pub fn finish<S: Sink + ?Sized>(&mut self, next: &mut S) {
        match self {
            Self::None => {}
            Self::B => next.tag_end("b"),
            Self::BI => {
                next.tag_end("i");
                next.tag_end("b");
            }
            Self::I => next.tag_end("i"),
            Self::IB => {
                next.tag_end("b");
                next.tag_end("i");
            }
        }
        *self = Self::None;
    }
}

/// Wraps bare text content in the root and in `<blockquote>` elements with a
/// `<p>`.
#[derive(Debug)]
pub(super) struct PWrapper<S: Sink> {
    /// The current DOM depth.
    depth: u8,
    /// If true, inside a p-wrapper.
    in_p: bool,
    /// The output.
    next: S,
    /// P-wrapper root depths.
    roots: Vec<u8>,
    /// The whitespace buffer for a potentially empty element.
    ws_buffer: Option<String>,
}

chainable!(PWrapper);

impl<S: Sink> PWrapper<S> {
    /// Creates a new `PWrapper` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            depth: <_>::default(),
            in_p: <_>::default(),
            next,
            roots: vec![0],
            ws_buffer: <_>::default(),
        }
    }

    /// Enters a graf wrapper if it is appropriate at the current DOM position.
    fn enter_p(&mut self) {
        if self.should_enter_p() {
            self.in_p = true;
            self.next.tag_start_full("p");
            self.flush();
        }
    }

    /// Exits a graf wrapper.
    fn exit_p(&mut self) {
        self.flush();
        if self.in_p {
            self.next.tag_end("p");
            self.in_p = false;
        }
    }

    /// Writes the buffered tag to the next sink.
    fn flush(&mut self) {
        if let Some(ws) = self.ws_buffer.take() {
            flush_ws(&mut self.next, &ws);
        }
    }

    /// Returns true if the emitter is in a state for inserting a graf wrapper.
    fn should_enter_p(&self) -> bool {
        !self.in_p && Some(&self.depth) == self.roots.last()
    }
}

impl<S: Sink> Sink for PWrapper<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.flush();
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.flush();
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.enter_p();
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(mut self) -> String {
        self.exit_p();
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.should_enter_p() {
            self.ws_buffer.get_or_insert_default().push('\n');
        } else {
            self.next.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        // TODO: Strip markers should be unstripped before here.
        log::warn!("Late strip marker");
        self.next.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        if name == "blockquote" {
            self.exit_p();
            self.roots.pop();
        } else if name == "p" {
            self.in_p = false;
        }
        self.next.tag_end(name);
        self.depth -= 1;
    }

    fn tag_start(&mut self, name: &str) {
        if Tag::known(name).is_some_and(Tag::is_inline) {
            self.enter_p();
        } else {
            self.exit_p();
        }
        self.next.tag_start(name);
        self.depth += 1;
        if name == "blockquote" {
            self.roots.push(self.depth);
        } else if name == "p" {
            self.in_p = true;
        }
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.should_enter_p() && text.bytes().all(|c| c.is_ascii_whitespace()) {
            *self.ws_buffer.get_or_insert_default() += text;
        } else {
            self.enter_p();
            self.next.text(text);
        }
    }
}

/// A DOM pseudo-node.
#[derive(Clone, Copy, Debug, Eq)]
enum TagNode {
    /// An HTML tag.
    Html(Tag),
    /// An implicit `<tbody>` that is not being emitted because it is a waste.
    ImplicitTbody,
    /// A marker on the “list of active formatting elements”.
    Marker(Option<NonZeroU8>),
    /// An optimised `<p>` element that holds a niche-optimised index of the
    /// previous `<p>` element in [`DomTree::stack`], if any.
    P(Option<NonZeroU8>),
}

impl From<Tag> for TagNode {
    #[inline]
    fn from(tag: Tag) -> Self {
        Self::Html(tag)
    }
}

impl PartialEq<Tag> for TagNode {
    #[inline]
    fn eq(&self, other: &Tag) -> bool {
        self.tag() == Some(*other)
    }
}

impl TagNode {
    /// Emits the close tag for this node to `next`, using the given set of
    /// `custom` tag names, and updating the `next_index` if applicable.
    fn close<S: Sink + ?Sized>(
        self,
        next: &mut S,
        custom: &IndexSet<Uncased<'static>>,
        next_index: &mut Option<u8>,
    ) {
        if let Some(name) = self.name(custom) {
            debug_assert!(!VOID_TAGS.contains(name.as_str()));
            next.tag_end(name.as_str());
            if let Self::P(next) | Self::Marker(next) = self {
                *next_index = next.map(|index| u8::from(index) - 1);
            }
        }
    }

    /// Returns the tag name of this node, or `None` if this is an anonymous
    /// marker node.
    fn name<'a>(self, custom: &'a IndexSet<Uncased<'static>>) -> Option<&'a UncasedStr> {
        match self {
            Self::Html(tag) => Some(tag.as_str(custom)),
            Self::ImplicitTbody | Self::Marker(_) => None,
            Self::P(_) => Some(UncasedStr::new("p")),
        }
    }

    /// Returns true if this is a definition list child.
    #[inline]
    fn is_dl_item(self) -> bool {
        self.tag().is_some_and(Tag::is_dl_item)
    }

    /// Returns true if this is an HTML heading node.
    #[inline]
    fn is_heading(self) -> bool {
        self.tag().is_some_and(Tag::is_heading)
    }

    /// Returns true if this node has an implied end tag, `except` not that one.
    #[inline]
    fn is_implied_close(self, except: Option<Tag>) -> bool {
        self.tag()
            .is_some_and(|tag| tag.is_implied_end() && Some(tag) != except)
    }

    /// Returns true if this is a “special category” node.
    #[inline]
    fn is_special(self) -> bool {
        self.tag().is_some_and(Tag::is_special)
    }

    /// Returns true if this is a `<table>` direct child.
    #[inline]
    fn is_table_body(self) -> bool {
        matches!(self, Self::ImplicitTbody) || self.tag().is_some_and(Tag::is_table_body)
    }

    /// Returns the corresponding HTML5 tag for this node, or `None` if this is
    /// an anonymous marker node.
    fn tag(self) -> Option<Tag> {
        match self {
            Self::Html(tag) => Some(tag),
            Self::ImplicitTbody => Some(Tag::Tbody),
            Self::Marker(_) => None,
            Self::P(_) => Some(Tag::P),
        }
    }
}

impl PartialEq for TagNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Html(lhs), Self::Html(rhs)) => lhs == rhs,
            (Self::Marker(_), Self::Marker(_)) | (Self::P(_), Self::P(_)) => true,
            (Self::Html(tag), Self::P(_)) | (Self::P(_), Self::Html(tag)) => *tag == Tag::P,
            (Self::Html(tag), Self::ImplicitTbody) | (Self::ImplicitTbody, Self::Html(tag)) => {
                *tag == Tag::Tbody
            }
            _ => false,
        }
    }
}

/// Generates the `Tag` enum and lookup table for known HTML5 tag names.
macro_rules! tags {
    ($($tag:literal => $id:ident),* $(,)?) => {
        /// An HTML tag.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Tag {
            $($id,)*
            /// A custom tag index.
            Custom(u8),
        }

        /// The lookup table for known HTML tags.
        static KNOWN_TAGS: phf::Map<&UncasedStr, Tag> = phf::phf_map! {
            $(UncasedStr::new($tag) => Tag::$id,)*
        };

        impl Tag {
            /// Returns the tag as a string.
            fn as_str<'a>(self, custom: &'a IndexSet<Uncased<'static>>) -> &'a UncasedStr {
                match self {
                    $(Self::$id => UncasedStr::new($tag),)*
                    Self::Custom(index) => custom.get_index(index.into()).unwrap(),
                }
            }
        }
    }
}

// The list of tags used here is the list of allowed Wikitext tags, plus tags
// that are special in the HTML5 tree construction algorithm and are emitted by
// extension tags
tags! {
    "a" => A, "abbr" => Abbr, "acronym" => Acronym, "address" => Address, "annotation-xml" => AnnotationXml, "aside" => Aside, "audio" => Audio,
    "b" => B, "basefont" => Basefont, "bdi" => Bdi, "bdo" => Bdo, "big" => Big, "button" => Button,
    "blockquote" => Blockquote, "br" => Br,
    "caption" => Caption, "center" => Center, "cite" => Cite, "code" => Code, "col" => Col, "colgroup" => Colgroup,
    "data" => Data, "dd" => Dd, "del" => Del, "desc" => Desc, "details" => Details, "dfn" => Dfn, "div" => Div, "dl" => Dl, "dt" => Dt,
    "em" => Em,
    "figcaption" => Figcaption, "figure" => Figure, "font" => Font, "foreignObject" => ForeignObject, "form" => Form,
    "h1" => H1, "h2" => H2, "h3" => H3, "h4" => H4, "h5" => H5, "h6" => H6, "hr" => Hr,
    "i" => I, "iframe" => Iframe, "img" => Img, "input" => Input, "ins" => Ins,
    "kbd" => Kbd,
    "label" => Label, "legend" => Legend, "li" => Li, "link" => Link,
    "map" => Map, "mark" => Mark, "math" => Math, "meta" => Meta, "mi" => Mi, "mo" => Mo, "mn" => Mn, "ms" => Ms, "mtext" => Mtext,
    "nobr" => Nobr,
    "object" => Object, "ol" => Ol, "optgroup" => Optgroup, "option" => Option,
    "p" => P, "param" => Param, "pre" => Pre,
    "q" => Q,
    "rb" => Rb, "rbc" => Rbc, "rp" => Rp, "rt" => Rt, "rtc" => Rtc, "ruby" => Ruby,
    "s" => S, "samp" => Samp, "select" => Select, "small" => Small, "source" => Source, "span" => Span, "strike" => Strike, "strong" => Strong, "sub" => Sub, "summary" => Summary, "sup" => Sup, "style" => Style, "svg" => Svg,
    "table" => Table, "tbody" => Tbody, "td" => Td, "textarea" => Textarea, "tfoot" => Tfoot, "th" => Th, "thead" => Thead, "time" => Time, "title" => Title, "tr" => Tr, "track" => Track, "tt" => Tt,
    "u" => U, "ul" => Ul,
    "var" => Var, "video" => Video,
    "wbr" => Wbr,
}

impl Tag {
    /// Create a new `Tag` for the known `name`, or `None` if `name` is not
    /// a known HTML5 tag.
    #[inline]
    fn known(name: &str) -> Option<Self> {
        KNOWN_TAGS.get(name.into()).copied()
    }

    /// Creates a new `Tag` with the given `name`. If the name is not a known
    /// HTML5 tag, a custom tag will be used or created in `custom`.
    fn new(name: &str, custom: &mut IndexSet<Uncased<'static>>) -> Self {
        if let Some(tag) = Self::known(name) {
            tag
        } else if let Some(index) = custom.get_index_of(UncasedStr::new(name)) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "if there are more than u8::MAX custom tags, this would have panicked during the insert"
            )]
            Tag::Custom(index as u8)
        } else {
            let index = custom.len();
            custom.insert(Uncased::from_borrowed(name).into_owned());
            Tag::Custom(index.try_into().unwrap())
        }
    }

    /// Returns true if this is a block-level start tag. (This is not a defined
    /// category in HTML5, just a long list of ad-hoc tag names.)
    #[inline]
    fn is_body_block(self) -> bool {
        matches!(
            self,
            Self::Address
                | Self::Aside
                | Self::Center
                | Self::Details
                | Self::Div
                | Self::Dl
                | Self::Figcaption
                | Self::Figure
                | Self::Ol
                | Self::P
                | Self::Summary
                | Self::Ul
        )
    }

    /// Returns true if this is an “element in button scope”.
    #[inline]
    fn is_button_scope(self) -> bool {
        self.is_general_scope() || matches!(self, Self::Button)
    }

    /// Returns true if this is a definition list item.
    #[inline]
    fn is_dl_item(self) -> bool {
        matches!(self, Self::Dd | Self::Dt)
    }

    /// Returns true if this is a tag in the HTML5 formatting category.
    #[inline]
    fn is_formatting(self) -> bool {
        matches!(
            self,
            Self::A
                | Self::B
                | Self::Big
                | Self::Code
                | Self::Em
                | Self::Font
                | Self::I
                | Self::Nobr
                | Self::S
                | Self::Small
                | Self::Strike
                | Self::Strong
                | Self::Tt
                | Self::U
        )
    }

    /// Returns true if this is an “element in scope”.
    #[inline]
    fn is_general_scope(self) -> bool {
        // Ignoring applet, html, marquee, and template
        matches!(
            self,
            Self::AnnotationXml
                | Self::Caption
                | Self::Desc
                | Self::ForeignObject
                | Self::Mi
                | Self::Mo
                | Self::Mn
                | Self::Ms
                | Self::Mtext
                | Self::Object
                | Self::Select
                | Self::Table
                | Self::Td
                | Self::Th
                | Self::Title
        )
    }

    /// Returns true if this is a `<head>` item.
    #[inline]
    fn is_head_item(self) -> bool {
        matches!(
            self,
            Self::Basefont | Self::Link | Self::Meta | Self::Style | Self::Title
        )
    }

    /// Returns true if this is a heading element.
    #[inline]
    fn is_heading(self) -> bool {
        matches!(
            self,
            Self::H1 | Self::H2 | Self::H3 | Self::H4 | Self::H5 | Self::H6
        )
    }

    /// Returns true if this is a tag with an implied end tag.
    #[inline]
    fn is_implied_end(self) -> bool {
        matches!(
            self,
            Self::Dd
                | Self::Dt
                | Self::Li
                | Self::Optgroup
                | Self::Option
                | Self::P
                | Self::Rb
                | Self::Rp
                | Self::Rt
                | Self::Rtc
        )
    }

    /// Returns true if this is an “inline” tag, according to MediaWiki’s
    /// `RemexCompatMunger`.
    fn is_inline(self) -> bool {
        matches!(
            self,
            Self::A
                | Self::Abbr
                | Self::Acronym
                | Self::Audio
                | Self::B
                | Self::Basefont
                | Self::Bdi
                | Self::Bdo
                | Self::Big
                | Self::Br
                | Self::Button
                | Self::Cite
                | Self::Code
                | Self::Data
                | Self::Del
                | Self::Dfn
                | Self::Em
                | Self::Font
                | Self::I
                | Self::Iframe
                | Self::Img
                | Self::Input
                | Self::Ins
                | Self::Kbd
                | Self::Label
                | Self::Legend
                | Self::Map
                | Self::Mark
                | Self::Object
                | Self::Param
                | Self::Q
                | Self::Rb
                | Self::Rbc
                | Self::Rp
                | Self::Rt
                | Self::Rtc
                | Self::Ruby
                | Self::S
                | Self::Samp
                | Self::Select
                | Self::Small
                | Self::Source
                | Self::Span
                | Self::Strike
                | Self::Strong
                | Self::Sub
                | Self::Sup
                | Self::Textarea
                | Self::Time
                | Self::Track
                | Self::Tt
                | Self::U
                | Self::Var
                | Self::Video
                | Self::Wbr
        )
    }

    /// Returns true if this is an “element in list item scope”.
    #[inline]
    fn is_list_item_scope(self) -> bool {
        self.is_general_scope() || matches!(self, Self::Ol | Self::Ul)
    }

    /// Returns true if this “is in the special category, but is not an address,
    /// div, or p element”.
    #[inline]
    fn is_list_special(self) -> bool {
        self.is_special() && !matches!(self, Self::Address | Self::Div | Self::P)
    }

    /// Returns true if this is a `<ruby>` item.
    #[inline]
    fn is_ruby_item(self) -> bool {
        matches!(self, Self::Rb | Self::Rp | Self::Rt | Self::Rtc)
    }

    /// Returns true if this tag is in the “special” category.
    fn is_special(self) -> bool {
        // Ignoring applet, area, article, base, bgsound, body, dir, embed,
        // fieldset, footer, frame, frameset, head, header, hgroup, html,
        // keygen, listing, main, marquee, menu, nav, noembed, noframes,
        // noscript, plaintext, script, search, section, template, and xmp,
        // which are unsupported in this implementation
        matches!(
            self,
            Self::Address
                | Self::Aside
                | Self::Basefont
                | Self::Blockquote
                | Self::Br
                | Self::Button
                | Self::Caption
                | Self::Center
                | Self::Col
                | Self::Colgroup
                | Self::Dd
                | Self::Details
                | Self::Div
                | Self::Dl
                | Self::Dt
                | Self::Figcaption
                | Self::Figure
                | Self::Form
                | Self::H1
                | Self::H2
                | Self::H3
                | Self::H4
                | Self::H5
                | Self::H6
                | Self::Hr
                | Self::Iframe
                | Self::Img
                | Self::Input
                | Self::Li
                | Self::Link
                | Self::Meta
                | Self::Object
                | Self::Ol
                | Self::P
                | Self::Param
                | Self::Pre
                | Self::Select
                | Self::Source
                | Self::Style
                | Self::Summary
                | Self::Table
                | Self::Tbody
                | Self::Td
                | Self::Textarea
                | Self::Tfoot
                | Self::Th
                | Self::Thead
                | Self::Title
                | Self::Tr
                | Self::Track
                | Self::Ul
                | Self::Wbr
                | Self::Mi
                | Self::Mo
                | Self::Mn
                | Self::Ms
                | Self::Mtext
                | Self::AnnotationXml
                | Self::ForeignObject
                | Self::Desc
        )
    }

    /// Returns true if this is a `<table>` direct child.
    #[inline]
    fn is_table_body(self) -> bool {
        matches!(self, Self::Tbody | Self::Tfoot | Self::Thead)
    }

    /// Returns true if this is a `<table>` element that cannot contain most
    /// non-table content.
    #[inline]
    fn is_table_fosterable(self) -> bool {
        self.is_table_body() || matches!(self, Self::Table | Self::Tr)
    }

    /// Returns true if this is a `<table>` item (including grandchildren).
    #[inline]
    fn is_table_item(self) -> bool {
        matches!(
            self,
            Self::Caption
                | Self::Col
                | Self::Colgroup
                | Self::Tbody
                | Self::Td
                | Self::Tfoot
                | Self::Th
                | Self::Thead
                | Self::Tr
        )
    }

    /// Returns true if this is an “element in table scope”.
    #[inline]
    fn is_table_scope(self) -> bool {
        // Ignoring html and template, which are unsupported
        matches!(self, Self::Table)
    }
}

/// Converts runs of text to typographically beautiful HTML.
#[derive(Debug)]
pub(super) struct PrettyText<S: Sink> {
    /// The current number of code contexts.
    ///
    /// Pretty typography does not apply in code contexts.
    in_code: u8,
    /// The output.
    next: S,
    /// The previous characters.
    ///
    /// This is used to determine the correct context for the next character
    /// and it is required to be a 2-character look-behind because MediaWiki
    /// does special stuff for “French spaces”.
    prev_chars: [char; 2],
    /// Saved contexts.
    ///
    /// Context switches occur on input to an attribute, since the content of
    /// an attribute is displayed out of the flow of the rest of the document.
    saved_contexts: Vec<[char; 2]>,
}

chainable!(PrettyText);

impl<S: Sink> PrettyText<S> {
    /// Creates a new `PrettyText` chained to `next`.
    #[inline]
    pub fn new(next: S) -> Self {
        Self {
            in_code: 0,
            next,
            prev_chars: Self::new_context(),
            saved_contexts: <_>::default(),
        }
    }

    /// Returns `true` if the `prev` or `next` characters indicate a word
    /// boundary.
    fn is_break(prev: char, next: Option<char>) -> bool {
        use unicode_general_category::{
            GeneralCategory::{
                DashPunctuation, InitialPunctuation, MathSymbol, OpenPunctuation, OtherPunctuation,
            },
            get_general_category,
        };
        prev.is_whitespace()
            || (matches!(
                get_general_category(prev),
                DashPunctuation | OpenPunctuation | InitialPunctuation
            ) && !next.is_some_and(char::is_whitespace))
            || (matches!(get_general_category(prev), MathSymbol | OtherPunctuation)
                && next.is_some_and(char::is_alphabetic))
    }

    /// Returns `true` if the given tag name is a code tag.
    #[inline]
    fn is_code_tag(name: &str) -> bool {
        matches!(name, "code" | "kbd" | "pre" | "samp" | "var")
    }

    /// Returns `Some(true)` if the character sequence requires a non-breaking
    /// space, or `None` if there is not enough information to determine whether
    /// it is required.
    #[inline]
    fn is_french_space(
        [second, first]: [char; 2],
        next: (Option<char>, Option<char>),
    ) -> Option<bool> {
        if matches!(first, '«' | '‹') && !second.is_alphabetic() {
            Some(true)
        } else {
            // Test 'TOC with french spacing (T324763)' suggests that if there
            // is nothing at the second position, then it should be treated as
            // if it is non-alphabetic
            let (first, second) = next;
            first.map(|first| {
                matches!(first, '?' | ':' | ';' | '!' | '%' | '»' | '›')
                    && second.is_none_or(|second| !second.is_alphabetic())
            })
        }
    }

    /// Returns the default previous character context.
    #[inline]
    const fn new_context() -> [char; 2] {
        ['\n', '\n']
    }

    /// Pop a look-behind buffer from the stack.
    #[inline]
    fn pop_context(&mut self) {
        self.prev_chars = self
            .saved_contexts
            .pop()
            .expect("symmetrical context stack");
    }

    /// Push the current look-behind buffer to the stack.
    #[inline]
    fn push_context(&mut self) {
        self.saved_contexts.push(self.prev_chars);
        self.prev_chars = Self::new_context();
    }

    /// Push the given `value` to the look-behind buffer.
    #[inline]
    fn push_char(&mut self, value: char) {
        self.prev_chars[0] = self.prev_chars[1];
        self.prev_chars[1] = value;
    }
}

impl<S: Sink> Sink for PrettyText<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.in_code -= 1;
        self.pop_context();
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.in_code += 1;
        self.push_context();
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.push_char(value);
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.push_char('\n');
        self.next.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        if let StripMarker::NoWiki(text) = marker {
            self.text(&decode_html(text));
        } else {
            self.next.strip_marker(marker);
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if name != "title" {
            self.in_code -= 1;
        }
        self.pop_context();
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if name != "title" {
            self.in_code += 1;
        }
        self.push_context();
        self.next.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        // TODO: PrettyText is used in GrafEmitter buffer without the DOM
        // balancing, this breaks things.
        self.in_code = self
            .in_code
            .saturating_sub(u8::from(Self::is_code_tag(name)));
        if !PHRASING_TAGS.contains(name) {
            self.push_char(' ');
        }
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        if name == "br" || name == "hr" {
            self.push_char('\n');
        }
        self.in_code += u8::from(Self::is_code_tag(name));
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        #[inline]
        fn flush<S: Sink + ?Sized>(
            next: &mut S,
            flushed: &mut usize,
            text: &str,
            index: usize,
            c: char,
        ) {
            if index != *flushed {
                next.text(&text[*flushed..index]);
            }
            *flushed = index + c.len_utf8();
        }

        #[inline]
        fn peek_array<I: Iterator<Item = (usize, char)>>(
            iter: &mut peeknth::SizedPeekN<I, 2>,
        ) -> (Option<char>, Option<char>) {
            let first = iter.peek().map(|(_, first)| *first);
            let second = iter.peek_nth(1).map(|(_, second)| *second);
            (first, second)
        }

        let mut chars = peeknth::sizedpeekn::<_, 2>(text.char_indices());

        let mut flushed = 0;
        while let Some((index, mut c)) = chars.next() {
            match c {
                // If full stops split across runs of text, it is reasonable to
                // assume that they were not designed to combine
                '.' if self.in_code == 0 && peek_array(&mut chars) == (Some('.'), Some('.')) => {
                    flush(&mut self.next, &mut flushed, text, index, c);
                    flushed += 2;
                    chars.clear_peeked();
                    self.next.text("…");
                    c = '…';
                }
                // TODO: Mark the source for a retry later if `None` returns.
                ' ' if Self::is_french_space(self.prev_chars, peek_array(&mut chars))
                    == Some(true) =>
                {
                    flush(&mut self.next, &mut flushed, text, index, c);
                    self.next.text("\u{00a0}");
                }
                // TODO: Track balance to differentiate between e.g.
                // `The ‘90s’` vs `In the ’90s` and other pathological cases
                // TODO: Escape plain '"' inside of an attribute
                '"' | '\'' if self.in_code == 0 => {
                    let next = chars.peek().map(|(_, c)| *c);
                    let break_before = Self::is_break(self.prev_chars[1], next);
                    let double = if c == '"' { 0 } else { 2 };
                    flush(&mut self.next, &mut flushed, text, index, c);
                    self.next
                        .text(["”", "“", "’", "‘"][double + usize::from(break_before)]);
                }
                '&' | '"' | '<' | '>' => {
                    flush(&mut self.next, &mut flushed, text, index, c);
                    self.next.entity(
                        c,
                        match c {
                            '&' => "&amp;",
                            '"' => "&quot;",
                            '<' => "&lt;",
                            '>' => "&gt;",
                            _ => unreachable!(),
                        },
                    );
                }
                _ => {}
            }
            self.push_char(c);
        }

        if flushed != text.len() {
            self.next.text(&text[flushed..]);
        }
    }
}

/// Ejects content from inside tables to outside tables.
///
/// This is, strictly speaking, unnecessary. Browsers all follow the HTML5 spec
/// from which this behaviour derives. However, to satisfy the MW test suite
/// without going insane, fostering is also implemented in the renderer.
#[derive(Debug)]
pub(super) struct TableFoster<S: Sink + Markable> {
    /// The output.
    next: S,
    /// The position just before the nearest table. Since tables can be nested,
    /// this is a stack.
    stack: Vec<TableFosterFrame>,
}

/// A currently processing table.
#[derive(Debug)]
struct TableFosterFrame {
    /// The position just before the nearest table.
    before_table: Mark,
    /// The starting position of the currently processing space between table
    /// children.
    interstitial: Option<Mark>,
}

impl<S: Sink + Markable> TableFoster<S> {
    /// Creates a new `TableFoster` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            next,
            stack: <_>::default(),
        }
    }

    /// Gives up the child to the state.
    fn foster(&mut self) {
        if let Some(TableFosterFrame {
            before_table: before,
            interstitial,
        }) = self.stack.last_mut()
            && let Some(start) = interstitial.take()
        {
            let end = self.next.mark();
            self.next
                .with_marks([before, &start, &end], |[before, start, end], out| {
                    if let (Some(before), Some(start), Some(end)) = (before, start, end)
                        && start != end
                        && out[start..end].bytes().any(|c| !c.is_ascii_whitespace())
                    {
                        out.move_range(start..end, before);
                    }
                });
            self.next.free_mark(start);
            self.next.free_mark(end);
        }
    }
}

chainable!(TableFoster);

impl<S: Sink + Markable> Sink for TableFoster<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        debug_assert!(self.stack.is_empty());
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        if !self.stack.is_empty() {
            log::warn!("TODO: TableFoster strip marker");
        }
        self.next.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.next.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        if matches!(name, "table" | "tbody" | "tfoot" | "thead" | "tr") {
            self.foster();
        }
        if name == "table" {
            let last = self.stack.pop().expect("table mark");
            self.next.free_mark(last.before_table);
        }
        self.next.tag_end(name);
        if matches!(
            name,
            "caption" | "tbody" | "tfoot" | "td" | "th" | "thead" | "tr"
        ) && let Some(last) = self.stack.last_mut()
        {
            debug_assert!(last.interstitial.is_none());
            last.interstitial = Some(self.next.mark());
        }
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        if name == "table" {
            self.stack.push(TableFosterFrame {
                before_table: self.next.mark(),
                interstitial: None,
            });
        } else if matches!(
            name,
            "caption" | "tbody" | "td" | "tfoot" | "th" | "thead" | "tr"
        ) {
            self.foster();
        }
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
        if matches!(name, "table" | "tbody" | "tfoot" | "thead" | "tr")
            && let Some(last) = self.stack.last_mut()
        {
            debug_assert!(last.interstitial.is_none(), "oops, {name}");
            last.interstitial = Some(self.next.mark());
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.next.text(text);
    }
}

/// Adds extra `data-wiki-rs` attributes to the root elements of anonymous
/// templates so they can be identified and styled.
#[derive(Debug)]
pub(super) struct TemplateTagger<S: Sink> {
    /// The current depth of the DOM tree.
    depth: u8,
    /// The output.
    next: S,
    /// The template processing stack used to identify which template was the
    /// source of a fragment of the assembled Wikitext document.
    ///
    /// This is a workaround for templates that do not identify themselves for
    /// styling but instead only emit inline styles (like
    /// 'Template:Climate chart'), which need to have their styles overridden
    /// nevertheless, which we can do by adding extra data attributes to
    /// identify the template source of an element.
    tag_blocks: Vec<(u8, String)>,
}

chainable!(TemplateTagger);

impl<S: Sink> TemplateTagger<S> {
    /// Creates a new `TemplateTagger` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            depth: 0,
            next,
            tag_blocks: <_>::default(),
        }
    }

    /// Ends a template section for a template with the given `name`.
    pub fn pop(&mut self, name: &str) {
        self.tag_blocks
            .pop_if(|(_, other)| name == other)
            .expect("valid tag block stack");
    }

    /// Starts a template section for a template with the given `name`.
    pub fn push(&mut self, name: &str) {
        self.tag_blocks.push((self.depth, name.to_owned()));
    }
}

impl<S: Sink> Sink for TemplateTagger<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        match marker {
            StripMarker::WikiRsSourceEnd(name) => self.pop(name),
            StripMarker::WikiRsSourceStart(name) => self.push(name),
            _ => {}
        }
        self.next.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.next.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        if !VOID_TAGS.contains(name) {
            self.depth -= 1;
        }
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.next.tag_start(name);
    }

    fn tag_start_end(&mut self, name: &str) {
        if !PHRASING_TAGS.contains(name) {
            // It is possible that a template starts in an ambiguous position
            // where the output of its first tag results in some other elements
            // being closed. To handle this case, `level` is treated as a
            // maximum which is reduced so child elements of the template do not
            // get tagged as it builds its own DOM tree.
            let mut has_some = false;
            for (depth, tag) in self
                .tag_blocks
                .iter_mut()
                .rev()
                .take_while(|(depth, _)| self.depth <= *depth)
            {
                *depth = self.depth;
                if !has_some {
                    self.next.tag_attribute_start("data-wiki-rs");
                    has_some = true;
                }
                self.next.text(tag);
            }
            if has_some {
                self.next.tag_attribute_end("data-wiki-rs");
            }
        }
        self.next.tag_start_end(name);
        if !VOID_TAGS.contains(name) {
            self.depth += 1;
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.next.text(text);
    }
}

/// Generates an implementation of [`Chain`] for a generic type with the given
/// ident.
macro_rules! chainable {
    ($ty:ident) => {
        chainable! { $ty<S> }
    };

    ($ty:ident<$($lt:lifetime,)* $s:ident $(, $gen:ident)* $(,)?>) => {
        impl<$($lt,)* $s $(, $gen)*> Chain for $ty<$($lt,)* $s $(, $gen)*>
        where
            $s: Sink + Markable,
        {
            type Next = $s;

            #[inline]
            fn next(&self) -> &Self::Next {
                &self.next
            }

            #[inline]
            fn next_mut(&mut self) -> &mut Self::Next {
                &mut self.next
            }
        }
    };
}

use chainable;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_range() {
        let mut s = MarkableString::default();
        let before = s.mark();
        s.push('a');
        let after_a = s.mark();
        s.push_str("bc");
        let after_c = s.mark();
        s.push('d');
        let after_d = s.mark();
        let start = s.inner.len();
        s.push('e');
        let after_e = s.mark();
        s.push_str("fghi");
        let after_i = s.mark();
        let end = s.inner.len();

        s.move_range(start..end, 0);
        let positions = [&before, &after_a, &after_c, &after_d, &after_e, &after_i]
            .map(|mark| s.restore_mark(mark));
        assert_eq!(s.inner, "efghiabcd");
        assert_eq!(
            positions,
            [Some(0), Some(6), Some(8), Some(9), Some(1), Some(5)]
        );

        s.move_range(0..4, 5);
        let positions = [&before, &after_a, &after_c, &after_d, &after_e, &after_i]
            .map(|mark| s.restore_mark(mark));
        assert_eq!(s.inner, "iefghabcd");
        assert_eq!(
            positions,
            [Some(0), Some(6), Some(8), Some(9), Some(2), Some(1)]
        );

        s.move_range(3..6, 5);
        let positions = [&before, &after_a, &after_c, &after_d, &after_e, &after_i]
            .map(|mark| s.restore_mark(mark));
        assert_eq!(s.inner, "iefbcghad");
        assert_eq!(
            positions,
            [Some(0), Some(8), Some(5), Some(9), Some(2), Some(1)]
        );

        s.free_mark(before);
        s.free_mark(after_a);
        s.free_mark(after_c);
        s.free_mark(after_d);
        s.free_mark(after_e);
        s.free_mark(after_i);
    }
}
