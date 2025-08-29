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
    at: usize,
) -> Result<(usize, Table<'gc>), VmError<'gc>> {
    let s = B::from_value(ctx, Value::String(s))?;
    let pattern = B::from_value(ctx, Value::String(pattern))?;
    Ok(
        if let Some(MatchRanges {
            full_match,
            captures,
        }) = find_first_match(s, pattern, at, false)?
        {
            let at = full_match.end;
            let result = Table::new(&ctx);
            if captures.is_empty() {
                result.set(ctx, 1, ctx.intern(&s.as_bytes()[full_match]))?;
            } else {
                for (index, capture) in captures.into_iter().enumerate() {
                    result.set(ctx, i64::try_from(index + 1)?, capture.into_value(ctx, s))?;
                }
            }
            (at + 1, result)
        } else {
            (at, Table::new(&ctx))
        },
    )
}
