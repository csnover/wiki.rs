//! A grammar for barely parsing CSS declarations out of style attributes,
//! and a compiler which barely supports a limited subset of CSS @import and
//! @mixin/@apply.

#![warn(
    clippy::pedantic,
    clippy::missing_docs_in_private_items,
    missing_docs,
    rust_2018_idioms
)]

use std::{
    fmt::Write as _,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub use barely_css::decl;

/// A CSS compiler error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A missing mixin error.
    #[error("mixin '{0}' not found")]
    Apply(String),
    /// A backtraced error.
    #[error("{err}\n  at '{path}'")]
    Backtrace {
        /// Path of the file which failed to compile.
        path: PathBuf,
        /// The error.
        #[source]
        err: Box<Self>,
    },
    /// An output error.
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    /// An I/O error.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A parser error.
    #[error(transparent)]
    Parse(#[from] peg::error::ParseError<peg::str::LineCol>),
    /// A path resolution error.
    #[error("path traversal above root")]
    Path,
}

/// The result type for the CSS compiler.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// The type used for recording CSS mixins.
type Mixins = std::collections::HashMap<String, String>;

/// Compiles a CSS file containing @import and @mixin/@apply.
pub fn compile(root: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<String> {
    let mut out = String::new();
    let mut mixins = Mixins::new();
    compile_part(&mut mixins, &mut out, root.as_ref(), path.as_ref())?;
    Ok(out)
}

peg::parser! {grammar barely_css() for str {
  /// Barely parses a single CSS declaration for streaming input.
  ///
  /// ```css
  /// k: v !important;
  /// ^^^^^^^^^^^^^^^^
  /// ```
  #[no_eof]
  pub rule decl() -> (Option<(&'input str, &'input str)>, usize)
  = d:maybe_decl() e:position!()
  { (d, e) }

  rule maybe_decl() -> Option<(&'input str, &'input str)>
  = _ name:$(name()) _ ":" _ value:$(value()) _ ("!" _ i("important"))? eol()
  { Some((name, value)) }
  / _ eol()
  { None }

  /// Barely parses a chunk from a CSS file.
  #[no_eof]
  pub(super) rule chunk() -> (Chunk<'input>, usize)
  = t:(import() / mixin() / apply() / opaque()) e:position!()
  { (t, e) }

  /// A CSS @import at-rule.
  ///
  /// ```css
  /// @import url(foo.css);
  /// @import url(foo.css) layer(foo);
  /// ```
  rule import() -> Chunk<'input>
  = kw_import() __ i("url") "(" url:url_string() ")" layer:import_layer()? eol()
  { Chunk::Import { url, layer } }

  /// CSS @import at-rule keyword.
  ///
  /// ```css
  /// @import url(foo.css);
  /// ^^^^^^^
  /// ```
  rule kw_import()
  = "@" i("import")

  /// The layer-part of a CSS @import at-rule.
  ///
  /// ```css
  /// @import url(foo.css) layer(foo);
  ///                     ^^^^^^^^^^^
  /// ```
  rule import_layer() -> &'input str
  = __ i("layer") "(" layer:$(name() ("." name())*) ")"
  { layer }

  /// The inner URL-part of a CSS @import at-rule.
  ///
  /// ```css
  /// @import url(foo.css);
  ///             ^^^^^^^
  /// @import url("foo.css");
  ///              ^^^^^^^
  /// @import url('foo.css');
  ///              ^^^^^^^
  /// ```
  rule url_string() -> &'input str
  = "\"" s:$(("\\\"" / [^'"'])*) "\"" { s }
  / "\'" s:$(("\\'" / [^'\''])*) "\'" { s }
  / $([^')']*)

  /// A CSS @mixin at-rule.
  ///
  /// ```css
  /// @mixin --foo() { /* ... */ }
  /// ```
  rule mixin() -> Chunk<'input>
  = kw_mixin() __ name:$(custom_name()) _ "(" /* *barely* CSS. */ ")"
    _ &"{" content:$(mixin_content())
  { Chunk::Mixin { name, content: &content[1..content.len() - 1] } }

  /// CSS @mixin at-rule keyword.
  ///
  /// ```css
  /// @mixin --foo() { /* ... */ }
  /// ^^^^^^
  /// ```
  rule kw_mixin()
  = "@" i("mixin")

  /// Possibly nested CSS with no inner at-rules.
  ///
  /// ```css
  /// .foo { .a { /* ... */ } b: c }
  /// ```
  rule mixin_content()
  = "{" (&"{" mixin_content() / [^'}'])+ "}"
  / [_]

  /// A CSS @apply at-rule.
  ///
  /// ```css
  /// @apply --foo;
  /// @apply --foo();
  /// ```
  rule apply() -> Chunk<'input>
  = kw_apply() __ name:$(custom_name()) _ "()"? eol()
  { Chunk::Apply { name } }

  /// CSS @apply at-rule keyword.
  ///
  /// ```css
  /// @apply --foo;
  /// ^^^^^^
  /// ```
  rule kw_apply()
  = "@" i("apply")

  /// Any CSS which is not part of an @import, @mixin, or @apply at-rule.
  rule opaque() -> Chunk<'input>
  = s:$(opaque_value()+)
  { Chunk::Opaque(s) }

  rule opaque_value()
  = [^'@']+
  / !(kw_import() / kw_mixin() / kw_apply()) [_]

  /// A CSS `<custom-ident>`.
  ///
  /// ```css
  /// --foo
  /// ```
  rule custom_name()
  = n:$(name())
  {? n.starts_with("--").then_some(()).ok_or("dashed-ident") }

  /// A CSS `<ident>`.
  ///
  /// ```css
  /// foo
  /// -prefix-foo
  /// --foo
  /// ```
  rule name()
  = ("--" / "-"? ident_start()) (ident_start() / ['0'..='9'|'-'])*

  rule ident_start()
  = ['A'..='Z'
      |'a'..='z'
      |'_'
      |'\u{00C0}'..='\u{00D6}'
      |'\u{00D8}'..='\u{00F6}'
      |'\u{00F8}'..='\u{037D}'
      |'\u{037F}'..='\u{1FFF}'
      |'\u{200C}'
      |'\u{200D}'
      |'\u{203F}'
      |'\u{2040}'
      |'\u{2070}'..='\u{218F}'
      |'\u{2C00}'..='\u{2FEF}'
      |'\u{3001}'..='\u{D7FF}'
      |'\u{F900}'..='\u{FDCF}'
      |'\u{FDF0}'..='\u{FFFD}'
      |'\u{10000}'..]

  rule value()
  = value_part()*

  rule value_part()
  = quoted_string() / [^';']

  rule quoted_string()
  = "\"" ("\\\"" / [^'"'])* "\""
  / "\'" ("\\'" / [^'\''])* "\'"

  rule eol()
  = _ (";" / ![_])

  rule _
  = space()*

  rule __
  = space()+

  rule space()
  = quiet!{
    [c if c.is_ascii_whitespace()]+
    / "/*" (!"*/" [_])* "*/"
  }

  rule i(lit: &'static str)
  = quiet!{
      input:$([_]*<{lit.chars().count()}>)
      {? if input.eq_ignore_ascii_case(lit) { Ok(()) } else { Err(lit) } }
  } / expected!(lit)
}}

/// Compiles part of a CSS file, with error wrapping.
fn compile_part(
    mixins: &mut Mixins,
    out: &mut String,
    root: &Path,
    path: impl AsRef<Path>,
) -> Result<()> {
    let path = path.as_ref();
    let input = read(root, path)?;
    compile_part_inner(mixins, out, root, &input).map_err(|err| Error::Backtrace {
        path: path.into(),
        err: Box::new(err),
    })
}

/// Compiles part of a CSS file.
fn compile_part_inner(
    mixins: &mut Mixins,
    out: &mut String,
    root: &Path,
    input: &str,
) -> Result<()> {
    let mut cursor = 0;
    while cursor != input.len() {
        let (chunk, end) = barely_css::chunk(&input[cursor..]).map_err(|mut err| {
            err.location = peg::Parse::position_repr(input, cursor + err.location.offset);
            err
        })?;

        match chunk {
            Chunk::Import { url, layer } => {
                let in_layer = layer.is_some();
                if let Some(layer) = layer {
                    writeln!(out, "@layer {layer} {{")?;
                }
                compile_part(mixins, out, root, url)?;
                if in_layer {
                    writeln!(out, "}}")?;
                }
            }
            Chunk::Mixin { name, content } => {
                mixins.insert(name.to_string(), content.to_string());
            }
            Chunk::Apply { name } => {
                if let Some(mixin) = mixins.get(name) {
                    write!(out, "{mixin}")?;
                } else {
                    return Err(Error::Apply(name.to_string()));
                }
            }
            Chunk::Opaque(css) => write!(out, "{css}")?,
        }
        cursor += end;
    }
    Ok(())
}

/// A parsed CSS chunk.
#[derive(Debug, Eq, PartialEq)]
enum Chunk<'a> {
    /// An `@import` rule.
    Import {
        /// The URL to import.
        url: &'a str,
        /// The CSS `@layer` to apply to the imported CSS.
        layer: Option<&'a str>,
    },
    /// A `@mixin` rule.
    Mixin {
        /// The name of the mixin.
        name: &'a str,
        /// The content of the mixin.
        content: &'a str,
    },
    /// An `@apply` rule.
    Apply {
        /// The name of the mixin to apply.
        name: &'a str,
    },
    /// Opaque CSS content.
    Opaque(&'a str),
}

/// Reads a string from a file with the given root and path part.
fn read(root: &Path, path: &Path) -> Result<String> {
    let path = root.join(path);
    if !path.starts_with(root) {
        return Err(Error::Path);
    }

    let mut buf = String::new();
    File::open(path)?.read_to_string(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mixin_apply() {
        let mut mixins = Mixins::new();
        let mut out = String::new();

        let source = "@mixin --foo() { a: b; } .c { @apply --foo; }";
        compile_part_inner(&mut mixins, &mut out, Path::new(""), source).unwrap();
        assert_eq!(out, " .c {  a: b;  }");
    }

    #[test]
    fn apply() {
        let source = "@apply --foo;";
        assert_eq!(
            barely_css::chunk(source),
            Ok((Chunk::Apply { name: "--foo" }, source.len()))
        );

        let source = "@apply --foo();";
        assert_eq!(
            barely_css::chunk(source),
            Ok((Chunk::Apply { name: "--foo" }, source.len()))
        );
    }

    #[test]
    fn mixin() {
        let source = "@mixin --name() { a: b; }";
        assert_eq!(
            barely_css::chunk(source),
            Ok((
                Chunk::Mixin {
                    name: "--name",
                    content: " a: b; "
                },
                source.len()
            ))
        );
    }

    #[test]
    fn import() {
        let source = "@import url(foo);";
        assert_eq!(
            barely_css::chunk(source),
            Ok((
                Chunk::Import {
                    url: "foo",
                    layer: None,
                },
                source.len()
            ))
        );

        let source = r#"@import url("foo");"#;
        assert_eq!(
            barely_css::chunk(source),
            Ok((
                Chunk::Import {
                    url: "foo",
                    layer: None,
                },
                source.len()
            ))
        );

        let source = "@import url('foo');";
        assert_eq!(
            barely_css::chunk(source),
            Ok((
                Chunk::Import {
                    url: "foo",
                    layer: None,
                },
                source.len()
            ))
        );
    }

    #[test]
    fn import_layer() {
        let source = "@import url(foo) layer(bar);";
        assert_eq!(
            barely_css::chunk(source),
            Ok((
                Chunk::Import {
                    url: "foo",
                    layer: Some("bar"),
                },
                source.len()
            ))
        );

        let source = "@import url(foo) layer(bar.baz);";
        assert_eq!(
            barely_css::chunk(source),
            Ok((
                Chunk::Import {
                    url: "foo",
                    layer: Some("bar.baz"),
                },
                source.len()
            ))
        );
    }
}
