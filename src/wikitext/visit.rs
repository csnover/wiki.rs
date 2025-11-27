//! Helper trait for implementing token tree visitors.

use super::{
    AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant, Output, Span,
    Spanned, TextStyle, Token,
};

/// A trait for visiting the tokens of a token tree.
pub trait Visitor<'tt, E> {
    /// Returns the source code of the token tree.
    fn source(&self) -> &'tt str;

    /// Visits a [`Token::Autolink`].
    #[inline]
    fn visit_autolink(
        &mut self,
        span: Span,
        target: &'tt [Spanned<Token>],
        content: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        visit_autolink(self, span, target, content)
    }

    /// Visits a [`Token::BehaviorSwitch`].
    #[inline]
    fn visit_behavior_switch(&mut self, _span: Span, _name: &'tt str) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Comment`].
    #[inline]
    fn visit_comment(&mut self, _span: Span, _content: &'tt str, _unclosed: bool) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::EndAnnotation`].
    #[inline]
    fn visit_end_annotation(&mut self, _span: Span, _name: &'tt str) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::EndInclude`].
    #[inline]
    fn visit_end_include(&mut self, _span: Span, _mode: InclusionMode) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::EndTag`].
    #[inline]
    fn visit_end_tag(&mut self, _span: Span, _name: &'tt str) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Entity`].
    #[inline]
    fn visit_entity(&mut self, _span: Span, _value: char) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Extension`].
    #[inline]
    fn visit_extension(
        &mut self,
        _span: Span,
        _name: &'tt str,
        _attributes: &'tt [Spanned<Argument>],
        _content: Option<&'tt str>,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::ExternalLink`].
    #[inline]
    fn visit_external_link(
        &mut self,
        span: Span,
        target: &'tt [Spanned<Token>],
        content: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        visit_external_link(self, span, target, content)
    }

    /// Visits a [`Token::Generated`].
    #[inline]
    fn visit_generated(&mut self, _span: Span, _text: &'tt str) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Heading`].
    #[inline]
    fn visit_heading(
        &mut self,
        span: Span,
        level: HeadingLevel,
        content: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        visit_heading(self, span, level, content)
    }

    /// Visits a [`Token::HorizontalRule`].
    #[inline]
    fn visit_horizontal_rule(&mut self, _span: Span, _line_content: bool) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::LangVariant`].
    #[inline]
    fn visit_lang_variant(
        &mut self,
        span: Span,
        flags: Option<&'tt LangFlags>,
        variants: &'tt [Spanned<LangVariant>],
        raw: bool,
    ) -> Result<(), E> {
        visit_lang_variant(self, span, flags, variants, raw)
    }

    /// Visits a [`Token::Link`].
    #[inline]
    fn visit_link(
        &mut self,
        span: Span,
        target: &'tt [Spanned<Token>],
        content: &'tt [Spanned<Argument>],
        trail: Option<&'tt str>,
    ) -> Result<(), E> {
        visit_link(self, span, target, content, trail)
    }

    /// Visits a [`Token::ListItem`].
    #[inline]
    fn visit_list_item(
        &mut self,
        span: Span,
        bullets: &'tt str,
        content: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        visit_list_item(self, span, bullets, content)
    }

    /// Visits a [`Token::NewLine`].
    #[inline]
    fn visit_new_line(&mut self, _span: Span) -> Result<(), E> {
        Ok(())
    }

    /// Visits an [`Output`].
    #[inline]
    fn visit_output(&mut self, output: &'tt Output) -> Result<(), E> {
        visit_output(self, output)
    }

    /// Visits a [`Token::Parameter`].
    #[inline]
    fn visit_parameter(
        &mut self,
        _span: Span,
        _name: &'tt [Spanned<Token>],
        _default: Option<&'tt [Spanned<Token>]>,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Redirect`].
    #[inline]
    fn visit_redirect(
        &mut self,
        span: Span,
        target: &'tt [Spanned<Token>],
        content: &'tt [Spanned<Argument>],
        trail: Option<&'tt str>,
    ) -> Result<(), E> {
        visit_redirect(self, span, target, content, trail)
    }

    /// Visits a [`Token::StartAnnotation`].
    #[inline]
    fn visit_start_annotation(
        &mut self,
        _span: Span,
        _name: &'tt str,
        _attributes: &'tt [Spanned<AnnoAttribute>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::StartInclude`].
    #[inline]
    fn visit_start_include(&mut self, _span: Span, _mode: InclusionMode) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::StartTag`].
    #[inline]
    fn visit_start_tag(
        &mut self,
        _span: Span,
        _name: &str,
        _attributes: &'tt [Spanned<Argument>],
        _self_closing: bool,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::StripMarker`].
    #[inline]
    fn visit_strip_marker(&mut self, _marker: usize) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Text`].
    #[inline]
    fn visit_text(&mut self, _text: &'tt str) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TextStyle`].
    #[inline]
    fn visit_text_style(&mut self, _span: Span, _style: TextStyle) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableCaption`].
    #[inline]
    fn visit_table_caption(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableData`].
    #[inline]
    fn visit_table_data(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableEnd`].
    #[inline]
    fn visit_table_end(&mut self, _span: Span) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableHeading`].
    #[inline]
    fn visit_table_heading(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableRow`].
    #[inline]
    fn visit_table_row(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableStart`].
    #[inline]
    fn visit_table_start(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Template`].
    #[inline]
    fn visit_template(
        &mut self,
        _span: Span,
        _target: &'tt [Spanned<Token>],
        _arguments: &'tt [Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token`].
    #[inline]
    fn visit_token(&mut self, token: &'tt Spanned<Token>) -> Result<(), E> {
        visit_token(self, token)
    }

    /// Visits a list of [`Token`]s.
    #[inline]
    fn visit_tokens(&mut self, tokens: &'tt [Spanned<Token>]) -> Result<(), E> {
        visit_tokens(self, tokens)
    }
}

