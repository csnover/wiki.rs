//! Basic string matching functions.

use super::{
    calculate_start_count, calculate_start_index,
    engine::{BackingType, MatchRanges, find_first_match},
};
use crate::lua::prelude::*;
use core::{borrow::Borrow, ops::RangeFrom, slice::SliceIndex};

/// Finds a pattern in a string, optionally starting from an index. Returns
/// the 1-indexed start and end positions of the match, plus any captured
/// strings.
pub fn find_lua<'gc, B>(
    ctx: Context<'gc>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, VmError<'gc>>
where
    B: BackingType + ?Sized + 'gc,
    <B as ToOwned>::Owned: Default
        + Extend<B::Primitive>
        + for<'a> Extend<&'a B::Primitive>
        + Borrow<B>
        + core::ops::Deref<Target = B>,
    RangeFrom<usize>: SliceIndex<B, Output = B>,
{
    let (s, pattern, init, plain) =
        stack.consume::<(VmString<'_>, VmString<'_>, Option<i64>, Option<bool>)>(ctx)?;

    let s = B::from_value(ctx, Value::String(s))?;
    let pattern = B::from_value(ctx, Value::String(pattern))?;
    let init0 = init;
    let init = calculate_start_index(s, init);

    let result = if plain == Some(true) {
        if pattern.is_empty() {
            let pos = {
                let len = s.chars().count();
                match init0 {
                    Some(init @ 1..) => usize::try_from(init - 1).unwrap().min(len),
                    Some(init @ ..=-1) => len.saturating_add_signed(isize::try_from(init).unwrap()),
                    _ => 0,
                }
            };
            Some((pos, pos, vec![]))
        } else if let Some(pos) = s[init..].find(pattern) {
            let start = s.char_count(init + pos);
            let end = start + pattern.chars().count();
            Some((start, end, vec![]))
        } else {
            None
        }
    } else if let Some(MatchRanges {
        full_match,
        captures,
    }) = find_first_match(s, pattern, init, true)?
    {
        let start = s.char_count(full_match.start);
        let end = start + s[full_match].chars().count();
        Some((start, end, captures))
    } else {
        None
    };

    if let Some((start, end, captures)) = result {
        stack.into_back(ctx, (i64::try_from(start + 1)?, i64::try_from(end)?));
        for capture in captures {
            stack.into_back(ctx, capture.into_value(ctx, s));
        }
    } else {
        stack.replace(ctx, Value::Nil);
    }

    Ok(CallbackReturn::Return)
}

/// Matches a pattern in a string, optionally starting from an index.
/// Returns the matched captures, or the whole match if no capture groups
/// were specified.
pub fn match_lua<'gc, B>(
    ctx: Context<'gc>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, VmError<'gc>>
where
    B: BackingType + ?Sized + 'gc,
    <B as ToOwned>::Owned: Default
        + Extend<B::Primitive>
        + for<'a> Extend<&'a B::Primitive>
        + Borrow<B>
        + core::ops::Deref<Target = B>,
    RangeFrom<usize>: SliceIndex<B, Output = B>,
{
    let (s, pattern, init) = stack.consume::<(VmString<'_>, VmString<'_>, Option<i64>)>(ctx)?;

    let s = B::from_value(ctx, Value::String(s)).unwrap();
    let pattern = B::from_value(ctx, Value::String(pattern)).unwrap();
    let init = calculate_start_index(s, init);

    if let Some(MatchRanges {
        full_match,
        captures,
    }) = find_first_match(s, pattern, init, true)?
    {
        if captures.is_empty() {
            stack.into_back(ctx, s[full_match].to_value(ctx));
        } else {
            stack.extend(
                captures
                    .into_iter()
                    .map(|capture| capture.into_value(ctx, s)),
            );
        }
    } else {
        stack.replace(ctx, Value::Nil);
    }

    Ok(CallbackReturn::Return)
}

/// Returns a slice of the given string.
pub fn sub_lua<'gc, B: BackingType + ?Sized + 'gc>(
    ctx: Context<'gc>,
    (s, start, end): (VmString<'gc>, Option<i64>, Option<i64>),
) -> Result<VmString<'gc>, VmError<'gc>> {
    let s = B::from_value(ctx, Value::String(s)).unwrap();
    let start = start.unwrap_or(1);
    let end = end.unwrap_or(-1);

    let Some((start, count)) = calculate_start_count(s, start, end)? else {
        return Ok(VmString::from_static(&ctx, ""));
    };

    let mut iter = s.char_indices();
    let start = iter.nth(start).map_or(s.len(), |(pos, _)| pos);
    let end = if count == 0 {
        start
    } else {
        iter.nth(count - 1).map_or(s.len(), |(pos, _)| pos)
    };

    Ok(ctx.intern(&s.as_bytes()[start..end]))
}
