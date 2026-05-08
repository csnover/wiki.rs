//! MediaWiki Scribunto Lua support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::{HostCall, MwInterface, prelude::*};
use core::cell::{Ref, RefCell};
use libwikitext_common::{
    db::IDatabase,
    title::{Namespace, Title},
};
use piccolo::{Stack, UserData};

/// A trait for interacting with the stack frame of the current Lua call in the
/// renderer.
pub trait HostFrame {
    /// Returns true if a child frame with the given `frame_id` exists.
    fn child_frame_exists(&self, frame_id: &str) -> bool;

    /// Returns all cached arguments for the given Lua context from the given
    /// `frame_id`.
    ///
    /// This is a performance optimisation.
    ///
    /// # Errors
    ///
    /// * The given `frame_id` is invalid
    fn expand_all_cached<'gc>(
        &self,
        ctx: Context<'gc>,
        frame_id: &str,
    ) -> Result<Option<Table<'gc>>, VmError<'gc>>;

    /// Returns the cached argument with the given `key` from the given
    /// `frame_id`.
    ///
    /// # Errors
    ///
    /// * The given `frame_id` is invalid
    fn expand_cached<'gc>(
        &self,
        ctx: Context<'gc>,
        frame_id: &str,
        key: &str,
    ) -> Result<Option<VmString<'gc>>, VmError<'gc>>;

    /// Adds a fake child frame to the given `frame_id` with the given `title`
    /// and `arguments`. Returns the ID of the new frame.
    ///
    /// # Errors
    ///
    /// * The given `frame_id` is invalid
    /// * `args` cannot be converted to a renderer argument list
    fn insert<'gc>(
        &self,
        ctx: Context<'gc>,
        frame_id: &str,
        title: Title,
        args: Table<'gc>,
    ) -> Result<VmString<'gc>, VmError<'gc>>;

    /// Returns the title of the frame with the given `frame_id`.
    ///
    /// # Errors
    ///
    /// * The given `frame_id` is invalid
    fn name<'gc>(&self, frame_id: &str) -> Result<Title, VmError<'gc>>;
}

/// The main Lua support library.
#[derive(gc_arena::Collect)]
#[collect(require_static)]
pub struct LuaEngine<Db: IDatabase, Sp: HostFrame> {
    /// The article database.
    pub(crate) db: RefCell<Option<Db>>,
    /// The stack frame of the current call.
    pub(crate) sp: RefCell<Option<Sp>>,
}

impl<Db: IDatabase, Sp: HostFrame> Default for LuaEngine<Db, Sp> {
    fn default() -> Self {
        Self {
            db: <_>::default(),
            sp: <_>::default(),
        }
    }
}

impl<Db: IDatabase + 'static, Sp: HostFrame + 'static> MwInterface for LuaEngine<Db, Sp> {
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

    fn setup<'gc, SetupDb: IDatabase>(
        &self,
        _: &SetupDb,
        ctx: Context<'gc>,
    ) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            allowEnvFuncs = false
        })
    }
}

impl<Db: IDatabase, Sp: HostFrame> LuaEngine<Db, Sp> {
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

        let title_str = title.to_str()?;
        if !Title::is_valid(self.db().config(), title_str) {
            return Err(anyhow::anyhow!(r#"expandTemplate: invalid title "{title_str}""#).into());
        }

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
                self.sp().child_frame_exists(name.to_str()?)
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

        let value = self.sp().expand_all_cached(ctx, frame_id.to_str()?)?;

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

        let value = self
            .sp()
            .expand_cached(ctx, frame_id.to_str()?, key.to_str()?)?;

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
        Ok(ctx.intern(self.sp().name(frame_id.to_str()?)?.key().as_bytes()))
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

        let db = self.db();
        let title = Title::new(
            db.config(),
            name.to_str()?,
            Namespace::find_by_id(db.config(), Namespace::MODULE),
        );

        if let Ok(article) = db.get(&title)
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
        let db = self.db();
        let title = title.to_str()?;
        let title_obj = Title::new(db.config(), title, None);
        let Ok(article) = db.get(&title_obj) else {
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

        let frame_id = frame_id.to_str()?;
        let sp = self.sp();
        let title = if title.to_bool() {
            Title::new(
                self.db().config(),
                title.into_string(ctx).unwrap().to_str()?,
                None,
            )
        } else {
            sp.name(frame_id)?
        };

        sp.insert(ctx, frame_id, title, args)
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
    pub fn set_db(&self, db: Db) {
        *self.db.borrow_mut() = Some(db);
    }

    /// Sets the stack frame for the current VM call.
    pub fn set_sp(&self, sp: Option<Sp>) -> Option<Sp> {
        core::mem::replace(&mut self.sp.borrow_mut(), sp)
    }

    /// Returns a reference to the database.
    ///
    /// # Panics
    ///
    /// * The database is not set
    #[inline]
    fn db(&self) -> Ref<'_, Db> {
        Ref::map(self.db.borrow(), |db| db.as_ref().unwrap())
    }

    /// Returns a reference to the host stack frame.
    ///
    /// # Panics
    ///
    /// * The stack frame is not set
    #[inline]
    fn sp(&self) -> Ref<'_, Sp> {
        Ref::map(self.sp.borrow(), |sp| sp.as_ref().unwrap())
    }
}

/// The list of built-in Lua libraries.
const BUILT_INS: &[(&str, &[u8])] = {
    // Scribunto set up library paths such that luabit was a library path, and
    // then also did path resolution converting '.' to '/'. Since wiki.rs does
    // not do filesystem anything, just set library names that work effectively
    // the same as if they were searching library paths by identifying these
    // modules with both the bare name and dot-separated library name.
    const LUABIT_BIT: &[u8] = include_bytes!("./modules/luabit/bit.lua");
    const LUABIT_HEX: &[u8] = include_bytes!("./modules/luabit/hex.lua");

    &[
        ("bit", LUABIT_BIT),
        ("bit32", include_bytes!("./modules/bit32.lua")),
        ("hex", LUABIT_HEX),
        ("libraryUtil", include_bytes!("./modules/libraryUtil.lua")),
        ("luabit.bit", LUABIT_BIT),
        ("luabit.hex", LUABIT_HEX),
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
    ]
};
