//! HTML emitters for Wikitext fragments that require state management.

use super::{
    State,
    document::{Attribute, Node},
    globals::Outline,
    tags::PHRASING_TAGS,
};
use core::fmt::{self, Write as _};
use libmisc::CowExt as _;
use libphp_rs::strtr;
use libwikitext_common::{
    AnchorEncodeMode, anchor_encode, decode_html, normalize_section_name, title::normalize_fragment,
};
use libwikitext_parse::{HeadingLevel, TextStyle, VOID_TAGS};

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
    fn finish(self, state: &mut State<'_, '_, '_>) -> String;

    /// A source newline.
    ///
    /// This is used for source-line-sensitive rules.
    fn new_line(&mut self);

    /// Writes opaque block HTML content.
    fn raw_html_block(&mut self, html: &str);

    /// Writes opaque inline HTML content.
    fn raw_html_inline(&mut self, html: &str);

    /// End a tag attribute with the given `name`.
    ///
    /// ```html
    /// <tag name="value">text&#8253;<!-- comment --></tag>
    ///                 ^
    /// ```
    fn tag_attribute_end(&mut self, name: &str);

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

/// Chomps all the whitespace after a heading. Nom nom nom nom nom.
#[derive(Debug)]
pub(super) struct AfterHeadingChomper<S: Sink> {
    /// Chomper hungers? Oog!
    hungry: HungerLevel,
    /// The next sink.
    next: S,
}

impl<S: Sink> AfterHeadingChomper<S> {
    /// Creates a new `AfterHeadingChomper`.
    #[inline]
    pub fn new(next: S) -> Self {
        Self {
            hungry: <_>::default(),
            next,
        }
    }
}

chainable!(AfterHeadingChomper);

impl<S: Sink> Sink for AfterHeadingChomper<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.hungry = HungerLevel::Low;
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.hungry = HungerLevel::Low;
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.hungry = HungerLevel::Low;
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(self, state: &mut State<'_, '_, '_>) -> String {
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        if self.hungry != HungerLevel::High {
            self.next.new_line();
            if self.hungry == HungerLevel::Medium {
                self.hungry = HungerLevel::High;
            }
        }
    }

    #[inline]
    fn raw_html_block(&mut self, html: &str) {
        self.hungry = HungerLevel::Low;
        self.next.raw_html_block(html);
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        self.hungry = HungerLevel::Low;
        self.next.raw_html_inline(html);
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
        if HeadingLevel::TAGS.contains(&name) {
            self.hungry = HungerLevel::Medium;
        } else {
            self.hungry = HungerLevel::Low;
        }
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.hungry = HungerLevel::Low;
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        if self.hungry == HungerLevel::Low {
            self.next.text(text);
        } else {
            let text = text.trim_ascii_start();
            if !text.is_empty() {
                self.next.text(text);
                self.hungry = HungerLevel::Low;
            }
        }
    }
}

/// How hungry is the chomper?
#[derive(Debug, Default, Eq, PartialEq)]
enum HungerLevel {
    /// Not very hungry. Allows all the things.
    #[default]
    Low,
    /// Medium hungry. Allows one newline to pass.
    Medium,
    /// Hungriest hungry. Consumes all whitespace.
    High,
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
    /// If `Some`, the accumulator has received a `tag_attribute_start` and is
    /// waiting for a `tag_attribute_end`.
    in_attr: Option<bool>,
}

