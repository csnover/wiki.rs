//! Lua 5.1-compatible mathematics standard library.

use crate::lua::prelude::*;
use piccolo::{
    Function, TypeError,
    meta_ops::{self, MetaResult},
};

/// Loads the maths library.
pub fn load_math(ctx: Context<'_>) -> Result<(), TypeError> {
    let math = ctx.get_global::<Table<'_>>("math")?;

    // Lua 5.4 does not allow implicit string conversion
    for key in ["min", "max"] {
        let function = ctx.stash(math.get::<_, Function<'_>>(ctx, key)?);
        math.set_field(
            ctx,
            key,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                for op in 0..stack.len() {
                    if matches!(stack[op], Value::String(_))
                        && let Some(value) = stack[op].to_numeric()
                    {
                        stack[op] = value;
                    }
                }
                Ok(CallbackReturn::Call {
                    function: ctx.fetch(&function),
                    then: None,
                })
            }),
        );
    }

    math.set_field(
        ctx,
        "mod",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (lhs, rhs) = stack.consume::<(Value<'_>, Value<'_>)>(ctx)?;

            Ok(match meta_ops::modulo(ctx, lhs, rhs)? {
                MetaResult::Value(value) => {
                    stack.push_back(value);
                    CallbackReturn::Return
                }
                MetaResult::Call(call) => {
                    stack.extend(call.args);
                    CallbackReturn::Call {
                        function: call.function,
                        then: None,
                    }
                }
            })
        }),
    );

    math.set_field(
        ctx,
        "log10",
        extras::callback("log10", &ctx, |_, v: f64| Some(v.log10())),
    );

    math.set_field(
        ctx,
        "pow",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (lhs, rhs) = stack.consume::<(f64, f64)>(ctx)?;
            stack.replace(ctx, lhs.powf(rhs));
            Ok(CallbackReturn::Return)
        }),
    );

    Ok(())
}

// TODO: Expose helpers upstream?
// SPDX-SnippetBegin
// SPDX-License-Identifier: MIT
// SPDX-SnippetComment: Copied from piccolo
mod extras {
    //! Utility functions extracted from piccolo.

    use gc_arena::Mutation;
    use piccolo::{Callback, CallbackReturn, Context, FromMultiValue, IntoMultiValue, IntoValue};

    /// A helper for writing simple callbacks which receive arguments of type
    /// `A` and return a value of type `R`.
    pub(super) fn callback<'gc, F, A, R>(
        name: &'static str,
        mc: &Mutation<'gc>,
        f: F,
    ) -> Callback<'gc>
    where
        F: Fn(Context<'gc>, A) -> Option<R> + 'static,
        A: FromMultiValue<'gc>,
        R: IntoMultiValue<'gc>,
    {
        Callback::from_fn(mc, move |ctx, _, mut stack| {
            if let Some(res) = f(ctx, stack.consume(ctx)?) {
                stack.replace(ctx, res);
                Ok(CallbackReturn::Return)
            } else {
                Err(format!("Bad argument to {name}").into_value(ctx).into())
            }
        })
    }
}
// SPDX-SnippetEnd
