//! Wikitext parser.

use crate::php::strtr;
pub(crate) use codemap::{FileMap, Span, Spanned};
pub(crate) use config::{Configuration, ConfigurationSource, HTML5_TAGS, MagicLinks};
pub(crate) use inspectors::inspect;
pub(crate) use peg::str::LineCol;
use regex::Regex;
use std::{borrow::Cow, cell::Cell, collections::HashSet};

pub(crate) mod builder;
mod codemap;
mod config;
pub(crate) mod helpers;
mod inspectors;
mod parser;
#[cfg(test)]
mod tests;
pub(crate) mod visit;

/// The strip marker prefix.
pub const MARKER_PREFIX: &str = "\x7f'\"`UNIQ-";
/// The strip marker suffix.
pub const MARKER_SUFFIX: &str = "-QINU`\"'\x7f";

/// A parser error.
pub type Error = peg::error::ParseError<LineCol>;

/// A Wikitext parser.
#[derive(Clone, Debug)]
pub(crate) struct Parser<'a> {
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
    pub fn new(config: &'a Configuration) -> Self {
        let stop_char = regex::escape(parser::STOP_CHAR);
        let urltext_lookahead = Regex::new(&format!(
            "^(?:([^{stop_char}]*?)(?:__|$|[{stop_char}]|(RFC|PMID|ISBN|(?i){})))",
            protocols_regex(config.protocols.iter().copied())
        ))
        .unwrap();

        let include_tags = ["noinclude", "includeonly", "onlyinclude"];
        let mut heading_end_lookahead = "^=*(?:[ \t]|<\\!--.*?-->|</?(?:".to_string();
        for (index, tag) in include_tags
            .iter()
            .chain(config.annotation_tags.iter())
            .enumerate()
        {
            if index != 0 {
                heading_end_lookahead.push('|');
            }
            heading_end_lookahead += tag;
        }
        heading_end_lookahead += ")>)*(?:[\r\n]|$)";
        let heading_end_lookahead = regex::RegexBuilder::new(&heading_end_lookahead)
            .dot_matches_new_line(true)
            .build()
            .unwrap();

        Self {
            config,
            heading_end_lookahead,
            urltext_lookahead,
        }
    }

    /// Parses Wikitext from `source` into a token tree.
    pub fn parse(&self, source: &str, including: bool) -> Result<Output, Error> {
        let globals = Globals {
            including,
            ..Default::default()
        };
        parser::wikitext::start(source, self, &globals).map(|root| Output {
            has_onlyinclude: globals.has_onlyinclude.get(),
            root,
        })
    }

    /// Parses Wikitext from `source` into a token tree, treating templates as
    /// plain text.
    pub fn parse_no_expansion(&self, source: &str) -> Result<Output, Error> {
        parser::wikitext::start_no_expansion(
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

    /// Parses a template argument list, for debugging purposes.
    pub fn debug_parse_args(&self, args: &str) -> Result<Vec<Spanned<Argument>>, Error> {
        parser::wikitext::debug_template_args(args, self, &<_>::default())
    }
}

/// A template argument or XML-like tag attribute.
///
/// Although template arguments and tag attributes are slightly different,
/// template arguments are used as tag attributes when forwarded through the
/// `#tag` parser function, so a unified data type is used.
///
/// ```wikitext
/// {{Template|name=value}}
///            ^^^^^^^^^^
///
/// <tag name="value">
///      ^^^^^^^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Argument {
    /// The argument body.
    ///
    /// Because parser functions treat arguments as scalar values, but templates
    /// treat arguments as key-value pairs, this format is designed to support
    /// both from the same allocation.
    pub content: Vec<Spanned<Token>>,
    /// The index of the k-v delimiter in `content`, if one exists. If present,
    /// the value is at `delimiter + 1`. Otherwise, it is at 0.
    pub delimiter: Option<usize>,
    /// The index of the terminator in `content`, if one exists. This applies to
    /// attributes with quoted values and arguments inside wikilinks.
    pub terminator: Option<usize>,
}

