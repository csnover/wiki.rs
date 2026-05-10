//! Lua interpreter support.

pub mod prelude;
pub mod stdlib;

use core::cell::Cell;
use libphp_rs::DateTime;
use piccolo::{ExternError, Lua};

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

/// A singleton object for setting the VM’s wall clock time.
#[derive(gc_arena::Collect)]
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
