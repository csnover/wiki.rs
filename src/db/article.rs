//! Types and functions for extracting articles from a compressed multistream
//! dump.

use super::{Error, index::IndexEntry};
use crate::{lru_limiter::ByMemoryUsageCalculator, title::NamespaceCase};
use bzip2_rs::DecoderReader;
use memmap2::Mmap;
use minidom::Element;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader, Read},
    path::Path,
    sync::Arc,
};
use time::{UtcDateTime, format_description::well_known::Iso8601};

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
    /// The time of the last edit.
    pub date: UtcDateTime,
    /// The data model of the article. This is usually "wikitext", but can be
    /// "json" for JSON data, "Scribunto" for Lua modules, etc.
    pub model: String,
    /// If this article is a redirection to another article, the title of the
    /// destination article.
    pub redirect: Option<String>,
}

impl ByMemoryUsageCalculator for Option<Arc<Article>> {
    type Target = Self;

    fn size_of(value: &Self::Target) -> usize {
        core::mem::size_of::<Option<Arc<Article>>>()
            + value.as_ref().map_or(0, |value| {
                core::mem::size_of::<Article>() + value.title.capacity() + value.body.capacity()
            })
    }
}

/// A database namespace.
pub struct DatabaseNamespace {
    /// The letter casing of the namespace name.
    pub case: NamespaceCase,
    /// The name of the namespace.
    pub name: String,
}

/// Information about the database.
pub struct Metadata {
    /// The namespaces from the database.
    pub namespaces: HashMap<i32, DatabaseNamespace>,
    /// The name of the site from the database.
    pub site_name: String,
}

/// A reader for a compressed multistream MediaWiki dump.
pub(super) struct ArticleDatabase {
    /// Read-only memory-mapped compressed `database.xml.bz2`.
    data: Mmap,
    /// Database metadata.
    metadata: Metadata,
}

impl ArticleDatabase {
    /// Opens a raw `multistream.xml.bz2` file using memory mapping.
    pub(super) fn from_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|err| Error::Io(err, path.into()))?;
        // SAFETY: This data is only ever used immutably.
        let data = unsafe { Mmap::map(&file).map_err(|err| Error::Io(err, path.into()))? };

        // Maybe someone decompressed the file, or mixed up the index and
        // database files.
        if &data[0..2] != b"BZ" || &data[4..10] != b"\x31\x41\x59\x26\x53\x59" {
            return Err(Error::Format(path.into()));
        }

        let metadata = Self::database_info(&data)?;

        Ok(Self { data, metadata })
    }

    /// Gets the article at the given index.
    pub(super) fn get_article(&self, entry: &IndexEntry<'_>) -> Result<Article, Error> {
        let chunk = self.get_article_chunk(entry.offset)?;
        let root = chunk.parse::<Element>()?;
        let article = root.children().find(|el| {
            el.get_child("id", "")
                .is_some_and(|id| id.text() == entry.id.to_string())
        });

        match article {
            Some(article) => Self::parse_article(article),
            None => Err(Error::NotFound),
        }
    }

    /// Returns the metadata for this database.
    pub(super) fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Reconstitutes a compressed multistream XML chunk into at the given
    /// offset.
    fn get_article_chunk(&self, offset: u64) -> Result<String, Error> {
        let offset = usize::try_from(offset)?;
        let bzip_data = &self.data[offset..];

        let mut decoded = Vec::from(br#"<pages xmlns="">"#);
        let mut reader = DecoderReader::new(bzip_data);
        io::copy(&mut reader, &mut decoded).map_err(Error::Decompression)?;
        decoded.extend(b"</pages>");
        Ok(String::from_utf8(decoded)?)
    }

    /// Parses basic information about the database from the `<siteinfo>` in the
    /// first chunk.
    fn database_info(data: &[u8]) -> Result<Metadata, Error> {
        // In case someone tries to load a non-multistream database, the number
        // of bytes read is limited to some amount well above the expected size
        // (the true expected data size is only ~2KiB).
        const OOPS_PROTECTION: usize = 128 * 1024;

        let mut decoded = BufReader::new(DecoderReader::new(data))
            .bytes()
            .take(OOPS_PROTECTION)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::Decompression)?;

        if decoded.len() == OOPS_PROTECTION {
            return Err(Error::NotMultistream);
        }

        decoded.extend(b"</mediawiki>");

        let root = String::from_utf8(decoded)?
            .parse::<Element>()
            .map_err(|_| Error::Siteinfo)?;
        let root_ns = root.ns();

        let siteinfo = try_get_child_ns(&root, "siteinfo", &root_ns)?;

        let site_name = try_get_child_ns(siteinfo, "sitename", &root_ns)?.text();
        let namespaces = try_get_child_ns(siteinfo, "namespaces", &root_ns)?;
        let namespaces = namespaces
            .children()
            .filter(|&ns| ns.name() == "namespace")
            .map(|ns| {
                let key = ns
                    .attr("key")
                    .ok_or(Error::XmlProperty("key".into()))?
                    .parse::<i32>()?;
                let case = ns.attr("case").ok_or(Error::XmlProperty("case".into()))?;
                let case = match case {
                    "first-letter" => NamespaceCase::FirstLetter,
                    "case-sensitive" => NamespaceCase::CaseSensitive,
                    _ => return Err(Error::NamespaceCase(case.into())),
                };
                let name = ns.text();

                Ok((key, DatabaseNamespace { case, name }))
            })
            .collect::<Result<_, _>>()?;
        Ok(Metadata {
            namespaces,
            site_name,
        })
    }

    /// Extracts article data from an XML element.
    fn parse_article(article: &Element) -> Result<Article, Error> {
        let id = try_get_child(article, "id")?.text().parse::<u64>()?;
        let title = try_get_child(article, "title")?.text();
        let revision = try_get_child(article, "revision")?;
        let body = try_get_child(revision, "text")?.text();
        let date = UtcDateTime::parse(
            &try_get_child(revision, "timestamp")?.text(),
            &Iso8601::DEFAULT,
        )?;
        let model = try_get_child(revision, "model")?.text();
        let redirect = try_get_child(article, "redirect")
            .ok()
            .and_then(|r| r.attr("title").map(ToString::to_string));
        // TODO: This may be needed eventually, so remember it exists
        // let last_changed_by = revision
        //     .try_get_child("contributor")?
        //     .try_get_child("username")?
        //     .text();

        Ok(Article {
            id,
            title,
            body,
            date,
            model,
            redirect,
        })
    }
}

/// Tries to get a child element by name and returns an [`Error`] if it does not
/// exist.
fn try_get_child<'a>(element: &'a Element, name: &str) -> Result<&'a Element, Error> {
    try_get_child_ns(element, name, "")
}

/// Tries to get a child element by name and namespace and returns an [`Error`]
/// if it does not exist.
fn try_get_child_ns<'a>(element: &'a Element, name: &str, ns: &str) -> Result<&'a Element, Error> {
    let child = element.get_child(name, ns);
    child.ok_or_else(|| Error::XmlProperty(name.into()))
}
