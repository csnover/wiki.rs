//! Lua 5.1-compatible table standard library.

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::extras::count_fuel;
use crate::lua::prelude::*;
use anyhow::Context as _;
use piccolo::{MetaMethod, SequenceReturn, TypeError, async_sequence, meta_ops};

/// Loads the table library.
#[allow(clippy::cast_precision_loss)]
pub fn load_table(ctx: Context<'_>) -> Result<(), TypeError> {
    let table = ctx.get_global::<Table<'_>>("table")?;

    table.set_field(
        ctx,
        "getn",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let t = stack.consume::<Table<'_>>(ctx)?;
            stack.replace(
                ctx,
                if let Some(n) = t.get_value(ctx, "n").to_numeric() {
                    n
                } else {
                    t.length().into()
                },
            );
            Ok(CallbackReturn::Return)
        }),
    );

    table.set_field(
        ctx,
        "maxn",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let t = stack.consume::<Table<'_>>(ctx)?;
            let max = t.into_iter().fold(0.0, |acc, (k, _)| match k {
                Value::Integer(n) => (n as f64).max(acc),
                Value::Number(n) => n.max(acc),
                _ => acc,
            });
            stack.replace(ctx, max);
            Ok(CallbackReturn::Return)
        }),
    );

    // These are Lua 5.1/5.2-compatible versions of these functions. Required
    // because at least 'Module:Article stub box' calls `table.insert` from
    // inside `__newindex`, which causes an infinite loop in Lua 5.4.
    table.set_field(ctx, "insert", Callback::from_fn(&ctx, table_insert_impl));

    table.set_field(ctx, "remove", Callback::from_fn(&ctx, table_remove_impl));

    Ok(())
}

/// Amount of fuel used per item added or removed from a table.
// SPDX-SnippetBegin
// SPDX-License-Identifier: MIT
// SPDX-SnippetComment: Copied from piccolo
const FUEL_PER_SHIFTED_ITEM: i32 = 1;

/// A Lua 5.1-compatible implementation of `table.remove`.
// Minor difference from PRLua: When the table is empty, table.remove(t, #t),
// will return nil, even if the table has an element at index 0.
fn table_remove_impl<'gc>(
    ctx: Context<'gc>,
    mut exec: Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
    let (table, index): (Table<'_>, Option<i64>) = stack.consume(ctx)?;
    let length;

    let metatable = table.metatable();
    let use_fallback = metatable.is_some_and(|mt| !mt.get_value(ctx, MetaMethod::Len).is_nil());

    if use_fallback {
        length = None;
    } else {
        // Try the fast path
        let mut inner = table.into_inner().borrow_mut(&ctx);
        match extras::array_remove_shift(&mut inner.raw_table, index) {
            (extras::RawArrayOpResult::Success(val), len) => {
                // Consume fuel after the operation to avoid computing length twice
                let start_idx = index.unwrap_or(len as i64).try_into().unwrap_or(0);
                let shifted_items = len.saturating_sub(start_idx);
                exec.fuel()
                    .consume(count_fuel(FUEL_PER_SHIFTED_ITEM, shifted_items));

                stack.push_back(val);
                return Ok(CallbackReturn::Return);
            }
            (extras::RawArrayOpResult::Possible, len) => {
                length = Some(len);
            }
            (extras::RawArrayOpResult::Failed, _) => {
                return Err("Invalid index passed to table.remove"
                    .into_value(ctx)
                    .into());
            }
        }
    }

    // Fast path failed, fall back to direct indexing
    let s = async_sequence(&ctx, |locals, mut seq| {
        let table = locals.stash(&ctx, table);
        async move {
            let length = if let Some(len) = length {
                len as i64
            } else {
                let call = seq.try_enter(|ctx, locals, _, stack| {
                    let table = locals.fetch(&table);
                    let call = meta_ops::len(ctx, Value::Table(table))
                        .context("error while calling __len")?;
                    Ok(super::extras::prep_metaop_call(ctx, stack, locals, call))
                })?;
                if let Some(call) = call {
                    seq.call(&call, 0).await?;
                }
                seq.try_enter(|ctx, _, _, mut stack| {
                    Ok(stack
                        .consume::<i64>(ctx)
                        .context("__len returned invalid length")?)
                })?
            };

            let index = index.unwrap_or(length);

            // either index and length are zero, or index == length + 1 (without overflow)
            if index.saturating_sub(1) == length {
                seq.enter(|_, _, _, mut stack| {
                    stack.push_back(Value::Nil);
                });
                Ok(SequenceReturn::Return)
            } else if index >= 1 && index <= length {
                seq.try_enter(|ctx, locals, mut exec, mut stack| {
                    let table = locals.fetch(&table);
                    // Get the value of the element to remove; we'll keep it on the stack.
                    stack.push_back(table.get_raw(index.into()));

                    // Could make this more efficient by inlining the stack manipulation;
                    // only pushing the table once.
                    for i in index..length {
                        let value = table.get_raw(i.into());
                        table.set_raw(&ctx, (i + 1).into(), value)?;
                        exec.fuel().consume(FUEL_PER_SHIFTED_ITEM);
                    }
                    table.set_raw(&ctx, length.into(), Value::Nil)?;
                    Ok(())
                })?;

                // The last value is still on the stack
                Ok(SequenceReturn::Return)
            } else {
                seq.try_enter(|ctx, _, _, _| {
                    Err("Invalid index passed to table.remove"
                        .into_value(ctx)
                        .into())
                })
            }
        }
    });
    Ok(CallbackReturn::Sequence(s))
}

