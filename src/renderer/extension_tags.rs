//! Code for handling MediaWiki extension tags.
//!
//! Because Wikitext uses an HTML whitelist which can never really be updated
//! (because this would change the rendering of old Wikitext documents that
//! expects those character sequences to be treated as plain text), extension
//! tags are the only way to emit all sorts of useful HTML tags like `<figure>`,
//! `<svg>`, `<math>`, etc. And because extension tags *can* emit such useful
//! HTML, their outputs cannot ever be sent to a Wikitext parser or they will be
//! mangled. Instead, strip markers must be used to smuggle extension tag
//! content through the parser to the final HTML document.
//!
//! Unfortunately, because Wikitext is truly the recurrent laryngeal nerve of
//! text formats, it is not so simple to just have all extension tags emit HTML
//! that will be injected into the output and call it a day, because there are
//! a bunch of stupid edge cases that must also be handled.
//!
//! ## Stupid edge case #1: The graf wrapping algorithm
//!
//! The insane Wikitext algorithm for wrapping phrasing content into HTML
//! paragraphs relies on being able to know whether a line of Wikitext contains
//! certain HTML elements. Since this implementation treats the output of an
//! extension tag as opaque HTML, it is necessary to attach some extra metadata
//! to the output that can be used as a signal for the graf wrapper to know
//! whether the extension tag output should be considered phrasing content or
//! block-level content. This ends up not being very different to MW, which also
//! has a mechanism for returning this kind of metadata, though for different
//! stupid reasons.
//!
//! ## Stupid edge case #2: Extension tags inside extension tags
//!
//! Since this implementation treats the output from extension tags as blobs of
//! ready-to-concatenate HTML, extension tags themselves have to unstrip all
//! strip markers instead of relying on some other processor to do it for them.
//! If they don’t, the marker will be emitted instead of its content, which is
//! wrong.
//!
//! ## Stupid edge case #3: The `#tag` parser function
//!
//! Unfortunately, there are actually two ways to use extension tags. One way
//! is the normal way of writing a some XML-like tag in the source before and
//! then stepping away to wash your hands. The other way is the `#tag` parser
//! function.
//!
//! The best reason to use `#tag` (probably) is that it allows the evaluation of
//! Wikitext in places where it would otherwise not be allowed. For example,
//! `<pre>{{Foo}}</pre>` would emit `{{Foo}}`, but maybe you want the expansion
//! of 'Template:Foo'. `{{#tag:pre|{{Foo}}}}` will do that for you. (This
//! could’ve been stated plainly in the documentation, but different choices
//! were made, so I got to waste a bunch of time reading impenetrable jarson
//! about the *irrelevant* concept of a “pre-save transform”.) This means that
//! extension tags that do not actually support Wikitext when written as XML
//! have to deal with things like strip markers anyway.
//!
//! ## Stupid edge case #3(b): Wikitext and XML attributes aren’t syntactically
//!    compatible
//!
//! The Wikitext template argument syntax is almost but not *actually*
//! compatible with XML attributes. This means extra work has to be done to fix
//! attribute values when authors inevitably write
//! `{{#tag:foo||key="value"}}` when they actually meant `key=value`. (And
//! because `key=multiple words` is also only a valid template argument, it is
//! not good enough to just pass the whole argument as-is.)
//!
//! ## Stupid edge case #3(c): Scripts can also use `#tag`
//!
//! Some scripts expect to be able to use `#tag` and get back some value that
//! they can cache globally and reuse (particularly `<templatestyles>`). Because
//! wiki.rs caches modules themselves across multiple page loads, it is not good
//! enough to return a strip marker in this case because those are only valid
//! for a single page render.
//!
//! Since the output of a module has to make at least one trip through the
//! Wikitext parser and modules are allowed to emit raw XML extension tags that
//! get processed later, it ends up being good enough to just return the
//! XML form of the extension tag and actually process it after the module is
//! done running.
//!
//! ## Stupid edge case #3(d): Scripts can return XML extension tags
//!
//! The solution to one problem becomes the cause of another problem. Because
//! scripts can return XML extension tags like `<ref>`, other extension tags
//! like `<references>` written directly in the original source text cannot be
//! evaluated during the preprocessing stage or they might miss refs that are
//! not inserted until the postprocessing stage.
//!
//! ## Stupid edge case #4: `<nowiki>`
//!
//! Inside `<nowiki>`, *most* Wikitext rules stop applying, but not all of them.
//! HTML entity handling rules still apply, so valid HTML entities are left
//! alone whilst invalid ones get their ampersands entity-encoded. `<` and `>`
//! and `-{` and `}-` are all explicitly entity-encoded.
//!
//! But wait! Unlike any other extension tag, scripts can call to `mw.text` to
//! replace the strip markers for `<nowiki>` elements with *either* the raw text
//! from the `<nowiki>` body *or* the processed text without the invalid-entity
//! handling but *with* the explicit encodings of `< > -{ }-`. So now this one
//! extension tag requires special handling mechanics. It also cannot be eagerly
//! evaluated because the transformations are not reversible:
//! `<nowiki>&lt;<</nowiki>`, after processing, is `&lt;&lt;`.
//!
//! TODO: Somehow, it also needs to be the case that `<<nowiki/>pre>` produces
//! `&lt;pre&gt;`, but `&<nowiki/>amp;` produces `&amp;`, which suggests that
//! entity processing has to happen *extremely* late. Like, possibly *too* late.
//!
//! ## Stupid edge case #5: `<pre>`
//!
//! `<pre>` is an extension tag. It is also an HTML tag. As a result, it is an
//! extension tag which emits *itself*, but when it is emitting itself, it is
//! emitting an HTML tag, and not an extension tag. This means that it *cannot*
//! be emitted in a way that causes its output to *ever* be passed through the
//! Wikitext parser.
//!
//! `<pre>` is also `<nowiki>`, except it is not `<nowiki>`. It encodes only `<`
//! and `>`, not `-{` or `}-`. Sure, why not.
//!
//! ## Stupid edge case #6: Smart quotes (a wiki.rs exclusive!)
//!
//! Because `<nowiki>` can be used in any position, its output must be treated
//! as a run of text, rather than as opaque HTML which can be emitted as-is
//! directly into the output stream.
//!
//! This is actually technically true of *all* extension tag content, but all
//! the other extension tags fall into some special category where it doesn’t
//! matter:
//!
//! 1. They emit code, which should not have smart quotes anyway (`<pre>`,
//!    `<syntaxhighlight>`)
//! 2. They emit block content, and so will not be interleaved with other runs
//!    of text (`<poem>`, `<references>`, `<timeline>`), and so can do their own
//!    typographical beautification

