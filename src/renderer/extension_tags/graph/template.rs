//! Vega string template engine.

use super::{
    Node,
    data::{ValueExt, get_nested_value},
    propset::Kind,
    renderer::NumberFormatter,
};
use crate::common::{CowExt as _, format_date_strftime};
use core::str::Chars;
use serde_json_borrow::Value;
use std::borrow::Cow;

/// The result type for template evaluation.
pub(super) type Result<T, E = Error> = core::result::Result<T, E>;

/// The error type for template evaluation.
pub(super) type Error = peg::error::ParseError<peg::str::LineCol>;

/// A template abstract syntax tree.
pub(super) struct Ast<'a> {
    /// The list of template chunks.
    chunks: Vec<Item<'a>>,
}

impl<'a> Ast<'a> {
    /// Creates a new AST for the given `text`.
    pub fn new(text: &'a str) -> Result<Self> {
        Ok(Self {
            chunks: handlebars::compile(text)?,
        })
    }

    /// Evaluates the template in the context of the given `node`.
    pub fn eval<'s>(&self, node: &Node<'s, '_>) -> Cow<'s, str> {
        if let [Item::Template { field, filters }] = self.chunks.as_slice()
            && filters.is_empty()
        {
            return field
                .eval(node)
                .as_deref()
                .map_or("undefined".into(), ValueExt::to_string);
        }

        let mut out = String::new();
        for item in &self.chunks {
            match item {
                &Item::Text(s) => out += s,
                Item::Template { field, filters } => {
                    let init = field.eval(node).unwrap_or(Cow::Owned("undefined".into()));
                    let value = filters.iter().fold(init, |out, filter| filter.apply(out));
                    out.push_str(&ValueExt::to_string(&*value));
                }
            }
        }
        Cow::Owned(out)
    }
}

/// A template chunk.
enum Item<'a> {
    /// Plain text.
    Text(&'a str),
    /// An interpolation.
    Template {
        /// The data field to interpolate.
        field: Field<'a>,
        /// Filters to apply to the interpolated data.
        filters: Vec<Filter<'a>>,
    },
}

/// A local or UTC time zone.
type Utc = bool;

/// UTC time zone.
const UTC: bool = true;

