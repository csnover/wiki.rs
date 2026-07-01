//! A bookmarkable string type.

use super::Chain;
use core::fmt;

/// A back-propagating bookmarker of output positions. Used to inject additional
/// unstructured HTML without buffering.
pub(super) trait Markable {
    /// Frees the given `mark` for reuse. This is a performance optimisation.
    fn free_mark(&mut self, mark: Mark);

    /// Mark the current output position for later investigation.
    fn mark(&mut self) -> Mark;

    /// Runs the callback `f` with the resolved positions for the given `marks`
    /// and a mutable reference to the corresponding `MarkableString`.
    fn with_marks<const N: usize, F: FnOnce([Option<usize>; N], &mut MarkableString) -> T, T>(
        &mut self,
        marks: [&Mark; N],
        f: F,
    ) -> T;
}

impl<T> Markable for T
where
    T: Chain,
    T::Next: Markable,
{
    #[inline]
    fn free_mark(&mut self, mark: Mark) {
        self.next_mut().free_mark(mark);
    }

    #[inline]
    fn mark(&mut self) -> Mark {
        self.next_mut().mark()
    }

    #[inline]
    fn with_marks<const N: usize, F: FnOnce([Option<usize>; N], &mut MarkableString) -> U, U>(
        &mut self,
        marks: [&Mark; N],
        f: F,
    ) -> U {
        self.next_mut().with_marks(marks, f)
    }
}

/// A string wrapper where positions can be bookmarked and retrieved later. The
/// bookmarked positions are automatically adjusted in response to mutations to
/// the underlying string. To reduce memory use, the size of the underlying
/// string is limited to [`i32::MAX`] bytes, and there can be no more than
/// [`u16::MAX`]`- 1` bookmarks.
///
/// You may be asking yourself: boy, this sure seems janky. Well, that’s not a
/// question. But I understand what you mean. Obviously the ‘pure’ way to do
/// this is to have any of the earlier handlers buffer outputs until they have
/// everything they need. Very good, such computer science, much purity. But
/// it is more efficient (citation needed) to allow everything to flow down to
/// the single final String allocation instead of having a bunch of intermediate
/// buffers.
///
/// Now you might say something like, “well, you know, you could just use a bump
/// allocator and then all your buffers end up in a contiguous allocation and
/// also computers are fast and so it is like not much of a big deal”. And then
/// I would say, well, the code that injected stuff into strings was already
/// written that way, and this was easier than rewriting everything right now.
#[derive(Clone, Debug)]
pub(super) struct MarkableString {
    /// The underlying string buffer.
    inner: String,
    /// The next free mark index, or [`Self::NO_FREE`] if [`marks`](Self::marks)
    /// needs to be resized.
    next_free: u16,
    /// An packed unordered list of marked positions interleaved with a free
    /// list.
    marks: Vec<u32>,
}

impl MarkableString {
    /// Flag for marks that are actually free list entries.
    const FREE_BIT: u32 = 0x8000_0000;
    /// Sentinel value for marks which were invalidated by range deletion.
    const INVALID: u32 = i32::MAX as u32;
    /// Marker for the end of the free list.
    const NO_FREE: u16 = u16::MAX;

    /// Updates the positions of marks above `start`
    fn adjust_marks(&mut self, start: u32, delta: i32) {
        for pos in self.iter_marks_mut(start) {
            *pos = pos.checked_add_signed(delta).unwrap_or(Self::INVALID);
        }
    }

    /// Releases the given mark to the free pool.
    // TODO: It is bad that this has to be done manually, marks will leak!
    #[inline]
    pub fn free_mark(&mut self, mut mark: Mark) {
        self.marks[usize::from(mark.0)] = Self::FREE_BIT | u32::from(self.next_free);
        self.next_free = mark.0;
        if cfg!(debug_assertions) {
            mark.0 = Self::NO_FREE;
        }
    }

    /// Returns the length of this `MarkableString` in bytes.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Inserts a mark at the given position `pos`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "free list entries only want the low word"
    )]
    fn insert_mark(&mut self, pos: u32) -> Mark {
        if self.next_free == Self::NO_FREE {
            let mark = Mark(u16::try_from(self.marks.len()).unwrap());
            assert!(mark.0 < Self::NO_FREE, "too many marks");
            self.marks.push(pos);
            mark
        } else {
            let mark = Mark(self.next_free);
            let slot = &mut self.marks[usize::from(self.next_free)];
            debug_assert!(*slot & Self::FREE_BIT != 0);
            self.next_free = *slot as u16;
            *slot = pos;
            mark
        }
    }

    /// Inserts `string` at byte position `idx`.
    #[inline]
    pub fn insert_str(&mut self, idx: usize, string: &str) {
        if string.is_empty() {
            return;
        }
        self.inner.insert_str(idx, string);
        let delta = i32::try_from(string.len()).unwrap();
        self.adjust_marks(u32::try_from(idx).unwrap(), delta);
    }

    /// Returns the underlying `String`, consuming this object.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> String {
        self.inner
    }

    /// Returns a mutable iterator over all mark positions above `start`.
    fn iter_marks_mut(&mut self, start: u32) -> impl Iterator<Item = &'_ mut u32> {
        self.marks
            .iter_mut()
            .rev()
            .filter(move |&&mut pos| pos & Self::FREE_BIT == 0 && pos > start)
    }

    /// Returns a new mark corresponding to the current length of the string.
    #[must_use]
    pub fn mark(&mut self) -> Mark {
        let pos = u32::try_from(self.inner.len()).unwrap();
        self.insert_mark(pos)
    }

    /// Appends `ch` to the string.
    #[inline]
    pub fn push(&mut self, ch: char) {
        self.inner.push(ch);
    }

    /// Appends `string` to the string.
    #[inline]
    pub fn push_str(&mut self, string: &str) {
        self.inner.push_str(string);
    }

    /// Returns the byte position of the given `mark` in the string, or
    /// `None` if the bookmarked position was erased by [`Self::remove`] or
    /// [`Self::replace_range`].
    pub fn restore_mark(&self, mark: &Mark) -> Option<usize> {
        self.marks.get(usize::from(mark.0)).and_then(|&pos| {
            assert!(pos & Self::FREE_BIT == 0, "mark use-after-free");
            (pos != Self::INVALID).then_some(pos as usize)
        })
    }

    /// Shortens this `MarkableString` to the specified length.
    #[inline]
    pub fn truncate(&mut self, new_len: usize) {
        self.inner.truncate(new_len);
    }
}

impl Default for MarkableString {
    fn default() -> Self {
        Self {
            inner: <_>::default(),
            next_free: Self::NO_FREE,
            marks: <_>::default(),
        }
    }
}

impl<I> core::ops::Index<I> for MarkableString
where
    I: core::slice::SliceIndex<str>,
{
    type Output = <I as core::slice::SliceIndex<str>>::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        self.inner.index(index)
    }
}

impl fmt::Write for MarkableString {
    #[inline]
    fn write_char(&mut self, c: char) -> fmt::Result {
        self.inner.write_char(c)
    }

    #[inline]
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        self.inner.write_fmt(args)
    }

    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.inner.write_str(s)
    }
}

/// A bookmarked position in a string.
#[derive(Debug)]
pub(super) struct Mark(u16);

#[cfg(debug_assertions)]
impl Drop for Mark {
    #[track_caller]
    fn drop(&mut self) {
        if !std::thread::panicking() && self.0 != MarkableString::NO_FREE {
            log::warn!("leaked {}", self.0);
        }
    }
}
