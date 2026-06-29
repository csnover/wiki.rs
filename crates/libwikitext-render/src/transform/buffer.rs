//! An intermediate [`Sink`] that temporarily buffers input.

use super::{Accumulator, Sink};
use crate::StripMarker;
use libwikitext_parse::MARKER_PREFIX;

/// Buffers a sink sequence for replay.
#[derive(Default)]
pub(super) struct Buffer {
    /// If true, the buffer received anything which is non-ASCII whitespace.
    contains_non_ascii_whitespace: bool,
    /// The backing store for the buffer.
    inner: Vec<u8>,
    /// The first character received by the buffer.
    starts_with: Option<char>,
}

impl Buffer {
    /// Hey, look! It’s your old buddy, the C null-string terminator!
    const TERMINATOR: u8 = b'\0';

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.contains_non_ascii_whitespace = false;
        self.inner.clear();
        self.starts_with = None;
    }

    /// Returns true if the buffer contains any non-ASCII-whitespace items.
    pub fn contains_non_ascii_whitespace(&self) -> bool {
        self.contains_non_ascii_whitespace
    }

    /// Flushes the buffer to `next`.
    pub fn flush<S: Sink + ?Sized>(&mut self, next: &mut S, skip_first_char: bool) {
        self.write(next, skip_first_char);
        self.clear();
    }

    /// Writes the buffer to `next` without clearing it.
    pub fn write<S: Sink + ?Sized>(&self, next: &mut S, mut skip_first_char: bool) {
        fn slice_term(data: &[u8]) -> (&str, &[u8]) {
            let end = data
                .iter()
                .position(|b| *b == Buffer::TERMINATOR)
                .expect("terminator");
            // SAFETY: This data came from a string.
            let value = unsafe { str::from_utf8_unchecked(&data[..end]) };
            (value, &data[end + 1..])
        }

        let mut cursor = self.inner.as_slice();
        let mut tag_name = "";
        let mut attr_name = "";
        while let Some((insn, mut data)) = cursor.split_first() {
            match BufferInsn::try_from(*insn).expect("valid buffer") {
                BufferInsn::CommentEnd => {
                    next.comment_end();
                }
                BufferInsn::CommentStart => {
                    next.comment_start();
                }
                BufferInsn::Entity => {
                    let value;
                    (value, data) = slice_term(data);
                    let mut iter = value.chars();
                    let value = iter.next().expect("value");
                    let raw = iter.as_str();
                    next.entity(value, raw);
                }
                BufferInsn::NewLine => {
                    next.new_line();
                }
                BufferInsn::StripMarkerGeneral => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::General(marker.into()));
                }
                BufferInsn::StripMarkerNoWiki => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::NoWiki(marker.into()));
                }
                BufferInsn::StripMarkerWikiRsSourceEnd => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::WikiRsSourceEnd(marker.into()));
                }
                BufferInsn::StripMarkerWikiRsSourceStart => {
                    let marker;
                    (marker, data) = slice_term(data);
                    next.strip_marker(&StripMarker::WikiRsSourceStart(marker.into()));
                }
                BufferInsn::TagAttributeEnd => {
                    next.tag_attribute_end(attr_name);
                }
                BufferInsn::TagAttributeStart => {
                    (attr_name, data) = slice_term(data);
                    next.tag_attribute_start(attr_name);
                }
                BufferInsn::TagEnd => {
                    (tag_name, data) = slice_term(data);
                    next.tag_end(tag_name);
                }
                BufferInsn::TagStart => {
                    (tag_name, data) = slice_term(data);
                    next.tag_start(tag_name);
                }
                BufferInsn::TagStartEnd => {
                    next.tag_start_end(tag_name);
                }
                BufferInsn::Text => {
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

    /// Updates some metadata used by `GrafEmitter`, which was originally a hack
    /// and so seems to always require a hack *somewhere* to function.
    pub fn update_metadata(&mut self, text: &str) {
        if !self.contains_non_ascii_whitespace {
            self.contains_non_ascii_whitespace = text.bytes().any(|b| !b.is_ascii_whitespace());
            if self.starts_with.is_none() {
                self.starts_with = text.chars().next();
            }
        }
    }

    /// Returns true if the contents of the buffer start with the given
    /// character.
    pub fn starts_with(&self, c: char) -> bool {
        self.starts_with.is_some_and(|ch| ch == c)
    }
}

impl core::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut inner = Accumulator::new();
        self.write(&mut inner, false);
        f.debug_struct("Buffer")
            .field(
                "contains_non_ascii_whitespace",
                &self.contains_non_ascii_whitespace,
            )
            .field("inner", &inner.as_str())
            .field("starts_with", &self.starts_with)
            .finish()
    }
}

impl Sink for Buffer {
    #[inline]
    fn comment_end(&mut self) {
        self.update_metadata("<");
        self.inner.push(BufferInsn::CommentEnd as u8);
    }

    #[inline]
    fn comment_start(&mut self) {
        self.update_metadata("<");
        self.inner.push(BufferInsn::CommentStart as u8);
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.update_metadata("&");
        self.inner.push(BufferInsn::Entity as u8);
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
        self.update_metadata("\n");
        self.inner.push(BufferInsn::NewLine as u8);
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        let insn = match marker {
            StripMarker::General(s) => {
                // In the original parser, general markers are unstripped for
                // GrafEmitter
                self.update_metadata(s);
                BufferInsn::StripMarkerGeneral
            }
            StripMarker::NoWiki(_) => {
                // In the original parser, nowiki markers are still markers for
                // GrafEmitter
                self.update_metadata(MARKER_PREFIX);
                BufferInsn::StripMarkerNoWiki
            }
            StripMarker::WikiRsSourceEnd(_) => BufferInsn::StripMarkerWikiRsSourceEnd,
            StripMarker::WikiRsSourceStart(_) => BufferInsn::StripMarkerWikiRsSourceStart,
        };
        self.inner.push(insn as u8);
        self.inner.extend(marker.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {
        self.inner.push(BufferInsn::TagAttributeEnd as u8);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.inner.push(BufferInsn::TagAttributeStart as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.update_metadata("<");
        self.inner.push(BufferInsn::TagEnd as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.update_metadata("<");
        self.inner.push(BufferInsn::TagStart as u8);
        self.inner.extend(name.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }

    #[inline]
    fn tag_start_end(&mut self, _: &str) {
        self.inner.push(BufferInsn::TagStartEnd as u8);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.update_metadata(text);
        self.inner.push(BufferInsn::Text as u8);
        self.inner.extend(text.as_bytes());
        self.inner.push(Self::TERMINATOR);
    }
}

/// It generates an enum with a `TryFrom` implementation from a primitive. Wow.
macro_rules! that_enum_thing_that_should_be_in_std {
    ($(#[$meta:meta])* enum $id:ident { $($var:ident),* $(,)? }) => {
        $(#[$meta])*
        enum $id {
            $($var,)*
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
    enum BufferInsn {
        CommentEnd,
        CommentStart,
        Entity,
        NewLine,
        StripMarkerGeneral,
        StripMarkerNoWiki,
        StripMarkerWikiRsSourceEnd,
        StripMarkerWikiRsSourceStart,
        TagAttributeEnd,
        TagAttributeStart,
        TagEnd,
        TagStart,
        TagStartEnd,
        Text,
    }
}
