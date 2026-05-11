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

mod document;
mod emitters;
mod expand_templates;
mod extension_tags;
mod globals;
mod image;
mod lua;
mod parser_fns;
mod stack;
mod surrogate;
mod tags;
mod template;
mod trim;

use crate::{
    document::Document,
    template::DbPrefetch,
    trim::{Trim, TrimMode},
};
use core::{fmt, time::Duration};
use expand_templates::{ExpandMode, ExpandTemplates};
use http::Uri;
use libphp_rs::DateTime;
use libwikitext_common::{
    db::{Article, DatabaseProvider},
    lru_limiter::ByMemoryUsage,
    title::Title,
};
use libwikitext_parse::{
    Configuration, FileMap, LineCol, MARKER_PREFIX, MARKER_SUFFIX, Output, inspect, strip,
};
use libwikitext_parse_gpl::Parser;
use piccolo::Lua;
use schnellru::LruMap;
use stack::{Kv, StackFrame};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, PoisonError, RwLock},
};
use surrogate::Surrogate;
use tags::{LinkKind, LinkKindOptions};

/// Preprocessor display options for evaluating text strings.
#[derive(Clone, Copy, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvalPp {
    /// Show unprocessed source as a tree.
    Tree,
    /// Show preprocessed source.
    Pre,
    /// Show preprocessed source as a tree.
    PreTree,
    /// Show post-processed result.
    #[default]
    Post,
}

/// Time and memory limits for the renderer.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Lua single call time limit.
    pub vm_time: Duration,
    /// Lua VM total memory limit, in bytes. One per renderer thread.
    pub vm_total_mem: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            vm_time: Duration::new(10, 0),
            vm_total_mem: 128 * 1024 * 1024,
        }
    }
}

/// The result of an article rendering operation.
pub struct RenderOutput {
    /// The main HTML content of the page.
    pub content: String,
    /// Indicator badges. [`Display`](core::fmt::Display) formats as HTML.
    pub indicators: globals::Indicators,
    /// The article outline (table of contents). [`Display`](core::fmt::Display)
    /// formats as HTML.
    pub outline: globals::Outline,
    /// Extra CSS required for correct article styling.
    pub styles: String,
}

/// Main renderer entrypoint for articles.
///
/// # Errors
///
/// * Rendering fails
pub fn render_article(
    statics: &mut Statics<'_>,
    messages: &serde_json_borrow::Value<'_>,
    article: &Arc<Article>,
    load_mode: LoadMode,
    redirect: bool,
) -> Result<RenderOutput> {
    let article = Arc::clone(article);
    let article = if redirect {
        resolve_redirects(&statics.db, article)?
    } else {
        article
    };

    let sp = StackFrame::new(
        Title::new(statics.db.config(), &article.title, None),
        FileMap::new(&article.body),
    );

    render(statics, messages, &sp, load_mode)
}

/// Main renderer entrypoint for eval.
///
/// # Errors
///
/// * Rendering fails
pub fn render_string(
    statics: &mut Statics<'_>,
    messages: &serde_json_borrow::Value<'_>,
    mut page_name: &str,
    source: &str,
    args: Option<&str>,
    mode: EvalPp,
    markers: bool,
) -> Result<RenderOutput> {
    if page_name.is_empty() {
        page_name = "(eval)";
    }

    let kvs = args.map_or(Ok(<_>::default()), |args| {
        statics.parser.debug_parse_args(args)
    })?;
    let kvs = kvs.iter().map(Kv::Argument).collect::<Vec<_>>();

    let mut sp = StackFrame::new(
        Title::new(statics.db.config(), page_name, None),
        FileMap::new(source),
    );
    let sp = if let Some(args) = args {
        let source = core::mem::replace(&mut sp.source, FileMap::new(args));
        sp.chain(
            Title::new(statics.db.config(), "(include-eval)", None),
            source,
            &kvs,
        )?
    } else {
        sp
    };

    let load_mode = LoadMode::Module;
    match mode {
        EvalPp::Post => render(statics, messages, &sp, load_mode),
        EvalPp::Pre | EvalPp::PreTree | EvalPp::Tree => {
            let (state, source) = preprocess(statics, messages, &sp, load_mode)?;
            let mut content = if mode == EvalPp::Pre {
                source
            } else if mode == EvalPp::PreTree {
                let root = state.statics.parser.parse(&source, false)?;
                format!("{:#?}", inspect(&FileMap::new(&source), &root.root))
            } else {
                let root = state.statics.parser.parse(&sp.source, false)?;
                format!("{:#?}", inspect(&sp.source, &root.root))
            };

            if markers {
                for (index, marker) in state.strip_markers.0.iter().enumerate() {
                    use core::fmt::Write as _;
                    write!(content, "\n\n=== Marker {index} ===\n\n{marker}\n")?;
                }
            }

            Ok(RenderOutput {
                content,
                indicators: <_>::default(),
                outline: <_>::default(),
                styles: <_>::default(),
            })
        }
    }
}

