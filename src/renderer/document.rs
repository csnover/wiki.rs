//! The root of a Wikitext document.

use super::{
    Error, Result, State, StripMarker, extension_tags,
    manager::RenderOutput,
    stack::{Kv, StackFrame},
    surrogate::{self, Surrogate},
    tags, template,
    trim::Trim,
};
use crate::{
    renderer::emitters::{ListEmitter, ListKind, TextStyleEmitter},
    wikitext::{
        AnnoAttribute, Argument, HeadingLevel, InclusionMode, LangFlags, LangVariant,
        MARKER_PREFIX, Output, Span, Spanned, TextStyle, Token, VOID_TAGS, builder::token,
    },
};
use core::fmt::{self, Write};
use either::Either;
use std::borrow::Cow;

/// The root of a Wikitext document.
#[derive(Debug, Default)]
pub struct Document {
    /// The final rendered output.
    pub(super) html: String,
    /// The stack of inclusion control tags.
    in_include: Vec<InclusionMode>,
    /// The last visible character rendered to the output.
    last_char: char,
    /// A hack.
    fragment: bool,
    /// A hack.
    seen_block: bool,
    /// The stack of open HTML elements.
    stack: Vec<Node>,
    /// Tags for block level elements emitted at a given stack level.
    ///
    /// This is a workaround for templates that do not identify themselves for
    /// styling but instead only emit inline styles (like
    /// 'Template:Climate chart'), which need to have their styles overridden
    /// nevertheless.
    ///
    /// This currently relies on templates sending through
    /// [`Self::adopt_output`], and because expanded templates get erased (due
    /// to reparsing) before they make it to `Document` this will not tag blocks
    /// that were inside nested templates.
    tag_blocks: Vec<(usize, String)>,
    /// The [`TextStyle`] emitter.
    text_style_emitter: TextStyleEmitter,
}

impl Document {
    /// Creates a new [`Document`].
    pub(crate) fn new(fragment: bool) -> Self {
        Self {
            fragment,
            html: <_>::default(),
            in_include: <_>::default(),
            last_char: ' ',
            seen_block: <_>::default(),
            stack: <_>::default(),
            tag_blocks: <_>::default(),
            text_style_emitter: <_>::default(),
        }
    }

    /// Finalises the document and returns the resulting output.
    pub(crate) fn finish(mut self, state: State<'_>) -> RenderOutput {
        for rest in self.stack.drain(..).rev() {
            let _ = rest.close(&mut self.html);
        }

        let mut timings = state.timing.into_iter().collect::<Vec<_>>();
        timings.sort_by(|(_, (_, a)), (_, (_, b))| b.cmp(a));
        for (the_baddie, (count, time)) in timings {
            log::trace!("{the_baddie}: {count} / {}s", time.as_secs_f64());
        }

        // Clippy: If memory usage is ever >2**52, something sure happened.
        #[allow(clippy::cast_precision_loss)]
        {
            log::debug!(
                "Caches:\n  Database: {:.2}KiB\n  Template: {:.2}KiB\n  VM: {:.2}KiB",
                (state.statics.db.cache_size() as f64) / 1024.0,
                (state.statics.template_cache.memory_usage() as f64) / 1024.0,
                (state.statics.vm_cache.memory_usage() as f64) / 1024.0,
            );
        }

        state
            .globals
            .categories
            .finish(&mut self.html, state.statics.base_uri.path())
            .unwrap();

        RenderOutput {
            content: self.html,
            indicators: state.globals.indicators,
            outline: state.globals.outline,
            styles: state.globals.styles.text,
        }
    }

    /// Finalises a document fragment and returns the resulting output as a
    /// strip marker object.
    pub(crate) fn finish_fragment(mut self) -> StripMarker {
        for rest in self.stack.drain(..).rev() {
            let _ = rest.close(&mut self.html);
        }

        if self.seen_block {
            StripMarker::Block(self.html)
        } else {
            StripMarker::Inline(self.html)
        }
    }

