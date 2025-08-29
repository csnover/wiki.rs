//! Dynamic compilation.

// This code was taken from commit 7b5ee021ba83954cc7230d1d9124900182688710
// of https://github.com/kyren/piccolo and modified to not require engine
// changes

use super::extras::count_fuel;
use core::pin::Pin;
use gc_arena::{Collect, Gc};
use piccolo::{
    BoxSequence, Callback, CallbackReturn, Closure, Context, Error, Execution, Function, IntoValue,
    Sequence, SequencePoll, Stack, String, Table, TypeError, Value,
};

/// The load mode.
#[derive(Collect, Copy, Clone)]
#[collect(require_static)]
enum LoadMode {
    /// Text.
    Text,
    /// Binary.
    Binary,
    /// Binary. Or text!
    BinaryOrText,
}

/// Information about the load.
struct LoadInfo<'gc> {
    /// The code chunk.
    chunk: String<'gc>,
    /// The name of the chunk.
    name: Option<String<'gc>>,
    /// The load mode.
    mode: Option<LoadMode>,
    /// The sandbox environment.
    env: Option<Table<'gc>>,
}

/// Number of code bytes to load per fuel.
const LOAD_BYTES_PER_FUEL: i32 = 32;

/// Loads the `load` function.
pub fn load_load_text(ctx: Context<'_>) {
    ctx.set_global(
        "load",
        load_wrapper(ctx, |ctx, info, mut exec| {
            let mode = info.mode.unwrap_or(LoadMode::BinaryOrText);
            let env = info.env.unwrap_or_else(|| ctx.globals());
            let name = match info.name {
                Some(name) => format!("{}", name.display_lossy()),
                None => "=(load)".into(),
            };

            if matches!(mode, LoadMode::Binary) {
                return Err("loading binary chunks is not currently supported"
                    .into_value(ctx)
                    .into());
            }

            let source = info.chunk.as_bytes();
            exec.fuel()
                .consume(count_fuel(LOAD_BYTES_PER_FUEL, source.len()));

            let closure = Closure::load_with_env(ctx, Some(&*name), source, env)?;
            Ok(closure.into())
        }),
    );
}

/// An implementation of the argument handling logic for `load` to simplify
/// custom load variants.
///
/// This implements the argument handling required for a spec-compliant load
/// implementation, and then calls the provided function with the processed
/// arguments (`LoadInfo`).  The callback should return either a `Function` or
/// an error, which this will convert to the format expected by `load`.
fn load_wrapper<'gc, F>(ctx: Context<'gc>, load_callback: F) -> Callback<'gc>
where
    F: Fn(Context<'gc>, LoadInfo<'gc>, Execution<'gc, '_>) -> Result<Function<'gc>, Error<'gc>>
        + 'static,
{
    let load_callback = Gc::new_static(&ctx, load_callback);

    Callback::from_fn_with(&ctx, load_callback, |&load_callback, ctx, _, mut stack| {
        let (chunk, name, mode, env): (
            Value<'_>,
            Option<String<'_>>,
            Option<String<'_>>,
            Option<Table<'_>>,
        ) = stack.consume(ctx)?;

        let mode = match mode.as_deref() {
            Some(b"t") => Some(LoadMode::Text),
            Some(b"b") => Some(LoadMode::Binary),
            Some(b"bt") => Some(LoadMode::BinaryOrText),
            Some(_m) => {
                let error = "invalid load mode";
                stack.replace(ctx, (Value::Nil, error));
                return Ok(CallbackReturn::Return);
            }
            None => None,
        };

        let root = (name, mode, env, load_callback);
        let inner = Callback::from_fn_with(&ctx, root, |&root, ctx, exec, mut stack| {
            let (name, mode, env, load_callback) = root;
            let chunk = stack.consume::<String<'_>>(ctx)?;
            let info = LoadInfo {
                chunk,
                name,
                mode,
                env,
            };
            match load_callback(ctx, info, exec) {
                Ok(func) => stack.push_back(Value::Function(func)),
                Err(e) => stack.replace(ctx, (Value::Nil, e.to_string())),
            }
            Ok(CallbackReturn::Return)
        });
        let inner = inner.into();

        match chunk {
            Value::String(_) => {
                stack.push_back(chunk);
                Ok(CallbackReturn::Call {
                    function: inner,
                    then: None,
                })
            }
            Value::Function(func) => {
                // Should this support metamethod-callable values?
                // PRLua only allows raw functions here.
                Ok(CallbackReturn::Sequence(BoxSequence::new(
                    &ctx,
                    BuildLoadString {
                        step: 0,
                        total_len: 0,
                        func,
                        then: inner,
                    },
                )))
            }
            _ => Err(TypeError {
                expected: "string or function",
                found: chunk.type_name(),
            }
            .into()),
        }
    })
}

