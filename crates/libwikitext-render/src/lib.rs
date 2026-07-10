//! Article rendering types and functions.
//!
//! This code is somewhat of a mess, even more than the rest, because it was
//! originally developed under the incorrect assumption that Wikitext is a
//! reasonable format and that any Wikitext could be correctly represented by a
//! unified AST. This is incorrect, but the wrong assumption persists in the way
//! that preprocessing steps (expanding templates, running extension tags and
//! replacing them with strip markers, running modules, etc.) are mixed up with
//! postprocessing steps (extracting the contents of strip markers, running
//! extension tags glued together by template expansions, converting the
//! Wikitext DSL into HTML, etc.).

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
mod transform;

use core::{fmt, time::Duration};
use document::{Document, ParseFully, ParseHalf};
use expand_templates::{ExpandMode, ExpandTemplates};
pub use extension_tags::{OutputMode, PluginExtensionTag, PluginTagArgs};
use libmisc::CowExt as _;
use libphp_rs::DateTime;
use libwikitext_common::{
    Messages,
    config::Configuration,
    db::{Article, BoxedDbError, DynDatabaseProvider, resolve_redirects},
    lru_limiter::ByMemoryUsage,
    title::Title,
    url::Url,
};
use libwikitext_parse::{
    FileMap, LineCol, MARKER_PREFIX, MARKER_SUFFIX, Output, Parser, inspect, strip,
};
pub use parser_fns::{PluginFnArgs, PluginParserFn};
use piccolo::Lua;
use schnellru::LruMap;
use stack::{Kv, StackFrame};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, PoisonError, RwLock},
};
use surrogate::Surrogate;
use tags::{LinkKind, LinkKindOptions};
use template::DbPrefetch;

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

/// The result of a plugin extension tag call.
pub type PluginResult<T = (), E = anyhow::Error> = core::result::Result<T, E>;

/// An opaque mutable state object for plugin calls.
pub struct PluginState<'call, 's, 'config, 'dict>(&'call mut State<'s, 'config, 'dict>);

impl PluginState<'_, '_, '_, '_> {
    /// Replaces all strip markers in the given `text` with their original
    /// contents.
    #[inline]
    #[must_use]
    pub fn unstrip<'a>(&self, text: &'a str) -> Cow<'a, str> {
        self.0.strip_markers.unstrip_all(text)
    }
}

/// The result of an article rendering operation.
pub struct RenderOutput {
    /// The article category list.
    pub categories: globals::Categories,
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

/// Preprocesses the given article, discarding intermediate state.
///
/// This is really useful only for debugging since the intermediate state
/// contains the data for things like strip markers which are required to fully
/// render the result Wikitext.
///
/// # Errors
///
/// * Rendering fails
pub fn preprocess_article(
    statics: &mut Statics<'_, '_>,
    article: &Arc<Article>,
    load_mode: LoadMode,
    redirect: bool,
) -> Result<String> {
    let article = Arc::clone(article);
    let article = if redirect {
        resolve_redirects(&statics.db, article)?
    } else {
        article
    };

    let sp = StackFrame::new(
        Title::new(statics.db.config(), article.title(), None)?,
        FileMap::new(article.body()),
    );

    render_preprocess(statics, &article, &sp, load_mode).map(|(_, source)| source)
}

/// Main renderer entrypoint for articles.
///
/// # Errors
///
/// * Rendering fails
pub fn render_article(
    statics: &mut Statics<'_, '_>,
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
        Title::new(statics.db.config(), article.title(), None)?,
        FileMap::new(article.body()),
    );

    render(statics, &article, &sp, load_mode)
}

