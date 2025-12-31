//! Tests for the `cli` Lua module.

use super::LuaRuntime;
use std::path::Path;

#[test]
fn basic_command() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'add', 'item'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local ran = false
        local captured_args = nil
        cli.run {
            name = "test",
            commands = {
                { name = "add", run = function(a) ran = true; captured_args = a end },
            },
        }
        assert(ran, "command should have run")
        assert(captured_args[1] == "item", "positional arg should be passed")
        "#,
    );
    assert!(result.is_ok(), "cli basic failed: {:?}", result);
}

#[test]
fn long_options() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'greet', '--name', 'Alice', '--verbose'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "greet",
                    options = {
                        { name = "name", short = "n" },
                        { name = "verbose", short = "v", flag = true },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.name == "Alice", "name should be Alice, got: " .. tostring(captured.name))
        assert(captured.verbose == true, "verbose should be true")
        "#,
    );
    assert!(result.is_ok(), "cli long options failed: {:?}", result);
}

#[test]
fn short_options() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'greet', '-n', 'Bob', '-v'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "greet",
                    options = {
                        { name = "name", short = "n" },
                        { name = "verbose", short = "v", flag = true },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.name == "Bob", "name should be Bob")
        assert(captured.verbose == true, "verbose should be true")
        "#,
    );
    assert!(result.is_ok(), "cli short options failed: {:?}", result);
}

#[test]
fn positional_args() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'copy', 'src.txt', 'dst.txt', '--force'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "copy",
                    args = { "source", "dest" },
                    options = {
                        { name = "force", short = "f", flag = true },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.source == "src.txt", "source should be src.txt")
        assert(captured.dest == "dst.txt", "dest should be dst.txt")
        assert(captured.force == true, "force should be true")
        "#,
    );
    assert!(result.is_ok(), "cli positional args failed: {:?}", result);
}

#[test]
fn default_values() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'run'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "run",
                    options = {
                        { name = "port", short = "p", default = "8080" },
                        { name = "host", short = "h", default = "localhost" },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.port == "8080", "port should default to 8080")
        assert(captured.host == "localhost", "host should default to localhost")
        "#,
    );
    assert!(result.is_ok(), "cli default values failed: {:?}", result);
}

#[test]
fn no_command_runs_default() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local ran_default = false
        cli.run {
            name = "test",
            run = function(a) ran_default = true end,
            commands = {
                { name = "sub", run = function() end },
            },
        }
        assert(ran_default, "default handler should run when no command given")
        "#,
    );
    assert!(result.is_ok(), "cli no command failed: {:?}", result);
}

#[test]
fn unknown_command_error() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'unknown'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local error_msg = nil
        local orig_exit = os.exit
        os.exit = function(code) error("exit:" .. code) end

        local ok = pcall(function()
            cli.run {
                name = "test",
                commands = {
                    { name = "known", run = function() end },
                },
            }
        end)

        os.exit = orig_exit
        assert(not ok, "should have errored on unknown command")
        "#,
    );
    assert!(result.is_ok(), "cli unknown command failed: {:?}", result);
}

#[test]
fn help_flag() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'--help'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local printed = {}
        local orig_print = print
        print = function(...) table.insert(printed, table.concat({...}, "\t")) end
        local orig_exit = os.exit
        os.exit = function() error("exit") end

        pcall(function()
            cli.run {
                name = "myapp",
                description = "My test application",
                commands = {
                    { name = "run", description = "Run the app" },
                    { name = "build", description = "Build the app" },
                },
            }
        end)

        print = orig_print
        os.exit = orig_exit

        local output = table.concat(printed, "\n")
        assert(output:match("myapp"), "help should show app name")
        assert(output:match("run"), "help should show run command")
        assert(output:match("build"), "help should show build command")
        "#,
    );
    assert!(result.is_ok(), "cli help flag failed: {:?}", result);
}

#[test]
fn command_help() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'run', '--help'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local printed = {}
        local orig_print = print
        print = function(...) table.insert(printed, table.concat({...}, "\t")) end
        local orig_exit = os.exit
        os.exit = function() error("exit") end

        pcall(function()
            cli.run {
                name = "myapp",
                commands = {
                    {
                        name = "run",
                        description = "Run the application",
                        args = { "config" },
                        options = {
                            { name = "port", short = "p", description = "Port to listen on" },
                        },
                        run = function() end,
                    },
                },
            }
        end)

        print = orig_print
        os.exit = orig_exit

        local output = table.concat(printed, "\n")
        assert(output:match("run"), "help should show command name")
        assert(output:match("port") or output:match("%-p"), "help should show options")
        "#,
    );
    assert!(result.is_ok(), "cli command help failed: {:?}", result);
}

#[test]
fn multiple_positional_remaining() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'exec', 'echo', 'hello', 'world'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "exec",
                    run = function(a) captured = a end,
                },
            },
        }
        -- Remaining positional args should be in numeric indices
        assert(captured[1] == "echo", "first arg")
        assert(captured[2] == "hello", "second arg")
        assert(captured[3] == "world", "third arg")
        "#,
    );
    assert!(
        result.is_ok(),
        "cli multiple positional failed: {:?}",
        result
    );
}

