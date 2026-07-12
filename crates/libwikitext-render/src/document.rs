//! The root of a Wikitext document.

use super::{
    Error, LinkKind, Result, State, StripMarker,
    emitters::{ListEmitter, TableState, TextStyleEmitter},
    extension_tags,
    globals::Outline,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    tags::{self, ExternalLinkKind},
    transform::{
        Accumulator, AttributeFilter, Chain as _, DomTree, EmptyMarker, GrafWrapper,
        OutlineGenerator, PrettyText, Sink, TemplateMarker,
    },
};
use either::Either;
use libmisc::{CowExt as _, to_ascii_lower};
use libphp_rs::strtr;
use libwikitext_common::{
    format_message,
    title::{Namespace, Title},
    title_decode,
};
use libwikitext_parse::{
    AnnoAttribute, Argument, FileMap, HeadingLevel, InclusionMode, LangFlags, LangVariant,
    MARKER_PREFIX, MARKER_SUFFIX, MagicLink, Output, Span, Spanned, TextStyle, Token,
};
use std::borrow::Cow;

/// The root of a Wikitext document.
#[derive(Debug)]
pub(crate) struct Document<S> {
    /// The stack of inclusion control tags.
    in_include: Vec<InclusionMode>,
    /// If true, inside a `<pre>` tag.
    in_pre: bool,
    /// The output sink.
    pub(super) next: S,
    /// The list emitter.
    list_emitter: Option<ListEmitter>,
    /// The Wikitext table emitters.
    table_emitter: Vec<TableState>,
    /// The [`TextStyle`] emitters.
    text_style_emitter: Vec<TextStyleEmitter>,
}

impl<S> Document<S>
where
    S: DocumentSink,
{
    /// Creates a new [`Document`].
    pub(crate) fn new(args: S::Args) -> Self {
        Self {
            in_include: <_>::default(),
            in_pre: <_>::default(),
            next: S::new(args),
            list_emitter: <_>::default(),
            table_emitter: <_>::default(),
            text_style_emitter: vec![TextStyleEmitter::default()],
        }
    }

    /// Transforms and writes an attribute.
    fn attribute(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        attribute: &Spanned<Argument>,
    ) -> Result {
        let name = attribute
            .name()
            .map(|name| sp.eval(state, name))
            .expect("k-v")?;
        let name = to_ascii_lower(name.trim_ascii());
        let value = attribute.value();
        self.next.tag_attribute_start(&name);
        // TODO: This should probably use its own visitor?
        for token in value {
            match token.node {
                Token::NewLine => self.next.new_line(),
                _ => self.adopt_token(state, sp, token)?,
            }
        }
        self.next.tag_attribute_end(&name);
        Ok(())
    }

    /// Finalises the document and returns the resulting output.
    pub(crate) fn finish(mut self) -> String {
        self.text_style_emitter
            .last_mut()
            .unwrap()
            .finish(&mut self.next);

        if let Some(mut list) = self.list_emitter {
            list.finish(&mut self.next);
        }

        for table in self.table_emitter.into_iter().rev() {
            table.finish(&mut self.next, true);
        }

        let result = self.next.finish();
        // God, this is so fucking stupid
        if result == "<table>\n<tr><td></td></tr>\n</table>" {
            <_>::default()
        } else {
            result
        }
    }

    /// Flushes the whitespace from the table end trim buffer to the next
    /// output.
    ///
    /// Because this step ran first in the original parser, inline items that
    /// emit nothing, like behaviour switches and category wikilinks, still
    /// cause whitespace to be emitted.
    fn flush_after_table(&mut self) {
        if let Some(table) = self.table_emitter.last_mut() {
            table.flush_after_table(&mut self.next);
        }
    }

    /// Handles a run of plain text that may need to be right-trimmed after a
    /// table end.
    // TODO: This sucks and seems like it ought to be handled in the grammar
    // by emitting some ambiguous token at the end of the table end line instead
    // of having to dump flushes all over the place.
    fn text_run(&mut self, text: &str) {
        if let Some(table) = self.table_emitter.last_mut() {
            table.after_table_text(&mut self.next, text);
        } else {
            self.next.text(text);
        }
    }

    /// Writes an HTML start tag with the given lowercase `name` and bag of
    /// crap `attributes`.
    fn write_start_tag(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        name: &str,
        attributes: &str,
    ) -> Result {
        self.next.tag_start(name);
        if !attributes.is_empty() {
            let sp = sp.clone_with_source(FileMap::new(attributes));
            let attributes = state.statics.parser.parse_attributes(&sp.source)?;
            for attribute in attributes {
                self.attribute(state, &sp, &attribute)?;
            }
        }
        self.next.tag_start_end(name);
        Ok(())
    }

    /// Writes a Wikitext table caption, header, or data cell.
    fn write_table_data(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &'static str,
        attributes: &[Spanned<Token>],
    ) -> Result {
        if let Some(table) = self.table_emitter.last_mut() {
            if let Some(last) = table.last_tag.replace(name) {
                // Because the original parser split table cells across lines,
                // the text styles also must reset every line
                self.text_style_emitter
                    .last_mut()
                    .unwrap()
                    .finish(&mut self.next);
                self.next.tag_end(last);
                self.next.new_line();
            }
            if name != "caption" {
                let attrs = core::mem::take(&mut table.tr_attrs);
                table.has_tbody = true;
                if !table.tr_emitted {
                    table.tr_emitted = true;
                    self.write_start_tag(state, sp, "tr", &attrs)?;
                    self.next.new_line();
                }
            }
            self.adopt_start_tag(state, sp, span, name, attributes, false)?;
        } else {
            let end = attributes.first().map_or(span.end, |a| a.span.start);
            self.next
                .text(&sp.source[span.start as usize..end as usize]);
            self.adopt_tokens(state, sp, attributes)?;
            if let Some(last) = attributes.last() {
                self.next
                    .text(&sp.source[last.span.end as usize..span.end as usize]);
            }
        }
        Ok(())
    }
}

