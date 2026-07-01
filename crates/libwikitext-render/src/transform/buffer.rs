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

    /// Returns the length of the buffer up to the first open tag.
    // TODO: This is a ridiculously garbage hack!
    pub fn first_tag_len(&self) -> usize {
        let mut cursor = self.inner.as_slice();
        let mut len = 0;
        while let Some((insn, mut data)) = cursor.split_first() {
            match Call::try_from(*insn).expect("valid buffer") {
                Call::CommentEnd | Call::CommentStart | Call::NewLine | Call::TagAttributeEnd => {}
                Call::Entity
                | Call::StripMarkerGeneral
                | Call::StripMarkerNoWiki
                | Call::StripMarkerWikiRsSourceEnd
                | Call::StripMarkerWikiRsSourceStart
                | Call::TagAttributeStart
                | Call::TagEnd
                | Call::TagStart
                | Call::Text => {
                    let s;
                    (s, data) = slice_term(data);
                    len += s.len();
                }
                Call::TagStartEnd => {
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
        let mut cursor = self.inner.as_slice();
        let mut tag_name = "";
        let mut attr_name = "";
        while let Some((insn, mut data)) = cursor.split_first() {
            match Call::try_from(*insn).expect("valid buffer") {
                Call::CommentEnd => {
                    next.comment_end();
                }
                Call::CommentStart => {
                    next.comment_start();
                }
                Call::Entity => {
                    let value;
                    (value, data) = slice_term(data);
                    let mut iter = value.chars();
                    let value = iter.next().expect("value");
                    let raw = iter.as_str();
                    next.entity(value, raw);
                }
                Call::NewLine => {
                    next.new_line();
                }
                Call::StripMarkerGeneral => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::General(marker.into()));
                }
                Call::StripMarkerNoWiki => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::NoWiki(marker.into()));
                }
                Call::StripMarkerWikiRsSourceEnd => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::WikiRsSourceEnd(marker.into()));
                }
                Call::StripMarkerWikiRsSourceStart => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::WikiRsSourceStart(marker.into()));
                }
                Call::TagAttributeEnd => {
                    next.tag_attribute_end(attr_name);
                }
                Call::TagAttributeStart => {
                    (attr_name, data) = slice_term(data);
                    next.tag_attribute_start(attr_name);
                }
                Call::TagEnd => {
                    (tag_name, data) = slice_term(data);
                    next.tag_end(tag_name);
                }
                Call::TagStart => {
                    (tag_name, data) = slice_term(data);
                    next.tag_start(tag_name);
                }
                Call::TagStartEnd => {
                    next.tag_start_end(tag_name);
                }
                Call::Text => {
                    let text;
                    (text, data) = slice_term(data);
                    if skip_first_char {
                        skip_first_char = false;
                        next.text(&text[1..]);
                    } else {
                        next.text(text);
                    }
                }
            }
            cursor = data;
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
        self.inner.push(Call::CommentEnd as u8);
    }

    #[inline]
    fn comment_start(&mut self) {
        self.inner.push(Call::CommentStart as u8);
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.inner.push(Call::Entity as u8);
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
        self.inner.push(Call::NewLine as u8);
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        let call = match marker {
            StripMarker::General(_) => Call::StripMarkerGeneral,
            StripMarker::NoWiki(_) => Call::StripMarkerNoWiki,
            StripMarker::WikiRsSourceEnd(_) => Call::StripMarkerWikiRsSourceEnd,
            StripMarker::WikiRsSourceStart(_) => Call::StripMarkerWikiRsSourceStart,
        };
        self.inner.push(call as u8);
        self.inner.extend(marker.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {
        self.inner.push(Call::TagAttributeEnd as u8);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.inner.push(Call::TagAttributeStart as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.inner.push(Call::TagEnd as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.inner.push(Call::TagStart as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_start_end(&mut self, _: &str) {
        self.inner.push(Call::TagStartEnd as u8);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.inner.push(Call::Text as u8);
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

that_enum_thing_that_should_be_in_std! {
    /// A buffered sink instruction.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Call {
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
