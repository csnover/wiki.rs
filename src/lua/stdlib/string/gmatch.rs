//! Piecewise text matching engine.

use super::engine::{BackingType, MatchRanges, Result, find_first_match};
use crate::lua::prelude::*;

/// Finds the next match of the pattern `pattern` in the string `s` starting
/// from `at`.
#[inline]
pub fn gmatch_next<'gc, B: BackingType + ?Sized + 'gc>(
    ctx: Context<'gc>,
    s: VmString<'gc>,
    pattern: VmString<'gc>,
    mut at: usize,
    last_next: Option<usize>,
) -> Result<Option<(usize, Table<'gc>)>, VmError<'gc>> {
    let s = B::from_value(ctx, Value::String(s))?;
    let pattern = B::from_value(ctx, Value::String(pattern))?;
    Ok(loop {
        if at <= s.len()
            && let Some(MatchRanges {
                full_match,
                captures,
            }) = find_first_match(s, pattern, at, false)?
        {
            let end = full_match.end;
            if last_next == Some(end) {
                at += 1;
                continue;
            }
            let result = Table::new(&ctx);
            if captures.is_empty() {
                result.set(ctx, 1, ctx.intern(&s.as_bytes()[full_match]))?;
            } else {
                for (index, capture) in captures.into_iter().enumerate() {
                    result.set(ctx, i64::try_from(index + 1)?, capture.into_value(ctx, s))?;
                }
            }
            break Some((end, result));
        }
        break None;
    })
}
