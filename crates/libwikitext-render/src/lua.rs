//! Renderer types and functions for calling the Lua interpreter.

use super::{
    ExpandMode, Kv, StackFrame, State, StripMarker,
    parser_fns::call_parser_fn,
    preprocess_frame,
    template::{call_template, resolve_callee},
};
use core::{ops::ControlFlow, pin::pin};
use gc_arena::Rootable;
use libmisc::CowExt as _;
use libphp_rs::DateTime;
use libwikitext_common::{
    Messages,
    db::{Article, DynDatabaseProvider},
    escape_no_wiki,
    title::{Namespace, Title},
    url::Url,
};
use libwikitext_lua::{HostCall, UnstripMode, WallTime, prelude::*};
use libwikitext_parse::{FileMap, Parser};
use piccolo::{
    Executor, ExecutorMode, ExternError, Fuel, Function, Lua, StashedClosure, StashedString,
    StashedTable, StashedValue, TypeError, thread::BadExecutorMode,
};
use std::{borrow::Cow, sync::Arc, time::Instant};

/// The concrete type used by the renderer for [`LanguageLibrary`](libwikitext_lua_gpl::LanguageLibrary).
pub type LanguageLibrary =
    libwikitext_lua_gpl::LanguageLibrary<'static, Arc<dyn DynDatabaseProvider>>;
/// The concrete type used by the renderer for [`LuaEngine`](libwikitext_lua_gpl::LuaEngine).
pub type LuaEngine =
    libwikitext_lua_gpl::LuaEngine<Arc<dyn DynDatabaseProvider>, &'static StackFrame<'static>>;
/// The concrete type used by the renderer for [`MessageLibrary`](libwikitext_lua_gpl::MessageLibrary).
pub type MessageLibrary =
    libwikitext_lua_gpl::MessageLibrary<'static, Arc<dyn DynDatabaseProvider>>;
/// The concrete type used by the renderer for [`SiteLibrary`](libwikitext_lua_gpl::SiteLibrary).
pub type SiteLibrary = libwikitext_lua_gpl::SiteLibrary<'static>;
/// The concrete type used by the renderer for [`TitleLibrary`](libwikitext_lua_gpl::TitleLibrary).
pub type TitleLibrary = libwikitext_lua_gpl::TitleLibrary<Arc<dyn DynDatabaseProvider>>;
/// The concrete type used by the renderer for [`UriLibrary`](libwikitext_lua_gpl::UriLibrary).
pub type UriLibrary = libwikitext_lua_gpl::UriLibrary<'static, Arc<dyn DynDatabaseProvider>>;

/// A cached Lua module.
#[derive(Clone)]
pub struct VmCacheEntry {
    /// The module’s sandbox environment.
    env: StashedTable,
    /// The module.
    module: StashedClosure,
}

/// Converts a Lua table into a list of k-v pairs suitable for template and
/// parser function calls.
pub(super) fn args_from_table<'a, 'gc>(
    ctx: Context<'gc>,
    args: Table<'gc>,
) -> Result<Vec<Kv<'a>>, VmError<'gc>> {
    args.into_iter()
        .filter_map(|(k, v)| {
            k.into_string(ctx)
                .zip(v.into_string(ctx))
                .map(|(k, v)| Ok(Kv::String(ctx.stash(k), ctx.stash(v))))
        })
        .collect()
}

/// Calls the parser function given by `name`, using the given `args`, in the
/// context given by `frame_id`, and returns the expanded Wikitext.
fn call_parser_function(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
    name: &StashedString,
    args: &StashedTable,
) -> Result<StashedString, ExternError> {
    let (frame_id, callee, args) = state.statics.vm.try_enter(|ctx| {
        let frame_id = ctx.fetch(frame_id).to_str()?;
        let name = ctx.fetch(name).to_str()?;
        let mut args = args_from_table(ctx, ctx.fetch(args))?;

        let (callee, first) = name
            .split_once(':')
            .map_or((name.to_lowercase(), None), |(callee, first)| {
                (callee.to_lowercase(), Some(first))
            });
        if let Some(first) = first {
            args.insert(
                0,
                Kv::String(
                    ctx.stash(ctx.intern_static(b"")),
                    ctx.stash(ctx.intern(first.as_bytes())),
                ),
            );
        }

        Ok((frame_id.to_owned(), callee, args))
    })?;

    with_sp(&frame_id, sp, |sp| {
        let mut result = String::new();
        let callee = resolve_callee(state.statics.db.config(), args.is_empty(), true, &callee)
            .ok_or_else(|| {
                anyhow::anyhow!("callParserFunction: function \"{callee}\" was not found")
            })?;
        call_parser_fn(&mut result, state, sp, None, callee, &args)?;
        Ok(state
            .statics
            .vm
            .enter(|ctx| ctx.stash(ctx.intern(result.as_bytes()))))
    })
    .map_err(Into::into)
}

