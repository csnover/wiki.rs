//! Wikitext parser.

use core::cell::Cell;
use libwikitext_common::config::Configuration;
use libwikitext_parse::{Argument, Error, Output, STOP_CHAR, Spanned};
use regex::Regex;

mod grammar;
#[cfg(test)]
mod tests;

/// A Wikitext parser.
#[derive(Clone, Debug)]
pub struct Parser<'a> {
    /// The configuration for the parser.
    config: &'a Configuration,
    /// A pattern used to identify the end of a heading.
    ///
    /// Normally a heading ends at the end of a line, but it is legal to have
    /// whitespace, comments, annotation end tags, and inclusion control end
    /// tags at the end of that line.
    heading_end_lookahead: Regex,
    /// A pattern used for the “Very Special Performance Hack”.
    urltext_lookahead: Regex,
}

impl<'a> Parser<'a> {
    /// Creates a new parser with the given configuration.
    ///
    /// # Panics
    ///
    /// * A regular expression fails to compile
    #[must_use]
    pub fn new(config: &'a Configuration) -> Self {
        let stop_char = regex::escape(STOP_CHAR);
        let urltext_lookahead = Regex::new(&format!(
            "^(?:([^{stop_char}]*?)(?:__|$|[{stop_char}]|(RFC|PMID|ISBN|(?i){})))",
            regex_switch(config.protocols.iter().copied())
        ))
        .unwrap();

        let heading_end_lookahead = regex::RegexBuilder::new(&format!(
            "^=*(?:[ \t]|<\\!--.*?-->|</?(?:{})>)*(?:[\r\n]|$)",
            regex_switch(
                INCLUDE_TAGS
                    .iter()
                    .chain(config.annotation_tags.iter())
                    .copied()
            )
        ))
        .dot_matches_new_line(true)
        .build()
        .unwrap();

        Self {
            config,
            heading_end_lookahead,
            urltext_lookahead,
        }
    }

    /// Returns the parser configuration.
    #[must_use]
    pub fn config(&self) -> &Configuration {
        self.config
    }

    /// Parses a template argument list, for debugging purposes.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as an argument list
    pub fn debug_parse_args(&self, args: &str) -> Result<Vec<Spanned<Argument>>, Error> {
        grammar::wikitext::debug_template_args(args, self, &<_>::default())
    }

    /// Parses Wikitext from `source` into a token tree.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as Wikitext
    pub fn parse(&self, source: &str, including: bool) -> Result<Output, Error> {
        let globals = Globals {
            including,
            ..Default::default()
        };
        grammar::wikitext::start(source, self, &globals).map(|root| Output {
            has_onlyinclude: globals.has_onlyinclude.get(),
            root,
        })
    }

    /// Parses a `<gallery>` media item.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as a `<gallery>` media item
    pub fn parse_gallery_media(&self, options: &str) -> Result<Vec<Spanned<Argument>>, Error> {
        grammar::wikitext::gallery_image_options(
            options,
            self,
            &Globals {
                including: true,
                ..Default::default()
            },
        )
    }

    /// Parses Wikitext from `source` into a token tree, treating templates as
    /// plain text.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as Wikitext
    pub fn parse_no_expansion(&self, source: &str) -> Result<Output, Error> {
        grammar::wikitext::start_no_expansion(
            source,
            self,
            &Globals {
                including: true,
                ..Default::default()
            },
        )
        .map(|root| Output {
            has_onlyinclude: false,
            root,
        })
    }

    /// Parses a single redirect and returns its target.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as a Wikitext redirect
    pub fn parse_redirect<'s>(&self, source: &'s str) -> Result<&'s str, Error> {
        grammar::wikitext::single_redirect(
            source,
            self,
            &Globals {
                including: false,
                ..Default::default()
            },
        )
    }
}

/// Temporary global state for a single document during parsing.
#[derive(Debug, Default)]
struct Globals {
    /// An `<onlyinclude>` tag was discovered somewhere in the input.
    /// This information needs to be passed out so the tree walker knows to
    /// skip everything by default, instead of needing to do a tree pre-scan or
    /// buffer everything Just In Case.
    has_onlyinclude: Cell<bool>,
    /// If true, parse the document in include mode.
    including: bool,
}

/// Converts a list of protocols into a regular expression alternates
/// subexpression.
fn regex_switch<'a>(protocols: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for proto in protocols {
        if !out.is_empty() {
            out.push('|');
        }
        out += &regex::escape(proto);
    }
    out
}

/// Inclusion control tags.
const INCLUDE_TAGS: phf::Set<&str> = phf::phf_set! {
    "includeonly", "noinclude", "onlyinclude"
};