impl<S> Surrogate<Error> for Document<S>
where
    S: DocumentSink,
{
    fn adopt_autolink(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
    ) -> Result {
        self.flush_after_table();
        tags::render_external_link(self, state, sp, target, &[], true)
    }

    fn adopt_behavior_switch(
        &mut self,
        state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
    ) -> Result {
        self.flush_after_table();
        match name {
            "hiddencat" if state.globals.title.namespace().id == Namespace::CATEGORY => {
                state
                    .globals
                    .categories
                    .tracking(&state.statics.messages, "hidden-category-category")?;
            }
            _ => log::warn!("TODO: BehaviorSwitch __{name}__"),
        }
        Ok(())
    }

    fn adopt_comment(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        content: &str,
        _unclosed: bool,
    ) -> Result {
        self.flush_after_table();
        self.next.comment_start();
        self.next.text(content);
        self.next.comment_end();
        Ok(())
    }

    fn adopt_end_annotation(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
    ) -> Result {
        log::warn!("TODO: EndAnnotation: {name}");
        Ok(())
    }

    fn adopt_end_include(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.in_include
            .pop_if(|expected| *expected == mode)
            .expect("balanced includes");
        Ok(())
    }

    fn adopt_end_tag(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
    ) -> Result {
        self.flush_after_table();
        let name = to_ascii_lower(name);
        self.in_pre &= name != "pre";
        self.next.tag_end(&name);
        Ok(())
    }

    fn adopt_entity(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        value: char,
    ) -> Result {
        self.flush_after_table();
        self.next.entity(value, &sp.source[span.into_range()]);
        Ok(())
    }

    fn adopt_extension(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        content: Option<&str>,
    ) -> Result {
        self.flush_after_table();
        // TODO: This is all outrageously hacky.
        let name = to_ascii_lower(name);
        assert_eq!(name, "wiki-rs-cached");

        let id = sp
            .eval(state, attributes[0].value())
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let marker = sp.eval(state, attributes[1].value()).unwrap();

        if state.vm_request_cache.contains(&id) {
            if S::UNSTRIP_MARKERS {
                let marker = state.strip_markers.get(&marker).unwrap();
                let marker = marker.map_ref(|s| state.strip_markers.unstrip_all(s));
                self.next.strip_marker(&marker);
            } else {
                self.next.text(MARKER_PREFIX);
                self.next.text(&marker);
                self.next.text(MARKER_SUFFIX);
            }
        } else {
            let source = content.unwrap();
            let sp = sp.clone_with_source(FileMap::new(source));
            let tree = state.statics.parser.preprocess(source, false)?;
            #[rustfmt::skip]
            let [ Spanned { span, node: Token::Extension {
                attributes, content, name,
            } } ] = tree.root.as_slice() else {
                panic!("should have been a single extension tag");
            };
            let name = &source[name.into_range()];
            let content = content.map(|span| &source[span.into_range()]);
            match extension_tags::render_extension_tag(
                state,
                &sp,
                Some(*span),
                name,
                &extension_tags::InArgs::Wikitext(attributes),
                content,
            )? {
                Some(Either::Left(marker)) => {
                    if S::UNSTRIP_MARKERS {
                        let marker = marker.map_ref(|s| state.strip_markers.unstrip_all(s));
                        self.next.strip_marker(&marker);
                    } else {
                        state.vm_request_cache.insert(id);
                        let id = &mut String::new();
                        state.strip_markers.push(id, name, marker);
                        self.next.text(id);
                    }
                }
                Some(Either::Right(_)) => todo!("this should never happen?"),
                None => {}
            }
        }

        Ok(())
    }

    fn adopt_external_link(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.flush_after_table();
        tags::render_external_link(self, state, sp, target, content, false)
    }

    fn adopt_generated(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Option<Span>,
        text: &str,
    ) -> Result {
        self.text_run(text);
        Ok(())
    }

    fn adopt_heading(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result {
        self.next.tag_start_full(level.tag_name());
        self.adopt_tokens(state, sp, content)?;
        self.next.tag_end(level.tag_name());
        Ok(())
    }

    fn adopt_horizontal_rule(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        self.next.tag_start_full("hr");
        Ok(())
    }

    fn adopt_lang_variant(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _flags: &LangFlags,
        _variants: &[LangVariant],
    ) -> Result {
        self.flush_after_table();
        // TODO: Implement language conversion.
        log::warn!("TODO: language conversion");
        self.next.text(&sp.source[span.into_range()]);
        Ok(())
    }

    fn adopt_link(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        prefix: &[Spanned<Token>],
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: &[Spanned<Token>],
    ) -> Result {
        self.flush_after_table();
        let title = sp.eval(state, target)?.map(title_decode);
        let title = title.trim_start_matches(' ');
        let force_link = title.starts_with(':');
        let (title, mut text) = state.globals.title.join(title);
        if text.is_empty() {
            text = Cow::Borrowed(title.as_ref());
        }
        let Ok(title) = Title::new(state.statics.db.config(), &title, None) else {
            // It is not possible to just emit the original span because it may
            // contain entities that must not be double-encoded
            self.adopt_tokens(state, sp, prefix)?;
            self.next.text("[[");
            self.adopt_tokens(state, sp, target)?;
            for content in content {
                self.next.text("|");
                self.adopt_tokens(state, sp, &content.content)?;
            }
            self.next.text("]]");
            self.adopt_tokens(state, sp, trail)?;
            return Ok(());
        };
        if !force_link
            && title.is_category(
                state.statics.db.config(),
                state.globals.title.namespace().is_talk(),
            )
        {
            // Normally the corresponding content-part is supposed to be used as
            // a sort key. However, since this implementation does not have any
            // category pages, and the sort key does not change the sort order
            // of the category list at the end of the page, the content-part of
            // a category is simply ignored.
            state.globals.categories.insert(&title);
            self.adopt_tokens(state, sp, prefix)?;
            self.adopt_tokens(state, sp, trail)?;
        } else if !force_link && title.is_local_file() {
            self.adopt_tokens(state, sp, prefix)?;
            self.next.set_in_caption(true);
            super::image::render_media(self, state, sp, title, content)?;
            self.next.set_in_caption(false);
            self.adopt_tokens(state, sp, trail)?;
        } else {
            self.text_style_emitter.push(<_>::default());
            tags::render_internal_link(self, state, sp, &text, prefix, content, trail, title)?;
            self.text_style_emitter
                .pop()
                .unwrap()
                .finish(&mut self.next);
        }
        Ok(())
    }

    fn adopt_inline_list_item(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        // An inline list item that gets to be adopted by the document is one
        // which did not ultimately have any associated term
        self.next.text(&sp.source[span.into_range()]);
        Ok(())
    }

    fn adopt_list_item(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result {
        fn split_term_once(
            content: &[Spanned<Token>],
        ) -> (&[Spanned<Token>], Option<&[Spanned<Token>]>) {
            let term = content
                .iter()
                .position(|t| matches!(t.node, Token::InlineListItem));
            term.map_or((content, None), |pos| {
                (&content[..pos], Some(&content[pos + 1..]))
            })
        }

        // TODO: The `in_pre` condition needs to be emitting all the tokens
        // instead of just taking the line as text since tags and entities are
        // actually supposed to be treated like tags and entities 🥴️
        if self.in_pre {
            self.next.text(&sp.source[span.into_range()]);
            return Ok(());
        }

        let mut content = if let Some((last, content)) = content.split_last()
            && matches!(last.node, Token::NewLine)
        {
            content
        } else {
            content
        };

        // Taking the list from `self` is required to avoid borrowck errors.
        // This is fine, since it is impossible for nested lists to exist. The
        // only tokens that continue over a newline are image captions, and they
        // defeat the block level algorithm by deleting newlines. The Wikitext
        // table indent hack is something that happens on the table pass, and so
        // also disappears before the block level algorithm runs. The block
        // level algorithm implementation itself does not hold a stack which
        // would allow for nesting.
        let mut list = if let Some(list) = self.list_emitter.take() {
            list
        } else {
            self.next.set_in_list(true);
            <_>::default()
        };

        if list.same(bullets) {
            list.emit_last(&mut self.next, bullets);
            if bullets.as_bytes()[bullets.len() - 1] == b';' {
                let (term, detail) = split_term_once(content);
                if let Some(detail) = detail {
                    self.adopt_tokens(state, sp, term)?;
                    list.emit_last(&mut self.next, ":");
                    content = detail;
                }
            }
        } else {
            let common_end = list.emit_common(&mut self.next, bullets);
            for item in &bullets.as_bytes()[common_end..] {
                list.push(&mut self.next, *item);
                if *item == b';' {
                    let (term, detail) = split_term_once(content);
                    if let Some(detail) = detail {
                        self.adopt_tokens(state, sp, term)?;
                        list.emit_last(&mut self.next, ":");
                        content = detail;
                    }
                }
            }
        }

        self.adopt_tokens(state, sp, content)?;

        self.text_style_emitter
            .last_mut()
            .unwrap()
            .finish(&mut self.next);

        self.list_emitter = Some(list);
        Ok(())
    }

    fn adopt_magic_link(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        magic: &MagicLink,
    ) -> Result {
        self.flush_after_table();

        let (link, content, tracking) = match magic {
            MagicLink::Isbn(id) => {
                const BOOKSOURCES: &str = "Booksources";
                let url_id = strtr(id, &[("-", ""), (" ", ""), ("x", "X")]);
                let canonical = state
                    .statics
                    .db
                    .config()
                    .special_pages
                    .canonical
                    .get(BOOKSOURCES)
                    .copied()
                    .unwrap_or(BOOKSOURCES);
                let link = LinkKind::Internal(Title::new(
                    state.statics.db.config(),
                    &format!("{canonical}/{url_id}"),
                    Some(Namespace::SPECIAL),
                )?);
                (link, format!("ISBN {id}"), "magiclink-tracking-isbn")
            }
            MagicLink::Pmid(id) => {
                let url =
                    format_message(&state.statics.messages, None, true, ["pubmedurl"], |key| {
                        Ok::<_, Error>(
                            (key == "1").then_some(Cow::Borrowed(&sp.source[id.into_range()])),
                        )
                    })?;
                let link = LinkKind::External(url, ExternalLinkKind::MagicPmid);
                (
                    link,
                    format!("PMID {}", &sp.source[id.into_range()]),
                    "magiclink-tracking-pmid",
                )
            }
            MagicLink::Rfc(id) => {
                let url = format_message(&state.statics.messages, None, true, ["rfcurl"], |key| {
                    Ok::<_, Error>(
                        (key == "1").then_some(Cow::Borrowed(&sp.source[id.into_range()])),
                    )
                })?;
                let link = LinkKind::External(url, ExternalLinkKind::MagicRfc);
                (
                    link,
                    format!("RFC {}", &sp.source[id.into_range()]),
                    "magiclink-tracking-rfc",
                )
            }
        };

        state
            .globals
            .categories
            .tracking(&state.statics.messages, tracking)?;

        tags::render_start_link(&mut self.next, state, &link);
        self.next.text(&content);
        self.next.tag_end("a");
        Ok(())
    }

    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        self.text_style_emitter
            .last_mut()
            .unwrap()
            .finish(&mut self.next);
        if let Some(mut list) = self.list_emitter.take() {
            list.finish(&mut self.next);
            self.next.new_line();
            self.next.set_in_list(false);
        } else {
            if let Some(table) = self
                .table_emitter
                .pop_if(|table| table.after_table.is_some())
            {
                table.finish(&mut self.next, false);
            }
            self.next.new_line();
        }
        Ok(())
    }

    fn adopt_output(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        output: &Output,
    ) -> Result {
        if output.has_onlyinclude {
            self.in_include.push(InclusionMode::OnlyInclude);
        }
        let result = self.adopt_tokens(state, sp, &output.root);
        if output.has_onlyinclude {
            self.in_include
                .pop_if(|i| *i == InclusionMode::OnlyInclude)
                .expect("include stack corruption");
        }
        result
    }

    fn adopt_parameter(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &[Spanned<Token>],
        _default: Option<&[Spanned<Token>]>,
    ) -> Result {
        panic!("templates should all be resolved by now");
    }

    fn adopt_redirect(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        prefix: &[Spanned<Token>],
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: &[Spanned<Token>],
    ) -> Result {
        self.next.tag_start("p");
        self.next.tag_attribute_start("class");
        self.next.text("redirectText");
        self.next.tag_attribute_end("class");
        self.next.tag_start_end("p");
        self.adopt_link(state, sp, span, prefix, target, content, trail)?;
        self.next.tag_end("p");
        Ok(())
    }

    fn adopt_start_annotation(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
        _attributes: &[Spanned<AnnoAttribute>],
    ) -> Result {
        log::warn!("TODO: StartAnnotation {name}");
        Ok(())
    }

    fn adopt_start_include(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.in_include.push(mode);
        Ok(())
    }

    fn adopt_start_tag(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
        attributes: &[Spanned<Token>],
        _self_closing: bool,
    ) -> Result {
        self.flush_after_table();
        let name = to_ascii_lower(name);
        self.in_pre |= name == "pre";
        let attributes = sp
            .eval(state, attributes)?
            .map(|out| state.strip_markers.unstrip_all(out));
        self.write_start_tag(state, sp, &name, &attributes)
    }

    fn adopt_strip_marker(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        marker: &str,
    ) -> Result {
        self.flush_after_table();
        if S::UNSTRIP_MARKERS {
            if let Some(marker) = state.strip_markers.get(marker) {
                let marker = marker.map_ref(|s| state.strip_markers.unstrip_all(s));
                self.next.strip_marker(&marker);
                Ok(())
            } else {
                Err(Error::StripMarker(marker.to_owned()))
            }
        } else {
            self.next.text(&sp.source[span.into_range()]);
            Ok(())
        }
    }

    #[inline]
    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Token>],
    ) -> Result {
        self.write_table_data(state, sp, span, "caption", attributes)
    }

    #[inline]
    fn adopt_table_data(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Token>],
    ) -> Result {
        self.write_table_data(state, sp, span, "td", attributes)
    }

    fn adopt_table_end(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        if let Some(table) = self.table_emitter.last_mut() {
            table.table_end(&mut self.next, false);
        } else {
            self.next.text(&sp.source[span.into_range()]);
        }
        Ok(())
    }

    #[inline]
    fn adopt_table_header(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Token>],
    ) -> Result {
        self.write_table_data(state, sp, span, "th", attributes)
    }

    fn adopt_table_row(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Token>],
    ) -> Result {
        if let Some(table) = self.table_emitter.last_mut() {
            let attributes = sp
                .eval(state, attributes)?
                .map(|out| state.strip_markers.unstrip_all(out));
            table.has_tbody = true;
            table.tr_attrs = attributes.into_owned();
            if let Some(name) = table.last_tag.take() {
                self.next.tag_end(name);
            }
            if core::mem::take(&mut table.tr_emitted) {
                self.next.tag_end("tr");
            }
        } else {
            let end = attributes.first().map_or(span.end, |a| a.span.start);
            self.next
                .text(&sp.source[span.start as usize..end as usize]);
            self.adopt_tokens(state, sp, attributes)?;
        }
        Ok(())
    }

    fn adopt_table_start(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Token>],
        indent: u8,
    ) -> Result {
        for _ in 0..indent {
            self.next.tag_start_full("dl");
            self.next.tag_start_full("dd");
        }
        self.adopt_start_tag(state, sp, span, "table", attributes, false)?;
        self.table_emitter.push(TableState::new(indent));
        Ok(())
    }

    fn adopt_template(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _target: &[Spanned<Token>],
        _arguments: &[Spanned<Argument>],
    ) -> Result {
        panic!("templates should all be resolved by now");
    }

    fn adopt_text(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        text: &str,
    ) -> Result {
        self.text_run(text);
        Ok(())
    }

    fn adopt_text_style(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        style: TextStyle,
    ) -> Result {
        self.flush_after_table();
        self.text_style_emitter
            .last_mut()
            .unwrap()
            .emit(&mut self.next, style);
        Ok(())
    }

    fn adopt_token(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        token: &Spanned<Token>,
    ) -> Result {
        if !matches!(token.node, Token::StartInclude(..) | Token::EndInclude(..))
            && let Some(InclusionMode::IncludeOnly) = self.in_include.last()
        {
            log::debug!("skipping includeonly");
            return Ok(());
        }

        surrogate::adopt_token(self, state, sp, token).map_err(|err| Error::Node {
            frame: sp.name.to_string(),
            start: sp.source.find_line_col(token.span.start),
            err: Box::new(err),
        })
    }
}

