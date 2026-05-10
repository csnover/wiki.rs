//! An implementation of [`IDatabase`] used for testing.

use super::{
    config::Configuration,
    db::{Article, DatabaseNamespace, Error, IDatabase},
    title::Title,
};
use indexmap::IndexSet;
use std::{collections::HashMap, sync::Arc};
use time::UtcDateTime;

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
        let id = self.next_id;
        self.next_id += 1;
        self.articles.insert(
            title.into(),
            Arc::new(Article {
                body: body.into(),
                id,
                model: "wikitext".into(),
                redirect: None,
                title: title.into(),
            }),
        );
    }
}

impl IDatabase for MockDatabase<'_> {
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

    #[inline]
    fn creation_date(&self) -> Option<UtcDateTime> {
        None
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
    fn namespaces(&self) -> &HashMap<i32, DatabaseNamespace> {
        unimplemented!()
    }

    #[inline]
    fn prefetch_all(&self, _templates: IndexSet<Title>, _links: IndexSet<Title>) {}
}