/// Default implementation of [`Visitor::visit_autolink`].
#[inline]
pub fn visit_autolink<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    target: &'tt [Spanned<Token>],
    content: &'tt [Spanned<Token>],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    if content.is_empty() {
        for token in target {
            visitor.visit_token(token)?;
        }
    } else {
        for token in content {
            visitor.visit_token(token)?;
        }
    }
    Ok(())
}

/// Default implementation of [`Visitor::visit_external_link`].
#[inline]
pub fn visit_external_link<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    target: &'tt [Spanned<Token>],
    content: &'tt [Spanned<Token>],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    if content.is_empty() {
        visitor.visit_tokens(target)?;
    } else {
        visitor.visit_tokens(content)?;
    }
    Ok(())
}

/// Default implementation of [`Visitor::visit_heading`].
#[inline]
pub fn visit_heading<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    _level: HeadingLevel,
    content: &'tt [Spanned<Token>],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    visitor.visit_tokens(content)
}

/// Default implementation of [`Visitor::visit_lang_variant`].
#[inline]
pub fn visit_lang_variant<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    _flags: Option<&'tt LangFlags>,
    variants: &'tt [Spanned<LangVariant>],
    _raw: bool,
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    for variant in variants {
        if let LangVariant::Text { text } = &variant.node {
            visitor.visit_tokens(text)?;
        }
    }
    Ok(())
}

/// Default implementation of [`Visitor::visit_link`].
#[inline]
pub fn visit_link<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    _target: &'tt [Spanned<Token>],
    content: &'tt [Spanned<Argument>],
    trail: Option<&'tt str>,
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    for token in content {
        visitor.visit_tokens(&token.content)?;
    }

    if let Some(trail) = trail {
        visitor.visit_text(trail)?;
    }

    Ok(())
}

/// Default implementation of [`Visitor::visit_list_item`].
#[inline]
pub fn visit_list_item<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    _bullets: &'tt str,
    content: &'tt [Spanned<Token>],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    visitor.visit_tokens(content)
}

/// Default implementation of [`Visitor::visit_output`].
#[inline]
pub fn visit_output<'tt, V, E>(visitor: &mut V, output: &'tt Output) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    visitor.visit_tokens(&output.root)
}

/// Default implementation of [`Visitor::visit_redirect`].
#[inline]
pub fn visit_redirect<'tt, V, E>(
    visitor: &mut V,
    span: Span,
    target: &'tt [Spanned<Token>],
    content: &'tt [Spanned<Argument>],
    trail: Option<&'tt str>,
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    visit_link(visitor, span, target, content, trail)
}

