use kata::TemplateContext;

use crate::{
    renderer::ArticleRenderer,
    resource::ResourceManager,
    wiki::{article::Article, index::IndexEntry},
};
use rayon::iter::ParallelIterator;
use std::time::Instant;

pub fn render_article_page(resources: &ResourceManager, article: &Article) -> String {
    let mut renderer = ArticleRenderer::new();
    renderer.render_article_body(article);

    let template = resources
        .find_template("article.html")
        .expect("Failed to find article template");

    let mut ctx = TemplateContext::new();
    ctx.set_str("body", renderer.html());
    ctx.set_str("title", &article.title);

    template
        .render(&ctx)
        .expect("Failed to render article template")
}

pub fn render_results_page<'a>(
    resources: &ResourceManager,
    query: &str,
    index_entries: impl ParallelIterator<Item = &'a IndexEntry<'a>>,
) -> String {
    println!("Searching for {query}");
    let time = Instant::now();
    let mut results = index_entries
        .map(|entry| entry.page_name)
        .collect::<Vec<&str>>();
    println!("Found {} matches in {:.2?}", results.len(), time.elapsed());
    let time = Instant::now();
    results.sort_unstable();
    println!("Sorted results in {:.2?}", time.elapsed());
    let count = format!("{}", results.len());

    let template = resources
        .find_template("search.html")
        .expect("Failed to find search template");

    let mut ctx = TemplateContext::new();
    ctx.set_str("query", query);
    ctx.set_str("count", &count);
    ctx.set_str_array("results", &results);

    template
        .render(&ctx)
        .expect("Failed to render search template")
}
