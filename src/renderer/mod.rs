//! Article rendering types and functions.
//!
//! Correct parsing of Wikitext documents requires out-of-band configuration
//! data:
//!
//! * Annotation XML tag names
//! * Extension XML tag names
//! * Double-underscore magic word names
//! * Redirect keyword names
//! * Language conversion enabled flag
//! * Supported URI schemes
//! * Registered variable names and case sensitivities
//! * Registered parser function names
//! * Namespace names and case rule
//! * Magic links flags
//! * Link trail regular expression
//! * Valid title regular expression character class
//!
//! When processing a Wikitext document, the smallest atom is a Wikitext token,
//! but that the smallest atom that a template can produce is a *character*.
//! This means that a Wikitext document can only be parsed correctly by an
//! algorithm that operates as-if this sequence of steps is run in order
//! (probably, this description is based mostly on black box analysis with some
//! review of MediaWiki source code):
//!
//! <style>.wiki-rs-step-list {
//!   ol ol { list-style-type: lower-alpha; }
//!   ol ol ol { list-style-type: lower-roman; }
//! }
//! </style>
//! <div class="wiki-rs-step-list">
//!
//! 1. Process annotation XML tags:
//!
//!    1. If an annotation start tag is not self-closing and has no balancing
//!       end tag, treat it as plain text.
//!    2. TODO: Expand on how to do this, if it is ever necessary.
//!
//! 2. Process extension XML tags:
//!
//!    1. If an extension start tag is not self-closing and has no balancing end
//!       tag, treat it as plain text.
//!    2. Record and store the original byte ranges of the extension tag. Some
//!       extension tag functions require this data.
//!    3. Extract and store the body of the tag, if any, as plain text. Some
//!       extension tag functions require this data.
//!    4. Replace the extension tag in the source text with a “strip marker”.
//!       Because the strip marker is exposed to Lua scripts and parser
//!       functions, it MUST match this format exactly:
//!
//!       1. The string ``\x7f'"`UNIQ-``;
//!       2. The tag name of the extension tag;
//!       3. The character `-`;
//!       4. A lowercase hexadecimal ordinal which, in combination with the tag
//!          name, is unique within the *entire* document;
//!       5. The string ``-QINU`"'\x7f``.
//!
//!       Because strip markers may be deleted during template expansion, the
//!       extension tag function SHOULD not be invoked until the extension tag
//!       is recovered from the strip marker in the final processing step.
//!
//!    If the tag name ends with an inclusion control pseudo-XML tag, the tag
//!    MUST NOT be treated as an extension tag, but instead MUST be treated as
//!    an HTML tag if the extension tag name matches a legal HTML tag, or as
//!    plain text otherwise (this is the “`<pre>` hack”).
//!
//! 3. Process inclusion control pseudo-XML tags (`<noinclude>`,
//!    `<onlyinclude>`, and `<includeonly>`):
//!
//!    1. Scan the entire document for any `<onlyinclude>` tag not inside a
//!       `<nowiki>`[^1] tag. If found, treat all content outside of
//!       `<onlyinclude>` tags as-if it were wrapped by `<noinclude>`.
//!    2. For each start or end inclusion control tag:
//!
//!       If the tag is inside a `<nowiki>`[^1] tag or it is an unbalanced
//!       `</includeonly>` tag, treat it as plain text. Otherwise, delete the
//!       tag.
//!
//!       If the tag is a start tag, also perform these steps:
//!
//!       1. If there is no explicit end tag, and the tag is not self-closing,
//!          treat the end of the file as the end tag.
//!       2. If the tag does not match the current processing mode, delete the
//!          text between the start and the end tags.
//!
//!    [^1]: Because `<nowiki>` is an extension tag, this exclusion should
//!          happen implicitly by running step 2 first.
//!
//! 4. Recursively expand template expressions:
//!
//!    Conceptually, the result of a template expansion should be as-if the
//!    plain text of the *fully expanded* template already existed in the
//!    root document’s source text before parsing ever began. Note that there
//!    are special whitespace rules for template expansions; a naïve approach
//!    which simply concatenates the result of a template expansion will result
//!    in an incorrect final document.
//!
//!    If the expression is a template parameter, interpolate into the source
//!    text:
//!
//!       1. The expansion of the matching argument from the parent; otherwise
//!       2. The expansion of the default value from the parameter; otherwise
//!       3. The template parameter expression itself, as plain text.
//!
//!    If the expression is a template, interpolate into the source text:
//!
//!    1. If the expression is prefixed by `subst:` or `safesubst:`, and the
//!       parser is not in save mode[^2], remove the prefix from the expression;
//!       then
//!    2. If the expression has no arguments, and it matches a variable name,
//!       the variable’s value; otherwise
//!    3. TODO: Change the parser’s configuration settings based on special
//!       symbols `msgnw`, `msg`, and `raw.`; then
//!    4. If the target-part of the expression contains a `:`, and the part
//!       before the `:` matches a parser function, and calling the parser
//!       function succeeds, the result of the parser function; otherwise
//!    5. If the target-part of the expression is a subpage title expression,
//!       convert it to a fully qualified title, then;
//!    6. If the target-part of the expression is a valid and existing template,
//!       the result of expanding the template; otherwise
//!    7. If the target-part of the expression is a valid but non-existing
//!       template according to the configurable list of allowed template target
//!       characters, the Wikitext expression `[[:Template:<target>]]`;
//!       otherwise
//!    8. The template expression itself, as plain text.
//!
//!    If the template expression was not immediately following a new line or
//!    the start of the file, and the result of the template expansion starts
//!    with `"{|"`, `":"`, `";"`, `"#"`, or `"*"`, prefix the result value with
//!    `"\n"`.
//!
//!    [^2]: Save mode, and therefore the other `subst` rules, are out of scope
//!          for this project.
//!
//! 6. Replace any strip markers in the output string with the stored original
//!    extension tag XML.
//!
//! 7. Parse the output string and generate a DOM:
//!
//!    When emitting tags:
//!
//!    TODO.
//!
//!    When emitting text:
//!
//!    1. For a valid HTML entity[^4] other than `&amp;`, `&lt;`, `&gt;`, or
//!       `&quot;`, decode the entity and emit the decoded value; otherwise
//!    2. For a character `['<'|'>'|'&'|'"']`, entity-encode the character and
//!       emit the entity-encoded value; otherwise
//!    3. For a character `\n`, emit nothing and run the apostrophe balancing
//!       algorithm and block wrapping algorithms; otherwise
//!    4. Emit the character.
//!
//!    When parsing attributes:
//!
//!    * Value parsing uses a non-standard parse where `>` or `/>` are
//!      terminators for attribute values, even if they are inside a quoted-text
//!      part. This violates the XML and HTML standards.
//!    * If the attribute name is not whitelisted for the tag where it appears,
//!      ignore the whole attribute.
//!    * If the attribute name is `style`, decode CSS escapes in the value, then
//!      sanitise the value.
//!    * For other attribute values, sanitise the value according to unspecified
//!      rules.
//!
//!    When parsing Wikitext table attributes:
//!
//!    * If the attribute name is actually a whitelisted HTML tag, discard the
//!      `<`, tag name, and `>`, and act as-if only the tag’s attributes were
//!      present in the source.
//!
//!    When running the apostrophe balancing algorithm:
//!
//!    TODO
//!
//!    When running the block wrapping algorithm:
//!
//!    TODO
//!
//!    For each Wikitext expression encountered during parsing:
//!
//!    * Template expression: Emit as plain text.
//!
//!    * Extension tag expression: Invoke the extension tag function and emit
//!      the result. The output of the extension tag function is
//!      implementation-specific, but will typically be a well-formed HTML
//!      fragment which is injected at the position where the extension tag is
//!      invoked. The output of an extension tag is opaque to the apostrophe
//!      balancing algorithm and the block wrapping algorithm (probably?).
//!
//!    * Wikitext internal link expression:
//!
//!      1. If the target’s namespace is of type Category, emit nothing and
//!         delete any run of whitespace which preceded the link and matches
//!         the regular expression `\n\s*$`; otherwise
//!      2. If the target’s namespace is of type File, treat the link content as
//!         a list of media parameters and emit HTML appropriate for displaying
//!         the media; otherwise
//!      3. Build the link content:
//!         1. If the link expression has a content-part, use it as the content;
//!            otherwise
//!         2. Use the target-part as the content, trimming any leading `':'`;
//!            then
//!         3. If the link expression is suffixed by text which matches the
//!            link-trail regular expression, move that text into the link
//!            content;
//!         4. Run the apostrophe balancing algorithm on the content.
//!      4. Build the link target:
//!         1. Resolve the target according to the target-part using a default
//!            namespace of type Main;
//!         2. If the link target is the current page, and the target URI has no
//!            fragment-part, emit the content only instead of creating a
//!            hyperlink.
//!      5. Emit the link as HTML.
//!
//!    * Wikilink external link expression:
//!
//!      1. If the target is not a valid URI with a whitelisted protocol, emit
//!         as plain text; otherwise
//!      2. Emit the link as HTML.
//!
//!    * Table start expression:
//!
//!      1. Collect the attributes by running the Wikitext table attribute
//!         algorithm;
//!      2. Emit an HTML `<table>` tag using the sanitised attributes;
//!      2. Increase the Wikitext table count by 1;
//!      3. Increase the HTML table count by 1.
//!
//!    * HTML table start tag:
//!
//!      1. Emit the tag;
//!      2. Increase the HTML table count by 1.
//!
//!    * Table end expression:
//!
//!      1. If the Wikitext table count is zero, emit as plain text;
//!         otherwise
//!      2. If the HTML table count is zero, decrease the Wikitext table
//!         count by 1 and emit nothing; otherwise
//!      3. Decrease both the Wikitext table count and HTML table count by 1,
//!         then emit `</table>`.
//!
//!    * HTML table end tag:
//!
//!      1. If the HTML table count is zero, emit nothing; otherwise
//!      2. Decrease the HTML table count by 1, run the inner element closing
//!         algorithm, then emit `</table>`.
//!
//!    * Table caption, row, heading, or cell expression:
//!
//!      1. If the Wikitext table count is zero, emit as plain text; otherwise
//!      2. If the HTML table count is zero, emit nothing; otherwise
//!      3. If the expression is a table row expression, and the next expression
//!         is also a table row expression, emit nothing; otherwise
//!      4. Collect the attributes by running the Wikitext table attribute
//!         algorithm;
//!      5. Emit an appropriate HTML tag (`<caption>`, `<tr>`, etc.) using the
//!         sanitised attributes.
//!
//!    * Wikitext list:
//!
//!      1. If the list item starts with one or more ':', and the first
//!         expression in the list item is a table start expression:
//!
//!         1. Close any previous list;
//!         2. Open a new list;
//!         3. Continue emitting until a table end expression;
//!         4. Close the definition list; otherwise
//!
//!      2. For a list item starting with a sequence of one or more '*' '#' ';'
//!         or ':' characters (the “bullet list”):
//!
//!         1. Calculate the longest common bullet list (LCD) between the
//!            current item and the previous item;
//!         2. For each bullet after the LCD in the previous item, from right
//!            to left, close the list;
//!         3. For each bullet after the LCD in the next item, from left to
//!            right, open a new list;
//!         4. Open a new list item.
//!
//!      3. Continue emitting expressions until a newline or end of file;
//!      4. Run the apostrophe balancing algorithm;
//!      5. Run the inner element closing algorithm;
//!      6. Close the list item;
//!      7. If the next expression is not a list item, for each bullet in the
//!         current item, from right to left, close the list.
//!
//!    * Wikitext heading, language conversion, or magic link expressions:
//!      Emit an appropriate HTML tag.
//!
//!    * Wikitext text style expressions: Add the output position to the
//!      accumulator for the apostrophe balancing algorithm.
//!
//!    * Whitelisted HTML tag expressions: Parse using the special Wikitext HTML
//!      attribute error correction algorithm[^3] and emit as HTML.
//!
//!    * Text expressions: Emit as plain text.
//!
//! 8. Run the paragraph wrapping algorithm on the resulting DOM. TODO: Document
//!    this additional insane thing whenever procrastination strikes again.
//!
//!    [^3]: In Wikitext, `/>` and `>` are treated as terminators for any quoted
//!          attribute value, which is not true in HTML5.
//!
//!    [^4]: Wikitext uses the standard HTML5 list of entities, plus two special
//!          entities `"&רלמ;"` and `"&رلم;"` which decode to RLM (U+200F).
//! </div>

