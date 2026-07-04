//! Heading transformers.

use super::{Accumulator, Chain as _, PrettyText, Sink, chainable, tokenise};
use crate::{StripMarker, globals::Outline};
use core::num::NonZeroU32;
use html_escape::encode_double_quoted_attribute;
use libmisc::CowExt as _;
use libwikitext_common::{
    AnchorEncodeMode, decode_html, escape_id, normalize_section_name, title::normalize_fragment,
};
use libwikitext_parse::HeadingLevel;
use std::borrow::Cow;

/// Creates a table of contents.
///
/// Since both Wikitext headings and HTML headings contribute to the outline,
/// and the HTML contents of a heading are used both to create the outline entry
/// as well as generate implicit IDs and stop (some) ID collisions, this process
/// is implemented as an HTML sink instead an AST visitor.
#[derive(Debug)]
pub(crate) struct OutlineGenerator<'a, S: Sink> {
    /// The buffer for the currently processing entries.
    buffer: Vec<Entry>,
    /// If true, processing a strip marker.
    ///
    /// The IDs of heading tags inside of strip markers are supposed to be fixed
    /// up, but they are not supposed to go in the outline, so this need to be
    /// tracked.
    in_strip_marker: bool,
    /// The output.
    next: S,
    /// The global outline.
    outline: &'a mut Outline,
    /// The generator state.
    state: State,
}

impl<'a, S: Sink> OutlineGenerator<'a, S> {
    /// Creates a new `OutlineGenerator` chained to `next`, emitting entries to
    /// `outline`.
    pub fn new(outline: &'a mut Outline, next: S) -> Self {
        Self {
            buffer: <_>::default(),
            in_strip_marker: <_>::default(),
            next,
            outline,
            state: <_>::default(),
        }
    }

    /// Adds the given `text` to the currently processing outline entry.
    fn add_text(&mut self, text: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.text(text);
            outline.outline_html.text(text);
            if self.state == State::Body
                && let Id::Implicit(id) = &mut outline.id
            {
                id.push_str(text);
            } else if self.state == State::StartId
                && let Id::Explicit(id) = &mut outline.id
            {
                id.push_str(text);
            }
        }
    }

    /// Saves the given `entry` to the global outline and emits the buffered
    /// HTML to the next sink.
    fn save_entry(&mut self, entry: Entry) {
        let id = match &entry.id {
            Id::Implicit(id) => normalize_section_name(id).map(normalize_fragment),
            Id::Explicit(id) => Cow::Borrowed(id.as_str()),
        };

        let html_id = escape_id(&id, AnchorEncodeMode::Html5);
        let legacy_id = {
            let id = escape_id(&id, AnchorEncodeMode::Legacy);
            (id != html_id).then(|| id.map(|id| self.outline.unique_id(id)))
        };
        let id = self.outline.unique_id(&html_id);

        if !self.in_strip_marker {
            let html = entry.outline_html.finish();
            self.outline.push(entry.level, html.trim_ascii(), &id);
        }

        let mut html = entry.document_html.finish();

        debug_assert!(entry.body_start > entry.id_start);

        if let Some(legacy_id) = legacy_id {
            html.insert_str(
                entry.body_start as usize,
                &format!(
                    r#"<span id="{}"></span>"#,
                    encode_double_quoted_attribute(&legacy_id)
                ),
            );
        }

        #[rustfmt::skip]
        if let Cow::Owned(id) = &id && let Some(end) = entry.id_end {
            html.replace_range(entry.id_start as usize..u32::from(end) as usize, id);
        } else if entry.id_end.is_none() {
            let id = format!(r#" id="{}""#, encode_double_quoted_attribute(&id));
            html.insert_str(entry.id_start as usize, &id);
        };

        // TODO: It would be faster and probably better to use a `Buffer`.
        tokenise(&mut self.next, &html);
    }
}

