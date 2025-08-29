//! MediaWiki Lua support libraries.

// Clippy: Methods are implementing an interface which is invisible to clippy.
#![allow(clippy::unnecessary_wraps, clippy::unused_self)]

use gc_arena::{Collect, Rootable};
pub(super) use mw::{LuaEngine, run_host_call};
pub(super) use mw_language::LanguageLibrary;
pub(super) use mw_title::TitleLibrary;
pub(super) use mw_uri::UriLibrary;
use piccolo::{Executor, Function, Lua, Stack};
use prelude::*;

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

/// The host interface for MediaWiki Scribunto Lua extensions.
trait MwInterface: Collect + Default + Sized + 'static {
    /// The name of the module. This will be the name seen in Lua tracebacks.
    const NAME: &str;
    /// The Lua code for the module.
    const CODE: &[u8];

    /// Returns the function table for the Lua side of the interface.
    ///
    /// The return value is a Lua table where each key is the name of the
    /// function on the Lua side and the value is a [`piccolo::Function`]. This
    /// value will be used assigned to the `mw_interface` global to be consumed
    /// later when `setup` is called.
    fn register(ctx: Context<'_>) -> Table<'_>;

    /// Returns the options for the correpsonding Lua `setupInterface` function.
    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError>;
}

/// Initialises a single interface.
fn init_interface<'gc, T: MwInterface>(vm: &'gc mut Lua) -> Result<(), RuntimeError> {
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

        let options = instance.setup(ctx)?;
        Ok(ctx.stash(Executor::start(ctx, setup, (options,))))
    })?;

    vm.execute::<()>(&executor).unwrap();

    Ok(())
}

/// Shorthand for running [`init_interface`] on a list of modules.
macro_rules! init_libraries {
    (using $vm:ident; $($ty:ty),* $(,)?) => {
        $(init_interface::<$ty>($vm)?;)*
    }
}

/// Initialises all the interfaces required for Wikipedia modules to work.
pub(super) fn init(vm: &mut Lua) -> Result<(), RuntimeError> {
    const MW_INIT: &[u8] = include_bytes!("./modules/mwInit.lua");

    log::debug!("Loading mwInit");

    let executor = vm.try_enter(|ctx| {
        let module = Closure::load(ctx, Some("mwInit"), MW_INIT)?;
        Ok(ctx.stash(Executor::start(ctx, module.into(), ())))
    })?;

    vm.finish(&executor)?;

    init_libraries!(
        using vm;

        LuaEngine,
        mw_site::SiteLibrary,
        mw_uri::UriLibrary,
        mw_ustring::UstringLibrary,
        mw_language::LanguageLibrary,
        mw_message::MessageLibrary,
        mw_title::TitleLibrary,
        mw_text::TextLibrary,
        mw_html::HtmlLibrary,
        mw_hash::HashLibrary,
        ext_mw_data::JCLuaLibrary,
        ext_mw_parserfunctions::LuaLibrary,
        ext_stubs::WikiRsStubs,
    );

    Ok(())
}

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
    T: MwInterface,
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
    T: MwInterface,
{
    let callback = Callback::from_fn(&ctx, move |ctx, _, stack| {
        let this = ctx.singleton::<Rootable![T]>();
        method(this, ctx, stack)
    });

    table.set_field(ctx, name, callback);
}
