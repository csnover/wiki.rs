use crate::resource::ResourceManager;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use kata::TemplateContext;
use rayon::iter::ParallelIterator;
use renderer::ArticleRenderer;
use std::{ffi::OsStr, sync::Arc, time::Instant};
use tokio::net::TcpListener;
use db::{Database, article::Error as ArticleError};

mod db;
mod pages;
mod renderer;
mod resource;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Article(#[from] ArticleError),
    #[error(transparent)]
    Resource(#[from] resource::Error),
    #[error(transparent)]
    Renderer(#[from] renderer::Error),
    #[error(transparent)]
    Kana(#[from] kata::RenderError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::Article(error) => match error {
                ArticleError::ArticleNotFound => (StatusCode::NOT_FOUND, format!("{error}")),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            },
            Error::Resource(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::Renderer(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::Kana(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
        }
        .into_response()
    }
}

struct WikiState<'a> {
    database: db::Database<'a>,
    resources: ResourceManager,
}

type AppState<'a> = Arc<WikiState<'a>>;

async fn resource(
    State(state): State<AppState<'_>>,
    Path(name): Path<String>,
) -> Result<Response, StatusCode> {
    state
        .resources
        .find_resource(&name)
        .map(Into::into)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn article(
    State(state): State<AppState<'_>>,
    Path(name): Path<String>,
) -> Result<Html<String>, Error> {
    let name = name.replace('_', " ");

    let article = state.database.get(&name)?;

    let time = Instant::now();

    let template = state.resources.find_template("article.html")?;
    let body = ArticleRenderer::render(&state.database, &article)?;
    let mut ctx = TemplateContext::new();
    ctx.set_str("body", &body);
    ctx.set_str("title", &article.title);
    let html = template.render(&ctx)?;

    log::trace!("Rendered article in {:.2?}", time.elapsed());

    Ok(html.into())
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<AppState<'_>>,
    Query(SearchQuery { q: query }): Query<SearchQuery>,
) -> Result<Html<String>, Error> {
    let query = regex::RegexBuilder::new(&query)
        .case_insensitive(true)
        .build()
        .unwrap();

    log::debug!("Searching for {query}");
    let time = Instant::now();
    let mut results = state.database.search(&query).collect::<Vec<&str>>();
    log::trace!("Found {} matches in {:.2?}", results.len(), time.elapsed());
    results.sort_unstable();
    log::trace!("Sorted results in {:.2?}", time.elapsed());

    let count = format!("{}", results.len());
    let template = state.resources.find_template("search.html")?;
    let mut ctx = TemplateContext::new();
    ctx.set_str("query", query.as_str());
    ctx.set_str("count", &count);
    ctx.set_str_array("results", &results);
    template.render(&ctx).map(Html::from).map_err(Into::into)
}

async fn index_page(State(state): State<AppState<'_>>) -> Result<Html<String>, Error> {
    state
        .resources
        .find_template("index.html")?
        .render(&kata::TemplateContext::new())
        .map(Html::from)
        .map_err(Into::into)
}

fn usage() {
    let exe = std::env::args().next().unwrap_or_default();
    eprintln!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    eprintln!("Usage: {exe} [options] <index.txt> <database.xml.bz2>\n");
    eprintln!("or, use environment variables:");
    eprintln!("    WIKI_INDEX_FILE");
    eprintln!("    WIKI_ARTICLE_DB\n");
    eprintln!("Options:");
    eprintln!("    --bind: Web server bind (default: 127.0.0.1:3000)\n");
}

fn free_arg(
    args: &mut pico_args::Arguments,
    key: &str,
    err: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(arg) = args.opt_free_from_str::<String>()? {
        Ok(arg)
    } else if let Ok(arg) = std::env::var(key) {
        Ok(arg)
    } else {
        Err(err.into())
    }
}

struct Args {
    bind: String,
    index_path: String,
    articles_path: String,
}

impl Args {
    fn new() -> Result<Args, Box<dyn std::error::Error>> {
        let mut args = pico_args::Arguments::from_env();
        let bind = args
            .opt_value_from_str("--bind")?
            .unwrap_or_else(|| "127.0.0.1:3000".to_string());
        let _ = args.contains("--");
        let index_path = free_arg(&mut args, "WIKI_INDEX_FILE", "Missing index file argument")?;
        let articles_path = free_arg(
            &mut args,
            "WIKI_ARTICLE_DB",
            "Missing article database argument",
        )?;

        let rest = args.finish();
        if !rest.is_empty() {
            return Err(format!(
                "Unknown arguments: {}",
                rest.join(OsStr::new(" ")).display()
            )
            .into());
        }

        Ok(Self {
            bind,
            index_path,
            articles_path,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args = match Args::new() {
        Ok(args) => args,
        Err(err) => {
            usage();
            return Err(err);
        }
    };

    log::info!("Starting up wiki.rs ...");

    let mut resources = ResourceManager::new();
    resources.register_template("index.html", include_bytes!("../res/index.html"))?;
    resources.register_template("article.html", include_bytes!("../res/article.html"))?;
    resources.register_template("search.html", include_bytes!("../res/search.html"))?;
    resources.register_resource("styles.css", include_bytes!("../res/styles.css"))?;

    let state = Arc::new(WikiState {
        database: Database::from_file(&args.index_path, &args.articles_path)?,
        resources,
    });

    let app = Router::new()
        .route("/res/{name}", get(resource))
        .route("/article/{*name}", get(article))
        .route("/search", get(search))
        .route("/", get(index_page))
        .with_state(state);

    let listener = TcpListener::bind(&args.bind).await?;
    log::info!("Listening at {}", args.bind);

    axum::serve(listener, app).await.map_err(Into::into)
}
