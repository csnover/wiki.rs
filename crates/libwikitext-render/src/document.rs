//! The root of a Wikitext document.

use super::{
    Error, LinkKind, Result, State, StripMarker,
    emitters::{
        Accumulator, AttributeFilter, CategoryTrim, Chain as _, DomTree, EmptyTagger, GrafEmitter,
        ListEmitter, OutlineEmitter, PrettyText, Sink, TableEmitter, TableFoster, TemplateTagger,
        TextStyleEmitter,
    },
    extension_tags,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    tags::{self, PHRASING_TAGS},
};
use crate::tags::ExternalLinkKind;
use core::fmt::Write as _;
use either::Either;
use libmisc::{CowExt as _, to_ascii_lower};
use libphp_rs::strtr;
use libwikitext_common::{
    AnchorEncodeMode, anchor_encode, decode_html, format_message, normalize_attr,
    title::{Namespace, Title},
    title_decode,
};
use libwikitext_parse::{
    AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant, MagicLink,
    Output, Span, Spanned, TextStyle, Token, VOID_TAGS,
};
use std::borrow::Cow;

/// The chain of render nodes used to render the document.
type RendererChain = TableEmitter<
    AttributeFilter<
        CategoryTrim<
            OutlineEmitter<
                DomTree<
                    TableFoster<GrafEmitter<TemplateTagger<EmptyTagger<PrettyText<Accumulator>>>>>,
                >,
            >,
        >,
    >,
>;

/// The root of a Wikitext document.
#[derive(Debug)]
pub(crate) struct Document {
    /// If true, this [`Document`] is used to render a document fragment rather
    /// than a complete document.
    fragment: bool,
    /// The stack of inclusion control tags.
    in_include: Vec<InclusionMode>,
    /// The output sink.
    next: RendererChain,
    /// The stack of open HTML elements.
    list_stack: Vec<ListEmitter>,
    /// The [`TextStyle`] emitter.
    text_style_emitter: Vec<TextStyleEmitter>,
}

