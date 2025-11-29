//! Types and functions for interacting with a MediaWiki compressed multistream
//! database dump.

pub use article::{Article, DatabaseNamespace};

use crate::{lru_limiter::ByMemoryUsage, php::strtr};
use article::ArticleDatabase;
use index::Index;
use rayon::iter::ParallelIterator;
use schnellru::LruMap;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, RwLock},
    time::Instant,
};
use time::UtcDateTime;

mod article;
mod index;

/// Errors that may occur when interacting with the article database.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A time value from the database was not a valid ISO-8601 time string.
    #[error("time error: {0}")]
    Date(#[from] time::error::Parse),

    /// An I/O error occurred during decompression.
    #[error("I/O error during decompression: {0}")]
    Decompression(std::io::Error),

    /// A DOM error occurred when processing the XML in the database.
    #[error("DOM error: {0}")]
    Dom(#[from] minidom::Error),

    /// The database file is in an unexpected format.
    #[error("{0}: file is not a compressed multistream bz2 file")]
    Format(std::path::PathBuf),

    /// Data from the database was not valid UTF-8.
    #[error("invalid utf-8: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),

    /// An error occurred within the index reader.
    #[error(transparent)]
    Index(#[from] index::Error),

    /// An I/O error ocurred reading from the database.
    #[error("{1}: I/O error: {0}")]
    Io(std::io::Error, std::path::PathBuf),

    /// Wrong kind of database.
    #[error("unknown namespace case rule '{0}' in siteinfo")]
    NamespaceCase(String),

    /// Article was not found.
    #[error("requested article not found")]
    NotFound,

    /// Wrong kind of database.
    #[error("database is not multi-stream")]
    NotMultistream,

    /// An ID from the database was not a valid number.
    #[error("id error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    /// Database is from another dimension.
    #[error("could not read siteinfo from database dump")]
    Siteinfo,

    /// You are running wiki.rs on a potato.
    #[error("offset out of range of memory address space: {0}")]
    TryFromInt(#[from] core::num::TryFromIntError),

    /// A required property was missing from the XML in the database.
    #[error("missing property on page: {0}")]
    XmlProperty(String),
}

/// The cacheable type for an article.
///
/// Because modules like 'Module:CountryData' abusively expand templates without
/// proper parameters, and those templates try to unconditionally use parameters
/// to load their own child templates, it is necessary to not merely cache just
/// articles which exist, but also requests to articles which *do not* exist, to
/// avoid very slow full table scans over and over again for these clearly
/// intentional (the module calls `string.find(s,"^%{%{ *%{%{%{1")` to decide
/// that it got what it wanted! FFS!) but bogus requests.
type CacheableArticle = Option<Arc<Article>>;

/// A MediaWiki multistream database reader.
pub struct Database<'a> {
    /// The uncompressed text index part of the database.
    index: Index<'a>,
    /// The compressed XML part of the database.
    articles: ArticleDatabase,
    /// A decompressed article LRU cache.
    cache: RwLock<LruMap<String, CacheableArticle, ByMemoryUsage<CacheableArticle>>>,
}

impl Database<'_> {
    /// Creates a new database from the given uncompressed text index and
    /// compressed multistream.xml.bz2 file.
    pub fn from_file(
        index_path: impl AsRef<Path>,
        articles_path: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let time = Instant::now();

        let index = Index::from_file(index_path)?;
        log::trace!("Read index in {:.2?}", time.elapsed());

        let articles = ArticleDatabase::from_file(articles_path)?;
        log::info!("Loaded {} articles from index", index.len());

        Ok(Self {
            index,
            articles,
            cache: RwLock::new(LruMap::new(ByMemoryUsage::new(32 * 1024 * 1024))),
        })
    }

    /// Returns the current memory usage of the cache, in bytes.
    pub fn cache_size(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.limiter().heap_usage() + cache.memory_usage()
    }

    /// Returns true if the database contains an article with the given title.
    pub fn contains(&self, title: &str) -> bool {
        self.cache.read().unwrap().peek(title).is_some() || self.index.find_article(title).is_some()
    }

    /// Gets an article with the given title from the database. The article will
    /// be cached in memory.
    pub fn get(&self, title: &str) -> Result<Arc<Article>, Error> {
        self.cache
            .write()
            .unwrap()
            .get_or_insert_fallible(title, || {
                self.fetch_article(title).map_or_else(
                    |err| {
                        if matches!(err, Error::NotFound) {
                            Ok(None)
                        } else {
                            Err(err)
                        }
                    },
                    |article| Ok(Some(Arc::new(article))),
                )
            })
            .and_then(|article| {
                if let Some(Some(article)) = article {
                    Ok(Arc::clone(article))
                } else {
                    Err(Error::NotFound)
                }
            })
    }

    /// The site name from the database.
    pub fn name(&self) -> &str {
        &self.articles.metadata().site_name
    }

    /// The registered namespaces in the database.
    pub fn namespaces(&self) -> &HashMap<i32, DatabaseNamespace> {
        &self.articles.metadata().namespaces
    }

    /// The total number of articles in the database.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Finds articles in the index whose titles match the given query.
    pub fn search(&self, query: &regex::Regex) -> impl ParallelIterator<Item = &str> {
        self.index.find_articles(query).map(|entry| entry.title)
    }

    /// Gets an article directly from the database.
    fn fetch_article(&self, title: &str) -> Result<Article, Error> {
        log::trace!("Loading article {title}");

        let hack = HACKS
            .iter()
            .find_map(|(hack_title, hack)| (*hack_title == title).then_some(*hack));

        if let Some(&Hack::Lobotomy(body)) = hack {
            log::warn!("Replacing {title} from hacks");
            return Ok(Article {
                id: 0xdead_beef,
                title: title.to_string(),
                body: body.to_string(),
                date: UtcDateTime::now(),
                redirect: None,
                model: if title.starts_with("Module:") {
                    "Scribunto"
                } else {
                    "wikitext"
                }
                .into(),
            });
        }

        let time = Instant::now();
        self.index
            .find_article(title)
            .ok_or(Error::NotFound)
            .and_then(|entry| {
                log::trace!("Located article in {:.2?}", time.elapsed());
                let time = Instant::now();
                let mut article = self.articles.get_article(entry);
                log::trace!("Extracted article in {:.2?}", time.elapsed());

                if let (Ok(article), Some(Hack::HorsePills(hacks))) = (article.as_mut(), hack) {
                    log::info!("Modifying {title} using hacks");
                    article.body = strtr(&article.body, hacks).into_owned();
                }

                article
            })
    }
}

/// Sometimes, modules will not work. Sometimes, we can fix that with
/// medication.
enum Hack {
    /// Replace bits of a thing with new things.
    HorsePills(&'static [(&'static str, &'static str)]),
    /// Replace the whole thing with a new thing.
    Lobotomy(&'static str),
}

/// A fix for 'Module:Citation/CS1'.
///
/// This module, instead of using this crazy thing called “function parameters”
/// to ensure functions calls are side-effect-free, instead decides to use
/// a shared global variable `z` to accumulate messages and then never resets
/// it, instead relying on `require('Module:Citation/CS1/Utilities')` somehow
/// giving a fresh copy. This is not how `require` works, except apparently in
/// the MW environment, where I guess performance is optional because the only
/// way this *could* work would be if the module’s closure is called on every
/// single `#invoke`.
///
/// Because the `z` table is *shared* across modules, it is not good enough to
/// do a deep clone. Instead, all its values have to be emptied out.
static MODULE_CITATION_CS1: Hack = Hack::HorsePills(&[(
    "z = utilities.z;",
    "z = utilities.z;\nfor k, v in pairs(z) do\nz[k] = {}\nend",
)]);

/// A fix for 'Module:Footnotes/anchor id list'.
///
/// This module misunderstands that Lua replacement strings are not the same as
/// Lua patterns and thus escapes replacement strings in a way that causes them
/// to contain escapements which are correct for a pattern but illegal in a
/// replacement.
static MODULE_FOOTNOTE_ANCHOR_ID_LIST: Hack = Hack::HorsePills(&[(
    r#"argument = argument:gsub("([%^%$%(%)%.%[%]%*%+%-%?])", "%%%1");"#,
    "",
)]);

/// A fix for 'Module:Hatnote list'.
///
/// This module asks for the current page name immediately when it is loaded,
/// then never checks again, so will show wrong title text when it is used more
/// than once during a session.
// TODO: Probably, this sort of pattern will show up frequently enough that it
// might just be necessary to eat the performance of reinvoking module closures
// every time a new page loads. A set of taint flags might work to limit the
// performance-killing blast radius, where the host Lua interface is monitored
// for any calls that occur during module initialisation that would require
// invalidation, and only those modules which are tainted get reinvoked. A
// different option would be to start returning userdata objects with
// `__tostring` metafunctions that update their internal state, but this would
// almost certainly require hacking the VM to cloak such objects, since the
// appearance of a userdata type is pretty much guaranteed to break all the type
// checks in the MW modules.
static MODULE_HATNOTE_LIST: Hack = Hack::HorsePills(&[
    ("	title = mw.title.getCurrentTitle().text,\n", ""),
    (
        "options = options or {}",
        concat!(
            "options = options or {}\n",
            "if options.title == nil then\n",
            "    options.title = mw.title.getCurrentTitle().text\n",
            "end"
        ),
    ),
]);

/// A fix for 'Module:TNT'.
///
/// Like its name accidentally implies, this module explodes if someone tries
/// to format a message using a key which does not exist. Since the data
/// comes from the interwiki Wikimedia Commons, we do not have it, and since
/// the module iterates to find keys, we cannot hack it by using an `__index`
/// metatable on data returned by `mw.ext.data.get`. Returning an error also
/// does not work because not everything uses pcall with a fallback path. So
/// just override the whole thing with a script that does nothing.
static MODULE_TNT: Hack = Hack::Lobotomy(
    r"
local p = {}
function p.msg(frame)
    local dataset, id
    local params = {}
    local lang = nil
    for k, v in pairs(frame.args) do
        if k == 1 then
            dataset = mw.text.trim(v)
        elseif k == 2 then
            id = mw.text.trim(v)
        elseif type(k) == 'number' then
            table.insert(params, mw.text.trim(v))
        elseif k == 'lang' and v ~= '_' then
            lang = mw.text.trim(v)
        end
    end
    return formatMessage(dataset, id, params, lang)
end
function p.format(dataset, key, ...)
    return formatMessage(dataset, key, {...})
end
function p.formatInLanguage(lang, dataset, key, ...)
    return formatMessage(dataset, key, {...}, lang)
end
function p.link(frame)
    return link(frame.args[1])
end
function p.doc(frame)
    return ''
end
formatMessage = function(dataset, key, params, lang)
    local result = mw.message.newRawMessage(key, unpack(params or {}))
    return result:plain()
end
return p
",
);

/// A fix for 'Module:Wikidata'.
///
/// This module contains an invalid string which does not lex in a
/// Lua 5.4-conforming engine.
static MODULE_WIKIDATA: Hack = Hack::HorsePills(&[(r#""^\-?%d+""#, r#""^-?%d+""#)]);

/// All the sad hacks that are required to successfully load modules in a Lua
/// engine which is not the modified Lua 5.1 engine used by Scribunto.
static HACKS: &[(&str, &Hack)] = &[
    ("Module:Citation/CS1", &MODULE_CITATION_CS1),
    (
        "Module:Footnotes/anchor id list",
        &MODULE_FOOTNOTE_ANCHOR_ID_LIST,
    ),
    ("Module:Hatnote list", &MODULE_HATNOTE_LIST),
    ("Module:TNT", &MODULE_TNT),
    ("Module:Wikidata", &MODULE_WIKIDATA),
];
