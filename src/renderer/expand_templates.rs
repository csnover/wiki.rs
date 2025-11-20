//! A helper for expanding templates in a Wikitext fragment into an intermediate
//! container.

use super::{
    Error, Kv, Result, State, extension_tags,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    tags, template,
};
use crate::{
    renderer::document::Document,
    wikitext::{
        AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant,
        MARKER_PREFIX, MARKER_SUFFIX, Output, Span, Spanned, TextStyle, Token,
    },
};
use core::{
    fmt::{self, Write as _},
    ops::Range,
};

/// Template expansion mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExpandMode {
    /// Expand templates in non-include mode. This is used when rendering the
    /// bodies of extension tags present in the root document.
    #[default]
    Normal,
    /// Expand templates in include mode. This mode is used by templates.
    Include,
    /// Expand templates, but defer processing extension tags by converting them
    /// to strip markers. This mode is used any time an extension tag may be in
    /// a position where it might not make it to the root document output.
    Strip,
}

/// Performs partial evaluation of a Wikitext string, extracting extension tags
/// into strip markers and expanding templates while converting all other tokens
/// back into their original Wikitext.
pub struct ExpandTemplates {
    /// The inclusion control tag stack.
    inclusion_mode: Vec<InclusionMode>,
    /// The processing mode.
    mode: ExpandMode,
    /// The result of the evaluation.
    out: String,
}

impl ExpandTemplates {
    /// Creates a new [`ExpandTemplates`] with the given writer and inclusion
    /// mode.
    pub fn new(mode: ExpandMode) -> Self {
        Self {
            inclusion_mode: vec![],
            mode,
            out: <_>::default(),
        }
    }

    /// Consumes this object, returning the result.
    pub fn finish(self) -> String {
        self.out
    }

    /// Serialises a token which is structured like
    /// `{prefix}{attributes}{delimiter}{content}{suffix}`.
    pub(crate) fn adopt_attributes_content(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, attributes, content);
        self.out.write_str(&sp.source[prefix])?;
        tags::render_single_attribute(self, state, sp, attributes)?;
        self.write_delimiter(sp, attributes, content)?;
        self.adopt_tokens(state, sp, content)?;
        self.out.write_str(&sp.source[suffix])?;
        Ok(())
    }

    /// Serialises the delimiter between two groups of spanned elements like
    /// `{before}{delimiter}{after}...`.
    pub(crate) fn write_delimiter<T, U>(
        &mut self,
        sp: &StackFrame<'_>,
        before: &[Spanned<T>],
        after: &[Spanned<U>],
    ) -> Result {
        if let (Some(last_before), Some(first_after)) = (before.last(), after.first()) {
            self.out
                .write_str(&sp.source[last_before.span.end..first_after.span.start])?;
        }
        Ok(())
    }
}

impl fmt::Write for ExpandTemplates {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.out.write_str(s)
    }
}

impl Surrogate<Error> for ExpandTemplates {
    fn adopt_autolink(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _target: &[Spanned<Token>],
        _content: &[Spanned<Token>],
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_attribute(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        name: Option<either::Either<&str, &[Spanned<Token>]>>,
        value: either::Either<&str, &[Spanned<Token>]>,
    ) -> Result {
        tags::render_attribute(self, state, sp, name, value)
    }

    fn adopt_behavior_switch(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _name: &str,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_comment(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _content: &str,
        _unclosed: bool,
    ) -> Result {
        // Comments are traditionally excluded from evaluation by some flag,
        // but we will just do it all the time
        Ok(())
    }

    fn adopt_end_annotation(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
    ) -> Result {
        todo!("annotation detected")
    }

    fn adopt_end_include(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.inclusion_mode
            .pop_if(|current| *current == mode)
            .expect("mismatched inclusion control");
        Ok(())
    }

    fn adopt_end_tag(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _name: &str,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_entity(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _value: char,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_extension(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        content: Option<&str>,
    ) -> Result {
        // TODO: Collecting into a `Vec<Kv>` first wastes time.
        let attributes = attributes.iter().map(Kv::Argument).collect::<Vec<_>>();
        let mut out = Document::new(true);
        extension_tags::render_extension_tag(
            &mut out,
            state,
            sp,
            Some(span),
            name,
            &attributes,
            content,
        )?;
        let content = out.finish_fragment();
        write!(
            self.out,
            "{MARKER_PREFIX}{}{MARKER_SUFFIX}",
            state.strip_markers.len()
        )?;
        state.strip_markers.push(content);
        Ok(())
    }

    fn adopt_external_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, target, content);
        self.out.write_str(&sp.source[prefix])?;
        self.adopt_tokens(state, sp, target)?;
        self.write_delimiter(sp, target, content)?;
        self.adopt_tokens(state, sp, content)?;
        self.out.write_str(&sp.source[suffix])?;
        Ok(())
    }

    fn adopt_generated(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Option<Span>,
        text: &str,
    ) -> Result {
        self.out.write_str(text)?;
        Ok(())
    }

    fn adopt_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, content, content);
        self.write_str(&sp.source[prefix])?;
        self.adopt_tokens(state, sp, content)?;
        self.write_str(&sp.source[suffix])?;
        Ok(())
    }