/// A data lookup.
enum Field<'a> {
    /// Look up data from the [`Node::item`].
    Datum(Option<&'a str>),
    /// Look up data from the [`Node::parent`]’s visual properties.
    Group(Option<&'a str>),
    /// Look up data from the [`Node::parent`]’s data object.
    Parent(Option<&'a str>),
}

impl Field<'_> {
    /// Retrieves some data as a string from the given `node`.
    fn eval<'b, 's>(&self, node: &'b Node<'s, '_>) -> Option<Cow<'b, Value<'s>>> {
        let (item, key) = match self {
            Field::Datum(key) => (Some(Cow::Borrowed(node.item)), key),
            Field::Group(key) => {
                let item = node
                    .parent
                    .and_then(|parent| {
                        parent
                            .mark
                            .and_then(|mark| mark.propset(Kind::Enter))
                            .map(|propset| propset.to_value(parent))
                    })
                    .map(Cow::Owned);
                (item, key)
            }
            Field::Parent(key) => (node.parent.map(|parent| Cow::Borrowed(parent.item)), key),
        };

        item.and_then(|item| {
            item.filter_map(|item| {
                if let Some(key) = key {
                    get_nested_value(item, key).map(Cow::Borrowed)
                } else {
                    Some(Cow::Borrowed(item))
                }
            })
        })
    }
}

/// A template filter.
#[derive(Clone, Copy, Debug)]
enum Filter<'a> {
    /// Take up to `n` UTF-16 codepoints from the left of the string.
    Left(i32),
    /// Return the length of the string in UTF-16 codepoints.
    Length,
    /// Convert the string to lowercase.
    Lower,
    /// Take `n` UTF-16 codepoints from the middle of the string.
    Mid(i32, i32),
    /// Format a numeric value using the given D3 formatting string.
    Number(&'a str),
    /// Pad the value to be at least `n` UTF-16 codepoints long.
    Pad(i32, Position),
    /// Return the quarter of the year of a date.
    Quarter(Utc),
    /// Take up to `n` UTF-16 codepoints from the right of the string.
    Right(i32),
    /// Take a slice of the string given a `start` and optional `end` UTF-16
    /// index. If the indices are negative, they will be calculated starting
    /// from the end of the string.
    Slice(i32, Option<i32>),
    /// Format a date using the given D3 time formatting string.
    Time(&'a str, Utc),
    /// Trim whitespace from the string.
    Trim,
    /// Take up to `n` UTF-16 codepoints from the string, inserting an ellipsis
    /// at the given position if the string is longer than `n`. The value of `n`
    /// includes the ellipsis.
    Truncate(i32, Position),
    /// Convert the string to uppercase.
    Upper,
}

impl Filter<'_> {
    /// Applies the filter to the given value.
    fn apply<'b, 's>(self, value: Cow<'b, Value<'s>>) -> Cow<'b, Value<'s>> {
        match self {
            Self::Left(n) => map_str(value, |value| (&value[..u16_index(value, n)]).into()),
            Self::Length => map_str(value, |value| format!("{}", u16_len(value)).into()),
            Self::Lower => map_str(value, |value| value.to_lowercase().into()),
            Self::Mid(start, end) => map_str(value, |value| {
                let start = u16_index(value, start);
                let end = u16_index(value, end);
                (&value[start..end]).into()
            }),
            Self::Number(fmt) => Cow::Owned(
                NumberFormatter::new(fmt)
                    .unwrap()
                    .format(value.to_f64())
                    .into(),
            ),
            Self::Pad(n, pos) => map_str(value, |value| {
                let Some(d) = u16_delta(value, n, |len, n| (n, len)) else {
                    return Cow::Borrowed(value);
                };
                let pad = " ".repeat(d);
                match pos {
                    Position::Left => format!("{pad}{value}"),
                    Position::Center => format!("{}{value}{}", &pad[..d / 2], &pad[d / 2..]),
                    Position::Right => format!("{value}{pad}"),
                }
                .into()
            }),
            Self::Quarter(is_utc) => {
                let date = value.to_date(is_utc).unwrap();
                Cow::Owned(f64::from((date.month() as u8 - 1) / 4).into())
            }
            Self::Right(n) => map_str(value, |value| (&value[u16_index(value, n)..]).into()),
            Self::Slice(start, end) => map_str(value, |value| {
                let start = u16_index(value, start);
                let end = end.map_or(value.len(), |end| u16_index(value, end));
                (&value[start..end]).into()
            }),
            Self::Time(fmt, is_utc) => {
                let date = value.to_date(is_utc).unwrap();
                let date = format_date_strftime(date, fmt.bytes()).unwrap();
                // SAFETY: The format string came from UTF-8.
                Cow::Owned(unsafe { String::from_utf8_unchecked(date) }.into())
            }
            Self::Trim => map_str(value, |value| value.trim().into()),
            Self::Truncate(n, pos) => map_str(value, |value| {
                let Ok(new_size) = usize::try_from(n) else {
                    return Cow::Borrowed("");
                };

                if new_size >= u16_len(value) {
                    return Cow::Borrowed(value);
                }

                let new_size = new_size.saturating_sub('…'.len_utf16());
                if new_size == 0 {
                    return Cow::Borrowed("…");
                }

                let mut indices = IndicesUtf16::new(value);
                match pos {
                    Position::Left => {
                        let index = indices.nth_back(new_size - 1).unwrap();
                        format!("…{}", &value[index..])
                    }
                    Position::Center => {
                        let lhs = indices.nth(new_size.div_ceil(2)).unwrap();
                        let rhs = indices.nth_back(new_size / 2 - 1).unwrap();
                        format!("{}…{}", &value[..lhs], &value[rhs..])
                    }
                    Position::Right => {
                        let index = indices.nth(new_size).unwrap();
                        format!("{}…", &value[..index])
                    }
                }
                .into()
            }),
            Self::Upper => map_str(value, |value| value.to_uppercase().into()),
        }
    }
}

/// A convenience function for extending a borrow for a string value.
fn map_str<'b, 's, F>(value: Cow<'b, Value<'s>>, f: F) -> Cow<'b, Value<'s>>
where
    for<'a> F: FnOnce(&'a str) -> Cow<'a, str>,
{
    value.map(|value| {
        let value = ValueExt::to_string(value).map(f);
        Cow::Owned(Value::Str(value))
    })
}

/// Calculates a delta, in UTF-16 codepoints, between the length of `value` and
/// the given value `n`. The function `f` decides the order of the operands.
///
/// Returns `None` if `n`, or the result of the subtraction, are ≤0.
fn u16_delta<F>(value: &str, n: i32, f: F) -> Option<usize>
where
    F: FnOnce(usize, usize) -> (usize, usize),
{
    let n = usize::try_from(n).ok()?;
    let (lhs, rhs) = f(u16_len(value), n);
    lhs.checked_sub(rhs).filter(|n| *n > 0)
}

/// Returns the number of UTF-16 codepoints in the given string.
fn u16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Returns the byte index of `n` UTF-16 codepoints for the given string.
/// Negative values of `n` start from the end of the string.
fn u16_index(value: &str, n: i32) -> usize {
    let mut as_utf16 = IndicesUtf16::new(value);
    if n < 0 {
        as_utf16.nth_back((-n).try_into().unwrap()).unwrap_or(0)
    } else {
        as_utf16.nth(n.try_into().unwrap()).unwrap_or(value.len())
    }
}

/// A padding or truncation position.
#[derive(Clone, Copy, Debug, Default)]
enum Position {
    /// Pad or truncate on the left.
    Left,
    /// Pad or truncate in the centre.
    Center,
    /// Pad or truncate on the right.
    #[default]
    Right,
}

