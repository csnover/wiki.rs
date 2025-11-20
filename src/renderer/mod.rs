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
//!       functions, it MUST be a text sequence starting with ``\x7f'"`UNIQ-``
//!       and ending with ``-QINU`"'\x7f``. The sequence MUST uniquely identify
//!       this extension tag within the *entire* document. Because strip markers
//!       may be deleted during template expansion, the extension tag function
//!       SHOULD not be invoked until the extension tag is recovered from the
//!       strip marker in the final processing step.
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
//!    root document’s source text before parsing ever began.[^2]
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
//!    5. If the target-part of the expression is a valid and existing template,
//!       the result of expanding the template; otherwise
//!    6. If the target-part of the expression is a valid but non-existing
//!       template, the Wikitext expression `[[:Template:<target>]]`; otherwise
//!    7. The template expression itself, as plain text.
//!
//!    [^2]: Save mode, and therefore the other `subst` rules, are out of scope
//!          for this project.
//!
//! 5. The Wikitext document is now “complete” and can be converted into a
//!    syntax tree and/or emitted as HTML.
//!
//!    Conversions from Wikitext tokens to HTML look like this:
//!
//!    * Template token: Emit as plain text.
//!    * Wikitext heading, link, list, table, language conversion, or magic
//!      link: convert to the corresponding HTML and emit the result.
//!    * Wikitext text style: Accumulate all text styles until an end-of-line
//!      token, then run the balancing algorithm to recover apostrophes, then
//!      emit as HTML. The end-of-line token also implicitly closes any unclosed
//!      text style tags.
//!    * Strip marker or extension tag: emit the result of calling the
//!      corresponding extension tag function. (TODO: It might be the case that
//!      some as-yet unseen extension tag *requires* emitting Wikitext character
//!      strings rather doing its own Wikitext conversions to HTML, in which
//!      case this actually has to occur as a separate step. It is definitely
//!      the case that extension tags are allowed to emit non-whitelisted HTML,
//!      so it can’t be the case that they must *always* emit valid Wikitext.)
//!    * Whitelisted HTML tag: parse using the special Wikitext HTML attribute
//!      error correction algorithm[^3] and emit as HTML.
//!    * A valid HTML entity[^4] other than `&amp;` `&lt;` `&gt;` and `&quot;`:
//!      Decode the entity and emit the decoded value.
//!    * A character `['<'|'>'|'&'|'"']`: entity-encode the character and emit
//!      the entity-encoded value.
//!
//!    [^3]: In Wikitext, `/>` and `>` are treated as terminators for any quoted
//!          attribute value, which is not true in HTML5.
//!
//!    [^4]: Wikitext uses the standard HTML5 list of entities, plus two special
//!          entities `"&רלמ;"` and `"&رلم;"` which decode to RLM (U+200F).
//! </div>

use crate::{
    LoadMode,
    db::{Article, Database},
    lru_limiter::ByMemoryUsage,
    lua::VmCacheEntry,
    php::DateTime,
    renderer::lru_limiter::OutputSizeCalculator,
    wikitext::{LineCol, Output, Parser},
};
use axum::http::Uri;
use core::fmt;
pub use expand_templates::{ExpandMode, ExpandTemplates};
pub use manager::{In, RenderManager as Manager, RenderOutput};
pub use parser_fns::call_parser_fn;
use piccolo::Lua;
use schnellru::LruMap;
pub use stack::{Kv, StackFrame};
use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration};
pub use surrogate::Surrogate;
use tags::LinkKind;
pub use template::call_template;

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
pub enum Error {
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

    /// Too many template calls.
    #[error("template stack overflow: {0}")]
    StackOverflow(String),

    /// A [`StripMarker`](crate::wikitext::Token::StripMarker) was encountered
    /// without a corresponding entry.
    #[error("invalid strip marker {0}")]
    StripMarker(usize),

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

/// The standard result type used by all fallible renderer functions.
pub type Result<T = (), E = Error> = core::result::Result<T, E>;

/// A unique scalar identifier for [`Article`]s.
type ArticleId = u64;

/// Global variables which are used for the entire lifetime of a renderer
/// thread.
pub struct Statics {
    /// The “current” time, according to the article database.
    pub base_time: DateTime,
    /// The server’s base URI.
    pub base_uri: Uri,
    /// The article database.
    pub db: Arc<Database<'static>>,
    /// The parser.
    pub parser: Parser<'static>,
    /// Parsed template cache.
    template_cache: LruMap<ArticleId, Rc<Output>, ByMemoryUsage<OutputSizeCalculator>>,
    /// The Lua interpreter.
    pub vm: Lua,
    /// VM module cache.
    pub vm_cache: LruMap<ArticleId, VmCacheEntry, ByMemoryUsage<VmCacheEntry>>,
}

/// A strip marker.
enum StripMarker {
    /// A strip marker containing block-level elements.
    Block(String),
    /// A strip marker containing only phrasing content.
    Inline(String),
}

/// Renderer state that is shared across stack frames.
pub struct State<'s> {
    /// Article data.
    pub globals: ArticleState,
    /// The page load strategy.
    pub load_mode: LoadMode,
    /// Thread static global variables.
    pub statics: &'s mut Statics,
    /// Stripped extension tag substitutions.
    // TODO: Store as Rc or something so these do not need to be cloned? Which
    // is faster?
    strip_markers: Vec<StripMarker>,
    /// Page performance timing data.
    timing: HashMap<String, (usize, Duration)>,
}

/// A convenience trait alias combining [`fmt::Write`] and [`Surrogate`].
pub trait WriteSurrogate: fmt::Write + Surrogate<Error> {}
impl<T> WriteSurrogate for T where T: fmt::Write + Surrogate<Error> {}

/// Shared article data.
#[derive(Debug, Default)]
pub struct ArticleState {
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
            article = db.get(target)?;
        } else {
            break;
        }
    }

    Ok(article)
}
