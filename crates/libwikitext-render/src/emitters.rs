//! HTML emitters for Wikitext fragments that require state management.

use super::transform::Sink;
use libwikitext_parse::{TextStyle, TextStyleHint};

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

/// A Wikitext table frame.
#[derive(Debug)]
pub(super) struct TableState {
    /// If `Some`, the table has ended, and this state waiting for the line end
    /// to chomp trailing whitespace and emit any indent end elements. The
    /// string contains any whitespace that maybe needs to be chomped.
    pub after_table: Option<String>,
    /// If true, a Wikitext table row, header, or data token has been seen.
    pub has_tbody: bool,
    /// The indent hack count for this table.
    pub indent: u8,
    /// The tag name of the currently open table caption, header, or data tag.
    pub last_tag: Option<&'static str>,
    /// The half-parsed attributes for a pending table row.
    pub tr_attrs: String,
    /// If true, a `<tr>` has been emitted and needs to be closed.
    pub tr_emitted: bool,
}

impl TableState {
    /// Creates a new `TableState` with the given `indent`.
    #[inline]
    pub fn new(indent: u8) -> Self {
        Self {
            after_table: <_>::default(),
            has_tbody: <_>::default(),
            indent,
            last_tag: <_>::default(),
            tr_attrs: <_>::default(),
            tr_emitted: <_>::default(),
        }
    }

    /// Buffers or emits `text` to `next` depending on the [`Self::after_table`]
    /// and [`Self::indent`].
    pub fn after_table_text<S: Sink + ?Sized>(&mut self, next: &mut S, text: &str) {
        if self.indent != 0
            && let Some(ws) = &mut self.after_table
        {
            let trimmed = text.trim_ascii_end();
            if !trimmed.is_empty() {
                next.text(ws);
                ws.clear();
            }
            next.text(trimmed);
            ws.push_str(&text[trimmed.len()..]);
        } else {
            next.text(text);
        }
    }

    /// Finishes a table frame.
    pub fn finish<S: Sink + ?Sized>(mut self, next: &mut S, last: bool) {
        if self.after_table.is_none() {
            self.table_end(next, last);
        }
        for _ in 0..self.indent {
            next.tag_end("dd");
            next.tag_end("dl");
        }
    }

    /// Flushes any whitespace stored in the [`Self::after_table`] buffer to
    /// `next`.
    #[inline]
    pub fn flush_after_table<S: Sink + ?Sized>(&mut self, next: &mut S) {
        if let Some(ws) = &mut self.after_table
            && !ws.is_empty()
        {
            next.text(ws);
            ws.clear();
        }
    }

    /// Closes the table for a table frame.
    pub fn table_end<S: Sink + ?Sized>(&mut self, next: &mut S, last: bool) {
        // Whitespace handling for the `last` case to have byte-exact output
        // against the PHP parser is annoying because that parser would just
        // chomp any trailing newline no matter where it came from. In wiki.rs
        // the newline from the source text is the source of the newline, but
        // for a half-finished table, the last line needs extra special handling
        if let Some(name) = self.last_tag {
            if last {
                next.new_line();
                // The original parser has a bug where it always emits `"td"`
                // and then relies on the HTML5 parser to fix it. This gets
                // exercised by the test 'Fuzz testing: Parser16', so to pass
                // the test suite, it is necessary to do this bogus thing.
                next.tag_end("td");
                next.new_line();
            } else {
                next.tag_end(name);
            }
        }
        if self.tr_emitted {
            next.tag_end("tr");
            if last {
                next.new_line();
            }
        }
        if !self.has_tbody {
            if last {
                next.new_line();
            }
            next.tag_start_full("tr");
            next.tag_start_full("td");
            next.tag_end("td");
            next.tag_end("tr");
            if last {
                next.new_line();
            }
        }
        next.tag_end("table");

        debug_assert!(self.after_table.is_none());
        self.after_table.get_or_insert_default();
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
            TextStyle::BoldItalic(hint) => match self {
                Self::None => match hint {
                    TextStyleHint::BoldFirst => {
                        next.tag_start_full("b");
                        next.tag_start_full("i");
                        Self::BI
                    }
                    TextStyleHint::ItalicFirst => {
                        next.tag_start_full("i");
                        next.tag_start_full("b");
                        Self::IB
                    }
                    TextStyleHint::Last => Self::None,
                },
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