/// A [`Sink`] supertrait used by [`Document`].
pub(super) trait DocumentSink: Sink {
    /// If true, the sink will process strip markers.
    const UNSTRIP_MARKERS: bool;

    /// Arguments to [`Self::new`].
    type Args;

    /// Creates a new `DocumentSink` with the given `args`.
    fn new(args: Self::Args) -> Self
    where
        Self: Sized;

    /// Enables or disables in-caption processing mode.
    fn set_in_caption(&mut self, in_caption: bool);
    /// Enables or disables in-list processing mode.
    fn set_in_list(&mut self, in_list: bool);
}

/// A [`DocumentSink`] for rendering a “half parsed” Wikitext document.
/// This is equivalent to `Parser::recursiveTagParse`.
pub(super) struct ParseHalf<'a>(AttributeFilter<OutlineGenerator<'a, Accumulator>>);
impl<'a> DocumentSink for ParseHalf<'a> {
    const UNSTRIP_MARKERS: bool = false;

    type Args = &'a mut Outline;

    #[inline]
    fn new(args: Self::Args) -> Self
    where
        Self: Sized,
    {
        Self(AttributeFilter::new(OutlineGenerator::new(
            args,
            Accumulator::new(),
        )))
    }

    #[inline]
    fn set_in_caption(&mut self, _: bool) {}

