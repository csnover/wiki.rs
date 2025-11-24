//! Helpers for improved debug formatting of token trees.

use crate::wikitext::{
    AnnoAttribute, Argument, InclusionMode, LangFlags, LangVariant, TextStyle, Token,
    codemap::{FileMap, Spanned},
};
use core::fmt::{self, Write as _};

/// Returns a debug inspector for a token list using the given source code.
pub fn inspect<'a, T>(input: &'a FileMap<'a>, tree: &'a [T]) -> VInspector<'a, T::Inspector<'a>>
where
    T: Inspectable,
    T::Inspector<'a>: TInspector<'a, Inspectee = T>,
{
    VInspector::<'a, T::Inspector<'a>>(input, tree)
}

/// An inspectable type.
pub trait Inspectable {
    /// The default inspector for the type.
    type Inspector<'a>: TInspector<'a>;
}

/// Convenience macro for defining inspectable types.
macro_rules! inspectable {
    ($($ty:ty => $by:ident),*, $(,)?) => {
        $(impl Inspectable for Spanned<$ty> {
            type Inspector<'a> = $by<'a>;
        })*
    }
}

inspectable! {
    AnnoAttribute => AnnoAttrInspector,
    Argument => ArgumentInspector,
    LangFlags => LangFlagsInspector,
    LangVariant => LangVariantInspector,
    Token => TokenInspector,
}

/// A trait for debug formatting of various parser items.
pub trait TInspector<'a>: fmt::Debug {
    /// The type to be inspected.
    type Inspectee;
    /// Create an debug formatter for the given object.
    fn inspect(input: &'a FileMap<'a>, object: &'a Self::Inspectee) -> Self
    where
        Self: Sized;
}

/// A debug formatter for slices of parser items.
pub struct VInspector<'a, T>(&'a FileMap<'a>, &'a [T::Inspectee])
where
    T: TInspector<'a>;

impl<'a, T> fmt::Debug for VInspector<'a, T>
where
    T: TInspector<'a>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.1.iter().map(|inspect| T::inspect(self.0, inspect)))
            .finish()
    }
}

/// A debug formatter for [`Argument`].
pub struct ArgumentInspector<'a>(&'a FileMap<'a>, &'a Argument);

impl<'a> TInspector<'a> for ArgumentInspector<'a> {
    type Inspectee = Spanned<Argument>;

    fn inspect(input: &'a FileMap<'a>, object: &'a Self::Inspectee) -> Self {
        Self(input, object)
    }
}

impl fmt::Debug for ArgumentInspector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entry(
                &self
                    .1
                    .name()
                    .map(|name| VInspector::<TokenInspector<'_>>(self.0, name)),
                &VInspector::<TokenInspector<'_>>(self.0, self.1.value()),
            )
            .finish()
    }
}

/// A debug formatter for [`AnnoAttribute`].
pub struct AnnoAttrInspector<'a>(&'a FileMap<'a>, &'a AnnoAttribute);

impl<'a> TInspector<'a> for AnnoAttrInspector<'a> {
    type Inspectee = Spanned<AnnoAttribute>;

    fn inspect(input: &'a FileMap<'a>, object: &'a Self::Inspectee) -> Self {
        Self(input, object)
    }
}

impl fmt::Debug for AnnoAttrInspector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entry(
                &self
                    .1
                    .name
                    .either(|name| name, |name| &self.0[name.into_range()]),
                &self.1.value.map(|value| &self.0[value.into_range()]),
            )
            .finish()
    }
}

/// A debug formatter for [`LangFlags`].
pub struct LangFlagsInspector<'a>(&'a FileMap<'a>, &'a LangFlags);

impl<'a> TInspector<'a> for LangFlagsInspector<'a> {
    type Inspectee = Spanned<LangFlags>;

    fn inspect(input: &'a FileMap<'a>, object: &'a Self::Inspectee) -> Self {
        Self(input, object)
    }
}

impl fmt::Debug for LangFlagsInspector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_list();
        match self.1 {
            LangFlags::Combined(hash_set) => {
                d.entries(hash_set.iter().map(|k| &self.0[k.into_range()]))
            }
            LangFlags::Common(hash_set) => d.entries(hash_set.iter()),
        }
        .finish()
    }
}

/// A debug formatter for [`LangVariant`].
pub struct LangVariantInspector<'a>(&'a FileMap<'a>, &'a LangVariant);

impl<'a> TInspector<'a> for LangVariantInspector<'a> {
    type Inspectee = Spanned<LangVariant>;

    fn inspect(input: &'a FileMap<'a>, object: &'a Self::Inspectee) -> Self {
        Self(input, object)
    }
}