/// Main renderer entrypoint for eval.
///
/// # Errors
///
/// * Rendering fails
pub fn render_string(
    statics: &mut Statics<'_, '_>,
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

    let article = Arc::new(
        Article::builder()
            .id(Article::UNSAVED_ID)
            .title(page_name)
            .body(source)
            .revision_id(Article::UNSAVED_ID)
            .build(),
    );

    let mut sp = StackFrame::new(
        Title::new(statics.db.config(), page_name, None)?,
        FileMap::new(source),
    );
    let sp = if let Some(args) = args {
        let source = core::mem::replace(&mut sp.source, FileMap::new(args));
        sp.chain(
            Title::new(statics.db.config(), "(include-eval)", None)?,
            source,
            &kvs,
        )?
    } else {
        sp
    };

    let load_mode = LoadMode::Module;
    match mode {
        EvalPp::Post => render(statics, &article, &sp, load_mode),
        EvalPp::Pre | EvalPp::PreTree | EvalPp::Tree => {
            let (state, source) = render_preprocess(statics, &article, &sp, load_mode)?;
            let mut content = if mode == EvalPp::Pre {
                source
            } else if mode == EvalPp::PreTree {
                let root = state.statics.parser.parse(&source)?;
                format!("{:#?}", inspect(&FileMap::new(&source), &root))
            } else {
                let root = state
                    .statics
                    .parser
                    .preprocess(&sp.source, args.is_some())?;
                format!("{:#?}", inspect(&sp.source, &root.root))
            };

            if markers {
                for (index, marker) in state.strip_markers.0.iter().enumerate() {
                    use core::fmt::Write as _;
                    write!(content, "\n\n=== Marker {index} ===\n\n{marker}\n")?;
                }
            }

            Ok(RenderOutput {
                categories: <_>::default(),
                content,
                indicators: <_>::default(),
                outline: <_>::default(),
                styles: <_>::default(),
            })
        }
    }
}

/// Main renderer entrypoint.
fn render(
    statics: &mut Statics<'_, '_>,
    article: &Arc<Article>,
    sp: &StackFrame<'_>,
    load_mode: LoadMode,
) -> Result<RenderOutput> {
    let (mut state, source) = render_preprocess(statics, article, sp, load_mode)?;

    let sp = sp.clone_with_source(FileMap::new(&source));
    let root = state.statics.parser.parse(&sp.source)?;

    let mut prefetcher = DbPrefetch::default();
    prefetcher.adopt_tokens(&mut state, &sp, &root)?;
    prefetcher.finish(&mut state);

    let mut outline = <_>::default();
    let mut renderer = Document::<ParseFully<'_>>::new(&mut outline);
    renderer.adopt_tokens(&mut state, &sp, &root)?;
    let content = renderer.finish();

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
        let tpl_mem = if let Some(cache) = &state.statics.template_cache {
            cache.read()?.limiter().heap_usage() + cache.read()?.memory_usage()
        } else {
            0
        };
        let vm_mem = state.statics.vm.total_memory();

        log::debug!(
            "Caches:\n  Database: {:.2}KiB\n  Template: {:.2}KiB\n  VM: {:.2}KiB",
            (state.statics.db.cache_size() as f64) / 1024.0,
            (tpl_mem as f64) / 1024.0,
            (vm_mem as f64) / 1024.0,
        );
    }

    Ok(RenderOutput {
        categories: state.globals.categories,
        content,
        indicators: state.globals.indicators,
        outline,
        styles: state.globals.styles.text,
    })
}

/// Expands all templates for the given root frame, collecting out-of-band
/// information and returning the incomplete state and the final pre-processed
/// Wikitext.
fn render_preprocess<'a, 'b, 'c>(
    statics: &'a mut Statics<'b, 'c>,
    article: &Arc<Article>,
    sp: &StackFrame<'_>,
    load_mode: LoadMode,
) -> Result<(State<'a, 'b, 'c>, String)> {
    let root = statics.parser.preprocess(&sp.source, sp.parent.is_some())?;

    lua::reset_vm(
        &mut statics.vm,
        &statics.messages,
        &sp.name,
        statics.base_time,
    )?;

    let mut state = State {
        globals: ArticleState::new(statics.db.config(), Arc::clone(article)),
        load_mode,
        statics,
        strip_markers: <_>::default(),
        timing: <_>::default(),
        vm_request_cache: <_>::default(),
    };

    let mut out = String::new();
    let mut preprocessor = ExpandTemplates::new(
        &mut out,
        if sp.parent.is_some() {
            ExpandMode::Include
        } else {
            ExpandMode::Normal
        },
    );
    preprocessor.adopt_output(&mut state, sp, &root)?;
    Ok((state, out))
}

