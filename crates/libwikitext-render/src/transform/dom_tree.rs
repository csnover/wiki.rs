//! HTML5(-ish) tree transformer.

use super::{
    Buffer, Chain, Sink, chainable,
    markable_string::{Mark, Markable},
};
use crate::StripMarker;
use core::num::NonZeroU8;
use indexmap::IndexSet;
use libwikitext_parse::VOID_TAGS;
use uncased::{Uncased, UncasedStr};

/// A “list of active formatting elements”.
#[derive(Debug, Default)]
struct DomTreeFormattingList {
    /// The buffer for active formatting elements’ attributes. Since most
    /// formatting elements have no attributes, this should be small and rarely
    /// allocated.
    attributes: String,
    /// If true, currently buffering the attributes of a formatting element.
    buffering: bool,
    /// The active formatting elements.
    elements: Vec<DomTreeFormattingItem>,
    /// The index of the rightmost marker in [`Self::elements`].
    marker_index: Option<u8>,
}

impl DomTreeFormattingList {
    /// Marker in [`Self::attributes`] for the end of an attribute value.
    const END_OF_ATTR: char = '\0';
    /// Marker in [`Self::attributes`] for the end of an attribute list.
    const END_OF_ATTRS: char = '\x01';
    /// Marker in [`Self::attributes`] for the end of an attribute name.
    const END_OF_NAME: char = '=';

    /// Returns the index of the first item in [`Self::elements`] after the
    /// rightmost marker.
    #[inline]
    fn after_marker(&self) -> usize {
        self.marker_index.map_or(0, |index| usize::from(index) + 1)
    }

    /// Pushes `value` to the attributes buffer if a new formatting element is
    /// being buffered.
    #[inline]
    fn buffer_char(&mut self, value: char) {
        if self.buffering {
            self.attributes.push(value);
        }
    }

    /// Pushes `text` to the attributes buffer if a new formatting element is
    /// being buffered.
    #[inline]
    fn buffer_text(&mut self, text: &str) {
        if self.buffering {
            self.attributes += text;
        }
    }

    /// Truncates the “list of formatting elements” before the rightmost marker.
    #[inline]
    fn clear_to_marker(&mut self) {
        if let Some(index) = self.marker_index.take() {
            let index = usize::from(index);
            self.attributes
                .truncate(self.elements[index].attr_index.into());
            let marker = self.elements.drain(index..).next().map(|node| node.node);
            if let Some(TagNode::Marker(marker)) = marker {
                self.marker_index = marker.map(|index| u8::from(index) - 1);
            } else {
                panic!("a marker should always point to the next marker");
            }
        } else {
            self.attributes.clear();
            self.elements.clear();
        }
    }

    /// Returns true if `tag` exists in the “list of active formatting
    /// elements”.
    #[inline]
    fn contains(&self, tag: Tag) -> bool {
        self.elements.iter().any(|node| node.node == tag)
    }

    /// Finds the index of the rightmost item in [`Self::elements`] that matches
    /// the given predicate, ending at the rightmost marker.
    fn index(&self, mut predicate: impl FnMut(&TagNode) -> bool) -> Option<usize> {
        let min = self.after_marker();
        self.elements[min..]
            .iter()
            .rposition(|node| predicate(&node.node))
            .map(|index| min + index)
    }

    /// Iterates over all formatting elements in the given `range`, returning
    /// the tag and the list of attributes.
    fn iter(
        &self,
        range: core::ops::RangeFrom<usize>,
    ) -> impl Iterator<Item = (TagNode, impl Iterator<Item = (&str, &str)>)> {
        self.elements[range].iter().map(|node| {
            let mut attrs = &self.attributes[usize::from(node.attr_index)..];
            let attrs_iter = core::iter::from_fn(move || {
                if attrs.is_empty() || attrs.starts_with(Self::END_OF_ATTRS) {
                    None
                } else {
                    let (name, value) = attrs.split_once(Self::END_OF_NAME).unwrap();
                    let (value, rest) = value.split_once(Self::END_OF_ATTR).unwrap();
                    attrs = rest;
                    Some((name, value))
                }
            });
            (node.node, attrs_iter)
        })
    }

    /// Pushes a new tag to the “list of active formatting elements”, enabling
    /// attribute buffering.
    fn push(&mut self, tag: Tag) {
        // “If there are already three … remove the earliest”
        let min = self.after_marker();
        let mut iter = self.elements[min..]
            .iter()
            .enumerate()
            .filter_map(|(index, node)| (node.node == tag).then_some(index));
        let first = iter.next();
        if iter.count() == 2 {
            self.remove(min + first.unwrap());
        }

        self.elements.push(DomTreeFormattingItem {
            attr_index: self.attributes.len().try_into().unwrap(),
            node: tag.into(),
        });
        self.buffering = true;
    }

    /// Pushes a marker to the “list of active formatting elements”.
    fn push_marker(&mut self) {
        let node = TagNode::Marker(next_index(&mut self.marker_index, self.elements.len()));
        self.elements.push(DomTreeFormattingItem {
            attr_index: self.attributes.len().try_into().unwrap(),
            node,
        });
    }

    /// Removes a formatting item at the given `index`, correcting the marker
    /// pointer chain if needed.
    fn remove(&mut self, index: usize) {
        let old = self.elements.remove(index);
        if let Some(next) = &mut self.marker_index
            && usize::from(*next) > index
        {
            *next -= 1;
            let mut marker = usize::from(*next);
            while let TagNode::Marker(Some(next)) = &mut self.elements[marker].node
                && let old = u8::from(*next)
                && usize::from(old) > index
            {
                let new = old - 1;
                *next = NonZeroU8::new(new).unwrap();
                marker = new.into();
            }
        }
        if index == self.elements.len() {
            self.attributes.truncate(old.attr_index.into());
        }
    }

    /// Finds the position of the element with a tag matching the given
    /// `predicate`.
    #[inline]
    fn rfind(&self, predicate: impl Fn(&TagNode) -> bool) -> Option<usize> {
        self.elements.iter().rposition(|node| predicate(&node.node))
    }

    /// Finishes buffering an attribute.
    #[inline]
    fn tag_attribute_end(&mut self) {
        if self.buffering {
            self.attributes.push(Self::END_OF_ATTR);
        }
    }

    /// Starts buffering an attribute.
    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if self.buffering {
            self.attributes += name;
            self.attributes.push(Self::END_OF_NAME);
        }
    }

    /// Finishes buffering a formatting tag.
    #[inline]
    fn tag_start_end(&mut self) {
        if self.buffering {
            // A terminator is used so that if a formatting element is removed
            // from the middle of the list of formatting elements, it does not
            // require any work to fix up indexes or move the buffer around.
            // This is what we in the biz call premature optimisation.
            self.attributes.push(Self::END_OF_ATTRS);
            self.buffering = false;
        }
    }
}

/// An active formatting element.
#[derive(Clone, Copy, Debug)]
struct DomTreeFormattingItem {
    /// The index into [`DomTreeFormattingList::attributes`].
    attr_index: u16,
    /// The tag.
    node: TagNode,
}

/// A newline filtering state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum DomTreeNewlineState {
    /// Emit newlines like normal.
    #[default]
    Idle,
    /// Filter out the next newline token.
    IgnoreNext,
    /// Oh god, unless there were two of them! Then put it back! Put it back
    /// right now! Undo! Undo!
    JustIgnored,
}

/// Balances the DOM tree using the HTML5 tree construction algorithm(ish).
#[derive(Debug)]
pub(crate) struct DomTree<S: Sink> {
    /// The set of tags not matching any known HTML5 tag.
    custom_tags: IndexSet<Uncased<'static>>,
    /// If true, filtering out an invalid start tag.
    filtering: bool,
    /// The index of the rightmost `<form>` element in [`Self::stack`].
    form_index: Option<u8>,
    /// The “list of active formatting elements”.
    format: DomTreeFormattingList,
    /// If true, currently in an HTML start tag.
    in_attr: bool,
    /// The current parser mode.
    mode: DomMode,
    /// The newline filtering state.
    newline_mode: DomTreeNewlineState,
    /// The output.
    next: DomTreeOutput<S>,
    /// The index of the rightmost `<p>` in [`Self::stack`].
    p_index: Option<u8>,
    /// The stack of currently open nodes.
    stack: Vec<TagNode>,
}

