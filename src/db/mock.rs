use super::{Article, DatabaseNamespace, Error, IDatabase, Result};
use crate::{config::CONFIG, title::Title, wikitext::Configuration};
use indexmap::IndexSet;
use rayon::prelude::ParallelIterator;
use std::{collections::HashMap, sync::Arc};
use time::UtcDateTime;

pub(crate) struct MockDatabase<'text> {
    articles: HashMap<&'text str, Arc<Article>>,
    next_id: u64,
}

impl<'text> MockDatabase<'text> {
    #[inline]
    pub fn new() -> Self {
        Self {
            articles: <_>::default(),
            next_id: 1,
        }
    }

    pub fn insert(&mut self, title: &'text str, body: &str) {
        let id = self.next_id;
        self.next_id += 1;
        self.articles.insert(
            title,
            Arc::new(Article {
                id,
                title: title.into(),
                body: body.into(),
                model: "wikitext".into(),
                redirect: None,
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
    fn config(&self) -> &'static Configuration {
        &CONFIG
    }

    #[inline]
    fn contains(&self, title: &Title) -> bool {
        self.articles.contains_key(title.key())
    }

    #[inline]
    fn creation_date(&self) -> Option<UtcDateTime> {
        None
    }

    fn get(&self, title: &Title) -> Result<Arc<Article>> {
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

    #[inline]
    fn search(&self, _query: &regex::Regex) -> impl ParallelIterator<Item = &str> {
        rayon::iter::empty()
    }
}
