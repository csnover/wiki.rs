//! A [`Sink`] that serialises its input into an HTML string.

use super::{
    Sink,
    markable_string::{Mark, Markable, MarkableString},
};
use crate::StripMarker;

/// Final accumulator for HTML.
///
/// This sink assumes that previous stages will have done all the necessary
/// work of ensuring that the DOM is as well-formed as it needs to be. This
/// sink should receive balanced tags and balanced attributes, and does nothing
/// other than concatenate calls into a string of HTML.
#[derive(Debug, Default)]
pub(crate) struct Accumulator {
    /// The target string buffer.
    inner: MarkableString,
    /// If true, currently in an HTML start tag.
    in_attr: bool,
}

impl Accumulator {
    /// Creates a new `Accumulator`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Extracts a string slice containing the entire `Accumulator`.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    /// Returns the length of this `Accumulator` in bytes.
    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Wikitext ≥2**32 is impossible"
    )]
    pub fn len(&self) -> u32 {
        self.inner.len() as u32
    }

    /// Truncates the accumulator at `pos`.
    #[inline]
    pub fn truncate(&mut self, pos: u32) {
        self.inner.truncate(pos as usize);
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
    fn strip_marker(&mut self, _: &StripMarker<'_>) {
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