    #[inline]
    fn set_in_list(&mut self, _: bool) {}
}

impl Sink for ParseHalf<'_> {
    #[inline]
    fn comment_end(&mut self) {
        self.0.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.0.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.0.entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        self.0.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.0.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        self.0.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.0.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.0.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.0.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.0.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.0.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.0.text(text);
    }
}

/// The chain of transformers used to fully parse a document.
type ParseFullyChain<'a> = AttributeFilter<
    OutlineGenerator<
        'a,
        GrafWrapper<DomTree<TemplateMarker<PrettyText<EmptyMarker<Accumulator>>>>>,
    >,
>;

/// A [`DocumentSink`] for rendering a complete Wikitext document.
/// This is equivalent to `Parser::parse` or `Parser::recursiveTagParseFully`.
pub(super) struct ParseFully<'a>(ParseFullyChain<'a>);

impl<'a> DocumentSink for ParseFully<'a> {
    const UNSTRIP_MARKERS: bool = true;

    type Args = &'a mut Outline;

    #[inline]
    fn new(args: Self::Args) -> Self
    where
        Self: Sized,
    {
        Self(AttributeFilter::new(OutlineGenerator::new(
            args,
            GrafWrapper::new(DomTree::new(TemplateMarker::new(PrettyText::new(
                EmptyMarker::new(Accumulator::new()),
            )))),
        )))
    }

    #[inline]
    fn set_in_caption(&mut self, in_caption: bool) {
        self.0.next_mut().next_mut().set_in_caption(in_caption);
    }

    #[inline]
    fn set_in_list(&mut self, in_list: bool) {
        self.0.next_mut().next_mut().set_in_list(in_list);
    }
}

impl Sink for ParseFully<'_> {
    #[inline]
    fn comment_end(&mut self) {
        self.0.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.0.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        self.0.entity(value, raw);
    }

    #[inline]
    fn finish(self) -> String {
        self.0.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        self.0.new_line();
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        self.0.strip_marker(marker);
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        self.0.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        self.0.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.0.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.0.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.0.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        self.0.text(text);
    }
}
