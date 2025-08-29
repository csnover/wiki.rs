//! Helper trait for implementing token tree walkers.
//!
//! The design approach taken here was to consider the design of Wikitext where
//! templates would be interpolated into the root Wikitext string and then the
//! whole thing would be re-parsed as a complete Wikitext document. As a
//! structured tree, the same thing can be accomplished by simply emitting
//! tokens up through an arbitrary chain of [`Surrogate`] implementations until
//! they reach the root where they can be immediately transformed into the final
//! HTML. This also (currently hypothetically) allows for some synchronous
//! control flow (i.e. if the user cancels loading a page) since if the root’s
//! [`write_str`](core::fmt::Write::write_str) returns an error, the entire
//! stack unwinds.

use super::{State, stack::StackFrame};
use crate::wikitext::{
    AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant, Output, Span,
    Spanned, TextStyle, Token,
};
use either::Either;

/// A trait for implementing token tree walkers.
// TODO: This would be better where `state` and `sp` are given as a GAT, but
// (1) GATs are not dyn compatible, and (2) it may be impossible to use a GAT
// for a mutable reference because smarter people than me suggest that there is
// no way to express reborrowing in the type system (and I did try the
// `reborrow` crate).
pub trait Surrogate<E> {
    /// Visits an [`Argument`] in attribute form.
    #[inline]
    fn adopt_attribute(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _name: Option<Either<&str, &[Spanned<Token>]>>,
        _value: Either<&str, &[Spanned<Token>]>,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Autolink`].
    #[inline]
    fn adopt_autolink(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_autolink(self, state, sp, span, target, content)
    }

    /// Visits a [`Token::BehaviorSwitch`].
    #[inline]
    fn adopt_behavior_switch(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Comment`].
    #[inline]
    fn adopt_comment(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _content: &str,
        _unclosed: bool,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::EndAnnotation`].
    #[inline]
    fn adopt_end_annotation(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::EndInclude`].
    #[inline]
    fn adopt_end_include(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _mode: InclusionMode,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::EndTag`].
    #[inline]
    fn adopt_end_tag(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Entity`].
    #[inline]
    fn adopt_entity(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _value: char,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Extension`].
    #[inline]
    fn adopt_extension(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
        _attributes: &[Spanned<Argument>],
        _content: Option<&str>,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::ExternalLink`].
    #[inline]
    fn adopt_external_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_external_link(self, state, sp, span, target, content)
    }

    /// Visits a [`Token::Generated`].
    #[inline]
    fn adopt_generated(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Option<Span>,
        _text: &str,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Heading`].
    #[inline]
    fn adopt_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_heading(self, state, sp, span, level, content)
    }

    /// Visits a [`Token::HorizontalRule`].
    #[inline]
    fn adopt_horizontal_rule(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _line_content: bool,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::LangVariant`].
    #[inline]
    fn adopt_lang_variant(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        flags: Option<&LangFlags>,
        variants: &[Spanned<LangVariant>],
        raw: bool,
    ) -> Result<(), E> {
        adopt_lang_variant(self, state, sp, span, flags, variants, raw)
    }

    /// Visits a [`Token::Link`].
    #[inline]
    fn adopt_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result<(), E> {
        adopt_link(self, state, sp, span, target, content, trail)
    }

    /// Visits a [`Token::ListItem`].
    #[inline]
    fn adopt_list_item(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_list_item(self, state, sp, span, bullets, content)
    }

    /// Visits a [`Token::NewLine`].
    #[inline]
    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits an [`Output`].
    #[inline]
    fn adopt_output(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        output: &Output,
    ) -> Result<(), E> {
        adopt_output(self, state, sp, output)
    }

    /// Visits a [`Token::Parameter`].
    #[inline]
    fn adopt_parameter(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &[Spanned<Token>],
        _default: Option<&[Spanned<Token>]>,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Redirect`].
    #[inline]
    fn adopt_redirect(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result<(), E> {
        adopt_redirect(self, state, sp, span, target, content, trail)
    }

    /// Visits a [`Token::StartAnnotation`].
    #[inline]
    fn adopt_start_annotation(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
        _attributes: &[Spanned<AnnoAttribute>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::StartInclude`].
    #[inline]
    fn adopt_start_include(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _mode: InclusionMode,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::StartTag`].
    #[inline]
    fn adopt_start_tag(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &str,
        _attributes: &[Spanned<Argument>],
        _self_closing: bool,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::StripMarker`].
    #[inline]
    fn adopt_strip_marker(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _marker: usize,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Text`].
    #[inline]
    fn adopt_text(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _text: &str,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TextStyle`].
    #[inline]
    fn adopt_text_style(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _style: TextStyle,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableCaption`].
    #[inline]
    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_table_caption(self, state, sp, span, attributes, content)
    }

    /// Visits a [`Token::TableData`].
    #[inline]
    fn adopt_table_data(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_table_data(self, state, sp, span, attributes, content)
    }

    /// Visits a [`Token::TableEnd`].
    #[inline]
    fn adopt_table_end(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableHeading`].
    #[inline]
    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_table_heading(self, state, sp, span, attributes, content)
    }

    /// Visits a [`Token::TableRow`].
    #[inline]
    fn adopt_table_row(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _attributes: &[Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::TableStart`].
    #[inline]
    fn adopt_table_start(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _attributes: &[Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token::Template`].
    #[inline]
    fn adopt_template(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _target: &[Spanned<Token>],
        _arguments: &[Spanned<Argument>],
    ) -> Result<(), E> {
        Ok(())
    }

    /// Visits a [`Token`].
    #[inline]
    fn adopt_token(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        token: &Spanned<Token>,
    ) -> Result<(), E> {
        adopt_token(self, state, sp, token)
    }

    /// Visits a list of [`Token`]s.
    #[inline]
    fn adopt_tokens(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        tokens: &[Spanned<Token>],
    ) -> Result<(), E> {
        adopt_tokens(self, state, sp, tokens)
    }
}

