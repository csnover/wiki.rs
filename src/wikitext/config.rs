//! Parser configuration data.

// This code is loosely based on `parse_wiki_text`. The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyright: Copyright 2019 Fredrik Portström and other contributors

use fancy_regex::{Regex, RegexBuilder};
use phf::Set;

/// Enabled magic links.
///
/// There will only ever be these three kinds of magic links.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MagicLinks {
    /// ISBN magic links.
    pub isbn: bool,
    /// PubMed magic links.
    pub pmid: bool,
    /// RFC magic links.
    pub rfc: bool,
}

/// Site specific configuration of a wiki.
///
/// This is generated using the program `fetch_mediawiki_configuration`.
#[derive(Debug)]
pub(crate) struct ConfigurationSource {
    /// Tag names of registered extension tags, lowercased.
    pub annotation_tags: Set<&'static str>,

    /// Whether annotations are enabled.
    pub annotations_enabled: bool,

    /// Words that can appear between `__` and `__`, lowercased.
    pub behavior_switch_words: Set<&'static str>,

    /// Tag names of registered extension tags, lowercased.
    pub extension_tags: Set<&'static str>,

    /// Registered function hooks, lowercased.
    pub function_hooks: Set<&'static str>,

    /// Whether language conversions are enabled.
    pub language_conversion_enabled: bool,

    /// A regular expression that matches link trails, in the PHP PCRE pattern
    /// format.
    pub link_trail: &'static str,

    /// The kinds of extra magic links which are enabled.
    pub magic_links: MagicLinks,

    /// Protocols that can be used for external links, lowercased.
    pub protocols: Set<&'static str>,

    /// Magic words that can be used for redirects, lowercased.
    pub redirect_magic_words: Set<&'static str>,

    /// Registered variables, lowercased.
    pub variables: Set<&'static str>,
}

/// Processed configuration data for the parser.
#[derive(Debug)]
pub(crate) struct Configuration {
    /// A compiled regular expression that matches link trails.
    pub(super) link_trail_pattern: Regex,
    /// A copy of magic links, for stupid testing purposes, since the rest of
    /// `ConfigurationSource` cannot be constructed at runtime, and I am lazy.
    #[cfg(test)]
    pub(super) magic_links: MagicLinks,
    /// Configuration source.
    source: &'static ConfigurationSource,
}

impl core::ops::Deref for Configuration {
    type Target = ConfigurationSource;

    fn deref(&self) -> &Self::Target {
        self.source
    }
}

impl Configuration {
    /// Allocates and returns a new configuration based on the given site
    /// specific configuration.
    #[must_use]
    pub fn new(source: &'static ConfigurationSource) -> Self {
        Self {
            link_trail_pattern: link_trail_regex(source.link_trail),
            #[cfg(test)]
            magic_links: source.magic_links,
            source,
        }
    }
}

/// Creates a link trail regular expression from the given string.
// This single use of `fancy_regex` is required because the ca.wiktionary.org
// linktrail contains a lookahead: `/^((?:[a-zàèéíòóúç·ïü]|'(?!'))+)(.*)$/sDu`
fn link_trail_regex(link_trail: &str) -> Regex {
    let Some((pattern, flags)) = link_trail
        .chars()
        .next()
        .and_then(|term| link_trail[1..].rsplit_once(term))
    else {
        panic!("mismatched link_trail regex");
    };

    // This end-anchored capture is on basically all of the link trail regexps,
    // but it is unused, so get rid of it for performance reasons
    let pattern = pattern.strip_suffix("(.*)$").unwrap_or(pattern);

    RegexBuilder::new(pattern)
        .dot_matches_new_line(flags.contains('s'))
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .build()
        .unwrap()
}

/// HTML5 tags allowed in Wikitext.
pub(super) static HTML5_TAGS: Set<&str> = phf::phf_set! {
    // Explicit `<a>` tags are forbidden in Wikitext.
    "abbr",
    "b", "bdi", "bdo", "big", "blockquote", "br",
    "caption", "center", "cite", "code",
    "data", "dd", "del", "dfn", "div", "dl", "dt",
    "em",
    "font",
    "h1", "h2", "h3", "h4", "h5", "h6", "hr",
    "i", "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p", "pre",
    "q",
    "rb", "rp", "rt", "rtc", "ruby",
    "s", "samp", "small", "span", "strike", "strong", "sub", "sup",
    "table", "td", "th", "time", "tr", "tt",
    "u", "ul",
    "var",
    "wbr",
};
