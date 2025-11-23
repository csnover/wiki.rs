//! A string trimmer for token trees.

use super::{
    Error, Result, State, WriteSurrogate,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    template,
};
use crate::wikitext::{
    AnnoAttribute, Argument, FileMap, HeadingLevel, InclusionMode, LangFlags, LangVariant, Output,
    Span, Spanned, TextStyle, Token,
};
use core::fmt;

/// A string trimmer for token trees that removes all whitespace from the start
/// and end of processed tokens.
///
/// This trimmer works by buffering whitespace tokens until a non-whitespace is
/// encountered (in which case they are flushed to the output) or until the
/// trimmer is finalised (in which case they are discarded).
pub(super) struct Trim<'a, W: WriteSurrogate + ?Sized> {
    /// The last sequence of tokens containing only whitespace.
    last_ws: Vec<Stored>,
    /// The output target.
    out: &'a mut W,
    /// The stack frame for the tokens in [`Self::last_ws`].
    sp: &'a StackFrame<'a>,
    /// Whether any tokens have been emitted to [`Self::out`] yet.
    emitted: bool,
}

impl<'a, W: WriteSurrogate + ?Sized> Trim<'a, W> {
    /// Creates a new [`Trim`].
    #[inline]
    pub fn new(out: &'a mut W, sp: &'a StackFrame<'a>) -> Self {
        Self {
            last_ws: <_>::default(),
            out,
            sp,
            emitted: <_>::default(),
        }
    }

    /// Flushes any pending whitespace tokens to output.
    #[inline]
    fn flush(&mut self, state: &mut State<'_>) -> Result {
        self.emitted = true;
        for last in self.last_ws.drain(..) {
            match last {
                Stored::Token(last) => {
                    self.out.adopt_token(state, self.sp, &last)?;
                }
                Stored::Memoised(source, last) => {
                    self.out.adopt_token(
                        state,
                        &self.sp.clone_with_source(FileMap::new(&source)),
                        &last,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Stores a whitespace token to be emitted or discarded later.
    #[inline]
    fn store(&mut self, sp: &StackFrame<'_>, token: Spanned<Token>) {
        if core::ptr::eq(self.sp, sp) {
            self.last_ws.push(Stored::Token(token));
        } else {
            // TODO: This is a lot of work just to avoid making stack frames
            // `Rc`, especially considering making them non-Rc requires unsafe
            // code in the Lua interpreter…
            log::warn!("Storing whitespace from a different frame");
            let content = &sp.source[token.span.into_range()];
            self.last_ws.push(Stored::Memoised(
                content.to_string(),
                Spanned::new(token.node, 0, content.len()),
            ));
        }
    }

    /// Processes a text token.
    fn trim_text<const IS_GENERATED: bool>(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        text: &str,
    ) -> Result {
        let start = if self.emitted {
            // This is not the first token, so leading whitespace is
            // valid in the output
            0
        } else if let Some(start) = text.find(|c: char| !c.is_ascii_whitespace()) {
            // Mix of whitespace and non-whitespace text can be trimmed
            // and emitted now
            start
        } else {
            // We are still trimming the start, and the whole token was
            // whitespace, and maybe the next token is also whitespace,
            // so just pretend like it never existed
            return Ok(());
        };

        // Position of trailing whitespace.
        let end = text
            .rfind(|c: char| !c.is_ascii_whitespace())
            .map_or(0, |end| {
                end + text[end..].chars().next().unwrap().len_utf8()
            });

        // The character part
        if end != 0 {
            let span = Span::new(span.start + start, span.start + end);
            let text = &text[start..end];
            self.flush(state)?;
            if IS_GENERATED {
                self.out.adopt_generated(state, sp, Some(span), text)?;
            } else {
                self.out.adopt_text(state, sp, span, text)?;
            }
        }

        // The whitespace part
        if end != text.len() {
            let span = Span::new(span.start + end, span.end);
            debug_assert!(text[end..].trim_ascii_end().is_empty());
            self.store(
                sp,
                Spanned {
                    node: if IS_GENERATED {
                        Token::Generated(text[end..].to_string())
                    } else {
                        Token::Text
                    },
                    span,
                },
            );
        }

        Ok(())
    }
}

impl<W: WriteSurrogate + ?Sized> fmt::Write for Trim<'_, W> {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.out.write_str(s)
    }
}

impl<W: WriteSurrogate + ?Sized> Surrogate<Error> for Trim<'_, W> {
    #[inline]
    fn adopt_autolink(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_autolink(state, sp, span, target, content)
    }

    #[inline]
    fn adopt_behavior_switch(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_behavior_switch(state, sp, span, name)
    }

    #[inline]
    fn adopt_comment(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _content: &str,
        _unclosed: bool,
    ) -> Result {
        // MW appears to ignore comments in these positions, presumably
        // to more easily remove any visual whitespace by `trim`
        Ok(())
    }

    #[inline]
    fn adopt_end_annotation(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
    ) -> Result {
        self.out.adopt_end_annotation(state, sp, span, name)
    }

    #[inline]
    fn adopt_end_include(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.out.adopt_end_include(state, sp, span, mode)
    }

    #[inline]
    fn adopt_end_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_end_tag(state, sp, span, name)
    }

    #[inline]
    fn adopt_entity(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        value: char,
    ) -> Result {
        // Entity-encoded whitespace is written that way explicitly to avoid
        // trimming
        self.flush(state)?;
        self.out.adopt_entity(state, sp, span, value)
    }

    #[inline]
    fn adopt_extension(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        content: Option<&str>,
    ) -> Result {
        self.flush(state)?;
        // TODO: Technically to work correctly this should be inspecting the
        // *output* of `self.out` but the only way that would be possible would
        // be to have these functions all return strings and that is yet another
        // rearchitecting that I am not keen to do now.
        self.out
            .adopt_extension(state, sp, span, name, attributes, content)
    }

    #[inline]
    fn adopt_external_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_external_link(state, sp, span, target, content)
    }