/// Default implementation of [`Surrogate::adopt_autolink`].
#[inline]
pub fn adopt_autolink<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    target: &[Spanned<Token>],
    content: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    if content.is_empty() {
        for token in target {
            surrogate.adopt_token(state, sp, token)?;
        }
    } else {
        for token in content {
            surrogate.adopt_token(state, sp, token)?;
        }
    }
    Ok(())
}

/// Default implementation of [`Surrogate::adopt_external_link`].
#[inline]
pub fn adopt_external_link<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    target: &[Spanned<Token>],
    content: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    if content.is_empty() {
        surrogate.adopt_tokens(state, sp, target)?;
    } else {
        surrogate.adopt_tokens(state, sp, content)?;
    }
    Ok(())
}

/// Default implementation of [`Surrogate::adopt_heading`].
#[inline]
pub fn adopt_heading<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    _level: HeadingLevel,
    content: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    surrogate.adopt_tokens(state, sp, content)
}

/// Default implementation of [`Surrogate::adopt_lang_variant`].
#[inline]
pub fn adopt_lang_variant<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    _flags: Option<&LangFlags>,
    variants: &[Spanned<LangVariant>],
    _raw: bool,
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    for variant in variants {
        if let LangVariant::Text { text } = &variant.node {
            surrogate.adopt_tokens(state, sp, text)?;
        }
    }
    Ok(())
}

/// Default implementation of [`Surrogate::adopt_link`].
#[inline]
pub fn adopt_link<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    _target: &[Spanned<Token>],
    content: &[Spanned<Argument>],
    trail: Option<Spanned<&str>>,
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    for token in content {
        surrogate.adopt_tokens(state, sp, &token.content)?;
    }

    if let Some(trail) = trail {
        surrogate.adopt_text(state, sp, trail.span, trail.node)?;
    }

    Ok(())
}

/// Default implementation of [`Surrogate::adopt_list_item`].
#[inline]
pub fn adopt_list_item<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    _bullets: &str,
    content: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    surrogate.adopt_tokens(state, sp, content)
}

/// Default implementation of [`Surrogate::adopt_output`].
#[inline]
pub fn adopt_output<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    output: &Output,
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    surrogate.adopt_tokens(state, sp, &output.root)
}

/// Default implementation of [`Surrogate::adopt_redirect`].
#[inline]
pub fn adopt_redirect<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    span: Span,
    target: &[Spanned<Token>],
    content: &[Spanned<Argument>],
    trail: Option<Spanned<&str>>,
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    adopt_link(surrogate, state, sp, span, target, content, trail)
}

/// Default implementation of [`Surrogate::adopt_table_caption`].
#[inline]
pub fn adopt_table_caption<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    _attributes: &[Spanned<Argument>],
    content: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    surrogate.adopt_tokens(state, sp, content)
}

/// Default implementation of [`Surrogate::adopt_table_data`].
#[inline]
pub fn adopt_table_data<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    _attributes: &[Spanned<Argument>],
    content: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    surrogate.adopt_tokens(state, sp, content)
}

/// Default implementation of [`Surrogate::adopt_table_heading`].
#[inline]
pub fn adopt_table_heading<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    _span: Span,
    _attributes: &[Spanned<Argument>],
    content: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    surrogate.adopt_tokens(state, sp, content)
}

