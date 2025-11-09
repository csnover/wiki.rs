//! Utilities for building spanned tokens.

use super::{Argument, Span, Spanned, Token};

/// Generates an item with correct surrounding text for that kind of item.
macro_rules! token {
    (@list, $source:ident, [$($acc:tt)*][]) => { [ $($acc)* ] };
    (@list, $source:ident, [$($acc:tt)*][$($last:tt)*]) => {
        [ $($acc)* $crate::wikitext::builder::token!($source, $($last)*) ]
    };
    (@list, $source:ident, [$($acc:tt)*][$($curr:tt)*], $($rest:tt)*) => {
        $crate::wikitext::builder::token!(@list, $source, [
            $($acc)* $crate::wikitext::builder::token!($source, $($curr)*),
        ][] $($rest)*)
    };
    (@list, $source:ident, [$($acc:tt)*][$($curr:tt)*] $next:tt $($rest:tt)*) => {
        $crate::wikitext::builder::token!(@list, $source, [$($acc)*][$($curr)* $next] $($rest)*)
    };

    // An array of `Argument`.
    ($source:ident, [ $($name:expr => $value:expr),* $(,)? ]) => {
        [ $($crate::wikitext::builder::tok_arg($source, $name, $value)),* ]
    };

    // An array of `Spanned<Token>`.
    ($source:ident, [ $($value:tt)* ]) => {
        $crate::wikitext::builder::token!(@list, $source, [][] $($value)*)
    };

    // Empty array handler.
    ($source:ident, []) => {};

    // An `Argument`.
    ($source:ident, Argument { $name:expr => $value:expr }) => {
        $crate::wikitext::builder::tok_arg($source, $name, $value)
    };

    // A `Span`, with prefix and suffix.
    ($source:ident, Span { $prefix:expr ; $text:expr ; $suffix:expr }) => {
        $crate::wikitext::builder::tok_span($source, $prefix, $text, $suffix)
    };

    // A `Span`.
    ($source:ident, Span { $text:expr }) => {
        $crate::wikitext::builder::tok_span($source, "", $text, "")
    };

    // A `Token::EndTag`.
    ($source:ident, Token::EndTag { $($tt:tt)* }) => {
        $crate::wikitext::builder::tok($source, "</", |$source| {
            Token::EndTag { $($tt)* }
        }, ">")
    };

    // A `Token::ExternalLink`.
    ($source:ident, Token::ExternalLink { $($tt:tt)* }) => {
        $crate::wikitext::builder::tok($source, "[", |$source| {
            Token::ExternalLink { $($tt)* }
        }, "]")
    };

    // A `Token::StartTag`.
    ($source:ident, Token::StartTag { $($tt:tt)* }) => {
        $crate::wikitext::builder::tok($source, "<", |$source| {
            Token::StartTag { $($tt)* }
        }, ">")
    };

    // A `Token::Text`.
    ($source:ident, Token::Text { $text:expr }) => {
        $crate::wikitext::builder::tok_text($source, $text)
    };
}

pub(crate) use token;

/// Builds a [`Spanned<T>`].
pub fn tok<P, T, S, D>(source: &mut String, prefix: P, data: D, suffix: S) -> Spanned<T>
where
    P: AsRef<str>,
    S: AsRef<str>,
    D: FnOnce(&mut String) -> T,
{
    let start = source.len();
    source.push_str(prefix.as_ref());
    let node = data(source);
    source.push_str(suffix.as_ref());
    Spanned::new(node, start, source.len())
}

/// Builds a [`Spanned<Argument>`].
pub fn tok_arg<K, V>(source: &mut String, key: K, value: V) -> Spanned<Argument>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    tok(
        source,
        " ",
        |source| Argument {
            content: vec![
                tok_text(source, key),
                tok_text(source, "=\""),
                tok_text(
                    source,
                    html_escape::encode_double_quoted_attribute(value.as_ref()),
                ),
                tok_text(source, "\""),
            ],
            delimiter: Some(1),
            terminator: Some(3),
        },
        "",
    )
}

/// Builds a [`Span`].
pub fn tok_span<T, P, S>(source: &mut String, prefix: P, text: T, suffix: S) -> Span
where
    T: AsRef<str>,
    P: AsRef<str>,
    S: AsRef<str>,
{
    source.push_str(prefix.as_ref());
    let start = source.len();
    source.push_str(text.as_ref());
    let end = source.len();
    source.push_str(suffix.as_ref());
    Span { start, end }
}

/// Builds a [`Spanned<Token>`] using [`Token::Text`].
pub fn tok_text(source: &mut String, text: impl AsRef<str>) -> Spanned<Token> {
    let start = source.len();
    source.push_str(text.as_ref());
    Spanned::new(Token::Text, start, source.len())
}