/// A Lua 5.1-compatible version of `table.insert`.
#[allow(clippy::too_many_lines)]
fn table_insert_impl<'gc>(
    ctx: Context<'gc>,
    mut exec: Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
    let table: Table<'gc>;
    let index: Option<i64>;
    let value: Value<'gc>;
    match stack.len() {
        0..=1 => return Err("Missing arguments to insert".into_value(ctx).into()),
        2 => {
            (table, value) = stack.consume(ctx)?;
            index = None;
        }
        _ => {
            let i: i64;
            // Index must not be nil
            (table, i, value) = stack.consume(ctx)?;
            index = Some(i);
        }
    }
    let length;

    let metatable = table.metatable();
    let use_fallback = metatable.is_some_and(|mt| !mt.get_value(ctx, MetaMethod::Len).is_nil());

    if use_fallback {
        length = None;
    } else {
        // Try the fast path
        match extras::array_insert_shift(
            &mut table.into_inner().borrow_mut(&ctx).raw_table,
            index,
            value,
        ) {
            (extras::RawArrayOpResult::Success(()), len) => {
                // Consume fuel after the operation to avoid computing length twice
                let shifted_items = len.saturating_sub(
                    index
                        .unwrap_or(len.saturating_add(1) as i64)
                        .saturating_sub(1)
                        .try_into()
                        .unwrap_or(0),
                );
                exec.fuel()
                    .consume(count_fuel(FUEL_PER_SHIFTED_ITEM, shifted_items));

                return Ok(CallbackReturn::Return);
            }
            (extras::RawArrayOpResult::Possible, len) => {
                length = Some(len);
            }
            (extras::RawArrayOpResult::Failed, _) => {
                return Err("Invalid index passed to table.insert"
                    .into_value(ctx)
                    .into());
            }
        }
    }

    // Fast path failed, fall back to direct indexing
    let s = async_sequence(&ctx, |locals, mut seq| {
        let table = locals.stash(&ctx, table);
        let value = locals.stash(&ctx, value);
        async move {
            let length = if let Some(len) = length {
                len as i64
            } else {
                let call = seq.try_enter(|ctx, locals, _, stack| {
                    let table = locals.fetch(&table);
                    let call = meta_ops::len(ctx, Value::Table(table))
                        .context("error while calling __len")?;
                    Ok(super::extras::prep_metaop_call(ctx, stack, locals, call))
                })?;
                if let Some(call) = call {
                    seq.call(&call, 0).await?;
                }
                seq.try_enter(|ctx, _, _, mut stack| {
                    Ok(stack
                        .consume::<i64>(ctx)
                        .context("__len returned invalid length")?)
                })?
            };

            let Some(end_index) = length.checked_add(1) else {
                return seq.try_enter(|ctx, _, _, _| {
                    Err("Invalid table length in table.insert"
                        .into_value(ctx)
                        .into())
                });
            };

            let index = index.unwrap_or(end_index);

            if !(1..=end_index).contains(&index) {
                return seq.try_enter(|ctx, _, _, _| {
                    Err("Invalid index passed to table.insert"
                        .into_value(ctx)
                        .into())
                });
            }

            // Avoid evaluating (index + 1), which may overflow, if the
            // index is already at or past the end.
            if index < end_index {
                // Could make this more efficient by inlining the stack manipulation;
                // only pushing the table once.
                seq.try_enter(|ctx, locals, mut exec, _| {
                    let table = locals.fetch(&table);
                    for i in (index + 1..=end_index).rev() {
                        let value = table.get_raw((i - 1).into());
                        table.set_raw(&ctx, i.into(), value)?;
                        exec.fuel().consume(FUEL_PER_SHIFTED_ITEM);
                    }
                    Ok(())
                })?;
            }

            seq.try_enter(|ctx, locals, _, _| {
                let table = locals.fetch(&table);
                let value = locals.fetch(&value);
                table.set_raw(&ctx, index.into(), value)?;
                Ok(())
            })?;

            Ok(SequenceReturn::Return)
        }
    });
    Ok(CallbackReturn::Sequence(s))
}