/// Expands the template with the given `title`, using the given `args`, in the
/// context given by `frame_id`, and returns the expanded Wikitext.
fn expand_template(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
    title: &StashedString,
    args: &StashedTable,
) -> Result<StashedString, ExternError> {
    let (frame_id, title, arguments) = state.statics.vm.try_enter(|ctx| {
        let frame_id = ctx.fetch(frame_id).to_str()?.to_owned();
        let title = ctx.fetch(title).to_str()?.to_owned();
        let arguments = args_from_table(ctx, ctx.fetch(args))?;
        Ok((frame_id, title, arguments))
    })?;

    with_sp(&frame_id, sp, |sp| {
        let config = state.statics.db.config();
        let Ok(title) = Title::new(config, &title, Some(Namespace::TEMPLATE)) else {
            return Err(anyhow::anyhow!(
                r#"expandTemplate: invalid title "{title}""#
            ))?;
        };
        let mut result = String::new();
        call_template(&mut result, state, sp, &title, &arguments, false)?;
        Ok(state
            .statics
            .vm
            .enter(|ctx| ctx.stash(ctx.intern(result.as_bytes()))))
    })
    .map_err(Into::into)
}

/// Fetches a possibly cached Lua module for execution.
fn fetch_module(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    code: &Arc<Article>,
) -> Result<(StashedClosure, StashedTable), ExternError> {
    let VmCacheEntry { module, env } = if let Some(cached) = state.statics.vm_cache.get(&code.id())
    {
        cached.clone()
    } else {
        let ex = state.statics.vm.try_enter(|ctx| {
            let mw = ctx.get_global::<Table<'_>>("mw")?;
            let make_env = mw.get::<_, Function<'_>>(ctx, "makeEnv")?;
            Ok(ctx.stash(Executor::start(ctx, make_env, Value::Nil)))
        })?;

        state.statics.vm.finish(&ex).map_err(RuntimeError::from)?;

        // Too many modules rely on their closure being re-executed on every
        // invocation, so that is what `mw.executeFunction` does. Some
        // modules also expect that `packageCache` will be reset, but
        // wiki.rs does *not* do that and instead gives those modules some
        // free therapy in `crate::db::HACKS` until they learn to work well
        // with others
        let (module, env) = state.statics.vm.try_enter(|ctx| {
            let env = ctx.fetch(&ex).take_result::<Table<'_>>(ctx)??;
            let module =
                Closure::load_with_env(ctx, Some(sp.name.key()), code.body().as_bytes(), env)?;

            Ok((ctx.stash(module), ctx.stash(env)))
        })?;

        let entry = VmCacheEntry { env, module };
        state.statics.vm_cache.insert(code.id(), entry.clone());

        if memory_exceeded(state) {
            return Err(RuntimeError::new(anyhow::anyhow!("memory limit exceeded")).into());
        }

        entry
    };
    Ok((module, env))
}