/// Default implementation of [`Surrogate::adopt_token`].
// Clippy: Literally impossible to be shorter.
#[allow(clippy::too_many_lines)]
pub fn adopt_token<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    token: &Spanned<Token>,
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    match &token.node {
        Token::Autolink { target, content } => {
            surrogate.adopt_autolink(state, sp, token.span, target, content)
        }
        Token::BehaviorSwitch { name } => {
            surrogate.adopt_behavior_switch(state, sp, token.span, &sp.source[name.into_range()])
        }
        Token::Comment { content, unclosed } => surrogate.adopt_comment(
            state,
            sp,
            token.span,
            &sp.source[content.into_range()],
            *unclosed,
        ),
        Token::EndAnnotation { name } => surrogate.adopt_end_annotation(
            state,
            sp,
            token.span,
            match name {
                either::Either::Left(name) => name,
                either::Either::Right(name) => &sp.source[name.into_range()],
            },
        ),
        Token::EndInclude(mode) => surrogate.adopt_end_include(state, sp, token.span, *mode),
        Token::EndTag { name } => {
            surrogate.adopt_end_tag(state, sp, token.span, &sp.source[name.into_range()])
        }
        Token::Entity { value } => surrogate.adopt_entity(state, sp, token.span, *value),
        Token::Extension {
            name,
            attributes,
            content,
        } => surrogate.adopt_extension(
            state,
            sp,
            token.span,
            &sp.source[name.into_range()],
            attributes,
            content.map(|content| &sp.source[content.into_range()]),
        ),
        Token::ExternalLink { target, content } => {
            surrogate.adopt_external_link(state, sp, token.span, target, content)
        }
        Token::Generated(text) => surrogate.adopt_generated(state, sp, Some(token.span), text),
        Token::Heading { level, content } => {
            surrogate.adopt_heading(state, sp, token.span, *level, content)
        }
        Token::HorizontalRule { line_content } => {
            surrogate.adopt_horizontal_rule(state, sp, token.span, *line_content)
        }
        Token::LangVariant {
            flags,
            variants,
            raw,
        } => surrogate.adopt_lang_variant(state, sp, token.span, flags.as_ref(), variants, *raw),
        Token::Link {
            target,
            content,
            trail,
        } => surrogate.adopt_link(
            state,
            sp,
            token.span,
            target,
            content,
            trail.map(|trail| Spanned {
                node: &sp.source[trail.into_range()],
                span: trail,
            }),
        ),
        Token::ListItem { bullets, content } => surrogate.adopt_list_item(
            state,
            sp,
            token.span,
            &sp.source[bullets.into_range()],
            content,
        ),
        Token::NewLine => surrogate.adopt_new_line(state, sp, token.span),
        Token::Parameter { name, default } => {
            surrogate.adopt_parameter(state, sp, token.span, name, default.as_deref())
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
            surrogate.adopt_redirect(
                state,
                sp,
                token.span,
                target,
                content,
                trail.map(|trail| Spanned {
                    node: &sp.source[trail.into_range()],
                    span: trail,
                }),
            )
        }
        Token::StartAnnotation { name, attributes } => surrogate.adopt_start_annotation(
            state,
            sp,
            token.span,
            &sp.source[name.into_range()],
            attributes,
        ),
        Token::StartInclude(mode) => surrogate.adopt_start_include(state, sp, token.span, *mode),
        Token::StartTag {
            name,
            attributes,
            self_closing,
        } => surrogate.adopt_start_tag(
            state,
            sp,
            token.span,
            &sp.source[name.into_range()],
            attributes,
            *self_closing,
        ),
        Token::StripMarker(marker) => surrogate.adopt_strip_marker(state, sp, token.span, *marker),
        Token::Text => {
            surrogate.adopt_text(state, sp, token.span, &sp.source[token.span.into_range()])
        }
        Token::TextStyle(style) => surrogate.adopt_text_style(state, sp, token.span, *style),
        Token::TableCaption {
            attributes,
            content,
        } => surrogate.adopt_table_caption(state, sp, token.span, attributes, content),
        Token::TableData {
            attributes,
            content,
        } => surrogate.adopt_table_data(state, sp, token.span, attributes, content),
        Token::TableEnd => surrogate.adopt_table_end(state, sp, token.span),
        Token::TableHeading {
            attributes,
            content,
        } => surrogate.adopt_table_heading(state, sp, token.span, attributes, content),
        Token::TableRow { attributes } => {
            surrogate.adopt_table_row(state, sp, token.span, attributes)
        }
        Token::TableStart { attributes } => {
            surrogate.adopt_table_start(state, sp, token.span, attributes)
        }
        Token::Template { target, arguments } => {
            surrogate.adopt_template(state, sp, token.span, target, arguments)
        }
    }
}

/// Default implementation of [`Surrogate::adopt_tokens`].
#[inline]
pub fn adopt_tokens<V, E>(
    surrogate: &mut V,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    tokens: &[Spanned<Token>],
) -> Result<(), E>
where
    V: Surrogate<E> + ?Sized,
{
    for token in tokens {
        surrogate.adopt_token(state, sp, token)?;
    }
    Ok(())
}
