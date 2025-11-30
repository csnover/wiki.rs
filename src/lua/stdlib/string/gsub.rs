//! Piecewise text substitution engine.

use super::engine::{
    BackingType, Capture, CaptureRange, Error, PrimitiveType, Result, find_first_match,
};
use crate::lua::prelude::*;
use core::{
    borrow::Borrow,
    ops::{Range, RangeFrom},
    slice::SliceIndex,
};
use piccolo::{Function, MetaMethod, SequenceReturn, async_sequence};

/// A piecewise text substitution engine.
///
/// Allows interruptible iterative replacement of strings by separating the
/// matching and replacing parts.
pub(crate) struct GSub<B: BackingType + ?Sized> {
    /// The search pattern.
    pattern: B::Owned,
    /// The number of possible replacements that may occur.
    replacements: usize,
    /// The number of replacements which have occurred.
    found: usize,
    /// The accumulator.
    result: B::Owned,
    /// The end position of the last matching candidate.
    last_pos: usize,
    /// The end position of the last replacement.
    last_replace: usize,
    /// The currently matched range awaiting a replacement.
    current: Range<usize>,
}

impl<B> GSub<B>
where
    B: BackingType + ?Sized,
    <B as ToOwned>::Owned:
        Default + Extend<B::Primitive> + for<'a> Extend<&'a B::Primitive> + Borrow<B>,
    RangeFrom<usize>: SliceIndex<B, Output = B>,
{
    /// Creates a new substitution engine.
    pub fn new(pattern: &B, n: Option<usize>) -> Self {
        Self {
            pattern: pattern.to_owned(),
            replacements: if pattern.starts_with(B::Primitive::from_ascii(b'^')) {
                1
            } else {
                n.unwrap_or(usize::MAX)
            },
            found: 0,
            result: B::Owned::default(),
            last_pos: 0,
            last_replace: usize::MAX,
            current: 0..0,
        }
    }

    /// Returns the final string and the number of replacements, consuming the
    /// engine.
    #[must_use]
    pub fn finish(mut self, input: &B) -> (B::Owned, usize) {
        if let Some(input) = input.get(self.last_pos..) {
            self.result.extend(input.chars());
        }
        (self.result, self.found)
    }

    /// Advances to the next match in the given input.
    ///
    /// # Errors
    ///
    /// If a syntax error is encountered in the pattern string, an [`Error`] is
    /// returned.
    pub fn next<'gc>(
        &mut self,
        ctx: Context<'gc>,
        input: &B,
    ) -> Result<Option<(Capture<'gc>, Vec<Capture<'gc>>)>> {
        if self.replacements == 0 {
            return Ok(None);
        }

        Ok(
            if let Some(ranges) =
                find_first_match(input, self.pattern.borrow(), self.last_pos, true)?
            {
                self.found += 1;
                self.replacements -= 1;
                self.current = ranges.full_match;
                Some(self.captures(ctx, input, &ranges.captures))
            } else {
                None
            },
        )
    }

    /// Replaces the current match with the given replacement text. If the given
    /// replacement is `None`, the original match is kept in the string.
    pub fn replace(&mut self, input: &B, replacement: Option<&B>) {
        if self.current.end != self.last_replace {
            self.result
                .extend(input[self.last_pos..self.current.start].chars());
            if let Some(replacement) = replacement {
                self.result.extend(replacement.chars());
            } else {
                self.result.extend(input[self.current.clone()].chars());
            }
        }

        self.last_replace = self.current.end;
        self.last_pos = self.current.end;

        if self.current.is_empty() {
            if let Some(input) = input.get(self.last_pos..)
                && let Some(c) = input.chars().next()
            {
                self.result.extend([c]);
                self.last_pos += c.len_utf8();
            } else {
                self.last_pos += 1;
            }
            if self.last_pos > input.len() {
                self.replacements = 0;
            }
        }
    }

    /// Returns the capture groups for the current match.
    fn captures<'gc>(
        &self,
        ctx: Context<'gc>,
        input: &B,
        captures: &[CaptureRange],
    ) -> (Capture<'gc>, Vec<Capture<'gc>>) {
        (
            ctx.intern(&input.as_bytes()[self.current.clone()]).into(),
            captures
                .iter()
                .map(|range| range.clone().into_value(ctx, input))
                .collect::<Vec<_>>(),
        )
    }
}

