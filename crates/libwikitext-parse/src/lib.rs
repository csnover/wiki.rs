//! Wikitext parser.

mod codemap;
mod grammar;
pub mod helpers;
mod inspectors;
pub mod lru_limiter;
pub mod strip;
pub mod visit;

pub use codemap::{FileMap, Span, Spanned};
use core::cell::Cell;
pub use inspectors::{inspect, inspect_one};
use libmisc::CowExt as _;
use libphp_rs::strtr;
use libwikitext_common::{
    AnchorEncodeMode, DEPRECATED_LANGUAGE_CODES, config::Configuration, escape_id_url,
    lang_to_bcp47, normalize_section_name, regex_switch,
};
pub use peg::str::LineCol;
use regex::{Captures, Regex};
use std::{borrow::Cow, collections::HashSet};

/// A parser result.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A parser error.
pub type Error = peg::error::ParseError<LineCol>;

/// A Wikitext parser.
#[derive(Clone, Debug)]
pub struct Parser<'config> {
    /// The prefix search for [behavior switches][1].
    ///
    /// [1]: libwikitext_common::config::ConfigurationSource::behavior_switch_words
    bs: Regex,

    /// The configuration for the parser.
    config: &'config Configuration,

    /// The prefix search for [language conversion variants][1].
    ///
    /// [1]: libwikitext_common::config::ConfigurationSource::language_conversions
    lang: Regex,

    /// The prefix search for [redirects][1].
    ///
    /// [1]: libwikitext_common::config::ConfigurationSource::redirect_magic_words
    redirect: Regex,
}

impl<'config> Parser<'config> {
    /// Creates a new `Parser` with the given `config`.
    ///
    /// # Panics
    ///
    /// * if prefix search regular expressions fail to build
    pub fn new(config: &'config Configuration) -> Self {
        let bs = Regex::new(&format!(
            "^(?i:{})",
            regex_switch(config.behavior_switch_words.keys())
        ))
        .unwrap();

        // Collecting into a hash set for value deduplication, which ends up
        // being convenient for having only a single set to check for adding
        // deprecated codes
        let mut lang = config
            .language_conversions
            .keys()
            .copied()
            .map(Cow::Borrowed)
            .chain(
                config
                    .language_conversions
                    .keys()
                    .copied()
                    .map(lang_to_bcp47),
            )
            .collect::<HashSet<_>>();
        for (&k, &v) in &DEPRECATED_LANGUAGE_CODES {
            if lang.contains(v) {
                lang.insert(Cow::Borrowed(k.as_str()));
            }
        }

        let lang = Regex::new(&format!("^(?:{})", regex_switch(lang.iter()))).unwrap();
        let redirect = Regex::new(&format!(
            "^(?i:{})",
            regex_switch(config.redirect_magic_words.iter())
        ))
        .unwrap();

        Self {
            bs,
            config,
            lang,
            redirect,
        }
    }

    /// Extracts a section name from `source` and encodes it in a format
    /// suitable for an element ID using the mode `mode`.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as a section name
    pub fn anchor_encode<'a>(
        &self,
        source: &'a str,
        mode: AnchorEncodeMode,
    ) -> Result<Cow<'a, str>> {
        let root = self.parse(source)?;
        let text = borrow_fast(source, &root).map_or_else(
            || {
                // TODO: Technically this is supposed to not care about whether
                // a link is a category or interwiki because the original code
                // was so shitty it just tried to scoop out the insides of a
                // link using yet more regular expressions.
                let mut extractor =
                    helpers::TextContent::new(self.config, false, source, String::new());
                let _ = visit::Visitor::visit_tokens(&mut extractor, &root);
                Cow::Owned(extractor.finish())
            },
            Cow::Borrowed,
        );

        Ok(text
            .map(normalize_section_name)
            .map(|s| escape_id_url(s, mode)))
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
    pub fn debug_parse_args(&self, args: &str) -> Result<Vec<Spanned<Argument>>> {
        grammar::wikitext::debug_template_args(args, self, &<_>::default())
    }

    /// Parses preprocessed Wikitext from `source` into a Wikitext token tree.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as Wikitext
    pub fn parse(&self, source: &str) -> Result<Vec<Spanned<Token>>> {
        grammar::wikitext::start(source, self)
    }

    /// Parses late-evaluated Wikitext from `source` into a list of attributes.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as Wikitext
    pub fn parse_attributes(&self, attributes: &str) -> Result<Vec<Spanned<Argument>>> {
        grammar::wikitext::late_attributes(attributes, self)
    }

    /// Parses a `<gallery>` media item.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as a `<gallery>` media item
    pub fn parse_gallery_media(&self, options: &str) -> Result<Vec<Spanned<Argument>>> {
        grammar::wikitext::gallery_image_options(options, self)
    }

    /// Parses a single redirect and returns its target.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as a Wikitext redirect
    pub fn parse_redirect<'s>(&self, source: &'s str) -> Result<&'s str> {
        grammar::wikitext::single_redirect(source, self)
    }

    /// Parses Wikitext from `source` into a preprocessor token tree.
    ///
    /// # Errors
    ///
    /// * `source` cannot be parsed as Wikitext
    pub fn preprocess(&self, source: &str, including: bool) -> Result<Output> {
        let options = PreprocessorOptions {
            has_onlyinclude: Cell::new(false),
            including,
        };

        grammar::wikitext::preprocess(source, self, &options).map(|root| Output {
            has_onlyinclude: options.has_onlyinclude.get(),
            root,
        })
    }
}

