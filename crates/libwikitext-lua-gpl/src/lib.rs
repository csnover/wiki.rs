//! MediaWiki Lua support libraries.

#![expect(
    clippy::unnecessary_wraps,
    clippy::unused_self,
    reason = "implementing an interface invisible to clippy"
)]

mod ext_mw_data;
mod ext_mw_parserfunctions;
mod ext_stubs;
mod macros;
mod mw;
mod mw_hash;
mod mw_html;
mod mw_language;
mod mw_message;
mod mw_site;
mod mw_text;
mod mw_title;
mod mw_uri;
mod mw_ustring;
mod prelude;

use gc_arena::{Collect, Rootable};
use libwikitext_common::db::IDatabase;
pub use mw::{HostFrame, LuaEngine};
pub use mw_language::LanguageLibrary;
pub use mw_message::MessageLibrary;
pub use mw_site::SiteLibrary;
pub use mw_title::TitleLibrary;
pub use mw_uri::UriLibrary;
use piccolo::{Executor, Function, Lua, Stack, StashedString, StashedTable};
use prelude::*;

/// The host interface for MediaWiki Scribunto Lua extensions.
trait MwInterface: Collect + Default + Sized {
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
    fn setup<'gc, Db: IDatabase>(
        &self,
        _: &Db,
        ctx: Context<'gc>,
    ) -> Result<Table<'gc>, RuntimeError>;
}

/// A call from a Lua module back into the renderer.
#[derive(Clone)]
pub enum HostCall {
    /// Call a [parser function](libwikitext_render::parser_fns).
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

/// Initialises all the interfaces required for Wikipedia modules to work.
///
/// # Errors
///
/// * An interface fails to initialise
pub fn init<Db: IDatabase + 'static, Sp: HostFrame + 'static>(
    vm: &mut Lua,
    db: &Db,
) -> Result<(), RuntimeError> {
    init_first(vm)?;

    init_libraries!(
        using vm, db;

        LuaEngine<Db, Sp>,
        mw_site::SiteLibrary<'_>,
        mw_uri::UriLibrary<'_, Db>,
        mw_ustring::UstringLibrary,
        mw_language::LanguageLibrary,
        mw_message::MessageLibrary<'_>,
        mw_title::TitleLibrary<Db>,
        mw_text::TextLibrary,
        mw_html::HtmlLibrary,
        mw_hash::HashLibrary,
        ext_mw_data::JCLuaLibrary,
        ext_mw_parserfunctions::LuaLibrary,
        ext_stubs::WikiRsStubs,
    );

    Ok(())
}

/// Bootstraps the Lua VM with the MediaWiki global.
fn init_first(vm: &mut Lua) -> Result<(), RuntimeError> {
    const MW_INIT: &[u8] = include_bytes!("./modules/mwInit.lua");

    log::debug!("Loading mwInit");

    let executor = vm.try_enter(|ctx| {
        let module = Closure::load(ctx, Some("mwInit"), MW_INIT)?;
        Ok(ctx.stash(Executor::start(ctx, module.into(), ())))
    })?;

    vm.finish(&executor)?;

    Ok(())
}

/// Initialises a single interface.
fn init_interface<T: MwInterface + 'static, Db: IDatabase>(
    vm: &mut Lua,
    db: &Db,
) -> Result<(), RuntimeError> {
    log::debug!("Initialising lua module {}", T::NAME);

    let executor = vm.try_enter(|ctx| {
        let module = Closure::load(ctx, Some(T::NAME), T::CODE)?;
        Ok(ctx.stash(Executor::start(ctx, module.into(), ())))
    })?;

    vm.finish(&executor)?;

    let executor = vm.try_enter(|ctx| {
        let library = ctx.fetch(&executor).take_result::<Table<'_>>(ctx)??;
        let setup = library.get::<_, Function<'_>>(ctx, "setupInterface")?;

        let instance = ctx.singleton::<Rootable![T]>();

        let interface = T::register(ctx);
        ctx.set_global("mw_interface", interface);

        let options = instance.setup(db, ctx)?;
        Ok(ctx.stash(Executor::start(ctx, setup, (options,))))
    })?;

    vm.execute::<()>(&executor).unwrap();

    Ok(())
}

/// Shorthand for running [`init_interface`] on a list of modules.
macro_rules! init_libraries {
    (using $vm:ident, $db:ident; $($ty:ty),* $(,)?) => {
        $(init_interface::<$ty, _>($vm, $db)?;)*
    }
}

use init_libraries;

/// Adds a callback to the given interface table that uses typed parameters and
/// return values.
fn make_interface_fn<'gc, F, A, R, T>(
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
fn make_raw_interface_fn<'gc, F, T>(
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
