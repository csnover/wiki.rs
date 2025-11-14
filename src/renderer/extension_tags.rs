//! Code for handling MediaWiki extension tags.

// Clippy: Methods are implementing an interface which is invisible to clippy.
#![allow(clippy::unnecessary_wraps)]

mod timeline;

use super::{
    Error, Result, State, WriteSurrogate,
    document::Document,
    stack::{IndexedArgs, KeyCacheKvs, Kv, StackFrame},
    surrogate::Surrogate as _,
};
use crate::{
    common::anchor_encode,
    db::Database,
    php::strtr,
    renderer::tags::{render_runtime, render_runtime_list},
    title::{Namespace, Title},
    wikitext::{
        self, FileMap, Output, Span, Token,
        builder::{tok_arg, token},
    },
};
use core::{fmt::Write as _, ops::Range};
use std::collections::{HashMap, HashSet};

/// A helper struct for passing arguments required by all extension tags.
struct ExtensionTag<'args, 'call, 'sp> {
    /// The attributes of the extension tag.
    arguments: IndexedArgs<'args, 'call, 'sp>,
    /// The raw body text of the extension tag, if one existed in the source
    /// text.
    body: Option<&'call str>,
}

impl<'args, 'call, 'sp> core::ops::Deref for ExtensionTag<'args, 'call, 'sp> {
    type Target = IndexedArgs<'args, 'call, 'sp>;

    fn deref(&self) -> &Self::Target {
        &self.arguments
    }
}

impl ExtensionTag<'_, '_, '_> {
    /// Returns the unevaluated body of the tag as a string.
    pub fn body(&self) -> &str {
        self.body.unwrap_or("")
    }

    /// Evaluates the body of the tag.
    pub fn eval_body(&self, state: &mut State<'_>) -> Result<String> {
        // TODO: Do this more intelligently to allow returning `Cow`
        let source = FileMap::new(self.body());
        let tt = state
            .statics
            .parser
            .parse(&source, self.sp.parent.is_some())?;
        Ok(self
            .sp
            .clone_with_source(source)
            .eval(state, &tt.root)?
            .to_string())
    }

    /// Returns the body of the tag as a token tree.
    pub fn parse_body(&self, state: &mut State<'_>) -> Result<(StackFrame<'_>, Output)> {
        let sp = self.sp.clone_with_source(FileMap::new(self.body()));
        state
            .statics
            .parser
            .parse(&sp.source, self.sp.parent.is_some())
            .map(|tree| (sp, tree))
            .map_err(Into::into)
    }
}

/// The `<math>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Math>
fn math(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    let latex = arguments.body();

    // TODO: Undocumented 'id' attribute

    // TODO: This might not be accurate enough to MW.
    // In MW:
    // * 'block' wraps "{\displaystyle{latex}}"
    // * 'inline' wraps "{\textstyle{latex}}"
    // * 'linebreak' is also an option, wraps "\[{latex}\]"
    let mode = if let Some(value) = arguments.get(state, "display")?
        && value == "block"
    {
        math_core::MathDisplay::Block
    } else {
        math_core::MathDisplay::Inline
    };

    match math_core::LatexToMathML::const_default().convert_with_local_counter(latex, mode) {
        Ok(maths) => {
            out.write_str(&wikitext::escape(&maths))?;
        }
        Err(err) => {
            write!(
                out,
                r#"<span class="error texerror">{}</span>"#,
                wikitext::escape_no_wiki(&err.to_string())
            )?;
        }
    }

    Ok(())
}

