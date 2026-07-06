//! HTML5(-ish) tree transformer.

use super::{Buffer, Chain, Sink};
use crate::StripMarker;
use core::{
    num::{NonZeroU8, NonZeroU16},
    ops::{Bound, RangeBounds},
};
use either::Either;
use indexmap::IndexSet;
use libwikitext_parse::VOID_TAGS;
use uncased::{Uncased, UncasedStr};

/// Balances the DOM tree using the HTML5 tree construction algorithm(ish).
#[derive(Debug)]
pub(crate) struct DomTree<S> {
    /// The set of tags not matching any known HTML5 tag.
    custom_tags: CustomTags,
    /// If true, filtering out an invalid start tag.
    filtering: bool,
    /// The index of the rightmost `<form>` element in [`Self::stack`].
    form_index: Option<u8>,
    /// The “list of active formatting elements”.
    format: FormattingList,
    /// If true, currently in an HTML start tag.
    in_attr: bool,
    /// The current parser mode.
    mode: Mode,
    /// The newline filtering state.
    newline_mode: NewlineState,
    /// The pending “in table text” text which may be fostered, or not,
    /// depending on whether the *entire run of text* contains only ASCII
    /// whitespace.
    pending_text: String,
    /// The “stack of open elements”.
    stack: Stack<S>,
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
            pending_text: <_>::default(),
            stack: Stack::new(next),
        }
    }

    /// Runs the “adoption agency algorithm”, either for a formatting end `tag`,
    /// or for a start `<nobr>`. Numbers are the step numbers from the HTML5 LS
    /// dated 25 June 2026.
    fn adopt(&mut self, tag: Tag) {
        // TODO: This ends up being O(n) but could be O(1) if `self.format` had
        // a counter table.
        // 2.
        if self.stack.pop_if(&self.custom_tags, |node| {
            *node == tag && !self.format.contains(tag)
        }) {
            // The top of the stack was the corresponding start tag, but somehow
            // it is not in the list of active formatting elements? How could
            // this possibly happen?
            return;
        }

        // 3..4.19. By wasting a lot of time, the source of these magic 8 and 3
        // loop limits was found: an arbitrary choice to parse ~99.95%
        // successfully in 2010.
        // <https://www.w3.org/Bugs/Public/show_bug.cgi?id=10801>

        // For steps 4 and later, the spec source has a diagram showing the
        // visual layout of the elements being manipulated by the algorithm,
        // which would be probably helpful for implementers if it were not
        // commented out. Here’s a more comprehensive adaptation of that
        // diagram:
        //
        // <commonElement>
        //   (<formattingElement> ...)*
        //     innerLoop:(<formattingElement> ...other nodes...)*<0,3>
        //       outerLoop:(<furthestBlock> ...other nodes...)*<0,8>
        //         (<furthestBlock> ...)*
        //           destroyer_of_worlds:</{{tag}}>
        //
        // For something like
        // `<b><i><u><s><tt>a<div><div><div><div><div><div><div><div><div></b>`,
        // the algorithm will end up emitting:
        //
        // (In the comment annotations below, `;` represents the position of
        // `furthestBlock` when the “list of active formatting elements” is
        // compared to the “list of open elements”.)
        //
        // ```html
        // <b><i><u><s><tt>a</tt></s></u></i></b> <!-- [1] AFE b,i,u,s,tt; => i,u,s,tt; -->
        // <u><s><tt>                             <!-- [2] AFE i,u,s,tt; => u,s,tt; -->
        //   <div><b>                             <!-- [3] AFE u,s,tt; => u,s,tt;b -->
        //   </b><div><b>
        //   </b><div><b>
        //   </b><div><b>
        //   </b><div><b>
        //   </b><div><b>
        //   </b><div><b>
        //   </b><div><b>                         <!-- [4] -->
        //   <div>
        // ```
        //
        // [1]: Closes `<{{tag}}>` at `<furthestBlock>`.
        // [2]: Reopens, at most, the three right-most formatting elements
        //      before `<furthestBlock>`, and discards the rest.
        // [3]: Puts a `<{{tag}}>` after `<furthestBlock>` and a
        //      `</{{tag}}>` before `<furthestBlock + 1>`, eight times at
        //      most.
        // [4]: Moves `<{{tag}}>` in the list of active formatting elements
        //      such that it is at the start of the list of formatting elements
        //      after the last reformatted `<furthestBlock>`.
        //
        // Each time an adoption occurs, the same process runs in the same
        // order, so this causes a pile-up of empty formatting elements
        // before the first `<furtherBlock>`. For instance, if the next
        // token after `</b>` in this example were `</u>`:
        //
        // ```html
        // <b><i><u><s><tt>a</tt></s></u></i></b> <!-- [0] -->
        // <u><s><tt></tt></s></u>                <!-- [1] AFE u,s,tt;b => s,tt;b -->
        // <s><tt>                                <!-- [2] AFE s,tt;b -->
        //   <div><u><b></b>                      <!-- [3] AFE s,tt;u,b -->
        //   </u><div><u><b></b>
        //   </u><div><u><b></b>
        //   </u><div><u><b></b>
        //   </u><div><u><b></b>
        //   </u><div><u><b></b>
        //   </u><div><u><b></b>
        //   </u><div><u><b>                      <!-- [4] -->
        //   <div>
        // ```
        //
        // [0]: The first line is not a participant in this run of the
        //      algorithm, because it was already closed by the previous
        //      run of the algorithm. The algorithm only runs on the open
        //      stack to the right of `<common>`.

        // 3..4.18.
        // 4.3. `format_index` is the index of the corresponding start tag of
        // `tag` in the list of formatting elements.
        let Some(format_index) = self.format.rfind_scoped(|node| *node == tag) else {
            // No corresponding formatting start tag after the last marker means
            // this is either a mismatched end tag which will be ignored, or the
            // corresponding start tag is outside the formatting scope, in which
            // case it is also ignored.
            self.tag_end_default(tag);
            return;
        };

        // 4.5. `start` is the index of the corresponding start tag in the list
        // of open elements. This scope checked scan goes first because it does
        // less traversal to fail than step 4.4.
        let Some(start) = self
            .stack
            .index_in_scope(|node| *node == tag, Tag::is_general_scope)
        else {
            // 4.4.
            if !self.stack.contains(&tag.into()) {
                // There was no corresponding start tag in *any* scope, so it
                // must have been implicitly closed and the spec says that means
                // it goes to the soylent factory, rip.
                self.format.remove(format_index);
            }

            // If there is no corresponding start tag in scope, then there is
            // nothing to do right now with this mismatched end tag and it can
            // be ignored.
            return;
        };

        // 4.7: `furthest` is the index of the “special” category element
        // closest to the formatting start tag (“furthest” from `</{{tag}}>`).
        let Some(mut furthest) = self.stack.next_special(start) else {
            // 4.8: There wasn’t any “furthest”, so all the formatting and
            // ordinary elements get closed now, and any formatting elements
            // that weren’t `<{{tag}}>` will be reopened by `reformat` later.
            self.stack.pop_range(&self.custom_tags, start..);
            self.format.remove(format_index);
            return;
        };

        // Now to brazenly ignore the rest of the spec to implement something
        // which is hopefully compatible, but easier to do without a whole-ass
        // tree.

        // First, calculate the split for the list of active formatting elements
        // at `furthest`.
        // TODO: An additional marker in the formatting list at the point of
        // `furthest` would be useful to avoid this scan.
        let next_format_index = format_index + 1;
        let reopen_end = self
            .format
            .rfind_scoped(|node| self.stack.contains_after(furthest, node))
            .unwrap_or(self.format.len());
        let reopen_start = reopen_end.saturating_sub(3).max(next_format_index);

        // Next, split the stack of open elements in half at `furthest`.
        self.stack.pop_range(&self.custom_tags, start..furthest);

        furthest = start;

        // Next, reopen up to three of the last formatting elements which were
        // neither before `tag` nor after `furthest`.
        for (tag, attrs) in self.format.iter(reopen_start..reopen_end) {
            self.stack.insert(&self.custom_tags, furthest, tag, attrs);
            furthest += 1;
        }

        // Now, anything in the formatting list that isn’t one of our lucky
        // three leftovers gets sent away on the train for orphans, and the
        // adopted tag gets moved to after the `furthest` split point.
        let num_closed = reopen_start - next_format_index;
        for _ in next_format_index..reopen_start {
            self.format.remove(next_format_index);
        }

        // Now, for up to 8 iterations, insert `<{{tag}}>` after `furthest` and
        // `</{{tag}}>` before the next `furthest`. This does not change the
        // stack because only the next `furthest` is technically still open.
        let name = tag.as_str(&self.custom_tags).as_str();
        let attrs = self.format.attributes(format_index);
        let mut max = 8;

        for (index, buffered) in self.stack.specials(furthest) {
            buffered.insert_first(name, attrs_iter(attrs));
            furthest = index;
            max -= 1;
            if max == 0 {
                break;
            }
            buffered.tag_end(name);
        }

        // Finally, if the algorithm terminated early because there were too
        // many nested elements, the formatting element just gets to continue
        // to be open for forever. Otherwise, it needs to be closed.
        if max == 0 {
            self.stack.split_element(furthest, tag);
            self.format.rotate(format_index, reopen_end - num_closed);
        } else {
            self.format.remove(format_index);
        }
    }

    /// Closes the nearest table cell element.
    fn close_cell(&mut self) {
        // The spec pops all implied end tags first to track errors, but this
        // implementation does not need to track errors
        self.pop_inclusive(|node| matches!(node.tag(), Some(Tag::Td | Tag::Th)));
        self.format.clear_to_marker();
        self.mode = Mode::Row;
    }

    /// Closes the nearest implicit `<p>` wrapper, if one exists.
    fn close_implicit_p(&mut self, scope: impl FnMut(Tag) -> bool) -> bool {
        if let Some(index) = self
            .stack
            .index_in_scope(|node| node.is_implicit_p(), scope)
        {
            // The decision about whether or not p-wrapping should occur needs
            // to be made before unclosed formatting elements are folded into
            // the implicit p-wrapper buffer because `<b><i><div>` should *not*
            // cause p-wrapping, but `<b><i></i><div>` (and `<b><i>a<div>`)
            // *should*, and there is no way to differentiate `<b><i><div>` from
            // `<b><i></i><div>` once folding has happened. The original parser
            // handles this by collecting everything, including block-level
            // elements, into a `<mw:p-wrap>`, and then because it is building
            // a whole-ass tree, will walk from the `<div>` up the tree until it
            // finds either a split candidate or hits the `<mw:p-wrap>`.
            let end = self.stack.next_non_pwrap(index);
            let reopen_end = self
                .format
                .rfind_scoped(|node| self.stack.contains_after(end, node))
                .unwrap_or(self.format.len());
            let reopen_start = self
                .format
                .lfind_scoped(|node| self.stack.contains_after(index, node))
                .unwrap_or(self.format.len());
            self.stack.pop_range(&self.custom_tags, index..end);
            self.reformat_range(index, reopen_start..reopen_end);
            true
        } else {
            false
        }
    }

    /// Closes the nearest `<p>` element “in button scope”, if one exists.
    #[inline]
    fn close_p(&mut self) {
        if !self.close_implicit_p(Tag::is_button_scope) {
            // The spec pops all implied end tags first to track errors, but
            // this implementation does not need to track errors
            self.pop_in_scope(|node| *node == Tag::P, Tag::is_button_scope);
        }
    }

    /// Performs special fixups for nested `<a>` tags.
    fn fixup_anchor(&mut self, tag: Tag) {
        if self.format.rfind_scoped(|node| *node == tag).is_some() {
            self.adopt(tag);
            // “remove that element from the list of active formatting elements
            // and the stack of open elements if the adoption agency algorithm
            // didn’t already remove it (it might not have if the element is not
            // in table scope)”. The spec suggests in §13.3 that anchors are
            // allowed to nest in the case of fostering, until they are
            // serialised, then they are not. Since this is a serialiser it
            // should be the case that these things never nest.
            if let Some(index) = self.format.rfind_any(|node| *node == tag) {
                self.format.remove(index);
            }
            self.pop_in_scope(|node| *node == tag, |_| false);
        }
    }

    /// Flushes pending “in table text” text to its correct final destination.
    fn flush_pending_text(&mut self) {
        if self.pending_text.is_empty() {
            return;
        }
        self.stack.flush_text(&self.pending_text);
        self.pending_text.clear();
    }

    /// Pop all elements on the stack with implied end tags except for `except`.
    fn implied_end(&mut self, except: Option<Tag>) {
        while self
            .stack
            .pop_if(&self.custom_tags, |node| node.is_implied_close(except))
        {}
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
        self.stack.index_in_scope(predicate, scope).is_some()
    }

    /// Returns true if the tree is “in table text”.
    #[inline]
    fn in_table_text(&self) -> bool {
        self.mode.foster_text() && self.stack.in_table_text()
    }

    /// Closes all elements up to `predicate`.
    fn pop_exclusive(&mut self, mut predicate: impl FnMut(&mut TagNode) -> bool) {
        while self
            .stack
            .pop_if(&self.custom_tags, |node| !predicate(node))
        {}
    }

    /// Closes all elements up to and including `predicate` if a match exists in
    /// the scope given by `scope`, returning `true` if elements were closed.
    fn pop_in_scope(
        &mut self,
        predicate: impl FnMut(&TagNode) -> bool,
        scope: impl FnMut(Tag) -> bool,
    ) -> bool {
        if let Some(index) = self.stack.index_in_scope(predicate, scope) {
            self.stack.pop_range(&self.custom_tags, index..);
            true
        } else {
            false
        }
    }

    /// Closes all elements up to and including `predicate`.
    fn pop_inclusive(&mut self, predicate: impl FnMut(&mut TagNode) -> bool) {
        self.pop_exclusive(predicate);
        self.stack.pop(&self.custom_tags);
    }

    /// Reopens any formatting elements which were closed due to element
    /// splitting.
    fn reformat(&mut self) {
        // TODO: This is O(n^2), but could be made O(n) by having a tag count
        // table for the stack.
        let first_missing = self
            .format
            .rfind_scoped(|node| self.stack.contains(node))
            .map_or_else(
                || {
                    self.format
                        .marker_index
                        .map_or(0, |index| usize::from(index) + 1)
                },
                |index| index + 1,
            );
        self.reformat_range(self.stack.len(), first_missing..);
    }

    /// Reopens formatting elements in the given formatting list `range` at the
    /// given stack `index`.
    fn reformat_range<R: core::slice::SliceIndex<[FormattingItem], Output = [FormattingItem]>>(
        &mut self,
        index: usize,
        range: R,
    ) {
        for (i, (tag, attrs)) in self.format.iter(range).enumerate() {
            self.stack.insert(&self.custom_tags, index + i, tag, attrs);
        }
    }

    /// Slowly recalculates the current insertion mode according to what
    /// elements are on the stack.
    fn reset_mode(&mut self) {
        let mode = self.stack.rfind_map(|(index, node)| match node.tag() {
            Some(Tag::Td | Tag::Th) if index != 0 => Some(Mode::Cell),
            Some(Tag::Tr) => Some(Mode::Row),
            Some(Tag::Tbody | Tag::Tfoot | Tag::Thead) => Some(Mode::TableBody),
            Some(Tag::Caption) => Some(Mode::Caption),
            Some(Tag::Table) => Some(Mode::Table),
            _ => None,
        });

        self.mode = mode.unwrap_or(Mode::Body);
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
                if self.stack.rfind(|node| **node == tag).is_some() {
                    if tag == Tag::Blockquote {
                        self.close_implicit_p(|tag| tag == Tag::Blockquote);
                    }
                    self.implied_end(None);
                    self.pop_inclusive(|node| *node == tag);
                }
            }
            Tag::Br => {
                if self.tag_start_body(tag) {
                    self.stack.target().tag_start_full("br");
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
                self.close_implicit_p(Tag::is_button_scope);
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
                self.mode = Mode::Table;
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
                self.mode = Mode::Row;
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
        if tag != Tag::Col
            && self
                .stack
                .pop_if(&self.custom_tags, |node| *node == Tag::Colgroup)
        {
            self.mode = Mode::Table;
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
                self.mode = Mode::TableBody;
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
            self.stack.enable_fostering();
            self.tag_end_body(tag);
            self.stack.disable_fostering();
        }
    }

    /// Inserts a new end `tag` in the “in table body” insertion mode.
    fn tag_end_table_body(&mut self, tag: Tag) {
        if tag == Tag::Table || tag.is_table_body() {
            if self.pop_in_scope(|node| *node == tag, Tag::is_table_scope) {
                self.mode = Mode::Table;
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
            Mode::Body => self.tag_start_body(tag),
            Mode::Table => self.tag_start_table(tag),
            Mode::Caption => self.tag_start_caption(tag),
            Mode::ColumnGroup => self.tag_start_colgroup(tag),
            Mode::TableBody => self.tag_start_table_body(tag),
            Mode::Row => self.tag_start_row(tag),
            Mode::Cell => self.tag_start_cell(tag),
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
                self.stack.push(tag);
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
                self.stack.push(tag);
                EMIT
            }
            Tag::Form => {
                if self.form_index.is_none() {
                    self.form_index = Some(self.stack.len().try_into().unwrap());
                    self.close_p();
                    self.stack.push(tag);
                    EMIT
                } else {
                    DISCARD
                }
            }
            tag if tag.is_heading() => {
                self.close_p();
                self.stack
                    .pop_if(&self.custom_tags, |node| node.is_heading());
                self.stack.push(tag);
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
                self.stack.push(tag);
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
                self.stack.push(tag);
                EMIT
            }
            Tag::Option | Tag::Optgroup => {
                if self.in_scope(|node| *node == Tag::Select, Tag::is_general_scope) {
                    let except = (tag == Tag::Option).then_some(Tag::Optgroup);
                    self.implied_end(except);
                } else {
                    self.stack
                        .pop_if(&self.custom_tags, |node| *node == Tag::Option);
                }
                self.reformat();
                self.stack.push(tag);
                EMIT
            }
            Tag::Pre | Tag::Textarea => {
                // For `<textarea>` the spec says to switch to RCDATA but this
                // is not a tokeniser
                if tag == Tag::Pre {
                    self.close_p();
                }
                self.newline_mode = NewlineState::IgnoreNext;
                self.stack.push(tag);
                EMIT
            }
            Tag::Select if self.pop_in_scope(|node| *node == tag, Tag::is_general_scope) => DISCARD,
            Tag::Select => {
                self.reformat();
                self.stack.push(tag);
                EMIT
            }
            Tag::Table => {
                self.close_p();
                self.mode = Mode::Table;
                self.stack.push_table();
                EMIT
            }
            tag if tag.is_body_block() => {
                self.close_p();
                self.stack.push(tag);
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
                self.stack.push(tag);
                EMIT
            }
            tag if tag.is_ruby_item() => {
                if self.in_scope(|node| *node == Tag::Ruby, Tag::is_general_scope) {
                    let except = matches!(tag, Tag::Rp | Tag::Rt).then_some(Tag::Rtc);
                    self.implied_end(except);
                }
                self.stack.push(tag);
                EMIT
            }
            tag if tag.is_table_item() => DISCARD,
            _ => {
                self.reformat();
                self.stack.push(tag);
                EMIT
            }
        }
    }

    /// Inserts a new start `tag` in the “in caption” insertion mode.
    fn tag_start_caption(&mut self, tag: Tag) -> bool {
        if tag.is_table_item() {
            if self.pop_in_scope(|node| *node == Tag::Caption, Tag::is_table_scope) {
                self.format.clear_to_marker();
                self.mode = Mode::Table;
                self.tag_start_table(tag)
            } else {
                DISCARD
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
        } else if self
            .stack
            .pop_if(&self.custom_tags, |node| *node == Tag::Colgroup)
        {
            self.mode = Mode::Table;
            self.tag_start_table(tag)
        } else {
            DISCARD
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
                self.stack.push(tag);
                EMIT
            }
            Tag::Style => {
                // This is supposed to use the generic raw text element parsing
                // algorithm, but since the tokeniser has already done its
                // thing, just treat it like a normal whatever
                self.stack.push(tag);
                EMIT
            }
            _ => panic!("should never get here"),
        }
    }

    /// Inserts a new start `tag` in the “in row” insertion mode.
    fn tag_start_row(&mut self, tag: Tag) -> bool {
        if matches!(tag, Tag::Td | Tag::Th) {
            self.pop_exclusive(|node| *node == Tag::Tr);
            self.mode = Mode::Cell;
            self.format.push_marker();
            self.stack.push(tag);
            EMIT
        } else if tag.is_table_item() {
            if self.pop_in_scope(|node| *node == Tag::Tr, Tag::is_table_scope) {
                self.mode = Mode::TableBody;
                self.tag_start_table_body(tag)
            } else {
                DISCARD
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
                self.mode = Mode::Caption;
                self.format.push_marker();
                self.stack.push(tag);
                EMIT
            }
            Tag::Colgroup => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = Mode::ColumnGroup;
                self.stack.push(tag);
                EMIT
            }
            Tag::Col => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.stack.push(Tag::Colgroup);
                self.stack.target().tag_start_full("colgroup");
                self.mode = Mode::ColumnGroup;
                self.tag_start_colgroup(tag)
            }
            Tag::Tbody | Tag::Tfoot | Tag::Thead => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.mode = Mode::TableBody;
                self.stack.push(tag);
                EMIT
            }
            Tag::Td | Tag::Th | Tag::Tr => {
                self.pop_exclusive(|node| *node == Tag::Table);
                self.stack.push_tbody();
                self.mode = Mode::TableBody;
                self.tag_start_table_body(tag)
            }
            Tag::Table => {
                if self.pop_in_scope(|node| *node == Tag::Table, Tag::is_table_scope) {
                    self.reset_mode();
                    self.tag_start_any(tag)
                } else {
                    DISCARD
                }
            }
            Tag::Style => self.tag_start_head(tag),
            Tag::Input => {
                // The spec says that <input type="hidden"> are not supposed to
                // be fostered but this is a needless complexity which would
                // require tracking attributes since this is not supported in
                // normal Wikitext anyway
                self.stack.enable_fostering();
                self.tag_start_body(tag)
            }
            Tag::Form => {
                // The spec says that form in a table is supposed to cause
                // the form pointer to be set, but then to not emit anything
                // to the output. For a serialiser, this just means to not
                // emit anything
                DISCARD
            }
            tag => {
                self.stack.enable_fostering();
                self.tag_start_body(tag)
            }
        }
    }

    /// Inserts a new start `tag` in the “in table body” insertion mode.
    fn tag_start_table_body(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Tr => {
                self.pop_exclusive(|node| node.is_table_body());
                self.mode = Mode::Row;
                self.stack.push(tag);
                EMIT
            }
            Tag::Td | Tag::Th => {
                self.pop_exclusive(|node| node.is_table_body());
                self.mode = Mode::Row;
                self.stack.push(Tag::Tr);
                self.stack.target().tag_start_full("tr");
                self.tag_start_row(tag)
            }
            tag if tag.is_table_item() => {
                if self.in_scope(|node| node.is_table_body(), Tag::is_table_scope) {
                    self.pop_exclusive(|node| *node == Tag::Table);
                    self.mode = Mode::Table;
                    self.tag_start_table(tag)
                } else {
                    DISCARD
                }
            }
            _ => self.tag_start_table(tag),
        }
    }
}