#[test]
fn bundling_disabled_by_default() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    // Without bundling, -vn should be treated as -v with value "n"
    runtime.run_string("args = {'run', '-vn'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "run",
                    options = {
                        { name = "verbose", short = "v" },
                        { name = "dry-run", short = "n", flag = true },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        -- Without bundling, -vn means -v with value "n"
        assert(captured.verbose == "n", "verbose should be 'n', got: " .. tostring(captured.verbose))
        assert(captured.dry_run == nil, "dry_run should be nil")
        "#,
    );
    assert!(
        result.is_ok(),
        "bundling disabled test failed: {:?}",
        result
    );
}

#[test]
fn bundling_enabled() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'run', '-vn'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            bundling = true,
            commands = {
                {
                    name = "run",
                    options = {
                        { name = "verbose", short = "v", flag = true },
                        { name = "dry-run", short = "n", flag = true },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.verbose == true, "verbose should be true")
        assert(captured.dry_run == true, "dry_run should be true")
        "#,
    );
    assert!(result.is_ok(), "bundling enabled test failed: {:?}", result);
}

#[test]
fn negatable_flags() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'run', '--no-verbose'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            negatable = true,
            commands = {
                {
                    name = "run",
                    options = {
                        { name = "verbose", short = "v", flag = true, default = true },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.verbose == false, "verbose should be false after --no-verbose")
        "#,
    );
    assert!(result.is_ok(), "negatable flags test failed: {:?}", result);
}

#[test]
fn type_coercion() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'run', '--port', '8080', '--count', '5'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "run",
                    options = {
                        { name = "port", short = "p", type = "number" },
                        { name = "count", short = "c", type = "integer" },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(type(captured.port) == "number", "port should be number")
        assert(captured.port == 8080, "port should be 8080")
        assert(type(captured.count) == "number", "count should be number")
        assert(captured.count == 5, "count should be 5")
        "#,
    );
    assert!(result.is_ok(), "type coercion test failed: {:?}", result);
}

#[test]
fn env_var_fallback() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    // Set env var
    std::env::set_var("TEST_CLI_PORT", "9000");
    runtime.run_string("args = {'run'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "run",
                    options = {
                        { name = "port", short = "p", type = "number", env = "TEST_CLI_PORT" },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.port == 9000, "port should be 9000 from env, got: " .. tostring(captured.port))
        "#,
    );
    std::env::remove_var("TEST_CLI_PORT");
    assert!(result.is_ok(), "env var fallback test failed: {:?}", result);
}

#[test]
fn strict_mode_required_arg() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'run'}").unwrap(); // missing required arg

    // Mock os.exit to catch the error
    let result = runtime.run_string(
        r#"
        local cli = require("cli")
        local exit_called = false
        local orig_exit = os.exit
        os.exit = function(code) exit_called = true; error("exit:" .. code) end

        local ok = pcall(function()
            cli.run {
                name = "test",
                strict = true,
                commands = {
                    {
                        name = "run",
                        args = { "file" },  -- required arg
                        run = function(a) end,
                    },
                },
            }
        end)

        os.exit = orig_exit
        assert(exit_called, "should have called exit due to missing required arg")
        "#,
    );
    assert!(
        result.is_ok(),
        "strict mode required arg test failed: {:?}",
        result
    );
}

#[test]
fn command_aliases() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime.run_string("args = {'rm', 'file.txt'}").unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            commands = {
                {
                    name = "remove",
                    aliases = { "rm", "delete" },
                    args = { "file" },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.file == "file.txt", "file should be file.txt")
        "#,
    );
    assert!(result.is_ok(), "command aliases test failed: {:?}", result);
}

#[test]
fn mutually_exclusive_options() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'run', '--json', '--yaml'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")
        local exit_called = false
        local orig_exit = os.exit
        os.exit = function(code) exit_called = true; error("exit:" .. code) end

        local ok = pcall(function()
            cli.run {
                name = "test",
                strict = true,
                commands = {
                    {
                        name = "run",
                        options = {
                            { name = "json", flag = true, conflicts = { "yaml" } },
                            { name = "yaml", flag = true, conflicts = { "json" } },
                        },
                        run = function(a) end,
                    },
                },
            }
        end)

        os.exit = orig_exit
        assert(exit_called, "should have called exit due to conflicting options")
        "#,
    );
    assert!(
        result.is_ok(),
        "mutually exclusive options test failed: {:?}",
        result
    );
}

#[test]
fn global_options_inheritance() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    runtime
        .run_string("args = {'--verbose', 'run', '--port', '8080'}")
        .unwrap();
    let result = runtime.run_string(
        r#"
        local cli = require("cli")

        local captured = nil
        cli.run {
            name = "test",
            options = {
                { name = "verbose", short = "v", flag = true },
            },
            commands = {
                {
                    name = "run",
                    options = {
                        { name = "port", short = "p" },
                    },
                    run = function(a) captured = a end,
                },
            },
        }
        assert(captured.verbose == true, "verbose should be inherited from global options")
        assert(captured.port == "8080", "port should be 8080")
        "#,
    );
    assert!(
        result.is_ok(),
        "global options inheritance test failed: {:?}",
        result
    );
}
