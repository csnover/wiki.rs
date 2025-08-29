//! Lua 5.1-compatible implementations of the Lua stdlib.
//!
//! piccolo targets Lua 5.4 compatibility, but Scribunto uses Lua 5.1, and many
//! of the authored modules *require* Lua 5.1 behaviour to not break
//! catastrophically.

use super::prelude::*;
#[cfg(test)]
pub use load::load_load_text;
pub use math::load_math;
pub use os::load_os;
use piccolo::{BoxSequence, Sequence, SequencePoll, meta_ops};
use std::pin::Pin;
pub use string::{
    calculate_start_count,
    find::{find_lua, match_lua, sub_lua},
    gmatch::gmatch_next,
    gsub, load_string,
};
pub use table::load_table;

#[cfg(test)]
mod load;
mod math;
mod os;
mod string;
mod table;
#[cfg(test)]
mod tests;

/// Loads Lua 5.1-compatible global functions and variables.
pub fn load_compat(ctx: Context<'_>) {
    let table = ctx.get_global::<Table<'_>>("table").unwrap();
    ctx.set_global("unpack", table.get_value(ctx, "unpack"));
    ctx.set_global("_G", ctx.globals());

    if ctx.get_global_value("io").is_nil() {
        ctx.set_global("io", Table::new(&ctx));
    }
}

/// Loads helpful debugging functionality.
pub fn load_debug(ctx: Context<'_>) {
    #[derive(gc_arena::Collect)]
    #[collect(require_static)]
    pub struct PCall;

    impl<'gc> Sequence<'gc> for PCall {
        fn poll(
            self: Pin<&mut Self>,
            ctx: Context<'gc>,
            _exec: Execution<'gc, '_>,
            mut stack: Stack<'gc, '_>,
        ) -> Result<SequencePoll<'gc>, VmError<'gc>> {
            stack.into_front(ctx, true);
            Ok(SequencePoll::Return)
        }

        fn error(
            self: Pin<&mut Self>,
            ctx: Context<'gc>,
            _exec: Execution<'gc, '_>,
            error: VmError<'gc>,
            mut stack: Stack<'gc, '_>,
        ) -> Result<SequencePoll<'gc>, VmError<'gc>> {
            log::debug!("pcall suppressed {error:#}");
            stack.replace(ctx, (false, error));
            Ok(SequencePoll::Return)
        }
    }

    ctx.set_global(
        "pcall",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let function = meta_ops::call(ctx, stack.get(0))?;
            stack.pop_front();
            Ok(CallbackReturn::Call {
                function,
                then: Some(BoxSequence::new(&ctx, PCall)),
            })
        }),
    );

    if ctx.get_global_value("debug").is_nil() {
        ctx.set_global("debug", Table::new(&ctx));
    }

    let debug = ctx.get_global::<Table<'_>>("debug").unwrap();
    debug.set_field(
        ctx,
        "inspect",
        Callback::from_fn(&ctx, |_, _, mut stack| {
            for s in stack.drain(..) {
                dbg!(s);
            }
            Ok(CallbackReturn::Return)
        }),
    );
}

// TODO: Expose helpers upstream?
// SPDX-SnippetBegin
// SPDX-License-Identifier: MIT
// SPDX-SnippetComment: Copied from piccolo
mod extras {
    //! Utility functions extracted from piccolo.

    use piccolo::{
        Context, Stack, StashedError, StashedFunction, StashedTable, StashedValue, Value,
        async_callback::{AsyncSequence, Locals},
        meta_ops::{self, MetaResult},
    };

    /// Calculates the amount of fuel that will be used when performing an
    /// action.
    pub(super) fn count_fuel(per_item: i32, len: usize) -> i32 {
        i32::try_from(len)
            .unwrap_or(i32::MAX)
            .saturating_mul(per_item)
    }

    /// Prepares the stack for a metacall.
    pub(super) fn prep_metaop_call<'gc, const N: usize>(
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
        locals: Locals<'gc, '_>,
        res: MetaResult<'gc, N>,
    ) -> Option<StashedFunction> {
        match res {
            MetaResult::Value(v) => {
                stack.push_back(v);
                None
            }
            MetaResult::Call(call) => {
                stack.extend(call.args);
                Some(locals.stash(&ctx, call.function))
            }
        }
    }

    /// Performs `__index` meta calls, if needed, for retrieving `key` from
    /// `table` and places the value on the stack.
    pub(super) async fn index_helper(
        seq: &mut AsyncSequence,
        table: &StashedTable,
        key: &StashedValue,
        bottom: usize,
    ) -> Result<(), StashedError> {
        let call = seq.try_enter(|ctx, locals, _, stack| {
            let table = locals.fetch(table);
            let key = locals.fetch(key);
            let call = meta_ops::index(ctx, Value::Table(table), key)?;
            Ok(prep_metaop_call(ctx, stack, locals, call))
        })?;
        if let Some(call) = call {
            seq.call(&call, bottom).await?;
            seq.enter(|_, _, _, mut stack| {
                stack.resize(bottom + 1); // Truncate stack
            });
        }
        Ok(())
    }
}
// SPDX-SnippetEnd
