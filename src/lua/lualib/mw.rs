//! MediaWiki Scribunto Lua support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::{MwInterface, prelude::*};
use crate::{
    db::Database,
    lua::{HostCall, LuaFrame},
    renderer::{
        CachedValue, ExpandMode, ExpandTemplates, Kv, StackFrame, State, StripMarker, Surrogate,
        call_parser_fn, call_template,
    },
    title::{Namespace, Title},
    wikitext::FileMap,
};
use arc_cell::OptionalArcCell;
use piccolo::{ExternError, Stack, StashedString, StashedTable, StashedValue, UserData};
use std::{borrow::Cow, cell::RefCell, pin::Pin};

/// The main Lua support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(crate) struct LuaEngine {
    /// The article database.
    pub(crate) db: OptionalArcCell<Database<'static>>,
    /// The stack frame of the current call.
    pub(crate) sp: RefCell<Option<Pin<&'static StackFrame<'static>>>>,
}

impl MwInterface for LuaEngine {
    const NAME: &'static str = "mw";
    const CODE: &'static [u8] = include_bytes!("./modules/mw.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            loadPackage = load_package,
            loadPHPLibrary = load_php_library,
            frameExists = frame_exists,
            newChildFrame = new_child_frame,
            ~ getExpandedArgument = get_expanded_argument,
            ~ getAllExpandedArguments = get_all_expanded_arguments,
            ~ expandTemplate = expand_template,
            ~ callParserFunction = call_parser_function,
            ~ preprocess = preprocess,
            incrementExpensiveFunctionCount = increment_expensive_function_count,
            isSubsting = is_substing,
            getFrameTitle = get_frame_title,
            ~ setTTL = set_ttl,
            addWarning = add_warning,
            loadJsonData = load_json_data,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            allowEnvFuncs = false
        })
    }
}

impl LuaEngine {
    mw_unimplemented! {
        incrementExpensiveFunctionCount = increment_expensive_function_count,
    }

