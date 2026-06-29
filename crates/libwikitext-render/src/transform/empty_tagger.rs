//! [`Sink`]s for marking certain elements by adding attributes.

use super::{Sink, chainable, flush_ws, markable_string::Markable};
use crate::{StripMarker, tags::PHRASING_TAGS};
use libwikitext_parse::VOID_TAGS;

/// Marks elements containing only whitespace.
#[derive(Debug)]
pub(crate) struct EmptyTagger<S: Sink> {
    /// The tag name of a potentially empty element.
    last: Option<&'static str>,
    /// The output.
    next: S,
    /// The whitespace buffer for a potentially empty element.
    ws_buffer: String,
}

chainable!(EmptyTagger);

impl<S: Sink> EmptyTagger<S> {
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
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
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

/// Adds extra `data-wiki-rs` attributes to the root elements of anonymous
/// templates so they can be identified and styled.
#[derive(Debug)]
pub(crate) struct TemplateTagger<S: Sink> {
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
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
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