/// A buffering output for HTML5 foster parenting.
#[derive(Debug)]
struct DomTreeOutput<S: Sink> {
    /// The output.
    next: S,
    /// The pending “in table text” text which may be fostered, or not,
    /// depending on whether the *entire run of text* contains only ASCII
    /// whitespace.
    pending_text: String,
    /// The stack of buffering tables.
    tables: Vec<DomTreeTable>,
}

/// A buffering output for an HTML table.
#[derive(Debug, Default)]
struct DomTreeTable {
    /// The buffer for the table and its non-fostered contents.
    buffer: Buffer,
    /// The count of fostered element children. All incoming items will be
    /// fostered until the depth reaches zero, at which point this will be
    /// unlatched.
    foster_depth: Option<u8>,
}

impl<S: Sink> DomTreeOutput<S> {
    /// Creates a new `DomTreeOutput` which emits to `next`.
    #[inline]
    fn new(next: S) -> Self {
        Self {
            next,
            pending_text: <_>::default(),
            tables: <_>::default(),
        }
    }

    /// Decrements the fostered child counter.
    #[inline]
    fn dec_foster(&mut self) {
        if let Some(table) = self.tables.last_mut()
            && let Some(depth) = &mut table.foster_depth
        {
            *depth -= 1;
            if *depth == 0 {
                table.foster_depth = None;
            }
        }
    }

    /// Disables fostering if there is no active content fostering.
    ///
    /// This is needed in situations where fostering would occur if a tag was
    /// actually emitted, but no tag was emitted. Otherwise, the next tag will
    /// end up being treated as fostered content when that was not intended.
    fn disable_fostering(&mut self) {
        if let Some(table) = self.tables.last_mut()
            && table.foster_depth == Some(0)
        {
            table.foster_depth = None;
        }
    }

    /// Enables content fostering. Once enabled, content fostering continues
    /// until the fostered child counter reaches zero.
    #[inline]
    fn enable_fostering(&mut self) {
        if let Some(table) = self.tables.last_mut() {
            table.foster_depth.get_or_insert_default();
        }
    }

    /// Increments the fostered child counter.
    #[inline]
    fn inc_foster(&mut self) {
        if let Some(table) = self.tables.last_mut()
            && let Some(depth) = &mut table.foster_depth
        {
            *depth += 1;
        }
    }

    /// Pops a buffered table and flushes it to the next buffer.
    fn pop_table(&mut self) {
        if let Some(mut table) = self.tables.pop() {
            debug_assert!(self.pending_text.is_empty());
            if let Some(next) = self.tables.last_mut() {
                table.buffer.flush(&mut next.buffer, false);
            } else {
                table.buffer.flush(&mut self.next, false);
            }
        }
    }

    /// Pushes a new table onto the stack of buffering tables.
    #[inline]
    fn push_table(&mut self) {
        self.tables.push(<_>::default());
    }

    /// Flushes pending text and returns the appropriate target sink for a
    /// possibly fostered, possibly buffered item.
    fn target(&mut self) -> &mut dyn Sink {
        if let Some((table, next)) = self.tables.split_last_mut() {
            let before: &mut dyn Sink = next
                .last_mut()
                .map_or(&mut self.next, |next| &mut next.buffer);
            let next = &mut table.buffer;
            if !self.pending_text.is_empty() {
                // Fostered content goes out as a nowiki strip marker to bypass
                // the p-wrapper, which, for whatever reason, was written to
                // ignore fostered content
                let content = StripMarker::NoWiki(self.pending_text.as_str().into());
                if self
                    .pending_text
                    .contains(|c: char| !c.is_ascii_whitespace())
                {
                    before.strip_marker(&content);
                } else {
                    next.strip_marker(&content);
                }
                self.pending_text.clear();
            }
            if table.foster_depth.is_some() {
                before
            } else {
                next
            }
        } else {
            debug_assert!(self.pending_text.is_empty());
            &mut self.next
        }
    }
}

impl<S: Sink> Sink for DomTreeOutput<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.target().comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.target().comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.target().entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        debug_assert!(self.tables.is_empty() && self.pending_text.is_empty());
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.target().new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        self.target().strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.target().tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.target().tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.target().tag_end(name);
        self.dec_foster();
        if name == "table" {
            self.pop_table();
        }
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.inc_foster();
        if name == "table" {
            self.push_table();
        }
        self.target().tag_start(name);
        if VOID_TAGS.contains(name) {
            self.dec_foster();
        }
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.target().tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.target().text(text);
    }
}

/// Emit the tag to the next sink.
const EMIT: bool = true;

/// Discard the tag instead of emitting it.
const SUPPRESS: bool = false;

/// An HTML5 tree construction mode. Modes which are not salient to this
/// fragment parsing implementation are omitted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum DomMode {
    /// The “in body” insertion mode.
    #[default]
    Body,
    /// The “in caption” insertion mode.
    Caption,
    /// The “in cell” insertion mode.
    Cell,
    /// The “in column group” insertion mode.
    ColumnGroup,
    /// The “in row” insertion mode.
    Row,
    /// The “in table” insertion mode.
    Table,
    /// The “in table body” insertion mode.
    TableBody,
}

impl DomMode {
    /// Returns true if text must be fostered out of the table in this mode.
    fn foster_text(self) -> bool {
        matches!(self, Self::Row | Self::Table | Self::TableBody)
    }

    /// Returns true if formatting elements should be reconstructed around text
    /// in this mode.
    fn reformat_text(self) -> bool {
        matches!(self, DomMode::Body | DomMode::Caption | DomMode::Cell)
    }
}

impl<S: Sink + Markable> Chain for DomTree<S> {
    type Next = S;

    fn next(&self) -> &Self::Next {
        &self.next.next
    }

    fn next_mut(&mut self) -> &mut Self::Next {
        &mut self.next.next
    }
}

impl<S: Sink> DomTree<S> {
    /// Creates a new `DomTree` which emits to `next`.
    #[inline]
    pub fn new(next: S) -> Self {
        Self {
            custom_tags: <_>::default(),
            filtering: <_>::default(),
            form_index: <_>::default(),
            format: <_>::default(),
            in_attr: <_>::default(),
            mode: <_>::default(),
            newline_mode: <_>::default(),
            next: DomTreeOutput::new(next),
            p_index: <_>::default(),
            stack: <_>::default(),
        }
    }

