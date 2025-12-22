//! HTML emitters for Wikitext fragments that require state management.

use super::tags;
use crate::wikitext::TextStyle;
use core::fmt;

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
#[derive(Debug, Default)]
// Clippy: Should care, don’t care, hate this code.
#[allow(clippy::struct_excessive_bools)]
pub(super) struct GrafEmitter {
    // State for a single line:
    /// If true, the line contains an end tag which triggers a graf state
    /// transition.
    close_match: bool,
    /// The start position of the current line of the document.
    line_start: usize,
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
    in_list: usize,
    /// If true, the document is currently inside a `<pre>`.
    in_pre: bool,
    /// The current DOM depth.
    level: usize,
    /// The next graf to emit.
    pending: GrafPendingState,
}

impl GrafEmitter {
    /// Updates the graf emitter state for the given end tag.
    #[inline]
    pub(super) fn after_end_tag(&mut self, out: &str, name_lower: &str) {
        if !tags::PHRASING_TAGS.contains(name_lower) {
            self.level -= 1;
            // After transitioning back to a blockquote root or document root,
            // the next content is unconditionally graf-wrapped. (This is the
            // `RemexCompatMunger` half of this bullshit)
            self.start_wrap(out.len());
        }
    }

    /// Updates the graf emitter state for the given start tag.
    pub(super) fn after_start_tag(&mut self, out: &str, name_lower: &str) {
        self.open_match |= BLOCK_TAG.contains(name_lower) || ALWAYS_TAG.contains(name_lower);
        self.close_match |= ANTI_BLOCK_TAG.contains(name_lower) || NEVER_TAG.contains(name_lower);

        if name_lower == "blockquote" {
            self.blockquote_roots.push(BlockquoteRoot {
                level: self.level,
                start: out.len(),
            });
            // Any transition into a blockquote needs to trigger a line
            // transition because all text in a blockquote is unconditionally
            // graf-wrapped. (This is the `RemexCompatMunger` half of this
            // bullshit)
            self.start_wrap(out.len());
        } else if name_lower == "pre" {
            self.in_pre = true;
            self.pre_open_match = true;
        }
    }

    /// Updates the graf emitter state for the given end tag.
    pub(super) fn before_end_tag(&mut self, out: &str, name_lower: &str) {
        // Any transition out of a blockquote needs to trigger a line transition
        // because all text in a blockquote is unconditionally graf-wrapped.
        // (This is the `RemexCompatMunger` half of this bullshit)
        if name_lower == "blockquote" {
            self.end_wrap(out.len());
            self.blockquote_roots
                .pop_if(|root| self.level == root.level)
                .expect("blockquote roots stack corruption");
        } else if name_lower == "pre" {
            self.pre_close_match = true;
        }

        self.open_match |= ANTI_BLOCK_TAG.contains(name_lower) || ALWAYS_TAG.contains(name_lower);
        self.close_match |= BLOCK_TAG.contains(name_lower) || NEVER_TAG.contains(name_lower);
    }

    /// Updates the graf emitter state for the given start tag.
    #[inline]
    pub(super) fn before_start_tag(&mut self, out: &str, name_lower: &str) {
        // Any transition from a document root or blockquote root to
        // non-phrasing content must trigger an unconditional graf-wrap of any
        // content on the line prior to the transition. (This is the
        // `RemexCompatMunger` half of this bullshit)
        if !tags::PHRASING_TAGS.contains(name_lower) {
            self.end_wrap(out.len());
            self.level += 1;
        }
    }

    /// Updates the graf emitter state for the end of a block strip marker.
    #[inline]
    pub(super) fn block_end(&mut self, out: &str) {
        self.close_match = true;
        self.level -= 1;
        self.start_wrap(out.len());
    }

    /// Updates the graf emitter state for the start of a block strip marker.
    #[inline]
    pub(super) fn block_start(&mut self, out: &str) {
        self.open_match = true;
        self.end_wrap(out.len());
        self.level += 1;
    }

    /// Emits the end of a graf to the output.
    fn close(&mut self, out: &mut String, index: Option<usize>) {
        self.in_pre = false;

        let tag = match core::mem::take(&mut self.current) {
            GrafState::None => return,
            GrafState::Graf => "</p>",
            GrafState::Pre => "</pre>",
        };

        if let Some(index) = index {
            out.insert_str(index, tag);
        } else {
            *out += tag;
        }
    }

