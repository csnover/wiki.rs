//! MediaWiki Lua support libraries.

#![expect(
    clippy::unnecessary_wraps,
    clippy::unused_self,
    reason = "implementing an interface invisible to clippy"
)]

mod ext_mw_data;
mod ext_mw_parserfunctions;
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

use gc_arena::Rootable;
use libwikitext_common::db::DatabaseProvider;
use libwikitext_lua::HostFrame;
pub use mw::LuaEngine;
pub use mw_language::LanguageLibrary;
pub use mw_message::MessageLibrary;
pub use mw_site::SiteLibrary;
pub use mw_title::TitleLibrary;
pub use mw_uri::UriLibrary;
use piccolo::{Executor, Function, Lua};
use prelude::*;

/// Initialises all the interfaces required for MediaWiki Lua modules to work.
///
/// # Errors
///
/// * An interface fails to initialise
pub fn init<Db: DatabaseProvider + 'static, Sp: HostFrame + 'static>(
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
        libwikitext_lua::ext_stubs::WikiRsStubs,
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
fn init_interface<T: MwInterface + 'static, Db: DatabaseProvider>(
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
