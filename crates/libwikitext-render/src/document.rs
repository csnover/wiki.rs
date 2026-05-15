//! The root of a Wikitext document.

use super::{
    Error, Result, State, StripMarker,
    emitters::{
        Accumulator, AfterHeadingChomper, CategoryTrim, Chain as _, DomTree, EmptyTagger,
        GrafEmitter, ListEmitter, OutlineEmitter, PrettyText, Sink, TemplateTagger,
        TextStyleEmitter,
    },
    extension_tags,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    tags::{self, PHRASING_TAGS},
    trim::Trim,
};
use core::fmt::Write as _;
use either::Either;
use libmisc::CowExt as _;
use libphp_rs::strtr;
use libwikitext_common::{AnchorEncodeMode, anchor_encode, decode_html};
use libwikitext_parse::{
    AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant, Output, Span,
    Spanned, TextStyle, Token, VOID_TAGS,
};
use std::borrow::Cow;

/// The chain of render nodes used to render the document.
type RendererChain = CategoryTrim<
    DomTree<
        AfterHeadingChomper<
            GrafEmitter<OutlineEmitter<PrettyText<TemplateTagger<EmptyTagger<Accumulator>>>>>,
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
    stack: Vec<Node>,
    /// The [`TextStyle`] emitter.
    text_style_emitter: TextStyleEmitter,
    /// The number of open Wikitext tables.
    wikitext_table_count: usize,
}

