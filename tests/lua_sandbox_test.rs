//! Tests for the Lua sandbox boundary.
//!
//! Screen scripts come from screen repos that byonk re-fetches on a timer, so
//! an upstream change runs new Lua without anyone looking at it. The VM must
//! therefore not hand a script the filesystem or the process: no `io`, no
//! `os.execute`, no `os.exit`. The time helpers screens actually use stay.

use std::collections::HashMap;
use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::services::LuaRuntime;
use tempfile::TempDir;

/// Create a screens dir holding `scripts`, plus a loader that reads from it.
fn setup_test_env(scripts: &[(&str, &str)]) -> (TempDir, Arc<AssetLoader>) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let screens_dir = temp_dir.path().to_path_buf();

    for (name, content) in scripts {
        std::fs::write(screens_dir.join(name), content).expect("Failed to write test script");
    }

    let asset_loader = Arc::new(AssetLoader::new(Some(screens_dir), None, None));
    (temp_dir, asset_loader)
}

/// The one that matters: a script must not be able to create a file. With the
/// `io` library loaded this wrote "pwned" to disk; the add-on maps host
/// directories, so that reach is a real escape, not a theoretical one.
#[test]
fn a_screen_script_cannot_write_a_file() {
    let target_dir = TempDir::new().expect("Failed to create target dir");
    let target = target_dir.path().join("escaped.txt");

    let script = format!(
        r#"
        local ok, err = pcall(function()
            local f = io.open([[{}]], "w")
            f:write("pwned")
            f:close()
        end)
        return {{ data = {{ ok = ok, err = tostring(err) }}, refresh_rate = 60 }}
        "#,
        target.display()
    );

    let (_temp_dir, asset_loader) = setup_test_env(&[("escape.lua", script.as_str())]);
    let runtime = LuaRuntime::new(asset_loader);

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("escape.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("the script guards its own failure with pcall, so the run succeeds");

    assert_eq!(
        result.data["ok"], false,
        "opening a file for writing must fail, got: {}",
        result.data["err"]
    );
    assert!(
        !target.exists(),
        "a screen script wrote {} — the sandbox leaks the filesystem",
        target.display()
    );
}

/// Every name a script could use to reach the filesystem, the environment or
/// the process is gone. `os.exit` alone would let one screen kill the server.
#[test]
fn dangerous_stdlib_entries_are_absent_from_a_screen_script() {
    let script = r#"
        local function gone(v) return v == nil end
        return {
            data = {
                io = gone(io),
                package = gone(package),
                debug = gone(debug),
                dofile = gone(dofile),
                loadfile = gone(loadfile),
                load = gone(load),
                os_execute = gone(os.execute),
                os_exit = gone(os.exit),
                os_getenv = gone(os.getenv),
                os_remove = gone(os.remove),
                os_rename = gone(os.rename),
                os_setlocale = gone(os.setlocale),
                os_tmpname = gone(os.tmpname),
            },
            refresh_rate = 60
        }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("probe.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("probe.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("the probe script only reads globals");

    for name in [
        "io",
        "package",
        "debug",
        "dofile",
        "loadfile",
        "load",
        "os_execute",
        "os_exit",
        "os_getenv",
        "os_remove",
        "os_rename",
        "os_setlocale",
        "os_tmpname",
    ] {
        assert_eq!(
            result.data[name], true,
            "`{name}` is still reachable from a screen script"
        );
    }
}

/// Taking `load` away from scripts is pointless if byonk itself will load a
/// precompiled chunk: mlua sniffs the source and switches to binary mode on
/// its own, and crafted bytecode escapes the VM whatever the globals say. A
/// `script.lua` is source code, so byonk must load it as text and say so.
#[test]
fn a_screen_script_is_loaded_as_text_never_as_bytecode() {
    // The Lua bytecode signature, ESC "Lua". Valid UTF-8, so it survives the
    // read; what follows does not matter, only which loader gets it.
    let script = "\u{1b}Lua not really bytecode";

    let (_temp_dir, asset_loader) = setup_test_env(&[("bytecode.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let err = runtime
        .run_script_from_asset(
            std::path::Path::new("bytecode.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect_err("a chunk that claims to be bytecode must not load");

    assert!(
        err.to_string().contains("attempt to load a binary chunk"),
        "byonk fed the chunk to the bytecode loader instead of refusing it: {err}"
    );
}

/// A sandbox that refuses things has to say whose script it refused. Without a
/// chunk name mlua labels the chunk with the Rust call site, so every screen
/// error read `src/services/lua_runtime.rs:<line>` — byonk blaming itself for
/// the author's typo.
#[test]
fn a_script_error_names_the_screen_not_byonks_own_source() {
    let script = r#"error("boom")"#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("broken.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let err = runtime
        .run_script_from_asset(
            std::path::Path::new("broken.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect_err("the script raises");
    let message = err.to_string();

    assert!(
        message.contains("boom"),
        "the author's own message must survive: {message}"
    );
    assert!(
        message.contains("broken"),
        "the error must name the screen it came from: {message}"
    );
    assert!(
        !message.contains("lua_runtime.rs"),
        "the error points at byonk's source instead of the screen: {message}"
    );
}

/// The counterweight to the test above: the clock functions screens do use
/// (`screens/examples/gphoto/script.lua` calls `os.time`) must survive.
#[test]
fn the_time_functions_screens_use_still_work() {
    let script = r#"
        local t = os.time()
        local d = os.date("!%Y", 0)
        return {
            data = {
                time_is_number = type(t) == "number",
                time_is_positive = t > 0,
                date_of_epoch = d,
                difftime = os.difftime(10, 4),
                clock_is_number = type(os.clock()) == "number",
            },
            refresh_rate = 60
        }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("clock.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("clock.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("the clock functions must stay available to screens");

    assert_eq!(result.data["time_is_number"], true);
    assert_eq!(result.data["time_is_positive"], true);
    assert_eq!(result.data["date_of_epoch"], "1970");
    assert_eq!(result.data["difftime"], 6.0);
    assert_eq!(result.data["clock_is_number"], true);
}
