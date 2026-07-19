//! Text replacement transformer.

use super::{Sink, chainable, flush_ws, tokenise};
use crate::StripMarker;
use icu_locale::Locale;
use libwikitext_convert::Converter;
use std::borrow::Cow;

/// Converts phrases in text using an automatic language converter.
#[derive(Debug)]
pub(crate) struct ReplaceText<S: Sink> {
    /// The accumulator for a run of text.
    buffer: String,
    /// The language converter.
    converter: Converter,
    /// If true, currently processing an attribute.
    in_attr: bool,
    /// The current number of code contexts.
    ///
    /// Text replacement does not apply in code contexts.
    in_code: u8,
    /// The output.
    next: S,
    /// Manual replacement terms table.
    terms: Vec<(String, String)>,
    /// Target locale.
    to: Locale,
}

chainable!(ReplaceText);

impl<S: Sink> ReplaceText<S> {
    /// Creates a new `PrettyText` chained to `next`.
    #[inline]
    pub fn new(converter: Converter, to: Locale, next: S) -> Self {
        Self {
            buffer: <_>::default(),
            converter,
            in_attr: <_>::default(),
            in_code: <_>::default(),
            next,
            terms: <_>::default(),
            to,
        }
    }

    /// Flushes the text buffer to the next sink.
    fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        let text = if self.in_attr && self.buffer.contains("://") {
            Cow::Borrowed(self.buffer.as_str())
        } else {
            (self.converter)(&self.buffer, &self.to)
        };
        flush_ws(&mut self.next, &text);
        self.buffer.clear();
    }

    /// Returns `true` if the given tag name is a code tag.
    #[inline]
    fn is_code_tag(name: &str) -> bool {
        matches!(name, "code" | "math" | "pre" | "script" | "style" | "svg")
    }
}

impl<S: Sink> Sink for ReplaceText<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.flush();
        self.in_code -= 1;
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.flush();
        self.in_code += 1;
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.in_code == 0 {
            self.buffer.push(value);
        } else {
            self.next.entity(value, raw);
        }
    }

    #[inline]
    fn finish(mut self) -> String {
        self.flush();
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.in_code == 0 {
            self.buffer.push('\n');
        } else {
            self.next.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if let StripMarker::General(html) = marker {
            tokenise(self, html);
        } else {
            self.flush();
            self.next.strip_marker(marker);
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if matches!(name, "alt" | "title") {
            self.flush();
        } else {
            self.in_code -= 1;
        }
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if !matches!(name, "alt" | "title") {
            self.in_code += 1;
        }
        self.next.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.flush();
        // Because this runs before DomTree, it may receive imbalanced tags.
        // TODO: *Should* this run before DomTree? This is this way because it
        // is this way in the original parser.
        self.in_code = self.in_code.saturating_sub(u8::from(Self::is_code_tag(name)));
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.flush();
        self.in_code += u8::from(Self::is_code_tag(name));
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        if self.in_code == 0 {
            self.buffer += text;
        } else {
            self.next.text(text);
        }
    }
}
