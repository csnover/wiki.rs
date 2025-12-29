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
use std::{ffi::OsStr, sync::Arc, time::Duration};
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

/// Time and memory limits.
#[derive(Clone, Copy, Debug)]
struct Limits {
    /// Database decompression cache size limit, in bytes. One per process.
    db_cache: usize,
    /// Template token tree cache size limit, in bytes. One per process.
    template_cache: usize,
    /// Maximum number of renderer threads.
    threads: u32,
    /// Lua single call time limit.
    vm_time: Duration,
    /// Lua VM total memory limit, in bytes. One per renderer thread.
    vm_total_mem: usize,
}

impl core::fmt::Display for Limits {
    // Clippy: If memory limits are ever >2**52, something sure happened.
    #[allow(clippy::cast_precision_loss)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let db_cache = self.db_cache as f64 / 1024.0;
        let template_cache = self.template_cache as f64 / 1024.0;
        let vm_total_mem = self.vm_total_mem as f64 / 1024.;
        writeln!(f, "Resource limits:")?;
        writeln!(f, "  Database cache:         {db_cache:.2}KiB")?;
        writeln!(f, "  Template cache:         {template_cache:.2}KiB")?;
        writeln!(f, "  Threads:                {}", self.threads)?;
        writeln!(f, "  VM memory (per thread): {vm_total_mem:.2}KiB")?;
        writeln!(f, "  VM time (per call):     {:?}", self.vm_time)?;
        Ok(())
    }
}

/// Page rendering strategy.
///
/// This exists purely as a performance optimisation. If first time to paint
/// could be guaranteed to be under one second for all pages, this could be
/// eliminated.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LoadMode {
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
pub(crate) struct LoadModeError(String);

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
    /// Byte size conversion error.
    #[error(
        "unknown byte size unit '{0}' (should be '', 'b', 'B', 'k', 'K', 'm', 'M', 'g', or 'G')"
    )]
    ByteSize(String),
    /// Missing the database argument.
    #[error("missing multistream.xml.bz2 argument")]
    Database,
    /// Duration conversion error.
    #[error("unknown duration unit '{0}' (should be 'ms' or 's')")]
    Duration(String),
    /// Extra unknown junk on the command line.
    #[error("unknown arguments: {}", _0.display())]
    Extra(std::ffi::OsString),
    /// Missing the index argument.
    #[error("missing index.txt argument")]
    Index,
    /// Float parsing error.
    #[error(transparent)]
    ParseFloat(#[from] core::num::ParseFloatError),
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
    /// Configurable resource limits.
    limits: Limits,
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

    /// Parses a time duration string in the format `\d+(\.\d+)?\s*(m?s)`.
    fn parse_duration(value: &str) -> Result<Duration, ArgsError> {
        let (number, unit) = Self::parse_number_with_unit(value)?;
        if unit.eq_ignore_ascii_case("ms") {
            // Clippy: Sub-millisecond precision is not needed here, and the
            // number is guaranteed to not have a sign.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(Duration::from_millis(number as u64))
        } else if unit.eq_ignore_ascii_case("s") {
            Ok(Duration::from_secs_f64(number))
        } else {
            Err(ArgsError::Duration(unit.to_string()))
        }
    }

    /// Parses a number in the format `\d+(\.\d+)?` and returns the remainder as
    /// a unit to be processed by the caller.
    fn parse_number_with_unit(value: &str) -> Result<(f64, &str), ArgsError> {
        let value = value.trim_ascii();
        let end = value
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(value.len());
        let number = value[..end].parse::<f64>()?;
        let unit = value[end..].trim_ascii_start();
        Ok((number, unit))
    }

    /// Parses a byte size in the format `\d+(\.\d+)?\s*[BbKkMmGg]?`
    fn parse_size(value: &str) -> Result<usize, ArgsError> {
        let (number, unit) = Self::parse_number_with_unit(value)?;
        // Clippy: Truncation is desirable and the number is guaranteed to not
        // have a sign.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(match unit {
            "" | "b" | "B" => number,
            "k" => number * 1000.0,
            "K" => number * 1024.0,
            "m" => number * 1_000_000.0,
            "M" => number * 1024.0 * 1024.0,
            "g" => number * 1_000_000_000.0,
            "G" => number * 1024.0 * 1024.0 * 1024.0,
            _ => return Err(ArgsError::ByteSize(unit.to_string())),
        } as usize)
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

        let db_cache = args
            .opt_value_from_fn("--db-cache", Self::parse_size)?
            .unwrap_or(32 * 1024 * 1024);
        let template_cache = args
            .opt_value_from_fn("--template-cache", Self::parse_size)?
            .unwrap_or(32 * 1024 * 1024);
        let vm_time = args
            .opt_value_from_fn("--vm-time", Self::parse_duration)?
            .unwrap_or(Duration::new(10, 0));
        let vm_total_mem = args
            .opt_value_from_fn("--vm-total-mem", Self::parse_size)?
            .unwrap_or(128 * 1024 * 1024);
        let threads = args.opt_value_from_str("--threads")?.unwrap_or(1);

        let rest = args.finish();
        if !rest.is_empty() {
            return Err(ArgsError::Extra(rest.join(OsStr::new(" "))));
        }

        Ok(Self {
            articles_path,
            base_uri,
            bind,
            limits: Limits {
                db_cache,
                template_cache,
                threads,
                vm_time,
                vm_total_mem,
            },
            index_path,
            load_mode,
        })
    }
}