    /// Runs the “adoption agency algorithm”, either for a formatting end `tag`,
    /// or for a start `<nobr>`. Numbers are the step numbers from the HTML5 LS
    /// dated 25 June 2026.
    fn adopt(&mut self, tag: Tag) {
        // TODO: This ends up being O(n) but could be O(1) if `self.format` had
        // a counter table.
        // 2.
        if let Some(e) = self
            .stack
            .pop_if(|node| *node == tag && !self.format.contains(tag))
        {
            // The top of the stack was the corresponding start tag, but somehow
            // it is not in the list of active formatting elements? How could
            // this possibly happen?
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
            return;
        }

        // 3..4.2. By wasting a lot of time, the source of these magic loop
        // limits was found: an arbitrary choice to parse ~99.95% successfully
        // in 2010. <https://www.w3.org/Bugs/Public/show_bug.cgi?id=10801>
        //
        // This outer loop is for each of the special-category elements in the
        // stack between `<{{tag}}>` and `</{{tag}}>` whose inner contents must
        // be wrapped by formatting elements, up to 8. The inner loop does the
        // wrapping.
        for _ in 0..8 {
            // 4.3. `format_index` is the index of the corresponding start tag
            // for `tag` in the list of formatting elements.
            let Some(format_index) = self.format.index(|node| *node == tag) else {
                // No corresponding start tag after the last marker means this
                // is either a mismatched end tag which will be suppressed, or
                // the corresponding start tag is in a different formatting
                // scope. (It is, once again, unclear how this is possible.)
                self.tag_end_default(tag);
                return;
            };

            // 4.5. `stack_index` is the index of the corresponding start tag
            // for `tag` in the list of open elements.
            // This scope checked scan goes first because it does less traversal
            // than step 4.4 does.
            let Some(stack_index) = self.index_in_scope(|node| *node == tag, Tag::is_general_scope)
            else {
                // 4.4.
                if !self.stack.contains(&tag.into()) {
                    // There was no corresponding start tag in *any* scope, so
                    // the formatting tag must have been implicitly closed and
                    // the spec says that means it goes to the soylent factory,
                    // rip.
                    self.format.remove(format_index);
                }

                // If there is no corresponding start tag in scope, then
                // there is nothing to do right now with this mismatched end tag
                return;
            };

            // 4.7: `furthest` is the index of the special-category element
            // closest to the start tag (“furthest” from `</{{tag}}>`) that sits
            // in between the start and end tags. (The spec probably calls this
            // “furthestBlock” because “special category” is the closest
            // descendant of “block-level element”, but who knows, I have spent
            // too many hours of my life on stupid archaeology about this.)
            let Some(furthest) = self.stack[stack_index + 1..]
                .iter()
                .position(|node| node.is_special())
            else {
                // 4.8: There wasn’t any “furthest”, so all the elements are
                // getting closed now, and any formatting elements after this
                // one will be reopened by `reformat` later. (This maybe makes a
                // little more sense when one thinks back to how HTML spec used
                // to define block-level and inline-level elements, and these
                // would have basically been all of the inline ones?)
                for e in self.stack.drain(stack_index..).rev() {
                    e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
                }
                self.format.remove(format_index);
                return;
            };

            // The spec source has an unhelpfully commented out diagram showing
            // the visual layout of the algorithm. Here is an adaptation of that
            // diagram:
            // <common> inner:(<{{tag}}> ...)*<1,3> ... outer:(<furthest> ...)*<0,7> ... destroyer_of_worlds:</{{tag}}>

            // 4.9.
            // In the spec language, there would always be a common element
            // because there would always be a root `<html>`. Since this is an
            // insertion point for inserting the adoptee `</{{tag}}>` after
            // `<{{tag}}>`, it can just point to `<{{tag}}>`.
            let common = stack_index;

            // 4.10.
            let mut bookmark = format_index;

            // 4.11.
            let mut node_index = furthest;
            let mut last_node_index = furthest;

            // 4.12..4.13. This inner loop is for each of the duplicate
            // formatting elements that will wrap the inner contents of the
            // outer loop’s element, up to 3.
            //
            // The order of the wrappers is defined by the order in which the
            // `</{{tag}}>` elements appeared.
            for inner in 1.. {
                if node_index == 0 {
                    break;
                }

                // 4.13.2.
                node_index -= 1;
                let node = self.stack[node_index];

                // 4.13.3.
                if node == tag {
                    break;
                }

                // 4.13.4.
                let mut format_index = self.format.rfind(|fmt_node| *fmt_node == node);

                if inner > 3
                    && let Some(index) = format_index.take()
                {
                    self.format.remove(index);
                }

                if let Some(index) = format_index {
                    // 4.13.6. “[clone] `node` … [and] replace the entry … in
                    // [formatting and the stack with the clone] … and let
                    // `node` be the [clone]”

                    // 4.13.7. “move the bookmark … to be immediately after the
                    // new node”. This only happens once because
                    // `last_node_index` stops being `furthest` after the first
                    // iteration. WHY IS THIS WRITTEN THIS WAY?
                    if last_node_index == furthest {
                        bookmark = index + 1;
                    }
                } else {
                    // 4.13.5.
                    let e = self.stack.remove(node_index);
                    e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
                    continue;
                }

                // 4.13.8. “append `lastNode` to `node`”, which means nothing
                // for the first loop, only for the subsequent loops does this
                // cause a change? WHY IS THIS ALGORITHM SO BAD?
                // <b><a></b><c> <d>
                //           ^^^ ^^^ lastNode
                //          node
                log::warn!(concat!(
                    "TODO: Go back in time and insert tags such that",
                    " `last_node` is inside `node`"
                ));

                // 4.13.9.
                last_node_index = node_index;
            }

            // 4.14. “insert `last_node` … at `common`”. Because `last_node` is
            // the node after the corresponding formatting start tag, this is
            // equivalent to doing something less insane TODO

            // Normally this would do content fostering if the mode had been
            // “in table” at the time of the insertion, but that is handled
            // separately, so this just does the normal insert
            // self.stack.insert(common, tag.into());

            // 4.15..4.17. These steps are all just injecting a single
            // formatting start tag in between the `max` and the children of
            // `max`
            // self.stack.insert(max, tag.into());

            // 4.18. This just shifts the position of the formatting element in
            // the stack to reflect its newly reparented position?
            // if bookmark != format_index {
            //     self.format[format_index..bookmark].rotate_left(1);
            // }

            // 4.19.
        }
    }

    /// Closes the nearest table cell element.
    fn close_cell(&mut self) {
        // The spec pops all implied end tags first to track errors, but this
        // implementation does not need to track errors
        self.pop_inclusive(|node| matches!(node.tag(), Some(Tag::Td | Tag::Th)));
        self.format.clear_to_marker();
        self.mode = DomMode::Row;
    }

    /// Closes the nearest `<p>` element “in button scope”, if one exists.
    #[inline]
    fn close_p(&mut self) {
        // The spec pops all implied end tags first to track errors, but
        // this implementation does not need to track errors
        self.pop_in_scope(|node| *node == Tag::P, Tag::is_button_scope);
    }

    /// Performs special fixups for nested `<a>` tags.
    fn fixup_anchor(&mut self, tag: Tag) {
        if self.format.index(|node| *node == tag).is_some() {
            self.adopt(tag);
            // “remove that element from the list of active formatting elements
            // and the stack of open elements if the adoption agency algorithm
            // didn’t already remove it (it might not have if the element is not
            // in table scope)” suggests some ability to identify the same
            // element in both stacks by identity after a mutation, which is not
            // possible here. the spec suggests in §13.3 that anchors are
            // allowed to nest in the case of fostering, until they are
            // serialised, then they are not. Since this is a serialiser it
            // should be the case that these things never nest.
            if let Some(index) = self.format.rfind(|node| *node == tag) {
                self.format.remove(index);
            }
            self.pop_in_scope(|node| *node == tag, |_| false);
        }
    }

