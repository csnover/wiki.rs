use super::index::IndexEntry;
use bzip2_rs::DecoderReader;
use memmap2::Mmap;
use minidom::Element;
use std::{fs::File, io};

#[derive(Debug, Clone)]
pub struct Article {
    pub title: String,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("requested article not found")]
    ArticleNotFound,

    #[error("missing property on page: {0}")]
    MissingProperty(String),

    #[error("invalid utf-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("DOM error: {0}")]
    Dom(#[from] minidom::Error),
}

pub struct ArticleDatabase {
    data: Mmap,
}

impl ArticleDatabase {
    pub fn from_file(path: &str) -> Result<Self, std::io::Error> {
        Self::load(File::open(path)?)
    }

    pub fn load(file: File) -> Result<Self, std::io::Error> {
        Ok(Self {
            data: unsafe { Mmap::map(&file)? },
        })
    }

    pub fn get_article(&self, idx: &IndexEntry) -> Result<Article, Error> {
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
            None => Err(Error::ArticleNotFound),
        }
    }

    fn get_article_chunk(&self, idx: &IndexEntry) -> Result<String, Error> {
        let offset = idx.offset as usize;
        let bzip_data = &self.data[offset..];

        let mut decoded = Vec::<u8>::new();
        let mut reader = DecoderReader::new(bzip_data);
        io::copy(&mut reader, &mut decoded)?;

        let raw_xml_pages = String::from_utf8(decoded)?;
        let reconstructed_xml = format!("<pages xmlns=\"\">{}</pages>", &raw_xml_pages);

        Ok(reconstructed_xml)
    }

    fn parse_article(article: &Element) -> Result<Article, Error> {
        let title = try_get_child(article, "title")?.text();
        let revision = try_get_child(article, "revision")?;
        let body = try_get_child(revision, "text")?.text();
        Ok(Article { title, body })
    }
}

fn try_get_child<'a>(element: &'a Element, name: &str) -> Result<&'a Element, Error> {
    let child = element.get_child(name, "");
    match child {
        Some(child) => Ok(child),
        None => Err(Error::MissingProperty(name.to_owned())),
    }
}
