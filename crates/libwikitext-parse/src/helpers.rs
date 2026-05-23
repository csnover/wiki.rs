//! Wikitext parser helpers.

use super::{
    Argument, Configuration, InclusionMode, Span, Spanned, Token,
    visit::{Visitor, visit_link},
};
use core::fmt;
use libwikitext_common::title::Title;

/// Extracts all text from a token tree.
pub struct TextContent<'tt, W>
where
    W: fmt::Write,
{
    /// The parser configuration.
    config: &'tt Configuration,
    /// The accumulated text.
    content: W,
    /// The token tree source.
    source: &'tt str,
}

impl<'tt, W> TextContent<'tt, W>
where
    W: fmt::Write,
{
    /// Creates a new text content extractor with the given source and output.
    pub fn new(config: &'tt Configuration, source: &'tt str, content: W) -> Self {
        Self {
            config,
            content,
            source,
        }
    }

    /// Returns the text content, consuming the extractor.
    pub fn finish(self) -> W {
        self.content
    }
}

impl<'tt, W> Visitor<'tt, fmt::Error> for TextContent<'tt, W>
where
    W: fmt::Write,
{
    fn source(&self) -> &'tt str {
        self.source
    }

    fn visit_entity(&mut self, _span: Span, value: char) -> fmt::Result {
        self.content.write_char(value)
    }

    fn visit_generated(&mut self, _span: Span, text: &'tt str) -> fmt::Result {
        self.content.write_str(text)
    }

    fn visit_link(
        &mut self,
        span: Span,
        target: &'tt [Spanned<Token>],
        content: &'tt [Spanned<Argument>],
        trail: Option<&'tt str>,
    ) -> Result<(), fmt::Error> {
        // TODO: Actually evaluate the target (which requires making this helper
        // capable of evaluating wikitext, which is annoying).
        #[rustfmt::skip]
        if let [Spanned { span, node: Token::Text }] = target
            && let Ok(title) = Title::new(self.config, &self.source[span.into_range()], None)
            && title.is_local_category() {
            return Ok(());
        };

        visit_link(self, span, target, content, trail)
    }

    fn visit_new_line(&mut self, _span: Span) -> fmt::Result {
        self.content.write_char(' ')
    }

    fn visit_start_include(&mut self, _span: Span, _mode: InclusionMode) -> fmt::Result {
        todo!("inclusion control in text extractor")
    }

    fn visit_text(&mut self, text: &str) -> fmt::Result {
        self.content.write_str(text)
    }
}