/// A sequence for building load strings from an iterator function.
#[derive(Collect)]
#[collect(no_drop)]
struct BuildLoadString<'gc> {
    /// The number of iterations.
    step: usize,
    /// The size of the accumulated code text.
    total_len: usize,
    /// The iterator function.
    func: Function<'gc>,
    /// The callback.
    then: Function<'gc>,
}

impl BuildLoadString<'_> {
    /// Collects the generated code string into a single [`String`].
    fn finalize<'gc>(&self, ctx: Context<'gc>, stack: &mut Stack<'gc, '_>) -> String<'gc> {
        // There's no easy way to construct the string in-place with gc-arena,
        // so we construct the string on the normal heap and copy then it to a
        // new piccolo String allocation.
        let mut bytes = Vec::with_capacity(self.total_len);
        for value in stack.drain(..) {
            let Value::String(s) = value else {
                unreachable!() // guaranteed by the BuildLoadString sequence
            };
            bytes.extend(s.as_bytes());
        }
        // This isn't interned as it will only be used by the parser
        String::from_slice(&ctx, &bytes)
    }
}

impl<'gc> Sequence<'gc> for BuildLoadString<'gc> {
    fn poll(
        mut self: Pin<&mut Self>,
        ctx: Context<'gc>,
        _exec: Execution<'gc, '_>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<SequencePoll<'gc>, Error<'gc>> {
        stack.resize(self.step);

        if self.step != 0 {
            let done = match stack.get(self.step - 1) {
                Value::Nil => true,
                v => {
                    // PRLua implicitly converts integer/number values to strings in load
                    let Some(s) = v.into_string(ctx) else {
                        let error = Error::from(TypeError {
                            expected: "string",
                            found: v.type_name(),
                        });
                        stack.replace(ctx, (Value::Nil, error.to_value(ctx)));
                        return Ok(SequencePoll::Return);
                    };
                    stack[self.step - 1] = Value::String(s);
                    // Clippy: More than 4GiB of code would be problematic, and
                    // string lengths cannot be negative.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        self.total_len += s.len() as usize;
                    }
                    s.is_empty()
                }
            };
            if done {
                // The last arg was nil or an empty string, so the load
                // function is done.
                stack.pop_back();
                let str = self.finalize(ctx, &mut stack);
                stack.push_back(Value::String(str));
                return Ok(SequencePoll::TailCall(self.then));
            }
        }

        let bottom = self.step;
        self.step += 1;
        Ok(SequencePoll::Call {
            function: self.func,
            bottom,
        })
    }

    fn error(
        self: Pin<&mut Self>,
        ctx: Context<'gc>,
        _exec: Execution<'gc, '_>,
        error: Error<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<SequencePoll<'gc>, Error<'gc>> {
        // This catches errors thrown by the inner function;
        // PUC-Rio's tests require it, but it's not documented.
        let error = error.to_value(ctx);
        stack.replace(ctx, (Value::Nil, error));
        Ok(SequencePoll::Return)
    }
}