impl<S> Chain for DomTree<S> {
    type Next = S;

    fn next(&self) -> &Self::Next {
        &self.stack.next.next
    }

    fn next_mut(&mut self) -> &mut Self::Next {
        &mut self.stack.next.next
    }
}

impl<S: Sink> Sink for DomTree<S> {
    #[inline]
    fn comment_end(&mut self) {
        if self.filtering {
            return;
        }

        debug_assert!(!self.in_attr);
        self.flush_pending_text();
        self.newline_mode = <_>::default();
        self.stack.target().comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        if self.filtering {
            return;
        }

        debug_assert!(!self.in_attr);
        self.flush_pending_text();
        self.newline_mode = <_>::default();
        self.stack.target().comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.format.buffer_char(value);
            self.stack.target().entity(value, raw);
        } else {
            if self.in_table_text() {
                self.pending_text.push(value);
            } else {
                self.stack.push_pwrap();
                if self.mode.reformat_text() {
                    self.reformat();
                }
                self.stack.target().entity(value, raw);
            }
            self.newline_mode = <_>::default();
        }
    }

    #[inline]
    fn finish(mut self) -> String {
        // Any implicit p must be explicitly closed to handle e.g. `<b>a<div>␄`
        self.close_implicit_p(|_| false);
        self.stack.finish(&self.custom_tags)
    }

    #[inline]
    fn new_line(&mut self) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.stack.target().new_line();
        } else if self.newline_mode == NewlineState::IgnoreNext {
            self.newline_mode = NewlineState::JustIgnored;
        } else if self.newline_mode == NewlineState::JustIgnored {
            if self.in_table_text() {
                self.pending_text += "\n\n";
            } else {
                self.stack.target().new_line();
                self.stack.target().new_line();
            }
            self.newline_mode = <_>::default();
        } else if self.in_table_text() {
            self.pending_text.push('\n');
        } else {
            self.stack.target().new_line();
            self.stack.push_pwrap();
            if self.mode.reformat_text() {
                self.reformat();
            }
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if self.filtering {
            return;
        }

        if !self.in_attr {
            self.flush_pending_text();
            self.newline_mode = <_>::default();
        }
        self.stack.target().strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if !self.filtering {
            self.format.tag_attribute_end();
            self.stack.target().tag_attribute_end(name);
        }
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if !self.filtering {
            self.format.tag_attribute_start(name);
            self.stack.target().tag_attribute_start(name);
        }
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.flush_pending_text();
        self.newline_mode = <_>::default();

        let tag = Tag::new(name, &mut self.custom_tags);

        match self.mode {
            Mode::Body => self.tag_end_body(tag),
            Mode::Table => self.tag_end_table(tag),
            Mode::Caption => self.tag_end_caption(tag),
            Mode::ColumnGroup => self.tag_end_colgroup(tag),
            Mode::TableBody => self.tag_end_table_body(tag),
            Mode::Row => self.tag_end_row(tag),
            Mode::Cell => self.tag_end_cell(tag),
        }

        // This is here exclusively to match the whitespace output of the PHP
        // parser, the `push_pwrap` calls in other places work just fine to
        // create the correct tree
        if !tag.is_inline() {
            self.stack.push_pwrap();
        }
    }

    #[inline]
    fn tag_start(&mut self, mut name: &str) {
        self.flush_pending_text();
        self.newline_mode = <_>::default();

        if name.eq_ignore_ascii_case("image") {
            name = "img";
        }

        let tag = Tag::new(name, &mut self.custom_tags);
        if tag.is_inline() {
            self.stack.push_pwrap();
        }
        if self.tag_start_any(tag) {
            self.in_attr = true;
            self.stack.target().tag_start(name);
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
            self.stack.target().tag_start_end(name);
            self.stack.push_pwrap();
        }
    }

    #[inline]
    fn text(&mut self, text: &str) {
        if self.filtering {
            return;
        }

        if self.in_attr {
            self.format.buffer_text(text);
            self.stack.target().text(text);
        } else {
            if self.in_table_text() {
                self.pending_text += text;
            } else {
                if text.contains(|c: char| !c.is_ascii_whitespace()) {
                    self.stack.push_pwrap();
                }
                if self.mode.reformat_text() {
                    self.reformat();
                }
                self.stack.target().text(text);
            }
            self.newline_mode = <_>::default();
        }
    }
}