/// The `<indicator>` extension tag.
/// <https://www.mediawiki.org/wiki/Help:Page_status_indicators>
fn indicator(
    _: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    let Some(name) = arguments.get(state, "name")? else {
        return Ok(());
    };

    let (sp, body) = arguments.parse_body(state)?;
    let image = body
        .root
        .iter()
        .find_map(|token| {
            #[rustfmt::skip]
            let Token::Link { target, .. } = &token.node else {
                return None;
            };

            let target = match sp.eval(state, target) {
                Ok(target) => Title::new(&target, None),
                Err(err) => return Some(Err(err)),
            };

            (target.namespace().id == Namespace::FILE).then_some(Ok(token))
        })
        .transpose()?;

    if let Some(image) = image {
        let mut out = Document::new(true);
        out.adopt_token(state, &sp, image)?;
        state.globals.indicators.insert(name.to_string(), out.html);
    }

    Ok(())
}

/// The `<nowiki>` extension tag.
fn no_wiki(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    // TODO: This is supposed to have a way to communicate to the caller that
    // it is nowiki content, so it does not get parsed as Wikitext later. Which
    // characters are being escaped is based on MW expecting that this content
    // would be used in that manner. Currently we just store it and YOLO into
    // the final document.
    render_runtime(out, state, arguments.sp, |source| {
        token!(source, Token::Text { strtr(arguments.body(), &[
            ("-{", "-&#123;"),
            ("}-", "&#125;-"),
            ("<", "&lt;"),
            (">", "&gt;"),
        ])})
    })
}

/// The `<pre>` extension tag.
fn pre(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    let sp = arguments.sp;
    render_runtime_list(out, state, sp, |state, source| {
        token![source, [
            Token::StartTag {
                name: token!(source, Span { "pre" }),
                attributes: {
                    // TODO: This is supposed to strip markers and use a
                    // whitelist of valid attribute names.
                    arguments.iter()
                        .map(|kv| tok_arg(
                            source,
                            kv.name(state, sp).unwrap().unwrap(),
                            kv.value(state, sp).unwrap()
                        ))
                        .collect::<Vec<_>>()
                },
                self_closing: false,
            },
            // '"' must be unescaped for strip markers;
            // '&' must be unescaped for entities
            // TODO: This is also supposed to replace `<nowiki>(.*)</nowiki>` by
            // `$1`
            Token::Text { strtr(arguments.body(), &[(">", "&gt;"), ("<", "&lt;")]) },
            Token::EndTag {
                name: token!(source, Span { "pre" })
            }
        ]]
        .into()
    })
}

/// Stored citation references.
#[derive(Debug, Default)]
pub struct References {
    /// Bump allocation of reference text.
    text: String,
    /// References in a group. Value is a map of ranges into `text`. For
    /// compatibility, the default group is an empty string.
    groups: HashMap<String, Vec<(usize, Range<usize>)>>,
    /// Named references. Key is `(group, name)`. Value is index into
    /// `groups[GroupKey]`.
    named: HashMap<(String, String), usize>,
    /// Global reference counter. Used for generating unique IDs.
    count: usize,
}

impl References {
    /// Appends text to a named reference, separated by a single space. If the
    /// reference does not already exist, it is created.
    fn append_named(&mut self, name: (String, String), value: &str) {
        let group = self.groups.entry(name.0.clone()).or_default();

        let index = self.named.entry(name).or_insert(group.len());

        if let Some((_, range)) = group.get_mut(*index) {
            if range.end == self.text.len() {
                if !Range::is_empty(range) {
                    self.text.push(' ');
                }
                self.text += value;
                range.end = self.text.len();
            } else {
                let old_range = range.clone();
                range.start = self.text.len();
                if !old_range.is_empty() {
                    self.text.extend_from_within(old_range);
                    self.text.push(' ');
                }
                self.text += value;
                range.end = self.text.len();
            }
        } else {
            let range = self.text.len()..(self.text.len() + value.len());
            self.text += value;
            self.count += 1;
            group.push((self.count, range));
        }
    }

