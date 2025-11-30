//! The Lua prelude.

pub(crate) use piccolo::{
    Callback, CallbackReturn, Closure, Context, Error as VmError, Execution, IntoValue, Stack,
    String as VmString, Table, Value,
};