    #[inline]
    fn adopt_generated(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Option<Span>,
        text: &str,
    ) -> Result {
        self.trim_text::<true>(state, sp, span.unwrap_or(Span::new(0, 0)), text)
    }

    #[inline]
    fn adopt_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_heading(state, sp, span, level, content)
    }

    #[inline]
    fn adopt_horizontal_rule(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        line_content: bool,
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_horizontal_rule(state, sp, span, line_content)
    }

    #[inline]
    fn adopt_lang_variant(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        flags: Option<&LangFlags>,
        variants: &[Spanned<LangVariant>],
        raw: bool,
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_lang_variant(state, sp, span, flags, variants, raw)
    }

    #[inline]
    fn adopt_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_link(state, sp, span, target, content, trail)
    }

    #[inline]
    fn adopt_list_item(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_list_item(state, sp, span, bullets, content)
    }

    #[inline]
    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        self.store(
            sp,
            Spanned {
                node: Token::NewLine,
                span,
            },
        );
        Ok(())
    }

    #[inline]
    fn adopt_output(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        output: &Output,
    ) -> Result {
        // TODO: What happens if the output has onlyinclude flag?
        assert!(!output.has_onlyinclude);
        surrogate::adopt_output(self, state, sp, output)
    }

    #[inline]
    fn adopt_parameter(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &[Spanned<Token>],
        default: Option<&[Spanned<Token>]>,
    ) -> Result {
        // Expanded non-numeric key values are trimmed by using `Trim`, so this
        // would create an infinitely expanding monomorphic type
        template::render_parameter(
            self as &mut dyn WriteSurrogate,
            state,
            sp,
            span,
            name,
            default,
        )
    }

    #[inline]
    fn adopt_redirect(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_redirect(state, sp, span, target, content, trail)
    }

    #[inline]
    fn adopt_start_annotation(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<AnnoAttribute>],
    ) -> Result {
        self.out
            .adopt_start_annotation(state, sp, span, name, attributes)
    }

    #[inline]
    fn adopt_start_include(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.out.adopt_start_include(state, sp, span, mode)
    }

    #[inline]
    fn adopt_start_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        self_closing: bool,
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_start_tag(state, sp, span, name, attributes, self_closing)
    }

    #[inline]
    fn adopt_strip_marker(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        marker: usize,
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_strip_marker(state, sp, span, marker)
    }

    #[inline]
    fn adopt_text(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        text: &str,
    ) -> Result {
        self.trim_text::<false>(state, sp, span, text)
    }

    #[inline]
    fn adopt_text_style(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        style: TextStyle,
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_text_style(state, sp, span, style)
    }

    #[inline]
    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_table_caption(state, sp, span, attributes, content)
    }

    #[inline]
    fn adopt_table_data(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_table_data(state, sp, span, attributes, content)
    }

    #[inline]
    fn adopt_table_end(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_table_end(state, sp, span)
    }

    #[inline]
    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush(state)?;
        self.out
            .adopt_table_heading(state, sp, span, attributes, content)
    }

    #[inline]
    fn adopt_table_row(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_table_row(state, sp, span, attributes)
    }

    #[inline]
    fn adopt_table_start(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.flush(state)?;
        self.out.adopt_table_start(state, sp, span, attributes)
    }

    #[inline]
    fn adopt_template(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        arguments: &[Spanned<Argument>],
    ) -> Result {
        template::render_template(self, state, sp, span, target, arguments)
    }
}

