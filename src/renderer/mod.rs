//! Article rendering types and functions.
//!
//! Wikitext can only be parsed correctly by an algorithm that operates as-if
//! this sequence of steps is run in order (probably, this description is based
//! mostly on black box analysis with some review of MediaWiki source code):
//!
//! 1. Preprocess annotation tags. Annotation tags which are not balanced are
//!    treated as plain text. (TODO: Expand on how to do this, if it is ever
//!    necessary.)
//!
//! 2. Preprocess extension tags by extracting each tag and its body (if any) as
//!    raw text and inserting a “strip marker” in its place in the source text.
//!    Unbalanced extension tags shall be treated as plain text.
//!
//!    ※ A strip marker is a character sequence that is unlikely to appear in
//!    a Wikitext document which can be used to recover the original content by
//!    scanning the source text again later. Strip markers are visible to Lua
//!    scripts and parser functions they may be stripped by those things, so the
//!    actual expansion of an extension tag shall not occur until later.
//!
//!    The list of possible extension tags is installation-specific.
//!
//! 3. Process inclusion control tags (`<noinclude>`, etc.) by this sequence of
//!    steps:
//!
//!    1. Scan the entire document for any `<onlyinclude>` not inside a
//!       `<nowiki>` tag. If found, treat all content outside of `<onlyinclude>`
//!       as-if it were wrapped in `<noinclude>`.
//!    2. For each inclusion control tag not inside a `<nowiki>` tag, delete the
//!       tag. If the tag does not match the current processing mode, and the
//!       tag has a balancing close tag, also delete all the text between the
//!       opening and closing tag.
//!
//!    Unbalanced `</onlyinclude>` tags are treated as plain text; all other
//!    unbalanced close tags are treated as-if they were written as self-closing
//!    tags. Unbalanced open tags are treated as-if they were closed at the end
//!    of the file.
//!
//!    Inclusion control tags can cut across Wikitext expressions so it is not
//!    possible to convert a Wikitext document into tree of Wikitext expressions
//!    with inclusion control tags as branch nodes.
//!
//!    After this step, any string that looks like an inclusion control tag
//!    shall be deleted(?).
//!
//! 4. Template expressions are expanded recursively. Steps 1–3 are performed on
//!    the template sp, then the result of the template expansion is
//!    interpolated into the source document as-if the plain text of the
//!    expanded template had existed at that position in the source text
//!    before parsing ever began.
//!
//!    As with inclusion control tags, templates can cut across Wikitext
//!    expressions (this happens frequently with tables), so it is not possible
//!    to convert a Wikitext document into a correct tree if templates are not
//!    expanded.
//!
//!    A template expression with a valid but non-existent target shall expand
//!    into the Wikitext expression `[[:Template:Name]]`. An invalid template
//!    expression shall be treated as plain text. A template parameter
//!    expression with no matching argument and no default value shall be
//!    treated as plain text.
//!
//!    After this step, any string that looks like a template expression shall
//!    be treated as plain text.
//!
//! 5. Scan the complete preprocessed source text for any strip markers or
//!    extension tags. Interpolated the output of those extension tags into the
//!    source text using the same as-if rule used for template expansions.
//!
//!    After this step, any string that looks like an strip marker or extension
//!    tag shall be treated as plain text(?).
//!
//! 6. The Wikitext document is now finally “complete” and can be converted into
//!    a tree. All other Wikitext expressions are processed at this step.
//!
//!    HTML entities shall be decoded to UTF-8, then the HTML control characters
//!    `['<'| '>'|'&'|'"']` shall be entity-encoded, unless the character forms
//!    part of a syntactically valid HTML5 tag and the tag name is in the
//!    allowlist, in which case the control character shall be emitted as-is.
//!
//! The theory of operation of *this* renderer is to consider that the final
//! output stage of a Wikitext renderer should operate as-if there were never
//! any templates at all, and so tokens generated within template expansions can
//! simply be sent directly up to the root as they are produced:
//!
//! ```text
//!                 ┌───────┬────────┬─────┐
//!                 │ text  │ entity │ ... │
//!         ┌───────┼╌╌╌↓╌╌╌┼╌╌╌╌↓╌╌╌┼╌╌↓╌╌┤
//!         │ <tag> │   ↓ {{ template }}↓  │
//! ┌───────┼╌╌╌↓╌╌╌┼╌╌╌↓╌╌╌┼╌╌╌╌↓╌╌╌┼╌╌↓╌╌┼──────┬────────┬─────┐
//! │ <tag> │   ↓   ┆   ↓ {{ template }}↓  │ text │ </tag> │ ... │
//! └───────┴───────┴───────┴────↓───┴─────┴──────┴────────┴─────┘
//!                          Document
//! ```
//!
//! The obvious and fundamental flaw in this approach is that it expects that
//! the smallest atom is a token, whereas in templates the smallest atom is
//! actually a character. This means that templates sometimes need to accumulate
//! into a string before they can be tokenised correctly. The assumption is that
//! because Parsoid uses this same kind of model that no Wikitext will be
//! totally broken by this approach (though, given that Parsoid is *still* under
//! active development, maybe some questioning of the soundness of this line of
//! thought is warranted).
//!
//! The most sound approach would be to expand templates while the PEG runs,
//! replacing the templates in the original source text with the output of the
//! expansion as the parser encounters them. The major downsides to *that*
//! approach are that it would cause the same template source to be parsed many
//! times instead of once (though having to re-parse the *result* of a lot of
//! template expansions either way raises a question of how much this actually
//! matters); it would require a custom [`peg::Parse`] implementation with
//! interior mutability; it would require passing the mutable global state
//! *into* the parser in a way which does not break; it would be impossible to
//! abort processing on error because [`peg`] does not currently have a way to
//! emit “fatal” errors. If it turns out that this absolutely *has* to be the
//! way that things work to render all articles correctly, there is an old
//! aborted attempt at this in another branch somewhere, which ran off the rails
//! at the point where template parameter default values had to be spliced into
//! overlapping memory.

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
    for attempt in 0..2 {
        if let Some(target) = &article.redirect {
            log::trace!("Redirection #{} to {target}", attempt + 1);
            article = db.get(target)?;
        } else {
            break;
        }
    }

    Ok(article)
}