chainable!(OutlineGenerator<'a, S>);

impl<S: Sink> Sink for OutlineGenerator<'_, S> {
    fn comment_end(&mut self) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.comment_end();
            outline.outline_html.comment_end();
        } else {
            self.next.comment_end();
        }
    }

    fn comment_start(&mut self) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.comment_start();
            outline.outline_html.comment_start();
        } else {
            self.next.comment_start();
        }
    }

    fn entity(&mut self, value: char, raw: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.entity(value, raw);
            outline.outline_html.entity(value, raw);
            if self.state == State::Body
                && let Id::Implicit(id) = &mut outline.id
            {
                id.push(value);
            } else if self.state == State::StartId
                && let Id::Explicit(id) = &mut outline.id
            {
                id.push(value);
            }
        } else {
            self.next.entity(value, raw);
        }
    }

    #[inline]
    fn finish(self) -> String {
        debug_assert!(self.buffer.is_empty());
        self.next.finish()
    }

    fn new_line(&mut self) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.new_line();
            outline.outline_html.new_line();
        } else {
            self.next.new_line();
        }
    }

    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        match marker {
            // Even if this is not processing any heading, tokenising is needed
            // by subsequent sinks, so just do it here always
            // TODO: OR MAYBE DON’T SINCE THE TOKENISER SCREWS WITH STUFF
            StripMarker::General(html) => {
                self.in_strip_marker = true;
                tokenise(self, html);
                self.in_strip_marker = false;
            }
            StripMarker::NoWiki(text) => {
                if self.buffer.is_empty() {
                    self.next.strip_marker(marker);
                } else {
                    self.add_text(&decode_html(text));
                }
            }
            StripMarker::WikiRsSourceEnd(_) | StripMarker::WikiRsSourceStart(_) => {
                if let Some(outline) = self.buffer.last_mut() {
                    outline.document_html.strip_marker(marker);
                } else {
                    self.next.strip_marker(marker);
                }
            }
        }
    }

    fn tag_attribute_end(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.tag_attribute_end(name);
            outline.outline_html.tag_attribute_end(name);
            if self.state == State::StartId {
                outline.id_end = NonZeroU32::new(outline.document_html.len());
                self.state = State::Start;
            } else if self.state == State::StartAttr {
                self.state = State::Start;
            } else {
                self.state = State::Body;
            }
        } else {
            self.next.tag_attribute_end(name);
        }
    }

    fn tag_attribute_start(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            if self.state == State::Start {
                if name == "id" {
                    self.state = State::StartId;
                    outline.id = Id::Explicit(<_>::default());
                    outline.id_start = outline.document_html.len();
                } else {
                    self.state = State::StartAttr;
                }
            } else {
                self.state = State::BodyAttr;
            }
            outline.document_html.tag_attribute_start(name);
            outline.outline_html.tag_attribute_start(name);
        } else {
            self.next.tag_attribute_start(name);
        }
    }

    fn tag_end(&mut self, name: &str) {
        if let Some(mut outline) = self.buffer.pop_if(|state| state.level.tag_name() == name) {
            outline.document_html.tag_end(name);
            self.save_entry(outline);
        } else if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.tag_end(name);
            outline.outline_html.tag_end(name);
        } else {
            self.next.tag_end(name);
        }
    }

    fn tag_start(&mut self, name: &str) {
        if let Ok(level) = name.parse() {
            let mut outline = Entry::new(level);
            outline.document_html.tag_start(name);
            self.buffer.push(outline);
            self.state = State::Start;
        } else if let Some(outline) = self.buffer.last_mut() {
            outline.document_html.tag_start(name);
            outline.outline_html.tag_start(name);
        } else {
            self.next.tag_start(name);
        }
    }

    fn tag_start_end(&mut self, name: &str) {
        if let Some(outline) = self.buffer.last_mut() {
            if self.state == State::Start && outline.id_end.is_none() {
                outline.id_start = outline.document_html.len();
            }
            outline.document_html.tag_start_end(name);
            if self.state == State::Start {
                outline.body_start = outline.document_html.len();
                self.state = State::Body;
            } else {
                outline.outline_html.tag_start_end(name);
            }
        } else {
            self.next.tag_start_end(name);
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.buffer.is_empty() {
            self.next.text(text);
        } else {
            self.add_text(text);
        }
    }
}

/// An outline entry.
#[derive(Debug)]
struct Entry {
    /// The position immediately after the heading tag in the document HTML.
    /// This is where an element for legacy anchor IDs will be injected if
    /// needed.
    body_start: u32,
    /// The accumulator for the heading tag emitted to the document.
    document_html: Accumulator,
    /// The plain text anchor ID for the outline.
    id: Id,
    /// The end position of the ID in the document HTML. If `None`, the ID was
    /// implicit, and needs to be inserted.
    id_end: Option<NonZeroU32>,
    /// The start position of the ID in the document HTML.
    id_start: u32,
    /// The accumulator for the outline entry emitted to the outline.
    outline_html: EntryAccumulator,
    /// The level of the heading tag.
    level: HeadingLevel,
}

