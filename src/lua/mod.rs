//! Lua interpreter support.

use crate::{
    db::{Article, Database},
    lru_limiter::ByMemoryUsageCalculator,
    renderer::{Kv, StackFrame, State},
    title::Title,
    wikitext::Parser,
};
use axum::http::Uri;
use core::ops::ControlFlow;
use gc_arena::{Rootable, metrics::Pacing};
use lualib::{LanguageLibrary, LuaEngine, TitleLibrary, UriLibrary};
use piccolo::{
    Executor, ExecutorMode, ExternError, Fuel, Function, Lua, RuntimeError, StashedClosure,
    StashedString, StashedTable, TypeError, thread::BadExecutorMode,
};
use prelude::*;
use std::{
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use time::UtcDateTime;

mod lualib;
mod prelude;
mod stdlib;

/// A cached Lua module.
#[derive(Clone)]
pub struct VmCacheEntry {
    /// The module.
    module: StashedClosure,
    /// The module’s sandbox environment.
    env: StashedTable,
    /// The approximate size of the module + data.
    size: usize,
}

impl ByMemoryUsageCalculator for VmCacheEntry {
    type Target = Self;

    fn size_of(value: &Self::Target) -> usize {
        value.size
    }
}

/// A child frame created by a Lua script.
#[derive(Debug)]
pub struct LuaFrame {
    /// The title for the frame.
    title: Title,
    /// The arguments for the frame.
    arguments: Vec<Kv<'static>>,
}

/// A call from a Lua module back into the renderer.
// Clippy: The fields are self-documenting.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Clone)]
enum HostCall {
    /// A call to a [parser function](crate::renderer::call_parser_fn).
    CallParserFunction {
        frame_id: StashedString,
        name: StashedString,
        args: StashedTable,
    },
    /// A call to [`expand_template`](LuaEngine::expand_template).
    ExpandTemplate {
        frame_id: StashedString,
        title: StashedString,
        args: StashedTable,
    },
    /// A call to [`get_all_expanded_arguments`](LuaEngine::get_all_expanded_arguments).
    GetAllExpandedArguments { frame_id: StashedString },
    /// A call to [`get_expanded_argument`](LuaEngine::get_expanded_argument).
    GetExpandedArgument {
        frame_id: StashedString,
        key: StashedString,
    },
    /// A call to [`preprocess`](LuaEngine::preprocess).
    Preprocess {
        frame_id: StashedString,
        text: StashedString,
    },
    /// A call to [`unstrip`](lualib::mw_text::TextLibrary::unstrip) or
    /// [`unstrip_no_wiki`](lualib::mw_text::TextLibrary::unstrip_no_wiki).
    Unstrip {
        text: StashedString,
        mode: UnstripMode,
    },
}

/// The mode to use when restoring strip markers.
#[derive(Clone, Copy)]
enum UnstripMode {
    /// Restore the original text of `<nowiki>` markers and retain other strip
    /// markers.
    OrigText,
    /// Restore the escaped text of `<nowiki>` markers and retain other strip
    /// markers.
    UnstripNoWiki,
    /// Restore the original text of `<nowiki>` markers and remove all other
    /// strip markers.
    Unstrip,
}

/// Creates a new standalone Lua VM.
pub(super) fn new_vm_core() -> Result<Lua, ExternError> {
    let mut vm = Lua::core();

    vm.try_enter(|ctx| {
        stdlib::load_math(ctx)?;
        stdlib::load_table(ctx)?;
        stdlib::load_string(ctx)?;
        stdlib::load_compat(ctx);
        stdlib::load_os(ctx);
        stdlib::load_debug(ctx);
        Ok(())
    })?;

    lualib::init(&mut vm)?;

    Ok(vm)
}

/// Creates a new Lua VM.
pub(super) fn new_vm(
    base_uri: &Uri,
    db: &Arc<Database<'static>>,
    parser: &Parser<'static>,
) -> Result<Lua, ExternError> {
    let mut vm = new_vm_core()?;

    vm.enter(|ctx| {
        let mw = ctx.singleton::<Rootable![LuaEngine]>();
        mw.set_db(db);

        let mw_title = ctx.singleton::<Rootable![TitleLibrary]>();
        mw_title.set_shared(base_uri, db);

        let mw_uri = ctx.singleton::<Rootable![UriLibrary]>();
        mw_uri.set_parser(parser.clone());
    });

    Ok(vm)
}

/// Resets the Lua VM for the given `article`.
pub(super) fn reset_vm(vm: &mut Lua, title: &Title, date: UtcDateTime) -> Result<(), ExternError> {
    vm.try_enter(|ctx| {
        let mw_title = ctx.singleton::<Rootable![TitleLibrary]>();
        mw_title.set_title(ctx, title)?;

        let mw_language = ctx.singleton::<Rootable![LanguageLibrary]>();
        mw_language.set_date(date);

        Ok(())
    })
}

/// Loads and calls a Scribunto module, returning the result.
#[allow(clippy::too_many_lines)]
pub(super) fn run_vm(
    state: &mut State<'_>,
    sp: Pin<&StackFrame<'_>>,
    code: &Arc<Article>,
    fn_name: &str,
) -> Result<String, ExternError> {
    let VmCacheEntry { module, env, .. } =
        if let Some(cached) = state.statics.vm_cache.get(&code.id) {
            cached.clone()
        } else {
            let ex = state.statics.vm.try_enter(|ctx| {
                let mw = ctx.get_global::<Table<'_>>("mw")?;
                let make_env = mw.get::<_, Function<'_>>(ctx, "makeEnv")?;
                Ok(ctx.stash(Executor::start(ctx, make_env, Value::Nil)))
            })?;

            state.statics.vm.finish(&ex).map_err(RuntimeError::from)?;

            state.statics.vm.gc_metrics().set_pacing(
                Pacing::default()
                    .with_min_sleep(0)
                    .with_pause_factor(0.0)
                    .with_timing_factor(0.0),
            );
            let old_size = {
                let mut last_size = state.statics.vm.total_memory();
                loop {
                    state.statics.vm.gc_collect();
                    let new_size = state.statics.vm.total_memory();
                    if new_size == last_size {
                        break new_size;
                    }
                    last_size = new_size;
                }
            };

            let result = state.statics.vm.try_enter(|ctx| {
                let env = ctx.fetch(&ex).take_result::<Table<'_>>(ctx)??;
                let module =
                    Closure::load_with_env(ctx, Some(sp.name.key()), code.body.as_bytes(), env)?;

                Ok((ctx.stash(module), ctx.stash(env)))
            });

            // TODO: The GC does not seem to be deterministic enough to
            // learn the size of a module by forcing GC, recording size,
            // loading the module, forcing another GC, and recording the
            // delta. When doing this, sometimes the delta ends up being
            // negative, which should be impossible.
            let new_size = {
                let mut last_size = state.statics.vm.total_memory();
                loop {
                    state.statics.vm.gc_collect();
                    let new_size = state.statics.vm.total_memory();
                    if new_size == last_size {
                        break new_size;
                    }
                    last_size = new_size;
                }
            };

            state.statics.vm.gc_metrics().set_pacing(Pacing::default());

            let size = new_size - old_size;
            let (module, env) = result?;
            let entry = VmCacheEntry { module, env, size };
            state.statics.vm_cache.insert(code.id, entry.clone());
            entry
        };

    let (old_sp, ex) = state.statics.vm.try_enter(|ctx| {
        let module = ctx.fetch(&module);
        let env = ctx.fetch(&env);
        let mw = ctx.get_global::<Table<'_>>("mw")?;
        let mw_exec = mw.get::<_, Function<'_>>(ctx, "executeFunction")?;

        let engine = ctx.singleton::<Rootable![lualib::LuaEngine]>();
        // After this point, there can be no early returns
        // TODO: What is this, C? Use a drop guard to avoid accidental
        // nightmares
        let old_sp = engine.set_sp(Some(unsafe {
            core::mem::transmute::<Pin<&StackFrame<'_>>, Pin<&'static StackFrame<'static>>>(sp)
        }));

        Ok((
            old_sp,
            ctx.stash(Executor::start(
                ctx,
                mw_exec,
                (module, ctx.intern(fn_name.trim().as_bytes()), env),
            )),
        ))
    })?;

    // TODO: This time limit should probably exclude time spent loading from the
    // database.
    let start = Instant::now();
    let result = 'outer: loop {
        const FUEL_PER_GC: i32 = 16384;

        loop {
            let mut fuel = Fuel::with(FUEL_PER_GC);
            match state
                .statics
                .vm
                .enter(|ctx| ctx.fetch(&ex).step(ctx, &mut fuel))
            {
                Ok(true) => break,
                Ok(false) => {
                    if state.statics.vm.total_memory() > 128 * 1_048_576 {
                        break 'outer Err(RuntimeError::new(anyhow::anyhow!(
                            "memory limit exceeded"
                        ))
                        .into());
                    }

                    if start.elapsed() > Duration::new(10, 0) {
                        break 'outer Err(RuntimeError::new(anyhow::anyhow!(
                            "time limit exceeded"
                        ))
                        .into());
                    }
                }
                Err(err) => break 'outer Err(RuntimeError::new(err).into()),
            }
        }

        let result = state.statics.vm.try_enter(|ctx| {
            let ex = ctx.fetch(&ex);
            if ex.mode() == ExecutorMode::Result {
                let result = ex.take_result::<Value<'_>>(ctx)??;
                if let Value::String(result) = result {
                    Ok(ControlFlow::Break(result.to_str()?.to_string()))
                } else if let Value::UserData(host_call) = result
                    && let Ok(host_call) = host_call.downcast_static::<HostCall>()
                {
                    Ok(ControlFlow::Continue(host_call.clone()))
                } else {
                    Err(TypeError {
                        expected: "string or host call",
                        found: result.type_name(),
                    }
                    .into())
                }
            } else {
                Err(BadExecutorMode {
                    found: ex.mode(),
                    expected: ExecutorMode::Result,
                }
                .into())
            }
        });

        match result {
            Ok(ControlFlow::Continue(host_call)) => {
                let result = lualib::run_host_call(state, &sp, &host_call)?;
                state.statics.vm.try_enter(|ctx| {
                    let ex = ctx.fetch(&ex);
                    let result = ctx.fetch(&result);
                    ex.resume(ctx, result)?;
                    Ok(())
                })?;
            }
            Ok(ControlFlow::Break(result)) => break Ok(result),
            Err(err) => break Err(err),
        }
    };

    state.statics.vm.enter(|ctx| {
        let engine = ctx.singleton::<Rootable![lualib::LuaEngine]>();
        engine.set_sp(old_sp);
    });

    result
}