/// Default implementation of [`Visitor::visit_token`].
// Clippy: Literally impossible to be shorter.
#[allow(clippy::too_many_lines)]
pub fn visit_token<'tt, V, E>(visitor: &mut V, token: &'tt Spanned<Token>) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    match &token.node {
        Token::Autolink { target, content } => visitor.visit_autolink(token.span, target, content),
        Token::BehaviorSwitch { name } => {
            visitor.visit_behavior_switch(token.span, &visitor.source()[name.into_range()])
        }
        Token::Comment { content, unclosed } => visitor.visit_comment(
            token.span,
            &visitor.source()[content.into_range()],
            *unclosed,
        ),
        Token::EndAnnotation { name } => visitor.visit_end_annotation(
            token.span,
            match name {
                either::Either::Left(name) => name,
                either::Either::Right(name) => &visitor.source()[name.into_range()],
            },
        ),
        Token::EndInclude(mode) => visitor.visit_end_include(token.span, *mode),
        Token::EndTag { name } => {
            visitor.visit_end_tag(token.span, &visitor.source()[name.into_range()])
        }
        Token::Entity { value } => visitor.visit_entity(token.span, *value),
        Token::Extension {
            name,
            attributes,
            content,
        } => visitor.visit_extension(
            token.span,
            &visitor.source()[name.into_range()],
            attributes,
            content.map(|content| &visitor.source()[content.into_range()]),
        ),
        Token::ExternalLink { target, content } => {
            visitor.visit_external_link(token.span, target, content)
        }
        Token::Generated(text) => visitor.visit_generated(token.span, text),
        Token::Heading { level, content } => visitor.visit_heading(token.span, *level, content),
        Token::HorizontalRule { line_content } => {
            visitor.visit_horizontal_rule(token.span, *line_content)
        }
        Token::LangVariant {
            flags,
            variants,
            raw,
        } => visitor.visit_lang_variant(token.span, flags.as_ref(), variants, *raw),
        Token::Link {
            target,
            content,
            trail,
        } => visitor.visit_link(
            token.span,
            target,
            content,
            trail.map(|trail| &visitor.source()[trail.into_range()]),
        ),
        Token::ListItem { bullets, content } => {
            visitor.visit_list_item(token.span, &visitor.source()[bullets.into_range()], content)
        }
        Token::NewLine => visitor.visit_new_line(token.span),
        Token::Parameter { name, default } => {
            visitor.visit_parameter(token.span, name, default.as_deref())
        }
        Token::Redirect { link } => {
            let Spanned {
                node:
                    Token::Link {
                        target,
                        content,
                        trail,
                    },
                ..
            } = link.as_ref()
            else {
                unreachable!();
            };
            visitor.visit_redirect(
                token.span,
                target,
                content,
                trail.map(|trail| &visitor.source()[trail.into_range()]),
            )
        }
        Token::StartAnnotation { name, attributes } => visitor.visit_start_annotation(
            token.span,
            &visitor.source()[name.into_range()],
            attributes,
        ),
        Token::StartInclude(mode) => visitor.visit_start_include(token.span, *mode),
        Token::StartTag {
            name,
            attributes,
            self_closing,
        } => visitor.visit_start_tag(
            token.span,
            &visitor.source()[name.into_range()],
            attributes,
            *self_closing,
        ),
        Token::StripMarker(marker) => visitor.visit_strip_marker(*marker),
        Token::Text => visitor.visit_text(&visitor.source()[token.span.into_range()]),
        Token::TextStyle(style) => visitor.visit_text_style(token.span, *style),
        Token::TableCaption { attributes } => visitor.visit_table_caption(token.span, attributes),
        Token::TableData { attributes } => visitor.visit_table_data(token.span, attributes),
        Token::TableEnd => visitor.visit_table_end(token.span),
        Token::TableHeading { attributes } => visitor.visit_table_heading(token.span, attributes),
        Token::TableRow { attributes } => visitor.visit_table_row(token.span, attributes),
        Token::TableStart { attributes } => visitor.visit_table_start(token.span, attributes),
        Token::Template { target, arguments } => {
            visitor.visit_template(token.span, target, arguments)
        }
    }
}

/// Default implementation of [`Visitor::visit_tokens`].
#[inline]
pub fn visit_tokens<'tt, V, E>(visitor: &mut V, tokens: &'tt [Spanned<Token>]) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    for token in tokens {
        visitor.visit_token(token)?;
    }
    Ok(())
}
