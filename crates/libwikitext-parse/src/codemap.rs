//! A data structure for tracking source positions in language implementations,
//! heavily adapted from [codemap](https://crates.io/crates/codemap).

use peg::str::LineCol;

/// A range of text within a string.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct Span {
    /// The position in the codemap representing the first byte of the span.
    pub start: u32,
    /// The position after the last byte of the span.
    pub end: u32,
}

impl Span {
    /// Creates a new span.
    #[inline]
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        Span { start, end }
    }

    #[inline]
    #[must_use]
    /// Converts the span into a range that can be used for string indexing.
    // This is not just using `From<core::ops::Range<usize>>` because type
    // resolution fails in common use with `.into()` which eliminates any
    // benefit of using a standard conversion trait
    pub fn into_range(self) -> core::ops::Range<usize> {
        self.start as usize..self.end as usize
    }

    /// Returns true if this span is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// The length of the span, in bytes.
    #[inline]
    #[must_use]
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Creates a span that encloses both `self` and `other`.
    #[inline]
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Associate a Span with a value of arbitrary type (e.g. an AST node).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Spanned<T> {
    /// The value.
    pub node: T,
    /// The span.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Creates a new [`Spanned`].
    #[inline]
    pub fn new(node: T, start: u32, end: u32) -> Self {
        Self {
            node,
            span: Span { start, end },
        }
    }

    /// Maps a `Spanned<T>` to `Spanned<U>` by applying the function to the node,
    /// leaving the span untouched.
    pub fn map_node<U, F: FnOnce(T) -> U>(self, op: F) -> Spanned<U> {
        Spanned {
            node: op(self.node),
            span: self.span,
        }
    }
}

impl<T> core::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.node
    }
}

/// A record of a source file’s lines.
#[derive(Clone)]
pub struct FileMap<'a> {
    /// Byte positions of line beginnings.
    lines: Vec<u32>,
    /// The source file.
    source: &'a str,
}

impl core::fmt::Debug for FileMap<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let limit = self.source.ceil_char_boundary(100);

        f.debug_struct("FileMap")
            .field(
                "source",
                &format!(
                    "{}{}",
                    &self.source[..limit],
                    if self.source.len() > limit { "…" } else { "" }
                ),
            )
            .finish()
    }
}

impl core::ops::Deref for FileMap<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.source
    }
}

impl<'a> FileMap<'a> {
    /// Creates a new file map with the given source.
    ///
    /// # Panics
    ///
    /// * `source` contains more than 2**32 newline characters
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        let lines = core::iter::once(0)
            .chain(
                source
                    .match_indices('\n')
                    .map(|(p, _)| u32::try_from(p + 1).unwrap()),
            )
            .collect();

        Self { lines, source }
    }

    /// Gets the line number of `pos`.
    ///
    /// The lines are 0-indexed (first line is numbered 0).
    ///
    /// # Panics
    ///
    /// * `pos` is out of range of the source
    #[must_use]
    fn find_line(&self, pos: u32) -> usize {
        assert!(pos as usize <= self.source.len());
        match self.lines.binary_search(&pos) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    /// Gets the line and character column of byte `pos`.
    ///
    /// # Panics
    ///
    /// * `pos` is out of range of the source
    /// * `pos` is in the middle of a multi-byte character
    #[must_use]
    pub fn find_line_col(&self, pos: u32) -> LineCol {
        let line = self.find_line(pos);
        let line_span = self.line_span(line);
        let column = self.source[line_span.start as usize..pos as usize]
            .chars()
            .count();
        LineCol {
            line: line + 1,
            column: column + 1,
            offset: pos as usize,
        }
    }

    /// Gets the span representing a line by line number.
    ///
    /// The line number is 0-indexed (first line is numbered 0). The returned
    /// span includes the line terminator.
    ///
    /// # Panics
    ///
    /// * `line` is out of range of the source
    #[must_use]
    fn line_span(&self, line: usize) -> Span {
        self.lines
            .get(line)
            .map(|start| Span {
                start: *start,
                end: self
                    .lines
                    .get(line + 1)
                    .map_or(u32::try_from(self.source.len()).unwrap(), |end| *end),
            })
            .unwrap()
    }
}