    /// Adds a named reference with the given text. If the reference already
    /// exists and contains text, this call does nothing. Returns the
    /// page-unique numeric ID of the reference.
    fn insert_named(&mut self, name: (String, String), value: &str) -> usize {
        if let Some(index) = self.named.get(&name) {
            // TODO: 'cite_error_references_duplicate_key'
            let &(id, ref range) = &self.groups[&name.0][*index];

            // Some pages like 'Wikidata' create empty named refs and then
            // populate the data later
            if range.is_empty() && !value.is_empty() {
                self.append_named(name, value);
            }

            return id;
        }

        let range = self.text.len()..(self.text.len() + value.len());
        self.text += value;

        let group = self.groups.entry(name.0.clone()).or_default();
        let index = group.len();
        self.named.insert(name, index);
        self.count += 1;
        group.push((self.count, range));
        self.count
    }

    /// Adds an named reference with the given text. Returns the page-unique
    /// numeric ID of the reference, or `None` if there was no text to add.
    fn insert_unnamed(&mut self, group: String, value: &str) -> usize {
        let range = self.text.len()..(self.text.len() + value.len());
        self.text += value;

        let group = self.groups.entry(group).or_default();
        self.count += 1;
        group.push((self.count, range));
        self.count
    }

    /// Returns an iterator over the references in the given group.
    fn iter_group(&self, group: &String) -> Option<impl Iterator<Item = (usize, &str)>> {
        self.groups.get(group).map(|group| {
            group
                .iter()
                .map(|(id, range)| (*id, &self.text[range.clone()]))
        })
    }
}

/// The `<ref>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Cite>
fn r#ref(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    // Due to transclusion it is necessary to render immediately instead of
    // storing the node list for later, since rendering later would require
    // retaining the stack frame too
    let reference = arguments.eval_body(state)?;

    let group = arguments
        .get(state, "group")?
        .as_deref()
        .map_or(<_>::default(), ToString::to_string);

    if let Some(follow) = arguments.get(state, "follow")? {
        state
            .globals
            .references
            .append_named((group, follow.to_string()), &reference);
        return Ok(());
    }

    let id = if let Some(name) = arguments.get(state, "name")? {
        Some(
            state
                .globals
                .references
                .insert_named((group.clone(), name.to_string()), &reference),
        )
    } else if !reference.is_empty() {
        Some(
            state
                .globals
                .references
                .insert_unnamed(group.clone(), &reference),
        )
    } else {
        None
    };

    if let Some(id) = id {
        // TODO: Avoid intermediates.
        let anchor = anchor_encode(&format!("cite_ref-{id}"));
        let source = format!(r#"<span class="reference" id="{anchor}">[[#ref_{id}|{id}]]</span>"#);
        let references = state.statics.parser.parse_no_expansion(&source)?;
        out.adopt_output(
            state,
            &arguments.sp.clone_with_source(FileMap::new(&source)),
            &references,
        )?;
    }

    Ok(())
}

