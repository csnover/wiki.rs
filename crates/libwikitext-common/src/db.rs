//! Database traits and types.

use super::{
    config::Configuration,
    lru_limiter::HeapUsageCalculator,
    title::{NamespaceCase, Title},
};
use indexmap::IndexSet;
use std::{collections::HashMap, sync::Arc};
use time::UtcDateTime;

/// A single MediaWiki article.
#[derive(Debug, Clone)]
pub struct Article {
    /// The article ID. (This is *not* the revision ID.)
    pub id: u64,
    /// The title of the article. This may contain a namespace name.
    pub title: String,
    /// The content of the article.
    ///
    /// This is arbitrary text content which must be interpreted according to
    /// the article’s [data model](Self::model).
    pub body: String,
    /// The data model of the article. This is usually "wikitext", but can be
    /// "json" for JSON data, "Scribunto" for Lua modules, etc.
    pub model: String,
    /// If this article is a redirection to another article, the title of the
    /// destination article.
    pub redirect: Option<String>,
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
    /// Article was not found.
    #[error("requested article not found")]
    NotFound,

    /// Some other error occurred.
    #[error(transparent)]
    Backend(Box<dyn core::error::Error + Send + Sync + 'static>),
}

/// A trait for implementing database backends.
#[expect(
    clippy::len_without_is_empty,
    reason = "knowing a database is empty is not useful information"
)]
pub trait IDatabase {
    /// Returns the current memory usage of the cache, in bytes.
    fn cache_size(&self) -> usize;

    /// Returns the configuration data for the database.
    fn config(&self) -> &Configuration;

    /// Returns true if the database contains an article with the given title.
    fn contains(&self, title: &Title) -> bool;

    /// The guessed creation date of the database.
    fn creation_date(&self) -> Option<UtcDateTime>;

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

    /// The registered namespaces in the database.
    fn namespaces(&self) -> &HashMap<i32, DatabaseNamespace>;

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

impl IDatabase for Arc<dyn IDatabase> {
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
    fn creation_date(&self) -> Option<UtcDateTime> {
        (**self).creation_date()
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
    fn namespaces(&self) -> &HashMap<i32, DatabaseNamespace> {
        (**self).namespaces()
    }

    #[inline]
    fn prefetch_all(&self, templates: IndexSet<Title>, links: IndexSet<Title>) {
        (**self).prefetch_all(templates, links);
    }
}

/// A database namespace.
pub struct DatabaseNamespace {
    /// The letter casing of the namespace name.
    pub case: NamespaceCase,
    /// The name of the namespace.
    pub name: String,
}
