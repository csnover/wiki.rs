//! Parser configuration data.

// This code is loosely based on `parse_wiki_text`. The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyright: Copyright 2019 Fredrik Portström and other contributors

use super::{regex_switch, title::Namespace};
use core::fmt::Write as _;
use fancy_regex::{Regex as FancyRegex, RegexBuilder as FancyRegexBuilder};
use libmisc::CowExt as _;
use phf::{Map, OrderedMap, Set};
use regex::{Regex, bytes::Regex as BytesRegex};
use std::borrow::Cow;

/// Image hotlinking configuration.
#[derive(Clone, Debug)]
pub enum ImageHotlinking {
    /// Hotlinking to external images is disabled.
    Disabled,
    /// Hotlinking to external images is conditionally enabled.
    Whitelist {
        /// Hotlinking to external images is enabled if the URL starts with one
        /// of the given strings. This is `wgAllowExternalImagesFrom`.
        config: &'static [&'static str],
        /// Hotlinking to external images is enabled if the URL matches a
        /// regular expression from the `external_image_whitelist` interface
        /// message. This is `wgEnableImageWhitelist`.
        message: bool,
    },
    /// Hotlinking to anything anywhere is enabled. Go nuts. I am sure nobody
    /// will replace that image with goatse. This is `wgAllowExternalImages`.
    Enabled,
}

/// A registered language.
#[derive(Clone, Copy, Debug)]
pub struct Language {
    /// The name of the language in its own language.
    pub autonym: &'static str,
    /// Whether the language is enabled.
    pub is_enabled: bool,
    /// Whether the language is written right-to-left.
    pub is_rtl: bool,
    /// The English name of the language.
    pub name: &'static str,
}

/// Enabled magic links.
///
/// There will only ever be these three kinds of magic links.
#[derive(Clone, Copy, Debug)]
pub struct MagicLinks {
    /// ISBN magic links.
    pub isbn: bool,
    /// PubMed magic links.
    pub pmid: bool,
    /// RFC magic links.
    pub rfc: bool,
}

/// Special pages configuration.
#[derive(Debug)]
pub struct SpecialPages {
    /// Special page alias, lowercased, to “real” name.
    pub aliases: phf::Map<&'static str, &'static str>,
    /// The “real” name for a special page to its canonical representation for
    /// this wiki.
    pub canonical: phf::Map<&'static str, &'static str>,
}

/// Site specific configuration of a wiki.
///
/// This is generated using the program `fetch_mediawiki_configuration`.
#[derive(Debug)]
pub struct ConfigurationSource {
    /// Tag names of registered extension tags, lowercased.
    pub annotation_tags: Set<&'static str>,

    /// Whether annotations are enabled.
    pub annotations_enabled: bool,

    /// Words that can appear between `__` and `__`, lowercased, by alias.
    pub behavior_switch_words: Map<&'static str, &'static str>,

    /// Tag names of registered extension tags, lowercased, by alias.
    pub extension_tags: Set<&'static str>,