/// The `<references>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Cite>
fn references(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    // Here, someone vibed the idea that the references tag -- which is supposed
    // to be an output -- should also accept content, which we must now evaluate
    // purely for the side effect that someone shoved a `<ref>` in there. This
    // is codified behaviour by 'Template:Reflist' having a `refs` property, and
    // is used on the page 'Donkey' (probably among thousands of others).
    // The Cite extension actually maintains a stack, which feels concerning to
    // me, since it means someone thought about what happens when you have refs
    // inside refs, and that sounds like a cursed thing to have to think about.
    if arguments.body.is_some() {
        arguments.eval_body(state)?;
    }

    let group = arguments
        .get(state, "group")?
        .as_deref()
        .map_or(<_>::default(), ToString::to_string);

    // TODO: For multiple references to the same name, there should be backrefs
    // to all of them, not just the first one.
    // TODO: Avoid accumulating before writing. This is needed only because
    // there is no convenient API for creating trees by hand and the text from
    // the `<ref>` is still partially Wikitext.
    let source = if let Some(refs) = state.globals.references.iter_group(&group) {
        let mut source = String::from(r#"<ol class="references">"#);
        for (id, text) in refs {
            if !text.is_empty() {
                let anchor = anchor_encode(&format!("ref_{id}"));
                write!(
                    source,
                    r#"<li id="{anchor}" class="mw-cite-backlink">[[#cite_ref-{id}|^]] {text}</li>"#
                )?;
            }
        }
        source.write_str("</ol>")?;
        Some(source)
    } else {
        None
    };

    if let Some(source) = source {
        let references = state.statics.parser.parse_no_expansion(&source)?;
        out.adopt_output(
            state,
            &arguments.sp.clone_with_source(FileMap::new(&source)),
            &references,
        )?;
    }

    Ok(())
}

/// Stored ranges for labelled section transclusion.
///
/// These are not currently used for anything; Lua modules which perform
/// transclusion sniff these tags themselves.
#[derive(Debug, Default)]
pub struct LabelledSections {
    /// A map from an article title to a map of the ranges of labelled sections
    /// within the article.
    titles: HashMap<String, HashMap<String, Range<usize>>>,
}

/// The `<section>` extension tag.
/// <https://en.wikipedia.org/wiki/Help:Labeled_section_transclusion>
fn section(
    _: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    // `{{#tag: ... }}` may have no bounds if it was invoked from a script for
    // some reason
    let Some(bounds) = arguments.span else {
        return Ok(());
    };

    let begin = arguments.get(state, "begin")?;
    let end = arguments.get(state, "end")?;

    let title = state
        .globals
        .sections
        .titles
        .entry(arguments.sp.name.key().to_string())
        .or_default();

    if let Some(name) = begin {
        title.insert(name.to_string(), bounds.into_range());
    }

    if let Some(name) = end
        && let Some(section) = title.get_mut(&*name)
    {
        section.end = bounds.start;
    }

    Ok(())
}

/// The `<syntaxhighlight>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Syntaxhighlight>
fn syntax_highlight(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    let tag = if arguments.get(state, "inline")?.is_some() {
        "code"
    } else {
        "pre"
    };

    // TODO: Hook a syntax highlighter
    // let lang = attrs.get(state, sp, "lang")?;

    let sp = arguments.sp;
    render_runtime_list(out, state, sp, |_, source| {
        token![source, [
            Token::StartTag {
                name: token!(source, Span { tag }),
                attributes: vec![],
                self_closing: false,
            },
            // TODO: (1) unstripNoWiki the body, and
            // (2) hook up a syntax highlighter. The `strtr` is an alternative
            // for having an actual syntax highlighter
            Token::Text { strtr(arguments.body().trim_start_matches('\n').trim_ascii_end(), &[(">", "&gt;"), ("<", "&lt;")]) },
            Token::EndTag {
                name: token!(source, Span { tag })
            }
        ]].into()
    })
}

/// The `<templatedata>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:TemplateData>
fn template_data(
    out: &mut dyn WriteSurrogate,
    _: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    // TODO: Hook process data
    // Confusingly, because this is emitting `<pre>`, and someone decided that
    // `<pre>` should be an extension tag, the output will sometimes end up
    // getting sent again to `<pre>`. Probably, just parsing and emitting the
    // content as a table, as intended, would be a good idea.
    write!(out, "<pre>{}</pre>", arguments.body())?;
    Ok(())
}

/// The `<timeline>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:EasyTimeline>
fn timeline(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    if let Some(body) = arguments.body {
        let result = timeline::timeline_to_svg(body, &state.statics.base_uri)
            .map_err(|err| Error::Extension(Box::new(err)))?;
        write!(out, "<figure>{result}</figure>")?;
    }
    Ok(())
}

/// Collected template style data.
#[derive(Debug, Default)]
pub struct Styles {
    /// The names of the included CSS files.
    sources: HashSet<String>,
    /// The accumulated CSS data.
    pub text: String,
}

impl Styles {
    /// Inserts CSS from the article given in `src` using an optional wrapper.
    pub fn insert(
        &mut self,
        db: &Database<'static>,
        src: &str,
        wrapper: Option<&str>,
    ) -> Result<()> {
        if self.sources.contains(src) {
            if wrapper.is_some() {
                // What happens if this happens? Probably the wrappers need to
                // be part of the key.
                log::warn!("CSS reuse with a wrapper; this might be broken");
            }
            return Ok(());
        }

        let title = Title::new(src, Namespace::find_by_id(Namespace::TEMPLATE));
        if let Ok(css) = db.get(title.key()) {
            if let Some(wrapper) = wrapper {
                writeln!(self.text, "{wrapper} {{ {} }}", &css.body)?;
            } else {
                self.text += &css.body;
                self.text.push('\n');
            }
        } else {
            log::warn!("Could not load CSS from '{src}'");
        }

        self.sources.insert(src.to_string());
        Ok(())
    }
}

/// The `<templatestyles>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:TemplateStyles>
fn template_styles(
    _: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    let src = arguments.get(state, "src")?;
    if let Some(src) = src {
        let wrapper = arguments.get(state, "wrapper")?;
        state
            .globals
            .styles
            .insert(&state.statics.db, &src, wrapper.as_deref())?;
    }
    Ok(())
}

/// The signature of an extension tag function.
type ExtensionTagFn =
    fn(&mut dyn WriteSurrogate, &mut State<'_>, &ExtensionTag<'_, '_, '_>) -> Result;

/// All supported extension tags.
static EXTENSION_TAGS: phf::Map<&'static str, ExtensionTagFn> = phf::phf_map! {
    "indicator" => indicator,
    "math" => math,
    "nowiki" => no_wiki,
    "pre" => pre,
    "ref" => r#ref,
    "references" => references,
    "section" => section,
    "syntaxhighlight" => syntax_highlight,
    "templatedata" => template_data,
    "templatestyles" => template_styles,
    "timeline" => timeline,
};