impl fmt::Debug for LangVariantInspector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            LangVariant::Empty => f.debug_struct("LangVariant::Empty").finish(),
            LangVariant::OneWay { from, lang, to } => f
                .debug_struct("LangVariant::OneWay")
                .field("from", &VInspector::<TokenInspector<'_>>(self.0, from))
                .field("lang", &TokenInspector(self.0, lang))
                .field("to", &VInspector::<TokenInspector<'_>>(self.0, to))
                .finish(),
            LangVariant::Text { text } => f
                .debug_tuple("LangVariant::Text")
                .field(&VInspector::<TokenInspector<'_>>(self.0, text))
                .finish(),
            LangVariant::TwoWay { lang, text } => f
                .debug_struct("LangVariant::TwoWay")
                .field("lang", &TokenInspector(self.0, lang))
                .field("text", &VInspector::<TokenInspector<'_>>(self.0, text))
                .finish(),
        }
    }
}

/// A debug formatter for [`Spanned<Token>`].
pub struct TokenInspector<'a>(&'a FileMap<'a>, &'a Spanned<Token>);

impl<'a> TInspector<'a> for TokenInspector<'a> {
    type Inspectee = Spanned<Token>;

    fn inspect(input: &'a FileMap<'a>, object: &'a Self::Inspectee) -> Self {
        Self(input, object)
    }
}