use crate::{
    Limits, LoadMode,
    db::{Article, Database},
    lru_limiter::ByMemoryUsage,
    lua::VmCacheEntry,
    php::DateTime,
    title::Title,
    wikitext::{LineCol, MARKER_PREFIX, MARKER_SUFFIX, Output, Parser},
};
use axum::http::Uri;
use core::fmt;
pub(crate) use expand_templates::{ExpandMode, ExpandTemplates};
pub(crate) use manager::{Command, In, RenderManager as Manager, RenderOutput};
use memchr::memmem;
pub(crate) use parser_fns::call_parser_fn;
use piccolo::Lua;
use schnellru::LruMap;
pub(crate) use stack::{CachedValue, Kv, StackFrame};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, LazyLock, PoisonError, RwLock},
    time::Duration,
};
pub(crate) use surrogate::Surrogate;
use tags::LinkKind;
pub(crate) use template::call_template;

mod document;
mod emitters;
mod expand_templates;
mod extension_tags;
mod globals;
mod image;
mod lru_limiter;
mod manager;
mod parser_fns;
mod stack;
mod surrogate;
mod tags;
mod template;
mod trim;

/// An article rendering error.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// A database call failed.
    #[error("db error: {0}")]
    Database(#[from] crate::db::Error),

    /// An arithmetic expression evaluation error.
    #[error("eval error: {0}")]
    Expr(#[from] crate::expr::Error),

    /// An extension tag error.
    #[error(transparent)]
    Extension(Box<dyn std::error::Error + Send + Sync + 'static>),

    /// A write to a buffer failed.
    #[error("fmt error: {0}")]
    Fmt(#[from] fmt::Error),

    /// Some Lua host code raised an error.
    #[error("{0:#}")]
    Lua(#[from] piccolo::ExternError),

    /// An `#invoke` call was missing the required function argument.
    #[error("script error: you must specify a function to call")]
    MissingFunctionName,

    /// A backtraced Lua module error.
    #[error("{err}\n  at '{name}'|{fn_name}")]
    Module {
        /// The title of the module.
        name: String,
        /// The name of the function.
        fn_name: String,
        /// The error.
        #[source]
        err: Box<Error>,
    },

    /// A backtraced template error.
    #[error("{err}\n  at '{frame}':{start}")]
    Node {
        /// The title of the template.
        frame: String,
        /// The line and column in the template where the error occurred.
        start: LineCol,
        /// The error.
        #[source]
        err: Box<Self>,
    },

    /// An error occurred parsing a floating point number.
    #[error(transparent)]
    ParseFloat(#[from] core::num::ParseFloatError),

    /// An error occurred while parsing a Wikitext string.
    #[error(transparent)]
    Peg(#[from] crate::wikitext::Error),

    /// An [`RwLock`] guard was poisoned.
    #[error("poisoned lock")]
    Poison,

    /// Too many template calls.
    #[error("template stack overflow: {0}")]
    StackOverflow(String),

    /// A [`StripMarker`](crate::wikitext::Token::StripMarker) was encountered
    /// without a corresponding entry.
    #[error("invalid strip marker {0}")]
    StripMarker(String),

    /// A template called back into itself.
    ///
    /// Note that loop detection does not—and must not—apply in cases where the
    /// loop is back to the root page, because this is used by (at least) all
    /// pages which use 'Template:Documentation' to demonstrate the output of
    /// a template from its own page.
    #[error("template loop detected: {0}")]
    TemplateRecursion(String),

    /// An error occurred parsing or formatting a date.
    #[error(transparent)]
    Time(#[from] crate::php::DateTimeError),
}

impl<T> From<PoisonError<T>> for Error {
    fn from(_: PoisonError<T>) -> Self {
        Self::Poison
    }
}

/// The standard result type used by all fallible renderer functions.
pub type Result<T = (), E = Error> = core::result::Result<T, E>;

/// A unique scalar identifier for [`Article`]s.
type ArticleId = u64;

/// A template cache.
type TemplateCache = Arc<RwLock<LruMap<ArticleId, Arc<Output>, ByMemoryUsage>>>;

/// Global variables which are used for the entire lifetime of a renderer
/// thread.
pub(crate) struct Statics {
    /// The “current” time, according to the article database.
    pub base_time: DateTime,
    /// The server’s base URI.
    pub base_uri: Uri,
    /// The article database.
    pub db: Arc<Database<'static>>,
    /// Time and memory limits.
    pub limits: Limits,
    /// The parser.
    pub parser: Parser<'static>,
    /// Parsed template cache.
    template_cache: TemplateCache,
    /// The Lua interpreter.
    pub vm: Lua,
    /// VM module cache.
    pub vm_cache: LruMap<ArticleId, VmCacheEntry, schnellru::UnlimitedCompact>,
}

/// A list of stripped extension tags.
#[derive(Default)]
pub(crate) struct StripMarkers(Vec<StripMarker>);

impl StripMarkers {
    /// Returns the index of the strip marker from a strip marker key.
    ///
    /// The strip marker key must be formatted in this specific way because it
    /// is exposed to modules, and of course some of them like 'Module:Infobox'
    /// rely on this implementation detail.
    fn key_index(key: &str) -> usize {
        let (_, index) = key.rsplit_once('-').expect("hyphenated marker key");
        usize::from_str_radix(index, 16).expect("hexadecimal index")
    }

    /// Gets the strip marker with the given key.
    fn get(&self, key: &str) -> Option<&StripMarker> {
        self.0.get(Self::key_index(key))
    }

    /// Invokes callback `f` for each strip marker in the given text.
    ///
    /// The callback should return `Some(string)` if it wants to replace the
    /// marker, or `None` if it wants the marker to be kept as-is in the text.
    #[inline]
    pub fn for_each_marker<'a, F>(&self, body: &'a str, mut f: F) -> Cow<'a, str>
    where
        for<'m> F: FnMut(&'m StripMarker) -> Option<Cow<'m, str>>,
    {
        Self::for_each_marker_key(body, |key| f(&self.0[Self::key_index(key)]))
    }

    /// Invokes callback `f` for each run of text delimited by split markers.
    ///
    /// The callback should return `Some(string)` if it wants to replace the
    /// text run, or `None` if it wants the text to be kept as-is.
    pub fn for_each_non_marker<'a, F>(body: &'a str, mut f: F) -> Cow<'a, str>
    where
        F: FnMut(&'a str) -> Option<Cow<'a, str>>,
    {
        let mut out = String::new();
        let mut flushed = 0;
        let mut cursor = 0;

        while cursor != body.len() {
            let end = FIND_PREFIX
                .find(&body.as_bytes()[cursor..])
                .map_or(body.len(), |pos| cursor + pos);
            if let Some(replacement) = f(&body[cursor..end]) {
                out += &body[flushed..cursor];
                out += &replacement;
                flushed = end;
            }

            cursor = FIND_SUFFIX
                .find(&body.as_bytes()[end..])
                .map_or(body.len(), |pos| end + pos + MARKER_SUFFIX.len());
        }

        if flushed == 0 {
            Cow::Borrowed(body)
        } else {
            out += &body[flushed..];
            Cow::Owned(out)
        }
    }

    /// Invokes callback `f` for each strip marker index in the given text.
    ///
    /// The callback should return `Some(string)` if it wants to replace the
    /// marker, or `None` if it wants the marker to be kept as-is in the text.
    fn for_each_marker_key<'a, F>(body: &str, mut f: F) -> Cow<'_, str>
    where
        F: FnMut(&str) -> Option<Cow<'a, str>>,
    {
        let mut out = String::new();
        let mut flushed = 0;
        let mut cursor = 0;
        while let Some(before) = FIND_PREFIX.find(&body.as_bytes()[cursor..])
            && let before = cursor + before
            && let start = before + MARKER_PREFIX.len()
            && let Some(len) = FIND_SUFFIX.find(&body.as_bytes()[start..])
        {
            let end = start + len;
            let key = &body[start..end];
            cursor = end + MARKER_SUFFIX.len();
            if let Some(replacement) = f(key) {
                out += &body[flushed..before];
                out += &replacement;
                flushed = cursor;
            }
        }

        if flushed == 0 {
            Cow::Borrowed(body)
        } else {
            out += &body[cursor..];
            Cow::Owned(out)
        }
    }

    /// Removes all strip markers from the given text.
    #[inline]
    pub fn kill(body: &str) -> Cow<'_, str> {
        Self::for_each_marker_key(body, |_| Some("".into()))
    }

    /// Pushes a new strip marker to the list, emitting the marker to the given
    /// `out` string.
    fn push<W: fmt::Write + ?Sized>(&mut self, out: &mut W, tag_name: &str, marker: StripMarker) {
        let _ = write!(
            out,
            "{MARKER_PREFIX}{tag_name}-{:x}{MARKER_SUFFIX}",
            self.0.len()
        );
        self.0.push(marker);
    }

    /// Recursively replaces all strip markers in the given string with their
    /// original contents.
    #[inline]
    fn unstrip<'a>(&self, body: &'a str) -> Cow<'a, str> {
        self.for_each_marker(body, |marker| Some(Cow::Borrowed(marker)))
    }
}

/// A precomputed finder for [`MARKER_PREFIX`].
static FIND_PREFIX: LazyLock<memmem::Finder<'static>> =
    LazyLock::new(|| memmem::Finder::new(MARKER_PREFIX.as_bytes()));

/// A precomputed finder for [`MARKER_SUFFIX`].
static FIND_SUFFIX: LazyLock<memmem::Finder<'static>> =
    LazyLock::new(|| memmem::Finder::new(MARKER_SUFFIX.as_bytes()));

/// A strip marker.
#[derive(Debug)]
pub(crate) enum StripMarker {
    /// A strip marker containing block-level elements.
    Block(String),
    /// A strip marker containing only phrasing content.
    Inline(String),
    /// A strip marker containing only phrasing content from a `<nowiki>` tag.
    NoWiki(String),
    /// A strip marker containing a wiki.rs-specific template source start
    /// marker.
    WikiRsSourceStart(String),
    /// A strip marker containing a wiki.rs-specific template source end marker.
    WikiRsSourceEnd(String),
}

impl fmt::Display for StripMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl core::ops::Deref for StripMarker {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            StripMarker::WikiRsSourceStart(_) | StripMarker::WikiRsSourceEnd(_) => "",
            StripMarker::Block(s) | StripMarker::Inline(s) | StripMarker::NoWiki(s) => s,
        }
    }
}