impl Document {
    /// Creates a new [`Document`].
    pub(crate) fn new(fragment: bool) -> Self {
        Self {
            fragment,
            in_include: <_>::default(),
            next: CategoryTrim::new(DomTree::new(AfterHeadingChomper::new(GrafEmitter::new(
                OutlineEmitter::new(PrettyText::new(TemplateTagger::new(EmptyTagger::new(
                    Accumulator::new(),
                )))),
            )))),
            stack: <_>::default(),
            text_style_emitter: <_>::default(),
            wikitext_table_count: <_>::default(),
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
            // TODO: Use non-allocating to_lowercase
            let name = sp.source[span.into_range()].trim_ascii().to_ascii_lowercase();
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
        let name = name.to_ascii_lowercase();
        // TODO: Probably *all* the values should be going through
        // StripMarkers::unstrip?
        let value = match name.as_str() {
            "class" => {
                // TODO: Look for the mw-collapse classes and dump appropriate
                // form hooks into the HTML to allow arbitrary collapsing
                // elements without scripts
                Either::Right(value)
            }
            "id" => Either::Left(
                sp.eval_unstrip(state, value)?
                    .map(|v| anchor_encode(v, AnchorEncodeMode::Html5)),
            ),
            "style" => {
                // MediaWiki does sanitising, wiki.rs does not. What wiki.rs
                // *does* do is get all these inline styles out of the way so
                // that `!important` is not required to style pages
                let value = sp.eval_unstrip(state, value)?.map(decode_html);
                let mut out = String::new();
                let mut input = &*value;
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
                    .map(|v| anchor_encode(v, AnchorEncodeMode::Html5))
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

    /// A hacky side-channel to dump the category whitespace in response to a
    /// Wikitext category link.
    // TODO: Less hacky?
    pub(crate) fn category(&mut self) {
        self.next.clear();
    }

    /// Finalises the document and returns the resulting output.
    pub(crate) fn finish(mut self) -> String {
        self.text_style_emitter.finish(&mut self.next);

        for rest in self.stack.drain(..).rev() {
            rest.close(&mut self.next);
        }

        self.next.finish()
    }

    /// Returns true if the document is currently processing any table.
    ///
    /// The way that Wikitext and HTML tables interact is, like everything about
    /// Wikitext, cursed. In MediaWiki, `<table>` cannot start a Wikitext table,
    /// but a bare `</table>` *will* terminate the table. So there are three
    /// possible states, requiring two variables (this function, and
    /// `wikitext_table_count`):
    ///
    /// * Not in Wikitext table: Emit raw text
    /// * In Wikitext table but not HTML table: Emit content
    /// * In both Wikitext table and HTML table: Emit table element
    #[inline]
    fn in_table(&self) -> bool {
        self.next.next().in_table()
    }

    /// Writes the contents of a strip marker to the output.
    fn write_strip_marker(&mut self, tag: &StripMarker) {
        match tag {
            StripMarker::NoWiki(text) => {
                // The mere presence of a strip marker needs to cause the
                // GrafEmitter to decide that there is content, even if the
                // marker is actually empty
                self.next.next_mut().next_mut().next_mut().force_content();
                self.next.text(&decode_html(text));
            }
            StripMarker::Inline(text) => {
                // TODO: This should be a redundant check and garbage-in-attr
                // is handled correctly elsewhere
                if matches!(self.stack.last(), Some(Node::Attribute)) {
                    let escaped = strtr(text, &[("<", "&lt;"), (">", "&gt;")]);
                    if matches!(escaped, Cow::Owned(_)) {
                        // At least malformed Wikitext tables can cause this to
                        // happen by putting extension tags in attributes.
                        // TODO: This is supposed to take the attributes from
                        // the outermost tag, put them into the attributes
                        // map for the element, and then be overwritten by any
                        // later duplicate attribute
                        log::warn!("Stripped tags inside attributes");
                    }
                    self.next.text(&escaped);
                } else {
                    self.next.raw_html_inline(text);
                }
            }
            StripMarker::Block(text) => {
                if matches!(self.stack.last(), Some(Node::Attribute)) {
                    let escaped = strtr(text, &[("<", "&lt;"), (">", "&gt;")]);
                    if matches!(escaped, Cow::Owned(_)) {
                        // At least malformed Wikitext tables can cause this to
                        // happen by putting extension tags in attributes.
                        // TODO: This is supposed to take the attributes from
                        // the outermost tag, put them into the attributes
                        // map for the element, and then be overwritten by any
                        // later duplicate attribute
                        log::warn!("Stripped tags inside attributes");
                    }
                    self.next.text(&escaped);
                } else {
                    // Using "div" is a hack but one which does not really matter
                    // since anything that cannot parent a `<div>` cannot parent any
                    // other block-level element
                    while let Some(e) = self.stack.pop_if(|e| !e.can_parent("div")) {
                        e.close(&mut self.next);
                        self.next.new_line();
                    }
                    self.next.raw_html_block(text);
                }
            }
            StripMarker::WikiRsSourceStart(name) => {
                self.next
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
        content: &[Spanned<Token>],
    ) -> Result {
        // TODO: Autolink inside another link = plain text
        // autourl have empty content, other magic links have generated
        // content
        let content = if content.is_empty() { target } else { content };
        tags::render_external_link(self, state, sp, target, content, true)
    }

    fn adopt_behavior_switch(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
    ) -> Result {
        log::warn!("TODO: BehaviorSwitch __{name}__");
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
        // TODO: Use non-allocating to_lowercase
        self.next.tag_end(&name.to_ascii_lowercase());
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
        let name = name.to_ascii_lowercase();
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
        // TODO: Trim can be handled later in a separate handler?
        Trim::new(self, sp).adopt_tokens(state, sp, content)?;
        self.next.tag_end(level.tag_name());
        Ok(())
    }

    fn adopt_horizontal_rule(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _line_content: bool,
    ) -> Result {
        self.next.tag_start_full("hr");
        Ok(())
    }

    fn adopt_lang_variant(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        span: Span,
        _flags: Option<&LangFlags>,
        _variants: &[Spanned<LangVariant>],
        _raw: bool,
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
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        tags::render_wikilink(
            self,
            state,
            sp,
            target,
            content,
            trail.map(|trail| trail.node),
        )
    }

    fn adopt_list_item(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result {
        if let Some(Node::List(list)) = self.stack.last_mut() {
            list.emit(&mut self.next, bullets);
        } else {
            let mut list = ListEmitter::default();
            list.emit(&mut self.next, bullets);
            self.stack.push(Node::List(list));
        }

        let list_index = self.stack.len() - 1;
        Trim::new(self, sp).adopt_tokens(state, sp, content)?;

        // It is possible that content “inside” a list item actually contains
        // terminator tags for items outside of the list item which implicitly
        // end the list item. This happens in
        // 'Template:Sidebar with collapsible lists'. When this occurs, the
        // list will have been terminated already, so trying to close more
        // elements here will corrupt the tree.
        if self.stack.len() > list_index && matches!(self.stack[list_index], Node::List(_)) {
            for e in self.stack.drain(list_index + 1..).rev() {
                self.next.tag_end(e.tag_name().unwrap());
            }

            // The parser removes the newlines between list items in order to
            // make it easier to disambiguate the list-terminating newline.
            // Since the list item must have ended at a newline, finish the line
            // now.
            // self.next.new_line();
        }

        Ok(())
    }

    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_, '_, '_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        self.text_style_emitter.finish(&mut self.next);
        if let Some(Node::List(list)) = self.stack.last_mut() {
            list.finish(&mut self.next);
            self.stack.pop();
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

    fn adopt_redirect(
        &mut self,
        state: &mut State<'_, '_, '_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        self.next.tag_start("p");
        self.next.tag_attribute_start("class");
        self.next.text("redirectText");
        self.next.tag_attribute_end("class");
        self.next.tag_start_end("p");
        tags::render_wikilink(self, state, sp, target, content, trail.map(|v| &**v))?;
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
        // TODO: Use non-allocating `to_lowercase`
        let name = name.to_ascii_lowercase();
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
        if self.wikitext_table_count != 0 {
            self.adopt_start_tag(state, sp, span, "caption", attributes, false)?;
        } else if !self.in_table() {
            self.next.text(&sp.source[span.into_range()]);
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
        if self.wikitext_table_count == 0 {
            self.next.text(&sp.source[span.into_range()]);
        } else if self.in_table() {
            // This relies on DOM error correction later in the chain to make
            // sure there is a <tr>
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
        if self.wikitext_table_count == 0 {
            self.next.text(&sp.source[span.into_range()]);
        } else {
            self.wikitext_table_count -= 1;
            if self.in_table() {
                self.next.tag_end("table");
            }
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
        if self.wikitext_table_count == 0 {
            self.next.text(&sp.source[span.into_range()]);
        } else if self.in_table() {
            // This relies on DOM error correction later in the chain to make
            // sure there is a <tr>
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
        if self.wikitext_table_count == 0 {
            self.next.text(&sp.source[span.into_range()]);
        } else if self.in_table() {
            self.adopt_start_tag(state, sp, span, "tr", attributes, false)?;
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
        self.wikitext_table_count += 1;
        self.adopt_start_tag(state, sp, span, "table", attributes, false)
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
        self.text_style_emitter.emit(&mut self.next, style);
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

/// An HTML tree node.
#[derive(Debug)]
pub(super) enum Node {
    /// An HTML attribute.
    Attribute,
    /// A run of Wikitext list items.
    List(ListEmitter),
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
                } else if PHRASING_TAGS.contains(parent) {
                    PHRASING_TAGS.contains(tag)
                } else {
                    // `parent` must be an unrestricted block element
                    true
                }
            }
            Node::List(list) => {
                // TODO: Ordered/Unordered have tag_names of ol/ul but they are
                // actually <li>s
                !list.is_empty()
            }
            Node::Attribute => unreachable!(),
        }
    }

    /// Writes the terminator for this element to the given output.
    pub(super) fn close<S: Sink>(self, next: &mut S) {
        match self {
            Node::Attribute => {}
            Node::Tag(name) => {
                debug_assert!(!VOID_TAGS.contains(&name));
                next.tag_end(&name);
            }
            Node::List(mut list) => {
                list.finish(next);
            }
        }
    }

    /// The tag name for this node.
    pub(super) fn tag_name(&self) -> Option<&str> {
        match self {
            Node::Attribute => None,
            Node::Tag(name) => Some(name),
            Node::List(list) => list.tag_name(),
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
