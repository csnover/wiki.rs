//! The root of a Wikitext document.

use super::{
    Error, Result, State, StripMarker,
    emitters::{GrafEmitter, ListEmitter, ListKind, TextStyleEmitter},
    extension_tags,
    stack::StackFrame,
    surrogate::{self, Surrogate},
    tags::{self, PHRASING_TAGS},
    trim::Trim,
};
use crate::{
    common::decode_html,
    wikitext::{
        AnnoAttribute, Argument, FileMap, HeadingLevel, InclusionMode, LangFlags, LangVariant,
        MARKER_PREFIX, Output, Span, Spanned, TextStyle, Token, VOID_TAGS, builder::token,
    },
};
use core::fmt::{self, Write};
use either::Either;
use std::borrow::Cow;

/// The root of a Wikitext document.
#[derive(Debug, Default)]
pub(crate) struct Document {
    /// If true, this [`Document`] is used to render a document fragment rather
    /// than a complete document.
    fragment: bool,
    /// The line graf emitter.
    graf_emitter: GrafEmitter,
    /// The final rendered output.
    html: String,
    /// The stack of inclusion control tags.
    in_include: Vec<InclusionMode>,
    /// The last visible character rendered to the output.
    last_char: char,
    /// The stack of open HTML elements.
    stack: Vec<Node>,
    /// The template processing stack used to identify which template was the
    /// source of a fragment of the assembled Wikitext document.
    ///
    /// This is a workaround for templates that do not identify themselves for
    /// styling but instead only emit inline styles (like
    /// 'Template:Climate chart'), which need to have their styles overridden
    /// nevertheless, which we can do by adding extra data attributes to
    /// identify the template source of an element.
    tag_blocks: Vec<(usize, String)>,
    /// The [`TextStyle`] emitter.
    text_style_emitter: TextStyleEmitter,
}

impl Document {
    /// Creates a new [`Document`].
    pub(crate) fn new(fragment: bool) -> Self {
        Self {
            fragment,
            graf_emitter: <_>::default(),
            html: <_>::default(),
            in_include: <_>::default(),
            last_char: '\n',
            stack: <_>::default(),
            tag_blocks: <_>::default(),
            text_style_emitter: <_>::default(),
        }
    }

    /// Finalises the document and returns the resulting output.
    pub(crate) fn finish(mut self) -> Result<String> {
        self.text_style_emitter.finish(&mut self.html)?;

        for rest in self.stack.drain(..).rev() {
            rest.close(&mut self.html, &mut self.graf_emitter)?;
        }

        self.graf_emitter.finish(&mut self.html);

        Ok(self.html)
    }

    /// Finishes formatting a line of Wikitext.
    pub(crate) fn finish_line(&mut self) -> Result {
        self.text_style_emitter.finish(&mut self.html)?;
        self.graf_emitter.end_line(&mut self.html);
        self.last_char = '\n';
        Ok(())
    }