/// A buffered element fragment.
#[derive(Debug, Default)]
struct BufferedNode {
    /// The position in [`Self::buffer`] where the element’s body begins.
    body_pos: Option<NonZeroU16>,
    /// The body.
    buffer: Buffer,
    /// If true, the buffer contains something other than ASCII whitespace.
    contains_non_whitespace: bool,
    /// If true, currently processing an attribute.
    in_attr: bool,
    /// The number of tags in the buffer.
    tag_count: u16,
}

impl BufferedNode {
    /// Creates a new `BufferedNode` with the given tag `name` and `attrs`.
    fn new<'a>(name: &str, attrs: impl Iterator<Item = (&'a str, &'a str)>) -> Self {
        let mut this = Self::default();
        emit_tag(&mut this, name, attrs);
        this
    }

    /// Adds `other` to this node.
    #[inline]
    fn extend(&mut self, other: BufferedNode) {
        self.contains_non_whitespace |= other.contains_non_whitespace;
        self.tag_count += other.tag_count;
        self.buffer.extend(other.buffer);
    }

    /// Flushes the contents of the node to `next`.
    #[inline]
    fn flush_into<S: Sink + ?Sized>(&mut self, next: &mut S) {
        self.buffer.flush_into(next, false);
        self.contains_non_whitespace = false;
        self.tag_count = 0;
    }

    /// Inserts a tag with the given `name` and `attrs` to the start of this
    /// node’s body.
    #[inline]
    fn insert_first<'a>(&mut self, name: &str, attrs: impl Iterator<Item = (&'a str, &'a str)>) {
        let index = self.body_pos.expect("no early late insertion").get();
        self.buffer.insert(usize::from(index), |body| {
            emit_tag(body, name, attrs);
        });
    }

    /// Returns true if this buffer contains content which should be implicitly
    /// p-wrapped.
    fn is_p_wrappable(&self) -> bool {
        self.contains_non_whitespace || self.tag_count != 0
    }
}