    /// Pop all elements on the stack with implied end tags except for `except`.
    fn implied_end(&mut self, except: Option<Tag>) {
        while let Some(e) = self.stack.pop_if(|node| node.is_implied_close(except)) {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
    }

    /// Returns the index of an element matching the given `predicate` on the
    /// stack of open elements in the scope given by `scope`, or `None` if there
    /// is no such element.
    #[inline]
    fn in_scope(
        &self,
        predicate: impl FnMut(&TagNode) -> bool,
        scope: impl FnMut(Tag) -> bool,
    ) -> bool {
        self.index_in_scope(predicate, scope).is_some()
    }

    /// Returns true if the tree is “in table text”.
    #[inline]
    fn in_table_text(&self) -> bool {
        self.mode.foster_text()
            && self
                .stack
                .last()
                .is_some_and(|node| node.is_table_fosterable())
    }

    /// Returns the index of an element matching the given `predicate` on the
    /// stack of open elements in the scope given by `scope`, or `None` if there
    /// is no such element.
    fn index_in_scope(
        &self,
        mut predicate: impl FnMut(&TagNode) -> bool,
        mut scope: impl FnMut(Tag) -> bool,
    ) -> Option<usize> {
        for (index, node) in self.stack.iter().enumerate().rev() {
            #[rustfmt::skip]
            if predicate(node) {
                return Some(index);
            } else if let Some(tag) = node.tag() && scope(tag) {
                break;
            };
        }
        None
    }

    /// Closes all elements up to `predicate`.
    fn pop_exclusive(&mut self, mut predicate: impl FnMut(&mut TagNode) -> bool) {
        while let Some(e) = self.stack.pop_if(|node| !predicate(node)) {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
    }

    /// Closes all elements up to and including `predicate` if a match exists in
    /// the scope given by `scope`, returning `true` if elements were closed.
    fn pop_in_scope(
        &mut self,
        predicate: impl FnMut(&TagNode) -> bool,
        scope: impl FnMut(Tag) -> bool,
    ) -> bool {
        if let Some(index) = self.index_in_scope(predicate, scope) {
            for e in self.stack.drain(index..).rev() {
                e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
            }
            true
        } else {
            false
        }
    }

    /// Closes all elements up to and including `predicate`.
    fn pop_inclusive(&mut self, predicate: impl FnMut(&mut TagNode) -> bool) {
        self.pop_exclusive(predicate);
        if let Some(e) = self.stack.pop() {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
    }

    /// Closes the element at the end of the stack if it matches `predicate`,
    /// returning `true` if the element was closed.
    fn pop_one(&mut self, predicate: impl FnOnce(&mut TagNode) -> bool) -> bool {
        if let Some(e) = self.stack.pop_if(predicate) {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
            true
        } else {
            false
        }
    }

    /// Pushes an indexed `<p>` tag to the stack. This is an optimisation.
    fn push_p(&mut self) {
        let node = TagNode::P(next_index(&mut self.p_index, self.stack.len()));
        self.stack.push(node);
    }

    /// Reopens any formatting elements which were closed due to element
    /// splitting.
    fn reformat(&mut self) {
        // TODO: This is O(n^2), but could be made O(n) by having a tag count
        // table for the stack.
        let Some(first_missing) = self.format.index(|node| !self.stack.contains(node)) else {
            return;
        };

        for (tag, attrs) in self.format.iter(first_missing..) {
            let tag_name = tag.name(&self.custom_tags).expect("named tag");
            self.next.tag_start(tag_name.as_str());
            for (name, value) in attrs {
                self.next.tag_attribute_full(name, value);
            }
            self.next.tag_start_end(tag_name.as_str());
            self.stack.push(tag);
        }
    }

    /// Slowly recalculates the current insertion mode according to what
    /// elements are on the stack.
    fn reset_mode(&mut self) {
        let mode = self
            .stack
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, node)| match node.tag() {
                Some(Tag::Td | Tag::Th) if index != 0 => Some(DomMode::Cell),
                Some(Tag::Tr) => Some(DomMode::Row),
                Some(Tag::Tbody | Tag::Tfoot | Tag::Thead) => Some(DomMode::TableBody),
                Some(Tag::Caption) => Some(DomMode::Caption),
                Some(Tag::Table) => Some(DomMode::Table),
                _ => None,
            });

        self.mode = mode.unwrap_or(DomMode::Body);
    }

    /// Inserts a new end `tag` in the “in body” insertion mode.
    fn tag_end_body(&mut self, tag: Tag) {
        match tag {
            Tag::Address
            | Tag::Aside
            | Tag::Blockquote
            | Tag::Button
            | Tag::Center
            | Tag::Details
            | Tag::Div
            | Tag::Dl
            | Tag::Figcaption
            | Tag::Figure
            | Tag::Form
            | Tag::Ol
            | Tag::Pre
            | Tag::Select
            | Tag::Summary
            | Tag::Ul => {
                if tag == Tag::Form {
                    self.form_index = None;
                }
                if self.stack.iter().rfind(|node| **node == tag).is_some() {
                    self.implied_end(None);
                    // The spec pops all implied end tags first to track errors,
                    // but this implementation does not need to track errors
                    self.pop_inclusive(|node| *node == tag);
                }
            }
            Tag::Br => {
                if self.tag_start_body(tag) {
                    self.next.tag_start_full("br");
                }
            }
            Tag::Dd | Tag::Dt => {
                // The spec pops implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.pop_in_scope(|node| *node == tag, Tag::is_general_scope);
            }
            tag if tag.is_heading() => {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.pop_in_scope(|node| node.is_heading(), Tag::is_general_scope);
            }
            Tag::Li => {
                // The spec pops implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.pop_in_scope(|node| *node == tag, Tag::is_list_item_scope);
            }
            Tag::Object => {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                if self.pop_in_scope(|node| *node == tag, Tag::is_general_scope) {
                    self.format.clear_to_marker();
                }
            }
            Tag::P => {
                if !self.in_scope(|node| *node == tag, Tag::is_button_scope) {
                    self.tag_start_full("p");
                }
                self.close_p();
            }
            tag if tag.is_formatting() => {
                self.adopt(tag);
            }
            tag => self.tag_end_default(tag),
        }
    }

    /// Inserts a new end `tag` in the “in caption” insertion mode.
    fn tag_end_caption(&mut self, tag: Tag) {
        if matches!(tag, Tag::Caption | Tag::Table) {
            if self.pop_in_scope(|node| *node == Tag::Caption, Tag::is_table_scope) {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.format.clear_to_marker();
                self.mode = DomMode::Table;
                if tag == Tag::Table {
                    self.tag_end_table(tag);
                }
            }
        } else if !tag.is_table_item() {
            self.tag_end_body(tag);
        }
    }

    /// Inserts a new end `tag` in the “in cell” insertion mode.
    fn tag_end_cell(&mut self, tag: Tag) {
        if matches!(tag, Tag::Td | Tag::Th) {
            if self.pop_in_scope(|node| *node == tag, Tag::is_table_scope) {
                // The spec pops all implied end tags first to track errors,
                // but this implementation does not need to track errors
                self.format.clear_to_marker();
                self.mode = DomMode::Row;
            }
        } else if tag.is_table_fosterable()
            && self.in_scope(|node| *node == tag, Tag::is_table_scope)
        {
            self.close_cell();
            self.tag_end_row(tag);
        } else if !tag.is_table_item() {
            self.tag_end_body(tag);
        }
    }

    /// Inserts a new end `tag` in the “in column group” insertion mode.
    fn tag_end_colgroup(&mut self, tag: Tag) {
        if tag != Tag::Col && self.pop_one(|node| *node == Tag::Colgroup) {
            self.mode = DomMode::Table;
            if tag != Tag::Colgroup {
                self.tag_end_table(tag);
            }
        }
    }

    /// The fallback implementation for inserting a new end `tag`.
    fn tag_end_default(&mut self, tag: Tag) {
        // The spec pops implied end tags first to track errors, but this
        // implementation does not need to track errors
        self.pop_in_scope(|node| *node == tag, Tag::is_special);
    }

    /// Inserts a new end `tag` in the “in row” insertion mode.
    fn tag_end_row(&mut self, tag: Tag) {
        if tag.is_table_fosterable() {
            if tag.is_table_body() && !self.in_scope(|node| *node == tag, Tag::is_table_scope) {
            } else if self.pop_in_scope(|node| *node == Tag::Tr, Tag::is_table_scope) {
                self.mode = DomMode::TableBody;
                if tag != Tag::Tr {
                    self.tag_end_table_body(tag);
                }
            }
        } else if !tag.is_table_item() {
            self.tag_end_table(tag);
        }
    }

    /// Inserts a new end `tag` in the “in table” insertion mode.
    fn tag_end_table(&mut self, tag: Tag) {
        if tag == Tag::Table {
            if self.pop_in_scope(|node| *node == Tag::Table, Tag::is_table_scope) {
                self.reset_mode();
            }
        } else if !tag.is_table_item() {
            // eprintln!("enable for close {tag:?} {:?}", self.stack);
            self.next.enable_fostering();
            self.tag_end_body(tag);
            self.next.disable_fostering();
        }
    }

    /// Inserts a new end `tag` in the “in table body” insertion mode.
    fn tag_end_table_body(&mut self, tag: Tag) {
        if tag == Tag::Table || tag.is_table_body() {
            if self.pop_in_scope(|node| *node == tag, Tag::is_table_scope) {
                self.mode = DomMode::Table;
                if tag == Tag::Table {
                    self.reset_mode();
                }
            }
        } else if !tag.is_table_item() {
            self.tag_end_table(tag);
        }
    }

    /// Inserts a new start `tag` in the mode defined by [`Self::mode`].
    fn tag_start_any(&mut self, tag: Tag) -> bool {
        match self.mode {
            DomMode::Body => self.tag_start_body(tag),
            DomMode::Table => self.tag_start_table(tag),
            DomMode::Caption => self.tag_start_caption(tag),
            DomMode::ColumnGroup => self.tag_start_colgroup(tag),
            DomMode::TableBody => self.tag_start_table_body(tag),
            DomMode::Row => self.tag_start_row(tag),
            DomMode::Cell => self.tag_start_cell(tag),
        }
    }

    /// Inserts a new start `tag` in the “in body” insertion mode.
    #[expect(clippy::too_many_lines, reason = "complaints go to WHATWG")]
    fn tag_start_body(&mut self, tag: Tag) -> bool {
        match tag {
            tag if tag.is_head_item() => self.tag_start_head(tag),

            Tag::Br | Tag::Img | Tag::Wbr => {
                self.reformat();
                EMIT
            }
            Tag::Button => {
                // The spec pops implied end tags first, but this does not
                // seem to make sense since these would all be closed anyway
                // on the way to the button element and it is already a
                // parse error
                self.pop_in_scope(|node| *node == tag, Tag::is_general_scope);
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Dd | Tag::Dt | Tag::Li => {
                // The spec pops implied end tags first to track errors,
                // but this implementation does not need to track errors
                if tag == Tag::Li {
                    self.pop_in_scope(|node| *node == tag, Tag::is_list_special);
                } else {
                    self.pop_in_scope(|node| node.is_dl_item(), Tag::is_list_special);
                }
                self.close_p();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Form => {
                if self.form_index.is_none() {
                    self.form_index = Some(self.stack.len().try_into().unwrap());
                    self.close_p();
                    self.stack.push(tag.into());
                    EMIT
                } else {
                    SUPPRESS
                }
            }
            tag if tag.is_heading() => {
                self.close_p();
                self.pop_one(|node| node.is_heading());
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Hr => {
                self.close_p();
                // For `<hr>` in `<select>`
                if self.in_scope(|node| *node == Tag::Select, Tag::is_general_scope) {
                    self.implied_end(None);
                }
                EMIT
            }
            Tag::Iframe => {
                // For `<iframe>` the spec says to switch to “generic raw text
                // parsing algorithm” but this is not a tokeniser
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Input => {
                self.pop_in_scope(|node| *node == Tag::Select, Tag::is_general_scope);
                self.reformat();
                EMIT
            }
            Tag::Object => {
                self.reformat();
                self.format.push_marker();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Option | Tag::Optgroup => {
                if self.in_scope(|node| *node == Tag::Select, Tag::is_general_scope) {
                    let except = (tag == Tag::Option).then_some(Tag::Optgroup);
                    self.implied_end(except);
                } else {
                    self.pop_one(|node| *node == Tag::Option);
                }
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Pre | Tag::Textarea => {
                // For `<textarea>` the spec says to switch to RCDATA but this
                // is not a tokeniser
                if tag == Tag::Pre {
                    self.close_p();
                }
                self.newline_mode = DomTreeNewlineState::IgnoreNext;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Select if self.pop_in_scope(|node| *node == tag, Tag::is_general_scope) => {
                SUPPRESS
            }
            Tag::Select => {
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Table => {
                self.close_p();
                self.mode = DomMode::Table;
                self.stack.push(tag.into());
                EMIT
            }
            tag if tag.is_body_block() => {
                self.close_p();
                if tag == Tag::P {
                    self.push_p();
                } else {
                    self.stack.push(tag.into());
                }
                EMIT
            }
            tag if tag.is_formatting() => {
                if tag == Tag::A {
                    self.fixup_anchor(tag);
                } else if tag == Tag::Nobr
                    && self.in_scope(|node| *node == tag, Tag::is_general_scope)
                {
                    self.reformat();
                    self.adopt(tag);
                }
                self.reformat();
                self.format.push(tag);
                self.stack.push(tag.into());
                EMIT
            }
            tag if tag.is_ruby_item() => {
                if self.in_scope(|node| *node == Tag::Ruby, Tag::is_general_scope) {
                    let except = matches!(tag, Tag::Rp | Tag::Rt).then_some(Tag::Rtc);
                    self.implied_end(except);
                }
                self.stack.push(tag.into());
                EMIT
            }
            tag if tag.is_table_item() => SUPPRESS,
            _ => {
                self.reformat();
                self.stack.push(tag.into());
                EMIT
            }
        }
    }

    /// Inserts a new start `tag` in the “in caption” insertion mode.
    fn tag_start_caption(&mut self, tag: Tag) -> bool {
        if tag.is_table_item() {
            if self.pop_in_scope(|node| *node == Tag::Caption, Tag::is_table_scope) {
                self.format.clear_to_marker();
                self.mode = DomMode::Table;
                self.tag_start_table(tag)
            } else {
                SUPPRESS
            }
        } else {
            self.tag_start_body(tag)
        }
    }

    /// Inserts a new start `tag` in the “in cell” insertion mode.
    fn tag_start_cell(&mut self, tag: Tag) -> bool {
        if tag.is_table_item() {
            self.close_cell();
            self.tag_start_row(tag)
        } else {
            self.tag_start_body(tag)
        }
    }

    /// Inserts a new start `tag` in the “in column group” insertion mode.
    fn tag_start_colgroup(&mut self, tag: Tag) -> bool {
        if tag == Tag::Col {
            EMIT
        } else if self.pop_one(|node| *node == Tag::Colgroup) {
            self.mode = DomMode::Table;
            self.tag_start_table(tag)
        } else {
            SUPPRESS
        }
    }

    /// Inserts a new start `tag` in the “in head” insertion mode.
    fn tag_start_head(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Basefont | Tag::Link | Tag::Meta => EMIT,
            Tag::Title => {
                // This is supposed to use the RCDATA element parsing algorithm,
                // but since the tokeniser has already done its thing, just
                // treat it like a normal whatever
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Style => {
                // This is supposed to use the generic raw text element parsing
                // algorithm, but since the tokeniser has already done its
                // thing, just treat it like a normal whatever
                self.stack.push(tag.into());
                EMIT
            }
            _ => panic!("should never get here"),
        }
    }

    /// Inserts a new start `tag` in the “in row” insertion mode.
    fn tag_start_row(&mut self, tag: Tag) -> bool {
        if matches!(tag, Tag::Td | Tag::Th) {
            self.pop_exclusive(|node| *node == Tag::Tr);
            self.mode = DomMode::Cell;
            self.format.push_marker();
            self.stack.push(tag.into());
            EMIT
        } else if tag.is_table_item() {
            if self.pop_in_scope(|node| *node == Tag::Tr, Tag::is_table_scope) {
                self.mode = DomMode::TableBody;
                self.tag_start_table_body(tag)
            } else {
                SUPPRESS
            }
        } else {
            self.tag_start_table(tag)
        }
    }

    /// Inserts a new start `tag` in the “in table” insertion mode.
    fn tag_start_table(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Caption => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = DomMode::Caption;
                self.format.push_marker();
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Colgroup => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = DomMode::ColumnGroup;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Col => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.next.tag_start_full("colgroup");
                self.stack.push(Tag::Colgroup.into());
                self.mode = DomMode::ColumnGroup;
                self.tag_start_colgroup(tag)
            }
            Tag::Tbody | Tag::Tfoot | Tag::Thead => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = DomMode::TableBody;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Td | Tag::Th | Tag::Tr => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.stack.push(TagNode::ImplicitTbody);
                self.mode = DomMode::TableBody;
                self.tag_start_table_body(tag)
            }
            Tag::Table => {
                if self.pop_in_scope(|node| *node == Tag::Table, Tag::is_table_scope) {
                    self.reset_mode();
                    self.tag_start_any(tag)
                } else {
                    SUPPRESS
                }
            }
            Tag::Style => self.tag_start_head(tag),
            Tag::Input => {
                // The spec says that hidden inputs are not supposed to be
                // fostered but this is a needless complexity which would
                // require tracking attributes since this is not supported in
                // normal Wikitext anyway
                self.next.enable_fostering();
                self.tag_start_body(tag)
            }
            Tag::Form => {
                // The spec says that form in a table is supposed to cause
                // the form pointer to be set, but then to not emit anything
                // to the output. For a serialiser, this just means to not
                // emit anything
                SUPPRESS
            }
            tag => {
                // eprintln!("enable for {tag:?} {:?}", self.stack);
                self.next.enable_fostering();
                self.tag_start_body(tag)
            }
        }
    }

    /// Inserts a new start `tag` in the “in table body” insertion mode.
    fn tag_start_table_body(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Tr => {
                self.pop_exclusive(|node| node.is_table_body());
                self.mode = DomMode::Row;
                self.stack.push(tag.into());
                EMIT
            }
            Tag::Td | Tag::Th => {
                self.pop_exclusive(|node| node.is_table_body());
                self.mode = DomMode::Row;
                self.next.tag_start_full("tr");
                self.stack.push(Tag::Tr.into());
                self.tag_start_row(tag)
            }
            tag if tag.is_table_item() => {
                if self.in_scope(|node| node.is_table_body(), Tag::is_table_scope) {
                    self.pop_exclusive(|node| *node == Tag::Table);
                    self.mode = DomMode::Table;
                    self.tag_start_table(tag)
                } else {
                    SUPPRESS
                }
            }
            _ => self.tag_start_table(tag),
        }
    }
}

impl<S: Sink> Sink for DomTree<S> {
    #[inline]
    fn comment_end(&mut self) {
        if self.filtering {
            return;
        }

        self.next.comment_end();
        if !self.in_attr {
            self.newline_mode = <_>::default();
        }
    }

    #[inline]
    fn comment_start(&mut self) {
        if self.filtering {
            return;
        }

        self.next.comment_start();
        if !self.in_attr {
            self.newline_mode = <_>::default();
        }
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.format.buffer_char(value);
            self.next.entity(value, raw);
        } else {
            if self.mode.reformat_text() {
                self.reformat();
            }
            if self.in_table_text() {
                self.next.pending_text.push(value);
            } else {
                self.next.entity(value, raw);
            }
            self.newline_mode = <_>::default();
        }
    }

    #[inline]
    fn finish(mut self) -> String {
        for e in self.stack.drain(..).rev() {
            e.close(&mut self.next, &self.custom_tags, &mut self.p_index);
        }
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.next.new_line();
        } else if self.newline_mode == DomTreeNewlineState::IgnoreNext {
            self.newline_mode = DomTreeNewlineState::JustIgnored;
        } else if self.newline_mode == DomTreeNewlineState::JustIgnored {
            if self.in_table_text() {
                self.next.pending_text += "\n\n";
            } else {
                self.next.new_line();
                self.next.new_line();
            }
            self.newline_mode = <_>::default();
        } else {
            if self.mode.reformat_text() {
                self.reformat();
            }
            if self.in_table_text() {
                self.next.pending_text.push('\n');
            } else {
                self.next.new_line();
            }
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if self.filtering {
            return;
        }

        self.next.strip_marker(marker);
        if !self.in_attr {
            self.newline_mode = <_>::default();
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if !self.filtering {
            self.format.tag_attribute_end();
            self.next.tag_attribute_end(name);
        }
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if !self.filtering {
            self.format.tag_attribute_start(name);
            self.next.tag_attribute_start(name);
        }
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.newline_mode = <_>::default();

        let tag = Tag::new(name, &mut self.custom_tags);

        match self.mode {
            DomMode::Body => self.tag_end_body(tag),
            DomMode::Table => self.tag_end_table(tag),
            DomMode::Caption => self.tag_end_caption(tag),
            DomMode::ColumnGroup => self.tag_end_colgroup(tag),
            DomMode::TableBody => self.tag_end_table_body(tag),
            DomMode::Row => self.tag_end_row(tag),
            DomMode::Cell => self.tag_end_cell(tag),
        }
    }

    #[inline]
    fn tag_start(&mut self, mut name: &str) {
        self.newline_mode = <_>::default();

        if name.eq_ignore_ascii_case("image") {
            name = "img";
        }

        let tag = Tag::new(name, &mut self.custom_tags);
        if self.tag_start_any(tag) {
            self.in_attr = true;
            self.next.tag_start(name);
        } else {
            self.filtering = true;
        }
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        if self.filtering {
            self.filtering = false;
        } else {
            self.in_attr = false;
            self.format.tag_start_end();
            self.next.tag_start_end(name);
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.format.buffer_text(text);
            self.next.text(text);
        } else {
            if self.mode.reformat_text() {
                self.reformat();
            }
            if self.in_table_text() {
                self.next.pending_text += text;
            } else {
                self.next.text(text);
            }
            self.newline_mode = <_>::default();
        }
    }
}

/// Takes a value from `index`, returning a niche-optimised `Option<NonZeroU8>`.
fn next_index(index: &mut Option<u8>, next: usize) -> Option<NonZeroU8> {
    index
        .replace(next.try_into().unwrap())
        .and_then(|n| NonZeroU8::new(n + 1))
}

/// Wraps bare text content in the root and in `<blockquote>` elements with a
/// `<p>`.
#[derive(Debug)]
pub(crate) struct PWrapper<S: Sink> {
    /// The current DOM depth.
    depth: u8,
    /// If true, currently in an HTML start tag.
    in_attr: bool,
    /// The output.
    next: S,
    /// P-wrapper root depths.
    roots: Vec<PWrapperRoot>,
}

/// A p-wrapper root.
#[derive(Debug, Default)]
struct PWrapperRoot {
    /// The candidate start position for a p-wrapper.
    candidate: Option<Mark>,
    /// The depth of the wrapper root.
    depth: u8,
    /// Whether this candidate wrapper has some non-ASCII-whitespace content.
    has_content: bool,
}

chainable!(PWrapper);

impl<S: Sink + Markable> PWrapper<S> {
    /// Creates a new `PWrapper` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            depth: <_>::default(),
            in_attr: <_>::default(),
            next,
            roots: vec![<_>::default()],
        }
    }

    /// Enters a graf wrapper if it is appropriate at the current DOM position.
    fn enter_p(&mut self, has_content: bool) {
        if self.in_attr {
            return;
        }

        let root = self.roots.last_mut().unwrap();
        if root.depth == self.depth && root.candidate.is_none() {
            root.candidate = Some(self.next.mark());
            root.has_content = false;
        }
        root.has_content |= has_content;
    }

    /// Exits a graf wrapper.
    fn exit_p(&mut self) -> bool {
        let root = self.roots.last_mut().unwrap();
        if root.depth == self.depth {
            if let Some(candidate) = root.candidate.take() {
                if root.has_content {
                    self.next.with_marks([&candidate], |[candidate], out| {
                        if let Some(candidate) = candidate {
                            out.insert_str(candidate, "<p>");
                            out.push_str("</p>");
                        }
                    });
                }
                self.next.free_mark(candidate);
            }
            true
        } else {
            if let Some(candidate) = root.candidate.take() {
                self.next.free_mark(candidate);
            }
            false
        }
    }
}

impl<S: Sink + Markable> Sink for PWrapper<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.enter_p(true);
        self.next.entity(value, raw);
    }

    #[inline]
    fn finish(mut self) -> String {
        debug_assert_eq!(self.depth, 0);
        self.exit_p();
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.enter_p(false);
        self.next.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        self.next.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.next.tag_attribute_start(name);
    }

    fn tag_end(&mut self, name: &str) {
        if !Tag::known(name).is_some_and(Tag::is_inline) && self.exit_p() {
            self.roots.pop();
        }
        self.next.tag_end(name);
        self.depth -= 1;
    }

    fn tag_start(&mut self, name: &str) {
        if Tag::known(name).is_some_and(Tag::is_inline) {
            self.enter_p(true);
        } else {
            self.exit_p();
        }
        self.next.tag_start(name);
        if !VOID_TAGS.contains(name) {
            self.depth += 1;
        }
        if name == "blockquote" {
            self.roots.push(PWrapperRoot {
                depth: self.depth,
                ..<_>::default()
            });
        }
        self.in_attr = true;
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.in_attr = false;
        self.next.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.enter_p(text.contains(|c: char| !c.is_ascii_whitespace()));
        self.next.text(text);
    }
}

/// A DOM pseudo-node.
#[derive(Clone, Copy, Debug, Eq)]
enum TagNode {
    /// An HTML tag.
    Html(Tag),
    /// An implicit `<tbody>` that is not being emitted because it is a waste.
    ImplicitTbody,
    /// A marker on the “list of active formatting elements”.
    Marker(Option<NonZeroU8>),
    /// An optimised `<p>` element that holds a niche-optimised index of the
    /// previous `<p>` element in [`DomTree::stack`], if any.
    P(Option<NonZeroU8>),
}

impl From<Tag> for TagNode {
    #[inline]
    fn from(tag: Tag) -> Self {
        Self::Html(tag)
    }
}

impl PartialEq<Tag> for TagNode {
    #[inline]
    fn eq(&self, other: &Tag) -> bool {
        self.tag() == Some(*other)
    }
}

impl TagNode {
    /// Emits the close tag for this node to `next`, using the given set of
    /// `custom` tag names, and updating the `next_index` if applicable.
    fn close<S: Sink>(
        self,
        next: &mut DomTreeOutput<S>,
        custom: &IndexSet<Uncased<'static>>,
        next_index: &mut Option<u8>,
    ) {
        if let Some(name) = self.name(custom) {
            // Because the HTML5 spec is designed to construct a tree, fostering
            // of a start tag “in table” mode would cause the whole tag to be
            // moved out, including its children and any implicit end tag, so in
            // order to close an element correctly it’s necessary to know if it
            // is in a position where the start tag was also fostered out.

            debug_assert!(!VOID_TAGS.contains(name.as_str()));
            next.tag_end(name.as_str());
            if let Self::P(next) | Self::Marker(next) = self {
                *next_index = next.map(|index| u8::from(index) - 1);
            }
        }
    }

