#![doc = include_str!("../README.md")]
#![warn(
    clippy::pedantic,
    clippy::missing_docs_in_private_items,
    missing_docs,
    rust_2018_idioms
)]

use crate::title::Namespace;
use axum::{Router, http::Uri, routing::get};
use db::Database;
use r2d2::Pool;
use renderer::Manager as RenderManager;
use std::{ffi::OsStr, sync::Arc};
use tokio::net::TcpListener;

mod common;
mod config;
mod db;
mod expr;
mod lru_limiter;
mod lua;
mod pages;
mod php;
mod renderer;
mod title;
mod wikitext;

/// Global application state.
struct WikiState {
    /// The base URI to use when emitting canonical URIs.
    base_uri: Uri,
    /// The global article database.
    database: Arc<Database<'static>>,
    /// The default load mode for new pages.
    load_mode: LoadMode,
    /// A pool of article renderers.
    ///
    /// Renderers are pooled like this because caching Lua modules significantly
    /// improves performance, but because gc-arena is currently !Send, Lua must
    /// live on its own thread and communication must occur via channels.
    ///
    /// Obviously, splitting this into a pool where there are potentially
    /// multiple Lua engines per process is less ideal, so maybe this changes in
    /// the future, but for now this is a reasonably sane approach to at least
    /// have the opportunity to cache, and it can be tuned appropriately over
    /// time to either isolate Lua entirely from the rest of the renderer and
    /// have only a single channel, or divide up the work differently, or
    /// whatever. Given the goal of wiki.rs, this is probably overengineering at
    /// its best.
    renderer: Pool<RenderManager>,
}

/// Global application state, shareable across threads.
type AppState = Arc<WikiState>;

/// Command line usage instructions.
fn usage() {
    let exe = std::env::args().next().unwrap_or_default();
    eprintln!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    eprintln!("Usage: {exe} [options] <index.txt> <database.xml.bz2>\n");
    eprintln!("or, use environment variables:");
    eprintln!("    WIKI_INDEX_FILE");
    eprintln!("    WIKI_ARTICLE_DB\n");
    eprintln!("Options:");
    eprintln!("    --base-uri: Base URI for site (default: bind address)");
    eprintln!("    --bind: Web server bind (default: 127.0.0.1:3000)");
    eprintln!("    --mode <mode>: Initial page rendering mode. One of:");
    eprintln!("      * 'base': Render only the base Wikitext");
    eprintln!("      * 'template': Expand templates (default)");
    eprintln!("      * 'module': Expand templates and run Lua modules");
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

/// The error when [`LoadMode`] cannot be parsed from a string.
#[derive(Debug, thiserror::Error)]
#[error("unexpected value '{0}'; expected 'base', 'template', or 'module'")]
pub struct LoadModeError(String);

impl core::str::FromStr for LoadMode {
    type Err = LoadModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("base") {
            Ok(Self::Base)
        } else if s.eq_ignore_ascii_case("template") {
            Ok(Self::Template)
        } else if s.eq_ignore_ascii_case("module") {
            Ok(Self::Module)
        } else {
            Err(LoadModeError(s.to_string()))
        }
    }
}

/// Errors that may occur when parsing arguments.
#[derive(Debug, thiserror::Error)]
enum ArgsError {
    /// Missing the database argument.
    #[error("missing multistream.xml.bz2 argument")]
    Database,
    /// Extra unknown junk on the command line.
    #[error("unknown arguments: {}", _0.display())]
    Extra(std::ffi::OsString),
    /// Missing the index argument.
    #[error("missing index.txt argument")]
    Index,
    /// Some other parsing error.
    #[error(transparent)]
    Pico(#[from] pico_args::Error),
}

/// Command-line arguments.
struct Args {
    /// The path to `database.xml.bz2`.
    articles_path: String,
    /// The base URI used when generating links to resources. Useful if you
    /// decide to put this behind a web proxy for some reason.
    base_uri: Option<String>,
    /// The bind address for the web server.
    bind: String,
    /// The path to `index.txt`.
    index_path: String,
    /// The default strategy for loading pages.
    load_mode: LoadMode,
}

impl Args {
    /// Tries to get an argument either from the arguments list or from
    /// an environment varible.
    fn free_arg(
        args: &mut pico_args::Arguments,
        key: &str,
        err: ArgsError,
    ) -> Result<String, ArgsError> {
        if let Some(arg) = args.opt_free_from_str::<String>()? {
            Ok(arg)
        } else if let Ok(arg) = std::env::var(key) {
            Ok(arg)
        } else {
            Err(err)
        }
    }