/// Preprocesses the given text in a root document scope.
fn preprocess_frame(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    text: &str,
    mode: ExpandMode,
) -> Result<String> {
    let sp = sp.clone_with_source(FileMap::new(text));
    let root = state
        .statics
        .parser
        .preprocess(&sp.source, mode == ExpandMode::Include)?;
    let mut out = String::new();
    let mut preprocessor = ExpandTemplates::new(&mut out, mode);
    preprocessor.adopt_output(state, &sp, &root)?;
    Ok(out)
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
    Database(#[from] BoxedDbError),

    /// An arithmetic expression evaluation error.
    #[error("eval error: {0}")]
    Expr(#[from] libwikitext_common_gpl::expr::Error),

    /// An extension tag error.
    #[error(transparent)]
    Extension(Box<dyn core::error::Error + Send + Sync + 'static>),

    /// A write to a buffer failed.
    #[error("fmt error: {0}")]
    Fmt(#[from] fmt::Error),

    /// ICU4X was sad about retrieving data.
    #[error(transparent)]
    IcuData(#[from] icu_provider::DataError),

    /// ICU4X was sad about parsing a locale name.
    #[error(transparent)]
    IcuLocale(#[from] icu_locale::ParseError),

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

    /// An plugin error.
    #[error(transparent)]
    Plugin(anyhow::Error),

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

    /// A [`StripMarker`](libwikitext_parse::Token::StripMarker) was encountered
    /// in what should have been just a run of plain text.
    #[error("strip marker got into text")]
    StripMarkerInText,

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

    /// An error occurred parsing a [`Title`].
    #[error(transparent)]
    Title(#[from] libwikitext_common::title::Error),
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
pub struct Statics<'config, 'dict> {
    /// The “current” time, according to the article database.
    #[builder(setter(doc = "Sets the “current” time."))]
    base_time: DateTime,
    /// The server’s base URI.
    #[builder(setter(doc = "Sets the server’s base URI."))]
    base_uri: Url,
    /// The article database.
    #[builder(setter(doc = "Sets the article database."))]
    db: Arc<dyn DynDatabaseProvider>,
    /// Extra extension tags.
    #[builder(default)]
    extension_tags: HashMap<&'static str, &'static dyn PluginExtensionTag>,
    /// Time and memory limits.
    #[builder(
        default,
        setter(doc = "Sets the time and memory limits for the renderer.")
    )]
    limits: Limits,
    /// The interface messages database.
    messages: Messages<'dict, Arc<dyn DynDatabaseProvider>>,
    /// The URI paths for links and media.
    paths: Paths,
    /// The parser.
    #[builder(setter(
        doc = "Sets the parser configuration.",
        transform = |config: &'config Configuration| Parser::new(config))
    )]
    pub parser: Parser<'config>,
    /// Extra parser functions.
    #[builder(default)]
    parser_fns: HashMap<&'static str, &'static dyn PluginParserFn>,
    /// Template AST cache.
    #[builder(default, setter(doc = "Sets the global template cache.", strip_option))]
    template_cache: Option<TemplateCache>,
    /// The Lua interpreter.
    #[builder(default = lua::new_vm(&base_uri.clone().extend_path(paths.article), messages, parser).unwrap(), setter(skip))]
    pub vm: Lua,
    /// VM module cache.
    #[builder(default = LruMap::new(schnellru::UnlimitedCompact), setter(skip))]
    vm_cache: LruMap<ArticleId, lua::VmCacheEntry, schnellru::UnlimitedCompact>,
    /// The next globally unique ID for a VM cache marker. This must be global
    /// because it must persist across requests.
    #[builder(default)]
    vm_cache_marker_id: u64,
}

/// URI resource paths.
#[derive(Clone, Copy, Debug)]
pub struct Paths {
    /// The path segment for articles.
    pub article: &'static str,
    /// The path segment for external links. If `None`, external links will be
    /// emitted as direct links.
    pub external: Option<&'static str>,
    /// The path segment for media.
    pub media: &'static str,
}

/// A list of stripped extension tags.
#[derive(Default)]
struct StripMarkers(Vec<StripMarker<'static>>);

impl StripMarkers {
    /// Invokes callback `f` for each strip marker in the given text.
    ///
    /// The callback should return `Some(string)` if it wants to replace the
    /// marker, or `None` if it wants the marker to be kept as-is in the text.
    #[inline]
    pub fn for_each_marker<'a, 'b, F>(&'b self, body: &'a str, mut f: F) -> Cow<'a, str>
    where
        F: FnMut(&'b StripMarker<'b>) -> Option<Cow<'b, str>>,
    {
        strip::for_each_marker_key(body, |key| f(&self.0[strip::key_index(key)]))
    }

    /// Gets the strip marker with the given key.
    fn get(&self, key: &str) -> Option<&StripMarker<'_>> {
        self.0.get(strip::key_index(key))
    }

    /// Pushes a new strip marker to the list, emitting the marker to the given
    /// `out` string.
    fn push<W: fmt::Write + ?Sized>(
        &mut self,
        out: &mut W,
        tag_name: &str,
        marker: StripMarker<'static>,
    ) {
        // The extra hyphen is just part of the joy of required bug-accuracy,
        // as these values are exposed to modules and then the modules expect
        // the unnecessary extra hyphen
        let _ = write!(
            out,
            "{MARKER_PREFIX}-{tag_name}-{:x}{MARKER_SUFFIX}",
            self.0.len()
        );
        self.0.push(marker);
    }

    /// Pushes a new strip marker to the list, returning its index.
    #[inline]
    fn push_indexed(&mut self, marker: StripMarker<'static>) -> usize {
        let index = self.0.len();
        self.0.push(marker);
        index
    }

    /// Recursively replaces all strip markers in the given `text` with their
    /// original contents.
    fn unstrip_all<'a>(&self, text: &'a str) -> Cow<'a, str> {
        self.unstrip_recursive(text, 0, &|marker| Some(Cow::Borrowed(marker.as_ref())))
    }

    /// Recursively replaces `<nowiki>` strip markers in the given `text` with
    /// their original contents.
    fn unstrip_no_wiki<'a>(&self, text: &'a str) -> Cow<'a, str> {
        self.unstrip_recursive(text, 0, &|marker| {
            if let StripMarker::NoWiki(text) = marker {
                Some(Cow::Borrowed(text))
            } else {
                None
            }
        })
    }

    /// An internal function for recursively replacing strip markers in the
    /// given `text` using `f`.
    #[inline]
    fn unstrip_recursive<'a, F>(&self, text: &'a str, level: u8, f: &F) -> Cow<'a, str>
    where
        for<'b> F: Fn(&'b StripMarker<'b>) -> Option<Cow<'b, str>>,
    {
        if level == 20 {
            log::error!("unstrip recursed over 20 times");
            Cow::Borrowed(text)
        } else {
            self.for_each_marker(text, |marker| {
                f(marker).map(|text| text.map(|text| self.unstrip_recursive(text, level + 1, f)))
            })
        }
    }

    /// Recursively replaces strip markers in the given `text` using a callback
    /// `f`.
    ///
    /// When `f` returns `Some(replacement)`, the associated strip marker is
    /// replaced with the result of calling this function on `replacement`.
    fn unstrip_with<'a, F>(&self, text: &'a str, f: F) -> Cow<'a, str>
    where
        for<'b> F: Fn(&'b StripMarker<'b>) -> Option<Cow<'b, str>>,
    {
        self.unstrip_recursive(text, 0, &f)
    }
}

