//! A data structure for tracking source positions in language implementations,
//! heavily adapted from [codemap](https://crates.io/crates/codemap).

use peg::str::LineCol;

/// A range of text within a string.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Span {
    /// The position in the codemap representing the first byte of the span.
    pub start: usize,

    /// The position after the last byte of the span.
    pub end: usize,
}

impl Span {
    /// Creates a new span.
    #[inline]
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    /// Returns true if this span is empty.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// The length of the span, in bytes.
    #[inline]
    pub fn len(self) -> usize {
        self.end - self.start
    }

    /// Creates a span that encloses both `self` and `other`.
    #[inline]
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    #[inline]
    /// Converts the span into a range that can be used for string indexing.
    // This is not just using `From<core::ops::Range<usize>` because type
    // resolution fails in common use with `.into()` which eliminates any
    // benefit of using a standard conversion trait
    pub fn into_range(self) -> core::ops::Range<usize> {
        self.start..self.end
    }
}

/// Associate a Span with a value of arbitrary type (e.g. an AST node).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct Spanned<T> {
    /// The value.
    pub node: T,
    /// The span.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Creates a new [`Spanned`].
    #[inline]
    pub fn new(node: T, start: usize, end: usize) -> Self {
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
pub(crate) struct FileMap<'a> {
    /// The source file.
    source: &'a str,

    /// Byte positions of line beginnings.
    lines: Vec<u32>,
}

impl core::fmt::Debug for FileMap<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut limit = 100.min(self.source.len());
        while !self.source.is_char_boundary(limit) {
            limit += 1;
        }

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
    /// Creates a new file with the given name, source, and start position.
    pub fn new(source: &'a str) -> Self {
        let lines = core::iter::once(0)
            .chain(
                source
                    .match_indices('\n')
                    .map(|(p, _)| u32::try_from(p + 1).unwrap()),
            )
            .collect();

        Self { source, lines }
    }

    /// Gets the line and column of a Pos.
    ///
    /// # Panics
    ///
    /// * If `pos` is not with this file's span
    /// * If `pos` points to a byte in the middle of a UTF-8 character
    pub fn find_line_col(&self, pos: usize) -> LineCol {
        let line = self.find_line(pos);
        let line_span = self.line_span(line);
        let column = self.source[line_span.start..pos].chars().count();
        LineCol {
            line: line + 1,
            column: column + 1,
            offset: pos,
        }
    }

    /// Gets the line number of a Pos.
    ///
    /// The lines are 0-indexed (first line is numbered 0)
    ///
    /// # Panics
    ///
    ///  * If `pos` is not within this file's span
    fn find_line(&self, pos: usize) -> usize {
        assert!(pos <= self.source.len());
        let pos = u32::try_from(pos).unwrap();
        match self.lines.binary_search(&pos) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    /// Gets the span representing a line by line number.
    ///
    /// The line number is 0-indexed (first line is numbered 0). The returned
    /// span includes the line terminator.
    ///
    /// # Panics
    ///
    ///  * If the line number is out of range
    fn line_span(&self, line: usize) -> Span {
        self.lines
            .get(line)
            .map(|start| Span {
                start: usize::try_from(*start).unwrap(),
                end: self
                    .lines
                    .get(line + 1)
                    .map_or(self.source.len(), |end| usize::try_from(*end).unwrap()),
            })
            .unwrap()
    }
}