    fn adopt_horizontal_rule(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _line_content: bool,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_lang_variant(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _flags: Option<&LangFlags>,
        _variants: &[Spanned<LangVariant>],
        _raw: bool,
    ) -> Result {
        todo!("lang variant detected")
    }

    fn adopt_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        _trail: Option<Spanned<&str>>,
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, target, content);
        self.out.write_str(&sp.source[prefix])?;
        self.adopt_tokens(state, sp, target)?;
        self.write_delimiter(sp, target, content)?;
        tags::render_single_attribute(self, state, sp, content)?;
        self.out.write_str(&sp.source[suffix])?;
        Ok(())
    }

    fn adopt_list_item(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, content, content);
        self.write_str(&sp.source[prefix])?;
        self.adopt_tokens(state, sp, content)?;
        self.write_str(&sp.source[suffix])?;
        Ok(())
    }

    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_output(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        output: &Output,
    ) -> Result<(), Error> {
        if output.has_onlyinclude {
            self.inclusion_mode.push(InclusionMode::NoInclude);
            surrogate::adopt_output(self, state, sp, output)?;
            self.inclusion_mode.pop();
        } else {
            surrogate::adopt_output(self, state, sp, output)?;
        }
        Ok(())
    }

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

    fn adopt_redirect(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _target: &[Spanned<Token>],
        _content: &[Spanned<Argument>],
        _trail: Option<Spanned<&str>>,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_start_annotation(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
        _attributes: &[Spanned<AnnoAttribute>],
    ) -> Result {
        todo!("annotation detected")
    }

    fn adopt_start_include(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.inclusion_mode.push(mode);
        Ok(())
    }

    fn adopt_start_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        self_closing: bool,
    ) -> Result {
        tags::render_start_tag(self, state, sp, name, attributes, self_closing)
    }

    fn adopt_strip_marker(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        marker: usize,
    ) -> Result {
        // Once an extension tag has been stripped once, there is not much
        // reason to reintroduce its content prior to the final output. At best
        // it just wastes time reserialising content; at worst it actually gets
        // deserialised in a way that is wrong since the output of
        // `ExpandTemplates` gets shoved back into a parser some time later and
        // content is not tagged to avoid e.g. content which had been in
        // `<nowiki>` getting parsed as Wikitext the second time.
        write!(self.out, "{MARKER_PREFIX}{marker}{MARKER_SUFFIX}")?;
        Ok(())
    }

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

    fn adopt_text(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _text: &str,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_text_style(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        _style: TextStyle,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, content)
    }

    fn adopt_table_data(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, content)
    }

    fn adopt_table_end(
        &mut self,
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, content)
    }

    fn adopt_table_row(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, &[])
    }

    fn adopt_table_start(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, &[])
    }

    fn adopt_token(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        token: &Spanned<Token>,
    ) -> Result {
        if should_skip(
            token,
            matches!(self.mode, ExpandMode::Include | ExpandMode::Strip),
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
pub(crate) fn should_skip(
    token: &Spanned<Token>,
    in_include: bool,
    current: Option<&InclusionMode>,
) -> bool {
    if let Spanned {
        node: Token::EndInclude(mode),
        ..
    } = token
        && Some(mode) == current
    {
        false
    // TODO: Think harder about what the actual conditions should be
    } else if let Spanned {
        node: Token::StartInclude(..),
        ..
    } = token
        && matches!(current, Some(InclusionMode::OnlyInclude))
    {
        false
    } else if in_include {
        !matches!(
            current,
            None | Some(InclusionMode::IncludeOnly | InclusionMode::OnlyInclude)
        )
    } else {
        !matches!(current, None | Some(InclusionMode::NoInclude))
    }
}

/// Calculates the ranges for the prefix and suffix in a token which is
/// structured like `{prefix}{content}{suffix}`.
pub(crate) fn calc_prefix_suffix<T, U>(
    span: Span,
    begin: &[Spanned<T>],
    end: &[Spanned<U>],
) -> (Range<usize>, Range<usize>) {
    let first = begin
        .first()
        .map(|first| first.span.start)
        .or_else(|| end.first().map(|first| first.span.start))
        .unwrap_or(span.end);
    let last = end
        .last()
        .map(|last| last.span.end)
        .or_else(|| begin.last().map(|last| last.span.end))
        .unwrap_or(span.end);
    let prefix = span.start..first;
    let suffix = last..span.end;
    (prefix, suffix)
}
