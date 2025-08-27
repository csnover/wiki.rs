use std::{fs::File, io};

use bzip2_rs::DecoderReader;
use memmap2::Mmap;
use minidom::Element;
use thiserror::Error;

use super::index::IndexEntry;

pub struct ArticleDatabase {
    data: Mmap,
}

#[derive(Debug, Clone)]
pub struct Article {
    pub title: String,
    pub body: String,
}

#[derive(Error, Debug)]
pub enum ArticleError {
    #[error("requested article not found")]
    ArticleNotFound,

    #[error("missing property on page")]
    MissingProperty(String),

    #[error("invalid utf-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("DOM error: {0}")]
    Dom(#[from] minidom::Error),
}

impl ArticleDatabase {
    pub fn from_file(path: &str) -> Result<Self, ArticleError> {
        Self::load(File::open(path)?)
    }

    pub fn load(file: File) -> Result<Self, ArticleError> {
        Ok(Self {
            data: unsafe { Mmap::map(&file)? },
        })
    }

    pub fn get_article(&self, idx: &IndexEntry) -> Result<Article, ArticleError> {
        let chunk = self.get_article_chunk(idx)?;
        let root = chunk.parse::<Element>()?;
        let article = root.children().find(|ch| {
            let option = ch.get_child("id", "");
            if let Some(id) = option {
                id.text() == idx.page_id.to_string()
            } else {
                false
            }
        });

        match article {
            Some(article) => Self::parse_article(article),
            None => Err(ArticleError::ArticleNotFound),
        }
    }

    fn get_article_chunk(&self, idx: &IndexEntry) -> Result<String, ArticleError> {
        let offset = idx.offset as usize;
        let bzip_data = &self.data[offset..];

        let mut decoded = Vec::<u8>::new();
        let mut reader = DecoderReader::new(bzip_data);
        io::copy(&mut reader, &mut decoded)?;

        let raw_xml_pages = String::from_utf8(decoded)?;
        let reconstructed_xml = format!("<pages xmlns=\"\">{}</pages>", &raw_xml_pages);

        Ok(reconstructed_xml)
    }

    fn parse_article(article: &Element) -> Result<Article, ArticleError> {
        let title = article.try_get_child("title")?.text();

        let revision = article.try_get_child("revision")?;
        let body = revision.try_get_child("text")?.text();

        Ok(Article { title, body })
    }
}

trait TryGetChild {
    fn try_get_child(&self, name: &str) -> Result<&Element, ArticleError>;
}

impl TryGetChild for &Element {
    fn try_get_child(&self, name: &str) -> Result<&Element, ArticleError> {
        let child = self.get_child(name, "");
        match child {
            Some(child) => Ok(child),
            None => Err(ArticleError::MissingProperty(name.to_owned())),
        }
    }
}