/// Expands all arguments passed to the given `frame_id` and returns the
/// expanded Wikitext as a table of k-vs.
fn get_all_expanded_arguments(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
) -> Result<StashedTable, ExternError> {
    let frame_id = state
        .statics
        .vm
        .try_enter(|ctx| Ok(ctx.fetch(frame_id).to_str()?.to_owned()))?;

    with_sp(&frame_id, sp, |sp| {
        let table = state.statics.vm.enter(|ctx| ctx.stash(Table::new(&ctx)));
        let mut keys = sp.keys();
        while let Some(key) = keys.next(state)? {
            let value = sp
                .expand(state, &key)?
                .expect("only keys with values should exist");
            state.statics.vm.try_enter(|ctx| {
                let key = if let Ok(key) = key.parse::<i64>() {
                    Value::Integer(key)
                } else if let Ok(key) = key.parse::<f64>() {
                    Value::Number(key)
                } else {
                    Value::String(ctx.intern(key.as_bytes()))
                };

                // eprintln!("renderparam: {key:?} = {value:?}");
                ctx.fetch(&table)
                    .set(ctx, key, ctx.intern(value.as_bytes()))?;
                Ok(())
            })?;
        }
        Ok(table)
    })
    .map_err(Into::into)
}

/// Expands an argument passed to the given `frame_id` with the given `key` and
/// returns the resulting Wikitext.
fn get_expanded_argument(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
    key: &StashedString,
) -> Result<StashedValue, ExternError> {
    let (frame_id, key) = state.statics.vm.try_enter(|ctx| {
        Ok((
            ctx.fetch(frame_id).to_str()?.to_owned(),
            ctx.fetch(key).to_str()?.to_owned(),
        ))
    })?;

    with_sp(&frame_id, sp, |sp| {
        Ok(if let Some(value) = sp.expand(state, &key)? {
            // eprintln!("renderparam2: {key} = {value}");
            state
                .statics
                .vm
                .enter(|ctx| ctx.stash(Value::String(ctx.intern(value.as_bytes()))))
        } else {
            state.statics.vm.enter(|ctx| ctx.stash(Value::Nil))
        })
    })
    .map_err(Into::into)
}

/// Naïvely reduces memory pressure on the VM if needed by running garbage
/// collection and evicting cached modules. Returns true if the VM’s memory
/// usage still exceeds the limit after doing everything possible to reduce
/// memory.
fn memory_exceeded(state: &mut State<'_, '_, '_>) -> bool {
    let mut old_size = state.statics.vm.total_memory();

    while old_size >= state.statics.limits.vm_total_mem {
        state.statics.vm.gc_collect();
        let new_size = state.statics.vm.total_memory();
        if old_size == new_size {
            break;
        }
        old_size = new_size;
    }

    while state.statics.vm.total_memory() >= state.statics.limits.vm_total_mem
        && !state.statics.vm_cache.is_empty()
    {
        state.statics.vm_cache.pop_oldest();
        state.statics.vm.gc_collect();
    }

    state.statics.vm.total_memory() >= state.statics.limits.vm_total_mem
}

/// Creates a new Lua VM.
///
/// # Errors
///
/// * VM initialisation fails
pub(super) fn new_vm<'config>(
    base_uri: &Url,
    messages: &Messages<'_, Arc<dyn DynDatabaseProvider>>,
    parser: &Parser<'config>,
) -> Result<Lua, ExternError> {
    let mut vm = libwikitext_lua::new_vm_core()?;
    libwikitext_lua_gpl::init::<_, &StackFrame<'_>>(&mut vm, messages)?;

    // TODO: Express this unsafe relationship in a way where it is harder to
    // violate.
    // SAFETY: The lifetime of these references are always at least as long as
    // the lifetime of the VM.
    let (db, parser) = unsafe {
        (
            core::mem::transmute::<&Arc<dyn DynDatabaseProvider>, &Arc<dyn DynDatabaseProvider>>(
                messages.db(),
            ),
            core::mem::transmute::<&Parser<'_>, &Parser<'static>>(parser),
        )
    };

    vm.enter(|ctx| {
        let mw = ctx.singleton::<Rootable![LuaEngine]>();
        mw.set_db(Arc::clone(db));

        let mw_site = ctx.singleton::<Rootable![SiteLibrary]>();
        mw_site.set_config(db.config());

        let mw_title = ctx.singleton::<Rootable![TitleLibrary]>();
        mw_title.set_shared(base_uri.clone(), Arc::clone(db));

        let mw_uri = ctx.singleton::<Rootable![UriLibrary]>();
        mw_uri.set_parser(parser.clone());
    });

    Ok(vm)
}

