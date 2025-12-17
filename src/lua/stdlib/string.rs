//! Lua 5.1-compatible string standard library.

use crate::lua::prelude::*;
use core::cell::Cell;
use find::match_lua;
use gmatch::gmatch_next;
use piccolo::TypeError;

pub(crate) mod engine;
pub(crate) mod find;
mod format;
pub(crate) mod gmatch;
pub(crate) mod gsub;

/// Loads the string library.
#[allow(clippy::too_many_lines)]
pub fn load_string(ctx: Context<'_>) -> Result<(), TypeError> {
    let string = ctx.get_global::<Table<'_>>("string")?;

    string.set_field(
        ctx,
        "gsub",
        Callback::from_fn(&ctx, |ctx, _, stack| gsub::gsub_lua::<[u8]>(ctx, stack)),
    );

    string.set_field(
        ctx,
        "find",
        Callback::from_fn(&ctx, |ctx, _, stack| find::find_lua::<[u8]>(ctx, stack)),
    );

    string.set_field(
        ctx,
        "rep",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (string, times) = stack.consume::<(VmString<'_>, i64)>(ctx)?;
            let result = string.to_str()?.repeat(times.try_into()?);
            stack.replace(ctx, result);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "match",
        Callback::from_fn(&ctx, |ctx, _, stack| match_lua::<[u8]>(ctx, stack)),
    );

    string.set_field(
        ctx,
        "gmatch",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (s, pattern, init) =
                stack.consume::<(VmString<'_>, VmString<'_>, Option<i64>)>(ctx)?;

            let pos = Cell::new(calculate_start_index(s.as_bytes(), init));
            let last_next = Cell::new(None);
            let s = ctx.stash(s);
            let pattern = ctx.stash(pattern);
            let func = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let s = ctx.fetch(&s);
                let pattern = ctx.fetch(&pattern);
                stack.clear();
                if let Some((next, captures)) =
                    gmatch_next::<[u8]>(ctx, s, pattern, pos.get(), last_next.get())?
                {
                    pos.set(next);
                    last_next.set(Some(next));
                    for (_, v) in captures {
                        stack.into_back(ctx, v);
                    }
                } else {
                    stack.push_back(Value::Nil);
                }
                Ok(CallbackReturn::Return)
            });

            stack.replace(ctx, func);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(ctx, "gfind", string.get_value(ctx, "gmatch"));

    string.set_field(ctx, "format", Callback::from_fn(&ctx, format::format_impl));

    Ok(())
}

/// Calculates the 0-based absolute start index and length from the 1-based
/// `start` and `end` indices. If an input index is negative, its origin is the
/// end of the string `s`.
#[inline]
pub fn calculate_start_count<'gc, B: engine::BackingType + ?Sized>(
    s: &B,
    mut start: i64,
    mut end: i64,
) -> Result<Option<(usize, usize)>, VmError<'gc>> {
    let len = i64::try_from(s.chars().count())?;
    if start < 0 {
        start += len + 1;
    }
    if end < 0 {
        end += len + 1;
    }

    Ok(if end < start {
        None
    } else {
        let start = usize::try_from(start.clamp(1, len + 1) - 1)?;
        let end = usize::try_from(end.clamp(1, len + 1))?;
        let count = end - start;
        Some((start, count))
    })
}

/// Calculates the 0-based absolute start index from the 1-based `init` index.
/// If the input index is negative, its origin is the end of the string `s`.
#[inline]
fn calculate_start_index<B: engine::BackingType + ?Sized>(s: &B, init: Option<i64>) -> usize {
    match init {
        Some(init @ 1..) => {
            let init = usize::try_from(init - 1).unwrap();
            s.char_indices().nth(init).map_or_else(
                || {
                    // When `B = str`, `init` cannot be used directly because it is
                    // a character count, but this is calculating a byte index.
                    // Also, because Lua treats end-of-string different from
                    // beyond-end-of-string in some cases, it is necessary to make
                    // the result be *at least* `s.len()`, but more if `init` was
                    // actually already beyond-end-of-string.
                    s.len() + init - s.chars().count()
                },
                |(pos, _)| pos,
            )
        }
        Some(init @ ..=-1) => {
            let init = (init + 1).unsigned_abs().try_into().unwrap();
            s.char_indices().nth_back(init).map_or(0, |(pos, _)| pos)
        }
        _ => 0,
    }
}
