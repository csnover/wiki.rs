//! Transforms “visual whitespace” into HTML paragraphs.

use super::{Buffer, Sink, chainable};
use crate::StripMarker;
use libwikitext_parse::MARKER_PREFIX;

/// Implicit “visual whitespace” paragraphs (grafs) wrapper. Implicit grafs may
/// be runs of plain text, which will be wrapped by `<p>`, or runs of plain text
/// prefixed by a single space, which will be wrapped by `<pre>`.
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
/// any one developer to understand how anything works. Because the former
/// operates on lines of text and the latter operates on a DOM, edge cases make
/// it difficult to combine them.
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "should care, don’t care. hate this code"
)]
pub(crate) struct GrafWrapper<S: Sink> {
    /// The next line buffer.
    buffer: Buffer,
    /// If true, the line contains an end tag which triggers a graf state
    /// transition.
    close_match: bool,
    /// The currently active graf.
    current: State,
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
    /// State tracking for the current line of source Wikitext.
    line_state: LineMetadata,
    /// The output.
    next: S,
    /// If true, the line contains a start tag which triggers a graf state
    /// transition.
    open_match: bool,
    /// The next graf to emit.
    pending: Pending,
    /// If true, the line contains a `</pre>`.
    pre_close_match: bool,
    /// If true, the line contains a `<pre>`.
    pre_open_match: bool,
}

impl<S: Sink> GrafWrapper<S> {
    /// Creates a new `GrafWrapper` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            buffer: <_>::default(),
            close_match: <_>::default(),
            current: <_>::default(),
            in_block: <_>::default(),
            in_blockquote: <_>::default(),
            in_caption: <_>::default(),
            in_list: <_>::default(),
            in_pre: <_>::default(),
            line_state: <_>::default(),
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
            State::None => return,
            State::Graf => self.next.tag_end("p"),
            State::Pre => self.next.tag_end("pre"),
        }
        if !finishing {
            self.next.new_line();
        }
    }

    /// Finishes processing of a line of source text.
    fn end_line(&mut self, last_line: bool) {
        let mut skip_first_char = false;
        if self.open_match || self.close_match {
            self.pending = Pending::None;
            if !self.in_pre || self.pre_open_match {
                self.close(false);
            }
            if self.pre_open_match || self.pre_close_match {
                self.in_pre = !self.pre_close_match;
            }
            self.in_block = !self.close_match;
        } else if !self.in_block && !self.in_pre {
            if self.is_pre_line() {
                if self.current != State::Pre {
                    self.pending = Pending::None;
                    self.close(false);
                    self.next.tag_start_full("pre");
                    self.current = State::Pre;
                }
                skip_first_char = true;
            } else if self.is_style_line() {
                if self.pending != Pending::None {
                    self.close(false);
                    self.pending = Pending::None;
                }
            } else if self.is_empty_line() {
                if let Some(new_state) = self.pending.emit(&mut self.next) {
                    self.next.tag_start_full("br");
                    self.current = new_state;
                } else if self.current != State::Graf {
                    self.close(false);
                    self.pending = Pending::Open;
                } else {
                    self.pending = Pending::Split;
                }
            } else if let Some(new_state) = self.pending.emit(&mut self.next) {
                self.current = new_state;
            } else if self.current != State::Graf {
                self.close(false);
                self.next.tag_start_full("p");
                self.current = State::Graf;
            }
        }

        if self.pending == Pending::None {
            self.buffer.flush_into(&mut self.next, skip_first_char);
            if !last_line || self.current != State::None {
                self.next.new_line();
            }
        } else {
            self.buffer.clear();
        }

        self.pre_open_match = <_>::default();
        self.pre_close_match = <_>::default();
        self.open_match = <_>::default();
        self.close_match = <_>::default();
        self.line_state = <_>::default();
    }

    /// Returns true if the currently buffered line is empty or contains only
    /// ASCII whitespace.
    #[inline]
    fn is_empty_line(&self) -> bool {
        !self.line_state.contains_non_ascii_whitespace
    }

    /// Returns true if the currently buffered line contains only `<style>` and
    /// `<link>` tags.
    #[inline]
    fn is_style_line(&self) -> bool {
        self.line_state.style_line == StyleLine::Yes
    }

    /// Returns true if the currently buffered line should be treated like a
    /// preformatted line.
    #[inline]
    fn is_pre_line(&self) -> bool {
        !self.in_blockquote
            && self.line_state.starts_with(' ')
            && (self.current == State::Pre || self.line_state.contains_non_ascii_whitespace)
    }

    /// Causes `GrafWrapper` to treat new line tokens as text. This is required
    /// for correct handling of image captions. (The original parser hacked out
    /// the newlines.)
    #[inline]
    pub fn set_in_caption(&mut self, in_caption: bool) {
        self.in_caption = in_caption;
    }

    /// Disables the `GrafWrapper`, causing it to pass through tokens. This is
    /// required for correct handling of lists. (The original parser handled
    /// lists in the same component.)
    #[inline]
    pub fn set_in_list(&mut self, in_list: bool) {
        self.pending = Pending::None;
        self.in_list = in_list;
        if in_list {
            self.close(false);
        }
    }
}