    /// Registered magic words used for flags and other miscellany, lowercased,
    /// by alias. Because the same alias can be used for different things in
    /// different places, the result of a map here is a list of possibilities.
    pub extra_words: Map<&'static str, &'static [&'static str]>,

    /// Registered function hooks, lowercased, by alias.
    pub function_hooks: Map<&'static str, &'static str>,

    /// Image hotlinking configuration.
    pub image_hotlinking: ImageHotlinking,

    /// A map from a registered title interwiki prefix to bcp47 code for
    /// interlanguages.
    pub interlanguage_map: Map<&'static str, &'static str>,

    /// Registered title interwikis.
    pub interwiki_map: Map<&'static str, &'static str>,

    /// The default page language code.
    pub language: &'static str,

    /// A reverse map from a BCP-47 language code to the corresponding index in
    /// [`Self::languages`].
    pub language_bcp47: Map<&'static str, u16>,

    /// Whether language conversions are enabled.
    pub language_conversion_enabled: bool,

    /// A map of registered language conversions, from a language code to a
    /// list of fallback language codes.
    pub language_conversions: Map<&'static str, &'static [&'static str]>,

    /// A map from a MediaWiki language code to language information.
    pub languages: OrderedMap<&'static str, Language>,

    /// A list of allowable characters that match link prefixes, in a format
    /// suitable for interpolation into a PHP PCRE character set pattern.
    pub link_prefix: &'static str,

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

    /// Registered special pages.
    pub special_pages: SpecialPages,

    /// The image thumbnail breakpoint sizes.
    pub thumb_limits: &'static [u32],

    /// The list of allowable bytes in an article title, in a format suitable
    /// for interpolation into a PHP PCRE character set pattern.
    pub valid_title_bytes: &'static str,

    /// Registered variables, lowercased, by alias.
    pub variables: Map<&'static str, &'static str>,
}

/// Processed configuration data for the parser.
#[derive(Debug)]
pub struct Configuration {
    /// A set of compiled regular expressions that match for protocols and magic
    /// links for escaping literal strings containing these patterns.
    pub escape_pattern: Option<Regex>,
    /// A compiled regular expression that matches parameterised magic words.
    extra_words_pattern: ExtraWordsPattern,
    /// A compiled regular expression that matches link prefixes.
    ///
    /// This is basically outsourcing the parsing and creation of a sparse bit
    /// map to the regex engine.
    pub link_prefix_pattern: Option<BytesRegex>,
    /// A compiled regular expression that matches link trails.
    pub link_trail_pattern: FancyRegex,
    /// A copy of magic links, for stupid testing purposes, since the rest of
    /// `ConfigurationSource` cannot be constructed at runtime, and I am lazy.
    #[cfg(test)]
    pub magic_links: MagicLinks,
    /// A prefix search for [`ConfigurationSource::protocols`].
    pub protocols_pattern: Regex,
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
    ///
    /// # Panics
    ///
    /// * `link_prefix` or `link_trail` cannot be parsed as a regular expression
    #[must_use]
    pub fn new(source: &'static ConfigurationSource) -> Self {
        let valid_title_bytes = char_class_to_bitmap(source.valid_title_bytes.bytes());

        let link_prefix_pattern = (!source.link_prefix.is_empty())
            .then(|| BytesRegex::new(&format!(r"^[{}]+\[\[", source.link_prefix)).unwrap());

        let protocols_pattern =
            Regex::new(&format!("^(?i:{})", regex_switch(source.protocols.iter()))).unwrap();

        Self {
            escape_pattern: build_escape_pattern(&source.protocols, source.magic_links),
            extra_words_pattern: ExtraWordsPattern::new(source.extra_words.entries()),
            link_prefix_pattern,
            link_trail_pattern: link_trail_regex(source.link_trail),
            #[cfg(test)]
            magic_links: source.magic_links,
            protocols_pattern,
            source,
            valid_title_bytes,
        }
    }

    /// Tries matching the given `alias` to one of the configured `extra_words`,
    /// returning the list of canonical names for the alias if it matches, and
    /// optionally a value if the `alias` was a parameterised alias.
    ///
    /// # Errors
    ///
    /// * returns the `alias` if no match
    pub fn magic_word_matches<'a>(
        &self,
        alias: Cow<'a, str>,
    ) -> Result<ExtraWordsMatch<'a, '_>, Cow<'a, str>> {
        if let Some(canonical) = self.extra_words.get(&alias).copied() {
            Ok((canonical, None))
        } else {
            let patterns = &self.extra_words_pattern.patterns;
            let matches = patterns.matches(&alias);
            let Some(index) = matches.iter().next() else {
                return Err(alias);
            };
            let which = &self.extra_words_pattern.which[index];
            let canonical = &which.canonical;
            let arg_range = usize::from(which.prefix)..alias.len() - usize::from(which.suffix);
            Ok((canonical, Some(alias.map_ref(|alias| &alias[arg_range]))))
        }
    }
}

