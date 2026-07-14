//! Database traits and types.

use super::{
    config::Configuration,
    lru_limiter::HeapUsageCalculator,
    title::{Namespace, Title},
};
use indexmap::IndexSet;
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use time::UtcDateTime;

/// A trait for implementing database backends.
pub trait DatabaseProvider {
    /// The type used for errors.
    type Error;

    /// Returns the current memory usage of the cache, in bytes.
    fn cache_size(&self) -> usize;

    /// Returns the configuration data for the database.
    fn config(&self) -> &Configuration;

    /// Returns true if the database contains an article or file with the given
    /// title.
    fn contains(&self, title: &Title) -> bool;

    /// Gets an article with the given title from the database. The article will
    /// be cached in memory.
    ///
    /// # Errors
    ///
    /// * The database implementation returns an error
    fn get(&self, title: &Title) -> Result<Option<Arc<Article>>, Self::Error>;

    /// Returns true if the database is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The total number of articles in the database.
    fn len(&self) -> usize;

    /// Gets file metadata for the given title from the database.
    ///
    /// # Errors
    ///
    /// * The database implementation returns an error
    fn metadata(&self, title: &Title) -> Result<Option<FileMetadata>, Self::Error>;

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

/// A type-erased database provider error.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct BoxedDbError(Box<dyn core::error::Error + Send + Sync + 'static>);

/// A database provider with an erased error type.
pub trait DynDatabaseProvider: private::Sealed {
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
    /// * The database implementation returns an error
    fn get(&self, title: &Title) -> Result<Option<Arc<Article>>, BoxedDbError>;

    /// Returns true if the database is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The total number of articles in the database.
    fn len(&self) -> usize;

    /// Gets file metadata for the given title from the database.
    ///
    /// # Errors
    ///
    /// * The database implementation returns an error
    fn metadata(&self, title: &Title) -> Result<Option<FileMetadata>, BoxedDbError>;

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

impl<Db> DynDatabaseProvider for Db
where
    Db: DatabaseProvider + ?Sized,
    Db::Error: core::error::Error + Send + Sync + 'static,
{
    #[inline]
    fn cache_size(&self) -> usize {
        DatabaseProvider::cache_size(self)
    }

    #[inline]
    fn config(&self) -> &Configuration {
        DatabaseProvider::config(self)
    }

    #[inline]
    fn contains(&self, title: &Title) -> bool {
        DatabaseProvider::contains(self, title)
    }

    #[inline]
    fn get(&self, title: &Title) -> Result<Option<Arc<Article>>, BoxedDbError> {
        DatabaseProvider::get(self, title).map_err(|err| BoxedDbError(Box::new(err)))
    }

    #[inline]
    fn len(&self) -> usize {
        DatabaseProvider::len(self)
    }

    #[inline]
    fn metadata(&self, title: &Title) -> Result<Option<FileMetadata>, BoxedDbError> {
        DatabaseProvider::metadata(self, title).map_err(|err| BoxedDbError(Box::new(err)))
    }

    #[inline]
    fn name(&self) -> &str {
        DatabaseProvider::name(self)
    }

    #[inline]
    fn prefetch_all(&self, templates: IndexSet<Title>, links: IndexSet<Title>) {
        DatabaseProvider::prefetch_all(self, templates, links);
    }
}

impl DatabaseProvider for Arc<dyn DynDatabaseProvider> {
    type Error = BoxedDbError;

    #[inline]
    fn cache_size(&self) -> usize {
        DynDatabaseProvider::cache_size(self.as_ref())
    }

    #[inline]
    fn config(&self) -> &Configuration {
        DynDatabaseProvider::config(self.as_ref())
    }

    #[inline]
    fn contains(&self, title: &Title) -> bool {
        DynDatabaseProvider::contains(self.as_ref(), title)
    }

    #[inline]
    fn get(&self, title: &Title) -> Result<Option<Arc<Article>>, Self::Error> {
        DynDatabaseProvider::get(self.as_ref(), title)
    }

    #[inline]
    fn len(&self) -> usize {
        DynDatabaseProvider::len(self.as_ref())
    }

    #[inline]
    fn metadata(&self, title: &Title) -> Result<Option<FileMetadata>, Self::Error> {
        DynDatabaseProvider::metadata(self.as_ref(), title)
    }

    #[inline]
    fn name(&self) -> &str {
        DynDatabaseProvider::name(self.as_ref())
    }

