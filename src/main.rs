// #![windows_subsystem = "windows"]

use std::{sync::Arc, time::Instant};

use crate::{
    pages::{render_article_page, render_results_page},
    resource::ResourceManager,
    wiki::{article::ArticleDatabase, index::Index}
};
use axum::{extract::{Path, Query, State}, http::StatusCode, response::{Html, Response}, routing::get, Router};
use tokio::net::TcpListener;

mod pages;
mod renderer;
mod resource;
mod wiki;

struct WikiState {
    index: Index,
    article_db: ArticleDatabase,
    resources: ResourceManager,
}

type AppState = Arc<WikiState>;

async fn get_resource(State(state): State<AppState>, Path(name): Path<String>) -> Result<Response, StatusCode> {
    state.resources.find_resource(&name).map(Into::into).ok_or(StatusCode::NOT_FOUND)
}

async fn get_article(State(state): State<AppState>, Path(name): Path<String>) -> Result<Html<String>, StatusCode> {
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
struct SearchQuery { q: String }

async fn search(State(state): State<AppState>, Query(SearchQuery { q: query }): Query<SearchQuery>) -> Html<String> {
    let results = state.index.find_article(&query);
    render_results_page(&state.resources, &query, &results).into()
}

async fn index_page() -> Html<String> {
    Html("Hello".into())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting up wiki.rs ...");

    let index_path = std::env::var("WIKI_INDEX_FILE")?;
    let articles_path = std::env::var("WIKI_ARTICLE_DB")?;

    let index = Index::from_file(&index_path)?;
    let article_db = ArticleDatabase::from_file(&articles_path)?;

    println!("Loaded {} articles from index", index.size());

    let mut resources = ResourceManager::new();
    resources.register_template("article.html", include_bytes!("../res/article.html"));
    resources.register_template("search.html", include_bytes!("../res/search.html"));
    resources.register_resource("styles.css", include_bytes!("../res/styles.css"));

    let state = Arc::new(WikiState {
        index,
        article_db,
        resources
    });

    let app = Router::new()
        .route("/res/{name}", get(get_resource))
        .route("/article/{*name}", get(get_article))
        .route("/search", get(search))
        .route("/", get(index_page))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();

    println!("Listening at 127.0.0.1:3000");

    axum::serve(listener, app).await?;

    Ok(())
}