/// Expands templates in the given `text` in the context of the given
/// `frame_id` and returns the resulting Wikitext.
fn preprocess(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
    text: &StashedString,
) -> Result<StashedString, ExternError> {
    let (frame_id, text) = state.statics.vm.try_enter(|ctx| {
        Ok((
            ctx.fetch(frame_id).to_str()?.to_owned(),
            ctx.fetch(text).to_str()?.to_owned(),
        ))
    })?;

    with_sp(&frame_id, sp, |sp| {
        let result = preprocess_frame(state, sp, &text, ExpandMode::Include)?;
        Ok(state
            .statics
            .vm
            .enter(|ctx| ctx.stash(ctx.intern(result.as_bytes()))))
    })
    .map_err(Into::into)
}

/// Resets the Lua VM for the given `article`.
pub(super) fn reset_vm(
    vm: &mut Lua,
    messages: &Messages<'_, Arc<dyn DynDatabaseProvider>>,
    title: &Title,
    date: DateTime,
) -> Result<(), ExternError> {
    // TODO: Express this unsafe relationship in a way where it is harder to
    // violate.
    // SAFETY: The lifetime of these references are always at least as long as
    // the lifetime of the VM.
    let messages = unsafe {
        core::mem::transmute::<
            &Messages<'_, Arc<dyn DynDatabaseProvider>>,
            &'static Messages<'static, Arc<dyn DynDatabaseProvider>>,
        >(messages)
    };

    vm.try_enter(|ctx| {
        let mw_lang = ctx.singleton::<Rootable![LanguageLibrary]>();
        mw_lang.set_messages(messages);

        let mw_message = ctx.singleton::<Rootable![MessageLibrary]>();
        mw_message.set_messages(messages);

        let mw_title = ctx.singleton::<Rootable![TitleLibrary]>();
        mw_title.set_title(ctx, title);

        ctx.singleton::<Rootable![WallTime]>().set(date);

        Ok(())
    })
}

/// Runs a VM host call.
fn run_host_call(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    host_call: &HostCall,
) -> Result<StashedValue, ExternError> {
    match host_call {
        HostCall::CallParserFunction {
            frame_id,
            name,
            args,
        } => call_parser_function(state, sp, frame_id, name, args).map(Into::into),
        HostCall::ExpandTemplate {
            frame_id,
            title,
            args,
        } => expand_template(state, sp, frame_id, title, args).map(Into::into),
        HostCall::GetAllExpandedArguments { frame_id } => {
            get_all_expanded_arguments(state, sp, frame_id).map(Into::into)
        }
        HostCall::GetExpandedArgument { frame_id, key } => {
            get_expanded_argument(state, sp, frame_id, key)
        }
        HostCall::Preprocess { frame_id, text } => {
            preprocess(state, sp, frame_id, text).map(Into::into)
        }
        HostCall::Unstrip { text, mode } => unstrip(state, text, *mode).map(Into::into),
    }
}

