//! Types and functions for communicating with article renderers.

use super::{
    Error, ExpandMode, ExpandTemplates, Result, State, Statics, TemplateCache,
    document::Document,
    globals::{Indicators, Outline},
    resolve_redirects,
    stack::StackFrame,
    surrogate::Surrogate as _,
};
use crate::{
    Limits, LoadMode,
    config::CONFIG,
    db::{Article, Database},
    lru_limiter::ByMemoryUsage,
    lua::{new_vm, reset_vm},
    pages::EvalPp,
    php::DateTime,
    title::Title,
    wikitext::{FileMap, Parser, inspect},
};
use axum::http::Uri;
use schnellru::LruMap;
use std::sync::{Arc, RwLock, mpsc};

/// A renderer channel message command.
pub(crate) enum Command {
    /// Render an article.
    Article {
        /// The article to render.
        article: Arc<Article>,
        /// The load mode to use when rendering the article.
        load_mode: LoadMode,
        /// If true, follow the article’s redirect before rendering.
        redirect: bool,
    },
    /// Render some arbitrary Wikitext.
    Eval {
        /// Arguments for parameters in the Wikitext.
        args: Option<String>,
        /// The Wikitext.
        code: String,
        /// If true, append marker content to the output.
        markers: bool,
        /// Which rendering step to return.
        mode: EvalPp,
        /// The root page name.
        page_name: String,
    },
    /// Extract the redirect target from an article.
    Redirect {
        /// The redirect source article.
        article: Arc<Article>,
    },
}

/// The input format for a renderer channel message.
pub(crate) struct In {
    /// The renderer command.
    pub command: Command,
    /// The return channel.
    pub tx: mpsc::Sender<Out>,
}

/// The output format for a renderer channel message.
pub type Out = Result<RenderOutput, Error>;

/// Manager for renderer connections.
pub(crate) struct RenderManager {
    /// The base URI to provide to spawned renderers.
    base_uri: Uri,
    /// The article database to provide to spawned renderers.
    database: Arc<Database<'static>>,
    /// Time and memory limits.
    limits: Limits,
    /// Template cache.
    template_cache: TemplateCache,
}

impl RenderManager {
    /// Creates a new render manager.
    pub fn new(base_uri: &Uri, database: &Arc<Database<'static>>, limits: Limits) -> Self {
        Self {
            base_uri: base_uri.clone(),
            database: Arc::clone(database),
            limits,
            template_cache: Arc::new(RwLock::new(LruMap::new(ByMemoryUsage::new(
                limits.template_cache,
            )))),
        }
    }
}

impl r2d2::ManageConnection for RenderManager {
    type Connection = mpsc::Sender<In>;

    type Error = Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let (tx, rx) = mpsc::channel::<In>();
        let base_uri = self.base_uri.clone();
        let limits = self.limits;
        let template_cache = Arc::clone(&self.template_cache);
        // TODO: This date should be calculated from the database file.
        let db = Arc::clone(&self.database);
        let base_time = db.creation_date().map_or_else(DateTime::now, |date| {
            DateTime::from_unix_timestamp(date.unix_timestamp())
        })?;
        let parser = Parser::new(&CONFIG);
        std::thread::spawn(move || {
            let vm = new_vm(&base_uri, &db, &parser).unwrap();
            let mut statics = Statics {
                base_uri,
                base_time,
                db,
                limits,
                parser,
                template_cache,
                vm,
                vm_cache: LruMap::new(schnellru::UnlimitedCompact),
            };

            for In { command, tx } in rx {
                let output = match command {
                    Command::Article {
                        article,
                        load_mode,
                        redirect,
                    } => render_article(&mut statics, &article, load_mode, redirect),
                    Command::Eval {
                        args,
                        code,
                        markers,
                        mode,
                        page_name,
                    } => render_string(
                        &mut statics,
                        &page_name,
                        &code,
                        args.as_deref(),
                        mode,
                        markers,
                    ),
                    Command::Redirect { article } => statics
                        .parser
                        .parse_redirect(&article.body)
                        .map(|redirect| RenderOutput {
                            content: redirect.to_string(),
                            indicators: <_>::default(),
                            outline: <_>::default(),
                            styles: <_>::default(),
                        })
                        .map_err(Error::from),
                };
                let _ = tx.send(output);
                statics.vm.gc_collect();
            }
        });

        Ok(tx)
    }

    fn is_valid(&self, _: &mut Self::Connection) -> Result<(), Self::Error> {
        Ok(())
    }

    fn has_broken(&self, _: &mut Self::Connection) -> bool {
        false
    }
}

/// The result of an article rendering operation.
pub(crate) struct RenderOutput {
    /// The main HTML content of the page.
    pub content: String,
    /// Indicator badges. [`Display`](core::fmt::Display) formats as HTML.
    pub indicators: Indicators,
    /// The article outline (table of contents). [`Display`](core::fmt::Display)
    /// formats as HTML.
    pub outline: Outline,
    /// Extra CSS required for correct article styling.
    pub styles: String,
}

/// Main renderer entrypoint for articles.
fn render_article(
    statics: &mut Statics,
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
        Title::new(&article.title, None),
        FileMap::new(&article.body),
    );

    render(statics, load_mode, &sp)
}

/// Main renderer entrypoint for eval.
fn render_string(
    statics: &mut Statics,
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
    let kvs = kvs.iter().map(super::Kv::Argument).collect::<Vec<_>>();

    let mut sp = StackFrame::new(Title::new(page_name, None), FileMap::new(source));
    let sp = if let Some(args) = args {
        let source = core::mem::replace(&mut sp.source, FileMap::new(args));
        sp.chain(Title::new("(include-eval)", None), source, &kvs)?
    } else {
        sp
    };

    let load_mode = LoadMode::Module;
    match mode {
        EvalPp::Post => render(statics, load_mode, &sp),
        EvalPp::Pre | EvalPp::PreTree | EvalPp::Tree => {
            let (state, source) = preprocess(statics, &sp, load_mode)?;
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

/// Main renderer entrypoint.
fn render(statics: &mut Statics, load_mode: LoadMode, sp: &StackFrame<'_>) -> Result<RenderOutput> {
    let (mut state, source) = preprocess(statics, sp, load_mode)?;

    let sp = sp.clone_with_source(FileMap::new(&source));
    let root = state.statics.parser.parse_no_expansion(&sp.source)?;

    let mut prefetcher = super::template::DbPrefetch::default();
    prefetcher.adopt_output(&mut state, &sp, &root)?;
    prefetcher.finish(&mut state);

    let mut renderer = Document::new(false);
    renderer.adopt_output(&mut state, &sp, &root)?;
    let mut content = renderer.finish()?;

    let mut timings = state.timing.into_iter().collect::<Vec<_>>();
    timings.sort_by(|(_, (_, a)), (_, (_, b))| b.cmp(a));
    for (the_baddie, (count, time)) in timings {
        log::trace!("{the_baddie}: {count} / {}s", time.as_secs_f64());
    }

    // Clippy: If memory usage is ever >2**52, something sure happened.
    #[allow(clippy::cast_precision_loss)]
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
fn preprocess<'a>(
    statics: &'a mut Statics,
    sp: &StackFrame<'_>,
    load_mode: LoadMode,
) -> Result<(State<'a>, String)> {
    let root = statics.parser.parse(&sp.source, false)?;

    reset_vm(&mut statics.vm, &sp.name, &statics.base_time)?;

    let mut state = State {
        globals: <_>::default(),
        load_mode,
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