// Clippy: Methods are implementing an interface which is invisible to clippy.
#![allow(clippy::unnecessary_wraps)]

mod timeline;

use super::{
    Error, ExpandMode, ExpandTemplates, State, StripMarker,
    document::Document,
    image,
    stack::{IndexedArgs, KeyCacheKvs, Kv, StackFrame},
    surrogate::Surrogate as _,
    text_run,
};
use crate::{
    common::{anchor_encode, decode_html},
    db::Database,
    php::strtr,
    title::{Namespace, Title},
    wikitext::{self, Argument, FileMap, Output, Span, Spanned, Token},
};
use core::{fmt::Write as _, ops::Range};
use either::Either;
use numerals::roman::Roman;
use regex::{Regex, RegexBuilder};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

/// The result type for an extension tag function.
type Result<T = OutputMode, E = Error> = super::Result<T, E>;

/// A helper struct for passing arguments required by all extension tags.
struct ExtensionTag<'args, 'call, 'sp> {
    /// The attributes of the extension tag.
    arguments: IndexedArgs<'args, 'call, 'sp>,
    /// The raw body text of the extension tag, if one existed in the source
    /// text.
    body: Option<&'call str>,
    /// If true, the extension tag is actually a `{{#tag}}` call.
    from_parser_fn: bool,
    /// If true, the extension tag is being rendered by a [`Document`].
    // TODO: This sucks, is there really no better way to signal this?
    in_document: bool,
}

impl<'args, 'call, 'sp> core::ops::Deref for ExtensionTag<'args, 'call, 'sp> {
    type Target = IndexedArgs<'args, 'call, 'sp>;

    fn deref(&self) -> &Self::Target {
        &self.arguments
    }
}