/// Renders an extension tag.
pub(super) fn render_extension_tag(
    out: &mut dyn WriteSurrogate,
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    span: Option<Span>,
    callee: &str,
    arguments: &[Kv<'_>],
    body: Option<&str>,
) -> Result {
    if let Some(extension_tag) = EXTENSION_TAGS.get(callee) {
        if let Some(span) = span {
            extension_tag(
                out,
                state,
                &ExtensionTag {
                    arguments: IndexedArgs {
                        sp,
                        callee,
                        arguments: KeyCacheKvs::new(arguments),
                        span: Some(span),
                    },
                    body,
                },
            )
            .map_err(|err| Error::Node {
                frame: sp.name.to_string() + "$<" + callee + ">",
                start: sp.source.find_line_col(span.start),
                err: Box::new(err),
            })
        } else {
            // At least 'Module:Navbox/configuration' invokes the `#tag` parser
            // function and then stores the returned value, expecting that the
            // return value can be cached and reused. So, give it a value that
            // can be cached and reused…
            write!(out, "<{callee}")?;
            for attr in arguments {
                let value = attr.value(state, sp)?;
                if let Some(name) = attr.name(state, sp)? {
                    write!(out, " {name}")?;
                    if !value.is_empty() {
                        write!(out, r#"="{value}""#)?;
                    }
                } else if !value.is_empty() {
                    write!(out, " {}", attr.value(state, sp)?)?;
                }
            }
            if let Some(body) = body
                && !body.is_empty()
            {
                write!(out, ">{body}</{callee}>")?;
            } else {
                write!(out, "/>")?;
            }
            Ok(())
        }
    } else {
        log::warn!("TODO: {callee} tag");
        write!(
            out,
            "&lt;{callee}&gt;{}&lt;/{callee}&gt;",
            body.unwrap_or("")
        )?;
        Ok(())
    }
}