    /// Returns the tag name of this node, or `None` if this is an anonymous
    /// marker node.
    fn name<'a>(self, custom: &'a IndexSet<Uncased<'static>>) -> Option<&'a UncasedStr> {
        match self {
            Self::Html(tag) => Some(tag.as_str(custom)),
            Self::ImplicitTbody | Self::Marker(_) => None,
            Self::P(_) => Some(UncasedStr::new("p")),
        }
    }

    /// Returns true if this is a definition list child.
    #[inline]
    fn is_dl_item(self) -> bool {
        self.tag().is_some_and(Tag::is_dl_item)
    }

    /// Returns true if this is an HTML heading node.
    #[inline]
    fn is_heading(self) -> bool {
        self.tag().is_some_and(Tag::is_heading)
    }

    /// Returns true if this node has an implied end tag, `except` not that one.
    #[inline]
    fn is_implied_close(self, except: Option<Tag>) -> bool {
        self.tag()
            .is_some_and(|tag| tag.is_implied_end() && Some(tag) != except)
    }

    /// Returns true if this is a “special category” node.
    #[inline]
    fn is_special(self) -> bool {
        self.tag().is_some_and(Tag::is_special)
    }

    /// Returns true if this is a `<table>` direct child.
    #[inline]
    fn is_table_body(self) -> bool {
        matches!(self, Self::ImplicitTbody) || self.tag().is_some_and(Tag::is_table_body)
    }

