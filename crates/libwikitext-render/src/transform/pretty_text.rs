//! Text transformer.

use super::{Sink, chainable};
use crate::{StripMarker, tags::PHRASING_TAGS};
use libwikitext_common::decode_html;

/// Converts runs of text to typographically beautiful HTML.
#[derive(Debug)]
pub(crate) struct PrettyText<S: Sink> {
    /// If true, currently in an HTML start tag.
    in_attr: bool,
    /// The current number of code contexts.
    ///
    /// Pretty typography does not apply in code contexts.
    in_code: u8,
    /// The DOM depth for a code context entered by a `[role=code]` attribute,
    /// as used by `<syntaxhighlight>`.
    in_code_role: CodeRole,
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
    saved_context: Option<[char; 2]>,
}

chainable!(PrettyText);

impl<S: Sink> PrettyText<S> {
    /// Creates a new `PrettyText` chained to `next`.
    #[inline]
    pub fn new(next: S) -> Self {
        Self {
            in_attr: <_>::default(),
            in_code: <_>::default(),
            in_code_role: <_>::default(),
            next,
            prev_chars: Self::new_context(),
            saved_context: <_>::default(),
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
            .saved_context
            .take()
            .expect("symmetrical context stack");
    }

    /// Push the current look-behind buffer to the stack.
    #[inline]
    fn push_context(&mut self) {
        debug_assert!(self.saved_context.is_none());
        self.saved_context = Some(self.prev_chars);
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
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if let StripMarker::NoWiki(text) = marker {
            self.text(&decode_html(text));
        } else {
            self.next.strip_marker(marker);
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if !matches!(name, "alt" | "title") {
            self.in_code -= 1;
            if self.in_code_role == CodeRole::Maybe {
                self.in_code_role = <_>::default();
            }
        }
        self.pop_context();
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if !matches!(name, "alt" | "title") {
            self.in_code += 1;
            if name == "role" {
                self.in_code_role = CodeRole::Maybe;
            }
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
        if let CodeRole::Yes(depth) = &mut self.in_code_role {
            *depth -= 1;
            if *depth == 0 {
                self.in_code -= 1;
                self.in_code_role = <_>::default();
            }
        }
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        if name == "br" || name == "hr" {
            self.push_char('\n');
        }
        self.in_attr = true;
        self.in_code += u8::from(Self::is_code_tag(name));
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.in_attr = false;
        self.next.tag_start_end(name);
        if let CodeRole::Yes(depth) = &mut self.in_code_role {
            if *depth == 0 {
                self.in_code += 1;
            }
            *depth = depth.checked_add(1).unwrap();
        }
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
                // This is supposed to also occur in code contexts except for
                // attributes
                ' ' if (self.in_code == 0 || !self.in_attr)
                    && Self::is_french_space(self.prev_chars, peek_array(&mut chars))
                        == Some(true) =>
                {
                    flush(&mut self.next, &mut flushed, text, index, c);
                    self.next.text("\u{00a0}");
                }
                // TODO: Track balance to differentiate between e.g.
                // `The ‘90s’` vs `In the ’90s` and other pathological cases
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

        if self.in_code_role == CodeRole::Maybe {
            if text == "code" {
                self.in_code_role = CodeRole::Yes(0);
            } else {
                self.in_code_role = CodeRole::No;
            }
        }
    }
}

/// A tracking state for an element which may contain a `role="code"` attribute.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CodeRole {
    /// No code role.
    #[default]
    No,
    /// Found `role`, awaiting value.
    Maybe,
    /// In a code role at the given DOM depth.
    Yes(u8),
}