    /// Emits a warning to be displayed to users.
    pub(crate) fn add_warning<'gc>(
        &self,
        _: Context<'gc>,
        warning: VmString<'_>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        log::warn!("stub: mw.addWarning({warning:?})");
        Ok(Value::Nil)
    }

    /// A trampoline for the [`call_parser_function`] host call.
    pub(crate) fn call_parser_function<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let (frame_id, name, args) =
            stack.consume::<(VmString<'_>, VmString<'_>, Table<'_>)>(ctx)?;
        stack.replace(
            ctx,
            UserData::new_static(
                &ctx,
                HostCall::CallParserFunction {
                    frame_id: ctx.stash(frame_id),
                    name: ctx.stash(name),
                    args: ctx.stash(args),
                },
            ),
        );
        Ok(CallbackReturn::Yield {
            to_thread: None,
            then: None,
        })
    }

    /// A trampoline for the [`expand_template`] host call.
    pub(crate) fn expand_template<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let (frame_id, title, args) =
            stack.consume::<(VmString<'_>, VmString<'_>, Table<'_>)>(ctx)?;
        // log::trace!("mw.expandTemplate({frame_id:?}, {title:?}, {args:?})");
        stack.replace(
            ctx,
            UserData::new_static(
                &ctx,
                HostCall::ExpandTemplate {
                    frame_id: ctx.stash(frame_id),
                    title: ctx.stash(title),
                    args: ctx.stash(args),
                },
            ),
        );
        Ok(CallbackReturn::Yield {
            to_thread: None,
            then: None,
        })
    }

    /// Returns whether a Lua frame with the given name exists.
    pub(crate) fn frame_exists<'gc>(
        &self,
        _: Context<'gc>,
        name: Value<'gc>,
    ) -> Result<bool, VmError<'gc>> {
        // log::trace!("stub: mw.frameExists({name:?})");
        Ok(if let Value::String(name) = name {
            if name == "empty" || name == "current" || name == "parent" {
                true
            } else {
                self.sp
                    .borrow()
                    .unwrap()
                    .children
                    .borrow()
                    .contains_key(name.to_str()?)
            }
        } else {
            false
        })
    }

    /// A trampoline for the [`get_all_expanded_arguments`] host call.
    pub(crate) fn get_all_expanded_arguments<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let frame_id = stack.consume::<VmString<'_>>(ctx)?;
        // log::trace!("stub: mw.getAllExpandedArguments({frame_id:?})");

        let value = with_sp(frame_id.to_str()?, self.sp.borrow().as_deref(), |sp| {
            Ok(sp.expand_all_cached(ctx))
        })?;

        Ok(if let Some(value) = value {
            stack.replace(ctx, value);
            CallbackReturn::Return
        } else {
            stack.replace(
                ctx,
                UserData::new_static(
                    &ctx,
                    HostCall::GetAllExpandedArguments {
                        frame_id: ctx.stash(frame_id),
                    },
                ),
            );
            CallbackReturn::Yield {
                to_thread: None,
                then: None,
            }
        })
    }

    /// A trampoline for the [`get_expanded_argument`] host call.
    pub(crate) fn get_expanded_argument<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let (frame_id, key) = stack.consume::<(VmString<'_>, VmString<'gc>)>(ctx)?;
        // log::trace!("mw.getExpandedArgument({frame_id:?}, {key:?})");

        let value = with_sp(frame_id.to_str()?, self.sp.borrow().as_deref(), |sp| {
            Ok(match sp.expand_cached(key.to_str()?) {
                CachedValue::Nil | CachedValue::Unknown => None,
                CachedValue::Cached(value) => Some(ctx.intern(value.as_bytes())),
            })
        })?;

        Ok(if let Some(value) = value {
            stack.replace(ctx, value);
            CallbackReturn::Return
        } else {
            stack.replace(
                ctx,
                UserData::new_static(
                    &ctx,
                    HostCall::GetExpandedArgument {
                        frame_id: ctx.stash(frame_id),
                        key: ctx.stash(key),
                    },
                ),
            );
            CallbackReturn::Yield {
                to_thread: None,
                then: None,
            }
        })
    }

    /// Returns the article title corresponding to the given frame.
    pub(crate) fn get_frame_title<'gc>(
        &self,
        ctx: Context<'gc>,
        frame_id: VmString<'gc>,
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        // log::trace!("mw.getFrameTitle({frame_id:?})");
        with_sp(frame_id.to_str()?, self.sp.borrow().as_deref(), |sp| {
            Ok(ctx.intern(sp.name.key().as_bytes()))
        })
    }

    /// Returns whether or not the engine is currently in save mode.
    ///
    /// wiki.rs is never in page save mode.
    pub(crate) fn is_substing<'gc>(&self, _: Context<'gc>, (): ()) -> Result<bool, VmError<'gc>> {
        Ok(false)
    }

    /// Loads a possibly built-in package, sandboxing it into the given
    /// environment, if provided.
    pub(crate) fn load_package<'gc>(
        &self,
        ctx: Context<'gc>,
        (name, env): (VmString<'_>, Option<Table<'gc>>),
    ) -> Result<Closure<'gc>, VmError<'gc>> {
        // log::trace!(
        //     "mw.loadPackage({name:?}, {})",
        //     if env.is_some() { "Some" } else { "None" }
        // );
        if let Some((name, source)) = BUILT_INS.iter().find(|(k, _)| name == k) {
            return Closure::load_with_env(ctx, Some(name), source, env.unwrap_or(ctx.globals()))
                .map_err(Into::into);
        }

        let title = Title::new(name.to_str()?, Namespace::find_by_id(Namespace::MODULE));

        if let Ok(article) = self.db.get().unwrap().get(title.key())
            && article.model == "Scribunto"
        {
            Closure::load_with_env(
                ctx,
                Some(&article.title),
                article.body.as_bytes(),
                env.unwrap_or(ctx.globals()),
            )
            .map_err(Into::into)
        } else {
            Err(format!("package '{}' not found", name.display_lossy())
                .into_value(ctx)
                .into())
        }
    }

    /// This alternative method for loading a package is a no-op in wiki.rs.
    pub(crate) fn load_php_library<'gc>(
        &self,
        _: Context<'gc>,
        _name: VmString<'_>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        Ok(Value::Nil)
    }

    /// A trampoline for the [`preprocess`] host call.
    pub(crate) fn preprocess<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let (frame_id, text) = stack.consume::<(VmString<'_>, VmString<'_>)>(ctx)?;
        // log::trace!("mw.preprocess({frame_id:?}, {text:?})");
        stack.replace(
            ctx,
            UserData::new_static(
                &ctx,
                HostCall::Preprocess {
                    frame_id: ctx.stash(frame_id),
                    text: ctx.stash(text),
                },
            ),
        );
        Ok(CallbackReturn::Yield {
            to_thread: None,
            then: None,
        })
    }

    /// Loads JSON data from the given article.
    fn load_json_data<'gc>(
        &self,
        ctx: Context<'gc>,
        title: VmString<'gc>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        let title = title.to_str()?;
        let title_obj = Title::new(title, None);
        let Ok(article) = self.db.get().unwrap().get(title_obj.key()) else {
            return Err(anyhow::anyhow!(
                "bad argument #1 to 'mw.loadJsonData' ('{title}' is not a valid JSON page)"
            ))?;
        };

        if article.model != "json" {
            return Err(anyhow::anyhow!(
                "bad argument #1 to 'mw.loadJsonData' ('{title}' is not a valid JSON page)"
            ))?;
        }

        let ser = piccolo_util::serde::ser::Serializer::new(ctx, <_>::default());
        let mut deser = serde_json::Deserializer::from_slice(article.body.as_bytes());
        Ok(serde_transcode::transcode(&mut deser, ser)?)
    }

    /// Creates a fake “child” frame with the given fake `title` and fake
    /// `args`.
    ///
    /// This function is, at least, used to perform inter-module calls to
    /// module functions which expect to receive a frame object. For an example,
    /// see 'Module:Hatnote inline'.
    fn new_child_frame<'gc>(
        &self,
        ctx: Context<'gc>,
        (frame_id, title, args): (VmString<'gc>, Value<'gc>, Table<'gc>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        // log::trace!("mw.newChildFrame({frame_id:?}, {title:?}, {args:?})");

        let frame = with_sp(frame_id.to_str()?, self.sp.borrow().as_deref(), |sp| {
            let title = if title.to_bool() {
                Title::new(title.into_string(ctx).unwrap().to_str()?, None)
            } else {
                sp.name.clone()
            };

            Ok(LuaFrame {
                title,
                arguments: args_from_table(ctx, args)?,
            })
        })?;

        let self_sp = self.sp.borrow().unwrap();
        let mut children = self_sp.children.borrow_mut();

        // In MW, the frame ID starts at 2 because 'current' and 'parent' always
        // exist. This value seems to be hidden to other Lua modules, but there
        // is no reason to not just follow the same convention since a string
        // must be made.
        let new_frame_id = format!("frame{}", children.len() + 2);
        let interned = ctx.intern(new_frame_id.as_bytes());
        children.insert(new_frame_id, frame);
        Ok(interned)
    }

    /// In MW, this would set the cache expiry for the value returned by the
    /// current VM call. In wiki.rs, this is deliberately a no-op.
    pub(crate) fn set_ttl<'gc>(
        &self,
        _: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        stack.clear();
        Ok(CallbackReturn::Return)
    }

    /// Sets the article database.
    pub(crate) fn set_db(&self, db: &Arc<Database<'static>>) {
        self.db.set(Some(Arc::clone(db)));
    }

    /// Sets the stack frame for the current VM call.
    pub(crate) fn set_sp(
        &self,
        sp: Option<Pin<&'static StackFrame<'static>>>,
    ) -> Option<Pin<&'static StackFrame<'static>>> {
        core::mem::replace(&mut self.sp.borrow_mut(), sp)
    }
}

