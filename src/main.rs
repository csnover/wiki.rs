use std::{ffi::OsStr, sync::Arc, time::Instant};

use crate::{
    pages::{render_article_page, render_results_page},
    resource::ResourceManager,
    wiki::{article::ArticleDatabase, index::Index},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Response},
    routing::get,
};
use tokio::net::TcpListener;

mod pages;
mod renderer;
mod resource;
mod wiki;

struct WikiState<'a> {
    index: Index<'a>,
    article_db: ArticleDatabase,
    resources: ResourceManager,
}

type AppState<'a> = Arc<WikiState<'a>>;

async fn get_resource(
    State(state): State<AppState<'_>>,
    Path(name): Path<String>,
) -> Result<Response, StatusCode> {
    state
        .resources
        .find_resource(&name)
        .map(Into::into)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_article(
    State(state): State<AppState<'_>>,
    Path(name): Path<String>,
) -> Result<Html<String>, StatusCode> {
    println!("Loading article {name}");
    let time = Instant::now();

    let name_cleaned = name.replace("_", " ");
    let article = state.index.find_article_exact(&name_cleaned);
    if article.is_none() {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }

    let article = article.unwrap();
    println!("Located article in {:.2?}", time.elapsed());

    let time = Instant::now();
    let article_data = state.article_db.get_article(article).unwrap();
    println!("Extracted article in {:.2?}", time.elapsed());

    let time = Instant::now();
    let article_html = render_article_page(&state.resources, &article_data);
    println!("Rendered article in {:.2?}", time.elapsed());

    Ok(article_html.into())
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<AppState<'_>>,
    Query(SearchQuery { q: query }): Query<SearchQuery>,
) -> Html<String> {
    let query = regex::RegexBuilder::new(&query)
        .case_insensitive(true)
        .build()
        .unwrap();
    let results = state.index.find_article(&query);
    render_results_page(&state.resources, query.as_str(), results).into()
}

async fn index_page(State(state): State<AppState<'_>>) -> Html<String> {
    state
        .resources
        .find_template("index.html")
        .expect("Failed to find index template")
        .render(&kata::TemplateContext::new())
        .expect("Failed to render search template")
        .into()
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
    let args = match Args::new() {
        Ok(args) => args,
        Err(err) => {
            usage();
            return Err(err);
        }
    };

    println!("Starting up wiki.rs ...");

    let time = Instant::now();
    let index = Index::from_file(&args.index_path)?;
    println!("Read index in {:.2?}", time.elapsed());

    let article_db = ArticleDatabase::from_file(&args.articles_path)?;
    println!("Loaded {} articles from index", index.len());

    let mut resources = ResourceManager::new();
    resources.register_template("index.html", include_bytes!("../res/index.html"));
    resources.register_template("article.html", include_bytes!("../res/article.html"));
    resources.register_template("search.html", include_bytes!("../res/search.html"));
    resources.register_resource("styles.css", include_bytes!("../res/styles.css"));

    let state = Arc::new(WikiState {
        index,
        article_db,
        resources,
    });

    let app = Router::new()
        .route("/res/{name}", get(get_resource))
        .route("/article/{*name}", get(get_article))
        .route("/search", get(search))
        .route("/", get(index_page))
        .with_state(state);

    let listener = TcpListener::bind(&args.bind).await?;
    println!("Listening at {}", args.bind);

    axum::serve(listener, app).await.map_err(Into::into)
}
