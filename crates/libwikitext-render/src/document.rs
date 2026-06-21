//! The root of a Wikitext document.

use super::{
    Error, LinkKind, Result, State, StripMarker, StripMarkers,
    emitters::{
        Accumulator, AttributeFilter, Chain as _, DomTree, EmptyTagger, ListEmitter,
        OutlineEmitter, PrettyText, Sink, TableFoster, TemplateTagger, TextStyleEmitter,
    },
    extension_tags,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    tags::{self, PHRASING_TAGS},
};
use crate::tags::ExternalLinkKind;
use either::Either;
use libmisc::{CowExt as _, to_ascii_lower};
use libphp_rs::strtr;
use libwikitext_common::{
    decode_html, format_message,
    title::{Namespace, Title},
    title_decode,
};
use libwikitext_parse::{
    AnnoAttribute, Argument, FileMap, HeadingLevel, InclusionMode, LangFlags, LangVariant,
    MagicLink, Output, Span, Spanned, TextStyle, Token, VOID_TAGS,
};
use std::borrow::Cow;

/// The chain of render nodes used to render the document.
type RendererChain = AttributeFilter<
    OutlineEmitter<DomTree<TemplateTagger<TableFoster<EmptyTagger<PrettyText<Accumulator>>>>>>,
>;

/// A Wikitext table frame.
#[derive(Debug, Default)]
struct TableState {
    /// If true, a Wikitext table row, header, or data token has been seen.
    has_tbody: bool,
    /// The tag name of the currently open table caption, header, or data tag.
    last_tag: Option<&'static str>,
    /// The half-parsed attributes for a pending table row.
    tr_attrs: String,
    /// If true, a `<tr>` has been emitted and needs to be closed.
    tr_emitted: bool,
}

/// The root of a Wikitext document.
#[derive(Debug)]
pub(crate) struct Document {
    /// If true, this [`Document`] is used to render a document fragment rather
    /// than a complete document.
    fragment: bool,
    in_block_elem: bool,
    in_blockquote: bool,
    /// The stack of inclusion control tags.
    in_include: Vec<InclusionMode>,
    /// If true, inside a `<pre>` tag.
    in_pre: bool,
    /// The output sink.
    pub(super) next: RendererChain,
    last_graf: Option<&'static str>,
    /// The list emitter.
    list_emitter: Option<ListEmitter>,
    pending_p_tag: Option<PendingGraf>,
    /// The Wikitext table emitters.
    table_emitter: Vec<TableState>,
    /// The [`TextStyle`] emitters.
    text_style_emitter: Vec<TextStyleEmitter>,
}

#[derive(Clone, Copy, Debug)]
enum PendingGraf {
    OpenGraf,
    SplitGraf,
}

impl PendingGraf {
    fn emit<S: Sink + ?Sized>(self, next: &mut S) {
        match self {
            Self::OpenGraf => next.tag_start_full("p"),
            Self::SplitGraf => {
                next.tag_end("p");
                next.tag_start_full("p");
            }
        }
    }
}

