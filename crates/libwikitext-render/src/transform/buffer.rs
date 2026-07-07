//! An intermediate [`Sink`] that temporarily buffers input.

use super::{Sink, debugger::Debugger};
use crate::StripMarker;

/// Buffers a sink sequence for replay.
#[derive(Default)]
pub(super) struct Buffer {
    /// The backing store for the buffer.
    inner: Vec<u8>,
}

impl Buffer {
    /// Hey, look! It’s your old buddy, the C null-string terminator!
    const TERMINATOR: u8 = b'\0';

    /// Clears the buffer.
    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Removes the given byte `range`, returning the removed calls as an
    /// iterator.
    pub fn drain<R: core::ops::RangeBounds<usize>>(&mut self, range: R) -> Drain<'_> {
        let start = match range.start_bound() {
            core::ops::Bound::Included(i) => *i,
            core::ops::Bound::Excluded(i) => *i + 1,
            core::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            core::ops::Bound::Included(i) => *i + 1,
            core::ops::Bound::Excluded(i) => *i,
            core::ops::Bound::Unbounded => self.inner.len(),
        };

        Drain {
            buffer: self,
            end,
            iter: Iter::new(&self.inner[start..end]),
            start,
        }
    }

    /// Returns the length of the buffer up to the first open tag.
    // TODO: This is a ridiculously garbage hack!
    pub fn first_tag_len(&self) -> usize {
        let mut cursor = self.inner.as_slice();
        let mut len = 0;
        while let Some((insn, mut data)) = cursor.split_first() {
            match Insn::try_from(*insn).expect("valid buffer") {
                Insn::CommentEnd | Insn::CommentStart | Insn::NewLine | Insn::TagAttributeEnd => {}
                Insn::Entity
                | Insn::StripMarkerGeneral
                | Insn::StripMarkerNoWiki
                | Insn::StripMarkerWikiRsSourceEnd
                | Insn::StripMarkerWikiRsSourceStart
                | Insn::TagAttributeStart
                | Insn::TagEnd
                | Insn::TagStart
                | Insn::Text => {
                    let s;
                    (s, data) = slice_term(data);
                    len += s.len();
                }
                Insn::TagStartEnd => {
                    return len;
                }
            }
            len += 1;
            cursor = data;
        }

        panic!("no first tag")
    }

    /// Adds `other` to this buffer.
    #[inline]
    pub fn extend(&mut self, other: Self) {
        self.inner.extend(other.inner);
    }

    /// Flushes the buffer to `next`.
    #[inline]
    pub fn flush_into<S: Sink + ?Sized>(&mut self, next: &mut S, skip_first_char: bool) {
        self.write_into(next, skip_first_char);
        self.clear();
    }

    /// Insert more calls at `index`.
    pub fn insert(&mut self, index: usize, f: impl FnOnce(&mut Buffer)) {
        let start = self.inner.len();
        f(self);
        let end = self.inner.len();
        self.inner[index..end].rotate_right(end - start);
    }

    /// Returns an iterator over the calls and their positions, in bytes, in
    /// this buffer.
    pub fn iter(&self) -> Iter<'_> {
        Iter::new(&self.inner)
    }

    /// Returns the length of the buffer, in bytes. This value should be
    /// considered opaque and used only with `insert`.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Splits the buffer in two `at` the given index.
    #[inline]
    pub fn split_off(&mut self, at: usize) -> Buffer {
        Self {
            inner: self.inner.split_off(at),
        }
    }

    /// Writes the buffer to `next` without clearing it.
    pub fn write_into<S: Sink + ?Sized>(&self, next: &mut S, mut skip_first_char: bool) {
        for (_, call) in Iter::new(&self.inner) {
            if let Call::Text(text) = call {
                if skip_first_char {
                    next.text(&text[1..]);
                    skip_first_char = false;
                } else {
                    next.text(text);
                }
            } else {
                call.emit(next);
            }
        }
    }
}

impl core::fmt::Debug for Buffer {
    fn fmt(&self, mut f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        struct NullSink;
        impl Sink for NullSink {
            fn comment_end(&mut self) {}
            fn comment_start(&mut self) {}
            fn entity(&mut self, _: char, _: &str) {}
            fn finish(self) -> String {
                unreachable!()
            }
            fn new_line(&mut self) {}
            fn strip_marker(&mut self, _: &StripMarker<'_>) {}
            fn tag_attribute_end(&mut self, _: &str) {}
            fn tag_attribute_start(&mut self, _: &str) {}
            fn tag_end(&mut self, _: &str) {}
            fn tag_start(&mut self, _: &str) {}
            fn tag_start_end(&mut self, _: &str) {}
            fn text(&mut self, _: &str) {}
        }

        write!(f, "Buffer(")?;
        self.write_into(&mut Debugger::new(&mut f, NullSink), false);
        write!(f, ")")
    }
}

impl Sink for Buffer {
    #[inline]
    fn comment_end(&mut self) {
        self.inner.push(Insn::CommentEnd as u8);
    }