/// Main renderer entrypoint for Wikitext tests.
///
/// # Errors
///
/// * Rendering fails
// TODO: This function should not exist here.
pub fn render_test(
    statics: &mut Statics<'_>,
    messages: &serde_json_borrow::Value<'_>,
    page_name: &str,
    source: &str,
) -> Result<RenderOutput> {
    let sp = StackFrame::new(
        Title::new(statics.db.config(), page_name, None),
        FileMap::new(source),
    );
    render(statics, messages, &sp, LoadMode::Module)
}

/// Main renderer entrypoint.
fn render(
    statics: &mut Statics<'_>,
    messages: &serde_json_borrow::Value<'_>,
    sp: &StackFrame<'_>,
    load_mode: LoadMode,
) -> Result<RenderOutput> {
    let (mut state, source) = preprocess(statics, messages, sp, load_mode)?;

    let sp = sp.clone_with_source(FileMap::new(&source));
    let root = state.statics.parser.parse_no_expansion(&sp.source)?;

    let mut prefetcher = DbPrefetch::default();
    prefetcher.adopt_output(&mut state, &sp, &root)?;
    prefetcher.finish(&mut state);

    let mut renderer = Document::new(false);
    Trim::new(&mut renderer, &sp, TrimMode::Category).adopt_output(&mut state, &sp, &root)?;
    let mut content = renderer.finish(&mut state)?;

    let mut timings = state.timing.into_iter().collect::<Vec<_>>();
    timings.sort_by(|(_, (_, a)), (_, (_, b))| b.cmp(a));
    for (the_baddie, (count, time)) in timings {
        log::trace!("{the_baddie}: {count} / {}s", time.as_secs_f64());
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "if memory usage is ever ≥2**53, something sure happened"
    )]
    {
        let tpl_mem = {
            let cache = &state.statics.template_cache;
            cache.read()?.limiter().heap_usage() + cache.read()?.memory_usage()
        };
        let vm_mem = state.statics.vm.total_memory();

        log::debug!(
            "Caches:\n  Database: {:.2}KiB\n  Template: {:.2}KiB\n  VM: {:.2}KiB",
            (state.statics.db.cache_size() as f64) / 1024.0,
            (tpl_mem as f64) / 1024.0,
            (vm_mem as f64) / 1024.0,
        );
    }

    state
        .globals
        .categories
        .finish(&mut content, state.statics.base_uri.path())?;

    Ok(RenderOutput {
        content,
        indicators: state.globals.indicators,
        outline: state.globals.outline,
        styles: state.globals.styles.text,
    })
}

/// Expands all templates for the given root frame, collecting out-of-band
/// information and returning the incomplete state and the final pre-processed
/// Wikitext.
fn preprocess<'a, 'b, 'c>(
    statics: &'a mut Statics<'b>,
    messages: &'c serde_json_borrow::Value<'c>,
    sp: &StackFrame<'_>,
    load_mode: LoadMode,
) -> Result<(State<'a, 'b, 'c>, String)> {
    let root = statics.parser.parse(&sp.source, false)?;

    lua::reset_vm(&mut statics.vm, messages, &sp.name, statics.base_time)?;

    let mut state = State {
        globals: <_>::default(),
        load_mode,
        messages,
        statics,
        strip_markers: <_>::default(),
        timing: <_>::default(),
    };

    // TODO: Rewrite the PEG so that it does the expansions instead of
    // doing this awful double-parsing.
    let mut preprocessor = ExpandTemplates::new(ExpandMode::Normal);
    preprocessor.adopt_output(&mut state, sp, &root)?;
    Ok((state, preprocessor.finish()))
}

/// Creates a new template cache with the given size in bytes.
#[must_use]
pub fn make_template_cache(size: usize) -> TemplateCache {
    Arc::new(RwLock::new(LruMap::new(ByMemoryUsage::new(size))))
}