/// Loads and calls a Scribunto module, returning the result.
pub(super) fn run_vm(
    state: &mut State<'_, '_, '_>,
    sp: &StackFrame<'_>,
    code: &Arc<Article>,
    fn_name: &str,
) -> Result<String, ExternError> {
    let sp = pin!(sp);
    let (module, env) = fetch_module(state, &sp, code)?;

    let mut state = {
        let old_sp = state.statics.vm.enter(|ctx| {
            let engine = ctx.singleton::<Rootable![LuaEngine]>();
            // SAFETY: So long as `old_sp` makes it into the scope guard, it
            // will be removed when this call returns.
            engine.set_sp(Some(unsafe {
                core::mem::transmute::<&StackFrame<'_>, &'static StackFrame<'static>>(&sp)
            }))
        });
        scopeguard::guard(state, move |state| {
            state.statics.vm.enter(|ctx| {
                let engine = ctx.singleton::<Rootable![LuaEngine]>();
                engine.set_sp(old_sp);
            });
        })
    };

    let ex = state.statics.vm.try_enter(|ctx| {
        let module = ctx.fetch(&module);
        let env = ctx.fetch(&env);
        let mw = ctx.get_global::<Table<'_>>("mw")?;
        let mw_exec = mw.get::<_, Function<'_>>(ctx, "executeFunction")?;
        Ok(ctx.stash(Executor::start(
            ctx,
            mw_exec,
            (module, ctx.intern(fn_name.trim().as_bytes()), env),
        )))
    })?;

    // TODO: This time limit should probably exclude time spent loading from the
    // database.
    let start = Instant::now();

    loop {
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
                    if memory_exceeded(&mut state) {
                        return Err(
                            RuntimeError::new(anyhow::anyhow!("memory limit exceeded")).into()
                        );
                    }

                    if start.elapsed() > state.statics.limits.vm_time {
                        return Err(
                            RuntimeError::new(anyhow::anyhow!("time limit exceeded")).into()
                        );
                    }
                }
                Err(err) => return Err(RuntimeError::new(err).into()),
            }
        }

        let result = state.statics.vm.try_enter(|ctx| {
            let ex = ctx.fetch(&ex);
            if ex.mode() == ExecutorMode::Result {
                let result = ex.take_result::<Value<'_>>(ctx)??;
                if let Value::String(result) = result {
                    Ok(ControlFlow::Break(result.to_str()?.to_owned()))
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
        })?;

        match result {
            ControlFlow::Continue(host_call) => {
                let result = run_host_call(&mut state, &sp, &host_call)?;
                state.statics.vm.try_enter(|ctx| {
                    let ex = ctx.fetch(&ex);
                    let result = ctx.fetch(&result);
                    ex.resume(ctx, result)?;
                    Ok(())
                })?;
            }
            ControlFlow::Break(result) => break Ok(result),
        }
    }
}

/// Replaces `<nowiki>` markers in the given `text`, optionally in an encoded
/// form, and optionally removing other markers.
///
/// This runs outside of the Lua VM to avoid having to wrap `StripMarkers` in
/// `Rc<RefCell>`.
fn unstrip(
    state: &mut State<'_, '_, '_>,
    text: &StashedString,
    mode: UnstripMode,
) -> Result<StashedString, ExternError> {
    state.statics.vm.try_enter(|ctx| {
        let text = ctx.fetch(text);

        let result = match mode {
            UnstripMode::OrigText => state.strip_markers.unstrip_no_wiki(text.to_str()?),
            UnstripMode::UnstripNoWiki => {
                // TODO: This is also supposed to erase any `</?nowiki[^>]*>`
                // for some reason?
                // Not recursively removing markers is deliberate and matches
                // the MW behaviour
                state
                    .strip_markers
                    .for_each_marker(text.to_str()?, |marker| {
                        if let StripMarker::NoWiki(text) = marker {
                            Some(escape_no_wiki(text))
                        } else {
                            None
                        }
                    })
            }
            UnstripMode::Unstrip => state.strip_markers.unstrip_with(text.to_str()?, |marker| {
                Some(Cow::Borrowed(if let StripMarker::NoWiki(text) = marker {
                    text
                } else {
                    ""
                }))
            }),
        };

        Ok(ctx.stash(result.owned_or_else(text, |text| ctx.intern(text.as_bytes()))))
    })
}

/// Calls the function `f` with the stack frame associated with the `frame_id`
/// relative to the given frame `sp`.
///
/// This indirect call is necessary because there is no known sound way to
/// map a [`core::cell::Ref`] from another [`core::cell::Ref`]. See
/// rust-lang/rust#54776.
pub(super) fn with_sp<'gc, R, F>(
    frame_id: &str,
    sp: &StackFrame<'_>,
    f: F,
) -> Result<R, VmError<'gc>>
where
    F: FnOnce(&StackFrame<'_>) -> Result<R, VmError<'gc>>,
{
    if frame_id == "current" {
        return f(sp);
    } else if frame_id == "parent"
        && let Some(parent) = sp.parent
    {
        return f(parent);
    }

    let mut frame = Some(sp);
    while let Some(sp) = frame {
        if let Some(child) = sp.children.borrow().get(frame_id) {
            return f(&sp.chain(child.title.clone(), FileMap::new(""), &child.arguments)?);
        }
        frame = sp.parent;
    }

    Err(RuntimeError::new(anyhow::anyhow!("missing sp")))?
}