impl Entry {
    /// Creates a new `OutlineEntry`.
    fn new(level: HeadingLevel) -> Self {
        Self {
            body_start: <_>::default(),
            document_html: <_>::default(),
            id: <_>::default(),
            id_end: <_>::default(),
            id_start: <_>::default(),
            outline_html: <_>::default(),
            level,
        }
    }
}

/// Accumulates the HTML for an outline entry, filtering tags (but not their
/// contents) and most tag attributes.
#[derive(Debug)]
struct EntryAccumulator {
    /// The accumulator for the outline entry.
    acc: PrettyText<Accumulator>,
    /// The last tag’s inner body position. Used to detect empty tags.
    body_pos: u32,
    /// The filter counter.
    filtering: u8,
    /// If true, processing a `<span>` start tag.
    in_span: bool,
    /// The last tag’s outer start position. Used to truncate empty tags.
    start_pos: u32,
}

impl EntryAccumulator {
    /// The list of tags allowed in outlines.
    ///
    /// This list comes from Parsoid `Wt2Html\DOM\Handlers\Headings`.
    const ALLOWED_TAGS: phf::Set<&str> = phf::phf_set! {
        "b", "bdi", "i", "q", "s", "span", "strike", "sub", "sup"
    };
}

impl Default for EntryAccumulator {
    fn default() -> Self {
        Self {
            acc: PrettyText::new(<_>::default()),
            body_pos: <_>::default(),
            filtering: <_>::default(),
            in_span: <_>::default(),
            start_pos: <_>::default(),
        }
    }
}

impl Sink for EntryAccumulator {
    #[inline]
    fn comment_end(&mut self) {}

    #[inline]
    fn comment_start(&mut self) {}

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.filtering == 0 {
            self.acc.entity(value, raw);
        }
    }

    #[inline]
    fn finish(self) -> String {
        self.acc.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        // Because the legacy ID mode is sensitive to newline characters, this
        // has to be emitted, even though it is nonsensical in the normal
        // context of an outline entry
        if self.filtering == 0 {
            self.acc.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, _: &StripMarker<'_>) {
        panic!("strip markers should not be sent here");
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if self.in_span && name == "dir" {
            self.acc.tag_attribute_end(name);
        } else {
            self.filtering -= 1;
        }
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if self.in_span && name == "dir" {
            self.acc.tag_attribute_start(name);
        } else {
            self.filtering += 1;
        }
    }

    fn tag_end(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            // If multiple nested tags are empty, the inner end tag will
            // truncate back to `start_pos` (`body_pos == len`), and then the
            // second one also needs to be suppressed (`body_pos > len`)
            if self.body_pos == self.acc.next().len() {
                // Empty tags are filtered out
                self.acc.next_mut().truncate(self.start_pos);
            } else if self.body_pos < self.acc.next().len() {
                self.acc.tag_end(name);
            }
        }
    }

    fn tag_start(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            let pos = self.acc.next().len();
            // If multiple nested tags are empty, they should be all be removed,
            // not just the innermost one
            if pos != self.body_pos {
                self.start_pos = self.acc.next().len();
            }
            self.acc.tag_start(name);
            self.in_span = name == "span";
        } else {
            self.filtering += 1;
        }
    }

    fn tag_start_end(&mut self, name: &str) {
        if Self::ALLOWED_TAGS.contains(name) {
            self.acc.tag_start_end(name);
            self.body_pos = self.acc.next().len();
        } else {
            self.filtering -= 1;
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.filtering == 0 {
            self.acc.text(text);
        }
    }
}

/// An outline entry ID.
#[derive(Debug)]
enum Id {
    /// The ID is generated implicitly from the body of the heading.
    Implicit(String),
    /// The ID is taken explicitly from an `id` attribute.
    Explicit(String),
}

impl Default for Id {
    fn default() -> Self {
        Self::Implicit(<_>::default())
    }
}

/// The state of an [`OutlineEmitter`].
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum State {
    /// In some other tag state.
    #[default]
    Body,
    /// In an attribute for some other tag.
    BodyAttr,
    /// In a new `<hN>` start tag.
    Start,
    /// In a new `<hN>` stat tag attribute.
    StartAttr,
    /// In the `id` attribute of a new `<hN>` start tag.
    StartId,
}
