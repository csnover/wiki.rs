//! Database traits and types.

use super::{config::Configuration, lru_limiter::HeapUsageCalculator, title::Title};
use indexmap::IndexSet;
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use time::UtcDateTime;

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
#[derive(Clone)]
pub struct Article {
    /// All article string data.
    data: String,
    /// The article ID. (This is *not* the revision ID.)
    id: u64,
    /// The end position of the data model section in [`Self::data`].
    model: u16,
    /// The end position of the redirect section in [`Self::data`].
    redirect: u16,
    /// The end position of the restrictions section in [`Self::data`].
    restrictions: u16,
    /// The end position of the revision author in [`Self::data`].
    revision_author: u16,
    /// The revision ID.
    revision_id: u64,
    /// The timestamp of the revision.
    revision_timestamp: UtcDateTime,
    /// The end position of the title section, and the start position of the
    /// body text, in [`Self::data`].
    title: u16,
}

impl Article {
    /// Creates a builder for building an `Article`.
    #[inline]
    pub fn builder<'a>() -> ArticleBuilder<'a> {
        ArticleOptions::builder()
    }

    /// Gets the content of the article.
    ///
    /// This is arbitrary text content which must be interpreted according to
    /// the article’s [data model](fn@Self::model).
    #[inline]
    #[must_use]
    pub fn body(&self) -> &str {
        &self.data[usize::from(self.title)..]
    }

    /// Gets the article ID.
    #[inline]
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Gets the data model of the article. This is usually "wikitext", but can
    /// be "json" for JSON data, "Scribunto" for Lua modules, etc.
    #[inline]
    #[must_use]
    pub fn model(&self) -> &str {
        &self.data[..usize::from(self.model)]
    }

    /// Gets the title of the destination article, if this article is a
    /// redirection to another article.
    #[must_use]
    pub fn redirect(&self) -> Option<&str> {
        let target = &self.data[usize::from(self.model)..usize::from(self.redirect)];
        (!target.is_empty()).then_some(target)
    }

    /// Replaces the body text.
    #[inline]
    pub fn replace_body<F: FnOnce(&str) -> Cow<'_, str>>(&mut self, repl: F) {
        let range = usize::from(self.title)..;
        if let Cow::Owned(body) = repl(&self.data[range.clone()]) {
            self.data.replace_range(range, &body);
        }
    }

    /// Gets any access restriction for the given action.
    #[must_use]
    pub fn restriction(&self, action: &str) -> Option<&str> {
        let restrictions = self.restrictions()?;
        restrictions.split(':').find_map(|restriction| {
            let (candidate, restriction) = restriction.split_once('=')?;
            (action == candidate).then_some(restriction)
        })
    }

    /// Gets the raw restrictions list.
    #[inline]
    fn restrictions(&self) -> Option<&str> {
        let restrictions = &self.data[usize::from(self.redirect)..usize::from(self.restrictions)];
        (!restrictions.is_empty()).then_some(restrictions)
    }

    /// Gets the revision ID.
    #[inline]
    #[must_use]
    pub fn revision_id(&self) -> u64 {
        self.revision_id
    }

    /// Gets the author of this article revision.
    #[inline]
    #[must_use]
    pub fn revision_author(&self) -> &str {
        &self.data[usize::from(self.restrictions)..usize::from(self.revision_author)]
    }

    /// Gets the creation date of this article revision.
    #[inline]
    #[must_use]
    pub fn revision_timestamp(&self) -> UtcDateTime {
        self.revision_timestamp
    }

    /// Gets the title of the article. This may contain a namespace name.
    #[inline]
    #[must_use]
    pub fn title(&self) -> &str {
        &self.data[usize::from(self.revision_author)..usize::from(self.title)]
    }
}

impl core::fmt::Debug for Article {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Article")
            .field("id", &self.id())
            .field("model", &self.model())
            .field("redirect", &self.redirect())
            .field("restrictions", &self.restrictions())
            .field("revision_author", &self.revision_author())
            .field("revision_id", &self.revision_id())
            .field("revision_timestamp", &self.revision_timestamp())
            .field("title", &self.title())
            .field("body", &self.body())
            .finish()
    }
}

/// Intermediate builder structure for `Article`.
// The docs for TypedBuilder are atrocious, so as far as I can tell it is
// impossible to do this kind of memory optimising transformation directly.
// Otherwise this would just a builder right on `Article`.
#[derive(typed_builder::TypedBuilder)]
#[builder(builder_type(name = ArticleBuilder, vis = "pub"), build_method(into = Article))]
struct ArticleOptions<'a> {
    /// The body text.
    body: &'a str,
    /// The article ID.
    id: u64,
    /// The data model.
    #[builder(default)]
    model: &'a str,
    /// The redirect target.
    #[builder(default)]
    redirect: &'a str,
    /// The access restrictions.
    #[builder(default)]
    restrictions: &'a str,
    /// The revision author.
    #[builder(default)]
    revision_author: &'a str,
    /// The revision ID.
    revision_id: u64,
    /// The revision timestamp.
    #[builder(default = UtcDateTime::UNIX_EPOCH)]
    revision_timestamp: UtcDateTime,
    /// The article title.
    title: &'a str,
}

impl From<ArticleOptions<'_>> for Article {
    fn from(value: ArticleOptions<'_>) -> Self {
        let mut data = String::new();

        let mut append = |value: &str| -> u16 {
            data += value;
            u16::try_from(data.len()).unwrap()
        };

        let model = append(value.model);
        let redirect = append(value.redirect);
        let restrictions = append(value.restrictions);
        let revision_author = append(value.revision_author);
        let title = append(value.title);
        data += value.body;
        Self {
            data,
            id: value.id,
            model,
            redirect,
            restrictions,
            revision_author,
            revision_id: value.revision_id,
            revision_timestamp: value.revision_timestamp,
            title,
        }
    }
}

impl HeapUsageCalculator for Article {
    #[inline]
    fn size_of(&self) -> usize {
        self.data.capacity()
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
}

impl<'config> MockDatabase<'config> {
    /// Creates a new database using the given `config`.
    #[inline]
    #[must_use]
    pub fn new(config: &'config Configuration) -> Self {
        Self {
            articles: <_>::default(),
            config,
        }
    }

    /// Inserts an `article` to the database.
    pub fn insert(&mut self, article: Article) {
        let title = Title::new(self.config, article.title(), None)
            .key()
            .to_owned();
        self.articles.insert(title.clone(), Arc::new(article));
    }

    /// Removes an `article` with the given title from the database.
    pub fn remove(&mut self, title: &str) {
        let title = Title::new(self.config, title, None);
        self.articles.remove(title.key());
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