/// Options for the preprocessor.
#[derive(Debug, Default)]
struct PreprocessorOptions {
    /// An `<onlyinclude>` tag was discovered somewhere in the input.
    /// This information needs to be passed out so the tree walker knows to
    /// skip everything by default, instead of needing to do a tree pre-scan or
    /// buffer everything Just In Case.
    has_onlyinclude: Cell<bool>,
    /// If true, parse the document in include mode.
    including: bool,
}

// This does not change the outcome of a rule match so can just hash to nothing
impl core::hash::Hash for PreprocessorOptions {
    fn hash<H: core::hash::Hasher>(&self, _: &mut H) {}
}

impl peg::Cacheable for PreprocessorOptions {
    type Cached = ();
    type Key = ();

    fn key(&self) -> &Self::Key {
        &()
    }

    fn to_cached(&self) -> Self::Cached {}
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
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Argument {
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
    #[must_use]
    pub fn combined(&self) -> &[Spanned<Token>] {
        &self.content[..self.terminator.unwrap_or(self.content.len())]
    }

    /// The name part of the argument, if one exists.
    #[inline]
    #[must_use]
    pub fn name(&self) -> Option<&[Spanned<Token>]> {
        self.delimiter.map(|delimiter| &self.content[..delimiter])
    }

    /// The value part of the argument.
    #[inline]
    #[must_use]
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
pub struct AnnoAttribute {
    /// Attribute name.
    pub name: either::Either<&'static str, Span>,
    /// Attribute value, excluding any quotes.
    pub value: Option<Span>,
}

/// Language conversion flags.
///
/// ```wikitext
/// -{ flag1 ; flag2 | ... }-
///    ^^^^^^^^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LangFlags {
    /// The language markup contained a set of variant names. Only these names
    /// should be considered for conversion.
    Combined(HashSet<Span>),
    /// The language markup contained a set of common flags.
    Common(CommonLangFlags),
}

impl LangFlags {
    /// Returns true if the associated rules should be interpreted as raw text.
    #[must_use]
    pub fn is_raw(&self) -> bool {
        match self {
            Self::Combined(_) => true,
            Self::Common(flags) => flags.intersects(CommonLangFlags::RAW | CommonLangFlags::NAME),
        }
    }
}

impl Default for LangFlags {
    fn default() -> Self {
        LangFlags::Common(CommonLangFlags::SHOW)
    }
}

bitflags::bitflags! {
    /// Common language conversion flags.
    ///
    /// Arbitrary combinations of flags can be used even though there are only a
    /// few things that make sense to do in combination; flags exposed in the
    /// API are converted into secret flags using what is effectively a very
    /// small state machine for flags.
    ///
    /// This entire feature is absolutely demented and no one should have ever
    /// written it, let alone allowed this code to pass code review. And now it
    /// is part of one of the most used document formats. Great!
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CommonLangFlags: u8 {
        /// Add a translation term for the source document and print the
        /// translated value for the current display language immediately.
        const AAAAH       = Self::ADD.bits() | Self::SHOW.bits();
        /// Add a page title override for a given language.
        const TITLE       = 1 << 0;
        /// Print the text in the tag without translation.
        const RAW         = 1 << 1;
        /// Print a debugging view of the rule.
        const DESCRIBE    = 1 << 2;
        /// Remove a term from the dictionary.
        const REMOVE      = 1 << 3;
        /// Display nothing. (Sorry, what else would 'H' stand for?)
        const HOLD_IT_IN  = 1 << 4;
        /// Print the localised name of the language of a language code.
        const NAME        = 1 << 5;
        /// Adds a term to the dictionary.
        const ADD         = 1 << 6;
        /// Print the translated value.
        const SHOW        = 1 << 7;
    }
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
pub enum LangVariant {
    /// Disabled word conversion.
    Empty,
    /// A one-way conversion.
    ///
    /// A one-way conversion translates a phrase from the source Wikitext to a
    /// specific target language.
    OneWay {
        /// The source text.
        from: Vec<Spanned<Token>>,
        /// The target language.
        lang: Span,
        /// The replacement text.
        to: Vec<Spanned<Token>>,
    },
    /// A tag containing raw text which should be excluded from conversion.
    Text {
        /// The raw text.
        text: Vec<Spanned<Token>>,
    },
    /// A two-way conversion.
    ///
    /// A two-way conversion defines a phrase in a given language. By combining
    /// several [`Self::TwoWay`] in a single [`Token::LangVariant`], a
    /// dictionary of translations is created. An instance of a defined phrase
    /// in a Wikitext document with the given source language will be replaced
    /// by an associated phrase in the viewer’s target language.
    TwoWay {
        /// The language.
        lang: Span,
        /// The text.
        text: Vec<Spanned<Token>>,
    },
}

/// A parsed magic link.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MagicLink {
    /// An ISBN identifier.
    Isbn(String),
    /// A PubMed identifier.
    Pmid(Span),
    /// An RFC identifier.
    Rfc(Span),
}

/// The parser output.
#[derive(Debug)]
pub struct Output {
    /// If true, the token tree contains an `<onlyinclude>`. Everything else
    /// should be treated as-if it is wrapped in `<noinclude>`.
    pub has_onlyinclude: bool,
    /// The token tree.
    pub root: Vec<Spanned<Token>>,
}

/// A Wikitext item.
// TODO: This should use a flat arena with refs, to avoid boxing
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Token {
    /// Plain text which can be turned into a link.
    Autolink(Vec<Spanned<Token>>),
    /// A behavior switch.
    BehaviorSwitch {
        /// The switch name, excluding `__` markers.
        name: &'static str,
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
    Entity(char),
    /// An extension tag.
    Extension {
        /// The tag attributes.
        attributes: Vec<Spanned<Argument>>,
        /// The tag content, if it was not self-closing.
        content: Option<Span>,
        /// The tag name.
        name: Span,
    },
    /// An external link.
    ExternalLink {
        /// The link content. If the `Vec` is empty, an ordinal should be used.
        content: Vec<Spanned<Token>>,
        /// The link target.
        target: Vec<Spanned<Token>>,
    },
    /// Generated content, not part of the original input.
    Generated(String),
    /// A heading.
    Heading {
        /// The heading content.
        content: Vec<Spanned<Token>>,
        /// The heading outline level.
        level: HeadingLevel,
    },
    /// A horizontal rule.
    HorizontalRule,
    /// An inline definition detail.
    InlineListItem,
    /// A language conversion markup.
    LangVariant {
        /// Metadata for the conversion.
        flags: LangFlags,
        /// Variants for the conversion.
        variants: Vec<LangVariant>,
    },
    /// An internal link.
    Link {
        /// The text content of the link. If this `Vec` is empty, a processed
        /// version of the target title should be used.
        content: Vec<Spanned<Argument>>,
        /// The link prefix to be prefixed to content.
        prefix: Vec<Spanned<Token>>,
        /// The target of the link.
        target: Vec<Spanned<Token>>,
        /// The link trail to be appended to content.
        trail: Vec<Spanned<Token>>,
    },
    /// A list item.
    ListItem {
        /// The raw bullet list for the item.
        bullets: Span,
        /// The content of the item.
        content: Vec<Spanned<Token>>,
    },
    /// Semi-structured plain text that can be turned into a link.
    MagicLink(MagicLink),
    /// A context-sensitive "\n".
    NewLine,
    /// A template parameter.
    Parameter {
        /// The default value.
        default: Option<Vec<Spanned<Token>>>,
        /// The parameter name.
        name: Vec<Spanned<Token>>,
    },
    /// A redirect block.
    Redirect {
        /// The target link of the redirect. This is always a [`Token::Link`].
        link: Box<Spanned<Token>>,
    },
    /// An annotation start tag.
    StartAnnotation {
        /// The tag attributes.
        attributes: Vec<Spanned<AnnoAttribute>>,
        /// The tag name.
        name: Span,
    },
    /// An inclusion control start tag.
    StartInclude(InclusionMode),
    /// An HTML start tag.
    StartTag {
        /// The tag attributes.
        attributes: Vec<Spanned<Token>>,
        /// The tag name.
        name: Span,
        /// Whether the tag is self-closing (void).
        self_closing: bool,
    },
    /// A strip marker. This will only ever appear in text that passed through
    /// an Evaluator.
    StripMarker(Span),
    /// A table caption.
    TableCaption {
        /// The caption attributes.
        attributes: Vec<Spanned<Token>>,
    },
    /// A table data cell.
    TableData {
        /// The cell attributes.
        attributes: Vec<Spanned<Token>>,
    },
    /// A table end.
    TableEnd,
    /// A table header cell.
    TableHeader {
        /// The header cell attributes.
        attributes: Vec<Spanned<Token>>,
    },
    /// A table row.
    TableRow {
        /// The table row attributes.
        attributes: Vec<Spanned<Token>>,
    },
    /// A table start.
    TableStart {
        /// The table attributes.
        attributes: Vec<Spanned<Token>>,
        /// The depth of the table indent hack.
        indent: u8,
    },
    /// A template.
    Template {
        /// The template arguments.
        arguments: Vec<Spanned<Argument>>,
        /// The template target.
        target: Vec<Spanned<Token>>,
    },
    /// A run of plain text.
    Text,
    /// A bold or italic style.
    TextStyle(TextStyle),
}

/// A conversion error for out-of-range heading levels.
#[derive(Debug, thiserror::Error)]
#[error("{0} is not a valid HTML heading level")]
pub struct HeadingRangeError(u8);

/// A conversion error for non-heading HTML tags.
#[derive(Debug, thiserror::Error)]
#[error("not a valid HTML heading tag")]
pub struct ParseHeadingError;

/// A heading level.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct HeadingLevel(u8);

impl HeadingLevel {
    /// The list of HTML heading tags.
    pub const TAGS: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];

    /// Returns the HTML tag name corresponding to this heading level.
    #[must_use]
    pub fn tag_name(self) -> &'static str {
        Self::TAGS[usize::from(self.0) - 1]
    }
}