impl Argument {
    /// The name + value parts of the argument, excluding the terminator.
    #[inline]
    pub fn combined(&self) -> &[Spanned<Token>] {
        &self.content[..self.terminator.unwrap_or(self.content.len())]
    }

    /// The name part of the argument, if one exists.
    #[inline]
    pub fn name(&self) -> Option<&[Spanned<Token>]> {
        self.delimiter.map(|delimiter| &self.content[..delimiter])
    }

    /// The value part of the argument.
    #[inline]
    pub fn value(&self) -> &[Spanned<Token>] {
        let start = self
            .delimiter
            .map_or(0, |delimiter| (delimiter + 1).min(self.content.len()));
        let end = self.terminator.unwrap_or(self.content.len());
        &self.content[start..end]
    }
}

/// An annotation tag attribute.
///
/// ```wikitext
/// <tag name="value">
///      ^^^^^^^^^^^^
/// ```
///
/// This is the same thing as Attribute, except annotation tag attributes cannot
/// contain Wikitext, and may have generated names for compatibility with
/// `<tvar|id>` syntax (where `id` is implicitly the value of a `name`
/// attribute).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnnoAttribute {
    /// Attribute name.
    pub name: either::Either<&'static str, Span>,
    /// Attribute value, excluding any quotes.
    pub value: Option<Span>,
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

/// Language conversion flags.
///
/// ```wikitext
/// -{ flag1 ; flag2 | ... }-
///    ^^^^^^^^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LangFlags {
    /// The language markup contained a set of variant names. Only these names
    /// should be considered for conversion.
    Combined(HashSet<Span>),
    /// The language markup contained a set of common flags.
    Common(HashSet<char>),
}

impl LangFlags {
    /// Special "$S" flag.
    const DOLLAR_S: char = '\x01';
    /// Special "$+" flag.
    const DOLLAR_PLUS: char = '\x02';
}

/// A language conversion variant.
///
/// ```wikitext
/// -{ text }-
///    ^^^^ (Text)
/// -{ flag | lang : text ; ... }-
///           ^^^^^^^^^^^ (TwoWay)
/// -{ flag1 ; flag2 | from => lang : to ; }-
///                    ^^^^^^^^^^^^^^^^^ (OneWay)
/// lor-{}-em
///     ^^ (Empty)
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LangVariant {
    /// Disabled language conversion.
    Text {
        /// The source text.
        text: Vec<Spanned<Token>>,
    },
    /// A one-way conversion.
    OneWay {
        /// The source language.
        from: Vec<Spanned<Token>>,
        /// The source language.
        lang: Box<Spanned<Token>>,
        /// The target language.
        to: Vec<Spanned<Token>>,
    },
    /// A bidirectional conversion.
    TwoWay {
        /// The target language.
        lang: Spanned<Token>,
        /// The text in the target language.
        text: Vec<Spanned<Token>>,
    },
    /// Disabled word conversion.
    Empty,
}

/// The parser output.
#[derive(Debug)]
pub(crate) struct Output {
    /// If true, the token tree contains an `<onlyinclude>`. Everything else
    /// should be treated as-if it is wrapped in `<noinclude>`.
    pub has_onlyinclude: bool,
    /// The token tree.
    pub root: Vec<Spanned<Token>>,
}

