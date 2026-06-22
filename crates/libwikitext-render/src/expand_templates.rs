//! A helper for expanding templates in a Wikitext fragment into an intermediate
//! container.

use super::{
    Error, Result, State, extension_tags,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    template,
};
use core::fmt::Write as _;
use either::Either;
use libmisc::to_ascii_lower;
use libwikitext_parse::{
    Argument, HeadingLevel, InclusionMode, Output, Span, Spanned, TextStyle, Token,
};

/// Template expansion mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ExpandMode {
    /// Expand templates in non-include mode. This is used when rendering the
    /// bodies of extension tags present in the root document.
    #[default]
    Normal,
    /// Expand templates in include mode. This mode is used by templates.
    Include,
}

/// Performs partial evaluation of a Wikitext string, extracting extension tags
/// into strip markers and expanding templates while converting all other tokens
/// back into their original Wikitext.
pub(crate) struct ExpandTemplates<'s> {
    /// The inclusion control tag stack.
    inclusion_mode: Vec<InclusionMode>,
    /// The processing mode.
    mode: ExpandMode,
    /// The result of the evaluation.
    out: &'s mut String,
}

impl<'s> ExpandTemplates<'s> {
    /// Creates a new [`ExpandTemplates`] with the given writer and inclusion
    /// mode.
    #[inline]
    pub fn new(out: &'s mut String, mode: ExpandMode) -> Self {
        Self {
            inclusion_mode: vec![],
            mode,
            out,
        }
    }

    /// Gets a reference to the output.
    #[inline]
    pub fn out(&mut self) -> &mut String {
        self.out
    }
}

impl Surrogate<Error> for ExpandTemplates<'_> {
    #[inline]
    fn adopt_behavior_switch(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _name: &str,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_comment(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _content: &str,
        _unclosed: bool,
    ) -> Result {
        // Comments are traditionally excluded from evaluation by some flag,
        // but we will just do it all the time
        Ok(())
    }

    #[inline]
    fn adopt_end_include(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.inclusion_mode
            .pop_if(|current| *current == mode)
            .expect("include stack corruption");
        Ok(())
    }

    fn adopt_entity(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _value: char,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_extension(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        content: Option<&str>,
    ) -> Result {
        let name = to_ascii_lower(name);
        match extension_tags::render_extension_tag(
            state,
            sp,
            Some(span),
            &name,
            &extension_tags::InArgs::Wikitext(attributes),
            content,
            false,
        )? {
            Some(Either::Left(marker)) => state.strip_markers.push(&mut self.out, &name, marker),
            Some(Either::Right(raw)) => write!(self.out, "{raw}")?,
            None => {}
        }

        Ok(())
    }

    #[inline]
    fn adopt_generated(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Option<Span>,
        text: &str,
    ) -> Result {
        self.out.write_str(text)?;
        Ok(())
    }

    fn adopt_heading(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result {
        let heading = Span::new(span.start, span.start + u32::from(u8::from(level)));
        self.out.write_str(&sp.source[heading.into_range()])?;
        self.adopt_tokens(state, sp, content)?;
        self.out.write_str(&sp.source[heading.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        self.out.push('\n');
        Ok(())
    }

    fn adopt_output(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        output: &Output,
    ) -> Result<(), Error> {
        let mut prefetcher = template::DbPrefetch::default();
        prefetcher.adopt_output(state, sp, output)?;
        prefetcher.finish(state);
        if output.has_onlyinclude {
            self.inclusion_mode.push(InclusionMode::NoInclude);
            surrogate::adopt_output(self, state, sp, output)?;
            self.inclusion_mode
                .pop_if(|mode| *mode == InclusionMode::NoInclude)
                .expect("include stack corruption");
        } else {
            surrogate::adopt_output(self, state, sp, output)?;
        }
        Ok(())
    }

    #[inline]
    fn adopt_parameter(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &[Spanned<Token>],
        default: Option<&[Spanned<Token>]>,
    ) -> Result {
        template::render_parameter(self, state, sp, span, name, default)
    }

    #[inline]
    fn adopt_start_include(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.inclusion_mode.push(mode);
        Ok(())
    }

    #[inline]
    fn adopt_strip_marker(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _marker: &str,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_template(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        arguments: &[Spanned<Argument>],
    ) -> Result {
        let line_start = self.out.is_empty() || self.out.ends_with('\n');
        template::render_template(self, state, sp, span, target, arguments, line_start)
    }

    #[inline]
    fn adopt_text(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _text: &str,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_text_style(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _style: TextStyle,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_token(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        token: &Spanned<Token>,
    ) -> Result {
        if !should_adopt(
            token,
            matches!(self.mode, ExpandMode::Include),
            self.inclusion_mode.last(),
        ) {
            return Ok(());
        }

        surrogate::adopt_token(self, state, sp, token).map_err(|err| Error::Node {
            frame: sp.name.to_string(),
            start: sp.source.find_line_col(token.span.start),
            err: Box::new(err),
        })
    }
}

/// Determines whether a node should be skipped according to the inclusion
/// control rules.
#[inline]
pub(crate) fn should_adopt(
    token: &Spanned<Token>,
    in_include: bool,
    current: Option<&InclusionMode>,
) -> bool {
    if matches!(token.node, Token::EndInclude(..) | Token::StartInclude(..)) {
        return true;
    }

    current.is_none_or(|current| in_include == (*current != InclusionMode::NoInclude))
}