/// An article rendering error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A database call failed.
    #[error("db error: {0}")]
    Database(#[from] libwikitext_common::db::Error),

    /// An arithmetic expression evaluation error.
    #[error("eval error: {0}")]
    Expr(#[from] libwikitext_common_gpl::expr::Error),

    /// An extension tag error.
    #[error(transparent)]
    Extension(Box<dyn core::error::Error + Send + Sync + 'static>),

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
    Peg(#[from] libwikitext_parse::Error),

    /// An [`RwLock`] guard was poisoned.
    #[error("poisoned lock")]
    Poison,

    /// Too many template calls.
    #[error("template stack overflow: {0}")]
    StackOverflow(String),

    /// A [`StripMarker`](libwikitext_parse::Token::StripMarker) was encountered
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
    Time(#[from] libphp_rs::DateTimeError),
}

/// Page rendering strategy.
///
/// This exists purely as a performance optimisation. If first time to paint
/// could be guaranteed to be under one second for all pages, this could be
/// eliminated.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LoadMode {
    /// Expand no templates.
    Base,
    /// Expand templates.
    #[default]
    Template,
    /// Expand templates and run Lua modules.
    Module,
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
pub type TemplateCache = Arc<RwLock<LruMap<ArticleId, Arc<Output>, ByMemoryUsage>>>;

/// Global variables which are used for the entire lifetime of a renderer
/// thread.
#[derive(typed_builder::TypedBuilder)]
#[builder(doc)]
pub struct Statics<'config> {
    /// The “current” time, according to the article database.
    #[builder(setter(doc = "Sets the “current” time."))]
    base_time: DateTime,
    /// The server’s base URI.
    #[builder(setter(doc = "Sets the server’s base URI."))]
    base_uri: Uri,
    /// The article database.
    #[builder(setter(doc = "Sets the article database."))]
    db: Arc<dyn DatabaseProvider>,
    /// Time and memory limits.
    #[builder(
        default,
        setter(doc = "Sets the time and memory limits for the renderer.")
    )]
    limits: Limits,
    /// The parser.
    #[builder(setter(
        doc = "Sets the parser configuration.",
        transform = |config: &'config Configuration| Parser::new(config))
    )]
    pub parser: Parser<'config>,
    /// Template AST cache.
    #[builder(
        default = make_template_cache(1024 * 1024),
        setter(doc = "Sets the global template cache. If unspecified, a 1MiB cache will be created.")
    )]
    template_cache: TemplateCache,
    /// The Lua interpreter.
    #[builder(default = lua::new_vm(base_uri, db, parser).unwrap(), setter(skip))]
    pub vm: Lua,
    /// VM module cache.
    #[builder(default = LruMap::new(schnellru::UnlimitedCompact), setter(skip))]
    vm_cache: LruMap<ArticleId, lua::VmCacheEntry, schnellru::UnlimitedCompact>,
}

/// A list of stripped extension tags.
#[derive(Default)]
pub(crate) struct StripMarkers(Vec<StripMarker>);

impl StripMarkers {
    /// Invokes callback `f` for each strip marker in the given text.
    ///
    /// The callback should return `Some(string)` if it wants to replace the
    /// marker, or `None` if it wants the marker to be kept as-is in the text.
    #[inline]
    pub fn for_each_marker<'a, F>(&self, body: &'a str, mut f: F) -> Cow<'a, str>
    where
        for<'m> F: FnMut(&'m StripMarker) -> Option<Cow<'m, str>>,
    {
        strip::for_each_marker_key(body, |key| f(&self.0[strip::key_index(key)]))
    }

    /// Gets the strip marker with the given key.
    fn get(&self, key: &str) -> Option<&StripMarker> {
        self.0.get(strip::key_index(key))
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

/// A strip marker.
#[derive(Debug)]
pub(crate) enum StripMarker {
    /// A strip marker containing block-level elements.
    Block(String),
    /// A strip marker containing only phrasing content.
    Inline(String),
    /// A strip marker containing only phrasing content from a `<nowiki>` tag.
    NoWiki(String),
    /// A strip marker containing a wiki.rs-specific template source end marker.
    WikiRsSourceEnd(String),
    /// A strip marker containing a wiki.rs-specific template source start
    /// marker.
    WikiRsSourceStart(String),
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
pub(crate) struct State<'s, 'config, 'dict> {
    /// Article data.
    pub globals: ArticleState,
    /// The page load strategy.
    pub load_mode: LoadMode,
    /// Messages dictionary.
    pub messages: &'s serde_json_borrow::Value<'dict>,
    /// Thread static global variables.
    pub statics: &'s mut Statics<'config>,
    /// Stripped extension tag substitutions.
    pub strip_markers: StripMarkers,
    /// Page performance timing data.
    timing: HashMap<String, (usize, Duration)>,
}

/// A convenience trait alias combining [`fmt::Write`] and [`Surrogate`].
trait WriteSurrogate: fmt::Write + Surrogate<Error> {}
impl<T> WriteSurrogate for T where T: fmt::Write + Surrogate<Error> {}

/// Shared article data.
#[derive(Debug, Default)]
struct ArticleState {
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
    variables: HashMap<String, String>,
}

/// Resolves any redirects for an article, returning the final article.
///
/// # Errors
///
/// * `db` returns an error getting an article
// TODO: This should really just resolve the redirects and then do the work, but
// borrowck is being unbearable today and this is a toy project so who cares
// TODO: This should be part of Database
pub fn resolve_redirects<Db: DatabaseProvider>(
    db: &Db,
    mut article: Arc<Article>,
) -> Result<Arc<Article>, Error> {
    // “Loop to fetch the article, with up to 2 redirects”
    for _ in 0..2 {
        if let Some(target) = &article.redirect {
            // log::trace!("Redirection #{} to {target}", attempt + 1);
            article = db.get(&Title::new(db.config(), target, None))?;
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
    encode: bool,
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
            '<' if encode => write!(out, "&lt;")?,
            '>' if encode => write!(out, "&gt;")?,
            '&' if encode => write!(out, "&amp;")?,
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