/// Finds and replaces matching patterns within a string.
pub fn gsub_lua<'gc, B>(
    ctx: Context<'gc>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, VmError<'gc>>
where
    B: BackingType + ?Sized + 'static,
    <B as ToOwned>::Owned: Default
        + Extend<B::Primitive>
        + for<'a> Extend<&'a B::Primitive>
        + Borrow<B>
        + core::ops::Deref<Target = B>,
    RangeFrom<usize>: SliceIndex<B, Output = B>,
{
    let (s, pattern, repl, n) =
        stack.consume::<(VmString<'_>, VmString<'_>, Value<'_>, Option<i64>)>(ctx)?;

    let pattern = B::from_value(ctx, Value::String(pattern))?;
    let n = n.map(usize::try_from).transpose()?;
    let mut engine = GSub::new(pattern, n);

    let (value, n) = match repl {
        repl if repl.is_implicit_string() => {
            let s = B::from_value(ctx, Value::String(s))?;
            let repl = B::from_value(ctx, repl)?;
            while let Some((ref full_match, rest)) = engine.next(ctx, s)? {
                let repl = repl_string(ctx, repl, full_match, &rest)?;
                engine.replace(s, Some(&repl));
            }
            engine.finish(s)
        }
        Value::Table(t) if can_go_fast(ctx, t) => {
            let s = B::from_value(ctx, Value::String(s))?;
            while let Some((full_match, rest)) = engine.next(ctx, s)? {
                let key = rest.first().unwrap_or(&full_match);
                repl_value(ctx, &mut engine, s, t.get_value(ctx, key))?;
            }
            engine.finish(s)
        }
        Value::Table(_) | Value::Function(_) => {
            return Ok(gsub_slow(ctx, s, engine, repl));
        }
        _ => {
            return Err(
                "invalid `repl` value, expected `string`, `table` or `function`"
                    .into_value(ctx)
                    .into(),
            );
        }
    };

    stack.replace(ctx, (value.to_value(ctx), i64::try_from(n)?));
    Ok(CallbackReturn::Return)
}

/// Returns true if the fast path for table lookups can be used.
fn can_go_fast<'gc>(ctx: Context<'gc>, t: Table<'gc>) -> bool {
    t.metatable()
        .is_none_or(|table| !matches!(table.get_value(ctx, MetaMethod::Index), Value::Function(_)))
}