impl Sink for BufferedNode {
    #[inline]
    fn comment_end(&mut self) {
        self.buffer.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.buffer.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.contains_non_whitespace |= !self.in_attr;
        self.buffer.entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        panic!("should not call this");
    }

    #[inline]
    fn new_line(&mut self) {
        self.buffer.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        log::warn!("TODO: strip marker contains not-whitespace?");
        self.buffer.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.buffer.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.buffer.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.buffer.tag_end(name);
        self.tag_count += 1;
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.buffer.tag_start(name);
        self.in_attr = true;
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.buffer.tag_start_end(name);
        self.body_pos
            .get_or_insert_with(|| NonZeroU16::new(self.buffer.len().try_into().unwrap()).unwrap());
        self.in_attr = false;
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.contains_non_whitespace |=
            !self.in_attr && text.contains(|c: char| !c.is_ascii_whitespace());
        self.buffer.text(text);
    }
}

/// A list of buffers for nodes that need to be buffered because they may be
/// victims of the foster parenting algorithm or the adoption agency algorithm.
#[derive(Debug)]
struct Buffers<S> {
    /// The base index for translating from a [`Self::stack`] index to a
    /// [`Self::buffered_nodes`] index.
    buffered_base_index: u8,
    /// The backing store for children that need to be buffered because they may
    /// be sent to the adoption agency, or because their children may be sent to
    /// the foster home. (Please forward all complaints to WHATWG.)
    buffered_nodes: Vec<BufferedNode>,
    /// The output.
    next: S,
}