impl ExtensionTag<'_, '_, '_> {
    /// Returns the unevaluated body of the tag as a string.
    #[inline]
    pub fn body(&self) -> &str {
        self.body.unwrap_or("")
    }

    /// Evaluates the body of the tag.
    #[inline]
    pub fn eval_body(&self, state: &mut State<'_>) -> Result<String> {
        eval_string(state, self.sp, self.body())
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

/// The `<gallery>` extension tag.
fn gallery(
    out: &mut String,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    // TODO: params mode, showfilename, showthumbnails (for mode = slideshow),
    // PLUS any valid HTML attribute that applies to a `<ul>`,
    // TODO: modes: nolines, packed, packed-hover, packed-overlay, slideshow
    let mode = arguments
        .get(state, "mode")?
        .unwrap_or("traditional".into());

    let class = arguments.get(state, "class")?.unwrap_or_default();
    let heights = arguments.get(state, "heights")?;

    let mut defaults = image::Options::default();
    defaults.align = Some("none".into());
    defaults.format = Some("thumb".into());
    if let Some(heights) = &heights {
        defaults
            .attrs
            .insert("height".into(), Cow::Borrowed(heights));
    }

    let per_row = arguments.get(state, "perrow")?;
    let widths = arguments.get(state, "widths")?;
    let attrs = if per_row.is_some() || widths.is_some() {
        let width = widths.map_or("1fr".into(), |widths| Cow::Owned(format!("{widths}px")));
        let per_row = per_row.unwrap_or("auto-fill".into());
        format!(r#" style="grid-template-columns: repeat({per_row}, {width})""#)
    } else {
        <_>::default()
    };

    // MW put this *inside* the list, which is obviously stupid and wrong
    if let Some(caption) = arguments.get(state, "caption")? {
        write!(out, r#"<div class="gallerycaption">"#)?;
        text_run(out, ' ', &caption, false, true)?;
        write!(out, "</div>")?;
    }

    write!(
        out,
        r#"<ul class="gallery mw-gallery-{mode} {class}"{attrs}>"#
    )?;
    for image in arguments.body().lines() {
        let Some((target, rest)) = image.split_once('|') else {
            continue;
        };

        let target = percent_encoding::percent_decode_str(target).decode_utf8_lossy();
        if !Title::is_valid(&target) {
            continue;
        }

        let title = Title::new(&target, Namespace::find_by_id(Namespace::FILE));

        let args = preprocess_frame(state, arguments.sp, rest)?;
        let sp = arguments.sp.clone_with_source(FileMap::new(&args));
        let args = state.statics.parser.parse_gallery_media(&sp.source)?;
        let options = image::media_options(state, &sp, title, &args, defaults.clone())?;

        let mut inner = Document::new(true);
        image::render_media_with_options(&mut inner, state, &sp, &options)?;
        write!(out, r#"<li class="gallerybox">{}</li>"#, inner.finish()?)?;
    }
    write!(out, "</ul>")?;

    Ok(OutputMode::Block)
}

/// The `<indicator>` extension tag.
/// <https://www.mediawiki.org/wiki/Help:Page_status_indicators>
fn indicator(
    _: &mut String,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    let Some(name) = arguments.get(state, "name")? else {
        return Ok(OutputMode::Empty);
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
        state
            .globals
            .indicators
            .insert(name.to_string(), out.finish()?);
    }

    Ok(OutputMode::Empty)
}

/// The `<mapframe>` extension tag.
/// <https://www.mediawiki.org/wiki/Extension:Kartographer>
fn map_frame(out: &mut String, _: &mut State<'_>, arguments: &ExtensionTag<'_, '_, '_>) -> Result {
    // TODO: Params: width (number, "100%", "full"), height (number), zoom
    // (0-19, default 12), latitude, longitude, align ("left", "center",
    // "right"), mapstyle ("osm", "osm-intl"), lang (code or "local"), alt,
    // text, frameless

    // TODO: Actually parse whatever JSON is there and emit a map
    let body = strtr(
        arguments.body(),
        &[
            ("-{", "-&#123;"),
            ("}-", "&#125;-"),
            ("<", "&lt;"),
            (">", "&gt;"),
        ],
    );
    write!(out, "<pre>{body}</pre>")?;
    Ok(OutputMode::Block)
}

/// The `<math>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Math>
fn math(out: &mut String, state: &mut State<'_>, arguments: &ExtensionTag<'_, '_, '_>) -> Result {
    let latex = arguments.body();

    // TODO: Undocumented 'id' attribute

    // TODO: This might not be accurate enough to MW.
    // In MW:
    // * 'block' wraps "{\displaystyle{latex}}"
    // * 'inline' wraps "{\textstyle{latex}}"
    // * 'linebreak' is also an option, wraps "\[{latex}\]"
    let (output, mode) = if let Some(value) = arguments.get(state, "display")?
        && value == "block"
    {
        (OutputMode::Block, math_core::MathDisplay::Block)
    } else {
        (OutputMode::Inline, math_core::MathDisplay::Inline)
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

    Ok(output)
}

/// The `<nowiki>` extension tag.
fn no_wiki(
    out: &mut String,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    let body = strtr(
        arguments.body(),
        &[
            ("-{", "-&#123;"),
            ("}-", "&#125;-"),
            ("<", "&lt;"),
            (">", "&gt;"),
        ],
    );
    let body = state.strip_markers.unstrip(&body);
    write!(out, "{body}")?;
    Ok(OutputMode::Nowiki)
}

/// The `<poem>` extension tag.
/// <https://www.mediawiki.org/wiki/Extension:Poem>
fn poem(out: &mut String, state: &mut State<'_>, arguments: &ExtensionTag<'_, '_, '_>) -> Result {
    let source = arguments.body();
    // The lines iterator strips a trailing newline
    let source = source
        .strip_prefix("\r\n")
        .or_else(|| source.strip_prefix('\n'))
        .unwrap_or(source);

    let class = arguments.get(state, "class")?.unwrap_or_default();
    let nl = arguments.get(state, "compact")?.map_or("\n", |_| "");
    let mut text = format!(r#"<div class="poem {class}">{nl}"#);
    let mut iter = source.lines().peekable();
    while let Some(line) = iter.next() {
        if let Some(indent) = line.find(|c: char| c != ':')
            && indent != 0
        {
            write!(
                text,
                r#"<span class="mw-poem-indented" style="margin-inline-start: {indent}em">{}</span>"#,
                &line[indent..]
            )?;
        } else if let Some(spaces) = line.find(|c: char| c != ' ')
            && spaces != 0
        {
            for _ in 0..spaces {
                write!(text, "&nbsp;")?;
            }
            write!(text, "{}", &line[spaces..])?;
        } else {
            write!(text, "{line}")?;
        }

        if line.ends_with("----") {
            writeln!(text)?;
        } else if iter.peek().is_some() {
            writeln!(text, "<br>")?;
        }
    }
    write!(text, "{nl}</div>")?;

    let body = eval_string(state, arguments.sp, &text)?.replace("<hr><br>", "<hr>");
    let body = state.strip_markers.unstrip(&body);
    write!(out, "{body}")?;

    Ok(OutputMode::Block)
}

/// The `<pre>` extension tag.
fn pre(out: &mut String, state: &mut State<'_>, arguments: &ExtensionTag<'_, '_, '_>) -> Result {
    // “Backwards-compatibility hack”
    static STRIP_NOWIKI: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new("<nowiki>(.*?)</nowiki>")
            .case_insensitive(true)
            .build()
            .unwrap()
    });

    write!(out, "<pre")?;
    for attribute in arguments.iter() {
        let value = attribute.value(state, arguments.sp)?;
        let name = attribute
            .name(state, arguments.sp)?
            .unwrap_or(value.clone());

        if name == "format" {
            continue;
        }

        // ha ha kill me
        let value = if arguments.from_parser_fn
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value[1..value.len() - 1].to_string().into()
        } else {
            value
        };

        // TODO: This is supposed to strip markers and use a whitelist of valid
        // attribute names.
        write!(out, r#" {name}="{}""#, strtr(&value, &[("\"", "&quot;")]))?;
    }

    let process_wikitext = arguments.get(state, "format")?.as_deref() == Some("wikitext");

    let body = if process_wikitext {
        Cow::Owned(arguments.eval_body(state)?)
    } else {
        // 'Template:Blockquote' dumps a `<syntaxhighlight>` into
        // 'Template:Markup' which blindly dumps that into a `<pre>`.
        // Unstripping strip markers *before* encoding the rest of the body will
        // result in double-encoding of the markup. MW does things differently
        // and does not unstrip markers at all in its tag hooks, obviously
        // preferring to commit a crime somewhere else to get the strip marker
        // content out. Since all the strip markers in wiki.rs are supposed to
        // contain well-formed HTML ready to be emitted to the final document
        // with no other Wikitext parsing, doing things in this order ‘should’
        // be ‘fine’.
        let body = STRIP_NOWIKI.replace_all(arguments.body(), "$1");
        match strtr(&body, &[("<", "&lt;"), (">", "&gt;")]) {
            Cow::Borrowed(_) => body,
            Cow::Owned(body) => Cow::Owned(body),
        }
    };

    let body = state.strip_markers.unstrip(&body);
    write!(out, ">{body}</pre>")?;
    Ok(OutputMode::Block)
}

/// A reference key.
#[derive(Debug, Eq, Hash, PartialEq)]
struct RefKey {
    /// The group name.
    group: String,
    /// The reference name.
    name: String,
}

/// Stored citation references.
#[derive(Debug, Default)]
pub(crate) struct References {
    /// Bump allocation of reference text.
    text: String,
    /// References in a group. Value is a map of ranges into `text`. For
    /// compatibility, the default group is an empty string.
    groups: HashMap<String, Vec<Range<usize>>>,
    /// Named references. Value is an index into `groups[key.group]`.
    named: HashMap<RefKey, usize>,
}

impl References {
    /// Appends text to a named reference, separated by a single space. If the
    /// reference does not already exist, it is created.
    fn append_named(&mut self, name: RefKey, value: &str) {
        let group = self.groups.entry(name.group.clone()).or_default();

        let index = self.named.entry(name).or_insert(group.len());

        if let Some(range) = group.get_mut(*index) {
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
            group.push(range);
        }
    }

    /// Adds a named reference with the given text. If the reference already
    /// exists and contains text, this call does nothing. Returns the
    /// page-unique ID of the reference.
    fn insert_named(&mut self, name: RefKey, value: &str) -> String {
        if let Some(index) = self.named.get(&name) {
            let key = Self::make_id(&name.group, *index);
            // TODO: 'cite_error_references_duplicate_key'
            let range = &self.groups[&name.group][*index];

            // Some pages like 'Wikidata' create empty named refs and then
            // populate the data later
            if range.is_empty() && !value.is_empty() {
                self.append_named(name, value);
            }

            return key;
        }

        let range = self.text.len()..(self.text.len() + value.len());
        self.text += value;

        let group = self.groups.entry(name.group.clone()).or_default();
        let index = group.len();
        let key = Self::make_id(&name.group, index);
        self.named.insert(name, index);
        group.push(range);
        key
    }

    /// Adds an named reference with the given text. Returns the page-unique
    /// numeric ID of the reference.
    fn insert_unnamed(&mut self, group: String, value: &str) -> String {
        let range = self.text.len()..(self.text.len() + value.len());
        self.text += value;

        let name = group.clone();
        let group = self.groups.entry(group).or_default();
        let key = Self::make_id(&name, group.len());
        group.push(range);
        key
    }

    /// Returns an iterator over the references in the given group.
    fn iter_group(&self, group_name: &String) -> Option<impl Iterator<Item = (String, &str)>> {
        self.groups.get(group_name).map(|group| {
            group
                .iter()
                .enumerate()
                .map(|(index, range)| (Self::make_id(group_name, index), &self.text[range.clone()]))
        })
    }

    /// Encodes forward and backward reference anchors for an ID.
    fn make_anchors(id: &str) -> (String, String) {
        (
            anchor_encode(&format!("cite_ref-{id}")),
            anchor_encode(&format!("ref_{id}")),
        )
    }

    /// Makes a unique key for a grouped reference.
    fn make_id(group: &str, index: usize) -> String {
        let index = index + 1;
        if group.is_empty() {
            index.to_string()
        } else {
            match group {
                "decimal" => index.to_string(),
                "lower-alpha" => Self::make_id_alpha(index - 1, 'a', 26),
                "upper-alpha" => Self::make_id_alpha(index - 1, 'A', 26),
                "lower-greek" => Self::make_id_alpha(index - 1, 'α', 24),
                "upper-greek" => Self::make_id_alpha(index - 1, 'Α', 24),
                "lower-roman" => format!("{:x}", Roman::from(i16::try_from(index).unwrap())),
                "upper-roman" => format!("{:X}", Roman::from(i16::try_from(index).unwrap())),
                _ => format!("{group} {index}"),
            }
        }
    }

    /// Makes an alphabetic ordinal key.
    fn make_id_alpha(mut n: usize, base: char, alphabet_size: usize) -> String {
        let mut buf = ['\0'; 8];
        let mut index = 0;
        // Clippy: `alphabet_size` is always smaller than u32
        #[allow(clippy::cast_possible_truncation)]
        loop {
            buf[index] = char::from_u32(u32::from(base) + (n % alphabet_size) as u32).unwrap();
            n /= alphabet_size;
            index += 1;
            if n == 0 {
                break;
            }
        }
        buf[..index].iter().rev().collect()
    }
}

/// The `<ref>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Cite>
fn r#ref(out: &mut String, state: &mut State<'_>, arguments: &ExtensionTag<'_, '_, '_>) -> Result {
    // Due to transclusion it is necessary to render immediately instead of
    // storing the node list for later, since rendering later would require
    // retaining the stack frame too
    let reference = eval_string(state, arguments.sp, arguments.body().trim_ascii())?;

    let group = arguments
        .get(state, "group")?
        .as_deref()
        .map_or(<_>::default(), ToString::to_string);

    if let Some(follow) = arguments.get(state, "follow")? {
        state.globals.references.append_named(
            RefKey {
                group,
                name: follow.to_string(),
            },
            &reference,
        );
        return Ok(OutputMode::Empty);
    }

    let id = if let Some(name) = arguments.get(state, "name")? {
        Some(state.globals.references.insert_named(
            RefKey {
                group: group.clone(),
                name: name.to_string(),
            },
            &reference,
        ))
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

    Ok(if let Some(id) = id {
        let (from, to) = References::make_anchors(&id);
        write!(
            out,
            r##"<span class="reference" id="{from}"><a href="#{to}">{id}</a></span>"##
        )?;
        OutputMode::Inline
    } else {
        OutputMode::Empty
    })
}

/// The `<references>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Cite>
fn references(
    out: &mut String,
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
        // TODO: Any refs inside here are supposed to be added to the group
        // matching the group attribute of the `<references>` tag, not the
        // empty group.
        arguments.eval_body(state)?;
    }

    // If some template expansion emits a `<ref>` tag, the content of that
    // reference will be lost if `<references>` is also processed during
    // template expansion. Because wiki.rs converts `#tag` calls from scripts
    // into XML tags to be processed during the final pass, scripts that call
    // that also will break if `<references>` processing is not deferred.
    if !arguments.in_document {
        return render_raw(
            state,
            arguments.sp,
            arguments.callee,
            // The body was processed, only the output remains.
            None,
            // buffalo buffalo buffalo 😵‍💫
            &arguments.arguments.arguments,
            out,
        );
    }

    let group = arguments
        .get(state, "group")?
        .as_deref()
        .map_or(<_>::default(), ToString::to_string);

    // TODO: For multiple references to the same name, there should be backrefs
    // to all of them, not just the first one.
    Ok(
        if let Some(refs) = state.globals.references.iter_group(&group) {
            write!(out, r#"<ol class="references">"#)?;
            for (index, (id, text)) in refs.enumerate() {
                if !text.is_empty() {
                    let index = index + 1;
                    let (from, to) = References::make_anchors(&id);
                    write!(
                        out,
                        r##"<li value="{index}" id="{to}" class="mw-cite-backlink"><a href="#{from}">^</a> {text}</li>"##
                    )?;
                }
            }
            write!(out, "</ol>")?;
            OutputMode::Block
        } else {
            OutputMode::Empty
        },
    )
}

/// Stored ranges for labelled section transclusion.
///
/// These are not currently used for anything; Lua modules which perform
/// transclusion sniff these tags themselves.
#[derive(Debug, Default)]
pub(crate) struct LabelledSections {
    /// A map from an article title to a map of the ranges of labelled sections
    /// within the article.
    titles: HashMap<String, HashMap<String, Range<usize>>>,
}

/// The `<section>` extension tag.
/// <https://en.wikipedia.org/wiki/Help:Labeled_section_transclusion>
fn section(_: &mut String, state: &mut State<'_>, arguments: &ExtensionTag<'_, '_, '_>) -> Result {
    // `{{#tag: ... }}` may have no bounds if it was invoked from a script for
    // some reason
    let Some(bounds) = arguments.span else {
        return Ok(OutputMode::Empty);
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

    Ok(OutputMode::Empty)
}

/// The `<syntaxhighlight>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:Syntaxhighlight>
fn syntax_highlight(
    out: &mut String,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    static SS: LazyLock<syntect::parsing::SyntaxSet> = LazyLock::new(|| {
        let mut ss = syntect::parsing::SyntaxSet::load_defaults_newlines().into_builder();
        ss.add(
            syntect::parsing::SyntaxDefinition::load_from_str(
                include_str!("../../res/MediawikiNG.sublime-syntax"),
                true,
                Some("wikitext"),
            )
            .unwrap(),
        );
        ss.build()
    });

    static THEME: LazyLock<syntect::highlighting::Theme> = LazyLock::new(|| {
        let themes = syntect::highlighting::ThemeSet::load_defaults();
        themes.themes.get("InspiredGitHub").unwrap().clone()
    });

    let (mode, tag, attrs) = if arguments.get(state, "inline")?.is_some() {
        (OutputMode::Inline, "code", "")
    } else {
        // Because this might get dumped into a `<pre>` (see the `pre` function
        // for more detailed and thrilling commentary about this), make it a
        // `<div>` like how the MW extension does it.
        (OutputMode::Block, "div", r#" role="code""#)
    };

    // TODO: `line`, `start`, `linelinks`, `highlight`, `class`, `style`, and
    // `copy` attributes, plus undocumented `id` and `dir` attributes

    let lang = arguments
        .get(state, "lang")?
        .unwrap_or(Cow::Borrowed("txt"));

    let syntax = SS
        .find_syntax_by_token(&lang)
        .unwrap_or(SS.find_syntax_plain_text());

    let body = state.strip_markers.unstrip(arguments.body());
    let body = body.trim_start_matches('\n').trim_ascii_end();

    write!(out, r#"<{tag}{attrs} class="mw-highlight">"#)?;

    let mut highlighter = syntect::easy::HighlightLines::new(syntax, &THEME);
    for line in syntect::util::LinesWithEndings::from(body) {
        let regions = highlighter
            .highlight_line(line, &SS)
            .map_err(|err| Error::Extension(Box::new(err)))?;
        syntect::html::append_highlighted_html_for_styled_line(
            &regions[..],
            syntect::html::IncludeBackground::No,
            out,
        )
        .map_err(|err| Error::Extension(Box::new(err)))?;
    }

    write!(out, "</{tag}>")?;
    Ok(mode)
}

/// The `<templatedata>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:TemplateData>
fn template_data(
    out: &mut String,
    _: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    // TODO: Actually parse the JSON and emit a table
    let body = strtr(
        arguments.body(),
        &[
            ("-{", "-&#123;"),
            ("}-", "&#125;-"),
            ("<", "&lt;"),
            (">", "&gt;"),
        ],
    );
    write!(out, "<pre>{body}</pre>")?;
    Ok(OutputMode::Block)
}

/// The `<timeline>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:EasyTimeline>
fn timeline(
    out: &mut String,
    state: &mut State<'_>,
    arguments: &ExtensionTag<'_, '_, '_>,
) -> Result {
    if let Some(body) = arguments.body {
        let result = timeline::timeline_to_svg(body, &state.statics.base_uri)
            .map_err(|err| Error::Extension(Box::new(err)))?;
        write!(out, r#"<figure class="wiki-rs-timeline">{result}</figure>"#)?;
    }
    Ok(OutputMode::Block)
}

/// Collected template style data.
#[derive(Debug, Default)]
pub(crate) struct Styles {
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
        let key = if let Some(wrapper) = wrapper {
            format!("{src}{wrapper}")
        } else {
            src.to_string()
        };

        if self.sources.contains(&key) {
            return Ok(());
        }

        let title = Title::new(src, Namespace::find_by_id(Namespace::TEMPLATE));
        if let Ok(css) = db.get(&title) {
            if let Some(wrapper) = wrapper {
                writeln!(self.text, "{wrapper} {{ {} }}", &css.body)?;
            } else {
                self.text += &css.body;
                self.text.push('\n');
            }
        } else {
            log::warn!("Could not load CSS from '{src}'");
        }

        self.sources.insert(key);
        Ok(())
    }
}

/// The `<templatestyles>` extension tag.
/// <https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:TemplateStyles>
fn template_styles(
    _: &mut String,
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
    Ok(OutputMode::Empty)
}

/// The signature of an extension tag function.
type ExtensionTagFn = fn(&mut String, &mut State<'_>, &ExtensionTag<'_, '_, '_>) -> Result;

/// All supported extension tags.
static EXTENSION_TAGS: phf::Map<&'static str, ExtensionTagFn> = phf::phf_map! {
    "chem" => math,
    "gallery" => gallery,
    "indicator" => indicator,
    "mapframe" => map_frame,
    "math" => math,
    "nowiki" => no_wiki,
    "poem" => poem,
    "pre" => pre,
    "ref" => r#ref,
    "references" => references,
    "section" => section,
    "source" => syntax_highlight,
    "syntaxhighlight" => syntax_highlight,
    "templatedata" => template_data,
    "templatestyles" => template_styles,
    "timeline" => timeline,
};

/// The output mode of an extension tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputMode {
    /// The extension tag outputs one or more block-level elements.
    Block,
    /// The extension tag outputs nothing directly.
    Empty,
    /// The extension tag outputs one or more phrasing elements.
    Inline,
    /// The extension tag outputs plain text.
    Nowiki,
    /// The extension tag outputs its unprocessed self.
    Raw,
}

/// Incoming extension tag arguments.
pub(crate) enum InArgs<'a, 'b> {
    /// The extension tag is invoked from a `#tag`
    /// [parser function](super::parser_fns).
    ParserFn(&'a [Kv<'b>]),
    /// The extension tag is invoked from Wikitext.
    Wikitext(&'a [Spanned<Argument>]),
}

/// Renders an extension tag.
pub(super) fn render_extension_tag(
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    span: Option<Span>,
    callee: &str,
    arguments: &InArgs<'_, '_>,
    body: Option<&str>,
    in_document: bool,
) -> Result<Option<Either<StripMarker, String>>> {
    let (arguments, from_parser_fn) = match arguments {
        InArgs::ParserFn(kvs) => (Cow::Borrowed(*kvs), true),
        InArgs::Wikitext(attrs) => {
            // TODO: Collecting into a `Vec<Kv>` first wastes time.
            let attrs = attrs.iter().map(Kv::Argument).collect::<Vec<_>>();
            (Cow::Owned(attrs), false)
        }
    };
    let arguments = KeyCacheKvs::new(&arguments);

    let mut out = String::new();
    let mode = if let Some(extension_tag) = EXTENSION_TAGS.get(callee) {
        if let Some(span) = span {
            extension_tag(
                &mut out,
                state,
                &ExtensionTag {
                    arguments: IndexedArgs {
                        sp,
                        callee,
                        arguments,
                        span: Some(span),
                    },
                    body,
                    from_parser_fn,
                    in_document,
                },
            )
            .map_err(|err| Error::Node {
                frame: sp.name.to_string() + "$<" + callee + ">",
                start: sp.source.find_line_col(span.start),
                err: Box::new(err),
            })?
        } else {
            // At least 'Module:Navbox/configuration' invokes the `#tag` parser
            // function and then stores the returned value, expecting that the
            // return value can be cached and reused. So, give it a value that
            // can be cached and reused…
            render_raw(state, sp, callee, body, &arguments, &mut out)?
        }
    } else {
        log::warn!("TODO: {callee} tag");
        write!(
            out,
            "&lt;{callee}&gt;{}&lt;/{callee}&gt;",
            html_escape::encode_text(&decode_html(body.unwrap_or("")))
        )?;
        OutputMode::Block
    };

    Ok(match mode {
        OutputMode::Block => Some(Either::Left(StripMarker::Block(out))),
        OutputMode::Empty => None,
        OutputMode::Inline => Some(Either::Left(StripMarker::Inline(out))),
        OutputMode::Nowiki => Some(Either::Left(StripMarker::NoWiki(out))),
        OutputMode::Raw => Some(Either::Right(out)),
    })
}

/// Evaluates a Wikitext string as a document fragment, returning the rendered
/// fragment.
fn eval_string(state: &mut State<'_>, sp: &StackFrame<'_>, text: &str) -> Result<String> {
    let source = preprocess_frame(state, sp, text)?;
    let sp = sp.clone_with_source(FileMap::new(&source));
    let root = state.statics.parser.parse_no_expansion(&sp.source)?;
    let mut out = Document::new(true);
    out.adopt_output(state, &sp, &root)?;
    out.finish()
}

/// Preprocesses the given text in a root document scope.
fn preprocess_frame(state: &mut State<'_>, sp: &StackFrame<'_>, text: &str) -> Result<String> {
    let sp = sp.clone_with_source(FileMap::new(text));
    let root = state.statics.parser.parse(&sp.source, false)?;
    let mut preprocessor = ExpandTemplates::new(ExpandMode::Normal);
    preprocessor.adopt_output(state, &sp, &root)?;
    Ok(preprocessor.finish())
}

/// Re-emits an extension tag.
fn render_raw(
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    callee: &str,
    body: Option<&str>,
    arguments: &KeyCacheKvs<'_, '_>,
    out: &mut String,
) -> Result {
    write!(out, "<{callee}")?;
    for attr in arguments.iter() {
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
    Ok(OutputMode::Raw)
}
