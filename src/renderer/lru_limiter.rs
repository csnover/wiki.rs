//! A limiter for [`schnellru`] which limits the size of the cache according to
//! its total size in bytes.

use crate::{
    lru_limiter::ByMemoryUsageCalculator,
    wikitext::{Argument, LangFlags, LangVariant, Output, Span, Spanned, Token, visit::Visitor},
};
use core::convert::Infallible;
use std::rc::Rc;

/// Calculates the in-memory size of a token tree.
pub(super) struct OutputSizeCalculator {
    /// The calculated size.
    size: usize,
}

impl ByMemoryUsageCalculator for OutputSizeCalculator {
    type Target = Rc<Output>;

    fn size_of(value: &Self::Target) -> usize {
        let mut calculator = Self { size: 0 };
        let _ = calculator.visit_output(value);
        calculator.size + size_of::<Self::Target>()
    }
}

impl OutputSizeCalculator {
    /// Calculates the size of the passed slice of arguments.
    fn visit_arguments(&mut self, arguments: &[Spanned<Argument>]) -> Result<(), Infallible> {
        for argument in arguments {
            self.size += size_of_val(argument);
            self.visit_tokens(&argument.content)?;
        }
        Ok(())
    }

    /// Calculates the size of the passed slice of language variants.
    fn visit_lang_variants(&mut self, variants: &[Spanned<LangVariant>]) -> Result<(), Infallible> {
        for variant in variants {
            self.size += size_of_val(variant);
            match &variant.node {
                LangVariant::Text { text } => {
                    self.visit_tokens(text)?;
                }
                LangVariant::OneWay { from, lang, to } => {
                    self.visit_tokens(from)?;
                    self.size += size_of_val(lang);
                    self.visit_tokens(to)?;
                }
                LangVariant::TwoWay { lang, text } => {
                    self.size += size_of_val(lang);
                    self.visit_tokens(text)?;
                }
                LangVariant::Empty => {}
            }
        }
        Ok(())
    }
}

impl<'tt> Visitor<'tt, Infallible> for OutputSizeCalculator {
    fn source(&self) -> &'tt str {
        ""
    }

    fn visit_token(&mut self, token: &'tt Spanned<Token>) -> Result<(), Infallible> {
        self.size += size_of::<Spanned<Token>>();
        match &token.node {
            Token::Autolink { target, content } | Token::ExternalLink { target, content } => {
                self.visit_tokens(target)?;
                self.visit_tokens(content)?;
            }
            Token::Generated(text) => {
                self.size += size_of_val(text) + text.len();
            }
            Token::Heading { content, .. } | Token::ListItem { content, .. } => {
                self.visit_tokens(content)?;
            }
            Token::LangVariant {
                flags, variants, ..
            } => {
                if let Some(flags) = flags {
                    match flags {
                        LangFlags::Combined(hash_set) => {
                            self.size += size_of_val(hash_set) + hash_set.len() * size_of::<Span>();
                        }
                        LangFlags::Common(hash_set) => {
                            self.size += size_of_val(hash_set) + hash_set.len() * size_of::<char>();
                        }
                    }
                }
                self.visit_lang_variants(variants)?;
            }
            Token::Link {
                target, content, ..
            } => {
                self.visit_tokens(target)?;
                self.visit_arguments(content)?;
            }
            Token::Parameter { name, default } => {
                self.visit_tokens(name)?;
                if let Some(default) = default {
                    self.visit_tokens(default)?;
                }
            }
            Token::Redirect { link } => {
                self.visit_token(link)?;
            }
            Token::StartAnnotation { attributes, .. } => {
                self.size += size_of_val(attributes.as_slice());
            }
            Token::Extension { attributes, .. }
            | Token::StartTag { attributes, .. }
            | Token::TableRow { attributes }
            | Token::TableStart { attributes }
            | Token::TableCaption { attributes }
            | Token::TableData { attributes }
            | Token::TableHeading { attributes } => {
                self.visit_arguments(attributes)?;
            }
            Token::Template { target, arguments } => {
                self.visit_tokens(target)?;
                self.visit_arguments(arguments)?;
            }
            Token::BehaviorSwitch { .. }
            | Token::Comment { .. }
            | Token::EndAnnotation { .. }
            | Token::EndInclude(_)
            | Token::EndTag { .. }
            | Token::Entity { .. }
            | Token::HorizontalRule { .. }
            | Token::NewLine
            | Token::StartInclude(_)
            | Token::StripMarker(_)
            | Token::Text
            | Token::TextStyle(_)
            | Token::TableEnd => {}
        }
        Ok(())
    }

    fn visit_output(&mut self, output: &'tt Output) -> Result<(), Infallible> {
        self.size += size_of::<Output>();
        self.visit_tokens(&output.root)
    }
}
