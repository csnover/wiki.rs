use memmap2::Mmap;
use rayon::prelude::*;
use std::{fs::File, str::FromStr};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("missing offset column in index")]
    MissingOffset,

    #[error("missing page ID column in index")]
    MissingId,

    #[error("missing page name column in index")]
    MissingName,

    #[error("failed integer conversion: {0}")]
    ParseInt(#[from] core::num::ParseIntError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct IndexEntry<'a> {
    pub offset: u64,
    pub page_id: u64,
    pub page_name: &'a str,
}

impl<'a> TryFrom<&'a str> for IndexEntry<'a> {
    type Error = Error;

    fn try_from(line: &'a str) -> Result<Self, Self::Error> {
        let mut line = line.splitn(3, ':');
        let offset = u64::from_str(line.next().ok_or(Error::MissingOffset)?)?;
        let page_id = u64::from_str(line.next().ok_or(Error::MissingId)?)?;
        let page_name = line.next().ok_or(Error::MissingName)?;

        Ok(Self {
            offset,
            page_id,
            page_name,
        })
    }
}

pub struct Index<'a> {
    _data: Mmap,
    entries: Vec<IndexEntry<'a>>,
}

impl<'a> Index<'a> {
    pub fn from_file(path: &str) -> Result<Self, Error> {
        let file = File::open(path)?;

        let (data, entries) = unsafe {
            let data = Mmap::map(&file)?;

            // Safety: Since the deref pointer is kernel allocated memory, it
            // will never move, but the borrow-checker does not understand this
            let view = core::slice::from_raw_parts(data.as_ptr(), data.len());

            // Safety: The index is specified as containing utf-8 text. If it
            // is not, the worst case scenario is that titles appear to be
            // garbage.
            let entries = std::str::from_utf8_unchecked(view)
                .par_lines()
                .map(IndexEntry::try_from)
                .collect::<Result<Vec<_>, _>>()?;

            (data, entries)
        };

        Ok(Self {
            _data: data,
            entries,
        })
    }

    pub fn find_articles(
        &self,
        query: &regex::Regex,
    ) -> impl ParallelIterator<Item = &IndexEntry<'_>> {
        self.entries
            .par_iter()
            .filter(|entry| query.is_match(entry.page_name))
    }

    pub fn find_article(&self, name: &str) -> Option<&IndexEntry> {
        self.entries
            .par_iter()
            .find_any(|entry| entry.page_name == name)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