impl From<HeadingLevel> for u8 {
    fn from(value: HeadingLevel) -> Self {
        value.0
    }
}

impl core::str::FromStr for HeadingLevel {
    type Err = ParseHeadingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Self::TAGS is only 6 elements"
        )]
        Self::TAGS
            .iter()
            .position(|t| *t == s)
            .map(|index| HeadingLevel(index as u8 + 1))
            .ok_or(ParseHeadingError)
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
pub enum InclusionMode {
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
pub enum TextStyle {
    /// Bold text.
    Bold(TextStylePosition),
    /// Bold and italic text. These are held as a combined style because it is
    /// ambiguous in the tokeniser at the time the input is consumed whether the
    /// balance is `'''text'''''text''` or `''text'''''text'''`.
    BoldItalic(TextStyleHint),
    /// Italic text.
    Italic,
}

/// A directionality hint for a [`TextStyle::BoldItalic`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextStyleHint {
    /// Bold on the left.
    BoldFirst,
    /// Italic on the left.
    ItalicFirst,
    /// No later styles on the line.
    Last,
}

/// The positional attributes of a bold text style. Used for decomposition when
/// balancing quotes. The numeric value is the position priority, with higher
/// numbers being the higher priority when decaying bold to italic to balance an
/// unbalanced line of text styles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextStylePosition {
    /// Any other position.
    Normal = 2,
    /// The text style is immediately after a space followed by a single
    /// non-space character.
    Orphan = 3,
    /// The text style is immediately after a space.
    Space = 1,
}