/// The slow (metacall) version of `string.gsub`.
fn gsub_slow<'gc, B>(
    ctx: Context<'gc>,
    s: VmString<'gc>,
    mut engine: GSub<B>,
    repl: Value<'gc>,
) -> CallbackReturn<'gc>
where
    B: BackingType + ?Sized + 'static,
    <B as ToOwned>::Owned: Default
        + Extend<B::Primitive>
        + for<'a> Extend<&'a B::Primitive>
        + Borrow<B>
        + core::ops::Deref<Target = B>,
    RangeFrom<usize>: SliceIndex<B, Output = B>,
{
    let s = async_sequence(&ctx, move |locals, mut seq| {
        let s = locals.stash(&ctx, s);
        let (f, table) = match repl {
            Value::Function(f) => (f, None),
            Value::Table(t) => (
                t.metatable()
                    .map(|t| t.get::<_, Function<'gc>>(ctx, MetaMethod::Index).unwrap())
                    .unwrap(),
                Some(locals.stash(&ctx, t)),
            ),
            _ => unreachable!(),
        };
        let f = locals.stash(&ctx, f);
        async move {
            loop {
                let bottom = seq.try_enter(|ctx, locals, _, mut stack| {
                    let s = B::from_value(ctx, Value::String(locals.fetch(&s)))?;
                    if let Some((full, captures)) = engine.next(ctx, s)? {
                        let bottom = stack.len();
                        if let Some(table) = &table {
                            let table = locals.fetch(table);
                            stack.into_back(ctx, (table, captures.first().unwrap_or(&full)));
                        } else if captures.is_empty() {
                            stack.push_back(full);
                        } else {
                            stack.extend(captures);
                        }
                        Ok(Some(bottom))
                    } else {
                        Ok(None)
                    }
                })?;

                if let Some(bottom) = bottom {
                    seq.call(&f, bottom).await?;
                    seq.try_enter(|ctx, locals, _, mut stack| {
                        let s = B::from_value(ctx, Value::String(locals.fetch(&s)))?;
                        let value = stack.from_front::<Value<'_>>(ctx)?;
                        stack.resize(bottom);
                        repl_value(ctx, &mut engine, s, value)
                    })?;
                } else {
                    break;
                }
            }

            seq.try_enter(|ctx, locals, _, mut stack| {
                let s = B::from_value(ctx, Value::String(locals.fetch(&s)))?;
                let (value, n) = engine.finish(s);
                stack.into_back(ctx, (value.to_value(ctx), i64::try_from(n)?));
                Ok(())
            })?;

            Ok(SequenceReturn::Return)
        }
    });

    CallbackReturn::Sequence(s)
}

/// Replaces a match using a replacement string.
fn repl_string<'gc, B>(
    ctx: Context<'gc>,
    repl: &B,
    full_match: &Capture<'gc>,
    captures: &[Capture<'gc>],
) -> Result<B::Owned>
where
    B: BackingType + ?Sized + 'gc,
    B::Owned: Default + for<'a> Extend<&'a B::Primitive> + Extend<B::Primitive>,
{
    let mut result = B::Owned::default();
    for token in ReplIter(repl.chars()) {
        match token? {
            ReplToken::Literal(b) => {
                result.extend([b]);
            }
            ReplToken::CaptureRef(idx) => {
                let idx = usize::from(idx);
                if idx == 0 || (idx == 1 && captures.is_empty()) {
                    result.extend(
                        B::from_value(ctx, *full_match)
                            .map_err(|_| Error::InvalidReplacement)?
                            .chars(),
                    );
                } else if idx <= captures.len() {
                    result.extend(
                        B::from_value(ctx, captures[idx - 1])
                            .map_err(|_| Error::InvalidReplacement)?
                            .chars(),
                    );
                } else {
                    return Err(Error::InvalidCaptureIndex { pos: 0, index: idx });
                }
            }
        }
    }
    Ok(result)
}

/// Replaces a match with a value from a table or function callback.
fn repl_value<'gc, B>(
    ctx: Context<'gc>,
    engine: &mut GSub<B>,
    s: &'gc B,
    value: Value<'gc>,
) -> Result<(), VmError<'gc>>
where
    B: BackingType + ?Sized,
    <B as ToOwned>::Owned:
        Default + Extend<B::Primitive> + for<'a> Extend<&'a B::Primitive> + Borrow<B>,
    RangeFrom<usize>: SliceIndex<B, Output = B>,
{
    match value {
        Value::Nil | Value::Boolean(false) => {
            engine.replace(s, None);
        }
        value if value.is_implicit_string() => {
            engine.replace(s, Some(B::from_value(ctx, value)?));
        }
        value => {
            Err(format!("invalid replacement value (a {})", value.type_name()).into_value(ctx))?;
        }
    }

    Ok(())
}

/// A replacement string token (`%N`) used to specify a capture group from the
/// input as the source of the replacement.
///
/// As in PUC-Lua, only capture groups 0–9 can be specified.
enum ReplToken<C: PrimitiveType> {
    /// A literal `%`.
    Literal(C),
    /// Use the string captured in group `n` as the replacement.
    CaptureRef(u8),
}

/// Converts the replacement string `repl` into a sequence of [`ReplToken`].
struct ReplIter<I>(I)
where
    I: Iterator,
    I::Item: PrimitiveType;

impl<I> Iterator for ReplIter<I>
where
    I: Iterator,
    I::Item: PrimitiveType,
{
    type Item = Result<ReplToken<I::Item>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|c| {
            if c.as_ascii() == b'%'
                && let Some(c) = self.0.next()
            {
                match c.as_ascii() {
                    // Clippy: Values are always 0-9
                    #[allow(clippy::cast_possible_truncation)]
                    next_byte if next_byte.is_ascii_digit() => {
                        Ok(ReplToken::CaptureRef(next_byte.to_digit().unwrap() as u8))
                    }
                    b'%' => Ok(ReplToken::Literal(c)),
                    _ => Err(Error::InvalidReplacement),
                }
            } else {
                Ok(ReplToken::Literal(c))
            }
        })
    }
}