/// Stored trailing whitespace.
enum Stored {
    /// A token containing only whitespace.
    Token(Spanned<Token>),
    /// A token containing only whitespace whose contents had to be memoised
    /// because it came from a foreign stack frame.
    Memoised(String, Spanned<Token>),
}

/// A string trimmer for token trees that removes leading colons from the output
/// text.
pub(super) struct TrimLink<'a, W: WriteSurrogate + ?Sized> {
    /// The output target.
    out: &'a mut W,
    /// Whether any tokens have been emitted to [`Self::out`] yet.
    emitted: bool,
}

impl<'a, W: WriteSurrogate + ?Sized> TrimLink<'a, W> {
    /// Creates a new [`TrimLink`].
    #[inline]
    pub fn new(out: &'a mut W) -> Self {
        Self {
            out,
            emitted: <_>::default(),
        }
    }

    /// Processes a text token.
    #[inline]
    fn trim_text<const IS_GENERATED: bool>(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        text: &str,
    ) -> Result {
        let start = if self.emitted {
            // This is not the first token, so leading whitespace is
            // valid in the output
            0
        } else if let Some(start) = text.find(|c: char| c != ':') {
            start
        } else {
            // We are still trimming the start, and the whole token was
            // colons, and maybe the next token is too, so just pretend like it
            // never existed
            return Ok(());
        };
        let span = Span {
            start,
            end: span.end,
        };
        let text = &text[start..];
        if IS_GENERATED {
            self.out.adopt_generated(state, sp, Some(span), text)
        } else {
            self.out.adopt_text(state, sp, span, text)
        }
    }
}

impl<W: WriteSurrogate + ?Sized> fmt::Write for TrimLink<'_, W> {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.out.write_str(s)
    }
}

