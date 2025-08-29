//! Helper macros for Scribunto library implementations.

/// Creates an `mw_interface` interface object.
///
/// ```ignore
/// interface! {
///   using Self, ctx;
///
///   luaFnName = rust_fn_name,
///   // ...
/// }
/// ```
macro_rules! interface {
    (@rule $Self:ty, $ctx:ident, $table:ident; $(,)?) => {};

    (@rule $Self:ty, $ctx:ident, $table:ident; ~ $lua_name:ident = $rust_name:ident , $($rest:tt)*) => {
        $crate::lua::lualib::make_raw_interface_fn($table, stringify!($lua_name), $ctx, <$Self>::$rust_name);
        interface!(@rule $Self, $ctx, $table; $($rest)*);
    };

    (@rule $Self:ty, $ctx:ident, $table:ident; $lua_name:ident = $rust_name:ident , $($rest:tt)*) => {
        $crate::lua::lualib::make_interface_fn($table, stringify!($lua_name), $ctx, <$Self>::$rust_name);
        interface!(@rule $Self, $ctx, $table; $($rest)*);
    };

    (using $Self:ty, $ctx:ident; $($rest:tt)*) => {{
        let table = Table::new(&$ctx);
        interface!(@rule $Self, $ctx, table; $($rest)*);
        table
    }}
}
pub(super) use interface;

/// Generates unimplemented stub functions for a Scribunto library which always
/// return the Lua error "{luaFnName} not implemented yet" when invoked.
///
/// ```ignore
/// impl MwLibrary {
///     mw_unimplemented! {
///         luaFnName = rust_fn_name,
///         // ...
///     }
/// }
/// ```
macro_rules! mw_unimplemented {
    ($($camelName:ident = $name:ident),+ $(,)?) => {
        $(
            fn $name<'gc>(&self, ctx: Context<'gc>, (): ()) -> Result<Value<'gc>, VmError<'gc>> {
                Err(concat!(stringify!($camelName), " not implemented yet").into_value(ctx).into())
            }
        )+
    }
}
pub(super) use mw_unimplemented;

/// Shorthand for creating a Lua table with multiple fields.
///
/// ```ignore
/// table! {
///     using ctx;
///
///     key = value,
///     // ...
/// }
/// ```
macro_rules! table {
    (using $ctx:ident; $($key:ident = $value:expr),* $(,)?) => {{
        let table = piccolo::Table::new(&$ctx);
        $(table.set_field($ctx, stringify!($key), $value);)*
        table
    }}
}
pub(super) use table;
