//! Parser configuration data.

// This code is loosely based on `parse_wiki_text`. The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyright: Copyright 2019 Fredrik Portström and other contributors

use crate::title::Namespace;
use fancy_regex::{Regex, RegexBuilder};
use phf::{Map, Set};

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

    /// Registered title interwikis.
    pub interwiki_map: Map<&'static str, &'static str>,

    /// Whether language conversions are enabled.
    pub language_conversion_enabled: bool,

    /// A regular expression that matches link trails, in the PHP PCRE pattern
    /// format.
    pub link_trail: &'static str,

    /// The kinds of extra magic links which are enabled.
    pub magic_links: MagicLinks,

    /// Registered title namespaces.
    pub namespaces: &'static [Namespace],

    /// Protocols that can be used for external links, lowercased.
    pub protocols: Set<&'static str>,

    /// Magic words that can be used for redirects, lowercased.
    pub redirect_magic_words: Set<&'static str>,

    /// The list of allowable bytes in an article title, in a format suitable
    /// for interpolation into a PHP PCRE character set pattern.
    pub valid_title_bytes: &'static str,

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
    /// A lookup table for valid title bytes.
    pub valid_title_bytes: BitMap,
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
        let valid_title_bytes = char_class_to_bitmap(source.valid_title_bytes.bytes());

        Self {
            link_trail_pattern: link_trail_regex(source.link_trail),
            #[cfg(test)]
            magic_links: source.magic_links,
            source,
            valid_title_bytes,
        }
    }
}

/// A simple bitmap.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BitMap([u8; 32]);

impl BitMap {
    /// Returns true if the bitmap contains the given byte.
    pub fn contains(&self, byte: u8) -> bool {
        self.0[usize::from(byte / 8)] & (1 << (byte & 7)) != 0
    }
}

/// Converts a PCRE character class to a bitmap.
fn char_class_to_bitmap(bytes: impl Iterator<Item = u8>) -> BitMap {
    #[inline]
    fn nibble(b: u8) -> u8 {
        (b & 0xf) + 9 * (b >> 6)
    }

    fn unescape(iter: &mut core::iter::Peekable<impl Iterator<Item = u8>>) -> u8 {
        match iter.next() {
            None => b'\\',
            Some(b'x') => {
                if iter.next_if(|b| b == &b'{').is_some() {
                    unimplemented!()
                } else if let Some(hi) = iter.next_if(u8::is_ascii_hexdigit)
                    && let Some(lo) = iter.next_if(u8::is_ascii_hexdigit)
                {
                    nibble(hi) << 4 | nibble(lo)
                } else {
                    b'x'
                }
            }
            Some(b'a') => 0x7,
            Some(b'c') => unimplemented!(),
            Some(b'e') => 0x1b,
            Some(b'f') => 0x0c,
            Some(b'n') => b'\n',
            Some(b'r') => b'\r',
            Some(b't') => b'\t',
            Some(b'u') => unimplemented!(),
            Some(b'v') => 0x0b,
            Some(b) if b.is_ascii_digit() => {
                let mut value = b & 7;
                let mut i = 0;
                while i < 2
                    && let Some(b) = iter.next_if(u8::is_ascii_digit)
                {
                    value <<= 3;
                    value += b & 7;
                    i += 1;
                }
                value
            }
            Some(b) => b,
        }
    }

    fn value(iter: &mut core::iter::Peekable<impl Iterator<Item = u8>>) -> Option<u8> {
        match iter.next() {
            None => None,
            Some(b'\\') => Some(unescape(iter)),
            Some(b) => Some(b),
        }
    }

    let mut bits = [0; 32];
    let mut set = |b| bits[usize::from(b / 8)] |= 1_u8 << (b & 7);
    let mut iter = bytes.peekable();

    while let Some(b) = value(&mut iter) {
        set(b);
        if iter.next_if(|b| *b == b'-').is_some() {
            if let Some(next) = value(&mut iter) {
                for b in b..=next {
                    set(b);
                }
            } else {
                set(b'-');
            }
        }
    }

    BitMap(bits)
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
