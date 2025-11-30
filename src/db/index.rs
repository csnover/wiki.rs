//! Types and functions for reading a multistream dump text index.

use html_escape::encode_double_quoted_attribute;
use memmap2::Mmap;
use rayon::prelude::*;
use std::{
    fs::File,
    path::{Path, PathBuf},
    str::FromStr,
};

/// Errors that may occur when reading the dump index.
#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    /// The index file appears to be compressed.
    #[error("{0}: index file must be decompressed before running wiki.rs")]
    Compressed(PathBuf),

    /// An I/O error occurred reading from the index.
    #[error("{1}: I/O error: {0}")]
    Io(std::io::Error, PathBuf),

    /// The page ID column was missing from a line in the `index.txt`.
    ///
    /// ```text
    /// 000000000:00000:TITLE
    ///           ^^^^^
    /// ```
    #[error("missing page ID column in index")]
    PageId,

    /// The title column was missing from a line in the `index.txt`.
    ///
    /// ```text
    /// 000000000:00000:TITLE
    ///                 ^^^^^
    /// ```
    #[error("missing page name column in index")]
    PageName,

    /// The offset column was missing from a line in the `index.txt`.
    ///
    /// ```text
    /// 000000000:00000:TITLE
    /// ^^^^^^^^^
    /// ```
    #[error("missing offset column in index")]
    PageOffset,

    /// The offset or page ID column contained something other than an integer.
    #[error("failed integer conversion: {0}")]
    ParseInt(#[from] core::num::ParseIntError),
}

/// An index entry.
pub(super) struct IndexEntry<'a> {
    /// The offset, in bytes, of an XML chunk which should contain the given
    /// article.
    pub(super) offset: u64,
    /// The canonical ID of the article.
    pub(super) id: u64,
    /// The title of the article.
    pub(super) title: &'a str,
}

impl<'a> TryFrom<&'a str> for IndexEntry<'a> {
    type Error = Error;

    fn try_from(line: &'a str) -> Result<Self, Self::Error> {
        let mut line = line.splitn(3, ':');
        let offset = u64::from_str(line.next().ok_or(Error::PageOffset)?)?;
        let page_id = u64::from_str(line.next().ok_or(Error::PageId)?)?;
        let page_name = line.next().ok_or(Error::PageName)?;

        Ok(Self {
            offset,
            id: page_id,
            title: page_name,
        })
    }
}

/// A structured form of the `index.txt` database.
pub(super) struct Index<'a> {
    /// The read-only memory-mapped `index.txt` file.
    _data: Mmap,
    /// Extracted entries from the index.
    entries: Vec<IndexEntry<'a>>,
}

impl Index<'_> {
    /// Creates an [`Index`] from the file given by `path`.
    pub(super) fn from_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let file = File::open(path).map_err(|err| Error::Io(err, path.into()))?;

        let (data, entries) = unsafe {
            // SAFETY: This data is only ever used immutably.
            let data = Mmap::map(&file).map_err(|err| Error::Io(err, path.into()))?;

            // Compressed index is not supported because it would need to either
            // be decompressed to disk once, or it would need to be decompressed
            // to memory every time the index is loaded. So it makes more sense
            // to just get the user to do it themselves, since then they can
            // control the process, instead of having a slow start-up and then
            // wondering why they lost a gigabyte of disk space or whatever. The
            // decompressed index always has ASCII digits at the start of the
            // file.
            if data[0..2].iter().any(|b| !b.is_ascii_digit()) {
                return Err(Error::Compressed(path.into()));
            }

            // SAFETY: Since the deref pointer is kernel allocated memory, it
            // will never move, but the borrow-checker does not understand this
            let view = core::slice::from_raw_parts(data.as_ptr(), data.len());

            // SAFETY: The index is specified as containing utf-8 text. If it
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

    /// Finds entries in the index with titles matching the given regular
    /// expression.
    pub(super) fn find_articles(
        &self,
        query: &regex::Regex,
    ) -> impl ParallelIterator<Item = &IndexEntry<'_>> {
        self.entries
            .par_iter()
            .filter(|entry| query.is_match(entry.title))
    }

    /// Finds a single entry in the index with the given article title.
    pub(super) fn find_article(&self, title: &str) -> Option<&IndexEntry<'_>> {
        // " and & are entity-encoded in the index; < and > are disallowed.
        let name = encode_double_quoted_attribute(title);
        self.entries
            .par_iter()
            .find_any(|entry| entry.title == name)
    }

    /// The total number of articles in the index.
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}