/// A Wikitext item.
// TODO: This should use a flat arena with refs, to avoid boxing
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Token {
    /// Plain text which can be turned into a link.
    Autolink {
        /// The link target.
        target: Vec<Spanned<Token>>,
        /// The link content.
        content: Vec<Spanned<Token>>,
    },
    /// A behavior switch.
    BehaviorSwitch {
        /// The switch name, excluding `__` markers.
        name: Span,
    },
    /// An HTML comment.
    Comment {
        /// The raw contents of the comment.
        content: Span,
        /// Whether the comment tag was unterminated.
        unclosed: bool,
    },
    /// An annotation end tag.
    EndAnnotation {
        /// The tag name.
        name: either::Either<&'static str, Span>,
    },
    /// An inclusion control end tag.
    EndInclude(InclusionMode),
    /// An HTML end tag.
    EndTag {
        /// The tag name.
        name: Span,
    },
    /// A decoded HTML entity.
    Entity {
        /// The decoded entity value.
        value: char,
    },
    /// An extension tag.
    Extension {
        /// The tag name.
        name: Span,
        /// The tag attributes.
        attributes: Vec<Spanned<Argument>>,
        /// The tag content, if it was not self-closing.
        content: Option<Span>,
    },
    /// An external link.
    ExternalLink {
        /// The link target.
        target: Vec<Spanned<Token>>,
        /// The link content. If the `Vec` is empty, an ordinal should be used.
        content: Vec<Spanned<Token>>,
    },
    /// Generated content, not part of the original input.
    Generated(String),
    /// A heading.
    Heading {
        /// The heading outline level.
        level: HeadingLevel,
        /// The heading content.
        content: Vec<Spanned<Token>>,
    },
    /// A horizontal rule.
    HorizontalRule {
        /// If true, additional content followed the horizontal rule on the same
        /// line.
        line_content: bool,
    },
    /// A language conversion markup.
    LangVariant {
        /// Metadata for the conversion.
        flags: Option<LangFlags>,
        /// Variants for the conversion.
        variants: Vec<Spanned<LangVariant>>,
        /// Whether the content should be emitted as plain text.
        raw: bool,
    },
    /// An internal link.
    Link {
        /// The target of the link.
        target: Vec<Spanned<Token>>,
        /// The text content of the link. If this `Vec` is empty, a processed
        /// version of the target title should be used.
        content: Vec<Spanned<Argument>>,
        /// The link trail to be appended to content.
        trail: Option<Span>,
    },
    /// A list item.
    ListItem {
        /// The raw bullet list for the item.
        bullets: Span,
        /// The content of the item.
        content: Vec<Spanned<Token>>,
    },
    /// A context-sensitive "\n".
    NewLine,
    /// A template parameter.
    Parameter {
        /// The parameter name.
        name: Vec<Spanned<Token>>,
        /// The default value.
        default: Option<Vec<Spanned<Token>>>,
    },
    /// A redirect block.
    Redirect {
        /// The target link of the redirect. This is always a [`Token::Link`].
        link: Box<Spanned<Token>>,
    },
    /// An annotation start tag.
    StartAnnotation {
        /// The tag name.
        name: Span,
        /// The tag attributes.
        attributes: Vec<Spanned<AnnoAttribute>>,
    },
    /// An inclusion control start tag.
    StartInclude(InclusionMode),
    /// An HTML start tag.
    StartTag {
        /// The tag name.
        name: Span,
        /// The tag attributes.
        attributes: Vec<Spanned<Argument>>,
        /// Whether the tag is self-closing (void).
        self_closing: bool,
    },
    /// A strip marker. This will only ever appear in text that passed through
    /// an Evaluator.
    StripMarker(usize),
    /// A run of plain text.
    Text,
    /// A bold or italic style.
    TextStyle(TextStyle),
    /// A table caption.
    TableCaption {
        /// The caption attributes.
        attributes: Vec<Spanned<Argument>>,
    },
    /// A table data cell.
    TableData {
        /// The cell attributes.
        attributes: Vec<Spanned<Argument>>,
    },
    /// A table end.
    TableEnd,
    /// A table heading cell.
    TableHeading {
        /// The heading cell attributes.
        attributes: Vec<Spanned<Argument>>,
    },
    /// A table row.
    TableRow {
        /// The table row attributes.
        attributes: Vec<Spanned<Argument>>,
    },
    /// A table start.
    TableStart {
        /// The table attributes.
        attributes: Vec<Spanned<Argument>>,
    },
    /// A template.
    Template {
        /// The template target.
        target: Vec<Spanned<Token>>,
        /// The template arguments.
        arguments: Vec<Spanned<Argument>>,
    },
}