    #[inline]
    fn comment_start(&mut self) {
        self.inner.push(Insn::CommentStart as u8);
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.inner.push(Insn::Entity as u8);
        let mut buffer = [0; 4];
        self.inner
            .extend(value.encode_utf8(&mut buffer[..]).as_bytes());
        self.inner.extend(raw.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn finish(self) -> String {
        panic!("should not call this");
    }

    #[inline]
    fn new_line(&mut self) {
        self.inner.push(Insn::NewLine as u8);
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        let call = match marker {
            StripMarker::General(_) => Insn::StripMarkerGeneral,
            StripMarker::NoWiki(_) => Insn::StripMarkerNoWiki,
            StripMarker::WikiRsSourceEnd(_) => Insn::StripMarkerWikiRsSourceEnd,
            StripMarker::WikiRsSourceStart(_) => Insn::StripMarkerWikiRsSourceStart,
        };
        self.inner.push(call as u8);
        self.inner.extend(marker.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {
        self.inner.push(Insn::TagAttributeEnd as u8);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.inner.push(Insn::TagAttributeStart as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.inner.push(Insn::TagEnd as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.inner.push(Insn::TagStart as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_start_end(&mut self, _: &str) {
        self.inner.push(Insn::TagStartEnd as u8);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.inner.push(Insn::Text as u8);
        self.inner.extend(text.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }
}

/// It generates an enum with a `TryFrom` implementation from a primitive. Wow.
macro_rules! that_enum_thing_that_should_be_in_std {
    ($(#[$meta:meta])* enum $id:ident { $($(#[$var_meta:meta])* $var:ident),* $(,)? }) => {
        $(#[$meta])*
        enum $id {
            $($(#[$var_meta])* $var,)*
        }

        impl TryFrom<u8> for $id {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                $(if value == Self::$var as u8 {
                    Ok(Self::$var)
                } else)* {
                    Err(())
                }
            }
        }
    }
}

/// A buffered sink call.
#[derive(Debug)]
pub enum Call<'a> {
    /// A call to [`Sink::comment_end`].
    CommentEnd,
    /// A call to [`Sink::comment_start`].
    CommentStart,
    /// A call to [`Sink::entity`].
    Entity {
        /// The decoded character value of the entity.
        value: char,
        /// The original raw HTML entity.
        raw: &'a str,
    },
    /// A call to [`Sink::new_line`].
    NewLine,
    /// A call to [`Sink::strip_marker`].
    StripMarker(StripMarker<'a>),
    /// A call to [`Sink::tag_attribute_end`].
    TagAttributeEnd(&'a str),
    /// A call to [`Sink::tag_attribute_start`].
    TagAttributeStart(&'a str),
    /// A call to [`Sink::tag_end`].
    TagEnd(&'a str),
    /// A call to [`Sink::tag_start`].
    TagStart(&'a str),
    /// A call to [`Sink::tag_start_end`].
    TagStartEnd(&'a str),
    /// A call to [`Sink::text`].
    Text(&'a str),
}

impl Call<'_> {
    /// Emits the call to `next`.
    pub fn emit<S: Sink + ?Sized>(&self, next: &mut S) {
        match self {
            Self::CommentEnd => {
                next.comment_end();
            }
            Self::CommentStart => {
                next.comment_start();
            }
            Self::Entity { value, raw } => {
                next.entity(*value, raw);
            }
            Self::NewLine => {
                next.new_line();
            }
            Self::StripMarker(marker) => {
                next.strip_marker(marker);
            }
            Self::TagAttributeEnd(attr_name) => {
                next.tag_attribute_end(attr_name);
            }
            Self::TagAttributeStart(attr_name) => {
                next.tag_attribute_start(attr_name);
            }
            Self::TagEnd(tag_name) => {
                next.tag_end(tag_name);
            }
            Self::TagStart(tag_name) => {
                next.tag_start(tag_name);
            }
            Self::TagStartEnd(tag_name) => {
                next.tag_start_end(tag_name);
            }
            Self::Text(text) => {
                next.text(text);
            }
        }
    }
}

that_enum_thing_that_should_be_in_std! {
    /// A buffered sink instruction.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Insn {
        /// A call to [`Sink::comment_end`].
        CommentEnd,
        /// A call to [`Sink::comment_start`].
        CommentStart,
        /// A call to [`Sink::entity`].
        Entity,
        /// A call to [`Sink::new_line`].
        NewLine,
        /// A call to [`Sink::strip_marker`] with a [`StripMarker::General`].
        StripMarkerGeneral,
        /// A call to [`Sink::strip_marker`] with a [`StripMarker::NoWiki`].
        StripMarkerNoWiki,
        /// A call to [`Sink::strip_marker`] with a
        /// [`StripMarker::WikiRsSourceEnd`].
        StripMarkerWikiRsSourceEnd,
        /// A call to [`Sink::strip_marker`] with a
        /// [`StripMarker::WikiRsSourceStart`].
        StripMarkerWikiRsSourceStart,
        /// A call to [`Sink::tag_attribute_end`].
        TagAttributeEnd,
        /// A call to [`Sink::tag_attribute_start`].
        TagAttributeStart,
        /// A call to [`Sink::tag_end`].
        TagEnd,
        /// A call to [`Sink::tag_start`].
        TagStart,
        /// A call to [`Sink::tag_start_end`].
        TagStartEnd,
        /// A call to [`Sink::text`].
        Text,
    }
}

/// A draining iterator for `Buffer`.
#[derive(Debug)]
pub struct Drain<'a> {
    /// The buffer.
    buffer: *mut Buffer,
    /// The end position, in bytes.
    end: usize,
    /// The call iterator.
    iter: Iter<'a>,
    /// The start position, in bytes.
    start: usize,
}

impl Drop for Drain<'_> {
    fn drop(&mut self) {
        // SAFETY: This is a copy-paste from std::string::Drain.
        unsafe {
            let self_vec = self.buffer.as_mut_unchecked();
            if self.start <= self.end && self.end <= self_vec.len() {
                self_vec.inner.drain(self.start..self.end);
            }
        }
    }
}

impl<'a> Iterator for Drain<'a> {
    type Item = (usize, Call<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl core::iter::FusedIterator for Drain<'_> {}

/// An iterator over buffered sink calls.
#[derive(Debug)]
pub struct Iter<'a> {
    /// The name of the last attribute.
    attr_name: &'a str,
    /// The data.
    data: &'a [u8],
    /// The iterator position in the data.
    pos: usize,
    /// The name of the last tag.
    tag_name: &'a str,
}

impl<'a> Iter<'a> {
    /// Creates a new `BufferIter` over the given `cursor`.
    fn new(data: &'a [u8]) -> Self {
        Self {
            attr_name: <_>::default(),
            data,
            pos: <_>::default(),
            tag_name: <_>::default(),
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = (usize, Call<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let pos = self.pos;
        let (&insn, mut data) = self.data.split_first()?;

        let call = match Insn::try_from(insn).expect("valid buffer") {
            Insn::CommentEnd => Call::CommentEnd,
            Insn::CommentStart => Call::CommentStart,
            Insn::Entity => {
                let value;
                (value, data) = slice_term(data);
                let mut iter = value.chars();
                let value = iter.next().expect("value");
                let raw = iter.as_str();
                Call::Entity { value, raw }
            }
            Insn::NewLine => Call::NewLine,
            Insn::StripMarkerGeneral => {
                let marker;
                (marker, data) = slice_term(data);
                Call::StripMarker(StripMarker::General(marker.into()))
            }
            Insn::StripMarkerNoWiki => {
                let marker;
                (marker, data) = slice_term(data);
                Call::StripMarker(StripMarker::NoWiki(marker.into()))
            }
            Insn::StripMarkerWikiRsSourceEnd => {
                let marker;
                (marker, data) = slice_term(data);
                Call::StripMarker(StripMarker::WikiRsSourceEnd(marker.into()))
            }
            Insn::StripMarkerWikiRsSourceStart => {
                let marker;
                (marker, data) = slice_term(data);
                Call::StripMarker(StripMarker::WikiRsSourceStart(marker.into()))
            }
            Insn::TagAttributeEnd => Call::TagAttributeEnd(self.attr_name),
            Insn::TagAttributeStart => {
                let attr_name;
                (attr_name, data) = slice_term(data);
                self.attr_name = attr_name;
                Call::TagAttributeStart(attr_name)
            }
            Insn::TagEnd => {
                let tag_name;
                (tag_name, data) = slice_term(data);
                Call::TagEnd(tag_name)
            }
            Insn::TagStart => {
                let tag_name;
                (tag_name, data) = slice_term(data);
                self.tag_name = tag_name;
                Call::TagStart(tag_name)
            }
            Insn::TagStartEnd => Call::TagStartEnd(self.tag_name),
            Insn::Text => {
                let text;
                (text, data) = slice_term(data);
                Call::Text(text)
            }
        };

        self.pos += self.data.len() - data.len();
        self.data = data;
        Some((pos, call))
    }
}

impl core::iter::FusedIterator for Iter<'_> {}

/// Splits `data` on a string terminator, returning the string and the remaining
/// data.
fn slice_term(data: &[u8]) -> (&str, &[u8]) {
    let end = data
        .iter()
        .position(|b| *b == Buffer::TERMINATOR)
        .expect("terminator");
    // SAFETY: This data came from a string.
    let value = unsafe { str::from_utf8_unchecked(&data[..end]) };
    (value, &data[end + 1..])
}