/// Tries to borrow a text token from `source`.
#[rustfmt::skip]
#[inline]
#[must_use]
pub fn borrow_fast<'a>(source: &'a str, expr: &[Spanned<Token>]) -> Option<&'a str> {
    if let [Spanned { span, node: Token::Text }] = expr {
        Some(&source[span.into_range()])
    } else {
        None
    }
}

/// Tries to borrow a text token from `source` or a generated token from `expr`.
#[rustfmt::skip]
#[inline]
#[must_use]
pub fn borrow_fastest<'a>(source: &'a str, expr: &'a [Spanned<Token>]) -> Option<&'a str> {
    if let [Spanned { node: Token::Generated(s), .. }] = expr {
        Some(s)
    } else {
        borrow_fast(source, expr)
    }
}

/// Escapes the world, marauding, questioning what the hell kind of text format
/// requires this kind of absolute madness in order to work.
///
/// Equivalent to `wfEscapeWikiText`, which is different from
/// `Sanitizer::safeEncodeAttribute`.
///
/// # Panics
///
/// * `config.escape_pattern` does not capture expected values
#[must_use]
pub fn escape_all<'a>(config: &Configuration, text: &'a str) -> Cow<'a, str> {
    const BOUNDARY_CHAR: phf::Map<char, &str> = phf::phf_map! {
        '\t' => "&#9;",
        '\n' => "&#10;",
        '\r' => "&#13;",
        '_' => "&#95;",
        '~' => "&#126;",
    };

    const FIRST_CHAR: phf::Map<char, &str> = phf::phf_map! {
        ' ' => "&#32;",
        '!' => "&#33;",
        '#' => "&#35;",
        '*' => "&#42;",
        '+' => "&#43;",
        '-' => "&#45;",
        ':' => "&#58;",
    };

    // TODO: This should be reconfigured to run in a single pass by creating a
    // very sad regular expression.
    const REPLS: &[(&str, &str)] = &[
        ("\n----", "\n&#45;---"),
        ("\r----", "\r&#45;---"),
        ("~~~", "~~&#126;"),
        ("://", "&#58;//"),
        ("＿", "&#xFF3F;"), // 3 bytes in UTF-8
        ("\n\t", "\n&#9;"),
        ("\r\t", "\r&#9;"),
        ("\n\n", "\n&#10;"),
        ("\n\r", "\n&#13;"),
        ("\r\r", "\r&#13;"),
        ("\n ", "\n&#32;"),
        ("\r ", "\r&#32;"),
        ("\n!", "\n&#33;"),
        ("\r!", "\r&#33;"),
        ("\n#", "\n&#35;"),
        ("\r#", "\r&#35;"),
        ("\n*", "\n&#42;"),
        ("\r*", "\r&#42;"),
        ("\n:", "\n&#58;"),
        ("\r:", "\r&#58;"),
        ("\r\n", "&#13;\n"),
        ("!!", "&#33;!"),
        ("__", "_&#95;"),
        ("\"", "&#34;"),
        ("&", "&#38;"),
        ("'", "&#39;"),
        (";", "&#59;"),
        ("<", "&#60;"),
        ("=", "&#61;"),
        (">", "&#62;"),
        ("[", "&#91;"),
        ("]", "&#93;"),
        ("{", "&#123;"),
        ("|", "&#124;"),
        ("}", "&#125;"),
    ];

    const PATTERN_REPLS: phf::Map<&str, &str> = phf::phf_map! {
        "\t" => "&#9;",
        "\n" => "&#10;",
        "\x0c" => "&#12;",
        "\r" => "&#13;",
        " " => "&#32;",
        ":" => "&#58;",
    };

    let mut text = strtr(text, REPLS);

    if let Some(first) = text.chars().next()
        && let Some(repl) = FIRST_CHAR.get(&first).or_else(|| BOUNDARY_CHAR.get(&first))
    {
        text.to_mut().replace_range(..first.len_utf8(), repl);
    }

    if let Some((index, last)) = text.char_indices().last()
        && let Some(repl) = BOUNDARY_CHAR.get(&last)
    {
        text.to_mut().replace_range(index.., repl);
    }

    if let Some(extras) = &config.escape_pattern {
        text.map(|text| {
            extras.replace_all(text, |capture: &Captures<'_>| {
                let terminator = capture
                    .get(1)
                    .or_else(|| capture.get(2))
                    .expect("at least one capture group");
                let repl = PATTERN_REPLS.get(terminator.as_str()).expect("replacement");
                let prefix = capture.get_match();
                let prefix = &prefix.as_str()[..terminator.start() - prefix.start()];
                format!("{prefix}{repl}")
            })
        })
    } else {
        text
    }
}

/// The strip marker prefix.
pub const MARKER_PREFIX: &str = "\x7f'\"`UNIQ-";

/// The strip marker suffix.
pub const MARKER_SUFFIX: &str = "-QINU`\"'\x7f";

/// Void HTML5 tags.
pub const VOID_TAGS: phf::Set<&str> = phf::phf_set! {
    "area", "base", "br", "col", "embed", "hr", "img",
    "input", "link", "meta", "param", "source",
    "track", "wbr",
};