/// A conversion error for out-of-range heading levels.
#[derive(Debug, thiserror::Error)]
#[error("{0} is not a valid HTML heading level")]
pub(crate) struct HeadingRangeError(u8);

/// A heading level.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct HeadingLevel(u8);

impl HeadingLevel {
    /// Returns the HTML tag name corresponding to this heading level.
    pub fn tag_name(self) -> &'static str {
        const TAGS: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];
        TAGS[usize::from(self.0) - 1]
    }
}

impl From<HeadingLevel> for u8 {
    fn from(value: HeadingLevel) -> Self {
        value.0
    }
}

impl TryFrom<u8> for HeadingLevel {
    type Error = HeadingRangeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (1..=6).contains(&value) {
            Ok(Self(value))
        } else {
            Err(HeadingRangeError(value))
        }
    }
}

/// An inclusion control tag mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InclusionMode {
    /// Display contents only when transcluded.
    IncludeOnly,
    /// Display contents only when not transcluded.
    NoInclude,
    /// Display contents only when transcluded, and treat all other content on
    /// the page as if it were wrapped by a `<noinclude>`.
    OnlyInclude,
}

/// A Wikitext text style.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextStyle {
    /// Bold text.
    Bold(TextStylePosition),
    /// Bold and italic text. These are held as a combined style because it is
    /// ambiguous in the tokeniser at the time the input is consumed whether the
    /// balance is `'''text'''''text''` or `''text'''''text'''`.
    BoldItalic,
    /// Italic text.
    Italic,
}

/// The positional attributes of a bold text style. Used for decomposition when
/// balancing quotes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextStylePosition {
    /// Any other position.
    Normal,
    /// The text style is immediately after a space followed by a single
    /// non-space character.
    Orphan,
    /// The text style is immediately after a space.
    Space,
}

/// Void HTML5 tags.
pub static VOID_TAGS: phf::Set<&str> = phf::phf_set! {
    "area", "base", "br", "col", "embed", "hr", "img",
    "input", "link", "meta", "param", "source",
    "track", "wbr",
};

/// Escapes all Wikitext and HTML control characters.
pub fn escape_no_wiki(text: &str) -> Cow<'_, str> {
    strtr(
        text,
        &[
            ("ISBN", "&#73;SBN"),
            ("PMID", "&#80;MID"),
            ("RFC", "&#82;FC"),
            ("\'\'", "&#39;&#39;"),
            ("__", "&#95;_"),
            ("!", "&#33;"),
            ("&", "&amp;"),
            (":", "&#58;"),
            (";", "&#59;"),
            ("<", "&lt;"),
            ("=", "&#61;"),
            (">", "&gt;"),
            ("[", "&#91;"),
            ("]", "&#93;"),
            ("{", "&#123;"),
            ("|", "&#124;"),
            ("}", "&#125;"),
        ],
    )
}

/// Escapes all non-HTML Wikitext control characters.
pub fn escape(text: &str) -> Cow<'_, str> {
    strtr(
        text,
        &[
            ("ISBN", "&#73;SBN"),
            ("PMID", "&#80;MID"),
            ("RFC", "&#82;FC"),
            ("__", "&#95;_"),
            ("\'\'", "&#39;&#39;"),
            ("!", "&#33;"),
            (":", "&#58;"),
            (";", "&#59;"),
            ("[", "&#91;"),
            ("]", "&#93;"),
            ("{", "&#123;"),
            ("|", "&#124;"),
            ("}", "&#125;"),
        ],
    )
}

/// Converts a list of protocols into a regular expression alternates
/// subexpression.
fn protocols_regex<'a>(protocols: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for proto in protocols {
        if !out.is_empty() {
            out.push('|');
        }
        out += &regex::escape(proto);
    }
    out
}