peg::parser! {grammar handlebars() for str {
  pub rule compile() -> Vec<Item<'input>>
  = (text() / template())*

  rule text() -> Item<'input>
  = s:$((!"{{" [_])+)
  { Item::Text(s) }

  rule template() -> Item<'input>
  = "{{" _ field:field() filters:(_ "|" _ f:filter() { f })* _ "}}"
  { Item::Template { field, filters } }

  rule field() -> Field<'input>
  = "datum" s:("." s:field_part() { s })?
  { Field::Datum(s) }
  / "group" s:("." s:field_part() { s })?
  { Field::Group(s) }
  / "parent" s:("." s:field_part() { s })?
  { Field::Parent(s) }

  rule field_part() -> &'input str
  = $((!("}}" / "|" / space()) [_])+)

  rule filter() -> Filter<'input>
  = "upper" "-locale"?
  { Filter::Upper }
  / "truncate" colon() n:number() p:(comma() p:position() { p })?
  { Filter::Truncate(n, p.unwrap_or_default()) }
  / "trim"
  { Filter::Trim }
  / "time" u:"-utc"? colon() a:arg()
  { Filter::Time(a, u.is_some()) }
  / "slice" colon() s:number() e:(comma() e:number() { e })?
  { Filter::Slice(s, e) }
  / "right" colon() n:number()
  { Filter::Right(n) }
  / "quarter" u:"-utc"?
  { Filter::Quarter(u.is_some()) }
  / "pad" colon() n:number() p:(comma() p:position() { p })?
  { Filter::Pad(n, p.unwrap_or_default()) }
  / "number" colon() f:quoted_string()
  { Filter::Number(f) }
  / "month-abbrev"
  { Filter::Time("%b", !UTC) }
  / "month"
  { Filter::Time("%B", !UTC) }
  // This filter is broken in Vega and does not match the documentation. It is
  // emitted the same as slice but with a required second argument.
  / "mid" colon() s:number() comma() e:number()
  { Filter::Mid(s, e) }
  / "lower" "-locale"?
  { Filter::Lower }
  / "length"
  { Filter::Length }
  / "left" colon() n:number()
  { Filter::Left(n) }
  / "day-abbrev"
  { Filter::Time("%a", !UTC) }
  / "day"
  { Filter::Time("%A", !UTC) }

  rule arg() -> &'input str
  = quoted_string()
  / $((!space() [^'"'|'\''|'|'|','])+)

  rule colon()
  = _ ":" _

  rule comma()
  = _ "," _

  rule number() -> i32
  = a:arg() { a.parse().unwrap_or(0) }

  rule position() -> Position
  = "left" { Position::Left }
  / ("middle" / "center") { Position::Center }
  / arg() { Position::Right }

  // The grammar, as implemented in Vega by our good boy the regular expression,
  // did not support escape sequences
  rule quoted_string() -> &'input str
  = q:['"'|'\''] s:$([c if c != q]*) [c if c == q]
  { s }

  rule space()
  = [c if c.is_whitespace()]

  rule _ = space()*

}}

// SPDX-SnippetBegin
// SPDX-License-Identifier: MIT-or-Apache-2.0
// SPDX-SnippetComment: Adapted from widestring 1.2.1

/// An iterator over the positions of UTF-16 codepoints in a string slice.
#[derive(Debug, Clone)]
pub struct IndicesUtf16<'a> {
    /// The current byte offset of the forward iterator.
    forward_offset: usize,
    /// The current byte offset of the reverse iterator.
    back_offset: usize,
    /// The inner character iterator.
    iter: Chars<'a>,
}

impl<'a> IndicesUtf16<'a> {
    /// Creates a new `IndicesUtf16`.
    fn new(s: &'a str) -> Self {
        Self {
            forward_offset: 0,
            back_offset: s.len(),
            iter: s.chars(),
        }
    }
}

impl Iterator for IndicesUtf16<'_> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let result = self.iter.next();
        if let Some(c) = result {
            let offset = self.forward_offset;
            self.forward_offset += c.len_utf16();
            Some(offset)
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl core::iter::FusedIterator for IndicesUtf16<'_> {}

impl DoubleEndedIterator for IndicesUtf16<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let result = self.iter.next_back();
        if let Some(c) = result {
            self.back_offset -= c.len_utf16();
            Some(self.back_offset)
        } else {
            None
        }
    }
}
// SPDX-SnippetEnd

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate() {
        let input = Cow::Borrowed(&Value::Str("hello world".into()));

        let result = Filter::Truncate(6, Position::Left).apply(input.clone());
        assert_eq!(ValueExt::to_string(&*result), "…world");

        let result = Filter::Truncate(6, Position::Right).apply(input.clone());
        assert_eq!(ValueExt::to_string(&*result), "hello…");

        let result = Filter::Truncate(6, Position::Center).apply(input);
        assert_eq!(ValueExt::to_string(&*result), "hel…ld");
    }
}
