use std::{
    fs::File,
    str::{self, FromStr},
};

use memmap2::Mmap;
use rayon::prelude::*;

pub struct Index<'a> {
    _data: Mmap,
    entries: Vec<IndexEntry<'a>>,
}

pub struct IndexEntry<'a> {
    pub offset: u64,
    pub page_id: u64,
    pub page_name: &'a str,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("missing offset column in index")]
    MissingOffset,
    #[error("missing page ID column in index")]
    MissingId,
    #[error("missing page name column in index")]
    MissingName,
    #[error("invalid number")]
    BadNumber(#[from] core::num::ParseIntError),
    #[error("bad i/o")]
    Io(#[from] std::io::Error),
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

impl<'a> Index<'a> {
    pub fn from_file(path: &str) -> Result<Self, Error> {
        let file = File::open(path)?;

        let data = unsafe { Mmap::map(&file)? };
        let entries = unsafe {
            let view = core::slice::from_raw_parts(data.as_ptr(), data.len());
            str::from_utf8_unchecked(view)
                .par_lines()
                .map(IndexEntry::try_from)
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(Self {
            _data: data,
            entries,
        })
    }

    pub fn find_article(
        &self,
        query: &regex::Regex,
    ) -> impl ParallelIterator<Item = &IndexEntry<'_>> {
        self.entries
            .par_iter()
            .filter(|entry| query.is_match(entry.page_name))
    }

    pub fn find_article_exact(&self, name: &str) -> Option<&IndexEntry> {
        self.entries
            .par_iter()
            .find_any(|entry| entry.page_name == name)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