    /// Finishes processing of a line of source text.
    pub(super) fn end_line(&mut self, out: &mut String) {
        // I’m doing the bad thing of writing some “what” comments in here
        // because this algorithm is incoherent

        if self.open_match || self.close_match {
            // This line had a state-changing tag somewhere inside, which means
            // that it is definitely not a graf line
            self.pending = GrafPendingState::None;

            // This is the `RemexCompatMunger` half of this bullshit which
            // inserts grafs around lines of text that are directly inside the
            // document root or a blockquote
            self.p_wrap(out);

            if !self.in_pre || self.pre_open_match {
                // If this line has a `<pre>` tag, or we were not already in a
                // preformatted context, then this line should not be included
                // in any previous graf, so finish any graf from the previous
                // line(s)
                self.close(out, Some(self.line_start));
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
            // If this line was not inside a graf-suppressing block or `<pre>`
            // element, maybe it’s time to emit something!
            let has_content = out[self.line_start..].contains(|c: char| !c.is_ascii_whitespace());

            if self.blockquote_roots.is_empty()
                && (self.current == GrafState::Pre || has_content)
                && out[self.line_start..].starts_with(' ')
            {
                // So long as this is not a line inside a blockquote—because
                // those are apparently special—this line is either a
                // continuation of, or a transition into, a preformatted graf

                if self.current == GrafState::Pre {
                    // The space prefix must be removed or the preformatted text
                    // will be improperly indented in the output
                    out.remove(self.line_start);
                } else {
                    // The tags are emitted backwards because this is an
                    // insertion; this will either be `</p><pre>` or `<pre>`.
                    // As in the other branch, the space prefix is removed, but
                    // here it is removed by overwriting
                    out.replace_range(self.line_start..=self.line_start, "<pre>");
                    self.close(out, Some(self.line_start));
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
                    out.insert_str(self.line_start, "<br>");
                    out.insert_str(self.line_start, self.pending.as_ref());
                    self.pending = GrafPendingState::None;
                    self.current = GrafState::Graf;
                } else if self.current != GrafState::Graf {
                    // An empty line when not in a graf means to transition into
                    // a pending graf, since the next line may be a continuation
                    // of a graf or it may be a line containing state-changing
                    // tags
                    self.close(out, Some(self.line_start));
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
                out.insert_str(self.line_start, self.pending.as_ref());
                self.pending = GrafPendingState::None;
                self.current = GrafState::Graf;
            } else if self.current != GrafState::Graf {
                // Got a new non-empty line, and we were *not* in a pending graf
                // state, but *were* in a non-graf context, so this line
                // transitioned from a non-graf or preformatted graf to a text
                // graf. These tags are emitted backwards because it is an
                // insertion; this will either be `<p>` or `</pre><p>`
                out.insert_str(self.line_start, "<p>");
                self.close(out, Some(self.line_start));
                self.current = GrafState::Graf;
            }
        }

        // This is the point where the “buffered” text would be emitted, so
        // anything before now needs to be `insert`, and anything after here
        // needs to be `append`
        if self.pending == GrafPendingState::None
            && self.current != GrafState::None
            && self.in_list == 0
        {
            out.push('\n');
        }

        self.line_start = out.len();
        self.open_match = false;
        self.close_match = false;
        self.pre_open_match = false;
        self.pre_close_match = false;
        debug_assert!(
            self.wrap_points.is_empty(),
            "did not drain wrappers somehow"
        );
    }

    /// Restores normal processing of lines.
    #[inline]
    pub(super) fn end_list(&mut self) {
        self.pending = GrafPendingState::None;
        self.in_list -= 1;
    }

    /// Finishes processing the document.
    #[inline]
    pub(super) fn finish(mut self, out: &mut String) {
        debug_assert_eq!(self.level, 0);
        self.p_wrap(out);
        self.close(out, Some(self.line_start));
    }

    /// Wraps bare plain text content within a line also containing non-phrasing
    /// elements into grafs.
    fn p_wrap(&mut self, out: &mut String) {
        if let Some(last) = self.wrap_points.last_mut() {
            if out[last.start..].bytes().all(|c| c.is_ascii_whitespace()) {
                // A non-phrasing element was at the end of the line
                self.wrap_points.pop();
            } else {
                last.end.get_or_insert(out.len());
            }
        }

        // Because the content is being inserted rather than appended, the
        // order of operations is backwards
        for GrafWrapPoint { start, end } in self.wrap_points.drain(..).rev() {
            out.insert_str(end.unwrap(), "</p>");
            out.insert_str(start, "<p>");
        }
    }

    /// Inhibits normal processing of lines.
    #[inline]
    pub(super) fn start_list(&mut self, out: &mut String) {
        self.close(out, None);
        self.pending = GrafPendingState::None;
        self.in_list += 1;
    }

    /// Marks the end of a p-wrapper.
    fn end_wrap(&mut self, end: usize) {
        let start = if self.level == 0 {
            self.line_start
        } else if let Some(root) = self.blockquote_roots.last()
            && root.level == self.level
        {
            root.start.max(self.line_start)
        } else {
            // Non-phrasing element in some intermediate root which is not the
            // document root nor the current blockquote root
            return;
        };

        if let Some(last) = self.wrap_points.last_mut() {
            if last.start == end {
                // Two non-phrasing elements were directly adjacent
                self.wrap_points.pop();
            } else {
                debug_assert!(last.end.is_none());
                last.end.get_or_insert(end);
            }
        } else if start != end {
            // Non-phrasing element, not at the start of the root
            self.wrap_points.push(GrafWrapPoint {
                start,
                end: Some(end),
            });
        }
    }

    /// Marks the start of a possible p-wrapper.
    fn start_wrap(&mut self, start: usize) {
        if self.level == 0
            || matches!(self.blockquote_roots.last(), Some(last) if last.level == self.level)
        {
            debug_assert!(matches!(
                self.wrap_points.last(),
                None | Some(GrafWrapPoint { end: Some(_), .. })
            ));
            self.wrap_points.push(GrafWrapPoint { start, end: None });
        }
    }
}

/// A record of the position of an unclosed `<blockquote>` element in a
/// document.
#[derive(Debug)]
struct BlockquoteRoot {
    /// The DOM depth of the blockquote element.
    level: usize,
    /// The position of the blockquote element in the output.
    start: usize,
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
#[derive(Debug, Eq, PartialEq)]
struct GrafWrapPoint {
    /// Insert `<p>` here.
    start: usize,
    /// Insert `</p>` here.
    end: Option<usize>,
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
    pub(super) stack: Vec<ListKind>,
}

impl ListEmitter {
    /// Emits HTML to match the new state given by `bullets`.
    pub fn emit<W: fmt::Write + ?Sized>(&mut self, out: &mut W, bullets: &str) -> fmt::Result {
        let bullets = bullets.as_bytes();

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
            item.end(out, true)?;
        }

        if common_end != 0 && common_end == self.stack.len() && common_end == bullets.len() {
            // Here we are either transitioning dl/dt or li/li
            let old = &mut self.stack[common_end - 1];
            let new = ListKind::from(bullets[common_end - 1]);
            old.end(out, false)?;
            new.start(out, false)?;
            *old = new;
        }

        for item in bullets[common_end..].iter().copied().map(ListKind::from) {
            item.start(out, true)?;
            self.stack.push(item);
        }

        Ok(())
    }

    /// Emits HTML to finish any incomplete list.
    pub fn finish<W: fmt::Write + ?Sized>(&mut self, out: &mut W) -> fmt::Result {
        for item in self.stack.drain(..).rev() {
            item.end(out, true)?;
        }
        Ok(())
    }
}

/// A list kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ListKind {
    /// Ordered list.
    ///
    /// ```wikitext
    /// # Ordered list
    /// ```
    Ordered,
    /// Unordered list.
    ///
    /// ```wikitext
    /// * Unordered list
    /// ```
    Unordered,
    /// Definition list term.
    ///
    /// ```wikitext
    /// ; Definition term
    /// ```
    Term,
    /// Definition list detail.
    ///
    /// ```wikitext
    /// ; Term : Detail
    ///        ^^^^^^^^
    /// : Definition detail
    /// ^^^^^^^^^^^^^^^^^^^
    /// ```
    Detail,
}

impl ListKind {
    /// Emits HTML for the end of this kind of list item.
    fn end<W: fmt::Write + ?Sized>(self, out: &mut W, end_of_list: bool) -> fmt::Result {
        match self {
            ListKind::Detail | ListKind::Term => {
                write!(out, "</{}>", self.tag_name())?;
                if end_of_list {
                    out.write_str("</dl>")?;
                }
            }
            ListKind::Ordered | ListKind::Unordered => {
                out.write_str("</li>")?;
                if end_of_list {
                    write!(out, "</{}>", self.tag_name())?;
                }
            }
        }
        Ok(())
    }

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