/// A strip marker.
#[derive(Debug)]
pub(crate) enum StripMarker<'a> {
    /// A strip marker containing general HTML.
    General(Cow<'a, str>),
    /// A strip marker containing only phrasing content from a `<nowiki>` tag.
    NoWiki(Cow<'a, str>),
    /// A strip marker containing a wiki.rs-specific template source end marker.
    WikiRsSourceEnd(Cow<'a, str>),
    /// A strip marker containing a wiki.rs-specific template source start
    /// marker.
    WikiRsSourceStart(Cow<'a, str>),
}

impl<'a> StripMarker<'a> {
    /// Makes a new `StripMarker` using a reference-returning callback.
    fn map_ref(&'a self, f: impl FnOnce(&'a str) -> Cow<'a, str>) -> Self {
        match self {
            Self::General(s) => Self::General(f(s)),
            Self::NoWiki(s) => Self::NoWiki(f(s)),
            Self::WikiRsSourceEnd(s) => Self::WikiRsSourceEnd(Cow::Borrowed(s)),
            Self::WikiRsSourceStart(s) => Self::WikiRsSourceStart(Cow::Borrowed(s)),
        }
    }
}

impl fmt::Display for StripMarker<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl core::ops::Deref for StripMarker<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            StripMarker::General(s)
            | StripMarker::NoWiki(s)
            | StripMarker::WikiRsSourceStart(s)
            | StripMarker::WikiRsSourceEnd(s) => s,
        }
    }
}

