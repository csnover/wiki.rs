use article::{Article, ArticleDatabase, Error};
use rayon::iter::ParallelIterator;
use schnellru::LruMap;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use index::Index;
use lru_limiter::ByMemoryUsage;

pub mod article;
pub mod index;
mod lru_limiter;

pub struct Database<'a> {
    index: Index<'a>,
    articles: ArticleDatabase,
    cache: Mutex<LruMap<String, Arc<Article>, ByMemoryUsage>>,
}

impl<'a> Database<'a> {
    pub fn from_file(index_path: &str, articles_path: &str) -> Result<Self, index::Error> {
        let time = Instant::now();

        let index = Index::from_file(index_path)?;
        log::trace!("Read index in {:.2?}", time.elapsed());

        let articles = ArticleDatabase::from_file(articles_path)?;
        log::info!("Loaded {} articles from index", index.len());

        Ok(Self {
            index,
            articles,
            cache: Mutex::new(LruMap::new(ByMemoryUsage::new(32 * 1024 * 1024))),
        })
    }

    pub fn search(&self, query: &regex::Regex) -> impl ParallelIterator<Item = &str> {
        self.index.find_articles(query).map(|entry| entry.page_name)
    }

    pub fn get(&self, name: &str) -> Result<Arc<Article>, Error> {
        self.cache
            .lock()
            .unwrap()
            .get_or_insert_fallible(name, || self.fetch_article(name).map(Arc::new))
            .map(|article| Arc::clone(article.unwrap()))
    }

    fn fetch_article(&self, name: &str) -> Result<Article, Error> {
        log::trace!("Loading article {name}");

        let time = Instant::now();
        self.index
            .find_article(name)
            .ok_or(Error::ArticleNotFound)
            .and_then(|entry| {
                log::trace!("Located article in {:.2?}", time.elapsed());
                let time = Instant::now();
                let article = self.articles.get_article(entry);
                log::trace!("Extracted article in {:.2?}", time.elapsed());
                article
            })
    }
}