    /// Finishes formatting a line of Wikitext.
    pub(crate) fn finish_line(&mut self) -> Result {
        self.text_style_emitter.finish(&mut self.html)?;

        // Paragraph rules:
        //
        // 1. Any multiple of two sequential newlines break a graf;
        // 2. A newline after a break emits a `<br>` into the new graf;
        // 3. When a graf is broken, the next element may be a non-graf, so it
        //    should not emit the new tag straight away.
        if let Some(Node::Graf(graf)) = self.stack.last_mut() {
            match graf {
                Graf::Start => {
                    write!(self.html, "<br>")?;
                    *graf = Graf::Break;
                }
                Graf::Text => *graf = Graf::Break,
                Graf::Break => {
                    writeln!(self.html, "</p>")?;
                    *graf = Graf::AfterBreak;
                }
                Graf::AfterBreak => {
                    write!(self.html, "<p><br>")?;
                    *graf = Graf::Break;
                }
            }
        }

        self.last_char = '\n';

        Ok(())
    }

    /// Ends an HTML element with the given tag name and attributes.
    fn end_tag(&mut self, name: &str) -> Result<(), Error> {
        // TODO: Avoid ownership
        let name = Cow::Owned(name.to_ascii_lowercase());

        if VOID_TAGS.contains(&name) {
            return Ok(());
        } else if !PHRASING_TAGS.contains(&name) {
            self.last_char = ' ';
        }

        if let Some(pair) = self.stack.iter().rposition(|e| e.tag_name() == Some(&name)) {
            for e in self.stack.drain(pair..).rev() {
                e.close(&mut self.html)?;
            }
        } else {
            log::warn!("TODO: <{name}> tag mismatch requires error recovery logic");
            write!(self.html, "</{name}>")?;
        }

        Ok(())
    }

    /// Updates the stack for a run of paragraph text.
    fn expect_graf(&mut self) -> Result {
        if let Some(Node::Graf(graf)) = self.stack.last_mut() {
            match graf {
                Graf::AfterBreak => {
                    write!(self.html, "<p>")?;
                    *graf = Graf::Text;
                }
                Graf::Text => {}
                Graf::Start | Graf::Break => *graf = Graf::Text,
            }
        } else if self.needs_graf() {
            write!(self.html, "<p>")?;
            self.stack.push(Node::Graf(Graf::Start));
        }

        Ok(())
    }

    /// Starts a new HTML element with the given tag name and attributes.
    fn start_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        tag: &str,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        // TODO: Avoid ownership
        let tag = Cow::Owned(tag.to_ascii_lowercase());

        // Sometimes, table-row templates get things like 'Template:Tfd'
        // applied to them. When this happens, allow the browser to foster the
        // content out of the table. However, also make sure that if a new
        // table or table row starts that it interacts properly with the table.
        if !matches!(
            self.stack.last(),
            Some(node @ Node::Tag(last, _))
            if (last == "table" || last == "tr") && *last != tag && !node.can_parent(&tag)
        ) {
            while let Some(e) = self.stack.pop_if(|e| !e.can_parent(&tag)) {
                e.close(&mut self.html)?;
            }
        }

        if PHRASING_TAGS.contains(&tag) {
            self.expect_graf()?;
        } else {
            self.seen_block = true;
            if let Some(Node::Tag(_, has_content)) = self.stack.last_mut() {
                *has_content = true;
            }
        }

        write!(self.html, "<{tag}")?;
        if !attributes.is_empty() {
            self.stack.push(Node::Attribute);
            for attribute in attributes {
                self.html.write_char(' ')?;
                tags::render_attribute(
                    self,
                    state,
                    sp,
                    attribute.name().map(Either::Right),
                    Either::Right(attribute.value()),
                )?;
            }
            self.stack
                .pop_if(|e| matches!(e, Node::Attribute))
                .expect("element stack corruption");
        }