    /// Tries to create an [`Args`] from the given command line arguments and
    /// environment variables.
    fn new() -> Result<Args, ArgsError> {
        let mut args = pico_args::Arguments::from_env();
        let bind = args
            .opt_value_from_str("--bind")?
            .unwrap_or_else(|| "127.0.0.1:3000".to_string());
        let base_uri = args.opt_value_from_str("--base-uri")?;
        let load_mode = args.opt_value_from_str("--mode")?.unwrap_or_default();
        let _ = args.contains("--");
        let index_path = Self::free_arg(&mut args, "WIKI_INDEX_FILE", ArgsError::Index)?;
        let articles_path = Self::free_arg(&mut args, "WIKI_ARTICLE_DB", ArgsError::Database)?;

        let rest = args.finish();
        if !rest.is_empty() {
            return Err(ArgsError::Extra(rest.join(OsStr::new(" "))));
        }

        Ok(Self {
            articles_path,
            base_uri,
            bind,
            index_path,
            load_mode,
        })
    }
}

/// Don’t run this. You’ve been warned!
#[tokio::main]
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args = match Args::new() {
        Ok(args) => args,
        Err(err) => {
            usage();
            return Err(err)?;
        }
    };

    log::info!("Starting up wiki.rs ...");

    let base_uri = if let Some(base_uri) = args.base_uri {
        base_uri.parse()
    } else {
        args.bind.parse()
    }?;

    let database = Arc::new(Database::from_file(&args.index_path, &args.articles_path)?);

    log::info!("Opened database {}", database.name());

    // The siteinfo in MediaWiki dumps does not provide enough information to
    // actually build the configuration from the dump, but it does at least
    // allow a sanity check of what namespace information is recorded.
    for (id, namespace) in database.namespaces() {
        if let Some(other) = Namespace::find_by_id(*id) {
            if other.case != namespace.case {
                log::warn!("Configuration mismatch: namespace {id} letter case does not match");
            }
            if other.name != namespace.name {
                log::warn!(
                    "Configuration mismatch: namespace {id} names do not match (config: {:?} database: {:?})",
                    other.name,
                    namespace.name
                );
            }
        } else {
            log::warn!("Configuration mismatch: missing namespace {id}");
        }
    }

    let renderer = r2d2::Builder::new()
        .max_size(1)
        .test_on_check_out(false)
        .max_lifetime(None)
        .idle_timeout(None)
        .build_unchecked(RenderManager::new(&base_uri, &database));
    let state = AppState::new(WikiState {
        base_uri,
        database,
        load_mode: args.load_mode,
        renderer,
    });

    let app = Router::new()
        .route("/article/{*name}", get(pages::article))
        .route("/eval", get(pages::eval_get).post(pages::eval_post))
        .route("/external/{*target}", get(pages::external))
        .route("/fonts/{*font}", get(pages::fonts))
        .route("/images/{*image}", get(pages::images))
        .route("/search", get(pages::search))
        .route("/source/{*name}", get(pages::source));

    // TODO: This is just for debugging to avoid having to restart the server
    // just to check CSS changes. Also there is probably a less dumb way to do
    // this that just allows the pages::styles to be a fallback, but ServeFile
    // does not expose things that make it easy and I do not feel like reading
    // more axum documentation right now. Priorities?!
    #[cfg(feature = "debug-styles")]
    let app = app.route_service(
        "/styles.css",
        tower_http::services::ServeFile::new("res/styles.css"),
    );
    #[cfg(not(feature = "debug-styles"))]
    let app = app.route("/styles.css", get(pages::styles));

    let app = app.route("/", get(pages::index_page)).with_state(state);

    let listener = TcpListener::bind(&args.bind).await?;
    log::info!("Listening at {}", args.bind);

    axum::serve(listener, app).await.map_err(Into::into)
}

/// Uses the [`Display`](core::fmt::Display) formatter for an error even when
/// the [`Debug`](core::fmt::Debug) formatter is requested.
struct DisplayError(Box<dyn std::error::Error>);

impl core::fmt::Debug for DisplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

impl<E: Into<Box<dyn std::error::Error>>> From<E> for DisplayError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

fn main() -> Result<(), DisplayError> {
    run().map_err(Into::into)
}