impl<S: Sink> Buffers<S> {
    /// Creates a new `Buffers` which emits to `next`.
    #[inline]
    fn new(next: S) -> Self {
        Self {
            buffered_base_index: <_>::default(),
            buffered_nodes: <_>::default(),
            next,
        }
    }

    /// Removes the subslice of nodes given by `range`.
    fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> std::vec::Drain<'_, BufferedNode> {
        let range = self.local_range(range);
        self.buffered_nodes.drain(range)
    }

    /// Gets a mutable reference to the node at the given `index`.
    #[inline]
    fn get_mut(&mut self, index: usize) -> Option<&mut BufferedNode> {
        self.local_index(index)
            .and_then(|index| self.buffered_nodes.get_mut(index))
    }

    /// Gets a slice of the nodes in the given `range`.
    #[inline]
    fn get_range<R: RangeBounds<usize>>(&self, range: R) -> &[BufferedNode] {
        let range = self.local_range(range);
        self.buffered_nodes.get(range).unwrap_or_default()
    }

    /// Gets a mutable slice of the nodes in the given `range`.
    #[inline]
    fn get_range_mut<R: RangeBounds<usize>>(&mut self, range: R) -> &mut [BufferedNode] {
        let range = self.local_range(range);
        self.buffered_nodes.get_mut(range).unwrap_or_default()
    }

    /// Inserts `node` at the given `index`.
    fn insert(&mut self, index: usize, node: BufferedNode) {
        if self.buffered_nodes.is_empty() {
            self.buffered_base_index = index.try_into().unwrap();
        }
        let index = index - usize::from(self.buffered_base_index);
        self.buffered_nodes.insert(index, node);
    }

    /// Returns true if this list of buffers is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.buffered_nodes.is_empty()
    }

    /// Converts a [`Stack`] index into a [`Buffers`] index.
    #[inline]
    fn local_index(&self, index: usize) -> Option<usize> {
        index.checked_sub(self.buffered_base_index.into())
    }

    /// Converts a [`Stack`] range into a [`Buffers`] range.
    fn local_range<R: RangeBounds<usize>>(&self, range: R) -> core::ops::Range<usize> {
        let map = |i: usize| -> usize {
            self.local_index(i)
                .map_or(0, |index| index.min(self.buffered_nodes.len()))
        };

        let start = match range.start_bound() {
            Bound::Included(i) => map(*i),
            Bound::Excluded(i) => map(*i + 1),
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(i) => map(*i + 1),
            Bound::Excluded(i) => map(*i),
            Bound::Unbounded => self.buffered_nodes.len(),
        };

        start..end
    }

    /// Gets the parent sink for the buffered node at the given [`Stack`] index.
    fn parent(&mut self, index: usize) -> Either<&mut BufferedNode, &mut S> {
        index
            .checked_sub(1)
            .and_then(|index| self.local_index(index))
            .and_then(|index| self.buffered_nodes.get_mut(index))
            .map_or(Either::Right(&mut self.next), Either::Left)
    }

    /// Pops the last buffer.
    fn pop(&mut self) -> Option<BufferedNode> {
        self.buffered_nodes.pop()
    }

    /// Splits the buffer at `index` in half at the body.
    fn split_element(&mut self, index: usize) {
        let Some(index) = self.local_index(index) else {
            return;
        };
        let Some(victim) = self.buffered_nodes.get_mut(index) else {
            return;
        };
        let split_at = usize::from(victim.body_pos.unwrap().get());
        let buffer = victim.buffer.split_off(split_at);
        // TODO: This is so fucking stupid. Because the adoption agency
        // algorithm can leave the final formatting element open if the outer
        // loop limit is exceeded, it has to be extracted into a separate buffer
        // or else the stack and buffer lists are desynced.
        let node = BufferedNode {
            body_pos: Some(NonZeroU16::new(buffer.first_tag_len().try_into().unwrap()).unwrap()),
            buffer,
            contains_non_whitespace: victim.contains_non_whitespace,
            in_attr: false,
            tag_count: victim.tag_count,
        };
        victim.contains_non_whitespace = false;
        victim.tag_count = 0;
        self.buffered_nodes.insert(index + 1, node);
    }

    /// Returns the next output target.
    fn target(&mut self) -> Either<&mut BufferedNode, &mut S> {
        self.buffered_nodes
            .last_mut()
            .map_or(Either::Right(&mut self.next), Either::Left)
    }
}

/// The type used to store tag names that are not known to the algorithm.
type CustomTags = IndexSet<Uncased<'static>>;

/// An active formatting element.
#[derive(Clone, Copy, Debug)]
struct FormattingItem {
    /// The index into [`FormattingList::attributes`].
    attr_index: u16,
    /// The tag.
    node: TagNode,
}

/// The “list of active formatting elements”.
#[derive(Debug, Default)]
struct FormattingList {
    /// The buffer for active formatting elements’ attributes. Since most
    /// formatting elements have no attributes, this should be small and rarely
    /// allocated.
    attributes: String,
    /// If true, currently buffering the attributes of a formatting element.
    buffering: bool,
    /// The active formatting elements.
    elements: Vec<FormattingItem>,
    /// The index of the rightmost marker in [`Self::elements`].
    marker_index: Option<u8>,
}