impl Document {
    /// Creates a new [`Document`].
    pub(crate) fn new(fragment: bool) -> Self {
        Self {
            fragment,
            in_block_elem: <_>::default(),
            in_blockquote: <_>::default(),
            in_include: <_>::default(),
            in_pre: <_>::default(),
            next: AttributeFilter::new(OutlineEmitter::new(DomTree::new(TemplateTagger::new(
                TableFoster::new(EmptyTagger::new(PrettyText::new(Accumulator::new()))),
            )))),
            last_graf: <_>::default(),
            list_emitter: <_>::default(),
            pending_p_tag: <_>::default(),
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
        self.adopt_tokens(state, sp, value)?;
        self.next.tag_attribute_end(&name);
        Ok(())
    }

    /// Finalises the document and returns the resulting output.
    pub(crate) fn finish(mut self, state: &mut State<'_, '_, '_>) -> String {
        self.text_style_emitter
            .last_mut()
            .unwrap()
            .finish(&mut self.next);

        if let Some(mut list) = self.list_emitter {
            list.finish(&mut self.next);
        }

        for table in self.table_emitter {
            if let Some(name) = table.last_tag {
                self.next.tag_end(name);
                self.next.new_line();
            }
            if table.tr_emitted {
                self.next.tag_end("tr");
                self.next.new_line();
            }
            if !table.has_tbody {
                self.next.tag_start_full("tr");
                self.next.tag_start_full("td");
                self.next.tag_end("td");
                self.next.tag_end("tr");
                self.next.new_line();
            }
            self.next.tag_end("table");
            self.next.new_line();
        }

        let result = self.next.finish(state);
        // God, this is so fucking stupid
        if result == "<table>\n<tr><td></td></tr>\n</table>" {
            <_>::default()
        } else {
            result
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

    fn pre_text<'a>(
        &self,
        strip_markers: &'a StripMarkers,
        source: &'a str,
        content: &[Spanned<Token>],
    ) -> Option<(usize, &'a str)> {
        // Because the block level algorithm normally runs after general strip
        // markers are unstripped, they must also participate in the prefix
        // detection if they contain content.
        let (index, text) = content.iter().enumerate().find_map(|(index, token)| {
            if let Spanned {
                span,
                node: Token::Text,
            } = token
            {
                Some((index, source[span.into_range()].strip_prefix(' ')))
            } else if let Token::StripMarker(key) = &token.node
                && let Some(marker) = strip_markers.get(&source[key.into_range()])
            {
                match marker {
                    StripMarker::Block(s) | StripMarker::Inline(s) => {
                        (!s.is_empty()).then(|| (index, s.strip_prefix(' ')))
                    }
                    StripMarker::NoWiki(_) => Some((index, None)),
                    _ => None,
                }
            } else {
                Some((index, None))
            }
        })?;

        text.and_then(|text| {
            let in_pre = self.last_graf == Some("pre")
                || content.len() > index
                || text.bytes().any(|b| !b.is_ascii_whitespace());
            (in_pre && !self.in_blockquote).then_some((index, text))
        })
    }

    fn is_meta_line(
        strip_markers: &StripMarkers,
        source: &str,
        content: &[Spanned<Token>],
    ) -> bool {
        let mut in_style = false;
        for token in content {
            if let Token::EndTag { name } = &token.node {
                let name = to_ascii_lower(&source[name.into_range()]);
                if in_style && name == "style" {
                    in_style = false;
                    continue;
                }
                return false;
            }

            if in_style {
                continue;
            }

            if let Token::StartTag { name, .. } = &token.node {
                let name = to_ascii_lower(&source[name.into_range()]);
                if name == "style" {
                    in_style = true;
                    continue;
                } else if name != "link" {
                    return false;
                }
            }

            if let Spanned {
                span,
                node: Token::Text,
            } = token
                && source[span.into_range()]
                    .as_bytes()
                    .iter()
                    .all(u8::is_ascii_whitespace)
            {
                continue;
            }

            if matches!(token.node, Token::NewLine | Token::Comment { .. }) {
                continue;
            }

            // Because this pass is supposed to behave as if general strip
            // markers were already unstripped, they have to be handled
            // specifically. wiki.rs extensions do not emit `<style>` or
            // `<link>` tags, so they can be treated as opaque blobs of HTML
            // for the purpose of this algorithm.
            // TODO: Probably this whole thing should be in the emitter chain,
            // but the way this stupid algorithm works requires an entire line
            // to be buffered before it knows what to do at the start of the
            // line. So there is literally no good place to put it.
            if let Token::StripMarker(key) = &token.node {
                let key = &source[key.into_range()];
                match strip_markers.get(key) {
                    Some(StripMarker::Block(s) | StripMarker::Inline(s))
                        if !s.as_bytes().iter().all(u8::is_ascii_whitespace) =>
                    {
                        return false;
                    }
                    Some(StripMarker::NoWiki(_)) => return false,
                    _ => {}
                }
            }

            return false;
        }

        // A `<style>` with no `</style>` is not a valid meta line
        !in_style
    }

    fn end_p(&mut self, finishing: bool) {
        if let Some(graf) = self.last_graf.take() {
            self.next.tag_end(graf);
            if !finishing {
                self.next.new_line();
            }
        }
        self.in_pre = false;
    }

    fn is_empty_line(
        strip_markers: &StripMarkers,
        source: &str,
        content: &[Spanned<Token>],
    ) -> bool {
        for token in content {
            if let Spanned {
                span,
                node: Token::Text,
            } = token
                && source[span.into_range()]
                    .bytes()
                    .any(|b| !b.is_ascii_whitespace())
            {
                return false;
            } else if let Token::StripMarker(key) = &token.node
                && let Some(marker) = strip_markers.get(&source[key.into_range()])
            {
                match marker {
                    StripMarker::Block(s) | StripMarker::Inline(s) if !s.is_empty() => {
                        return s.bytes().any(|b| !b.is_ascii_whitespace());
                    }
                    StripMarker::NoWiki(_) => return false,
                    _ => {}
                }
            } else if !matches!(token.node, Token::NewLine | Token::Comment { .. }) {
                return false;
            }
        }
        true
    }
}

impl Surrogate<Error> for Document {
    fn adopt_autolink(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
    ) -> Result {
        tags::render_external_link(self, state, sp, target, &[], true)
    }

    fn adopt_behavior_switch(
        &mut self,
        state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
    ) -> Result {
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
        // If we are in an attribute, comments are actually text content!
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
        self.next.entity(value, &sp.source[span.into_range()]);
        Ok(())
    }

    fn adopt_extension(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        content: Option<&str>,
    ) -> Result {
        let name = to_ascii_lower(name);
        match extension_tags::render_extension_tag(
            state,
            sp,
            Some(span),
            &name,
            &extension_tags::InArgs::Wikitext(attributes),
            content,
            true,
        )? {
            Some(Either::Left(marker)) => self.next.strip_marker(&marker),
            Some(Either::Right(_)) => todo!("this should never happen?"),
            None => {}
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
        tags::render_external_link(self, state, sp, target, content, false)
    }

    fn adopt_generated(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Option<Span>,
        text: &str,
    ) -> Result {
        self.next.text(text);
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
        // TODO: It is extremely unclear what these tokens are supposed to do
        // given that they do not seem to do anything at all on MW and just emit
        // plain text like this.
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
            super::image::render_media(self, state, sp, title, content)?;
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

        if self.in_pre {
            self.next.text(&sp.source[span.into_range()]);
            return Ok(());
        }

        // TODO: If the newline is just going to be ignored, why have it in the
        // AST at all?
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
        let mut list = self.list_emitter.take().unwrap_or_default();

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
        }
        self.next.new_line();
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

    fn adopt_line(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        content: &[Spanned<Token>],
        last: bool,
    ) -> Result {
        let mut pre_open_match = false;
        let mut pre_close_match = false;
        let mut open_match = false;
        let mut close_match = false;
        let mut start_at = 0;
        for token in content {
            if let Token::StartTag { name, .. } = &token.node {
                let name = to_ascii_lower(&sp.source[name.into_range()]);
                pre_open_match |= name == "pre";
                open_match |= ALWAYS_TAG.contains(&name) || BLOCK_TAG.contains(&name);
                close_match |= NEVER_TAG.contains(&name);
                self.in_blockquote |= name == "blockquote";
            } else if let Token::EndTag { name } = &token.node {
                let name = to_ascii_lower(&sp.source[name.into_range()]);
                pre_close_match |= name == "pre";
                open_match |= ALWAYS_TAG.contains(&name);
                close_match |= NEVER_TAG.contains(&name) || ANTI_BLOCK_TAG.contains(&name);
                self.in_blockquote &= name != "blockquote";
            }
        }

        if self.in_pre {
            self.next.text(&sp.source[span.into_range()]);
        }

        if open_match || close_match {
            self.pending_p_tag = None;
            if !self.in_pre || pre_open_match {
                self.end_p(false);
            }
            self.in_pre |= pre_open_match && !pre_close_match;
            self.in_block_elem = !close_match;
        } else if !self.in_block_elem && !self.in_pre {
            if let Some((index, pre_text)) =
                self.pre_text(&state.strip_markers, &sp.source, content)
            {
                if self.last_graf != Some("pre") {
                    self.pending_p_tag = None;
                    self.end_p(false);
                    self.next.tag_start_full("pre");
                    self.last_graf = Some("pre");
                }
                if self.pending_p_tag.is_none() {
                    self.next.text(pre_text);
                }
                start_at = index + 1;
            } else if Self::is_meta_line(&state.strip_markers, &sp.source, content) {
                if self.pending_p_tag.take().is_some() {
                    self.end_p(false);
                }
            } else if Self::is_empty_line(&state.strip_markers, &sp.source, content) {
                if let Some(pending) = self.pending_p_tag.take() {
                    pending.emit(&mut self.next);
                    self.next.tag_start_full("br");
                    self.last_graf = Some("p");
                } else if self.last_graf != Some("p") {
                    self.end_p(false);
                    self.pending_p_tag = Some(PendingGraf::OpenGraf);
                } else {
                    self.pending_p_tag = Some(PendingGraf::SplitGraf);
                }
            } else if let Some(pending) = self.pending_p_tag.take() {
                pending.emit(&mut self.next);
                self.last_graf = Some("p");
            } else if self.last_graf != Some("p") {
                self.end_p(false);
                self.next.tag_start_full("p");
                self.last_graf = Some("p");
            }
        }

        if pre_close_match {
            self.in_pre = false;
        }

        if self.pending_p_tag.is_none() {
            self.adopt_tokens(state, sp, &content[start_at..])?;
            if !last || self.last_graf.is_some() {
                self.next.new_line();
            }
        }

        Ok(())
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
        let name = to_ascii_lower(name);
        self.in_pre |= name == "pre";
        let attributes = sp
            .eval(state, attributes)?
            .map(|out| state.strip_markers.unstrip(out));
        self.write_start_tag(state, sp, &name, &attributes)
    }

    fn adopt_strip_marker(
        &mut self,
        state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        marker: &str,
    ) -> Result {
        if let Some(tag) = state.strip_markers.get(marker) {
            self.next.strip_marker(tag);
            Ok(())
        } else {
            Err(Error::StripMarker(marker.to_owned()))
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
        if let Some(table) = self.table_emitter.pop() {
            if let Some(name) = table.last_tag {
                self.next.tag_end(name);
            }
            if table.tr_emitted {
                self.next.tag_end("tr");
            }
            if !table.has_tbody {
                self.next.tag_start_full("tr");
                self.next.tag_start_full("td");
                self.next.tag_end("td");
                self.next.tag_end("tr");
            }
            self.next.tag_end("table");
        } else {
            self.next.text(&sp.source[span.into_range()]);
        }
        Ok(())
    }

    #[inline]
    fn adopt_table_heading(
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
                .map(|out| state.strip_markers.unstrip(out));
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
    ) -> Result {
        self.adopt_start_tag(state, sp, span, "table", attributes, false)?;
        self.table_emitter.push(<_>::default());
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
        self.next.text(text);
        Ok(())
    }

    fn adopt_text_style(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        style: TextStyle,
    ) -> Result {
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

/// An HTML attribute state.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum Attribute {
    /// In the name part.
    Name,
    /// In the value part.
    Value,
}

/// An HTML tree node.
#[derive(Debug)]
pub(super) enum Node {
    /// An HTML attribute.
    Attribute(Attribute),
    /// An HTML tag.
    Tag(Cow<'static, str>),
}

impl Node {
    /// Whether this element can parent the element with the given lowercase tag
    /// name.
    pub(super) fn can_parent(&self, tag: &str) -> bool {
        match self {
            Node::Tag(parent) => {
                if VOID_TAGS.contains(parent) {
                    panic!("void tag on element stack")
                } else if let Some(children) = PARENTS.get(parent) {
                    children.contains(&tag)
                } else if matches!(parent.as_ref(), "td" | "th" | "caption") {
                    !matches!(tag, "tr" | "td" | "th" | "caption")
                } else if parent == "span" && tag == "div" {
                    // 'Template:Infobox element' thinks it can put a div in a
                    // span. And technically it works in browsers, even though
                    // it is illegal in HTML.
                    true
                } else if parent == "p" || PHRASING_TAGS.contains(parent) {
                    PHRASING_TAGS.contains(tag)
                } else if matches!(tag, "dt" | "dd") {
                    // Technically it is supposed to be only allowed in
                    // `<dl>` or `<dl><div>` but Wikitext is not compliant and
                    // only cares about these tags not being themselves
                    matches!(parent.as_ref(), "div" | "dl")
                } else if tag == "li" {
                    // Technically `<li>` is supposed to be only parented by
                    // `<menu>` `<ol>` `<ul>` but Wikitext is not compliant and
                    // only cares about these tags not being themselves. (There
                    // are unit tests that explicitly allow `<dd><li>`.)
                    tag != parent
                } else {
                    // `parent` must be an unrestricted block element
                    true
                }
            }
            Node::Attribute(_) => unreachable!(),
        }
    }

    /// Writes the terminator for this element to the given output.
    pub(super) fn close<S: Sink>(self, next: &mut S) {
        match self {
            Node::Attribute(_) => {}
            Node::Tag(name) => {
                debug_assert!(!VOID_TAGS.contains(&name));
                next.tag_end(&name);
            }
        }
    }

    /// The tag name for this node.
    pub(super) fn tag_name(&self) -> Option<&str> {
        match self {
            Node::Attribute(_) => None,
            Node::Tag(name) => Some(name),
        }
    }
}

/// HTML tags which start a new block when they are encountered as either a
/// start or end tag.
static ALWAYS_TAG: phf::Set<&str> = phf::phf_set! {
    "caption", "dd", "dt", "li", "tr"
};

/// HTML tags which terminate a block when they are encountered as an end tag.
static ANTI_BLOCK_TAG: phf::Set<&str> = phf::phf_set! { "td", "th" };

/// HTML tags which start a new block when they are encountered as start tags.
static BLOCK_TAG: phf::Set<&str> = phf::phf_set! {
    "h1", "h2", "h3", "h4", "h5", "h6", "ol", "p", "pre", "table", "ul"
};

/// HTML tags which terminate a block when they are encountered as start or end
/// tags.
static NEVER_TAG: phf::Set<&str> = phf::phf_set! {
    "aside", "blockquote", "center", "div", "figure", "hr"
};

/// Tags with restricted allowable children.
static PARENTS: phf::Map<&str, &[&str]> = phf::phf_map! {
    // Tables are ‘allowed’ to hold td/th because the tr will be implicitly
    // inserted
    "table" => &["caption", "td", "th", "tr"],
    "tr" => &["td", "th"],
    "dl" => &["dd", "dt"],
    "ol" => &["li"],
    "ul" => &["li"]
};
