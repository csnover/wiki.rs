//! HTML emitters for Wikitext fragments that require state management.

use crate::wikitext::TextStyle;
use core::fmt;

/// Emitter for implicit grafs.
///
/// Paragraph rules, like everything in Wikitext (try grepping for *that*!), are
/// absolutely insane nonsense. Just look at this:
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
/// becomes:
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
/// Three newlines in a row are supposed to also emit a `<br>`, but that looks
/// ugly and is harder to implement, so we don’t do that (until, sigh, it
/// becomes necessary for something to render properly).
#[derive(Debug, Default)]
pub(super) struct GrafEmitter {
    /// Whether the current line includes a block level element.
    pub(super) has_block: bool,
    /// The start position of the last line in the graf.
    last_line: usize,
    /// The number of new lines seen in the current sequence of new lines.
    sequential: usize,
    /// The start position of the current graf.
    start: usize,
}

impl GrafEmitter {
    /// Completes the line emitter.
    #[inline]
    pub(super) fn finish(mut self, out: &mut String) {
        self.next(out);
    }

    /// Resets the state of the line emitter for a new line.
    #[inline]
    pub(super) fn next(&mut self, out: &mut String) {
        // if seen a block:
        //   emit graf around the previous content
        //   reset the start and the empty-line count
        // else if seen no content:
        //   increment the empty-line count
        // else:
        //   keep going

        if self.has_block {
            // TODO: Do something with `sequential`
            if self.last_line != self.start {
                out.insert_str(self.last_line, "</p>");
                out.insert_str(self.start, "<p>");
            }
            self.start = out.len() + 1;
            self.sequential = 0;
        } else if self.last_line == out.len() {
            self.sequential += 1;
            // TODO: Do the third sequence <br>
            if self.sequential.is_multiple_of(2)
                && out[self.start..self.last_line].contains(|c: char| c != '\n')
            {
                out.insert_str(self.last_line, "</p>");
                out.insert_str(self.start, "<p>");
                self.start = out.len() + 1;
            }
        } else {
            self.sequential = 1;
        }

        out.push('\n');
        self.has_block = false;
        self.last_line = out.len();
    }
}

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
