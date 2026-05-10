//! Database traits and types.

use super::{config::Configuration, lru_limiter::HeapUsageCalculator, title::Title};
use indexmap::IndexSet;
use std::{collections::HashMap, sync::Arc};

/// A trait for implementing database backends.
#[expect(
    clippy::len_without_is_empty,
    reason = "knowing a database is empty is not useful information"
)]
pub trait DatabaseProvider {
    /// Returns the current memory usage of the cache, in bytes.
    fn cache_size(&self) -> usize;

    /// Returns the configuration data for the database.
    fn config(&self) -> &Configuration;

    /// Returns true if the database contains an article with the given title.
    fn contains(&self, title: &Title) -> bool;

    /// Gets an article with the given title from the database. The article will
    /// be cached in memory.
    ///
    /// # Errors
    ///
    /// * An article with the given `title` does not exist
    /// * The database implementation returns an error
    fn get(&self, title: &Title) -> Result<Arc<Article>, Error>;

    /// The total number of articles in the database.
    fn len(&self) -> usize;

    /// The site name from the database.
    fn name(&self) -> &str;

    /// Prefetches a collection of titles.
    ///
    /// Because the MW database dump index is totally unordered, finding a title
    /// in the index requires a full table scan. Batching titles into request
    /// sets reduces the number of scans required, increasing performance.
    ///
    /// Both templates and links need to check for existence in the index, but
    /// templates are both more time-critical and also require decompressing
    /// article data, so they are collected separately.
    fn prefetch_all(&self, templates: IndexSet<Title>, links: IndexSet<Title>);
}

impl DatabaseProvider for Arc<dyn DatabaseProvider> {
    #[inline]
    fn cache_size(&self) -> usize {
        (**self).cache_size()
    }

    #[inline]
    fn config(&self) -> &Configuration {
        (**self).config()
    }

    #[inline]
    fn contains(&self, title: &Title) -> bool {
        (**self).contains(title)
    }

    #[inline]
    fn get(&self, title: &Title) -> Result<Arc<Article>, Error> {
        (**self).get(title)
    }

    #[inline]
    fn len(&self) -> usize {
        (**self).len()
    }

    #[inline]
    fn name(&self) -> &str {
        (**self).name()
    }

    #[inline]
    fn prefetch_all(&self, templates: IndexSet<Title>, links: IndexSet<Title>) {
        (**self).prefetch_all(templates, links);
    }
}

/// A single MediaWiki article.
#[derive(Debug, Clone)]
pub struct Article {
    /// The content of the article.
    ///
    /// This is arbitrary text content which must be interpreted according to
    /// the article’s [data model](Self::model).
    pub body: String,
    /// The article ID. (This is *not* the revision ID.)
    pub id: u64,
    /// The data model of the article. This is usually "wikitext", but can be
    /// "json" for JSON data, "Scribunto" for Lua modules, etc.
    pub model: String,
    /// If this article is a redirection to another article, the title of the
    /// destination article.
    pub redirect: Option<String>,
    /// The title of the article. This may contain a namespace name.
    pub title: String,
}

impl HeapUsageCalculator for Article {
    #[inline]
    fn size_of(&self) -> usize {
        self.title.capacity()
            + self.body.capacity()
            + self.model.capacity()
            + self.redirect.as_ref().map_or(0, String::capacity)
    }
}

/// A common database error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Some other error occurred.
    #[error(transparent)]
    Backend(Box<dyn core::error::Error + Send + Sync + 'static>),

    /// Article was not found.
    #[error("requested article not found")]
    NotFound,
}

/// A fake database used for testing.
pub struct MockDatabase<'config> {
    /// The mock articles.
    articles: HashMap<String, Arc<Article>>,
    /// The mock configuration.
    config: &'config Configuration,
    /// The article ID for the next inserted article.
    next_id: u64,
}

impl<'config> MockDatabase<'config> {
    /// Creates a new database using the given `config`.
    #[inline]
    #[must_use]
    pub fn new(config: &'config Configuration) -> Self {
        Self {
            articles: <_>::default(),
            config,
            next_id: 1,
        }
    }

    /// Inserts a Wikitext article with the given `title` and `body` text.
    pub fn insert(&mut self, title: &str, body: &str) {
        let title = Title::new(self.config, title, None).key().to_owned();
        let id = self.next_id;
        self.next_id += 1;
        self.articles.insert(
            title.clone(),
            Arc::new(Article {
                body: body.into(),
                id,
                model: "wikitext".into(),
                redirect: None,
                title,
            }),
        );
    }
}

impl DatabaseProvider for MockDatabase<'_> {
    #[inline]
    fn cache_size(&self) -> usize {
        0
    }

    #[inline]
    fn config(&self) -> &Configuration {
        self.config
    }

    #[inline]
    fn contains(&self, title: &Title) -> bool {
        self.articles.contains_key(title.key())
    }

    fn get(&self, title: &Title) -> Result<Arc<Article>, Error> {
        self.articles
            .get(title.key())
            .cloned()
            .ok_or(Error::NotFound)
    }

    #[inline]
    fn len(&self) -> usize {
        self.articles.len()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Mock"
    }

    #[inline]
    fn prefetch_all(&self, _templates: IndexSet<Title>, _links: IndexSet<Title>) {}
}