        // It is possible that a template starts in an ambiguous position where
        // the output of its first tag results in some other elements being
        // closed. To handle this case, `level` is treated as a maximum which
        // is reduced so child elements of the template do not get tagged as it
        // builds its own DOM tree.
        if !PHRASING_TAGS.contains(&tag)
            && let Some((level, class)) = self.tag_blocks.last_mut()
            && self.stack.len() <= *level
        {
            *level = self.stack.len();
            write!(self.html, r#" data-wiki-rs="{class}""#)?;
        }

        self.html.write_char('>')?;
        if !VOID_TAGS.contains(&tag) {
            self.stack.push(Node::Tag(tag, false));
        }
        Ok(())
    }

    /// Writes a run of text, also converting wretched typewriter quote marks to
    /// beautiful works of fine typographical art, as we are not savages.
    fn text_run(&mut self, text: &str) -> Result {
        fn is_break(prev: char, next: Option<char>) -> bool {
            use unicode_general_category::{
                GeneralCategory::{InitialPunctuation, OpenPunctuation},
                get_general_category,
            };
            prev.is_whitespace()
                || (matches!(
                    get_general_category(prev),
                    OpenPunctuation | InitialPunctuation
                ) && !next.is_some_and(char::is_whitespace))
        }

        fn is_code(tag: &str) -> bool {
            matches!(tag, "code" | "kbd" | "pre" | "samp" | "var")
        }

        assert!(
            self.fragment || !text.contains(MARKER_PREFIX),
            "strip marker got into text"
        );
        self.expect_graf()?;

        let in_attr = matches!(self.stack.last(), Some(Node::Attribute));
        let in_code = in_attr
            || self
                .stack
                .iter()
                .rev()
                .any(|e| matches!(e, Node::Tag(tag, _) if is_code(tag)));

        let mut prev = self.last_char;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' if !in_code => {
                    self.html
                        .write_char(if is_break(prev, chars.peek().copied()) {
                            '“'
                        } else {
                            '”'
                        })?;
                }
                '\'' if !in_code => {
                    self.html
                        .write_char(if is_break(prev, chars.peek().copied()) {
                            '‘'
                        } else {
                            '’'
                        })?;
                }
                '<' => self.html += "&lt;",
                '>' => self.html += "&gt;",
                '&' => self.html += "&amp;",
                c => self.html.write_char(c)?,
            }
            prev = c;
        }
        if !in_attr {
            self.last_char = prev;
        }

        Ok(())
    }

    /// Writes a complete HTML element.
    fn write_element(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        tag: &str,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.start_tag(state, sp, tag, attributes)?;
        Trim::new(self, sp).adopt_tokens(state, sp, content)?;
        self.end_tag(tag)
    }

    /// Returns true if a `<p>` needs to be added for text within the given
    /// `parent`.
    fn needs_graf(&self) -> bool {
        // List items mustn’t receive graf tags at first because this breaks
        // at least the layout of 'Template:Navbox'. Similarly, they cannot be
        // given automatically in <div> because that breaks the
        // header of 'Template:Documentation'. Basically, the expected Wikitext
        // output is all so fragile that there is no way to emit grafs in a sane
        // way, it pretty much always needs need to be the case that the first run
        // of text in a block is emitted raw, which makes having good layout for
        // text really difficult because having phrasing content directly inside
        // blocks means a general selector is impossible.
        let parent = self.stack.last();
        (!self.fragment && parent.is_none())
            || matches!(parent, Some(Node::Tag(tag, true)) if !PHRASING_TAGS.contains(tag))
    }
}

impl fmt::Write for Document {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.html.write_str(s)
    }
}

impl Surrogate<Error> for Document {
    fn adopt_autolink(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.adopt_external_link(
            state,
            sp,
            span,
            target,
            // autourl have empty content, other magic links have generated
            // content
            if content.is_empty() { target } else { content },
        )
    }

