use std::{sync::Arc, time::Instant};

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

fn usage<T>(err: &'static str) -> anyhow::Result<T> {
    let exe = std::env::args().next().unwrap_or_default();
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    println!("Usage: {exe} [options] <index.txt> <database.xml.bz2>\n");
    println!("or, use environment variables:");
    println!("    WIKI_INDEX_FILE");
    println!("    WIKI_ARTICLE_DB\n");
    println!("Options:");
    println!("    --bind: Web server bind (default: 127.0.0.1:3000)\n");
    Err(anyhow::Error::msg(err))
}

fn free_arg(
    args: &mut pico_args::Arguments,
    key: &str,
    err: &'static str,
) -> anyhow::Result<String> {
    if let Some(arg) = args.opt_free_from_str::<String>()? {
        Ok(arg)
    } else if let Ok(arg) = std::env::var(key) {
        Ok(arg)
    } else {
        usage(err)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let listen = args
        .opt_value_from_str("--listen")?
        .unwrap_or_else(|| "127.0.0.1:3000".to_string());
    let _ = args.contains("--");
    let index_path = free_arg(&mut args, "WIKI_INDEX_FILE", "Missing index file argument")?;
    let articles_path = free_arg(
        &mut args,
        "WIKI_ARTICLE_DB",
        "Missing article database argument",
    )?;

    if !args.finish().is_empty() {
        return usage("Unknown extra arguments passed");
    }

    println!("Starting up wiki.rs ...");

    let index = Index::from_file(&index_path)?;
    let article_db = ArticleDatabase::from_file(&articles_path)?;

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

    let listener = TcpListener::bind(&listen).await?;
    println!("Listening at {listen}");

    axum::serve(listener, app).await.map_err(Into::into)
}