impl fmt::Debug for TokenInspector<'_> {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.1.node {
            Token::Autolink { target, content } => f
                .debug_struct(&span_name("Autolink", self.0, self.1))
                .field("target", &VInspector::<TokenInspector<'_>>(self.0, target))
                .field(
                    "content",
                    &VInspector::<TokenInspector<'_>>(self.0, content),
                )
                .finish(),
            Token::BehaviorSwitch { name } => f
                .debug_tuple(&span_name("BehaviorSwitch", self.0, self.1))
                .field(&&self.0[name.into_range()])
                .finish(),
            Token::Comment { content, unclosed } => f
                .debug_struct(&span_name("Comment", self.0, self.1))
                .field("content", &&self.0[content.into_range()])
                .field("unclosed", unclosed)
                .finish(),
            Token::EndAnnotation { name } => f
                .debug_struct(&span_name("EndAnnotation", self.0, self.1))
                .field(
                    "name",
                    &name.either(|name| name, |name| &self.0[name.into_range()]),
                )
                .finish(),
            Token::EndInclude(inclusion_mode) => f.write_str(&span_name(
                match inclusion_mode {
                    InclusionMode::IncludeOnly => "</includeonly>",
                    InclusionMode::NoInclude => "</noinclude>",
                    InclusionMode::OnlyInclude => "</onlyinclude>",
                },
                self.0,
                self.1,
            )),
            Token::EndTag { name } => f
                .debug_struct(&span_name("EndTag", self.0, self.1))
                .field("name", &&self.0[name.into_range()])
                .finish(),
            Token::Entity { value } => f
                .debug_tuple(&span_name("Entity", self.0, self.1))
                .field(value)
                .finish(),
            Token::Extension {
                name,
                attributes,
                content,
            } => f
                .debug_struct(&span_name("Extension", self.0, self.1))
                .field("name", &&self.0[name.into_range()])
                .field(
                    "attributes",
                    &VInspector::<ArgumentInspector<'_>>(self.0, attributes),
                )
                .field(
                    "content",
                    &content.map(|content| &self.0[content.into_range()]),
                )
                .finish(),
            Token::ExternalLink { target, content } => f
                .debug_struct(&span_name("ExternalLink", self.0, self.1))
                .field("target", &VInspector::<TokenInspector<'_>>(self.0, target))
                .field(
                    "content",
                    &VInspector::<TokenInspector<'_>>(self.0, content),
                )
                .finish(),
            Token::Generated(text) => f.debug_tuple("Generated").field(text).finish(),
            Token::Heading { level, content } => f
                .debug_struct(&span_name("Heading", self.0, self.1))
                .field("level", level)
                .field(
                    "content",
                    &VInspector::<TokenInspector<'_>>(self.0, content),
                )
                .finish(),
            Token::HorizontalRule { line_content } => f
                .debug_struct(&span_name("HorizontalRule", self.0, self.1))
                .field("line_content", line_content)
                .finish(),
            Token::LangVariant {
                flags: meta,
                variants,
                raw,
            } => f
                .debug_struct(&span_name("LangVariant", self.0, self.1))
                .field(
                    "meta",
                    &meta.as_ref().map(|meta| LangFlagsInspector(self.0, meta)),
                )
                .field(
                    "variants",
                    &VInspector::<LangVariantInspector<'_>>(self.0, variants),
                )
                .field("raw", raw)
                .finish(),
            Token::Link {
                target,
                content,
                trail,
            } => f
                .debug_struct(&span_name("Link", self.0, self.1))
                .field("target", &VInspector::<TokenInspector<'_>>(self.0, target))
                .field(
                    "content",
                    &VInspector::<ArgumentInspector<'_>>(self.0, content),
                )
                .field("trail", &trail.map(|trail| &self.0[trail.into_range()]))
                .finish(),
            Token::ListItem { bullets, content } => f
                .debug_struct(&span_name("ListItem", self.0, self.1))
                .field("bullets", &&self.0[bullets.into_range()])
                .field(
                    "content",
                    &VInspector::<TokenInspector<'_>>(self.0, content),
                )
                .finish(),
            Token::NewLine => f.write_str("\\n"),
            Token::Parameter { name, default } => f
                .debug_struct(&span_name("Parameter", self.0, self.1))
                .field("name", &VInspector::<TokenInspector<'_>>(self.0, name))
                .field(
                    "default",
                    &default
                        .as_deref()
                        .map(|default| VInspector::<TokenInspector<'_>>(self.0, default)),
                )
                .finish(),
            Token::Redirect { link } => f
                .debug_struct(&span_name("Redirect", self.0, self.1))
                .field("link", &TokenInspector(self.0, link))
                .finish(),
            Token::StartAnnotation { name, attributes } => f
                .debug_struct(&span_name("StartAnnotation", self.0, self.1))
                .field("name", &&self.0[name.into_range()])
                .field(
                    "attributes",
                    &VInspector::<AnnoAttrInspector<'_>>(self.0, attributes),
                )
                .finish(),
            Token::StartInclude(inclusion_mode) => f.write_str(&span_name(
                match inclusion_mode {
                    InclusionMode::IncludeOnly => "<includeonly>",
                    InclusionMode::NoInclude => "<noinclude>",
                    InclusionMode::OnlyInclude => "<onlyinclude>",
                },
                self.0,
                self.1,
            )),
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } => f
                .debug_struct(&span_name("StartTag", self.0, self.1))
                .field("name", &&self.0[name.into_range()])
                .field(
                    "attributes",
                    &VInspector::<ArgumentInspector<'_>>(self.0, attributes),
                )
                .field("self_closing", self_closing)
                .finish(),
            Token::StripMarker(marker) => {
                write!(f, "(marker {marker})")
            }
            Token::TableCaption {
                attributes,
                content,
            } => f
                .debug_struct(&span_name("TableCaption", self.0, self.1))
                .field(
                    "attributes",
                    &VInspector::<ArgumentInspector<'_>>(self.0, attributes),
                )
                .field(
                    "content",
                    &VInspector::<TokenInspector<'_>>(self.0, content),
                )
                .finish(),
            Token::TableData {
                attributes,
                content,
            } => f
                .debug_struct(&span_name("TableData", self.0, self.1))
                .field(
                    "attributes",
                    &VInspector::<ArgumentInspector<'_>>(self.0, attributes),
                )
                .field(
                    "content",
                    &VInspector::<TokenInspector<'_>>(self.0, content),
                )
                .finish(),
            Token::TableEnd => f
                .debug_struct(&span_name("TableEnd", self.0, self.1))
                .finish(),
            Token::TableHeading {
                attributes,
                content,
            } => f
                .debug_struct(&span_name("TableHeading", self.0, self.1))
                .field(
                    "attributes",
                    &VInspector::<ArgumentInspector<'_>>(self.0, attributes),
                )
                .field(
                    "content",
                    &VInspector::<TokenInspector<'_>>(self.0, content),
                )
                .finish(),
            Token::TableRow { attributes } => f
                .debug_struct(&span_name("TableRow", self.0, self.1))
                .field(
                    "attributes",
                    &VInspector::<ArgumentInspector<'_>>(self.0, attributes),
                )
                .finish(),
            Token::TableStart { attributes } => f
                .debug_struct(&span_name("TableStart", self.0, self.1))
                .field(
                    "attributes",
                    &VInspector::<ArgumentInspector<'_>>(self.0, attributes),
                )
                .finish(),
            Token::Template {
                target,
                arguments: parameters,
            } => f
                .debug_struct(&span_name("Template", self.0, self.1))
                .field("target", &VInspector::<TokenInspector<'_>>(self.0, target))
                .field(
                    "parameters",
                    &VInspector::<ArgumentInspector<'_>>(self.0, parameters),
                )
                .finish(),
            Token::Text => fmt::Debug::fmt(&&self.0[self.1.span.into_range()], f),
            Token::TextStyle(style) => {
                let name = match style {
                    TextStyle::Italic => "(italic)",
                    TextStyle::Bold(..) => "(bold)",
                    TextStyle::BoldItalic => "(bolditalic)",
                };
                f.write_str(&span_name(name, self.0, self.1))
            }
        }
    }
}

/// Decorates an item name with the line and column information of the object in
/// the source code.
fn span_name<T>(name: &str, input: &FileMap<'_>, spanned: &Spanned<T>) -> String {
    let start = input.find_line_col(spanned.span.start);
    let end = input.find_line_col(spanned.span.end);
    let mut out = format!("{name} @ {start}..");
    if start.line == end.line {
        write!(out, "{}", end.column)
    } else {
        write!(out, "{}:{}", end.line, end.column)
    }
    .unwrap();
    out
}