/// Renderer state that is shared across stack frames.
pub(crate) struct State<'s> {
    /// Article data.
    pub globals: ArticleState,
    /// The page load strategy.
    pub load_mode: LoadMode,
    /// Thread static global variables.
    pub statics: &'s mut Statics,
    /// Stripped extension tag substitutions.
    pub strip_markers: StripMarkers,
    /// Page performance timing data.
    timing: HashMap<String, (usize, Duration)>,
}

/// A convenience trait alias combining [`fmt::Write`] and [`Surrogate`].
pub trait WriteSurrogate: fmt::Write + Surrogate<Error> {}
impl<T> WriteSurrogate for T where T: fmt::Write + Surrogate<Error> {}

/// Shared article data.
#[derive(Debug, Default)]
pub(crate) struct ArticleState {
    /// Collected categories to append to the footer of the page.
    categories: globals::Categories,
    /// The last ordinal used by an unlabelled external link.
    external_link_ordinal: u32,
    /// Indicator icons for the `<indicator>` extension tag.
    indicators: globals::Indicators,
    /// Table of contents.
    outline: globals::Outline,
    /// Collected references for the `<ref>` and `<references>` extension tags.
    references: extension_tags::References,
    /// Labelled section transclusion sections.
    sections: extension_tags::LabelledSections,
    /// Collected CSS for the `<templatestyles>` extension tag.
    styles: extension_tags::Styles,
    /// Sometimes settable magic variables, e.g. `{{SHORTDESC}}`.
    pub variables: HashMap<String, String>,
}

