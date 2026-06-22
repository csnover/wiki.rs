//! HTML emitters for Wikitext fragments that require state management.

use super::{
    StripMarker,
    document::{Attribute, Node},
    globals::Outline,
    tags::PHRASING_TAGS,
};
use core::{cell::RefCell, fmt, num::NonZeroU32};
use html5gum::emitters::callback::{CallbackEmitter, CallbackEvent};
use indexmap::IndexMap;
use libmisc::CowExt as _;
use libwikitext_common::{
    AnchorEncodeMode, decode_html, escape_id, normalize_section_name, title::normalize_fragment,
};
use libwikitext_parse::{HeadingLevel, TextStyle, VOID_TAGS};
use regex::Regex;
use std::{borrow::Cow, collections::HashSet, rc::Rc, sync::LazyLock};

/// An intermediate sink.
pub(super) trait Chain: Sink {
    /// The type of the next sink in the chain.
    type Next;

    /// Returns a mutable reference to the next sink in the chain.
    fn next_mut(&mut self) -> &mut Self::Next;
}

/// A back-propagating bookmarker of output positions. Used to inject additional
/// unstructured HTML without buffering.
pub(super) trait Markable {
    /// Creates a clone of an existing mark.
    fn clone_mark(&mut self, mark: &Mark) -> Mark;

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
    fn clone_mark(&mut self, mark: &Mark) -> Mark {
        self.next_mut().clone_mark(mark)
    }

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
    fn clone_mark(&mut self, mark: &Mark) -> Mark {
        self.inner.clone_mark(mark)
    }

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

/// Balances the DOM tree.
#[derive(Debug)]
pub(super) struct DomTree<S: Sink> {
    /// The output.
    next: S,
    /// The stack of currently open nodes.
    stack: Vec<Node>,
}

chainable!(DomTree);

impl<S: Sink> DomTree<S> {
    /// Returns a new `DomTree` which emits to `next`.
    #[inline]
    pub fn new(next: S) -> Self {
        Self {
            next,
            stack: <_>::default(),
        }
    }