    /// Returns true if this is a `<table>` element that cannot contain most
    /// non-table content.
    #[inline]
    fn is_table_fosterable(self) -> bool {
        matches!(self, Self::ImplicitTbody) || self.tag().is_some_and(Tag::is_table_fosterable)
    }

    /// Returns the corresponding HTML5 tag for this node, or `None` if this is
    /// an anonymous marker node.
    fn tag(self) -> Option<Tag> {
        match self {
            Self::Html(tag) => Some(tag),
            Self::ImplicitTbody => Some(Tag::Tbody),
            Self::Marker(_) => None,
            Self::P(_) => Some(Tag::P),
        }
    }
}

impl PartialEq for TagNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Html(lhs), Self::Html(rhs)) => lhs == rhs,
            (Self::Marker(_), Self::Marker(_)) | (Self::P(_), Self::P(_)) => true,
            (Self::Html(tag), Self::P(_)) | (Self::P(_), Self::Html(tag)) => *tag == Tag::P,
            (Self::Html(tag), Self::ImplicitTbody) | (Self::ImplicitTbody, Self::Html(tag)) => {
                *tag == Tag::Tbody
            }
            _ => false,
        }
    }
}

/// Generates the `Tag` enum and lookup table for known HTML5 tag names.
macro_rules! tags {
    ($($tag:literal => $id:ident),* $(,)?) => {
        /// An HTML tag.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Tag {
            $($id,)*
            /// A custom tag index.
            Custom(u8),
        }

        /// The lookup table for known HTML tags.
        static KNOWN_TAGS: phf::Map<&UncasedStr, Tag> = phf::phf_map! {
            $(UncasedStr::new($tag) => Tag::$id,)*
        };

        impl Tag {
            /// Returns the tag as a string.
            fn as_str<'a>(self, custom: &'a IndexSet<Uncased<'static>>) -> &'a UncasedStr {
                match self {
                    $(Self::$id => UncasedStr::new($tag),)*
                    Self::Custom(index) => custom.get_index(index.into()).unwrap(),
                }
            }
        }
    }
}

