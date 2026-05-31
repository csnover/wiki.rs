//! A helper for expanding templates in a Wikitext fragment into an intermediate
//! container.

use super::{
    Error, Result, State, extension_tags,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    tags, template,
};
use core::{fmt::Write as _, ops::Range};
use either::Either;
use libmisc::to_ascii_lower;
use libwikitext_parse::{
    AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant, MagicLink,
    Output, Span, Spanned, TextStyle, Token,
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

    /// Serialises a token which is structured like
    /// `{prefix}{attributes}{delimiter}{content}{suffix}`.
    #[inline]
    fn adopt_attributes_content(
        &mut self,
        state: &mut State<'_, '_, '_>,
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

    /// Serialises a token which is structured like
    /// `{prefix}{target}{delimiter}{arguments}{suffix}`.
    fn adopt_target_arguments(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        arguments: &[Spanned<Argument>],
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, target, arguments);
        self.out.write_str(&sp.source[prefix])?;
        self.adopt_tokens(state, sp, target)?;
        self.write_delimiter(sp, target, arguments)?;
        tags::render_single_attribute(self, state, sp, arguments)?;
        self.out.write_str(&sp.source[suffix])?;
        Ok(())
    }

    /// Serialises the delimiter between two groups of spanned elements like
    /// `{before}{delimiter}{after}...`.
    #[inline]
    fn write_delimiter<T, U>(
        &mut self,
        sp: &StackFrame<'_>,
        before: &[Spanned<T>],
        after: &[Spanned<U>],
    ) -> Result {
        if let (Some(last_before), Some(first_after)) = (before.last(), after.first()) {
            self.out.write_str(
                &sp.source[last_before.span.end as usize..first_after.span.start as usize],
            )?;
        }
        Ok(())
    }
}

impl Surrogate<Error> for ExpandTemplates<'_> {
    #[inline]
    fn adopt_autolink(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _target: &[Spanned<Token>],
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

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
    fn adopt_end_annotation(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
    ) -> Result {
        todo!("annotation detected")
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

    #[inline]
    fn adopt_end_tag(
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

    fn adopt_external_link(
        &mut self,
        state: &mut State<'_, '_, '_>,
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
        _level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, content, content);
        self.out.write_str(&sp.source[prefix])?;
        self.adopt_tokens(state, sp, content)?;
        self.out.write_str(&sp.source[suffix])?;
        Ok(())
    }

    #[inline]
    fn adopt_horizontal_rule(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_lang_variant(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _flags: &LangFlags,
        _variants: &[LangVariant],
    ) -> Result {
        // TODO: It is extremely unclear what these tokens are supposed to do
        // given that they do not seem to do anything at all on MW and just emit
        // plain text like this.
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_link(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _prefix: Option<Spanned<&str>>,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        _trail: Option<Spanned<&str>>,
    ) -> Result {
        self.adopt_target_arguments(state, sp, span, target, content)
    }

    #[inline]
    fn adopt_list_item(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result {
        let (prefix, suffix) = calc_prefix_suffix(span, content, content);
        self.out.write_str(&sp.source[prefix])?;
        self.adopt_tokens(state, sp, content)?;
        // Any non-whitespace in the list item suffix should be ignored since it
        // could be an `include_limits` trailer or other detritus that the
        // parser picked up and discarded in `inlineline`. Retaining whitespace
        // is still required to make sure that list items actually terminate.
        let suffix = &sp.source[suffix];
        let suffix = suffix
            .rsplit_once(|c: char| !c.is_ascii_whitespace())
            .map_or(suffix, |(_, suffix)| suffix);
        self.out.write_str(suffix)?;
        Ok(())
    }

    #[inline]
    fn adopt_magic_link(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _magic: &MagicLink,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        // `Token::NewLine` is usually just a newline character, but it can also
        // be an empty line consisting of only whitespace *and comments*, the
        // latter of which must not be included in most output. (It would be OK
        // in `ExpandMode::Normal` but I think we are just going all-in on not
        // ever outputting comments because there’s not much reason to do it,
        // but it creates lots of headaches.)
        self.out.write_char('\n')?;
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
    fn adopt_redirect(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _prefix: Option<Spanned<&str>>,
        _target: &[Spanned<Token>],
        _content: &[Spanned<Argument>],
        _trail: Option<Spanned<&str>>,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_start_annotation(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
        _attributes: &[Spanned<AnnoAttribute>],
    ) -> Result {
        todo!("annotation detected")
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

    fn adopt_start_tag(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        self_closing: bool,
    ) -> Result {
        // Attributes may contain templates, so start tags must be reconstructed
        // instead of copied directly into the output
        write!(self.out, "<{name}")?;
        for attr in attributes {
            self.out.write_char(' ')?;
            if let Some(name) = attr.name() {
                self.adopt_tokens(state, sp, name)?;
                let value = attr.value();
                if !value.is_empty() {
                    self.out.write_str("=\"")?;
                    self.adopt_tokens(state, sp, value)?;
                    self.out.write_str("\"")?;
                }
            } else {
                self.adopt_tokens(state, sp, attr.value())?;
            }
        }
        if self_closing {
            self.out.write_char('/')?;
        }
        self.out.write_char('>')?;
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
        // Once an extension tag has been stripped once, there is not much
        // reason to reintroduce its content prior to the final output. At best
        // it just wastes time reserialising content; at worst it actually gets
        // deserialised in a way that is wrong since the output of
        // `ExpandTemplates` gets shoved back into a parser some time later and
        // content is not tagged to avoid e.g. content which had been in
        // `<nowiki>` getting parsed as Wikitext the second time.
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, &[])
    }

    #[inline]
    fn adopt_table_data(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, &[])
    }

    #[inline]
    fn adopt_table_end(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        self.out.write_str(&sp.source[span.into_range()])?;
        Ok(())
    }

    #[inline]
    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, &[])
    }

    #[inline]
    fn adopt_table_row(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, &[])
    }

    #[inline]
    fn adopt_table_start(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_attributes_content(state, sp, span, attributes, &[])
    }

    fn adopt_template(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        arguments: &[Spanned<Argument>],
    ) -> Result {
        let line_start = self.out.is_empty() || self.out.ends_with('\n');
        let rendered = template::render_template(
            &mut self.out,
            state,
            sp,
            span,
            target,
            arguments,
            line_start,
        )?;

        if rendered {
            Ok(())
        } else {
            self.adopt_target_arguments(state, sp, span, target, arguments)
        }
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
    if matches!(
        token,
        Spanned {
            node: Token::EndInclude(..) | Token::StartInclude(..),
            ..
        }
    ) {
        return true;
    }

    let Some(current) = current else {
        return true;
    };

    in_include == (*current != InclusionMode::NoInclude)
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
    let prefix = span.start as usize..first as usize;
    let suffix = last as usize..span.end as usize;
    (prefix, suffix)
}
