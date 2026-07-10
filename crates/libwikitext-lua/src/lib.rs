//! Lua interpreter support.

pub mod ext_stubs;
pub mod macros;
pub mod prelude;
pub mod stdlib;

use core::cell::Cell;
use gc_arena::{Collect, Rootable};
use libphp_rs::DateTime;
use libwikitext_common::{Messages, db::DatabaseProvider, title::Title};
use piccolo::{ExternError, Lua, StashedString, StashedTable};
use prelude::*;

/// A trait for interacting with the stack frame of the current Lua call in the
/// renderer.
pub trait HostFrame {
    /// Returns true if a child frame with the given `frame_id` exists. The
    /// `"empty"`, `"current"`, and `"parent"` frames do *not* need to be
    /// handled by this function.
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

/// The host interface for MediaWiki Scribunto Lua extensions.
pub trait MwInterface: Collect + Default + Sized {
    /// The Lua code for the module.
    const CODE: &[u8];
    /// The name of the module. This will be the name seen in Lua tracebacks.
    const NAME: &str;

    /// Returns the function table for the Lua side of the interface.
    ///
    /// The return value is a Lua table where each key is the name of the
    /// function on the Lua side and the value is a [`piccolo::Function`]. This
    /// value will be used assigned to the `mw_interface` global to be consumed
    /// later when `setup` is called.
    fn register(ctx: Context<'_>) -> Table<'_>;

    /// Returns the options for the corresponding Lua `setupInterface` function.
    ///
    /// # Errors
    ///
    /// * The Lua setup function raised an error
    fn setup<'gc, Db>(
        &self,
        _: &Messages<'_, Db>,
        ctx: Context<'gc>,
    ) -> Result<Table<'gc>, RuntimeError>
    where
        Db: DatabaseProvider,
        RuntimeError: From<Db::Error>;
}

/// A call from a Lua module back into the renderer.
#[derive(Clone)]
pub enum HostCall {
    /// Call a parser function.
    CallParserFunction {
        /// The ID of the context frame to be used for the call.
        frame_id: StashedString,
        /// The name of the parser function.
        name: StashedString,
        /// The arguments to the parser function.
        args: StashedTable,
    },
    /// Expand a template from the database.
    ExpandTemplate {
        /// The ID of the context frame to be used for the call.
        frame_id: StashedString,
        /// The title of the template to expand.
        title: StashedString,
        /// The arguments to the template.
        args: StashedTable,
    },
    /// Get all arguments passed to a template as expanded Wikitext.
    GetAllExpandedArguments {
        /// The ID of the frame to be used when getting arguments.
        frame_id: StashedString,
    },
    /// Get one argument passed to a template as expanded Wikitext.
    GetExpandedArgument {
        /// The ID of the frame to be used when getting the argument.
        frame_id: StashedString,
        /// The argument key.
        key: StashedString,
    },
    /// Expand raw Wikitext.
    Preprocess {
        /// The ID of the context frame to be used for the call.
        frame_id: StashedString,
        /// The Wikitext to parse.
        text: StashedString,
    },
    /// Restore content from `<nowiki>` strip markers and optionally remove
    /// other strip markers.
    Unstrip {
        /// Text which may contain strip markers.
        text: StashedString,
        /// The mode to use when restoring the content.
        mode: UnstripMode,
    },
}

/// A mode for restoring content from strip markers.
#[derive(Clone, Copy)]
pub enum UnstripMode {
    /// Replace `<nowiki>` markers with their inner content and retain other
    /// strip markers.
    OrigText,
    /// Replace `<nowiki>` markers with their inner content and remove all
    /// other strip markers.
    Unstrip,
    /// Replace `<nowiki>` markers with escaped Wikitext of their inner content
    /// and retain other strip markers.
    UnstripNoWiki,
}

/// A singleton object for setting the VM’s wall clock time.
#[derive(Collect)]
#[collect(require_static)]
pub struct WallTime(Cell<DateTime>);

impl core::ops::Deref for WallTime {
    type Target = Cell<DateTime>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for WallTime {
    fn default() -> Self {
        Self(DateTime::now().unwrap_or(DateTime::UNIX_EPOCH).into())
    }
}

/// Adds a callback to the given interface table that uses typed parameters and
/// return values.
pub fn make_interface_fn<'gc, F, A, R, T>(
    table: Table<'gc>,
    name: &'static str,
    ctx: Context<'gc>,
    method: F,
) where
    F: Fn(&T, Context<'gc>, A) -> Result<R, VmError<'gc>> + 'static,
    A: piccolo::FromMultiValue<'gc>,
    R: piccolo::IntoMultiValue<'gc>,
    T: MwInterface + 'static,
{
    make_raw_interface_fn(table, name, ctx, move |this, ctx, mut stack| {
        let args = stack.consume::<A>(ctx)?;
        let ret = method(this, ctx, args)?;
        stack.replace(ctx, ret);
        Ok(CallbackReturn::Return)
    });
}

/// Adds a callback to the given interface table that operates directly on the
/// stack.
pub fn make_raw_interface_fn<'gc, F, T>(
    table: Table<'gc>,
    name: &'static str,
    ctx: Context<'gc>,
    method: F,
) where
    F: Fn(&T, Context<'gc>, Stack<'gc, '_>) -> Result<CallbackReturn<'gc>, VmError<'gc>> + 'static,
    T: MwInterface + 'static,
{
    let callback = Callback::from_fn(&ctx, move |ctx, _, stack| {
        let this = ctx.singleton::<Rootable![T]>();
        method(this, ctx, stack)
    });

    table.set_field(ctx, name, callback);
}

/// Creates a new standalone Lua VM.
///
/// # Errors
///
/// * Loading the standard library fails
pub fn new_vm_core() -> Result<Lua, ExternError> {
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

    Ok(vm)
}