// The list of tags used here is the list of allowed Wikitext tags, plus tags
// that are special in the HTML5 tree construction algorithm and are emitted by
// extension tags
tags! {
    "a" => A, "abbr" => Abbr, "acronym" => Acronym, "address" => Address, "annotation-xml" => AnnotationXml, "aside" => Aside, "audio" => Audio,
    "b" => B, "basefont" => Basefont, "bdi" => Bdi, "bdo" => Bdo, "big" => Big, "button" => Button,
    "blockquote" => Blockquote, "br" => Br,
    "caption" => Caption, "center" => Center, "cite" => Cite, "code" => Code, "col" => Col, "colgroup" => Colgroup,
    "data" => Data, "dd" => Dd, "del" => Del, "desc" => Desc, "details" => Details, "dfn" => Dfn, "div" => Div, "dl" => Dl, "dt" => Dt,
    "em" => Em,
    "figcaption" => Figcaption, "figure" => Figure, "font" => Font, "foreignObject" => ForeignObject, "form" => Form,
    "h1" => H1, "h2" => H2, "h3" => H3, "h4" => H4, "h5" => H5, "h6" => H6, "hr" => Hr,
    "i" => I, "iframe" => Iframe, "img" => Img, "input" => Input, "ins" => Ins,
    "kbd" => Kbd,
    "label" => Label, "legend" => Legend, "li" => Li, "link" => Link,
    "map" => Map, "mark" => Mark, "math" => Math, "meta" => Meta, "mi" => Mi, "mo" => Mo, "mn" => Mn, "ms" => Ms, "mtext" => Mtext,
    "nobr" => Nobr,
    "object" => Object, "ol" => Ol, "optgroup" => Optgroup, "option" => Option,
    "p" => P, "param" => Param, "pre" => Pre,
    "q" => Q,
    "rb" => Rb, "rbc" => Rbc, "rp" => Rp, "rt" => Rt, "rtc" => Rtc, "ruby" => Ruby,
    "s" => S, "samp" => Samp, "select" => Select, "small" => Small, "source" => Source, "span" => Span, "strike" => Strike, "strong" => Strong, "sub" => Sub, "summary" => Summary, "sup" => Sup, "style" => Style, "svg" => Svg,
    "table" => Table, "tbody" => Tbody, "td" => Td, "textarea" => Textarea, "tfoot" => Tfoot, "th" => Th, "thead" => Thead, "time" => Time, "title" => Title, "tr" => Tr, "track" => Track, "tt" => Tt,
    "u" => U, "ul" => Ul,
    "var" => Var, "video" => Video,
    "wbr" => Wbr,
}

impl Tag {
    /// Create a new `Tag` for the known `name`, or `None` if `name` is not
    /// a known HTML5 tag.
    #[inline]
    fn known(name: &str) -> Option<Self> {
        KNOWN_TAGS.get(name.into()).copied()
    }