impl FormattingList {
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

    /// Returns the raw attributes string for the formatting element with the
    /// given `index`.
    fn attributes(&self, index: usize) -> &str {
        &self.attributes[usize::from(self.elements[index].attr_index)..]
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
            // Truncating attributes here should be safe since reordering of
            // the formatting elements by adoption shouldn’t cross marker
            // boundaries, but the word ‘should’ is doing heavy lifting
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
    /// elements”, ignoring any marker barriers.
    #[inline]
    fn contains(&self, tag: Tag) -> bool {
        self.elements.iter().any(|node| node.node == tag)
    }

    /// Iterates over all formatting elements in the given `range`, returning
    /// the tag and the list of attributes.
    fn iter<R: core::slice::SliceIndex<[FormattingItem], Output = [FormattingItem]>>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (TagNode, impl Iterator<Item = (&str, &str)>)> {
        self.elements[range].iter().map(|node| {
            let attrs_iter = attrs_iter(&self.attributes[usize::from(node.attr_index)..]);
            (node.node, attrs_iter)
        })
    }

    /// Returns the number of active formatting elements.
    #[inline]
    fn len(&self) -> usize {
        self.elements.len()
    }

    /// Finds the position of the leftmost element matching the given
    /// `predicate`, stopping at the rightmost marker.
    fn lfind_scoped(&self, mut predicate: impl FnMut(&TagNode) -> bool) -> Option<usize> {
        let min = self.after_marker();
        self.elements[min..]
            .iter()
            .position(|node| predicate(&node.node))
            .map(|index| min + index)
    }

    /// Pushes a new tag to the “list of active formatting elements”, enabling
    /// attribute buffering.
    fn push(&mut self, tag: Tag) {
        // TODO: Just to be maximally difficult to implement efficiently, this
        // is supposed to also consider that the attributes, in any order, are
        // the same.
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

        self.elements.push(FormattingItem {
            attr_index: self.attributes.len().try_into().unwrap(),
            node: tag.into(),
        });
        self.buffering = true;
    }

    /// Pushes a marker to the “list of active formatting elements”.
    fn push_marker(&mut self) {
        let node = TagNode::Marker(next_index(&mut self.marker_index, self.elements.len()));
        self.elements.push(FormattingItem {
            attr_index: self.attributes.len().try_into().unwrap(),
            node,
        });
    }

    /// Removes a formatting item at the given `index`, correcting the marker
    /// pointer chain if needed.
    fn remove(&mut self, index: usize) {
        self.elements.remove(index);
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
        // It is not possible to truncate if this is the last element because
        // the adoption agency might reorder the list so the last element may
        // not point to the last item in the string. Flags could be added to
        // deal with this, but even in this tryhard world, that is more trouble
        // than it is worth
        if self.elements.is_empty() {
            self.attributes.clear();
        }
    }

    /// Finds the position of the rightmost element matching the given
    /// `predicate`, ignoring any marker barriers.
    #[inline]
    fn rfind_any(&self, predicate: impl Fn(&TagNode) -> bool) -> Option<usize> {
        self.elements.iter().rposition(|node| predicate(&node.node))
    }

    /// Finds the position of the rightmost element matching the given
    /// `predicate`, stopping at the rightmost marker.
    fn rfind_scoped(&self, mut predicate: impl FnMut(&TagNode) -> bool) -> Option<usize> {
        let min = self.after_marker();
        self.elements[min..]
            .iter()
            .rposition(|node| predicate(&node.node))
            .map(|index| min + index)
    }

    /// Moves `from` to `to`, shifting the intermediate elements to the left.
    fn rotate(&mut self, from: usize, to: usize) {
        self.elements[from..to].rotate_left(1);
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
            // Since we are tryharding to avoid allocation, the previous
            // elements now need to see an `END_OF_ATTRS` to avoid overreading
            #[expect(clippy::cast_possible_truncation, reason = "char can be only 4")]
            if self.attributes.is_empty() && self.elements.len() > 1 {
                self.attributes.push(Self::END_OF_ATTRS);
                self.elements.last_mut().unwrap().attr_index = Self::END_OF_ATTRS.len_utf8() as u16;
            }
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
            if !self.attributes.is_empty() {
                self.attributes.push(Self::END_OF_ATTRS);
            }
            self.buffering = false;
        }
    }
}

/// An HTML5 tree construction mode. Modes which are not salient to this
/// fragment parsing implementation are omitted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Mode {
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

impl Mode {
    /// Returns true if text must be fostered out of the table in this mode.
    fn foster_text(self) -> bool {
        matches!(self, Self::Row | Self::Table | Self::TableBody)
    }

    /// Returns true if formatting elements should be reconstructed around text
    /// in this mode.
    fn reformat_text(self) -> bool {
        matches!(self, Mode::Body | Mode::Caption | Mode::Cell)
    }
}

/// A newline filtering state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NewlineState {
    /// Emit newlines like normal.
    #[default]
    Idle,
    /// Filter out the next newline token.
    IgnoreNext,
    /// Oh god, unless there were two of them! Then put it back! Put it back
    /// right now! Undo! Undo!
    JustIgnored,
}

/// A “stack of open elements”.
#[derive(Debug)]
struct Stack<S> {
    /// If not `None`, content is being fostered out of a table.
    ///
    /// Whilst foster parenting changes where the content is *inserted*, the
    /// “stack of open elements” is not similarly reordered. This means that it
    /// should be impossible for there to be more than one fostering table at
    /// the same time, since a `<table>` in a fostering position will close the
    /// table to open a new table, so this only needs to be tracked once
    /// globally instead of per-table.
    fostering: Option<u8>,
    /// The stack of currently open nodes.
    inner: Vec<TagNode>,
    /// The output.
    next: Buffers<S>,
    /// The index of the rightmost `<table>` in [`Self::inner`].
    table_index: Option<u8>,
}

impl<S: Sink> Stack<S> {
    /// Creates a new `Stack` which emits to `next`.
    #[inline]
    fn new(next: S) -> Self {
        Self {
            fostering: <_>::default(),
            inner: <_>::default(),
            next: Buffers::new(next),
            table_index: <_>::default(),
        }
    }

    /// Emits the `buffer` (if `Some`) and close tag for `node` to the parent of
    /// the element with the given `index`, or to the next sink if `index` is
    /// `None`.
    fn close_tag(
        &mut self,
        custom_tags: &IndexSet<Uncased<'static>>,
        index: Option<usize>,
        node: TagNode,
        buffer: Option<BufferedNode>,
    ) {
        let mut target = if let Some(index) = index {
            self.next.parent(index)
        } else {
            self.next.target()
        };

        let mut emit_tag_end = true;
        if let Some(mut buffer) = buffer {
            if matches!(node, TagNode::ImplicitP) {
                if buffer.is_p_wrappable() {
                    target.tag_start_full("p");
                } else {
                    emit_tag_end = false;
                }
            }

            match &mut target {
                Either::Left(next) => next.extend(buffer),
                Either::Right(next) => buffer.flush_into(*next),
            }
        }

        if emit_tag_end && let Some(name) = node.name(custom_tags) {
            debug_assert!(!VOID_TAGS.contains(name.as_str()));
            target.tag_end(name.as_str());
            if let TagNode::Table(next) = node {
                self.table_index = next.map(|index| u8::from(index) - 1);
            }
        }

        self.dec_foster();
    }