/// Renderer state that is shared across stack frames.
pub(crate) struct State<'s, 'config, 'dict> {
    /// Article data.
    pub globals: ArticleState,
    /// The page load strategy.
    pub load_mode: LoadMode,
    /// Thread static global variables.
    pub statics: &'s mut Statics<'config, 'dict>,
    /// Stripped extension tag substitutions.
    pub strip_markers: StripMarkers,
    /// Page performance timing data.
    timing: HashMap<String, (usize, Duration)>,
    /// VM cache marker IDs already rendered for this rendering request.
    vm_request_cache: HashSet<u64>,
}

/// Shared article data.
#[derive(Debug)]
struct ArticleState {
    /// The article data.
    article: Arc<Article>,
    /// Collected categories to append to the footer of the page.
    categories: globals::Categories,
    /// The last ordinal used by an unlabelled external link.
    external_link_ordinal: u32,
    /// Indicator icons for the `<indicator>` extension tag.
    indicators: globals::Indicators,
    /// Collected references for the `<ref>` and `<references>` extension tags.
    references: extension_tags::References,
    /// Labelled section transclusion sections.
    sections: extension_tags::LabelledSections,
    /// Collected CSS for the `<templatestyles>` extension tag.
    styles: extension_tags::Styles,
    /// The title of the article.
    title: Title,
    /// Sometimes settable magic variables, e.g. `{{SHORTDESC}}`.
    variables: HashMap<String, String>,
}

impl ArticleState {
    /// Creates a new article processing state for the given `article`.
    fn new(config: &Configuration, article: Arc<Article>) -> Self {
        let title = Title::new(config, article.title(), None).expect("valid title");
        Self {
            article,
            categories: <_>::default(),
            external_link_ordinal: <_>::default(),
            indicators: <_>::default(),
            references: <_>::default(),
            sections: <_>::default(),
            styles: <_>::default(),
            title,
            variables: <_>::default(),
        }
    }
}

/// Evaluates the given `source` in the context of the given `sp`, returning
/// either a half-parsed Wikitext or fully-parsed HTML string according to
/// `parse_fully`.
fn eval_plugin(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    fully_parse: bool,
    source: &str,
) -> Result<String> {
    let sp = sp.clone_with_source(FileMap::new(source));
    let root = state.statics.parser.parse(&sp.source)?;
    let mut outline = <_>::default();
    Ok(if fully_parse {
        let mut out = Document::<ParseFully<'_>>::new(&mut outline);
        out.adopt_tokens(state, &sp, &root)?;
        out.finish()
    } else {
        let mut out = Document::<ParseHalf<'_>>::new(&mut outline);
        out.adopt_tokens(state, &sp, &root)?;
        out.finish()
    })
}

/// Writes a run of text to the given output as entity-encoded HTML, converting
/// wretched typewriter quote marks to beautiful works of fine typographical
/// art. We are not savages here today.
// TODO: This sucks, do something better.
#[inline]
fn text_run(text: &str) -> String {
    use transform::Sink as _;
    let mut emitter = transform::PrettyText::new(transform::Accumulator::new());
    emitter.text(text);
    emitter.finish()
}