impl Accumulator {
    /// Creates a new `Accumulator`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Emits the value separator for an attribute, if needed.
    fn writing(&mut self) {
        if let Some(has_value) = &mut self.in_attr
            && !*has_value
        {
            self.inner.push_str(r#"=""#);
            *has_value = true;
        }
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
        self.writing();
        self.inner.push_str("-->");
    }

    #[inline]
    fn comment_start(&mut self) {
        self.writing();
        self.inner.push_str("<!--");
    }

    fn entity(&mut self, value: char, raw: &str) {
        self.writing();
        if matches!(value, '<' | '>' | '&') || (self.in_attr.is_some() && value == '"') {
            self.inner.push_str(raw);
        } else {
            self.inner.push(value);
        }
    }

    #[inline]
    fn finish(self, _: &mut State<'_, '_, '_>) -> String {
        self.inner.into_inner()
    }

    #[inline]
    fn new_line(&mut self) {
        self.writing();
        self.inner.push('\n');
    }

    fn raw_html_block(&mut self, html: &str) {
        self.writing();
        if self.in_attr.is_some() {
            self.inner.push_str(&strtr(html, &[("\"", "&quot;")]));
        } else {
            self.inner.push_str(html);
        }
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        self.raw_html_block(html);
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {
        let has_value = self.in_attr.take().expect("balanced attribute");
        if has_value {
            self.inner.push('"');
        }
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.inner.push(' ');
        self.inner.push_str(name);
        self.in_attr = Some(false);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.writing();
        self.inner.push_str("</");
        self.inner.push_str(name);
        self.inner.push_str(">");
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.writing();
        self.inner.push('<');
        self.inner.push_str(name);
    }

    #[inline]
    fn tag_start_end(&mut self, _: &str) {
        self.writing();
        self.inner.push('>');
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.writing();
        self.inner.push_str(text);
    }
}

/// Implements the leading whitespace trimming rule for category links:
///
/// “Strip newlines from the left hand context of Category links.
///  See T2087, T87753, T174639, T359886”

#[derive(Debug)]
pub(super) struct CategoryTrim<S: Sink> {
    /// The trimmed whitespace buffer.
    buffer: String,
    /// The output.
    next: S,
}

impl<S: Sink> CategoryTrim<S> {
    /// Creates a new `CategoryTrim` chained to `next`.
    #[inline]
    pub fn new(next: S) -> Self {
        Self {
            buffer: <_>::default(),
            next,
        }
    }

    /// Clears the whitespace buffer.
    #[inline]
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Flushes buffered whitespace to [`Self::next`].
    fn flush(&mut self) {
        let mut next_text = 0;
        for index in memchr::memchr_iter(b'\n', self.buffer.as_bytes()) {
            if index != next_text {
                self.next.text(&self.buffer[next_text..index]);
            }
            next_text = index + 1;
            self.next.new_line();
        }
        if next_text != self.buffer.len() {
            self.next.text(&self.buffer[next_text..]);
        }
        self.buffer.clear();
    }
}

chainable!(CategoryTrim);

impl<S: Sink> Sink for CategoryTrim<S> {
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
        self.flush();
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(mut self, state: &mut State<'_, '_, '_>) -> String {
        self.flush();
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        self.buffer.push('\n');
    }

    #[inline]
    fn raw_html_block(&mut self, html: &str) {
        self.flush();
        self.next.raw_html_block(html);
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        self.flush();
        self.next.raw_html_inline(html);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.flush();
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.flush();
        self.next.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.flush();
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.flush();
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.flush();
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        if !self.buffer.is_empty() && text.bytes().all(|c| c.is_ascii_whitespace()) {
            self.buffer.push_str(text);
        } else {
            self.flush();
            self.next.text(text);
        }
    }
}

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

    /// Returns `true` if the emitter is currently inside any table.
    pub fn in_table(&self) -> bool {
        self.stack
            .iter()
            .rev()
            .any(|e| matches!(e, Node::Tag(name) if name == "table"))
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
    fn finish(mut self, state: &mut State<'_, '_, '_>) -> String {
        for e in self.stack.drain(..).rev() {
            e.close(&mut self.next);
        }
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn raw_html_block(&mut self, html: &str) {
        if let Some(Node::Attribute(pos)) = self.stack.last() {
            if *pos == Attribute::Name {
                log::warn!("invalid HTML block in attribute position; ignoring");
            } else {
                log::warn!("invalid HTML block in attribute position; treating as text");
                self.next.text(html);
            }
        } else {
            self.next.raw_html_block(html);
        }
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        if let Some(Node::Attribute(pos)) = self.stack.last() {
            if *pos == Attribute::Name {
                log::warn!("invalid HTML block in attribute position; ignoring");
            } else {
                log::warn!("invalid HTML block in attribute position; treating as text");
                self.next.text(html);
            }
        } else {
            self.next.raw_html_inline(html);
        }
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
        if let Some(pair) = self.stack.iter().rposition(|e| e.tag_name() == Some(name)) {
            for e in self.stack.drain(pair..).rev() {
                e.close(&mut self.next);
            }
        } else {
            log::warn!("TODO: <{name}> tag mismatch requires error recovery logic");
        }
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
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
                self.next.new_line();
            }
        }

        if matches!(name, "td" | "th")
            && !matches!(self.stack.last(), Some(Node::Tag(last)) if last == "tr")
        {
            self.tag_start_full("tr");
            self.next.new_line();
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
    /// The position of the attribute list for a potentially empty element.
    last: Option<Mark>,
    /// The output.
    next: S,
}

chainable!(EmptyTagger);

impl<S: Sink + Markable> EmptyTagger<S> {
    /// Creates a new `EmptyTagger` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            last: <_>::default(),
            next,
        }
    }

    /// Clears the empty tag mark.
    #[inline]
    fn clear(&mut self) {
        if let Some(last) = self.last.take() {
            self.next.free_mark(last);
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
        self.clear();
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(self, state: &mut State<'_, '_, '_>) -> String {
        debug_assert!(self.last.is_none());
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn raw_html_block(&mut self, html: &str) {
        if html.bytes().any(|c| !c.is_ascii_whitespace()) {
            self.clear();
        }
        self.next.raw_html_block(html);
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        if html.bytes().any(|c| !c.is_ascii_whitespace()) {
            self.clear();
        }
        self.next.raw_html_inline(html);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.clear();
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        if let Some(last) = self.last.take() {
            self.next.with_marks([&last], |[last], out| {
                if let Some(last) = last {
                    out.insert_str(last, r#" class="mw-empty-elt""#);
                }
            });
            self.next.free_mark(last);
        }
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.clear();
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        if matches!(name, "p" | "li" | "tr") {
            debug_assert!(self.last.is_none());
            self.last = Some(self.next.mark());
        }
        self.next.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if text.bytes().any(|c| !c.is_ascii_whitespace()) {
            self.clear();
        }
        self.next.text(text);
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
    /// The output.
    next: S,

    // State for a single line:
    /// If true, the line contains an end tag which triggers a graf state
    /// transition.
    close_match: bool,
    /// The line should be treated as if it contained content, even if it did
    /// not.
    force_content: bool,
    /// The start position of the current line of the document.
    line_start: Mark,
    /// If true, the line contains a start tag which triggers a graf state
    /// transition.
    open_match: bool,
    /// If true, the line contains a `</pre>`.
    pre_close_match: bool,
    /// If true, the line contains a `<pre>`.
    pre_open_match: bool,
    /// Positions within the current line where graf wrappers should be
    /// inserted.
    wrap_points: Vec<GrafWrapPoint>,

    // State which spans multiple lines:
    /// The start positions and depths of currently open `<blockquote>`
    /// elements.
    blockquote_roots: Vec<BlockquoteRoot>,
    /// The currently active graf.
    current: GrafState,
    /// If true, the document is currently inside a graf block.
    in_block: bool,
    /// If non-zero, the document is currently inside a Wikitext list.
    in_list: u8,
    /// If true, the document is currently inside an explicitly defined `<pre>`.
    in_pre: bool,
    /// The current DOM depth.
    level: u8,
    /// The next graf to emit.
    pending: GrafPendingState,
}

impl<S: Sink + Markable> GrafEmitter<S> {
    /// Creates a new `GrafEmitter` chained to `next`.
    pub fn new(mut next: S) -> Self {
        let line_start = next.mark();
        Self {
            next,
            close_match: <_>::default(),
            force_content: <_>::default(),
            line_start,
            open_match: <_>::default(),
            pre_close_match: <_>::default(),
            pre_open_match: <_>::default(),
            wrap_points: <_>::default(),
            blockquote_roots: <_>::default(),
            current: <_>::default(),
            in_block: <_>::default(),
            in_list: <_>::default(),
            in_pre: <_>::default(),
            level: <_>::default(),
            pending: <_>::default(),
        }
    }

    /// Emits the end of a graf to the output.
    fn close(&mut self, line_start: bool) {
        self.in_pre = false;
        let tag = match core::mem::take(&mut self.current) {
            GrafState::None => return,
            GrafState::Graf => "</p>\n",
            GrafState::Pre => "</pre>\n",
        };

        if line_start {
            self.next.with_marks([&self.line_start], |[pos], out| {
                if let Some(pos) = pos {
                    out.insert_str(pos, tag);
                }
            });
        } else {
            self.next.raw_html_inline(tag);
        }
    }

    /// Finishes processing of a line of source text.
    #[expect(
        clippy::too_many_lines,
        reason = "lots of comments and a bad algorithm"
    )]
    pub(super) fn end_line(&mut self, last_line: bool) {
        // I’m doing the bad thing of writing some “what” comments in here
        // because this algorithm is incoherent

        let end = self.next.mark();

        if self.open_match || self.close_match {
            // This line had a state-changing tag somewhere inside, which means
            // that it is definitely not a graf line
            self.pending = GrafPendingState::None;

            // This is the `RemexCompatMunger` half of this bullshit which
            // inserts grafs around lines of text that are directly inside the
            // document root or a blockquote
            self.p_wrap();

            if !self.in_pre || self.pre_open_match {
                // If this line has a `<pre>` tag, or we were not already in a
                // preformatted context, then this line should not be included
                // in any previous graf, so finish any graf from the previous
                // line(s)
                self.close(true);
            }

            // Now, if an explicit `<pre>` was started but not ended in this
            // line, what comes next is part of that `<pre>` element. If we
            // were already inside a `<pre>` context, stay inside of it
            if self.pre_close_match {
                self.in_pre = false;
            } else {
                self.in_pre |= self.pre_open_match;
            }

            // And if this line contained a graf-suppressing block start tag,
            // but not a terminating tag, then the whole line is considered
            // to be part of a graf-suppressing block
            self.in_block = !self.close_match;
        } else if self.in_list == 0 && !self.in_block && !self.in_pre {
            // If this line was not inside a graf-suppressing block or explicit
            // `<pre>` element, maybe it’s time to emit something!
            let has_content = self.force_content
                || self.next.with_marks([&self.line_start], |[pos], out| {
                    pos.is_some_and(|pos| out[pos..].bytes().any(|c| !c.is_ascii_whitespace()))
                });

            if self.blockquote_roots.is_empty()
                && (self.current == GrafState::Pre || has_content)
                && self.next.with_marks([&self.line_start], |[pos], out| {
                    pos.is_some_and(|pos| out[pos..].starts_with(' '))
                })
            {
                // So long as this is not a line inside a blockquote—because
                // those are apparently special—this line is either a
                // continuation of, or a transition into, a preformatted graf

                if self.current == GrafState::Pre {
                    // The space prefix must be removed or the preformatted text
                    // will be improperly indented in the output
                    self.next.with_marks([&self.line_start], |[pos], out| {
                        if let Some(pos) = pos {
                            out.remove(pos);
                        }
                    });
                } else {
                    // The tags are emitted backwards because this is an
                    // insertion; this will either be `</p><pre>` or `<pre>`.
                    // As in the other branch, the space prefix is removed, but
                    // here it is removed by overwriting
                    self.next.with_marks([&self.line_start], |[pos], out| {
                        if let Some(pos) = pos {
                            out.replace_range(pos..=pos, "<pre>");
                        }
                    });
                    self.close(true);
                    self.current = GrafState::Pre;

                    // Having just performed a state transition, there can be
                    // nothing pending
                    self.pending = GrafPendingState::None;
                }
            /* TODO: if whole line is only a style or link tag, do not wrap */
            } else if !has_content {
                // Got a new empty line.

                if self.pending != GrafPendingState::None {
                    // An empty line when a graf is already pending means to
                    // start a new graf with an extra newline. These tags are
                    // emitted backwards because it is an insertion; this will
                    // either be `<p><br>` or `</p><p><br>`, and then we will be
                    // definitively inside of a graf
                    self.next.with_marks([&self.line_start], |[pos], out| {
                        if let Some(pos) = pos {
                            out.insert_str(pos, "<br>");
                            out.insert_str(pos, self.pending.as_ref());
                        }
                    });
                    self.pending = GrafPendingState::None;
                    self.current = GrafState::Graf;
                } else if self.current != GrafState::Graf {
                    // An empty line when not in a graf means to transition into
                    // a pending graf, since the next line may be a continuation
                    // of a graf or it may be a line containing state-changing
                    // tags
                    self.close(true);
                    self.pending = GrafPendingState::Graf;
                } else {
                    // An empty line when already in a graf means to transition
                    // into a pending graf break, since the next line may be a
                    // new graf line (resulting in a graf break) or it may be a
                    // line containing state-changing tags (resulting in a graf
                    // end)
                    self.pending = GrafPendingState::GrafBreak;
                }
            } else if self.pending != GrafPendingState::None {
                // The line was not empty, contained only phrasing content, and
                // we were already in a pending graf state, so this was a graf
                // line, and we are now in a graf
                self.next.with_marks([&self.line_start], |[pos], out| {
                    if let Some(pos) = pos {
                        out.insert_str(pos, self.pending.as_ref());
                    }
                });
                self.pending = GrafPendingState::None;
                self.current = GrafState::Graf;
            } else if self.current != GrafState::Graf {
                // Got a new non-empty line, and we were *not* in a pending graf
                // state, but *were* in a non-graf context, so this line
                // transitioned from a non-graf or preformatted graf to a text
                // graf. These tags are emitted backwards because it is an
                // insertion; this will either be `<p>` or `</pre><p>`
                self.next.with_marks([&self.line_start], |[pos], out| {
                    if let Some(pos) = pos {
                        out.insert_str(pos, "<p>");
                    }
                });
                self.close(true);
                self.current = GrafState::Graf;
            }
        }

        // This is the point where the “buffered” text would be emitted. Since
        // there is no buffering here, the text already exists, and so it is
        // instead necessary to remove text that would have *not* been inserted
        if self.pending == GrafPendingState::None {
            if !last_line || self.current != GrafState::None {
                self.next.new_line();
            }
        } else {
            self.next
                .with_marks([&self.line_start, &end], |[pos, end], out| {
                    if let (Some(pos), Some(end)) = (pos, end) {
                        out.replace_range(pos..end, "");
                    }
                });
        }

        self.next.free_mark(end);
        let old = core::mem::replace(&mut self.line_start, self.next.mark());
        self.next.free_mark(old);
        self.force_content = false;
        self.open_match = false;
        self.close_match = false;
        self.pre_open_match = false;
        self.pre_close_match = false;
        if !self.wrap_points.is_empty() {
            if !last_line {
                log::warn!("did not drain wrappers somehow");
            }
            for point in self.wrap_points.drain(..) {
                self.next.free_mark(point.start);
                if let Some(end) = point.end {
                    self.next.free_mark(end);
                }
            }
        }
    }

    /// Restores normal processing of lines.
    #[inline]
    pub(super) fn end_list(&mut self) {
        self.pending = GrafPendingState::None;
        self.in_list -= 1;
        if self.in_list == 0 {
            let old = core::mem::replace(&mut self.line_start, self.next.mark());
            self.next.free_mark(old);
            self.close_match = true;
        }
    }

    /// Marks the end of a p-wrapper.
    fn end_wrap(&mut self) {
        if self.in_list != 0 {
            return;
        }

        let start = if self.level == 0 {
            &self.line_start
        } else if let Some(root) = self.blockquote_roots.last()
            && root.level == self.level
        {
            let marks = [&root.start, &self.line_start];
            let which = self.next.with_marks(marks, |[a, b], _| usize::from(a < b));
            marks[which]
        } else {
            // Non-phrasing element in some intermediate root which is not the
            // document root nor the current blockquote root
            return;
        };

        let end = self.next.mark();
        if let Some(last) = self.wrap_points.last_mut() {
            if self
                .next
                .with_marks([&last.start, &end], |[a, b], _| a == b)
            {
                // Two non-phrasing elements were directly adjacent
                let last = self.wrap_points.pop().unwrap();
                self.next.free_mark(last.start);
                self.next.free_mark(end);
                debug_assert!(last.end.is_none());
            } else {
                debug_assert!(last.end.is_none());
                last.end = Some(end);
            }
        } else if self.next.with_marks([start, &end], |[a, b], _| a != b) {
            // Non-phrasing element, not at the start of the root
            self.wrap_points.push(GrafWrapPoint {
                start: self.next.clone_mark(start),
                end: Some(end),
            });
        } else {
            self.next.free_mark(end);
        }
    }

    /// Forces the emitter to act as if content was emitted.
    #[inline]
    pub(super) fn force_content(&mut self) {
        self.force_content = true;
    }

    /// Wraps bare plain text content within a line also containing non-phrasing
    /// elements into grafs.
    fn p_wrap(&mut self) {
        if let Some(last) = self.wrap_points.last_mut() {
            if self.next.with_marks([&last.start], |[start], out| {
                start.is_some_and(|start| out[start..].bytes().all(|c| c.is_ascii_whitespace()))
            }) {
                // A non-phrasing element was at the end of the line
                let last = self.wrap_points.pop().unwrap();
                self.next.free_mark(last.start);
                debug_assert!(last.end.is_none());
            } else {
                last.end.get_or_insert_with(|| self.next.mark());
            }
        }

        // Because the content is being inserted rather than appended, the
        // order of operations is backwards
        for GrafWrapPoint { start, end } in self.wrap_points.drain(..).rev() {
            let end = end.unwrap();
            self.next.with_marks([&start, &end], |[start, end], out| {
                if let (Some(start), Some(end)) = (start, end)
                    && out[start..end].bytes().any(|c| !c.is_ascii_whitespace())
                {
                    out.insert_str(end, "</p>");
                    out.insert_str(start, "<p>");
                }
            });
            self.next.free_mark(start);
            self.next.free_mark(end);
        }
    }

    /// Inhibits normal processing of lines.
    #[inline]
    pub(super) fn start_list(&mut self) {
        self.close(false);
        self.pending = GrafPendingState::None;
        self.in_list += 1;
    }

    /// Marks the start of a possible p-wrapper.
    fn start_wrap(&mut self) {
        if self.in_list != 0 {
            return;
        }

        if self.level == 0
            || matches!(self.blockquote_roots.last(), Some(last) if last.level == self.level)
        {
            let start = self.next.mark();
            debug_assert!(matches!(
                self.wrap_points.last(),
                None | Some(GrafWrapPoint { end: Some(_), .. })
            ));
            self.wrap_points.push(GrafWrapPoint { start, end: None });
        }
    }
}

chainable!(GrafEmitter);

impl<S: Sink + Markable> Sink for GrafEmitter<S> {
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

    fn finish(mut self, state: &mut State<'_, '_, '_>) -> String {
        self.end_line(true);
        self.end_wrap();
        self.close(true);
        debug_assert_eq!(self.level, 0);
        debug_assert!(self.blockquote_roots.is_empty());
        debug_assert!(self.wrap_points.is_empty());
        self.next.free_mark(self.line_start);
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        self.end_line(false);
    }

    fn raw_html_block(&mut self, html: &str) {
        self.open_match = true;
        self.end_wrap();
        self.next.raw_html_block(html);
        self.close_match = true;
        self.start_wrap();
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        self.force_content = true;
        self.next.raw_html_inline(html);
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
        // Any transition out of a blockquote needs to trigger a line transition
        // because all text in a blockquote is unconditionally graf-wrapped.
        // (This is the `RemexCompatMunger` half of this bullshit)
        if name == "blockquote" {
            self.end_wrap();
            let root = self
                .blockquote_roots
                .pop_if(|root| self.level == root.level)
                .expect("blockquote roots stack corruption");
            self.next.free_mark(root.start);
        } else if name == "pre" {
            self.pre_close_match = true;
        }
        self.open_match |= ANTI_BLOCK_TAG.contains(name) || ALWAYS_TAG.contains(name);
        self.close_match |= BLOCK_TAG.contains(name) || NEVER_TAG.contains(name);

        // Emitting before `end_wrap` will cause that to try to wrap
        // `</blockquote>`
        self.next.tag_end(name);

        if matches!(name, "dl" | "ol" | "ul") {
            self.end_list();
        } else if !PHRASING_TAGS.contains(name) {
            self.level -= 1;
            // After transitioning back to a blockquote root or document root,
            // the next content is unconditionally graf-wrapped. (This is the
            // `RemexCompatMunger` half of this bullshit)
            self.start_wrap();
        }
    }

    fn tag_start(&mut self, name: &str) {
        // Any transition from a document root or blockquote root to
        // non-phrasing content must trigger an unconditional graf-wrap of any
        // content on the line prior to the transition. (This is the
        // `RemexCompatMunger` half of this bullshit)
        if matches!(name, "dl" | "ol" | "ul") {
            self.start_list();
        } else if !PHRASING_TAGS.contains(name) {
            self.end_wrap();
            self.level += 1;
        }
        self.next.tag_start(name);
    }

    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);

        self.open_match |= BLOCK_TAG.contains(name) || ALWAYS_TAG.contains(name);
        self.close_match |= ANTI_BLOCK_TAG.contains(name) || NEVER_TAG.contains(name);

        if name == "blockquote" {
            self.blockquote_roots.push(BlockquoteRoot {
                level: self.level,
                start: self.next.mark(),
            });
            // Any transition into a blockquote needs to trigger a line
            // transition because all text in a blockquote is unconditionally
            // graf-wrapped. (This is the `RemexCompatMunger` half of this
            // bullshit)
            self.start_wrap();
        } else if name == "pre" {
            self.in_pre = true;
            self.pre_open_match = true;
        } else if self.in_list == 0 && !PHRASING_TAGS.contains(name) && VOID_TAGS.contains(name) {
            // <wbr> is both phrasing and void
            self.level -= 1;
            self.start_wrap();
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.next.text(text);
    }
}

/// A record of the position of an unclosed `<blockquote>` element in a
/// document.
#[derive(Debug)]
struct BlockquoteRoot {
    /// The DOM depth of the blockquote element.
    level: u8,
    /// The position of the blockquote element in the output.
    start: Mark,
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
    Graf,
    /// Maybe this line should be a break between two grafs.
    GrafBreak,
}

impl AsRef<str> for GrafPendingState {
    #[inline]
    fn as_ref(&self) -> &str {
        match self {
            GrafPendingState::None => "",
            GrafPendingState::Graf => "<p>",
            GrafPendingState::GrafBreak => "</p><p>",
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

/// A record of a possible `<p>` wrapper.
#[derive(Debug)]
struct GrafWrapPoint {
    /// Insert `<p>` here.
    start: Mark,
    /// Insert `</p>` here.
    end: Option<Mark>,
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

/// List emitter.
#[derive(Debug, Default)]
pub(super) struct ListEmitter {
    /// The stack of currently open list items.
    stack: Vec<ListKind>,
}

impl ListEmitter {
    /// Emits HTML to match the new state given by `bullets`.
    pub fn emit<S: Sink + ?Sized>(&mut self, next: &mut S, bullets: &str) {
        let bullets = bullets.as_bytes();

        let last = self.stack.len();

        // There are three possible states here:
        //
        // 1. transition between dt and dd (new list item)
        // 2. no changes (new list item)
        // 3. more bullets (new list inside last list item)
        // 4. fewer bullets (new list item outside last list)
        let common_end = self
            .stack
            .iter()
            .zip(bullets.iter())
            .take_while(|(lhs, rhs)| lhs.same_parent(ListKind::from(**rhs)))
            .count();

        for item in self.stack.drain(common_end..).rev() {
            Self::end(next, item, true);
        }

        if common_end != 0 && common_end == self.stack.len() && common_end == bullets.len() {
            // Here we are either transitioning dl/dt or li/li
            let old = &mut self.stack[common_end - 1];
            let new = ListKind::from(bullets[common_end - 1]);
            Self::end(next, *old, false);
            next.new_line();
            Self::start(next, new, false);
            *old = new;
        }

        if last != 0 && bullets.len() > common_end {
            next.new_line();
        }

        for item in bullets[common_end..].iter().copied().map(ListKind::from) {
            Self::start(next, item, true);
            self.stack.push(item);
        }
    }

    /// Emits HTML for the end of this kind of list item.
    fn end<S: Sink + ?Sized>(next: &mut S, item: ListKind, end_of_list: bool) {
        match item {
            ListKind::Detail | ListKind::Term => {
                next.tag_end(item.tag_name());
                if end_of_list {
                    next.tag_end("dl");
                }
            }
            ListKind::Ordered | ListKind::Unordered => {
                next.tag_end("li");
                if end_of_list {
                    next.tag_end(item.tag_name());
                }
            }
        }
    }

    /// Emits HTML to finish any incomplete list.
    pub fn finish<S: Sink + ?Sized>(&mut self, next: &mut S) {
        for item in self.stack.drain(..).rev() {
            Self::end(next, item, true);
        }
    }

    /// Returns `true` if there are no list items in the stack.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Emits HTML for the start of this kind of list item.
    fn start<S: Sink + ?Sized>(next: &mut S, item: ListKind, start_of_list: bool) {
        match item {
            ListKind::Detail | ListKind::Term => {
                if start_of_list {
                    next.tag_start_full("dl");
                }
                next.tag_start_full(item.tag_name());
            }
            ListKind::Ordered | ListKind::Unordered => {
                if start_of_list {
                    next.tag_start_full(item.tag_name());
                }
                next.tag_start_full("li");
            }
        }
    }

    /// Returns the tag name of the list item on the top of the stack, or
    /// `None` if there are no current list items.
    pub fn tag_name(&self) -> Option<&str> {
        self.stack.last().map(|kind| match kind {
            ListKind::Ordered | ListKind::Unordered => "li",
            ListKind::Term => "dt",
            ListKind::Detail => "dd",
        })
    }
}

/// A list kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ListKind {
    /// Definition list detail.
    ///
    /// ```wikitext
    /// ; Term : Detail
    ///        ^^^^^^^^
    /// : Definition detail
    /// ^^^^^^^^^^^^^^^^^^^
    /// ```
    Detail,
    /// Ordered list.
    ///
    /// ```wikitext
    /// # Ordered list
    /// ```
    Ordered,
    /// Definition list term.
    ///
    /// ```wikitext
    /// ; Definition term
    /// ```
    Term,
    /// Unordered list.
    ///
    /// ```wikitext
    /// * Unordered list
    /// ```
    Unordered,
}

impl ListKind {
    /// Returns true if `self` is a definition list item.
    #[inline]
    fn is_definition_list(self) -> bool {
        matches!(self, ListKind::Term | ListKind::Detail)
    }

    /// Returns true if `self` has the same parent element as `other`.
    #[inline]
    fn same_parent(self, other: Self) -> bool {
        match self {
            ListKind::Ordered | ListKind::Unordered => self == other,
            ListKind::Term | ListKind::Detail => other.is_definition_list(),
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
        if !std::thread::panicking() {
            debug_assert!(self.0 == MarkableString::NO_FREE, "leaked");
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
///
/// This implementation tries to avoid redoing work by taking already-processed
/// HTML directly from the corresponding processed document HTML.
#[derive(Debug)]
pub(super) struct OutlineEmitter<S: Sink + Markable> {
    /// Pending outline entries.
    entries: Vec<OutlineEmitterEntry>,
    /// The HTML string buffer for this emitter.
    html_buffer: String,
    /// The ID string buffer for this emitter.
    id_buffer: String,
    /// The output.
    next: S,
    /// The currently processing heading.
    state: OutlineEmitterState,
}

impl<S: Sink + Markable> OutlineEmitter<S> {
    /// The list of tags allowed in outlines.
    ///
    /// This list comes from Parsoid `Wt2Html\DOM\Handlers\Headings`.
    const ALLOWED_TAGS: phf::Set<&str> = phf::phf_set! {
        "b", "bdi", "i", "q", "s", "span", "strike", "sub", "sup"
    };

    /// HTML ID attribute prefix.
    const PREFIX: &str = r#" id=""#;

    /// HTML ID attribute suffix.
    const SUFFIX: &str = r#"""#;

    /// Creates a new `OutlineEmitter` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            entries: <_>::default(),
            html_buffer: <_>::default(),
            id_buffer: <_>::default(),
            next,
            state: <_>::default(),
        }
    }

    /// Adds text content from an opaque blob of HTML to the currently
    /// processing outline entry.
    fn add_html(&mut self, html: &str) {
        if self.state.tag.is_none() || self.state.in_attrs {
            return;
        }

        // TODO: SIGH, all extension tag outputs need to be tokenised like this,
        // but then the inner elements need to *not* apply to the outline.
        let text = htmlparser::Tokenizer::from(html).filter_map(|token| {
            token.ok().and_then(|token| {
                if let htmlparser::Token::Text { text } = token {
                    Some(text.as_str())
                } else {
                    None
                }
            })
        });

        for text in text {
            self.html_buffer.push_str(text);

            if matches!(self.state.id, OutlineEmitterStateId::Implicit { .. }) {
                self.id_buffer.push_str(text);
            }
        }
    }

    /// Emits an outline entry with the given `level`, `html`, and `id` to
    /// `outline`.
    fn emit_entry(
        &mut self,
        outline: &mut Outline,
        level: HeadingLevel,
        html: core::ops::Range<usize>,
        id: OutlineEmitterEntryId,
        body_pos: Mark,
    ) {
        let legacy = match id {
            OutlineEmitterEntryId::Implicit {
                start,
                end,
                out_pos,
            } => {
                let id = normalize_section_name(&self.id_buffer[start as usize..end as usize])
                    .map(normalize_fragment)
                    .map(|id| anchor_encode(id, AnchorEncodeMode::Html5));
                let id = outline
                    .push(level, self.html_buffer[html].trim_ascii(), &id)
                    .unwrap_or(&id);
                let legacy = Self::id_to_legacy(id);
                self.next.with_marks([&out_pos], |[out_pos], out| {
                    if let Some(out_pos) = out_pos {
                        let attr = Self::id_to_attr(id);
                        out.insert_str(out_pos, &attr);
                    }
                });
                self.next.free_mark(out_pos);
                legacy
            }
            OutlineEmitterEntryId::Explicit { start, end } => {
                let legacy = self.next.with_marks([&start, &end], |[start, end], out| {
                    let (Some(start), Some(end)) = (start, end) else {
                        return None;
                    };
                    let id =
                        decode_html(&out[start + Self::PREFIX.len()..end - Self::SUFFIX.len()]);
                    if let Some(id) = outline.push(level, self.html_buffer[html].trim_ascii(), &id)
                    {
                        out.replace_range(start..end, &Self::id_to_attr(id));
                        Self::id_to_legacy(id)
                    } else {
                        Self::id_to_legacy(&id)
                    }
                });
                self.next.free_mark(start);
                self.next.free_mark(end);
                legacy
            }
        };

        if let Some(legacy) = legacy {
            self.next.with_marks([&body_pos], |[body_pos], out| {
                if let Some(body_pos) = body_pos {
                    let attr = Self::id_to_attr(&legacy);
                    out.insert_str(body_pos, &format!("<span{attr}></span>"));
                }
            });
        }

        self.next.free_mark(body_pos);
    }

    /// Converts an unencoded ID string to an HTML string.
    #[inline]
    #[must_use]
    fn id_to_attr(id: &str) -> String {
        let id = html_escape::encode_double_quoted_attribute(id);
        format!("{}{id}{}", Self::PREFIX, Self::SUFFIX)
    }

    /// Returns the legacy ID for an entry if it is different from the HTML5 ID.
    #[inline]
    #[must_use]
    fn id_to_legacy(id: &str) -> Option<String> {
        let legacy = anchor_encode(id, AnchorEncodeMode::Legacy);
        (legacy != id).then(|| legacy.into_owned())
    }

    /// Records an entry with the given `level` to be emitted to the global
    /// outline when the emitter is finished.
    fn save_entry(&mut self, level: HeadingLevel) {
        debug_assert_eq!(Some(level), self.state.tag);
        debug_assert!(self.state.stack.is_empty());

        let state = core::mem::take(&mut self.state);
        let id = match state.id {
            OutlineEmitterStateId::Undefined => unreachable!(),
            OutlineEmitterStateId::Explicit { start, end } => {
                let end = end.unwrap_or_else(|| self.next.mark());
                OutlineEmitterEntryId::Explicit { start, end }
            }
            OutlineEmitterStateId::Implicit {
                id_pos: start,
                out_pos,
            } => {
                let end = u16::try_from(self.id_buffer.len()).unwrap();
                OutlineEmitterEntryId::Implicit {
                    out_pos,
                    start,
                    end,
                }
            }
        };
        self.entries.push(OutlineEmitterEntry {
            body_pos: state.body_pos.unwrap(),
            html: state.html,
            id,
            level,
        });

        if let Some(pos) = state.dir {
            self.next.free_mark(pos);
        }
    }
}

chainable!(OutlineEmitter);

impl<S: Sink + Markable> Sink for OutlineEmitter<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.next.comment_start();
    }

    fn entity(&mut self, value: char, raw: &str) {
        if self.state.tag.is_none() || self.state.in_attrs {
            self.next.entity(value, raw);
            return;
        }

        let start = self.next.mark();
        self.next.entity(value, raw);
        self.next.with_marks([&start], |[pos], out| {
            self.html_buffer
                .push_str(pos.map_or("", |start| &out[start..]));
        });
        self.next.free_mark(start);

        if matches!(self.state.id, OutlineEmitterStateId::Implicit { .. }) {
            self.id_buffer.push_str(raw);
        }
    }

    #[inline]
    fn finish(mut self, state: &mut State<'_, '_, '_>) -> String {
        debug_assert!(self.state.body_pos.is_none());
        debug_assert!(self.state.dir.is_none());
        debug_assert!(self.state.tag.is_none());
        let mut entries = core::mem::take(&mut self.entries).into_iter().peekable();
        while let Some(entry) = entries.next() {
            let next_html = entries
                .peek()
                .map_or(self.html_buffer.len(), |next| next.html as usize);
            self.emit_entry(
                &mut state.globals.outline,
                entry.level,
                entry.html as usize..next_html,
                entry.id,
                entry.body_pos,
            );
        }
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn raw_html_block(&mut self, html: &str) {
        self.add_html(html);
        self.next.raw_html_block(html);
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        self.add_html(html);
        self.next.raw_html_inline(html);
    }

    fn tag_attribute_end(&mut self, name: &str) {
        self.next.tag_attribute_end(name);

        if matches!(self.state.stack.last(), Some(tag) if tag.name == "span")
            && name == "dir"
            && let Some(dir) = self.state.dir.take()
        {
            self.next.with_marks([&dir], |[dir], out| {
                let value = dir.map_or("", |dir| &out[dir..]);
                if value.len() > " dir".len() {
                    self.html_buffer.push_str(value);
                }
            });
            self.next.free_mark(dir);
        } else if self.state.in_entry_attrs && name == "id" {
            let OutlineEmitterStateId::Explicit { end, .. } = &mut self.state.id else {
                panic!("attributes all goofed up")
            };
            *end = Some(self.next.mark());
        }
    }

    fn tag_attribute_start(&mut self, name: &str) {
        if matches!(self.state.stack.last(), Some(tag) if tag.name == "span") && name == "dir" {
            self.state.dir = Some(self.next.mark());
        } else if self.state.in_entry_attrs && name == "id" {
            self.state.id = OutlineEmitterStateId::Explicit {
                start: self.next.mark(),
                end: None,
            };
        }
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        self.next.tag_end(name);

        if let Some(tag) = self.state.stack.pop_if(|tag| tag.name == name) {
            if tag.open_end == self.html_buffer.len() {
                self.html_buffer.truncate(tag.open_start);
            } else {
                let _ = write!(self.html_buffer, "</{name}>");
            }
        } else if let Ok(level) = name.parse() {
            self.save_entry(level);
        }
    }

    fn tag_start(&mut self, name: &str) {
        self.next.tag_start(name);

        if let Ok(level) = name.parse() {
            if self.state.tag.is_some() {
                todo!("invalid heading tag nesting");
            }
            self.state.tag = Some(level);
            self.state.html = u32::try_from(self.html_buffer.len()).unwrap();
            self.state.in_entry_attrs = true;
        } else if self.state.tag.is_some()
            && let Some(name) = Self::ALLOWED_TAGS.get_key(name)
        {
            self.state.stack.push(OutlineEmitterTag {
                name,
                open_start: self.html_buffer.len(),
                open_end: self.html_buffer.len(),
            });
            let _ = write!(self.html_buffer, "<{name}");
        }

        self.state.in_attrs = true;
    }

    fn tag_start_end(&mut self, name: &str) {
        if let Some(tag) = self.state.stack.last_mut()
            && tag.name == name
        {
            self.html_buffer.push('>');
            tag.open_end = self.html_buffer.len();
        } else if self.state.in_entry_attrs
            && matches!(self.state.id, OutlineEmitterStateId::Undefined)
        {
            self.state.id = OutlineEmitterStateId::Implicit {
                out_pos: self.next.mark(),
                id_pos: u16::try_from(self.id_buffer.len()).unwrap(),
            };
        }
        self.state.in_attrs = false;
        self.next.tag_start_end(name);
        if self.state.in_entry_attrs {
            self.state.body_pos = Some(self.next.mark());
            self.state.in_entry_attrs = false;
        }
    }

    fn text(&mut self, text: &str) {
        if self.state.tag.is_none() || self.state.in_attrs {
            self.next.text(text);
            return;
        }

        let start = self.next.mark();
        self.next.text(text);
        self.next.with_marks([&start], |[pos], out| {
            self.html_buffer
                .push_str(pos.map_or("", |start| &out[start..]));
        });
        self.next.free_mark(start);

        if matches!(self.state.id, OutlineEmitterStateId::Implicit { .. }) {
            self.id_buffer.push_str(text);
        }
    }
}

/// A fully resolved outline entry, pending insertion to the article outline.
#[derive(Debug)]
struct OutlineEmitterEntry {
    /// The inner position of the heading’s start tag in the output.
    body_pos: Mark,
    /// The start position of the outline entry HTML in the HTML string buffer.
    html: u32,
    /// The position of the ID in the output.
    id: OutlineEmitterEntryId,
    /// The heading level.
    level: HeadingLevel,
}

/// A fully resolved outline anchor ID.
#[derive(Debug)]
enum OutlineEmitterEntryId {
    /// An explicit ID given by the `id` attribute of the heading.
    Explicit {
        /// The start position of the ID in the output.
        start: Mark,
        /// The end position of the ID in the output.
        end: Mark,
    },
    /// An implicit ID generated from the text of the heading.
    Implicit {
        /// The start position of the generated ID in the ID string buffer.
        start: u16,
        /// The end position of the generated ID in the ID string buffer.
        end: u16,
        /// The position where the ID should be inserted in the output.
        out_pos: Mark,
    },
}

/// An in-progress heading for the outline.
#[derive(Debug, Default)]
struct OutlineEmitterState {
    /// The position of the start of the heading tag inner HTML.
    body_pos: Option<Mark>,
    /// The position of the `dir` attribute of a span element.
    dir: Option<Mark>,
    /// The start position for the HTML to emit to the table of contents in the
    /// HTML buffer.
    html: u32,
    /// The ID of the current outline entry.
    id: OutlineEmitterStateId,
    /// If true, the emitter is currently processing attributes of some element.
    in_attrs: bool,
    /// If true, the emitter is currently processing the attributes of the root
    /// element of the outline entry.
    in_entry_attrs: bool,
    /// The stack of open inner HTML elements.
    stack: Vec<OutlineEmitterTag>,
    /// The current level.
    tag: Option<HeadingLevel>,
}

/// An anchor ID.
#[derive(Debug, Default)]
enum OutlineEmitterStateId {
    /// Indeterminate.
    #[default]
    Undefined,
    /// Use an existing ID.
    Explicit {
        /// The start position of the `id` attribute in the output.
        start: Mark,
        /// The end position of the `id` attribute in the output. `None` if the
        /// attribute has not been closed yet.
        end: Option<Mark>,
    },
    /// Generate an ID from the given text.
    Implicit {
        /// The output position for the eventual `id` attribute.
        out_pos: Mark,
        /// The start position of the ID text in the ID buffer.
        id_pos: u16,
    },
}

/// An HTML tag in the outline.
#[derive(Debug)]
struct OutlineEmitterTag {
    /// The HTML tag name.
    name: &'static str,
    /// The position of the start of the last start tag. Used to filter empty
    /// tags from the outline output.
    open_start: usize,
    /// The position of the end of the last start tag. Used to filter empty
    /// tags from the outline output.
    open_end: usize,
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
                    next.tag_start_full("b");
                    next.tag_start_full("i");
                    Self::BI
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
                DashPunctuation, InitialPunctuation, OpenPunctuation, OtherPunctuation,
            },
            get_general_category,
        };
        prev.is_whitespace()
            || (matches!(
                get_general_category(prev),
                DashPunctuation | OpenPunctuation | InitialPunctuation
            ) && !next.is_some_and(char::is_whitespace))
            || (matches!(get_general_category(prev), OtherPunctuation)
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
    fn comment_end(&mut self) {
        self.in_code -= 1;
        self.pop_context();
        self.next.comment_end();
    }

    fn comment_start(&mut self) {
        self.in_code += 1;
        self.push_context();
        self.next.comment_start();
    }

    fn entity(&mut self, value: char, raw: &str) {
        self.push_char(value);
        self.next.entity(value, raw);
    }

    fn finish(self, state: &mut State<'_, '_, '_>) -> String {
        self.next.finish(state)
    }

    fn new_line(&mut self) {
        self.push_char('\n');
        self.next.new_line();
    }

    fn raw_html_block(&mut self, html: &str) {
        self.next.raw_html_block(html);
    }

    fn raw_html_inline(&mut self, html: &str) {
        self.next.raw_html_inline(html);
    }

    fn tag_attribute_end(&mut self, name: &str) {
        if name != "title" {
            self.in_code -= 1;
        }
        self.pop_context();
        self.next.tag_attribute_end(name);
    }

    fn tag_attribute_start(&mut self, name: &str) {
        if name != "title" {
            self.in_code += 1;
        }
        self.push_context();
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        self.in_code -= u8::from(Self::is_code_tag(name));
        if !PHRASING_TAGS.contains(name) {
            self.push_char(' ');
        }
        self.next.tag_end(name);
    }

    fn tag_start(&mut self, name: &str) {
        if name == "br" || name == "hr" {
            self.push_char('\n');
        }
        self.in_code += u8::from(Self::is_code_tag(name));
        self.next.tag_start(name);
    }

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

        // TODO:
        // #[cfg(debug_assertions)]
        // if text.contains(MARKER_PREFIX) {
        //     return Err(Error::StripMarkerInText);
        // }

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
                ' ' if self.in_code == 0
                    && Self::is_french_space(self.prev_chars, peek_array(&mut chars))
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
                    {
                        if out[start..end].bytes().all(|c| c.is_ascii_whitespace()) {
                            out.replace_range(start..end, "\n");
                        } else {
                            out.move_range(start..end, before);
                        }
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
    fn finish(self, state: &mut State<'_, '_, '_>) -> String {
        debug_assert!(self.stack.is_empty());
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn raw_html_block(&mut self, html: &str) {
        self.next.raw_html_block(html);
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        self.next.raw_html_inline(html);
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
    pub fn push(&mut self, name: String) {
        self.tag_blocks.push((self.depth, name));
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
    fn finish(self, state: &mut State<'_, '_, '_>) -> String {
        self.next.finish(state)
    }

    #[inline]
    fn new_line(&mut self) {
        self.next.new_line();
    }

    #[inline]
    fn raw_html_block(&mut self, html: &str) {
        self.next.raw_html_block(html);
    }

    #[inline]
    fn raw_html_inline(&mut self, html: &str) {
        self.next.raw_html_inline(html);
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
    }
}