    /// Returns true if the stack contains `node`.
    #[expect(clippy::trivially_copy_pass_by_ref, reason = "API consistency")]
    #[inline]
    fn contains(&self, node: &TagNode) -> bool {
        self.inner.contains(node)
    }

    /// Returns true if the stack contains `node` at or after `index`.
    #[expect(clippy::trivially_copy_pass_by_ref, reason = "API consistency")]
    #[inline]
    fn contains_after(&self, index: usize, node: &TagNode) -> bool {
        self.inner[index..].contains(node)
    }

    /// Decrements the foster parenting counter. At zero, foster parenting is
    /// disabled.
    #[inline]
    fn dec_foster(&mut self) {
        if let Some(depth) = &mut self.fostering {
            if *depth == 0 {
                self.fostering = None;
            } else {
                *depth -= 1;
            }
        }
    }

    /// Disables foster parenting if there is no active content fostering.
    ///
    /// This is needed in situations where fostering would occur if a tag was
    /// actually emitted, but no tag was emitted. Otherwise, the next tag will
    /// end up being treated as fostered content when that was not intended.
    #[inline]
    fn disable_fostering(&mut self) {
        if self.fostering == Some(0) {
            self.fostering = None;
        }
    }

    /// Enables fostering parenting. Once enabled, fostering continues until the
    /// counter reaches zero.
    #[inline]
    fn enable_fostering(&mut self) {
        self.fostering.get_or_insert_default();
    }

    /// Finishes processing input.
    #[inline]
    fn finish(mut self, custom_tags: &CustomTags) -> String {
        self.pop_range(custom_tags, ..);
        self.next.next.finish()
    }

    /// Flushes pending text from the “in table text” mode to the appropriate
    /// sink.
    #[inline]
    fn flush_text(&mut self, text: &str) {
        let has_content = text.contains(|c: char| !c.is_ascii_whitespace());
        if has_content {
            self.parent(self.table_index.unwrap()).text(text);
        } else {
            self.next.target().text(text);
        }
    }

    /// Returns true if the stack is in a “in table text” state.
    #[inline]
    fn in_table_text(&self) -> bool {
        self.inner
            .last()
            .is_some_and(|node| node.is_table_fosterable())
    }

    /// Increments the foster parenting counter.
    #[inline]
    fn inc_foster(&mut self) {
        if let Some(depth) = &mut self.fostering {
            *depth += 1;
        }
    }

    /// Returns the index of an element matching the given `predicate` on the
    /// stack of open elements in the scope given by `scope`, or `None` if there
    /// is no such element.
    fn index_in_scope(
        &self,
        mut predicate: impl FnMut(&TagNode) -> bool,
        mut scope: impl FnMut(Tag) -> bool,
    ) -> Option<usize> {
        for (index, node) in self.inner.iter().enumerate().rev() {
            #[rustfmt::skip]
            if predicate(node) {
                return Some(index);
            } else if let Some(tag) = node.tag() && scope(tag) {
                break;
            };
        }
        None
    }

    /// Inserts a node with the given `tag` and `attrs` at the given stack
    /// `index`.
    fn insert<'a>(
        &mut self,
        custom_tags: &CustomTags,
        index: usize,
        tag: TagNode,
        attrs: impl Iterator<Item = (&'a str, &'a str)>,
    ) {
        let buffered = BufferedNode::new(tag.name(custom_tags).unwrap().as_str(), attrs);
        self.next.insert(index, buffered);
        self.inner.insert(index, tag);
    }

    /// Returns the length of the stack, in elements.
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns the index of the next non-p-wrappable element in the stack.
    fn next_non_pwrap(&self, index: usize) -> usize {
        let start = index + 1;
        self.next.get_range(start..)
            .iter()
            .rposition(BufferedNode::is_p_wrappable)
            .map_or(start, |index| start + index + 1)
    }

    /// Returns the index of the next “special” category element after `start`.
    fn next_special(&self, start: usize) -> Option<usize> {
        let start = start + 1;
        let stack = self.inner.get(start..)?;
        stack
            .iter()
            .position(|node| node.is_special())
            .map(|index| start + index)
    }

    /// Gets the parent sink for the buffered node at the given [`Self::stack`]
    /// index.
    #[inline]
    fn parent(&mut self, index: u8) -> Either<&mut BufferedNode, &mut S> {
        self.next.parent(index.into())
    }

    /// Pops the top element from the “stack of open elements”, flushes the
    /// corresponding [`BufferedNode`], if any, to the next output, and emits
    /// an end tag to the next output.
    fn pop(&mut self, custom_tags: &CustomTags) {
        let Some(e) = self.inner.pop() else {
            return;
        };

        let buffer = self.next.pop();
        self.close_tag(
            custom_tags,
            self.fostering
                .is_some()
                .then(|| self.table_index.unwrap().into()),
            e,
            buffer,
        );
    }

    /// [Pops](Self::pop) the top element and returns true if the top element
    /// matches the given `predicate`.
    fn pop_if(
        &mut self,
        custom_tags: &CustomTags,
        predicate: impl FnOnce(&mut TagNode) -> bool,
    ) -> bool {
        if self.inner.last_mut().is_some_and(predicate) {
            self.pop(custom_tags);
            true
        } else {
            false
        }
    }

    /// [Pops](Self::pop) the given `range` of elements from the “stack of open
    /// elements”.
    fn pop_range<R: RangeBounds<usize>>(&mut self, custom_tags: &CustomTags, range: R) {
        let start = match range.start_bound() {
            Bound::Included(i) => *i,
            Bound::Excluded(i) => *i + 1,
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(i) => *i + 1,
            Bound::Excluded(i) => *i,
            Bound::Unbounded => self.inner.len(),
        };

        // This loop is unpleasant so that the entire ranges of stack and buffer
        // are deleted at once, instead of one entry at a time, and because it
        // is necessary to access the buffer list disjoint mutable to fold a
        // buffer into its parent. (TODO: This latter thing would not need to be
        // a thing if the buffer were a single allocation, since then, folding
        // is just a matter of shifting some indexes forward!)
        let mut index = end;
        while index != start {
            index -= 1;
            let e = self.inner[index];
            let buffer = self.next.get_mut(index).map(core::mem::take);
            let index = if self.fostering.is_some() {
                self.table_index.map(usize::from)
            } else {
                Some(index)
            };
            self.close_tag(custom_tags, index, e, buffer);
        }

        self.inner.drain(start..end);
        self.next.drain(start..end);
    }

    /// Pushes a new element with the given `tag` name to the “stack of open
    /// elements”, additionally pushing a new buffer if needed.
    fn push(&mut self, tag: Tag) {
        if !self.next.is_empty() || tag.is_formatting() {
            self.next.insert(self.inner.len(), <_>::default());
        }
        self.inner.push(tag.into());
        if tag.is_table_item() {
            self.fostering = None;
        } else {
            self.inc_foster();
        }
    }

    /// Pushes a new implicit `<p>` to the stack.
    fn push_pwrap(&mut self) {
        if self
            .inner
            .last()
            .is_none_or(|node| *node == Tag::Blockquote)
        {
            self.next.insert(self.inner.len(), <_>::default());
            self.inner.push(TagNode::ImplicitP);
        }
    }

    /// Pushes a new implicit `<tbody>` to the stack.
    #[inline]
    fn push_tbody(&mut self) {
        self.next.insert(self.inner.len(), <_>::default());
        self.inner.push(TagNode::ImplicitTbody);
        self.fostering = None;
    }

    /// Pushes a `<table>` element to the “stack of open elements”.
    fn push_table(&mut self) {
        self.next.insert(self.inner.len(), <_>::default());
        let node = TagNode::Table(next_index(&mut self.table_index, self.inner.len()));
        self.inner.push(node);
        self.fostering = None;
    }

    /// Searches for an element in the stack matching `predicate`, starting from
    /// the right.
    #[inline]
    fn rfind(&self, predicate: impl FnMut(&&TagNode) -> bool) -> Option<&TagNode> {
        self.inner.iter().rfind(predicate)
    }

    /// Applies `f` to each element in the stack starting from the right,
    /// returning the first `Some`.
    #[inline]
    fn rfind_map<T>(&self, f: impl FnMut((usize, &TagNode)) -> Option<T>) -> Option<T> {
        self.inner.iter().enumerate().rev().find_map(f)
    }

    /// Returns an iterator of “special” category element indexes and buffers at
    /// or after `start`.
    fn specials(&mut self, start: usize) -> impl Iterator<Item = (usize, &mut BufferedNode)> {
        self.inner[start..]
            .iter()
            .enumerate()
            .map(move |(index, tag)| (index + start, tag))
            .zip(self.next.get_range_mut(start..))
            .filter_map(|((index, tag), node)| tag.is_special().then_some((index, node)))
    }

    /// Splits the node at `index` in half, adding a `tag` to the stack at the
    /// split point.
    fn split_element(&mut self, index: usize, tag: Tag) {
        self.next.split_element(index);
        self.inner.insert(index + 1, tag.into());
        self.inc_foster();
    }

    /// Returns the next output [`Sink`].
    fn target(&mut self) -> Either<&mut BufferedNode, &mut S> {
        if self.fostering.is_some() {
            self.parent(self.table_index.unwrap())
        } else {
            self.next.target()
        }
    }
}