    fn adopt_attribute(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        name: Option<Either<&str, &[Spanned<Token>]>>,
        value: Either<&str, &[Spanned<Token>]>,
    ) -> Result {
        self.stack.push(Node::Attribute);
        let result = tags::render_attribute(self, state, sp, name, value);
        self.stack
            .pop_if(|e| matches!(e, Node::Attribute))
            .expect("element stack corruption");
        result
    }

    fn adopt_behavior_switch(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
    ) -> Result {
        log::warn!("TODO: BehaviorSwitch __{name}__");
        Ok(())
    }

    fn adopt_comment(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _content: &str,
        _unclosed: bool,
    ) -> Result {
        // TODO: Is there actually any reason to do this?
        // write!(self.html, "<!-- {content} -->")?;
        Ok(())
    }

    fn adopt_end_annotation(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        name: &str,
    ) -> Result {
        log::warn!("TODO: EndAnnotation: {name}");
        Ok(())
    }

    fn adopt_end_include(
        &mut self,
        _state: &mut State<'_>,
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
        _state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
    ) -> Result {
        if self
            .stack
            .iter()
            .rev()
            .any(|e| matches!(e, Node::Attribute))
        {
            log::error!("tag inside attribute (probably due to `render_runtime`)");
            return self.text_run(&sp.source[span.into_range()]);
        }

        self.end_tag(name)?;
        Ok(())
    }

    fn adopt_entity(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        value: char,
    ) -> Result {
        self.expect_graf()?;

        match value {
            '<' => self.html += "&lt;",
            '>' => self.html += "&gt;",
            '&' => self.html += "&amp;",
            '"' => self.html += "&quot;",
            c => self.html.write_char(c)?,
        }
        if !matches!(self.stack.last(), Some(Node::Attribute)) {
            self.last_char = value;
        }
        Ok(())
    }

    fn adopt_extension(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        content: Option<&str>,
    ) -> Result {
        let attributes = attributes.iter().map(Kv::Argument).collect::<Vec<_>>();
        extension_tags::render_extension_tag(
            self,
            state,
            sp,
            Some(span),
            name,
            &attributes,
            content,
        )?;
        Ok(())
    }

    fn adopt_external_link(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.expect_graf()?;
        tags::render_external_link(self, state, sp, target, content)
    }

    fn adopt_generated(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Option<Span>,
        text: &str,
    ) -> Result {
        self.text_run(text)?;
        Ok(())
    }

    fn adopt_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result {
        let id = state
            .globals
            .outline
            .push(&sp.source, span, level, content)?;

        tags::render_runtime(self, state, sp, |source| {
            token!(
                source,
                Token::StartTag {
                    name: token!(source, Span { level.tag_name() }),
                    attributes: token![source, [ "id" => id ]].into(),
                    self_closing: false,
                }
            )
        })?;
        self.adopt_tokens(state, sp, content)?;
        self.end_tag(level.tag_name())
    }

    fn adopt_horizontal_rule(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        _line_content: bool,
    ) -> Result {
        self.write_element(state, sp, "hr", &[], &[])
    }

    fn adopt_lang_variant(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _flags: Option<&LangFlags>,
        _variants: &[Spanned<LangVariant>],
        _raw: bool,
    ) -> Result {
        log::warn!("TODO: LangVariant");
        Ok(())
    }

    fn adopt_link(
        &mut self,
        state: &mut State<'_>,
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
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        bullets: &str,
        content: &[Spanned<Token>],
    ) -> Result {
        if let Some(Node::List(list)) = self.stack.last_mut() {
            list.emit(&mut self.html, bullets)?;
        } else {
            // Using "ol" is a hack but one which does not really matter since
            // anything that cannot parent an `<ol>` cannot parent any of the
            // other list kinds either
            while let Some(e) = self.stack.pop_if(|e| !e.can_parent("ol")) {
                e.close(&mut self.html)?;
            }

            let mut list = ListEmitter::default();
            list.emit(&mut self.html, bullets)?;
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
                e.close(&mut self.html)?;
            }

            // The parser removes the newlines between list items in order to make
            // it easier to disambiguate the list-terminating newline. Since the
            // list item must have ended at a newline, finish the line now.
            self.finish_line()?;
        }