    /// Emits HTML for the start of this kind of list item.
    fn start<W: fmt::Write + ?Sized>(self, out: &mut W, start_of_list: bool) -> fmt::Result {
        match self {
            ListKind::Detail | ListKind::Term => {
                if start_of_list {
                    out.write_str("<dl>")?;
                }
                write!(out, "<{}>", self.tag_name())?;
            }
            ListKind::Ordered | ListKind::Unordered => {
                if start_of_list {
                    write!(out, "<{}>", self.tag_name())?;
                }
                out.write_str("<li>")?;
            }
        }
        Ok(())
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
    pub fn emit<W: fmt::Write + ?Sized>(&mut self, out: &mut W, style: TextStyle) -> fmt::Result {
        // Because I don’t care and we aren’t buffering tags, this does not
        // bother with the pedantic attempt to avoid extra formatting tags by
        // recording the position of a None -> BoldItalic transition and then
        // only emitting once the next tag shows up so that it is known whether
        // the order should be BI or IB. Instead we just emit BI and suffer the
        // consequences of emitting a whole extra tag later if it should’ve been
        // IB (which, technically, because the HTML5 spec has defined rules
        // about fixing mismatched tags, it does not even really matter if they
        // are emitted in order).
        match style {
            TextStyle::Bold(..) => match self {
                Self::B => {
                    out.write_str("</b>")?;
                    *self = Self::None;
                }
                Self::BI => {
                    out.write_str("</i></b><i>")?;
                    *self = Self::I;
                }
                Self::None => {
                    out.write_str("<b>")?;
                    *self = Self::B;
                }
                Self::I => {
                    out.write_str("<b>")?;
                    *self = Self::IB;
                }
                Self::IB => {
                    out.write_str("</b>")?;
                    *self = Self::I;
                }
            },
            TextStyle::BoldItalic => match self {
                Self::None => {
                    out.write_str("<b><i>")?;
                    *self = Self::BI;
                }
                Self::B => {
                    out.write_str("</b><i>")?;
                    *self = Self::I;
                }
                Self::BI => {
                    out.write_str("</i></b>")?;
                    *self = Self::None;
                }
                Self::I => {
                    out.write_str("</i><b>")?;
                    *self = Self::B;
                }
                Self::IB => {
                    out.write_str("</b></i>")?;
                    *self = Self::None;
                }
            },
            TextStyle::Italic => match self {
                Self::None => {
                    out.write_str("<i>")?;
                    *self = Self::I;
                }
                Self::B => {
                    out.write_str("<i>")?;
                    *self = Self::BI;
                }
                Self::BI => {
                    out.write_str("</i>")?;
                    *self = Self::B;
                }
                Self::I => {
                    out.write_str("</i>")?;
                    *self = Self::None;
                }
                Self::IB => {
                    out.write_str("</b></i><b>")?;
                    *self = Self::B;
                }
            },
        }
        Ok(())
    }

    /// Emits HTML to finish any incomplete style.
    pub fn finish<W: fmt::Write + ?Sized>(&mut self, out: &mut W) -> fmt::Result {
        match self {
            Self::None => {}
            Self::B => out.write_str("</b>")?,
            Self::BI => out.write_str("</i></b>")?,
            Self::I => out.write_str("</i>")?,
            Self::IB => out.write_str("</b></i>")?,
        }
        *self = Self::None;
        Ok(())
    }
}