    /// Ends an HTML element with the given tag name and attributes.
    fn end_tag(&mut self, name: &str) -> Result<(), Error> {
        // TODO: Avoid ownership
        let name = Cow::Owned(name.to_ascii_lowercase());

        if VOID_TAGS.contains(&name) {
            self.graf_emitter.before_end_tag(&self.html, &name);
            self.graf_emitter.after_end_tag(&self.html, &name);
            return Ok(());
        } else if !PHRASING_TAGS.contains(&name) {
            self.last_char = ' ';
        }

        if let Some(pair) = self.stack.iter().rposition(|e| e.tag_name() == Some(&name)) {
            for e in self.stack.drain(pair..).rev() {
                e.close(&mut self.html, &mut self.graf_emitter)?;
            }
        } else {
            log::warn!("TODO: <{name}> tag mismatch requires error recovery logic");
            write!(self.html, "</{name}>")?;
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

        // Normally, receiving a new start tag should close any tags which cause
        // it to be in an invalid position in the DOM. This is especially
        // important for wikitable markup because wikitable children are
        // implicitly closed by the production of a new wikitable element.
        // However, there is one case where elements should be allowed to be
        // placed in an illegal position: when table-row templates get things
        // like 'Template:Tfd' applied to them, this will try to put non-table
        // content into the table—but this is actually desirable, because the
        // browser will automatically foster content in this position out of the
        // table. So, parenting-close rules are skipped if the last element is a
        // table or tr.
        let close_tags = !matches!(
            self.stack.last(),
            Some(node @ Node::Tag(last))
            if (last == "table" || last == "tr") && *last != tag && !node.can_parent(&tag));

        if close_tags {
            while let Some(e) = self.stack.pop_if(|e| !e.can_parent(&tag)) {
                // The transition from a wikitable caption directly into a table
                // cell requires extra recovery gymnastics to avoid walking too
                // far up the stack. 'Template:Football squad start' does this.
                let in_caption = matches!(e, Node::Tag(ref name) if name == "caption");
                e.close(&mut self.html, &mut self.graf_emitter)?;
                if in_caption && matches!(&*tag, "td" | "th") {
                    self.start_tag(state, sp, "tr", &[])?;
                }
            }
        }

        self.graf_emitter.before_start_tag(&self.html, &tag);
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

        if !PHRASING_TAGS.contains(&tag) {
            let mut has_some = false;
            // It is possible that a template starts in an ambiguous position
            // where the output of its first tag results in some other elements
            // being closed. To handle this case, `level` is treated as a
            // maximum which is reduced so child elements of the template do not
            // get tagged as it builds its own DOM tree.
            for (level, class) in self
                .tag_blocks
                .iter_mut()
                .rev()
                .take_while(|(level, _)| self.stack.len() <= *level)
            {
                *level = self.stack.len();
                if has_some {
                    write!(self.html, " ")?;
                } else {
                    write!(self.html, " data-wiki-rs=\"")?;
                    has_some = true;
                }
                write!(self.html, "{class}")?;
            }
            if has_some {
                write!(self.html, "\"")?;
            }
        }

        self.html.write_char('>')?;
        self.graf_emitter.after_start_tag(&self.html, &tag);
        if !VOID_TAGS.contains(&tag) {
            self.stack.push(Node::Tag(tag));
        } else if tag == "br" {
            self.last_char = '\n';
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

        let in_attr = matches!(self.stack.last(), Some(Node::Attribute));
        let in_code = in_attr
            || self
                .stack
                .iter()
                .rev()
                .any(|e| matches!(e, Node::Tag(tag) if is_code(tag)));

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

    /// Writes the contents of a strip marker to the output.
    fn write_strip_marker(&mut self, tag: &StripMarker) -> Result {
        match tag {
            StripMarker::NoWiki(text) => {
                self.text_run(&decode_html(text))?;
            }
            StripMarker::Inline(text) => {
                self.html += text;
            }
            StripMarker::Block(text) => {
                // Using "div" is a hack but one which does not really matter
                // since anything that cannot parent a `<div>` cannot parent any
                // other block-level element
                while let Some(e) = self.stack.pop_if(|e| !e.can_parent("div")) {
                    e.close(&mut self.html, &mut self.graf_emitter)?;
                }
                self.html += text;
            }
            StripMarker::WikiRsSourceStart(name) => {
                self.tag_blocks.push((self.stack.len(), name.clone()));
            }
            StripMarker::WikiRsSourceEnd(name) => {
                self.tag_blocks
                    .pop_if(|(_, other)| name == other)
                    .expect("tag block stack corruption");
            }
        }

        Ok(())
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
        match extension_tags::render_extension_tag(
            state,
            sp,
            Some(span),
            name,
            &extension_tags::InArgs::Wikitext(attributes),
            content,
        )? {
            Some(Either::Left(marker)) => {
                if self.fragment {
                    state.strip_markers.push(&mut self.html, name, marker);
                } else {
                    self.write_strip_marker(&marker)?;
                }
            }
            Some(Either::Right(_)) => todo!("this should never happen?"),
            None => {}
        }

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

        tags::render_runtime(self, state, sp, |_, source| {
            token!(
                source,
                Token::StartTag {
                    name: token!(source, Span { level.tag_name() }),
                    attributes: token![source, [ "id" => id ]].into(),
                    self_closing: false,
                }
            )
        })?;
        Trim::new(self, sp).adopt_tokens(state, sp, content)?;
        self.end_tag(level.tag_name())
    }

    fn adopt_horizontal_rule(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        _line_content: bool,
    ) -> Result {
        self.start_tag(state, sp, "hr", &[])?;
        self.end_tag("hr")
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
                e.close(&mut self.html, &mut self.graf_emitter)?;
            }

            self.graf_emitter.start_list(&mut self.html);
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
                e.close(&mut self.html, &mut self.graf_emitter)?;
            }

            // The parser removes the newlines between list items in order to
            // make it easier to disambiguate the list-terminating newline.
            // Since the list item must have ended at a newline, finish the line
            // now.
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
            Some(Node::Attribute) => {}
            Some(Node::List(list)) => {
                list.finish(&mut self.html)?;
                self.stack.pop();
                self.finish_line()?;
                self.graf_emitter.end_list();
            }
            None | Some(Node::Tag(_)) => {
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
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _name: &[Spanned<Token>],
        _default: Option<&[Spanned<Token>]>,
    ) -> Result {
        panic!("parameters should all be resolved by now");
    }

    fn adopt_redirect(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        target: &[Spanned<Token>],
        content: &[Spanned<Argument>],
        trail: Option<Spanned<&str>>,
    ) -> Result {
        let source = &mut String::new();
        let attributes = token! { source, [ "class" => "redirectText" ] };
        self.start_tag(
            state,
            &sp.clone_with_source(FileMap::new(source)),
            "p",
            &attributes,
        )?;
        tags::render_wikilink(self, state, sp, target, content, trail.map(|v| &**v))?;
        self.end_tag("p")?;
        Ok(())
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
        marker: &str,
    ) -> Result {
        let Some(tag) = state.strip_markers.get(marker) else {
            return Err(Error::StripMarker(marker.to_string()));
        };

        self.write_strip_marker(tag)?;

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
        self.text_style_emitter.emit(&mut self.html, style)?;
        Ok(())
    }

    fn adopt_table_caption(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        if !self
            .stack
            .iter()
            .rev()
            .any(|e| matches!(e, Node::Tag(tag) if tag == "table"))
        {
            self.start_tag(state, sp, "table", &[])?;
        }
        self.start_tag(state, sp, "caption", attributes)
    }

    fn adopt_table_data(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        if !self
            .stack
            .iter()
            .rev()
            .any(|e| matches!(e, Node::Tag(tag) if tag == "tr"))
        {
            if !self
                .stack
                .iter()
                .rev()
                .any(|e| matches!(e, Node::Tag(tag) if tag == "table"))
            {
                self.start_tag(state, sp, "table", &[])?;
            }
            self.start_tag(state, sp, "tr", &[])?;
        }
        self.start_tag(state, sp, "td", attributes)
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
    ) -> Result {
        if !self
            .stack
            .iter()
            .rev()
            .any(|e| matches!(e, Node::Tag(tag) if tag == "tr"))
        {
            if !self
                .stack
                .iter()
                .rev()
                .any(|e| matches!(e, Node::Tag(tag) if tag == "table"))
            {
                self.start_tag(state, sp, "table", &[])?;
            }
            self.start_tag(state, sp, "tr", &[])?;
        }
        self.start_tag(state, sp, "th", attributes)
    }

    fn adopt_table_row(
        &mut self,
        state: &mut State<'_>,
        sp: &StackFrame<'_>,
        _span: Span,
        attributes: &[Spanned<Argument>],
    ) -> Result {
        if !self
            .stack
            .iter()
            .rev()
            .any(|e| matches!(e, Node::Tag(tag) if tag == "table"))
        {
            self.start_tag(state, sp, "table", &[])?;
        }
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
        _state: &mut State<'_>,
        _sp: &StackFrame<'_>,
        _span: Span,
        _target: &[Spanned<Token>],
        _arguments: &[Spanned<Argument>],
    ) -> Result {
        panic!("templates should all be resolved by now");
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
    /// An HTML tag.
    Tag(Cow<'static, str>),
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
            Node::Tag(parent) => {
                if VOID_TAGS.contains(parent) {
                    panic!("void tag on element stack")
                } else if let Some(children) = PARENTS.get(parent) {
                    children.contains(&tag)
                } else if matches!(parent.as_ref(), "td" | "th" | "caption") {
                    !matches!(tag, "tr" | "td" | "th" | "caption")
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
    fn close(self, out: &mut String, graf_emitter: &mut GrafEmitter) -> fmt::Result {
        match self {
            Node::Attribute => {}
            Node::Tag(name) => {
                if !VOID_TAGS.contains(&name) {
                    graf_emitter.before_end_tag(out, &name);
                    write!(out, "</{name}>")?;
                    graf_emitter.after_end_tag(out, &name);
                }
            }
            Node::List(mut list) => {
                list.finish(out)?;
                graf_emitter.end_list();
            }
        }
        Ok(())
    }

    /// The tag name for this node.
    fn tag_name(&self) -> Option<&str> {
        match self {
            Node::Attribute => None,
            Node::Tag(name) => Some(name),
            Node::List(list) => list.stack.last().map(|kind| match kind {
                ListKind::Ordered | ListKind::Unordered => "li",
                ListKind::Term => "dt",
                ListKind::Detail => "dd",
            }),
        }
    }
}

/// Tags with restricted allowable children.
static PARENTS: phf::Map<&str, &[&str]> = phf::phf_map! {
    "table" => &["caption", "tr"],
    "tr" => &["td", "th"],
    "dl" => &["dd", "dt"],
    "ol" => &["li"],
    "ul" => &["li"]
};