// TODO: Expose helpers upstream?
mod extras {
    //! Utility functions extracted from piccolo.

    use piccolo::{Value, table::RawTable};

    /// The result of an array operation.
    #[derive(PartialEq, Debug)]
    pub(super) enum RawArrayOpResult<T> {
        /// The operation succeeded; the result value is given.
        Success(T),
        /// The operation is possible, but is deferred.
        Possible,
        /// The operation is impossible.
        Failed,
    }

    /// Try to efficiently remove a key from the array part of the table.
    ///
    /// `key` is one-indexed; if it is `None`, the length of the array is used
    /// instead.
    ///
    /// If successful, returns the removed value; otherwise, indicates whether
    /// the operation is possible to implement with a fallback, or is impossible
    /// due to an out-of-range index.
    ///
    /// Additionally, always returns the computed length of the array from
    /// before the operation.
    pub(super) fn array_remove_shift<'gc>(
        table: &mut RawTable<'gc>,
        key: Option<i64>,
    ) -> (RawArrayOpResult<Value<'gc>>, usize) {
        fn inner<'gc>(
            table: &mut RawTable<'gc>,
            length: usize,
            key: Option<i64>,
        ) -> RawArrayOpResult<Value<'gc>> {
            let index;
            if let Some(k) = key {
                if k == 0 && length == 0 || k == length as i64 + 1 {
                    return RawArrayOpResult::Success(Value::Nil);
                } else if k >= 1 && k <= length as i64 {
                    index = (k - 1) as usize;
                } else {
                    return RawArrayOpResult::Failed;
                }
            } else if length == 0 {
                return RawArrayOpResult::Success(Value::Nil);
            } else {
                index = length - 1;
            }

            let array = table.array_mut();
            if length > array.len() {
                return RawArrayOpResult::Possible;
            }

            let value = core::mem::replace(&mut array[index], Value::Nil);
            if length - index > 1 {
                array[index..length].rotate_left(1);
            }
            RawArrayOpResult::Success(value)
        }

        let length = table.length() as usize;
        (inner(table, length, key), length)
    }

    /// Try to efficiently insert a key and value into the array part of the
    /// table.
    ///
    /// `key` is one-indexed; if it is `None`, the length of the array is used
    /// instead.
    ///
    /// The returned [`RawArrayOpResult`] indicates whether the operation was
    /// successful, or if it failed, whether the operation is possible to
    /// implement with a fallback, or is impossible due to an out-of-range index.
    ///
    /// Additionally, always returns the computed length of the array from
    /// before the operation.
    pub(super) fn array_insert_shift<'gc>(
        table: &mut RawTable<'gc>,
        key: Option<i64>,
        value: Value<'gc>,
    ) -> (RawArrayOpResult<()>, usize) {
        fn inner<'gc>(
            table: &mut RawTable<'gc>,
            length: usize,
            key: Option<i64>,
            value: Value<'gc>,
        ) -> RawArrayOpResult<()> {
            let index;
            if let Some(k) = key {
                if k >= 1 && k <= length as i64 + 1 {
                    index = (k - 1) as usize;
                } else {
                    return RawArrayOpResult::Failed;
                }
            } else {
                index = length;
            }

            let array_len = table.array().len();
            if length > array_len {
                return RawArrayOpResult::Possible;
            }

            assert!(index <= length);

            if length == array_len {
                // If the array is full, grow it.
                table.grow_array(1);
            }

            let array = table.array_mut();
            // We know here that length < array.len(), so we shift each
            // element to the right by one.
            // array[length] == nil, which gets rotated back to array[index];
            // we replace it with the value to insert.
            array[index..=length].rotate_right(1);
            array[index] = value;
            RawArrayOpResult::Success(())
        }

        let length = table.length() as usize;
        (inner(table, length, key, value), length)
    }
}
// SPDX-SnippetEnd