/// Generates the `Tag` enum and lookup table for known HTML5 tag names.
macro_rules! tags {
    ($($tag:literal => $id:ident),* $(,)?) => {
        /// An HTML tag.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Tag {
            $(#[doc = concat!("The `<", $tag, ">` element.")] $id,)*
            /// A custom tag index.
            Custom(u8),
        }

        /// The lookup table for known HTML tags.
        static KNOWN_TAGS: phf::Map<&UncasedStr, Tag> = phf::phf_map! {
            $(UncasedStr::new($tag) => Tag::$id,)*
        };

        impl Tag {
            /// Returns the tag as a string.
            fn as_str(self, custom: &CustomTags) -> &UncasedStr {
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
// extension tags, uh, more or less.
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
    fn new(name: &str, custom: &mut CustomTags) -> Self {
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
        // Ignoring article, dialog, dir, fieldset, footer, header, hgroup,
        // main, menu, nav, search, section
        matches!(
            self,
            Self::Address
                | Self::Aside
                | Self::Blockquote
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

/// A DOM pseudo-node.
#[derive(Clone, Copy, Debug, Eq)]
enum TagNode {
    /// An HTML tag.
    Html(Tag),
    /// An implicit `<tbody>` that is not being emitted because it is a waste.
    ImplicitTbody,
    /// A marker on the “list of active formatting elements” that holds a
    /// niche-optimised index of the previous marker in [`DomTree::format`], if
    /// any.
    Marker(Option<NonZeroU8>),
    /// An implicit p-wrapper.
    ImplicitP,
    /// An optimised `<table>` element that holds a niche-optimised index of the
    /// previous `<table>` element in [`DomTree::stack`], if any.
    Table(Option<NonZeroU8>),
}

impl From<Tag> for TagNode {
    #[inline]
    fn from(tag: Tag) -> Self {
        debug_assert!(tag != Tag::Table, "specialised tag kind");
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
    /// Returns the tag name of this node, or `None` if this is an anonymous
    /// marker node.
    fn name(self, custom: &CustomTags) -> Option<&UncasedStr> {
        match self {
            Self::Html(tag) => Some(tag.as_str(custom)),
            Self::ImplicitTbody | Self::Marker(_) => None,
            Self::ImplicitP => Some(UncasedStr::new("p")),
            Self::Table(_) => Some(UncasedStr::new("table")),
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

    /// Returns true if this is an implicit `<p>` (a Wikitext p-wrapper).
    #[inline]
    fn is_implicit_p(self) -> bool {
        matches!(self, Self::ImplicitP)
    }

    /// Returns true if this is a “special” category node.
    #[inline]
    fn is_special(self) -> bool {
        self.tag().is_some_and(Tag::is_special)
    }

    /// Returns true if this is a `<table>` direct child.
    #[inline]
    fn is_table_body(self) -> bool {
        self.tag().is_some_and(Tag::is_table_body)
    }

    /// Returns true if this is a `<table>` element that cannot contain most
    /// non-table content.
    #[inline]
    fn is_table_fosterable(self) -> bool {
        self.tag().is_some_and(Tag::is_table_fosterable)
    }

    /// Returns the corresponding HTML5 tag for this node, or `None` if this is
    /// an anonymous marker node.
    fn tag(self) -> Option<Tag> {
        match self {
            Self::Html(tag) => Some(tag),
            Self::ImplicitTbody => Some(Tag::Tbody),
            Self::ImplicitP => Some(Tag::P),
            Self::Marker(_) => None,
            Self::Table(_) => Some(Tag::Table),
        }
    }
}

impl PartialEq for TagNode {
    fn eq(&self, other: &Self) -> bool {
        self.tag() == other.tag()
    }
}

/// Discard the tag instead of emitting it.
const DISCARD: bool = false;

/// Emit the tag to the next sink.
const EMIT: bool = true;

/// Parses a raw formatting element attributes string into an iterator of
/// key-value pairs.
fn attrs_iter(mut attrs: &str) -> impl Iterator<Item = (&str, &str)> {
    core::iter::from_fn(move || {
        if attrs.is_empty() || attrs.starts_with(FormattingList::END_OF_ATTRS) {
            None
        } else {
            let (name, value) = attrs.split_once(FormattingList::END_OF_NAME).unwrap();
            let (value, rest) = value.split_once(FormattingList::END_OF_ATTR).unwrap();
            attrs = rest;
            Some((name, value))
        }
    })
}

/// Emits the given `tag` with attributes `attrs` to `next`, using `custom_tags`
/// to resolve tag names that are not known HTML5 tags.
fn emit_tag<'a, S: Sink + ?Sized>(
    next: &mut S,
    name: &str,
    attrs: impl Iterator<Item = (&'a str, &'a str)>,
) {
    next.tag_start(name);
    for (name, value) in attrs {
        next.tag_attribute_full(name, value);
    }
    next.tag_start_end(name);
}

/// Takes a value from `index`, returning a niche-optimised `Option<NonZeroU8>`.
fn next_index(index: &mut Option<u8>, next: usize) -> Option<NonZeroU8> {
    index
        .replace(next.try_into().unwrap())
        .and_then(|n| NonZeroU8::new(n + 1))
}