/// Calls the function `f` with the stack frame associated with the `frame_id`
/// relative to the given frame `sp`.
///
/// This indirect call is necessary because there is no known sound way to
/// map a [`core::cell::Ref`] from another [`core::cell::Ref`]. See
/// rust-lang/rust#54776.
fn with_sp<'gc, R, F>(frame_id: &str, sp: Option<&StackFrame<'_>>, f: F) -> Result<R, VmError<'gc>>
where
    F: FnOnce(&StackFrame<'_>) -> Result<R, VmError<'gc>>,
{
    if let Some(sp) = sp {
        if frame_id == "current" {
            return f(sp);
        } else if frame_id == "parent"
            && let Some(parent) = sp.parent
        {
            return f(&parent);
        } else if let Some(child) = sp.children.borrow().get(frame_id) {
            return f(&sp.chain(child.title.clone(), FileMap::new(""), &child.arguments)?);
        }
    }

    Err(RuntimeError::new(anyhow::anyhow!("missing sp")))?
}

/// Converts a Lua table into a list of k-v pairs suitable for template and
/// parser function calls.
fn args_from_table<'a, 'gc>(
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

/// Runs a VM host call.
pub(crate) fn run_host_call(
    state: &mut State<'_>,
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
        HostCall::Unstrip { text, mode } => {
            super::mw_text::unstrip(state, text, *mode).map(Into::into)
        }
    }
}

/// Calls the parser function given by `name`, using the given `args`, in the
/// context given by `frame_id`, and returns the expanded Wikitext.
fn call_parser_function(
    state: &mut State<'_>,
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

        Ok((frame_id.to_string(), callee, args))
    })?;

    with_sp(&frame_id, Some(sp), |sp| {
        let mut result = String::new();
        call_parser_fn(&mut result, state, sp, None, &callee, &args)?;
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
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
    title: &StashedString,
    args: &StashedTable,
) -> Result<StashedString, ExternError> {
    let (frame_id, title, arguments) = state.statics.vm.try_enter(|ctx| {
        let frame_id = ctx.fetch(frame_id).to_str()?.to_string();
        let title = ctx.fetch(title).to_str()?.to_string();
        let arguments = args_from_table(ctx, ctx.fetch(args))?;
        Ok((frame_id, title, arguments))
    })?;

    with_sp(&frame_id, Some(sp), |sp| {
        let mut result = String::new();
        call_template(
            &mut result,
            state,
            sp,
            &Kv::Borrowed(&title),
            Title::new(&title, Namespace::find_by_id(Namespace::TEMPLATE)),
            &arguments,
        )?;
        Ok(state
            .statics
            .vm
            .enter(|ctx| ctx.stash(ctx.intern(result.as_bytes()))))
    })
    .map_err(Into::into)
}

/// Expands an argument passed to the given `frame_id` and returns the resulting
/// Wikitexts as a table of k-vs.
fn get_all_expanded_arguments(
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
) -> Result<StashedTable, ExternError> {
    let frame_id = state
        .statics
        .vm
        .try_enter(|ctx| Ok(ctx.fetch(frame_id).to_str()?.to_string()))?;

    with_sp(&frame_id, Some(sp), |sp| {
        let table = state.statics.vm.enter(|ctx| ctx.stash(Table::new(&ctx)));
        let mut keys = sp.keys();
        while let Some(key) = keys.next(state)? {
            let value = sp.expand(state, &key)?;
            let value = value
                .as_ref()
                .map_or(Cow::Borrowed(""), |(value, is_named)| {
                    strip_wiki_rs_markers(state, *is_named, value)
                });
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
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
    key: &StashedString,
) -> Result<StashedValue, ExternError> {
    let (frame_id, key) = state.statics.vm.try_enter(|ctx| {
        Ok((
            ctx.fetch(frame_id).to_str()?.to_string(),
            ctx.fetch(key).to_str()?.to_string(),
        ))
    })?;

    with_sp(&frame_id, Some(sp), |sp| {
        Ok(if let Some((value, is_named)) = sp.expand(state, &key)? {
            // eprintln!("renderparam2: {key} = {value}");
            let value = strip_wiki_rs_markers(state, is_named, &value);
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

/// Removes wiki.rs source markers from the given input.
///
/// This is necessary because 'Module:Citation/CS1' is unbearable and complains
/// visibly if a strip marker exists inside of any of its parameters instead of
/// just ignoring or stripping them itself (is it any wonder it runs so
/// slowly?).
///
/// More importantly, 'Module:Infobox' does that thing that PHP-adjacent
/// programmers love to do and runs uses pattern matching on raw Wikitext
/// inputs and expects that those inputs will be in a very specific format that
/// does not include any extra strip markers or whitespace or *anything*,
/// because that causes it to fail to match, and then it fucks the entire
/// markup.
///
/// Eventually it will turn out that source tracking has to occur totally out of
/// band using a code map, and then the problem will be solved forever, but why
/// do work when you can pretend like it is unnecessary, lol.
fn strip_wiki_rs_markers<'a>(state: &State<'_>, trim: bool, input: &'a str) -> Cow<'a, str> {
    match state
        .strip_markers
        .for_each_marker(input, |marker| match marker {
            StripMarker::WikiRsSourceStart(_) | StripMarker::WikiRsSourceEnd(_) => {
                Some(Cow::Borrowed(""))
            }
            _ => None,
        }) {
        s @ Cow::Borrowed(_) => s,
        Cow::Owned(s) => {
            if trim {
                Cow::Owned(s.trim_ascii().to_string())
            } else {
                Cow::Owned(s)
            }
        }
    }
}

/// Expands templates in the given `text` in the context of the given
/// `frame_id` and returns the resulting Wikitext.
fn preprocess(
    state: &mut State<'_>,
    sp: &StackFrame<'_>,
    frame_id: &StashedString,
    text: &StashedString,
) -> Result<StashedString, ExternError> {
    let (frame_id, text) = state.statics.vm.try_enter(|ctx| {
        Ok((
            ctx.fetch(frame_id).to_str()?.to_string(),
            ctx.fetch(text).to_str()?.to_string(),
        ))
    })?;

    with_sp(&frame_id, Some(sp), |sp| {
        let source = FileMap::new(&text);
        let root = state.statics.parser.parse(&source, true)?;
        let sp = sp.clone_with_source(source);
        let mut expand = ExpandTemplates::new(ExpandMode::Include);
        expand.adopt_output(state, &sp, &root)?;
        let result = expand.finish();
        Ok(state
            .statics
            .vm
            .enter(|ctx| ctx.stash(ctx.intern(result.as_bytes()))))
    })
    .map_err(Into::into)
}

/// The list of built-in Lua libraries.
const BUILT_INS: &[(&str, &[u8])] = &[
    ("bit32", include_bytes!("./modules/bit32.lua")),
    ("libraryUtil", include_bytes!("./modules/libraryUtil.lua")),
    ("package", include_bytes!("./modules/package.lua")),
    ("strict", include_bytes!("./modules/strict.lua")),
    ("ustring", include_bytes!("./modules/ustring/ustring.lua")),
    (
        "ustring/charsets",
        include_bytes!("./modules/ustring/charsets.lua"),
    ),
    (
        "ustring/lower",
        include_bytes!("./modules/ustring/lower.lua"),
    ),
    (
        "ustring/normalization-data",
        include_bytes!("./modules/ustring/normalization-data.lua"),
    ),
    (
        "ustring/string",
        include_bytes!("./modules/ustring/string.lua"),
    ),
    (
        "ustring/upper",
        include_bytes!("./modules/ustring/upper.lua"),
    ),
];