    /// Creates a new `Tag` with the given `name`. If the name is not a known
    /// HTML5 tag, a custom tag will be used or created in `custom`.
    fn new(name: &str, custom: &mut IndexSet<Uncased<'static>>) -> Self {
        if let Some(tag) = Self::known(name) {
            tag
        } else if let Some(index) = custom.get_index_of(UncasedStr::new(name)) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "if there are more than u8::MAX custom tags, this would have panicked during the insert"
            )]
            Tag::Custom(index as u8)
        } else {
            let index = custom.len();
            custom.insert(Uncased::from_borrowed(name).into_owned());
            Tag::Custom(index.try_into().unwrap())
        }
    }

    /// Returns true if this is a block-level start tag. (This is not a defined
    /// category in HTML5, just a long list of ad-hoc tag names.)
    #[inline]
    fn is_body_block(self) -> bool {
        matches!(
            self,
            Self::Address
                | Self::Aside
                | Self::Center
                | Self::Details
                | Self::Div
                | Self::Dl
                | Self::Figcaption
                | Self::Figure
                | Self::Ol
                | Self::P
                | Self::Summary
                | Self::Ul
        )
    }

    /// Returns true if this is an “element in button scope”.
    #[inline]
    fn is_button_scope(self) -> bool {
        self.is_general_scope() || matches!(self, Self::Button)
    }

    /// Returns true if this is a definition list item.
    #[inline]
    fn is_dl_item(self) -> bool {
        matches!(self, Self::Dd | Self::Dt)
    }

    /// Returns true if this is a tag in the HTML5 formatting category.
    #[inline]
    fn is_formatting(self) -> bool {
        matches!(
            self,
            Self::A
                | Self::B
                | Self::Big
                | Self::Code
                | Self::Em
                | Self::Font
                | Self::I
                | Self::Nobr
                | Self::S
                | Self::Small
                | Self::Strike
                | Self::Strong
                | Self::Tt
                | Self::U
        )
    }

    /// Returns true if this is an “element in scope”.
    #[inline]
    fn is_general_scope(self) -> bool {
        // Ignoring applet, html, marquee, and template
        matches!(
            self,
            Self::AnnotationXml
                | Self::Caption
                | Self::Desc
                | Self::ForeignObject
                | Self::Mi
                | Self::Mo
                | Self::Mn
                | Self::Ms
                | Self::Mtext
                | Self::Object
                | Self::Select
                | Self::Table
                | Self::Td
                | Self::Th
                | Self::Title
        )
    }

    /// Returns true if this is a `<head>` item.
    #[inline]
    fn is_head_item(self) -> bool {
        matches!(
            self,
            Self::Basefont | Self::Link | Self::Meta | Self::Style | Self::Title
        )
    }

    /// Returns true if this is a heading element.
    #[inline]
    fn is_heading(self) -> bool {
        matches!(
            self,
            Self::H1 | Self::H2 | Self::H3 | Self::H4 | Self::H5 | Self::H6
        )
    }

    /// Returns true if this is a tag with an implied end tag.
    #[inline]
    fn is_implied_end(self) -> bool {
        matches!(
            self,
            Self::Dd
                | Self::Dt
                | Self::Li
                | Self::Optgroup
                | Self::Option
                | Self::P
                | Self::Rb
                | Self::Rp
                | Self::Rt
                | Self::Rtc
        )
    }

    /// Returns true if this is an “inline” tag, according to MediaWiki’s
    /// `RemexCompatMunger`.
    fn is_inline(self) -> bool {
        matches!(
            self,
            Self::A
                | Self::Abbr
                | Self::Acronym
                | Self::Audio
                | Self::B
                | Self::Basefont
                | Self::Bdi
                | Self::Bdo
                | Self::Big
                | Self::Br
                | Self::Button
                | Self::Cite
                | Self::Code
                | Self::Data
                | Self::Del
                | Self::Dfn
                | Self::Em
                | Self::Font
                | Self::I
                | Self::Iframe
                | Self::Img
                | Self::Input
                | Self::Ins
                | Self::Kbd
                | Self::Label
                | Self::Legend
                | Self::Map
                | Self::Mark
                | Self::Object
                | Self::Param
                | Self::Q
                | Self::Rb
                | Self::Rbc
                | Self::Rp
                | Self::Rt
                | Self::Rtc
                | Self::Ruby
                | Self::S
                | Self::Samp
                | Self::Select
                | Self::Small
                | Self::Source
                | Self::Span
                | Self::Strike
                | Self::Strong
                | Self::Sub
                | Self::Sup
                | Self::Textarea
                | Self::Time
                | Self::Track
                | Self::Tt
                | Self::U
                | Self::Var
                | Self::Video
                | Self::Wbr
        )
    }

    /// Returns true if this is an “element in list item scope”.
    #[inline]
    fn is_list_item_scope(self) -> bool {
        self.is_general_scope() || matches!(self, Self::Ol | Self::Ul)
    }

    /// Returns true if this “is in the special category, but is not an address,
    /// div, or p element”.
    #[inline]
    fn is_list_special(self) -> bool {
        self.is_special() && !matches!(self, Self::Address | Self::Div | Self::P)
    }

    /// Returns true if this is a `<ruby>` item.
    #[inline]
    fn is_ruby_item(self) -> bool {
        matches!(self, Self::Rb | Self::Rp | Self::Rt | Self::Rtc)
    }

    /// Returns true if this tag is in the “special” category.
    fn is_special(self) -> bool {
        // Ignoring applet, area, article, base, bgsound, body, dir, embed,
        // fieldset, footer, frame, frameset, head, header, hgroup, html,
        // keygen, listing, main, marquee, menu, nav, noembed, noframes,
        // noscript, plaintext, script, search, section, template, and xmp,
        // which are unsupported in this implementation
        matches!(
            self,
            Self::Address
                | Self::Aside
                | Self::Basefont
                | Self::Blockquote
                | Self::Br
                | Self::Button
                | Self::Caption
                | Self::Center
                | Self::Col
                | Self::Colgroup
                | Self::Dd
                | Self::Details
                | Self::Div
                | Self::Dl
                | Self::Dt
                | Self::Figcaption
                | Self::Figure
                | Self::Form
                | Self::H1
                | Self::H2
                | Self::H3
                | Self::H4
                | Self::H5
                | Self::H6
                | Self::Hr
                | Self::Iframe
                | Self::Img
                | Self::Input
                | Self::Li
                | Self::Link
                | Self::Meta
                | Self::Object
                | Self::Ol
                | Self::P
                | Self::Param
                | Self::Pre
                | Self::Select
                | Self::Source
                | Self::Style
                | Self::Summary
                | Self::Table
                | Self::Tbody
                | Self::Td
                | Self::Textarea
                | Self::Tfoot
                | Self::Th
                | Self::Thead
                | Self::Title
                | Self::Tr
                | Self::Track
                | Self::Ul
                | Self::Wbr
                | Self::Mi
                | Self::Mo
                | Self::Mn
                | Self::Ms
                | Self::Mtext
                | Self::AnnotationXml
                | Self::ForeignObject
                | Self::Desc
        )
    }

    /// Returns true if this is a `<table>` direct child.
    #[inline]
    fn is_table_body(self) -> bool {
        matches!(self, Self::Tbody | Self::Tfoot | Self::Thead)
    }

    /// Returns true if this is a `<table>` element that cannot contain most
    /// non-table content.
    #[inline]
    fn is_table_fosterable(self) -> bool {
        self.is_table_body() || matches!(self, Self::Table | Self::Tr)
    }

    /// Returns true if this is a `<table>` item (including grandchildren).
    #[inline]
    fn is_table_item(self) -> bool {
        matches!(
            self,
            Self::Caption
                | Self::Col
                | Self::Colgroup
                | Self::Tbody
                | Self::Td
                | Self::Tfoot
                | Self::Th
                | Self::Thead
                | Self::Tr
        )
    }

    /// Returns true if this is an “element in table scope”.
    #[inline]
    fn is_table_scope(self) -> bool {
        // Ignoring html and template, which are unsupported
        matches!(self, Self::Table)
    }
}
