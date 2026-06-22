//! Helper trait for implementing token tree visitors.

use super::{
    AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant, MagicLink,
    Output, Span, Spanned, TextStyle, Token,
};

/// A trait for visiting the tokens of a token tree.
#[expect(
    clippy::missing_errors_doc,
    reason = "the default implementations are infallible"
)]
pub trait Visitor<'tt, E> {
    /// Returns the source code of the token tree.
    fn source(&self) -> &'tt str;

    /// Visits a [`Token::Autolink`].
    #[inline]
    fn visit_autolink(&mut self, span: Span, target: &'tt [Spanned<Token>]) -> Result<(), E> {
        visit_autolink(self, span, target)
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
    fn visit_horizontal_rule(&mut self, _span: Span) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::InlineListItem`].
    #[inline]
    fn visit_inline_list_item(&mut self, _span: Span) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::LangVariant`].
    #[inline]
    fn visit_lang_variant(
        &mut self,
        span: Span,
        flags: &'tt LangFlags,
        variants: &'tt [LangVariant],
    ) -> Result<(), E> {
        visit_lang_variant(self, span, flags, variants)
    }

    /// Visits a [`Token::Link`].
    #[inline]
    fn visit_link(
        &mut self,
        span: Span,
        prefix: &'tt [Spanned<Token>],
        target: &'tt [Spanned<Token>],
        content: &'tt [Spanned<Argument>],
        trail: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        visit_link(self, span, prefix, target, content, trail)
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

    /// Visits a [`Token::MagicLink`].
    #[inline]
    fn visit_magic_link(&mut self, _span: Span, _magic: &MagicLink) -> Result<(), E> {
        Ok(())
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
        prefix: &'tt [Spanned<Token>],
        target: &'tt [Spanned<Token>],
        content: &'tt [Spanned<Argument>],
        trail: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        visit_redirect(self, span, prefix, target, content, trail)
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
        _attributes: &'tt [Spanned<Token>],
        _self_closing: bool,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::StripMarker`].
    #[inline]
    fn visit_strip_marker(&mut self, _marker: &'tt str) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableCaption`].
    #[inline]
    fn visit_table_caption(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableData`].
    #[inline]
    fn visit_table_data(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Token>],
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
        _attributes: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableRow`].
    #[inline]
    fn visit_table_row(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Token>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableStart`].
    #[inline]
    fn visit_table_start(
        &mut self,
        _span: Span,
        _attributes: &'tt [Spanned<Token>],
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
///
/// # Errors
///
/// * A call to `visitor` returns an error
#[inline]
pub fn visit_autolink<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    target: &'tt [Spanned<Token>],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    for token in target {
        visitor.visit_token(token)?;
    }
    Ok(())
}

/// Default implementation of [`Visitor::visit_external_link`].
///
/// # Errors
///
/// * A call to `visitor` returns an error
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
///
/// # Errors
///
/// * A call to `visitor` returns an error
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
///
/// # Errors
///
/// * A call to `visitor` returns an error
#[inline]
pub fn visit_lang_variant<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    _flags: &'tt LangFlags,
    variants: &'tt [LangVariant],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    for variant in variants {
        if let LangVariant::Text { text } = variant {
            visitor.visit_tokens(text)?;
        }
    }
    Ok(())
}

/// Default implementation of [`Visitor::visit_link`].
///
/// # Errors
///
/// * A call to `visitor` returns an error
#[inline]
pub fn visit_link<'tt, V, E>(
    visitor: &mut V,
    _span: Span,
    prefix: &'tt [Spanned<Token>],
    target: &'tt [Spanned<Token>],
    content: &'tt [Spanned<Argument>],
    trail: &'tt [Spanned<Token>],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    visitor.visit_tokens(prefix)?;

    if content.is_empty() {
        visitor.visit_tokens(target)?;
    } else {
        for token in content {
            visitor.visit_tokens(&token.content)?;
        }
    }

    visitor.visit_tokens(trail)?;

    Ok(())
}

/// Default implementation of [`Visitor::visit_list_item`].
///
/// # Errors
///
/// * A call to `visitor` returns an error
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
///
/// # Errors
///
/// * A call to `visitor` returns an error
#[inline]
pub fn visit_output<'tt, V, E>(visitor: &mut V, output: &'tt Output) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    visitor.visit_tokens(&output.root)
}

/// Default implementation of [`Visitor::visit_redirect`].
///
/// # Errors
///
/// * A call to `visitor` returns an error
#[inline]
pub fn visit_redirect<'tt, V, E>(
    visitor: &mut V,
    span: Span,
    prefix: &'tt [Spanned<Token>],
    target: &'tt [Spanned<Token>],
    content: &'tt [Spanned<Argument>],
    trail: &'tt [Spanned<Token>],
) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    visit_link(visitor, span, prefix, target, content, trail)
}

/// Default implementation of [`Visitor::visit_token`].
///
/// # Errors
///
/// * A call to `visitor` returns an error
#[expect(clippy::too_many_lines, reason = "this is just a big switch")]
pub fn visit_token<'tt, V, E>(visitor: &mut V, token: &'tt Spanned<Token>) -> Result<(), E>
where
    V: Visitor<'tt, E> + ?Sized,
{
    match &token.node {
        Token::Autolink(target) => visitor.visit_autolink(token.span, target),
        Token::BehaviorSwitch { name } => visitor.visit_behavior_switch(token.span, name),
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
        Token::Entity(value) => visitor.visit_entity(token.span, *value),
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
        Token::HorizontalRule => visitor.visit_horizontal_rule(token.span),
        Token::InlineListItem => visitor.visit_inline_list_item(token.span),
        Token::LangVariant { flags, variants } => {
            visitor.visit_lang_variant(token.span, flags, variants)
        }
        Token::Link {
            target,
            content,
            prefix,
            trail,
        } => visitor.visit_link(token.span, prefix, target, content, trail),
        Token::ListItem { bullets, content } => {
            visitor.visit_list_item(token.span, &visitor.source()[bullets.into_range()], content)
        }
        Token::MagicLink(magic) => visitor.visit_magic_link(token.span, magic),
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
                        prefix,
                        trail,
                    },
                ..
            } = link.as_ref()
            else {
                unreachable!();
            };
            visitor.visit_redirect(token.span, prefix, target, content, trail)
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
        Token::StripMarker(marker) => {
            visitor.visit_strip_marker(&visitor.source()[marker.into_range()])
        }
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
///
/// # Errors
///
/// * A call to `visitor` returns an error
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