    /// Tries closing all tags up to and including the nearest `name`. Returns
    /// `true` if some elements were closed.
    fn try_close(&mut self, name: &str) -> bool {
        // TODO: Any `<a>` elements that were drained need to be restored.
        // Which means that there needs to be another component that tracks
        // those specifically.
        if let Some(pair) = self.stack.iter().rposition(|e| e.tag_name() == Some(name)) {
            for e in self.stack.drain(pair..).rev() {
                e.close(&mut self.next);
            }
            true
        } else {
            false
        }
    }
}

impl<S: Sink> Sink for DomTree<S> {
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
    fn finish(mut self) -> String {
        for e in self.stack.drain(..).rev() {
            e.close(&mut self.next);
        }
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        self.next.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if let Some(Node::Attribute(pos)) = self.stack.last_mut() {
            *pos = Attribute::Name;
        }
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if let Some(Node::Attribute(pos)) = self.stack.last_mut() {
            *pos = Attribute::Value;
        }
        self.next.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        if !self.try_close(name) {
            if name == "p" {
                // Why????
                self.next.tag_start_full("p");
                self.next.tag_end("p");
            } else {
                log::warn!("TODO: <{name}> tag mismatch required error recovery logic");
            }
        }
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        if name == "a" {
            self.try_close(name);
        }

        // Normally, receiving a new start tag should close any tags which cause
        // it to be in an invalid position in the DOM. This is especially
        // important for wikitable markup because wikitable children are
        // implicitly closed by the production of a new wikitable element.
        // However, there is one case where elements should be allowed to be
        // placed in an illegal position: when table-row templates get things
        // like 'Template:Tfd' applied to them, this will try to put non-table
        // content into the table, and this content is supposed to be fostered
        // out of the table later instead of ending the table.
        let close_tags = !matches!(
            self.stack.last(),
            Some(node @ Node::Tag(last))
            if (last == "table" || last == "tr") && *last != name && !node.can_parent(name));

        if close_tags {
            while let Some(e) = self.stack.pop_if(|e| !e.can_parent(name)) {
                e.close(&mut self.next);
            }
        }

        if matches!(name, "td" | "th")
            && !matches!(self.stack.last(), Some(Node::Tag(last)) if last == "tr")
        {
            self.tag_start_full("tr");
        }

        if !VOID_TAGS.contains(name) {
            self.stack.push(Node::Tag(name.to_owned().into()));
        }
        self.stack.push(Node::Attribute(Attribute::Name));
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.stack
            .pop_if(|node| matches!(node, Node::Attribute(_)))
            .expect("attribute node");
        self.next.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.next.text(text);
    }
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
            let mut ws = ws.as_str();
            while let Some((text, rest)) = ws.split_once('\n') {
                self.next.text(text);
                self.next.new_line();
                ws = rest;
            }
            self.next.text(ws);
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
    buffer: Accumulator,
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
    /// If true, the document is currently inside a list.
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
            buffer: <_>::default(),
            close_match: <_>::default(),
            current: <_>::default(),
            in_block: <_>::default(),
            in_blockquote: <_>::default(),
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
                self.next
                    .text(&core::mem::take(&mut self.buffer).as_str()[1..]);
            } else if self.meta_line == GrafMetaLine::Yes {
                if self.pending != GrafPendingState::None {
                    self.close(false);
                    self.pending = GrafPendingState::None;
                }
            } else if self
                .buffer
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_whitespace())
            {
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

        if self.pending == GrafPendingState::None {
            tokenise(&mut self.next, core::mem::take(&mut self.buffer).as_str());
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

    /// Returns true if the currently buffered line should be treated like a
    /// preformatted line.
    fn is_pre_line(&self) -> bool {
        !self.in_blockquote
            && self.buffer.as_str().strip_prefix(' ').is_some_and(|text| {
                self.current == GrafState::Pre || text.bytes().any(|b| !b.is_ascii_whitespace())
            })
    }

    /// Tells the `GrafEmitter` whether a Wikitext list is being processed.
    pub(super) fn set_in_list(&mut self, in_list: bool) {
        self.pending = GrafPendingState::None;
        self.in_list = in_list;
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
        if !self.in_list {
            self.end_line(true);
        }
        self.close(true);
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.in_list {
            self.next.new_line();
        } else {
            self.end_line(false);
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
        if !self.in_list {
            match marker {
                StripMarker::General(_) => {
                    panic!("general strip marker should be decomposed already")
                }
                StripMarker::NoWiki(_) => {
                    self.meta_line = GrafMetaLine::No;
                }
                _ => {}
            }
        }
        self.next.strip_marker(marker);
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
    "h1", "h2", "h3", "h4", "h5", "h6", "ol", "p", "pre", "table", "ul"
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
                next.text(value);
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

    /// Updates the positions of marks above `start`
    fn adjust_marks(&mut self, start: u32, delta: i32) {
        for pos in self.iter_marks_mut(start) {
            *pos = pos.checked_add_signed(delta).unwrap_or(Self::INVALID);
        }
    }

    /// Duplicates a mark.
    #[inline]
    pub fn clone_mark(&mut self, mark: &Mark) -> Mark {
        if let Some(&pos) = self.marks.get(usize::from(mark.0)) {
            self.insert_mark(pos)
        } else {
            Mark(Self::NO_FREE)
        }
    }

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

    /// Inserts `string` at byte position `idx`.
    #[inline]
    pub fn insert_str(&mut self, idx: usize, string: &str) {
        if string.is_empty() {
            return;
        }
        self.inner.insert_str(idx, string);
        let delta = i32::try_from(string.len()).unwrap();
        self.adjust_marks(u32::try_from(idx).unwrap(), delta);
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

    /// Replaces the `range` of this string with `replace_with`.
    pub fn replace_range<R: core::ops::RangeBounds<usize>>(
        &mut self,
        range: R,
        replace_with: &str,
    ) {
        let at = match range.start_bound() {
            core::ops::Bound::Excluded(&start) => start + 1,
            core::ops::Bound::Included(&start) => start,
            core::ops::Bound::Unbounded => 0,
        };

        let before = self.inner.len();
        self.inner.replace_range(range, replace_with);
        // TODO: Negative deltas destroy marks in the range.
        let delta = self
            .inner
            .len()
            .checked_signed_diff(before)
            .expect("isize-sized delta");
        if delta != 0 {
            self.adjust_marks(u32::try_from(at).unwrap(), i32::try_from(delta).unwrap());
        }
    }

    /// Removes one character at `idx`.
    #[inline]
    pub fn remove(&mut self, idx: usize) -> char {
        let c = self.inner.remove(idx);
        self.adjust_marks(u32::try_from(idx).unwrap(), -1);
        c
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
pub(super) struct OutlineEmitter<S: Sink + Markable> {
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
    outline: Rc<RefCell<Outline>>,

    /// The emitter state.
    state: OutlineEmitterState,
}

impl<S: Sink + Markable> OutlineEmitter<S> {
    /// Creates a new `OutlineEmitter` chained to `next`.
    pub fn new(outline: &Rc<RefCell<Outline>>, next: S) -> Self {
        Self {
            buffer: <_>::default(),
            in_strip_marker: <_>::default(),
            next,
            outline: Rc::clone(outline),
            state: <_>::default(),
        }
    }

    /// Adds the given `text` to the currently processing outline entry.
    fn add_text(&mut self, text: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.text(text);
            outline.entry_html.text(text);
            if self.state == OutlineEmitterState::Body
                && let OutlineId::Implicit(id) = &mut outline.id
            {
                id.push_str(text);
            } else if self.state == OutlineEmitterState::StartId
                && let OutlineId::Explicit(id) = &mut outline.id
            {
                id.push_str(text);
            }
        }
    }

    /// Saves the given `entry` to the global outline and emits the buffered
    /// HTML to the next sink.
    fn save_entry(&mut self, entry: OutlineEntry) {
        let id = entry.id.into_inner();
        let id = normalize_section_name(&id).map(normalize_fragment);
        let mut outlines = self.outline.borrow_mut();
        let override_id = if dbg!(self.in_strip_marker) {
            eprintln!("SKIPPING {id}");
            // Headings in strip markers are not supposed to go to the outline
            None
        } else {
            eprintln!("pushing {id}");
            outlines.push(entry.level, entry.entry_html.finish().trim_ascii(), &id)
        };
        let mut html = entry.document_html.finish();

        let id = override_id.unwrap_or(&id);
        let html_id = escape_id(id, AnchorEncodeMode::Html5);
        let legacy_id = escape_id(id, AnchorEncodeMode::Legacy);

        debug_assert!(entry.body_start > entry.id_start);

        if html_id != legacy_id {
            html.insert_str(
                entry.body_start as usize,
                &format!(
                    r#"<span id="{}"></span>"#,
                    encode_double_quoted_attribute(&legacy_id)
                ),
            );
        }

        if override_id.is_some()
            && let Some(end) = entry.id_end
        {
            html.replace_range(entry.id_start as usize..u32::from(end) as usize, &html_id);
        } else if entry.id_end.is_none() {
            html.insert_str(
                entry.id_start as usize,
                &format!(r#" id="{}""#, encode_double_quoted_attribute(&html_id)),
            );
        }
        tokenise(&mut self.next, &html);
    }
}

chainable!(OutlineEmitter);

impl<S: Sink + Markable> Sink for OutlineEmitter<S> {
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
            if self.state == OutlineEmitterState::Body
                && let OutlineId::Implicit(id) = &mut outline.id
            {
                id.push(value);
            } else if self.state == OutlineEmitterState::StartId
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
                eprintln!("start");
                tokenise(self, dbg!(html));
                eprintln!("end");
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
            if self.state == OutlineEmitterState::StartId {
                outline.id_end = NonZeroU32::new(outline.document_html.len());
                self.state = OutlineEmitterState::Start;
            } else if self.state == OutlineEmitterState::StartAttr {
                self.state = OutlineEmitterState::Start;
            } else {
                self.state = OutlineEmitterState::Body;
            }
        } else {
            self.next.tag_attribute_end(name);
        }
    }

    fn tag_attribute_start(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            if self.state == OutlineEmitterState::Start {
                if name == "id" {
                    self.state = OutlineEmitterState::StartId;
                    outline.id = OutlineId::Explicit(<_>::default());
                    outline.id_start = outline.document_html.len();
                } else {
                    self.state = OutlineEmitterState::StartAttr;
                }
            } else {
                self.state = OutlineEmitterState::BodyAttr;
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
            self.state = OutlineEmitterState::Start;
        } else if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.tag_start(name);
            outline.entry_html.tag_start(name);
        } else {
            self.next.tag_start(name);
        }
    }

    fn tag_start_end(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            if self.state == OutlineEmitterState::Start && outline.id_end.is_none() {
                outline.id_start = outline.document_html.len();
            }
            outline.document_html.tag_start_end(name);
            if self.state == OutlineEmitterState::Start {
                outline.body_start = outline.document_html.len();
                self.state = OutlineEmitterState::Body;
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

/// The state of an `OutlineEmitter`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum OutlineEmitterState {
    /// In a new `<hN>` start tag.
    Start,
    /// In a new `<hN>` stat tag attribute.
    StartAttr,
    /// In the `id` attribute of a new `<hN>` start tag.
    StartId,
    /// In some other tag state.
    #[default]
    Body,
    /// In an attribute for some other tag.
    BodyAttr,
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
            if self.body_pos == self.buffer.next_mut().len() {
                // Empty tags are filtered out
                self.buffer.next_mut().truncate(self.start_pos);
            } else {
                self.buffer.tag_end(name);
            }
        }
    }

    fn tag_start(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            self.start_pos = self.buffer.next_mut().len();
            self.buffer.tag_start(name);
            self.in_span = name == "span";
        } else {
            self.filtering += 1;
        }
    }

    fn tag_start_end(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            self.buffer.tag_start_end(name);
            self.body_pos = self.buffer.next_mut().len();
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

impl OutlineId {
    /// Consumes the `OutlineId`, returning the string value.
    fn into_inner(self) -> String {
        match self {
            Self::Implicit(s) | Self::Explicit(s) => s,
        }
    }
}

impl Default for OutlineId {
    fn default() -> Self {
        Self::Implicit(<_>::default())
    }
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
}

chainable!(PWrapper);

impl<S: Sink> PWrapper<S> {
    /// Creates a new `PWrapper` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            depth: <_>::default(),
            in_p: <_>::default(),
            next,
        }
    }

    fn change_state(&mut self, name: &str) {
        if INLINE.contains(name) {
            self.enter_p();
        } else if self.in_p {
            self.next.tag_end("p");
            self.in_p = false;
        }
    }

    fn enter_p(&mut self) {
        if self.depth == 0 && !self.in_p {
            self.in_p = true;
            self.next.tag_start_full("p");
        }
    }
}

impl<S: Sink> Sink for PWrapper<S> {
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
        self.enter_p();
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(mut self) -> String {
        if self.in_p {
            self.next.tag_end("p");
        }
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker) {
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
        self.next.tag_end(name);
        self.depth -= 1;
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.change_state(name);
        self.next.tag_start(name);
        self.depth += 1;
    }

    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.enter_p();
        self.next.text(text);
    }
}

static FORMATTING: phf::Set<&str> = phf::phf_set! {
    "a"
    |"b"|"big"|"code"|"em"|"font"|"i"|"nobr"|"s"|"small"|"strike"|"strong"|"tt"
    |"u"
};

static INLINE: phf::Set<&str> = phf::phf_set! {
    "a"|"abbr"
    |"acronym"|"applet"|"audio"|"b"|"basefont"|"bdi"|"bdo"|"big"|"br"|"button"
    |"cite"|"code"|"data"|"del"|"dfn"|"em"|"font"|"i"|"iframe"|"img"|"input"
    |"ins"|"kbd"|"label"|"legend"|"map"|"mark"|"object"|"param"|"q"|"rb"|"rbc"
    |"rp"|"rt"|"rtc"|"ruby"|"s"|"samp"|"select"|"small"|"source"|"span"|"strike"
    |"strong"|"sub"|"sup"|"textarea"|"time"|"track"|"tt"|"u"|"var"|"video"
    |"wbr"
};

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
        self.in_code -= u8::from(Self::is_code_tag(name));
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
        log::warn!("TODO: TableFoster strip marker");
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
        if matches!(name, "table" | "tr") {
            self.foster();
        }
        if name == "table" {
            let last = self.stack.pop().expect("table mark");
            self.next.free_mark(last.before_table);
        }
        self.next.tag_end(name);
        if matches!(name, "caption" | "td" | "th" | "tr")
            && let Some(last) = self.stack.last_mut()
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
        } else if matches!(name, "caption" | "td" | "th" | "tr") {
            self.foster();
        }
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
        if matches!(name, "table" | "tr")
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
        impl<$s $(, $gen)*> Chain for $ty<$($lt,)* $s $(, $gen)*>
        where
            $s: Sink + Markable,
        {
            type Next = $s;

            #[inline]
            fn next_mut(&mut self) -> &mut Self::Next {
                &mut self.next
            }
        }
    };
}

use chainable;
use html_escape::encode_double_quoted_attribute;

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