impl<W: WriteSurrogate + ?Sized> Surrogate<Error> for TrimLink<'_, W> {
    #[inline]
    fn adopt_autolink(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.emitted = true;
        self.out.adopt_autolink(state, sp, span, target, content)
    }

    #[inline]
    fn adopt_behavior_switch(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
    ) -> Result {
        self.emitted = true;
        self.out.adopt_behavior_switch(state, sp, span, name)
    }

    #[inline]
    fn adopt_comment(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _content: &str,
        _unclosed: bool,
    ) -> Result {
        // MW appears to ignore comments in these positions
        Ok(())
    }

    #[inline]
    fn adopt_end_annotation(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
    ) -> Result {
        self.out.adopt_end_annotation(state, sp, span, name)
    }

    #[inline]
    fn adopt_end_include(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.out.adopt_end_include(state, sp, span, mode)
    }

    #[inline]
    fn adopt_end_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
    ) -> Result {
        self.emitted = true;
        self.out.adopt_end_tag(state, sp, span, name)
    }

    #[inline]
    fn adopt_entity(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        value: char,
    ) -> Result {
        // Entity-encoded stuff is written that way explicitly to avoid trimming
        self.emitted = true;
        self.out.adopt_entity(state, sp, span, value)
    }

    #[inline]
    fn adopt_extension(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        content: Option<&str>,
    ) -> Result {
        self.emitted = true;
        // TODO: Technically to work correctly this should be inspecting the
        // *output* of `self.out` but the only way that would be possible would
        // be to have these functions all return strings and that is yet another
        // rearchitecting that I am not keen to do now.
        self.out
            .adopt_extension(state, sp, span, name, attributes, content)
    }

    #[inline]
    fn adopt_external_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_external_link(state, sp, span, target, content)
    }

    #[inline]
    fn adopt_generated(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Option<Span>,
        text: &str,
    ) -> Result {
        self.trim_text::<true>(state, sp, span.unwrap_or(Span::new(0, 0)), text)
    }

    #[inline]
    fn adopt_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result {
        self.emitted = true;
        self.out.adopt_heading(state, sp, span, level, content)
    }

    #[inline]
    fn adopt_horizontal_rule(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        line_content: bool,
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_horizontal_rule(state, sp, span, line_content)
    }

    #[inline]
    fn adopt_lang_variant(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        flags: Option<&LangFlags>,
        variants: &[Spanned<LangVariant>],
        raw: bool,
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_lang_variant(state, sp, span, flags, variants, raw)
    }

    #[inline]
    fn adopt_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        self.emitted = true;
        self.out.adopt_link(state, sp, span, target, content, trail)
    }

    #[inline]
    fn adopt_list_item(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result {
        self.emitted = true;
        self.out.adopt_list_item(state, sp, span, bullets, content)
    }

    #[inline]
    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        self.emitted = true;
        Ok(())
    }

    #[inline]
    fn adopt_output(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        output: &Output,
    ) -> Result {
        // TODO: What happens if the output has onlyinclude flag?
        assert!(!output.has_onlyinclude);
        surrogate::adopt_output(self, state, sp, output)
    }

    #[inline]
    fn adopt_parameter(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &[Spanned<Token>],
        default: Option<&[Spanned<Token>]>,
    ) -> Result {
        template::render_parameter(self, state, sp, span, name, default)
    }

    #[inline]
    fn adopt_redirect(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_redirect(state, sp, span, target, content, trail)
    }

    #[inline]
    fn adopt_start_annotation(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<AnnoAttribute>],
    ) -> Result {
        self.out
            .adopt_start_annotation(state, sp, span, name, attributes)
    }

    #[inline]
    fn adopt_start_include(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.out.adopt_start_include(state, sp, span, mode)
    }

    #[inline]
    fn adopt_start_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        self_closing: bool,
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_start_tag(state, sp, span, name, attributes, self_closing)
    }

    #[inline]
    fn adopt_strip_marker(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        marker: usize,
    ) -> Result {
        self.emitted = true;
        self.out.adopt_strip_marker(state, sp, span, marker)
    }

    #[inline]
    fn adopt_text(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        text: &str,
    ) -> Result {
        self.trim_text::<false>(state, sp, span, text)
    }

    #[inline]
    fn adopt_text_style(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        style: TextStyle,
    ) -> Result {
        self.emitted = true;
        self.out.adopt_text_style(state, sp, span, style)
    }

    #[inline]
    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_table_caption(state, sp, span, attributes, content)
    }

    #[inline]
    fn adopt_table_data(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_table_data(state, sp, span, attributes, content)
    }

    #[inline]
    fn adopt_table_end(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        self.emitted = true;
        self.out.adopt_table_end(state, sp, span)
    }

    #[inline]
    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.emitted = true;
        self.out
            .adopt_table_heading(state, sp, span, attributes, content)
    }

    #[inline]
    fn adopt_table_row(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.emitted = true;
        self.out.adopt_table_row(state, sp, span, attributes)
    }

    #[inline]
    fn adopt_table_start(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.emitted = true;
        self.out.adopt_table_start(state, sp, span, attributes)
    }

    #[inline]
    fn adopt_template(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        arguments: &[Spanned<Argument>],
    ) -> Result {
        template::render_template(self, state, sp, span, target, arguments)
    }
}
