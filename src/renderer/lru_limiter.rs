//! A limiter for [`schnellru`] which limits the size of the cache according to
//! its total size in bytes.

use crate::{
    lru_limiter::ByMemoryUsageCalculator,
    wikitext::{Argument, LangFlags, LangVariant, Output, Spanned, Token, visit::Visitor},
};
use core::convert::Infallible;
use std::{collections::HashSet, sync::Arc};

/// Calculates the in-memory size of a token tree.
pub(super) struct OutputSizeCalculator {
    /// The calculated size.
    size: usize,
}

impl ByMemoryUsageCalculator for OutputSizeCalculator {
    type Target = Arc<Output>;

    fn size_of(value: &Self::Target) -> usize {
        let mut calculator = Self { size: 0 };
        let _ = calculator.visit_output(value);
        calculator.size
    }
}

impl OutputSizeCalculator {
    /// Calculates the size of the passed slice of arguments.
    fn visit_arguments(&mut self, arguments: &[Spanned<Argument>]) -> Result<(), Infallible> {
        for argument in arguments {
            self.size += vec_size(&argument.content);
            self.visit_tokens(&argument.content)?;
        }
        Ok(())
    }

    /// Calculates the size of the passed slice of language variants.
    fn visit_lang_variants(&mut self, variants: &[Spanned<LangVariant>]) -> Result<(), Infallible> {
        for variant in variants {
            match &variant.node {
                LangVariant::Text { text } => {
                    self.size += vec_size(text);
                    self.visit_tokens(text)?;
                }
                LangVariant::OneWay { from, lang, to } => {
                    self.size += vec_size(from) + size_of_val(lang) + vec_size(to);
                    self.visit_tokens(from)?;
                    self.visit_token(lang)?;
                    self.visit_tokens(to)?;
                }
                LangVariant::TwoWay { lang, text } => {
                    self.size += size_of_val(lang) + vec_size(text);
                    self.visit_token(lang)?;
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
        match &token.node {
            Token::Autolink { target, content } | Token::ExternalLink { target, content } => {
                self.size += vec_size(target) + vec_size(content);
                self.visit_tokens(target)?;
                self.visit_tokens(content)?;
            }
            Token::Generated(text) => {
                self.size += text.capacity();
            }
            Token::Heading { content, .. } | Token::ListItem { content, .. } => {
                self.size += vec_size(content);
                self.visit_tokens(content)?;
            }
            Token::LangVariant {
                flags, variants, ..
            } => {
                if let Some(flags) = flags {
                    match flags {
                        LangFlags::Combined(hash_set) => {
                            self.size += hash_set_size(hash_set);
                        }
                        LangFlags::Common(hash_set) => {
                            self.size += hash_set_size(hash_set);
                        }
                    }
                }
                self.size += vec_size(variants);
                self.visit_lang_variants(variants)?;
            }
            Token::Link {
                target, content, ..
            } => {
                self.size += vec_size(target) + vec_size(content);
                self.visit_tokens(target)?;
                self.visit_arguments(content)?;
            }
            Token::Parameter { name, default } => {
                self.size += vec_size(name);
                self.visit_tokens(name)?;
                if let Some(default) = default {
                    self.size += vec_size(default);
                    self.visit_tokens(default)?;
                }
            }
            Token::Redirect { link } => {
                self.size += size_of_val(link);
                self.visit_token(link)?;
            }
            Token::StartAnnotation { attributes, .. } => {
                self.size += vec_size(attributes);
            }
            Token::Extension { attributes, .. }
            | Token::StartTag { attributes, .. }
            | Token::TableRow { attributes }
            | Token::TableStart { attributes }
            | Token::TableCaption { attributes }
            | Token::TableData { attributes }
            | Token::TableHeading { attributes } => {
                self.size += vec_size(attributes);
                self.visit_arguments(attributes)?;
            }
            Token::Template { target, arguments } => {
                self.size += vec_size(target) + vec_size(arguments);
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
        self.size += vec_size(&output.root);
        self.visit_tokens(&output.root)
    }
}

/// Returns the total size of the heap allocation of a `HashSet<T>`, including
/// any wasted space.
fn hash_set_size<T>(set: &HashSet<T>) -> usize {
    set.capacity() * size_of::<T>()
}

/// Returns the total size of the heap allocation of a `Vec<T>`, including any
/// wasted space.
fn vec_size<T>(vec: &Vec<T>) -> usize {
    vec.capacity() * size_of::<T>()
}