/// Command line usage instructions.
fn usage() {
    let exe = std::env::args().next().unwrap_or_default();
    eprintln!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    eprintln!("Usage: {exe} [options] <index.txt> <database.xml.bz2>\n");
    eprintln!("or, use environment variables:");
    eprintln!("  WIKI_INDEX_FILE");
    eprintln!("  WIKI_ARTICLE_DB\n");
    eprintln!("Options:");
    eprintln!("  Network:");
    eprintln!("    --base-uri: Base URI for site (default: bind address)");
    eprintln!("    --bind: Web server bind (default: 127.0.0.1:3000)");
    eprintln!("  CPU:");
    eprintln!("    --mode <mode>: Initial page rendering mode. One of:");
    eprintln!("      * 'base': Render only the base Wikitext");
    eprintln!("      * 'template': Expand templates (default)");
    eprintln!("      * 'module': Expand templates and run Lua modules");
    eprintln!("    --threads: Max number of renderer threads (default: 1)");
    eprintln!("    --vm-time: Max Lua VM single call execution time (default: 10s)");
    eprintln!("  Memory:");
    eprintln!("    --db-cache: Max decompressed article cache size (default: 32M)");
    eprintln!("    --template-cache: Max template cache size (default: 32M)");
    eprintln!("    --vm-total-mem: Max Lua VM memory usage (per thread) (default: 128M)");
}

/// Don’t run this. You’ve been warned!
#[tokio::main]
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

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

    let limits = args.limits;

    log::info!("{limits}");

    let database = Arc::new(Database::from_file(
        &args.index_path,
        &args.articles_path,
        limits.db_cache,
    )?);

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

    if let Some(date) = database.creation_date() {
        log::info!("Database version (guessed from filename): {}", date.date());
    } else {
        // Some things, like 'Module:Selected recent additions', use the
        // “current” date to look up articles that it assumes will always exist
        // for the “current” month, so this needs to be mocked for those things
        // to work properly
        log::warn!(
            "Could not determine the database creation date; some pages may be missing data"
        );
    }

    let renderer = r2d2::Builder::new()
        .max_size(limits.threads)
        .test_on_check_out(false)
        .max_lifetime(None)
        .idle_timeout(None)
        .build_unchecked(RenderManager::new(&base_uri, &database, limits));
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
        .route("/media/{*image}", get(pages::media))
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