/// A simple bitmap.
#[derive(Clone, Copy, Debug, Default)]
pub struct BitMap([u8; 32]);

impl BitMap {
    /// Returns true if the bitmap contains the given byte.
    #[must_use]
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

/// Parameterised extra words.
#[derive(Debug)]
pub struct ExtraWordsPattern {
    /// A regular expression set for matching any parameterised extra words.
    patterns: regex::RegexSet,
    /// For each pattern in the set, data required to return the canonical list
    /// and argument value.
    which: Vec<ExtraWordsValue>,
}

/// The type for a matching extra word.
pub type ExtraWordsMatch<'a, 'b> = (&'b [&'b str], Option<Cow<'a, str>>);

impl ExtraWordsPattern {
    /// Creates a new `ExtraWordsPattern`.
    fn new<'a, I>(extra_words: I) -> Self
    where
        I: Iterator<Item = (&'a &'static str, &'a &'static [&'static str])> + Clone,
    {
        let param_words = extra_words.filter(|(key, _)| key.contains("$1"));

        let res = param_words
            .clone()
            .map(|(key, _)| format!("^{}", regex::escape(key).replace("\\$1", ".*")));

        let patterns = regex::RegexSetBuilder::new(res)
            .case_insensitive(true)
            .build()
            .unwrap();

        let which = param_words
            .map(|(key, canonical)| {
                let pos = key.find("$1").unwrap();
                ExtraWordsValue {
                    canonical,
                    prefix: u8::try_from(pos).unwrap(),
                    suffix: u8::try_from(key.len() - pos - 2).unwrap(),
                }
            })
            .collect();

        Self { patterns, which }
    }
}

/// Data associated with a parameterised extra word.
#[derive(Debug)]
struct ExtraWordsValue {
    /// The canonical value list from [`ConfigurationSource::extra_words`] for
    /// this pattern.
    canonical: &'static [&'static str],
    /// The length of the parameter prefix.
    prefix: u8,
    /// The length of the parameter suffix.
    suffix: u8,
}

/// Builds a regular expression from the given `protocols` and `magic_links`
/// that can be used to match these things in a string so that they can be
/// escaped in the blessed way by the cursed MediaWiki.
///
/// # Panics
///
/// * Building the regular expression fails
#[must_use]
pub fn build_escape_pattern(protocols: &phf::Set<&str>, magic_links: MagicLinks) -> Option<Regex> {
    let mut escape_pattern = String::new();
    let switch = regex_switch(protocols.iter().filter_map(|proto| proto.strip_suffix(':')));

    if !switch.is_empty() {
        write!(escape_pattern, "(?:(?i){switch})(:)").unwrap();
    }

    let switch = regex_switch(
        magic_links
            .isbn
            .then_some("ISBN")
            .into_iter()
            .chain(magic_links.pmid.then_some("PMID"))
            .chain(magic_links.rfc.then_some("RFC")),
    );
    if !switch.is_empty() {
        if !escape_pattern.is_empty() {
            escape_pattern.push('|');
        }
        write!(escape_pattern, r"(?:{switch})(\s)").unwrap();
    }

    (!escape_pattern.is_empty()).then(|| Regex::new(&escape_pattern).unwrap())
}

/// Creates a link trail regular expression from the given string.
// This single use of `fancy_regex` is required because the ca.wiktionary.org
// linktrail contains a lookahead: `/^((?:[a-zàèéíòóúç·ïü]|'(?!'))+)(.*)$/sDu`
fn link_trail_regex(link_trail: &str) -> FancyRegex {
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

    FancyRegexBuilder::new(pattern)
        .dot_matches_new_line(flags.contains('s'))
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .build()
        .unwrap()
}