impl Document {
    /// Creates a new [`Document`].
    pub(crate) fn new(fragment: bool) -> Self {
        Self {
            fragment,
            in_include: <_>::default(),
            next: TableEmitter::new(AttributeFilter::new(CategoryTrim::new(
                OutlineEmitter::new(DomTree::new(TableFoster::new(GrafEmitter::new(
                    TemplateTagger::new(EmptyTagger::new(PrettyText::new(Accumulator::new()))),
                )))),
            ))),
            list_stack: <_>::default(),
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
        let name = attribute.name();
        let is_kv = name.is_some();
        let value = attribute.value();
        #[rustfmt::skip]
        if let [Spanned { span, node: Token::Text }] = name.unwrap_or(value)
        {
            let name = to_ascii_lower(sp.source[span.into_range()].trim_ascii());
            self.next.tag_attribute_start(&name);
            if is_kv {
                self.attribute_value(state, sp, &name, value)?;
            }
            self.next.tag_attribute_end(&name);
        } else {
            // Maybe it is a Wikitext table and someone shoved e.g. `<ref>`
            // into the attribute list, smile smile. When this happens, content
            // needs to be ignored, only attributes should be emitted, using the
            // awful Wikitext table attributes whitelisted HTML tag rule
            self.adopt_tokens(state, sp, name.unwrap_or(value))?;

            // At least 'Template:Skip to top and bottom' contains invalid HTML
            // where an attribute is missing a close quote, and this is error
            // corrected differently in HTML5 versus the MW parser, so it is
            // necessary to handle the key and value parts separately and always
            // make sure the value is quoted or most of the page content ends up
            // in the attribute.
            if is_kv {
                self.adopt_tokens(state, sp, value)?;
            }
        };

        Ok(())
    }

    /// Transforms and writes an attribute value.
    fn attribute_value(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        name: &str,
        value: &[Spanned<Token>],
    ) -> Result {
        let name = to_ascii_lower(name);
        // TODO: Probably *all* the values should be going through
        // StripMarkers::unstrip?
        let value = match name.as_ref() {
            "class" => {
                // TODO: Look for the mw-collapse classes and dump appropriate
                // form hooks into the HTML to allow arbitrary collapsing
                // elements without scripts
                Either::Right(value)
            }
            "id" => Either::Left(
                sp.eval_unstrip(state, value)?
                    .map(normalize_attr)
                    .map(|v| anchor_encode(v, AnchorEncodeMode::Html5)),
            ),
            "style" => {
                // MediaWiki does sanitising, wiki.rs does not. What wiki.rs
                // *does* do is get all these inline styles out of the way so
                // that `!important` is not required to style pages
                let value = sp.eval_unstrip(state, value)?.map(decode_html);
                let mut out = String::new();
                let mut input = value.as_ref();
                while !input.is_empty() {
                    // 'Template:Table cell templates' contains a bunch of
                    // invalid garbage. When this happens, just try skipping to
                    // the next possibly valid declaration.
                    if let Ok((decl, next)) = barely_css::decl(input) {
                        input = &input[next..];
                        if let Some((name, value)) = decl {
                            if name.starts_with("--") {
                                write!(out, "{name}:{value};")?;
                            } else {
                                write!(out, "--mw-output-{name}:{value};")?;
                            }
                        }
                    } else if let Some(next) = input.find(';') {
                        input = &input[next + 1..];
                    } else {
                        break;
                    }
                }
                Either::Left(out.into())
            }
            "aria-describedby" | "aria-flowto" | "aria-labelledby" | "aria-owns" => {
                let value = sp.eval_unstrip(state, value)?.map(decode_html);
                // https://github.com/rust-lang/rust/issues/79524
                let mut out = String::new();
                for v in value
                    .split_ascii_whitespace()
                    .map(|v| normalize_attr(v).map(|v| anchor_encode(v, AnchorEncodeMode::Html5)))
                {
                    if !out.is_empty() {
                        out += " ";
                    }
                    out += &v;
                }
                Either::Left(out.into())
            }
            _ => Either::Right(value),
        };

        match value {
            Either::Left(value) => self.next.text(&value),
            Either::Right(value) => self.adopt_tokens(state, sp, value)?,
        }
        Ok(())
    }

    /// Finalises the document and returns the resulting output.
    pub(crate) fn finish(mut self, state: &mut State<'_, '_, '_>) -> String {
        self.text_style_emitter
            .last_mut()
            .unwrap()
            .finish(&mut self.next);

        for mut rest in self.list_stack.into_iter().rev() {
            rest.finish(&mut self.next);
        }

        self.next.finish(state)
    }

    /// Writes the contents of a strip marker to the output.
    fn write_strip_marker(&mut self, tag: &StripMarker) {
        match tag {
            StripMarker::NoWiki(text) => {
                // The mere presence of a strip marker needs to cause the
                // GrafEmitter to decide that there is content, even if the
                // marker is actually empty, because the spec defines this
                // code as running before strip markers are unstripped
                self.next
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .force_content();
                self.next.text(&decode_html(text));
            }
            StripMarker::Inline(text) => {
                self.next.raw_html_inline(text);
            }
            StripMarker::Block(text) => {
                self.next.raw_html_block(text);
            }
            StripMarker::WikiRsSourceStart(name) => {
                self.next
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .push(name.clone());
            }
            StripMarker::WikiRsSourceEnd(name) => {
                self.next
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .next_mut()
                    .pop(name);
            }
        }
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
        tags::render_external_link(self, state, sp, target, target, true)
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
                // TODO: This is supposed to ignore if a message is "-", but
                // `format_message` filters those away.
                let title = format_message(state.messages, ["hidden-category-category"], |_| {
                    Ok::<_, Error>(None)
                })?;
                let title = Title::from_parts(
                    state.statics.db.config(),
                    state.globals.title.namespace(),
                    &title,
                    None,
                    None,
                )?;
                state.globals.categories.insert(&title);
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
        self.next.tag_end(&to_ascii_lower(name));
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
            Some(Either::Left(marker)) => {
                if self.fragment {
                    let strip_text = &mut String::new();
                    state.strip_markers.push(strip_text, &name, marker);
                    self.next.raw_html_inline(strip_text);
                } else {
                    self.write_strip_marker(&marker);
                }
            }
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
        span: Span,
        prefix: Option<Spanned<&str>>,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        let target = sp.eval(state, target)?.map(title_decode);
        let target = state.globals.title.join(&target);
        let Ok(title) = Title::new(state.statics.db.config(), &target, None) else {
            return self.adopt_text(state, sp, span, &sp.source[span.into_range()]);
        };
        self.text_style_emitter.push(<_>::default());
        let force_link = target.starts_with(':');
        if !force_link && title.is_local_category() {
            // Normally the corresponding content-part is supposed to be used as
            // a sort key. However, since this implementation does not have any
            // category pages, and the sort key does not change the sort order
            // of the category list at the end of the page, the content-part of
            // a category is simply ignored.
            state.globals.categories.insert(&title);
            self.next.next_mut().next_mut().clear();
            if let Some(prefix) = prefix {
                self.adopt_generated(state, sp, None, &prefix)?;
            }
            if let Some(trail) = trail {
                self.adopt_generated(state, sp, None, &trail)?;
            }
        } else if !force_link && title.is_local_file() {
            if let Some(prefix) = prefix {
                self.adopt_generated(state, sp, None, &prefix)?;
            }
            super::image::render_media(self, state, sp, title, content)?;
            if let Some(trail) = trail {
                self.adopt_generated(state, sp, None, &trail)?;
            }
        } else {
            tags::render_internal_link(
                self,
                state,
                sp,
                &target,
                prefix.map(|v| v.as_ref()),
                content,
                trail.map(|v| v.as_ref()),
                title,
            )?;
        }
        self.text_style_emitter.pop();
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
        _span: Span,
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

        let mut content = if let Some((last, content)) = content.split_last()
            && matches!(last.node, Token::NewLine)
        {
            content
        } else {
            content
        };

        let mut list = self.list_stack.pop().unwrap_or_default();

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

        self.list_stack.push(list);
        Ok(())
    }

    fn adopt_magic_link(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        magic: &MagicLink,
    ) -> Result {
        let (link, content) = match magic {
            MagicLink::Isbn(id) => {
                let url_id = strtr(id, &[("-", ""), (" ", ""), ("x", "X")]);
                let link = LinkKind::Internal(Title::new(
                    state.statics.db.config(),
                    &format!("Booksources/{url_id}"),
                    Some(Namespace::SPECIAL),
                )?);
                (link, format!("ISBN {id}"))
            }
            MagicLink::Pmid(id) => {
                let url = format_message(state.messages, ["pubmedurl"], |key| {
                    Ok::<_, Error>(
                        (key == "1").then_some(Cow::Borrowed(&sp.source[id.into_range()])),
                    )
                })?;
                let link = LinkKind::External(url, ExternalLinkKind::MagicPmid);
                (link, format!("PMID {}", &sp.source[id.into_range()]))
            }
            MagicLink::Rfc(id) => {
                let url = format_message(state.messages, ["rfcurl"], |key| {
                    Ok::<_, Error>(
                        (key == "1").then_some(Cow::Borrowed(&sp.source[id.into_range()])),
                    )
                })?;
                let link = LinkKind::External(url, ExternalLinkKind::MagicRfc);
                (link, format!("RFC {}", &sp.source[id.into_range()]))
            }
        };
        tags::render_start_link(self, state, sp, &link)?;
        self.next.text(&content);
        tags::render_end_link(self, state, sp)?;
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
        if let Some(mut list) = self.list_stack.pop() {
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
        sp: &StackFrame<'_>,
        span: Span,
        _name: &[Spanned<Token>],
        _default: Option<&[Spanned<Token>]>,
    ) -> Result {
        // 'Template:Human-centric' uses a parameter in an invalid way which
        // causes it to be emitted as a literal inside of an HTML attribute
        log::warn!(
            "Unresolved parameter {} in output",
            &sp.source[span.into_range()]
        );
        self.next.text(&sp.source[span.into_range()]);
        Ok(())
    }

    fn adopt_preformatted(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        content: &[Spanned<Token>],
    ) -> Result {
        self.next.tag_start_full("pre");
        self.adopt_tokens(state, sp, content)?;
        self.next.tag_end("pre");
        Ok(())
    }

    fn adopt_redirect(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        prefix: Option<Spanned<&str>>,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
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
        attributes: &[Spanned<Argument>],
        _self_closing: bool,
    ) -> Result {
        let name = to_ascii_lower(name);
        self.next.tag_start(&name);
        for attr in attributes {
            self.attribute(state, sp, attr)?;
        }
        self.next.tag_start_end(&name);
        Ok(())
    }

    fn adopt_strip_marker(
        &mut self,
        state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        marker: &str,
    ) -> Result {
        if let Some(tag) = state.strip_markers.get(marker) {
            self.write_strip_marker(tag);
            Ok(())
        } else {
            Err(Error::StripMarker(marker.to_owned()))
        }
    }

    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        if self.next.is_empty() {
            self.next.text(&sp.source[span.into_range()]);
        } else {
            self.adopt_start_tag(state, sp, span, "caption", attributes, false)?;
        }
        Ok(())
    }

    fn adopt_table_data(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        if self.next.is_empty() {
            self.next.text(&sp.source[span.into_range()]);
        } else {
            self.next.data();
            // This relies on DOM error correction later in the chain to make
            // sure the appropriate elements exist in the appropriate places
            self.adopt_start_tag(state, sp, span, "td", attributes, false)?;
        }
        Ok(())
    }

    fn adopt_table_end(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
    ) -> Result {
        if self.next.is_empty() {
            self.next.text(&sp.source[span.into_range()]);
        } else {
            self.next.end();
            self.next.tag_end("table");
        }
        Ok(())
    }

    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        if self.next.is_empty() {
            self.next.text(&sp.source[span.into_range()]);
        } else {
            self.next.data();
            // This relies on DOM error correction later in the chain to make
            // sure the appropriate elements exist in the appropriate places
            self.adopt_start_tag(state, sp, span, "th", attributes, false)?;
        }
        Ok(())
    }

    fn adopt_table_row(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        if self.next.is_empty() {
            self.next.text(&sp.source[span.into_range()]);
        } else {
            self.next.row_start();
            // This relies on DOM error correction later in the chain to make
            // sure the appropriate elements exist in the appropriate places
            self.adopt_start_tag(state, sp, span, "tr", attributes, false)?;
            self.next.row_end();
        }
        Ok(())
    }

    fn adopt_table_start(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.adopt_start_tag(state, sp, span, "table", attributes, false)?;
        self.next.start();
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
