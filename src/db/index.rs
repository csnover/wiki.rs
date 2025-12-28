//! Types and functions for reading a multistream dump text index.

use crate::lru_limiter::{ByMemoryUsage, HeapUsageCalculator};
use core::{marker::PhantomData, num::NonZeroU64};
use html_escape::encode_double_quoted_attribute;
use memmap2::Mmap;
use rayon::prelude::*;
use schnellru::LruMap;
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::RwLock,
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
#[derive(Clone, Copy, Debug)]
pub(super) struct IndexEntry {
    /// The canonical ID of the article.
    pub(super) id: NonZeroU64,

    /// The offset, in bytes, of an XML chunk which should contain the given
    /// article.
    pub(super) offset: u64,
}

impl HeapUsageCalculator for Option<IndexEntry> {
    #[inline]
    fn size_of(&self) -> usize {
        0
    }
}

/// A structured form of the `index.txt` database.
pub(super) struct Index<'a> {
    /// The read-only memory-mapped `index.txt` file.
    _data: Mmap,

    /// A small fixed-size cache for existence checks.
    cache: RwLock<LruMap<String, Option<IndexEntry>, ByMemoryUsage>>,

    /// Offsets of the *title text* for each line in the index.
    entries: Vec<PackedOffset<'a>>,

    /// String view into the index.
    view: &'a str,
}

impl Index<'_> {
    /// Creates an [`Index`] from the file given by `path`.
    pub(super) fn from_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let file = File::open(path).map_err(|err| Error::Io(err, path.into()))?;

        let (data, view) = unsafe {
            // SAFETY: This data is only ever used immutably.
            let data = Mmap::map(&file).map_err(|err| Error::Io(err, path.into()))?;
            #[cfg(unix)]
            let _ = data.advise(memmap2::Advice::Sequential);

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
            (data, std::str::from_utf8_unchecked(view))
        };

        // To reduce the amount of memory used by the index ~as much as
        // possible, only the offset of each title is stored, and it is stored
        // in a packed format. Theoretically the index could be reduced to 4
        // bytes per entry by storing only relative offsets, at least until some
        // index file grows to over 4GiB, which I guess might happen in another
        // few decades. To do 4 bytes only, incoming search regexes would need
        // to be adjusted to be `^.*{query}.*(?:\n|$)` and the plain text query
        // would need to use `starts_with` + a newline or end-of-file check. I
        // prototyped this approach because I could not quit golfing and it had
        // a ~25% runtime penalty compared to the current approach. I did not
        // inspect the disassembly to try to understand more about where the
        // performance was being lost, but it seems unlikely that mask + shift +
        // eq + strcmp should be so much slower than add + sub + eq + strcmp…
        //
        // To avoid wasting time scanning past the offset and ID when searching
        // for an article in the index, the offset of the title is stored
        // instead of the offset of the line.
        //
        // The offset and ID columns are validated here since we are already
        // doing a full index scan anyway. This avoids any requirement for error
        // handling when requesting entries from the index.
        //
        // It is definitely *not* fast to avoid storing offsets at all and just
        // use `par_lines` for every query, in case you were wondering.
        let entries = view
            .par_lines()
            .map(|line| {
                fn is_ascii_number(offset: &str) -> bool {
                    offset.bytes().all(|b| b.is_ascii_digit())
                }
                let mut line = line.splitn(3, ':');
                let offset = line.next();
                if !offset.is_some_and(is_ascii_number) {
                    return Err(Error::PageOffset);
                }
                let id = line.next();
                if !id.is_some_and(is_ascii_number) {
                    return Err(Error::PageId);
                }
                let title = line.next().ok_or(Error::PageName)?;
                Ok(PackedOffset::new(title))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            _data: data,
            cache: RwLock::new(LruMap::new(ByMemoryUsage::new(4 * 1_048_576))),
            entries,
            view,
        })
    }

    /// Finds entries in the index with titles matching the given regular
    /// expression.
    pub(super) fn find_articles(&self, query: &regex::Regex) -> impl ParallelIterator<Item = &str> {
        self.entries.par_iter().filter_map(|title| {
            let title = title.into_str();
            query.is_match(title).then_some(title)
        })
    }

    /// Finds a single entry in the index with the given article title.
    pub(super) fn find_article(&self, title: &str) -> Option<IndexEntry> {
        self.cache
            .write()
            .unwrap()
            .get_or_insert(title, || {
                // " and & are entity-encoded in the index; < and > are disallowed.
                let name = encode_double_quoted_attribute(title);
                self.entries.par_iter().find_map_any(|title| {
                    let title = title.into_str();
                    (title == name).then(|| make_index(self.view, title))
                })
            })
            .copied()
            .flatten()
    }

    /// The total number of articles in the index.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// A memory address and length packed into a single u64.
#[derive(Clone, Copy)]
struct PackedOffset<'a>(u64, PhantomData<&'a ()>);

impl<'a> PackedOffset<'a> {
    /// Offset size, in bits.
    const DATA_SIZE: u32 = 52;

    /// Offset mask.
    const DATA_MASK: u64 = (1 << Self::DATA_SIZE) - 1;

    /// Creates a new [`PackedOffset`].
    #[inline]
    fn new(s: &str) -> Self {
        let data = s.as_ptr() as u64;
        let len = s.len() as u64;
        debug_assert!(data <= Self::DATA_MASK);
        debug_assert!(len < (1 << (u64::BITS - Self::DATA_SIZE)));
        Self(len << Self::DATA_SIZE | data, PhantomData)
    }

    /// Converts the [`PackedOffset`] into a string reference.
    #[inline]
    fn into_str(self) -> &'a str {
        let data = (self.0 & Self::DATA_MASK) as *const u8;
        let len = (self.0 >> Self::DATA_SIZE) as usize;
        // SAFETY: This data started its life as a string slice.
        unsafe { str::from_utf8_unchecked(core::slice::from_raw_parts(data, len)) }
    }
}

/// Returns an [`IndexEntry`] from a reference to the given title.
///
#[inline]
fn make_index(view: &str, title: &str) -> IndexEntry {
    // SAFETY: The reference `title` is guaranteed by the caller to come from
    // the same memory allocation as `view`. During startup, the entire
    // allocation was scanned and validated, so the offset and ID conversions
    // are also guaranteed to succeed.
    let (offset, id) = unsafe {
        let base = view.as_ptr();
        let mut s = title.as_ptr().sub(2);

        let mut id = 0;
        let mut mul = 1;
        while *s != b':' {
            let d = u64::from((*s).unchecked_sub(b'0'));
            id += d * mul;
            s = s.sub(1);
            mul *= 10;
        }

        let mut offset = 0;
        mul = 1;
        s = s.sub(1);
        while *s != b'\n' {
            let d = u64::from((*s).unchecked_sub(b'0'));
            offset += d * mul;
            if s == base {
                break;
            }
            s = s.sub(1);
            mul *= 10;
        }

        (offset, id)
    };

    IndexEntry {
        id: NonZeroU64::new(id).expect("non-zero page id"),
        offset,
    }
}