    #[inline]
    fn prefetch_all(&self, templates: IndexSet<Title>, links: IndexSet<Title>) {
        DynDatabaseProvider::prefetch_all(self.as_ref(), templates, links);
    }
}

#[doc(hidden)]
mod private {
    pub trait Sealed {}
    impl<Db> Sealed for Db
    where
        Db: super::DatabaseProvider + ?Sized,
        Db::Error: core::error::Error + Send + Sync + 'static,
    {
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
    /// The article and revision ID for an unsaved article.
    pub const UNSAVED_ID: u64 = 0;

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

/// A fake database used for testing.
#[derive(Debug)]
pub struct MockDatabase<'config> {
    /// The mock articles.
    articles: HashMap<String, Arc<Article>>,
    /// The mock files.
    files: HashMap<String, FileMetadata>,
    /// The mock configuration.
    config: &'config Configuration,
    /// The mock configuration name.
    name: &'config str,
}

/// Media file metadata.
#[derive(Clone, Copy, Debug)]
pub enum FileMetadata {
    /// Beeps and boops.
    Audio,
    /// A stolen soul.
    Image {
        /// Image height.
        height: u32,
        /// If true, a scalable vector image format.
        scalable: bool,
        /// Image width.
        width: u32,
    },
    /// Witchcraft, sometimes with added beeps and boops.
    Video {
        /// Video height.
        height: u32,
        /// Video width.
        width: u32,
    },
}

impl<'config> MockDatabase<'config> {
    /// Creates a new database using the given `config`.
    #[inline]
    #[must_use]
    pub fn new(name: &'config str, config: &'config Configuration) -> Self {
        Self {
            articles: <_>::default(),
            files: <_>::default(),
            config,
            name,
        }
    }

    /// Inserts an `article` to the database.
    ///
    /// # Panics
    ///
    /// * `article` has an invalid title
    pub fn insert(&mut self, article: Article) {
        let title = Title::new(self.config, article.title(), None)
            .expect("valid title")
            .key()
            .to_owned();
        self.articles.insert(title.clone(), Arc::new(article));
    }

    /// Inserts a `file` to the database.
    ///
    /// # Panics
    ///
    /// * `title` is an invalid title
    pub fn insert_file(&mut self, filename: &str, file: FileMetadata) {
        self.files.insert(filename.to_owned(), file);
    }

    /// Removes an `article` with the given title from the database.
    ///
    /// # Panics
    ///
    /// * `title` is an invalid title
    pub fn remove(&mut self, title: &str) {
        let title = Title::new(self.config, title, None);
        self.articles.remove(title.expect("valid title").key());
    }

    /// Resolves redirects for `title`.
    fn resolve<'a>(&self, title: &'a Title) -> Cow<'a, Title> {
        let mut key = title.key();
        for _ in 0..2 {
            if let Some(article) = self.articles.get(key)
                && let Some(redirect) = article.redirect()
            {
                key = redirect;
            } else {
                break;
            }
        }
        if key == title.key() {
            Cow::Borrowed(title)
        } else {
            Cow::Owned(Title::new(self.config, key, None).expect("valid redirect"))
        }
    }
}

impl DatabaseProvider for MockDatabase<'_> {
    type Error = MockError;

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
            || (matches!(title.namespace().id, Namespace::FILE | Namespace::MEDIA)
                && self.files.contains_key(title.text()))
    }

    fn get(&self, title: &Title) -> Result<Option<Arc<Article>>, Self::Error> {
        Ok(self.articles.get(title.key()).cloned())
    }

    #[inline]
    fn len(&self) -> usize {
        self.articles.len()
    }

    #[inline]
    fn name(&self) -> &str {
        self.name
    }

    fn metadata(&self, title: &Title) -> Result<Option<FileMetadata>, Self::Error> {
        Ok(
            matches!(title.namespace().id, Namespace::FILE | Namespace::MEDIA)
                .then(|| {
                    let title = self.resolve(title);
                    self.files.get(title.text()).copied()
                })
                .flatten(),
        )
    }

    #[inline]
    fn prefetch_all(&self, _templates: IndexSet<Title>, _links: IndexSet<Title>) {}
}

/// An error from the mock database.
///
/// This is effectively `!` since there is no way for the mock database to
/// fail.
#[derive(Debug, thiserror::Error)]
#[error("error")]
pub struct MockError;

/// Fetches an article from the given `db`, returning `Ok(None)` if the given
/// title string is not valid for the given database.
///
/// # Errors
///
/// * the database broke
pub fn fetch<Db: DatabaseProvider + ?Sized>(
    db: &Db,
    text: &str,
    default_ns: Option<i32>,
) -> Result<Option<Arc<Article>>, Db::Error> {
    Title::new(db.config(), text, default_ns)
        .ok()
        .and_then(|title| db.get(&title).transpose())
        .transpose()
}

/// Resolves any redirects for an article, returning the final article.
///
/// # Errors
///
/// * the database broke
pub fn resolve_redirects<Db>(db: &Db, mut article: Arc<Article>) -> Result<Arc<Article>, Db::Error>
where
    Db: DatabaseProvider + ?Sized,
{
    // “Loop to fetch the article, with up to 2 redirects”
    for _ in 0..2 {
        if let Some(target) = article.redirect()
            && let Some(target) = fetch(db, target, None)?
        {
            // log::trace!("Redirection #{} to {target}", attempt + 1);
            article = target;
        } else {
            break;
        }
    }

    Ok(article)
}