// TODO: This should really just resolve the redirects and then do the work, but
// borrowck is being unbearable today and this is a toy project so who cares
// TODO: This should be part of Database
pub fn resolve_redirects(
    db: &Database<'static>,
    mut article: Arc<Article>,
) -> Result<Arc<Article>, Error> {
    // “Loop to fetch the article, with up to 2 redirects”
    for _ in 0..2 {
        if let Some(target) = &article.redirect {
            // log::trace!("Redirection #{} to {target}", attempt + 1);
            article = db.get(&Title::new(target, None))?;
        } else {
            break;
        }
    }

    Ok(article)
}

/// Writes a run of text to the given output as entity-encoded HTML, converting
/// wretched typewriter quote marks to beautiful works of fine typographical
/// art. We are not savages here today.
fn text_run<W: fmt::Write + ?Sized>(
    out: &mut W,
    mut prev: char,
    text: &str,
    in_code: bool,
) -> Result<char> {
    fn is_break(prev: char, next: Option<char>) -> bool {
        use unicode_general_category::{
            GeneralCategory::{DashPunctuation, InitialPunctuation, OpenPunctuation},
            get_general_category,
        };
        prev.is_whitespace()
            || (matches!(
                get_general_category(prev),
                DashPunctuation | OpenPunctuation | InitialPunctuation
            ) && !next.is_some_and(char::is_whitespace))
    }

    let mut chars = text.chars().peekable();
    let mut dot_count = 0;
    while let Some(mut c) = chars.next() {
        if c == '.' && !in_code && dot_count != 3 {
            dot_count += 1;
            continue;
        }

        if !in_code && dot_count == 3 {
            out.write_char('…')?;
            prev = '…';
            dot_count = 0;
        }

        for _ in 0..dot_count {
            out.write_char('.')?;
            prev = '.';
        }

        match c {
            '"' if !in_code => {
                out.write_char(if is_break(prev, chars.peek().copied()) {
                    c = '“';
                    c
                } else {
                    c = '”';
                    c
                })?;
            }
            '\'' if !in_code => {
                out.write_char(if is_break(prev, chars.peek().copied()) {
                    c = '‘';
                    c
                } else {
                    c = '’';
                    c
                })?;
            }
            '<' => write!(out, "&lt;")?,
            '>' => write!(out, "&gt;")?,
            '&' => write!(out, "&amp;")?,
            c => out.write_char(c)?,
        }
        prev = c;
        dot_count = 0;
    }

    if dot_count == 3 {
        out.write_char('…')?;
        prev = '…';
    } else {
        for _ in 0..dot_count {
            out.write_char('.')?;
            prev = '.';
        }
    }

    Ok(prev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markers() {
        let text = format!(
            "0123{MARKER_PREFIX}a-0{MARKER_SUFFIX}{MARKER_PREFIX}a-1{MARKER_SUFFIX}abcd{MARKER_PREFIX}b-a{MARKER_SUFFIX}4567"
        );
        let result = StripMarkers::for_each_marker_key(&text, |key| {
            let index = StripMarkers::key_index(key);
            if index == 0 {
                Some(Cow::Borrowed("?"))
            } else if index == 10 {
                Some(Cow::Borrowed("!"))
            } else if index == 1 {
                None
            } else {
                panic!("bogus index {index:?}");
            }
        });
        assert_eq!(
            result,
            Cow::Owned::<str>(format!("0123?{MARKER_PREFIX}a-1{MARKER_SUFFIX}abcd!4567"))
        );
    }

    #[test]
    fn test_strip_non_markers() {
        let text = format!(
            "0123{MARKER_PREFIX}a-0{MARKER_SUFFIX}{MARKER_PREFIX}a-1{MARKER_SUFFIX}abcd{MARKER_PREFIX}b-a{MARKER_SUFFIX}4567"
        );
        let result = StripMarkers::for_each_non_marker(&text, |text| {
            if text == "0123" {
                Some(Cow::Borrowed("?"))
            } else if text == "abcd" {
                Some(Cow::Borrowed("!"))
            } else if text == "4567" {
                None
            } else if text.is_empty() {
                Some(Cow::Borrowed("."))
            } else {
                panic!("bogus text {text:?}");
            }
        });
        assert_eq!(
            result,
            Cow::Owned::<str>(format!(
                "?{MARKER_PREFIX}a-0{MARKER_SUFFIX}.{MARKER_PREFIX}a-1{MARKER_SUFFIX}!{MARKER_PREFIX}b-a{MARKER_SUFFIX}4567"
            ))
        );
    }

    #[test]
    fn test_strip_non_markers_end() {
        let text =
            format!("0123{MARKER_PREFIX}a-0{MARKER_SUFFIX}{MARKER_PREFIX}a-1{MARKER_SUFFIX}4567");
        let result = StripMarkers::for_each_non_marker(&text, |text| {
            if text == "0123" || text.is_empty() {
                None
            } else if text == "4567" {
                Some(Cow::Borrowed("!"))
            } else {
                panic!("bogus text {text:?}");
            }
        });
        assert_eq!(
            result,
            Cow::Owned::<str>(format!(
                "0123{MARKER_PREFIX}a-0{MARKER_SUFFIX}{MARKER_PREFIX}a-1{MARKER_SUFFIX}!"
            ))
        );
    }
}
