//! Unit tests for UTF-8 engine.

// This code is adapted from piccolo. The upstream copyright is:
//
// SPDX-License-Identifier: MIT

use crate::lua::new_vm_core;
use piccolo::{Closure, Executor, ExternError, io};
use std::{
    fs::{File, read_dir},
    io::{Read, Write, stdout},
};

const BASE_DIR: &str = "./src/lua/lualib/mw_ustring/tests";

fn run_lua_code(name: &str, code: &[u8]) -> Result<(), ExternError> {
    let mut lua = new_vm_core()?;

    let exec = lua.try_enter(|ctx| {
        piccolo::stdlib::load_io(ctx);
        let mut file =
            io::buffered_read(File::open(format!("{BASE_DIR}/testframework.lua")).unwrap())
                .unwrap();
        let mut code = Vec::new();
        file.read_to_end(&mut code).unwrap();

        let closure = Closure::load(ctx, Some("testframework"), &code)?;
        Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
    })?;

    lua.execute::<()>(&exec)?;

    let exec = lua.try_enter(|ctx| {
        let closure = Closure::load(ctx, Some(name), code)?;
        Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
    })?;

    lua.execute::<()>(&exec)?;

    Ok(())
}

fn run_tests(dir: &str) -> bool {
    let _ = writeln!(stdout(), "running all test scripts in {dir:?}");

    let mut file_failed = false;
    for dir in read_dir(dir).expect("could not list dir contents") {
        let path = dir.expect("could not read dir entry").path();
        if let Some(ext) = path.extension()
            && ext == "lua"
            && let Some(name) = path.file_name()
            && name != "testframework.lua"
        {
            let mut file = io::buffered_read(File::open(&path).unwrap()).unwrap();
            let mut source = Vec::new();
            file.read_to_end(&mut source).unwrap();

            let _ = writeln!(stdout(), "running {path:?}");
            if let Err(err) = run_lua_code(path.to_string_lossy().as_ref(), &source) {
                let _ = writeln!(stdout(), "error encountered running: {err:#}");
                file_failed = true;
            }
        } else {
            let _ = writeln!(stdout(), "skipping file {path:?}");
        }
    }
    file_failed
}

#[test]
fn test_scripts() {
    let mut file_failed = false;
    file_failed |= run_tests(BASE_DIR);
    assert!(!file_failed, "one or more errors occurred");
}