chainable!(GrafWrapper);

impl<S: Sink> Sink for GrafWrapper<S> {
    #[inline]
    fn comment_end(&mut self) {
        if self.in_list {
            self.next.comment_end();
        } else {
            self.line_state.update_text("<");
            self.buffer.comment_end();
        }
    }

    #[inline]
    fn comment_start(&mut self) {
        if self.in_list {
            self.next.comment_start();
        } else {
            self.line_state.update_text("<");
            self.buffer.comment_start();
        }
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.in_list {
            self.next.entity(value, raw);
        } else {
            self.line_state.update_text("&");
            self.buffer.entity(value, raw);
        }
    }

    #[inline]
    fn finish(mut self) -> String {
        self.end_line(true);
        self.close(true);
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.in_caption {
            self.line_state.update_text("\n");
            self.buffer.new_line();
        } else if self.in_list {
            self.next.new_line();
        } else {
            self.end_line(false);
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if self.in_list {
            self.next.strip_marker(marker);
        } else {
            match marker {
                StripMarker::General(s) => {
                    // General strip markers need to be unstripped before now
                    // TODO: Make things less silly.
                    debug_assert!(!matches!(marker, StripMarker::General(_)));
                    // In the original parser, general markers are unstripped for
                    // this step
                    self.line_state.update_text(s);
                }
                StripMarker::NoWiki(_) => {
                    // In the original parser, nowiki markers are still markers for
                    // this step
                    self.line_state.update_text(MARKER_PREFIX);
                }
                _ => {}
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
            self.line_state.update_tag_end(name);
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
            self.line_state.update_tag_start(name);
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
            self.line_state.update_text(text);
            self.buffer.text(text);
        }
    }
}

/// Required metadata about a line of input text.
#[derive(Debug, Default)]
struct LineMetadata {
    /// If true, the line contained something other than ASCII whitespace.
    contains_non_ascii_whitespace: bool,
    /// The first raw HTML character of the line.
    starts_with: Option<char>,
    /// The tracking state for a line which may contain only `<style>` and
    /// `<link>` elements.
    style_line: StyleLine,
}

impl LineMetadata {
    /// Returns true if the contents of the buffer start with the given
    /// character.
    fn starts_with(&self, c: char) -> bool {
        self.starts_with.is_some_and(|ch| ch == c)
    }

    /// Update the state for an HTML end tag with the given `name`.
    fn update_tag_end(&mut self, name: &str) {
        self.contains_non_ascii_whitespace = true;
        self.starts_with.get_or_insert('<');

        if name != "style" || self.style_line != StyleLine::InStyle {
            self.style_line = StyleLine::No;
        } else {
            self.style_line = StyleLine::Yes;
        }
    }

    /// Update the state for an HTML start tag with the given `name`.
    fn update_tag_start(&mut self, name: &str) {
        self.contains_non_ascii_whitespace = true;
        self.starts_with.get_or_insert('<');

        if name == "style" {
            // `<style><style>` is stupid, but legal
            if self.style_line != StyleLine::No {
                self.style_line = StyleLine::InStyle;
            }
        } else if name == "link" {
            if self.style_line == StyleLine::Start {
                self.style_line = StyleLine::Yes;
            }
        } else {
            self.style_line = StyleLine::No;
        }
    }

    /// Updates some metadata used by `GrafWrapper`, which was originally a hack
    /// and so seems to always require a hack *somewhere* to function.
    fn update_text(&mut self, text: &str) {
        if !self.contains_non_ascii_whitespace {
            self.contains_non_ascii_whitespace = text.bytes().any(|b| !b.is_ascii_whitespace());
            if self.starts_with.is_none() {
                self.starts_with = text.chars().next();
            }
        }

        if self.style_line == StyleLine::Start
            || (self.style_line == StyleLine::Yes && text.bytes().any(|b| !b.is_ascii_whitespace()))
        {
            self.style_line = StyleLine::No;
        }
    }
}

/// Graf wrapper pending output state.
///
/// This is used when the production of a line is ambiguous and cannot be
/// resolved until a subsequent line can offer disambiguation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Pending {
    /// Emitting nothing.
    #[default]
    None,
    /// Maybe this line should be a graf.
    Open,
    /// Maybe this line should be a break between two grafs.
    Split,
}

impl Pending {
    /// Emits `self` to `next`, returning a new `GrafState` if something was
    /// emitted.
    fn emit<S: Sink + ?Sized>(&mut self, next: &mut S) -> Option<State> {
        match self {
            Self::None => None,
            Self::Open => {
                next.tag_start_full("p");
                *self = Self::None;
                Some(State::Graf)
            }
            Self::Split => {
                next.tag_end("p");
                next.tag_start_full("p");
                *self = Self::None;
                Some(State::Graf)
            }
        }
    }
}

/// Graf wrapper state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum State {
    /// Emitting nothing.
    #[default]
    None,
    /// Emitting a normal graf (`<p>`).
    Graf,
    /// Emitting a preformatted graf (`<pre>`).
    Pre,
}

/// A tracking state for a line of source HTML which may contain only `<style>`
/// or `<link>`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum StyleLine {
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