        Ok(())
    }

    fn adopt_new_line(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        match self.stack.last_mut() {
            None | Some(Node::Attribute) => {}
            Some(Node::List(list)) => {
                list.finish(&mut self.html)?;
                self.stack.pop();
            }
            Some(Node::Graf(_) | Node::Tag(_, _)) => {
                self.finish_line()?;
            }
        }
        Ok(())
    }

    fn adopt_output(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        output: &Output,
    ) -> Result {
        if output.has_onlyinclude {
            self.in_include.push(InclusionMode::OnlyInclude);
        }
        let class_name = sp.parent.is_some().then(|| {
            sp.name
                .text()
                .to_ascii_lowercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
        });
        if let Some(class_name) = &class_name {
            self.tag_blocks.push((self.stack.len(), class_name.clone()));
        }
        let result = self.adopt_tokens(state, sp, &output.root);
        if let Some(class_name) = class_name {
            self.tag_blocks
                .pop_if(|(_, name)| *name == class_name)
                .expect("tag block stack corruption");
        }
        if output.has_onlyinclude {
            self.in_include
                .pop_if(|i| *i == InclusionMode::OnlyInclude)
                .expect("include stack corruption");
        }
        result
    }

    fn adopt_parameter(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &[Spanned<Token>],
        default: Option<&[Spanned<Token>]>,
    ) -> Result {
        template::render_parameter(self, state, sp, span, name, default)
    }

    fn adopt_redirect(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _target: &[Spanned<Token>],
        _content: &[Spanned<Argument>],
        _trail: Option<Spanned<&str>>,
    ) -> Result {
        panic!("seeing a redirect should be impossible; was there too much redirection?");
    }

    fn adopt_start_annotation(
        &mut self,
        _state: &mut State<'_>,
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
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        mode: InclusionMode,
    ) -> Result {
        self.in_include.push(mode);
        Ok(())
    }

    fn adopt_start_tag(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        name: &str,
        attributes: &[Spanned<Argument>],
        self_closing: bool,
    ) -> Result {
        if self
            .stack
            .iter()
            .rev()
            .any(|e| matches!(e, Node::Attribute))
        {
            log::error!("tag inside attribute (probably due to `render_runtime`)");
            return self.text_run(&sp.source[span.into_range()]);
        }

        self.start_tag(state, sp, name, attributes)?;
        if self_closing {
            self.end_tag(name)?;
        }
        Ok(())
    }

    fn adopt_strip_marker(
        &mut self,
        state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        marker: usize,
    ) -> Result {
        let Some(tag) = state.strip_markers.get(marker) else {
            return Err(Error::StripMarker(marker));
        };

        match tag {
            StripMarker::Inline(text) => {
                self.html += text;
            }
            StripMarker::Block(text) => {
                // Using "div" is a hack but one which does not really matter
                // since anything that cannot parent a `<div>` cannot parent any
                // other block-level element
                while let Some(e) = self.stack.pop_if(|e| !e.can_parent("div")) {
                    e.close(&mut self.html)?;
                }
                self.html += text;
            }
        }

        Ok(())
    }

    fn adopt_text(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        text: &str,
    ) -> Result {
        self.text_run(text)
    }

    fn adopt_text_style(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        style: TextStyle,
    ) -> Result {
        self.expect_graf()?;
        self.text_style_emitter.emit(&mut self.html, style)?;
        Ok(())
    }

    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        self.write_element(state, sp, "caption", attributes, content)
    }

    fn adopt_table_data(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        if !matches!(self.stack.last(), Some(Node::Tag(tag, _)) if tag == "tr") {
            self.start_tag(state, sp, "tr", &[])?;
        }
        self.write_element(state, sp, "td", attributes, content)
    }

    fn adopt_table_end(
        &mut self,
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
    ) -> Result {
        self.end_tag("table")
    }

    fn adopt_table_heading(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
        content: &[Spanned<Token>],
    ) -> Result {
        if !matches!(self.stack.last(), Some(Node::Tag(tag, _)) if tag == "tr") {
            self.start_tag(state, sp, "tr", &[])?;
        }
        self.write_element(state, sp, "th", attributes, content)
    }

    fn adopt_table_row(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.start_tag(state, sp, "tr", attributes)
    }

    fn adopt_table_start(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        self.start_tag(state, sp, "table", attributes)
    }

    fn adopt_template(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        span: Span,
        target: &[Spanned<Token>],
        arguments: &[Spanned<Argument>],
    ) -> Result {
        template::render_template(self, state, sp, span, target, arguments)
    }

    fn adopt_token(
        &mut self,
        state: &mut State<'_>,
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
enum Node {
    /// A paragraph.
    Graf(Graf),
    /// An HTML tag.
    Tag(Cow<'static, str>, bool),
    /// A run of Wikitext list items.
    List(ListEmitter),
    /// An HTML attribute.
    Attribute,
}

impl Node {
    /// Whether this element can parent the element with the given lowercase tag
    /// name.
    fn can_parent(&self, tag: &str) -> bool {
        match self {
            Node::Graf(_) => PHRASING_TAGS.contains(tag),
            Node::Tag(parent, _) => {
                if VOID_TAGS.contains(parent) {
                    panic!("void tag on element stack")
                } else if let Some(children) = PARENTS.get(parent) {
                    children.contains(&tag)
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
                list.stack.last().is_some()
            }
            Node::Attribute => unreachable!(),
        }
    }

    /// Writes the terminator for this element to the given output.
    fn close<W: fmt::Write + ?Sized>(self, out: &mut W) -> fmt::Result {
        match self {
            Node::Attribute | Node::Graf(Graf::AfterBreak) => Ok(()),
            Node::Graf(graf) => {
                if matches!(graf, Graf::AfterBreak) {
                    Ok(())
                } else {
                    write!(out, "</p>")
                }
            }
            Node::Tag(name, _) => {
                if VOID_TAGS.contains(&name) {
                    Ok(())
                } else {
                    write!(out, "</{name}>")
                }
            }
            Node::List(mut list) => list.finish(out),
        }
    }

    /// The tag name for this node.
    fn tag_name(&self) -> Option<&str> {
        match self {
            Node::Attribute => None,
            Node::Graf(_) => Some("p"),
            Node::Tag(name, _) => Some(name),
            Node::List(list) => list.stack.last().map(|kind| match kind {
                ListKind::Ordered | ListKind::Unordered => "li",
                ListKind::Term => "dt",
                ListKind::Detail => "dd",
            }),
        }
    }
}

/// A paragraph.
#[derive(Debug)]
enum Graf {
    /// The paragraph just started.
    Start,
    /// The paragraph has content.
    Text,
    /// The paragraph should break if another newline is received.
    Break,
    /// The paragraph should restart if more content is received.
    AfterBreak,
}

/// Tags with restricted allowable children.
static PARENTS: phf::Map<&str, &[&str]> = phf::phf_map! {
    "table" => &["caption", "tr"],
    "tr" => &["td", "th"],
    "dl" => &["dd", "dt"],
    "ol" => &["li"],
    "ul" => &["li"]
};

/// Phrasing content, per the HTML5 specification.
static PHRASING_TAGS: phf::Set<&str> = phf::phf_set! {
    "a", "abbr", "area", "audio", "b", "bdi", "bdo", "br", "button", "canvas",
    "cite", "code", "data", "datalist", "del", "dfn", "em", "embed", "i",
    "iframe", "img", "input", "ins", "kbd", "label", "link", "map", "mark",
    "math", "meta", "meter", "noscript", "object", "output", "picture",
    "progress", "q", "ruby", "s", "samp", "script", "selectedcontent", "slot",
    "small", "span", "strong", "sub", "sup", "svg", "template", "textarea",
    "time", "u", "var", "video", "wbr"
};
