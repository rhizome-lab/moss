//! Lua-based workflow runtime using LuaJIT.

use crate::parsers;
use std::path::Path;
use std::process::Command;

use mlua::{
    FromLua, Lua, LuaSerdeExt, Result as LuaResult, Table, Thread, UserData, UserDataMethods, Value,
};

use super::memory::MemoryStore;
use super::shadow::ShadowGit;

/// What the runtime is waiting for from the frontend.
#[derive(Debug, Clone)]
pub enum RuntimeYield {
    /// Waiting for user to enter text.
    Prompt { message: String },
    /// Waiting for user to pick from options.
    Menu { options: Vec<String> },
}

/// State of an interactive workflow.
#[derive(Debug)]
pub enum RuntimeState {
    /// Waiting for input from the frontend.
    Waiting(RuntimeYield),
    /// Finished successfully.
    Done(Option<CommandResult>),
    /// Errored.
    Error(String),
}

/// Lua workflow runtime.
pub struct LuaRuntime {
    lua: Lua,
}

/// Interactive workflow session (coroutine-based).
pub struct WorkflowSession {
    thread: Thread,
}

/// Result of a command execution.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub output: String,
    pub success: bool,
}

impl UserData for CommandResult {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("output", |_, this| Ok(this.output.clone()));
        fields.add_field_method_get("success", |_, this| Ok(this.success));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method("__tostring", |_, this, ()| Ok(this.output.clone()));
    }
}

/// Options for `view` command.
#[derive(Debug, Default)]
struct ViewOpts {
    target: Option<String>,
    depth: Option<i32>,
    deps: bool,
    context: bool,
}

impl FromLua for ViewOpts {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Nil => Ok(Self::default()),
            Value::String(s) => Ok(Self {
                target: Some(s.to_str()?.to_string()),
                ..Default::default()
            }),
            Value::Table(t) => Ok(Self {
                target: t.get("target").ok(),
                depth: t.get("depth").ok(),
                deps: t.get("deps").unwrap_or(false),
                context: t.get("context").unwrap_or(false),
            }),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ViewOpts".to_string(),
                message: None,
            }),
        }
    }
}

/// Options for `analyze` command.
#[derive(Debug, Default)]
struct AnalyzeOpts {
    target: Option<String>,
    health: bool,
    complexity: bool,
}

impl FromLua for AnalyzeOpts {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Nil => Ok(Self::default()),
            Value::Table(t) => Ok(Self {
                target: t.get("target").ok(),
                health: t.get("health").unwrap_or(false),
                complexity: t.get("complexity").unwrap_or(false),
            }),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "AnalyzeOpts".to_string(),
                message: None,
            }),
        }
    }
}

/// Options for `grep` command.
#[derive(Debug)]
struct GrepOpts {
    pattern: String,
    path: Option<String>,
    file_type: Option<String>,
}

impl FromLua for GrepOpts {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::String(s) => Ok(Self {
                pattern: s.to_str()?.to_string(),
                path: None,
                file_type: None,
            }),
            Value::Table(t) => Ok(Self {
                pattern: t.get("pattern")?,
                path: t.get("path").ok(),
                file_type: t.get("type").ok(),
            }),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "GrepOpts".to_string(),
                message: None,
            }),
        }
    }
}

impl LuaRuntime {
    pub fn new(root: &Path) -> LuaResult<Self> {
        // Load .env files (project root, then global config)
        let _ = dotenvy::from_path(root.join(".env"));
        if let Some(config_dir) = dirs::config_dir() {
            let _ = dotenvy::from_path(config_dir.join("moss").join(".env"));
        }

        let lua = Lua::new();

        {
            let globals = lua.globals();
            let root = root.to_path_buf();

            globals.set("_moss_root", root.to_string_lossy().to_string())?;

            // Expose the moss binary path for subprocess calls
            let moss_bin = std::env::current_exe()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "moss".to_string());
            globals.set("_moss_bin", moss_bin)?;

            Self::register_commands(&lua, &globals)?;
            Self::register_helpers(&lua, &globals, &root)?;
            Self::register_llm(&lua, &globals)?;
            Self::register_drivers(&lua, &globals, &root)?;
            Self::register_shadow(&lua, &globals, &root)?;
            Self::register_memory(&lua, &globals, &root)?;
            Self::register_treesitter(&lua, &globals)?;
            Self::register_modules(&lua)?;
        }

        Ok(Self { lua })
    }

    pub fn run_file(&self, path: &Path) -> LuaResult<()> {
        let script = std::fs::read_to_string(path)
            .map_err(|e| mlua::Error::external(format!("Failed to read script: {}", e)))?;
        self.run_string(&script)
    }

    pub fn run_string(&self, script: &str) -> LuaResult<()> {
        self.lua.load(script).exec()
    }

    /// Create an interactive workflow session from a script.
    /// The script runs as a coroutine that can yield for user input.
    pub fn create_session(&self, script: &str) -> LuaResult<WorkflowSession> {
        // Wrap script in coroutine.create
        let wrapped = format!(
            r#"return coroutine.create(function()
                {}
            end)"#,
            script
        );
        let thread: Thread = self.lua.load(&wrapped).eval()?;
        Ok(WorkflowSession { thread })
    }
}

impl WorkflowSession {
    /// Start or resume the workflow. Call with None to start, Some(input) to resume.
    pub fn step(&self, input: Option<&str>) -> LuaResult<RuntimeState> {
        use mlua::ThreadStatus;

        match self.thread.status() {
            ThreadStatus::Resumable => {
                // Resume with input (or nothing if starting)
                let result: mlua::MultiValue = if let Some(inp) = input {
                    self.thread.resume(inp)?
                } else {
                    self.thread.resume(())?
                };

                // Check if we yielded or finished
                match self.thread.status() {
                    ThreadStatus::Resumable => {
                        // Yielded - parse what we're waiting for
                        let mut values = result.into_iter();
                        let yield_type = values
                            .next()
                            .and_then(|v| v.as_str().map(|s| s.to_string()));

                        match yield_type.as_deref() {
                            Some("prompt") => {
                                let message = values
                                    .next()
                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                                    .unwrap_or_default();
                                Ok(RuntimeState::Waiting(RuntimeYield::Prompt { message }))
                            }
                            Some("menu") => {
                                let options = values
                                    .next()
                                    .and_then(|v| {
                                        if let Value::Table(t) = v {
                                            let opts: Vec<String> = t
                                                .sequence_values::<String>()
                                                .filter_map(|r| r.ok())
                                                .collect();
                                            Some(opts)
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or_default();
                                Ok(RuntimeState::Waiting(RuntimeYield::Menu { options }))
                            }
                            _ => Ok(RuntimeState::Error("Unknown yield type".to_string())),
                        }
                    }
                    ThreadStatus::Finished => {
                        // Finished - try to get CommandResult from return value
                        let cmd_result = result.into_iter().next().and_then(|v| {
                            if let Value::UserData(ud) = v {
                                ud.borrow::<CommandResult>().ok().map(|r| r.clone())
                            } else {
                                None
                            }
                        });
                        Ok(RuntimeState::Done(cmd_result))
                    }
                    ThreadStatus::Running => {
                        Ok(RuntimeState::Error("Thread still running".to_string()))
                    }
                    ThreadStatus::Error => Ok(RuntimeState::Error("Thread error".to_string())),
                }
            }
            ThreadStatus::Finished => Ok(RuntimeState::Done(None)),
            ThreadStatus::Running => Ok(RuntimeState::Error("Thread already running".to_string())),
            ThreadStatus::Error => Ok(RuntimeState::Error("Thread in error state".to_string())),
        }
    }
}

impl LuaRuntime {
    fn register_commands(lua: &Lua, globals: &Table) -> LuaResult<()> {
        // TODO: Refactor cmd_* functions to take typed structs, then call directly.
        // For now, convert typed opts to CLI args and use subprocess.

        // view(opts: ViewOpts) -> CommandResult
        globals.set(
            "view",
            lua.create_function(|_, opts: ViewOpts| {
                let mut args = vec!["view".to_string()];
                if let Some(t) = opts.target {
                    args.push(t);
                }
                if opts.deps {
                    args.push("--deps".to_string());
                }
                if opts.context {
                    args.push("--context".to_string());
                }
                if let Some(d) = opts.depth {
                    args.push("--depth".to_string());
                    args.push(d.to_string());
                }
                run_subprocess(&args)
            })?,
        )?;

        // analyze(opts: AnalyzeOpts) -> CommandResult
        globals.set(
            "analyze",
            lua.create_function(|_, opts: AnalyzeOpts| {
                let mut args = vec!["analyze".to_string()];
                if opts.health {
                    args.push("--health".to_string());
                }
                if opts.complexity {
                    args.push("--complexity".to_string());
                }
                if let Some(t) = opts.target {
                    args.push(t);
                }
                run_subprocess(&args)
            })?,
        )?;

        // grep(opts: GrepOpts) -> CommandResult
        globals.set(
            "grep",
            lua.create_function(|_, opts: GrepOpts| {
                let mut args = vec!["grep".to_string(), opts.pattern];
                if let Some(p) = opts.path {
                    args.push(p);
                }
                if let Some(t) = opts.file_type {
                    args.push("--type".to_string());
                    args.push(t);
                }
                run_subprocess(&args)
            })?,
        )?;

        // Simple commands
        macro_rules! simple_command {
            ($name:literal) => {{
                globals.set(
                    $name,
                    lua.create_function(|_, arg: Option<String>| {
                        let mut args = vec![$name.to_string()];
                        if let Some(a) = arg {
                            args.push(a);
                        }
                        run_subprocess(&args)
                    })?,
                )?;
            }};
        }

        simple_command!("edit");
        simple_command!("index");
        simple_command!("lint");
        simple_command!("plans");
        simple_command!("sessions");

        Ok(())
    }

    fn register_helpers(lua: &Lua, globals: &Table, root: &Path) -> LuaResult<()> {
        let root_path = root.to_path_buf();

        // shell(cmd: string) -> CommandResult
        let root_clone = root_path.clone();
        globals.set(
            "shell",
            lua.create_function(move |_, cmd: String| {
                let shell = if cfg!(windows) { "cmd" } else { "sh" };
                let flag = if cfg!(windows) { "/C" } else { "-c" };

                let output = Command::new(shell)
                    .args([flag, &cmd])
                    .current_dir(&root_clone)
                    .output()
                    .map_err(mlua::Error::external)?;

                Ok(CommandResult {
                    output: String::from_utf8_lossy(&output.stdout).to_string(),
                    success: output.status.success(),
                })
            })?,
        )?;

        // is_dirty() -> boolean
        let root_clone = root_path.clone();
        globals.set(
            "is_dirty",
            lua.create_function(move |_, ()| {
                let output = Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&root_clone)
                    .output()
                    .map_err(mlua::Error::external)?;
                Ok(!output.stdout.is_empty())
            })?,
        )?;

        // tests_pass() -> boolean
        let root_clone = root_path.clone();
        globals.set(
            "tests_pass",
            lua.create_function(move |_, ()| {
                let status = Command::new("cargo")
                    .args(["test", "--quiet"])
                    .current_dir(&root_clone)
                    .status()
                    .map_err(mlua::Error::external)?;
                Ok(status.success())
            })?,
        )?;

        // file_exists(path: string) -> boolean
        let root_clone = root_path.clone();
        globals.set(
            "file_exists",
            lua.create_function(move |_, path: String| Ok(root_clone.join(&path).exists()))?,
        )?;

        // read_file(path: string) -> string
        let root_clone = root_path.clone();
        globals.set(
            "read_file",
            lua.create_function(move |_, path: String| {
                std::fs::read_to_string(root_clone.join(&path)).map_err(mlua::Error::external)
            })?,
        )?;

        // write_file(path: string, content: string) -> boolean
        let root_clone = root_path.clone();
        globals.set(
            "write_file",
            lua.create_function(move |_, (path, content): (String, String)| {
                std::fs::write(root_clone.join(&path), content).map_err(mlua::Error::external)?;
                Ok(true)
            })?,
        )?;

        // print(...)
        globals.set(
            "print",
            lua.create_function(|lua, args: mlua::Variadic<Value>| {
                let parts: Vec<String> = args
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                        Value::Integer(i) => i.to_string(),
                        Value::Number(n) => n.to_string(),
                        Value::Boolean(b) => b.to_string(),
                        Value::Nil => "nil".to_string(),
                        Value::UserData(ud) => {
                            let tostring: mlua::Function = lua.globals().get("tostring").unwrap();
                            tostring
                                .call::<String>(ud.clone())
                                .unwrap_or_else(|_| format!("{:?}", v))
                        }
                        other => format!("{:?}", other),
                    })
                    .collect();
                println!("{}", parts.join("\t"));
                Ok(())
            })?,
        )?;

        // prompt(message) -> string (yields to frontend)
        // menu(options) -> string (yields to frontend)
        // These are Lua functions because yield must happen from Lua, not Rust
        lua.load(
            r#"
            function prompt(message)
                return coroutine.yield("prompt", message or "")
            end

            function menu(options)
                return coroutine.yield("menu", options)
            end
            "#,
        )
        .exec()?;

        // edit table for batch editing
        let edit_table = lua.create_table()?;
        let root_clone = root_path.clone();
        edit_table.set(
            "batch",
            lua.create_function(move |lua, (edits, opts): (Table, Option<Table>)| {
                use crate::edit::{BatchAction, BatchEdit, BatchEditOp};

                let mut batch = BatchEdit::new();

                // Parse edits table
                for pair in edits.pairs::<usize, Table>() {
                    let (_, edit) = pair.map_err(mlua::Error::external)?;
                    let target: String = edit.get("target").map_err(mlua::Error::external)?;
                    let action: String = edit.get("action").map_err(mlua::Error::external)?;

                    let action = match action.as_str() {
                        "delete" => BatchAction::Delete,
                        "replace" => {
                            let content: String =
                                edit.get("content").map_err(mlua::Error::external)?;
                            BatchAction::Replace { content }
                        }
                        "insert" => {
                            let content: String =
                                edit.get("content").map_err(mlua::Error::external)?;
                            let position: String =
                                edit.get("position").unwrap_or_else(|_| "after".to_string());
                            BatchAction::Insert { content, position }
                        }
                        other => {
                            return Err(mlua::Error::external(format!(
                                "Unknown action: {}",
                                other
                            )));
                        }
                    };

                    batch.add(BatchEditOp { target, action });
                }

                // Apply message from opts if provided
                if let Some(opts) = opts {
                    if let Ok(msg) = opts.get::<String>("message") {
                        batch = batch.with_message(&msg);
                    }
                }

                // Apply the batch
                match batch.apply(&root_clone) {
                    Ok(result) => {
                        let result_table = lua.create_table()?;
                        result_table.set("success", true)?;
                        result_table.set("edits_applied", result.edits_applied)?;
                        let files: Vec<String> = result
                            .files_modified
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();
                        result_table.set("files_modified", files)?;
                        Ok(result_table)
                    }
                    Err(e) => {
                        let result_table = lua.create_table()?;
                        result_table.set("success", false)?;
                        result_table.set("error", e)?;
                        Ok(result_table)
                    }
                }
            })?,
        )?;
        globals.set("edit", edit_table)?;

        Ok(())
    }

    fn register_llm(lua: &Lua, globals: &Table) -> LuaResult<()> {
        let llm_table = lua.create_table()?;

        #[cfg(feature = "llm")]
        {
            use super::llm::LlmClient;

            llm_table.set(
                "complete",
                lua.create_function(
                    |_,
                     (provider, model, system, prompt, max_tokens): (
                        Option<String>,
                        Option<String>,
                        Option<String>,
                        String,
                        Option<usize>,
                    )| {
                        let provider_str = provider.as_deref().unwrap_or("anthropic");
                        let client = LlmClient::new(provider_str, model.as_deref())
                            .map_err(mlua::Error::external)?;
                        client
                            .complete(system.as_deref(), &prompt, max_tokens)
                            .map_err(mlua::Error::external)
                    },
                )?,
            )?;

            // Chat with message history
            // Args: provider, model, system, prompt, history (table of {role, content})
            llm_table.set(
                "chat",
                lua.create_function(
                    |_,
                     (provider, model, system, prompt, history_table): (
                        Option<String>,
                        Option<String>,
                        Option<String>,
                        String,
                        mlua::Table,
                    )| {
                        // Convert Lua table to Vec<(String, String)>
                        let mut history = Vec::new();
                        for pair in history_table.sequence_values::<mlua::Table>() {
                            let pair = pair?;
                            let role: String = pair.get(1)?;
                            let content: String = pair.get(2)?;
                            history.push((role, content));
                        }

                        let provider_str = provider.as_deref().unwrap_or("anthropic");
                        let client = LlmClient::new(provider_str, model.as_deref())
                            .map_err(mlua::Error::external)?;
                        client
                            .chat(system.as_deref(), &prompt, history, None)
                            .map_err(mlua::Error::external)
                    },
                )?,
            )?;
        }

        #[cfg(not(feature = "llm"))]
        {
            llm_table.set(
                "complete",
                lua.create_function(|_, _: (Option<String>, Option<String>, Option<String>, String, Option<usize>)| {
                    Err::<String, _>(mlua::Error::external(
                        "llm.complete requires the 'llm' feature. Rebuild with: cargo build --features llm",
                    ))
                })?,
            )?;
        }

        globals.set("llm", llm_table)?;
        Ok(())
    }

    fn register_drivers(lua: &Lua, _globals: &Table, _root: &Path) -> LuaResult<()> {
        // manual { actions = {...} } - user-driven interactive loop
        // Defined in Lua because it needs to yield for user input
        lua.load(
            r#"
            function manual(config)
                local actions = config.actions
                if not actions then
                    error("manual{} requires actions table")
                end

                -- Build menu options from action names
                local options = {}
                for name, _ in pairs(actions) do
                    table.insert(options, name)
                end
                table.insert(options, "quit")
                table.sort(options)

                -- Main loop
                while true do
                    local choice = menu(options)
                    if choice == "quit" then
                        break
                    end

                    local action = actions[choice]
                    if action then
                        local ok, result = pcall(action)
                        if ok and result then
                            print(result)
                        elseif not ok then
                            print("Error: " .. tostring(result))
                        end
                    end
                end

                return { output = "", success = true }
            end
            "#,
        )
        .exec()?;

        // auto { actions = {...} } - run all actions automatically with streaming output
        lua.load(
            r#"
            function auto(config)
                local actions = config.actions
                if not actions then
                    error("auto{} requires actions table")
                end

                -- Sort action names for consistent ordering
                local names = {}
                for name, _ in pairs(actions) do
                    table.insert(names, name)
                end
                table.sort(names)

                local results = {}
                local all_ok = true

                -- Run each action with streaming output
                for _, name in ipairs(names) do
                    io.stdout:write("▶ " .. name .. "... ")
                    io.stdout:flush()

                    local action = actions[name]
                    local ok, result = pcall(action)

                    if ok then
                        print("✓")
                        if result then
                            print("  " .. tostring(result):gsub("\n", "\n  "))
                        end
                        results[name] = { success = true, output = result }
                    else
                        print("✗")
                        print("  Error: " .. tostring(result))
                        results[name] = { success = false, error = result }
                        all_ok = false
                        if config.stop_on_error then
                            break
                        end
                    end
                end

                return { results = results, success = all_ok }
            end
            "#,
        )
        .exec()?;

        Ok(())
    }

    fn register_shadow(lua: &Lua, globals: &Table, root: &Path) -> LuaResult<()> {
        let shadow_table = lua.create_table()?;
        let root_path = root.to_path_buf();

        // shadow.open() -> initializes/opens shadow git, returns snapshot id
        let root_clone = root_path.clone();
        shadow_table.set(
            "open",
            lua.create_function(move |lua, ()| {
                let sg = ShadowGit::open(&root_clone).map_err(mlua::Error::external)?;
                let head = sg.head().map_err(mlua::Error::external)?;
                // Store in registry for later use
                lua.set_named_registry_value("_shadow_git", LuaShadowGit(std::sync::Arc::new(sg)))?;
                Ok(head)
            })?,
        )?;

        // shadow.snapshot(files) -> snapshot id
        shadow_table.set(
            "snapshot",
            lua.create_function(|lua, files: Vec<String>| {
                let sg: LuaShadowGit = lua.named_registry_value("_shadow_git")?;
                let paths: Vec<std::path::PathBuf> =
                    files.iter().map(std::path::PathBuf::from).collect();
                let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
                let id = sg.0.snapshot(&refs).map_err(mlua::Error::external)?;
                Ok(id)
            })?,
        )?;

        // shadow.hunks() -> table of hunks
        shadow_table.set(
            "hunks",
            lua.create_function(|lua, ()| {
                let sg: LuaShadowGit = lua.named_registry_value("_shadow_git")?;
                let hunks = sg.0.hunks().map_err(mlua::Error::external)?;

                let result = lua.create_table()?;
                for (i, hunk) in hunks.iter().enumerate() {
                    let h = lua.create_table()?;
                    h.set("id", hunk.id)?;
                    h.set("file", hunk.file.to_string_lossy().to_string())?;
                    h.set("old_start", hunk.old_start)?;
                    h.set("old_lines", hunk.old_lines)?;
                    h.set("new_start", hunk.new_start)?;
                    h.set("new_lines", hunk.new_lines)?;
                    h.set("header", hunk.header.clone())?;
                    h.set("content", hunk.content.clone())?;
                    h.set("is_deletion", hunk.is_pure_deletion())?;
                    h.set("deletion_ratio", hunk.deletion_ratio())?;
                    result.set(i + 1, h)?;
                }
                Ok(result)
            })?,
        )?;

        // shadow.hunks_since(snapshot_id) -> table of hunks
        shadow_table.set(
            "hunks_since",
            lua.create_function(|lua, snapshot_id: String| {
                let sg: LuaShadowGit = lua.named_registry_value("_shadow_git")?;
                let hunks =
                    sg.0.hunks_since(&snapshot_id)
                        .map_err(mlua::Error::external)?;

                let result = lua.create_table()?;
                for (i, hunk) in hunks.iter().enumerate() {
                    let h = lua.create_table()?;
                    h.set("id", hunk.id)?;
                    h.set("file", hunk.file.to_string_lossy().to_string())?;
                    h.set("old_start", hunk.old_start)?;
                    h.set("old_lines", hunk.old_lines)?;
                    h.set("new_start", hunk.new_start)?;
                    h.set("new_lines", hunk.new_lines)?;
                    h.set("header", hunk.header.clone())?;
                    h.set("content", hunk.content.clone())?;
                    h.set("is_deletion", hunk.is_pure_deletion())?;
                    h.set("deletion_ratio", hunk.deletion_ratio())?;
                    result.set(i + 1, h)?;
                }
                Ok(result)
            })?,
        )?;

        // shadow.restore(snapshot_id, files?) -> restores files to snapshot state
        shadow_table.set(
            "restore",
            lua.create_function(|lua, (snapshot_id, files): (String, Option<Vec<String>>)| {
                let sg: LuaShadowGit = lua.named_registry_value("_shadow_git")?;
                let file_refs = files.as_ref().map(|f| {
                    let paths: Vec<std::path::PathBuf> =
                        f.iter().map(std::path::PathBuf::from).collect();
                    paths
                });
                let refs: Option<Vec<&Path>> = file_refs
                    .as_ref()
                    .map(|p| p.iter().map(|x| x.as_path()).collect());
                sg.0.restore(&snapshot_id, refs.as_deref())
                    .map_err(mlua::Error::external)?;
                Ok(())
            })?,
        )?;

        // shadow.head() -> current snapshot id
        shadow_table.set(
            "head",
            lua.create_function(|lua, ()| {
                let sg: LuaShadowGit = lua.named_registry_value("_shadow_git")?;
                let head = sg.0.head().map_err(mlua::Error::external)?;
                Ok(head)
            })?,
        )?;

        // shadow.list() -> list of all snapshots
        shadow_table.set(
            "list",
            lua.create_function(|lua, ()| {
                let sg: LuaShadowGit = lua.named_registry_value("_shadow_git")?;
                let snapshots = sg.0.list_snapshots().map_err(mlua::Error::external)?;

                let result = lua.create_table()?;
                for (i, (id, msg)) in snapshots.iter().enumerate() {
                    let s = lua.create_table()?;
                    s.set("id", id.clone())?;
                    s.set("message", msg.clone())?;
                    result.set(i + 1, s)?;
                }
                Ok(result)
            })?,
        )?;

        globals.set("shadow", shadow_table)?;
        Ok(())
    }

    fn register_memory(lua: &Lua, globals: &Table, root: &Path) -> LuaResult<()> {
        // Open memory store and keep in registry
        let store = MemoryStore::open(root).map_err(mlua::Error::external)?;
        lua.set_named_registry_value("_memory_store", LuaMemoryStore(std::sync::Arc::new(store)))?;

        // store(content, opts?) -> id
        // opts: { context = "...", weight = 1.0, metadata = {...} }
        globals.set(
            "store",
            lua.create_function(|lua, (content, opts): (String, Option<Table>)| {
                let ms: LuaMemoryStore = lua.named_registry_value("_memory_store")?;

                let context: Option<String> = opts.as_ref().and_then(|t| t.get("context").ok());
                let weight: Option<f64> = opts.as_ref().and_then(|t| t.get("weight").ok());

                // Convert metadata table to JSON if present
                let metadata: Option<String> = if let Some(ref t) = opts
                    && let Ok(meta) = t.get::<Value>("metadata")
                {
                    Some(value_to_json(lua, &meta)?)
                } else {
                    None
                };

                let id =
                    ms.0.store(&content, context.as_deref(), weight, metadata.as_deref())
                        .map_err(mlua::Error::external)?;

                Ok(id)
            })?,
        )?;

        // recall(query, limit?) -> list of items
        // Each item: { id, content, context, weight, created_at, accessed_at, metadata }
        globals.set(
            "recall",
            lua.create_function(|lua, (query, limit): (Value, Option<usize>)| {
                let ms: LuaMemoryStore = lua.named_registry_value("_memory_store")?;
                let limit = limit.unwrap_or(10);

                let items = match query {
                    Value::String(s) => {
                        let q = s.to_str()?;
                        ms.0.recall(&q, limit).map_err(mlua::Error::external)?
                    }
                    Value::Table(t) => {
                        // Query by metadata: recall({ slot = "system", author = { name = "X" } })
                        // Nested tables are flattened to path segments, arrays matched exactly
                        let filters = flatten_table_to_filters(lua, &t, "")?;
                        let filter_refs: Vec<(&str, &str)> = filters
                            .iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect();
                        ms.0.recall_by_metadata(&filter_refs, limit)
                            .map_err(mlua::Error::external)?
                    }
                    _ => {
                        return Err(mlua::Error::external(
                            "recall requires string query or table with key/value",
                        ));
                    }
                };

                let result = lua.create_table()?;
                for (i, item) in items.iter().enumerate() {
                    let t = lua.create_table()?;
                    t.set("id", item.id)?;
                    t.set("content", item.content.clone())?;
                    t.set("context", item.context.clone())?;
                    t.set("weight", item.weight)?;
                    t.set("created_at", item.created_at)?;
                    t.set("accessed_at", item.accessed_at)?;
                    t.set("metadata", item.metadata.clone())?;
                    result.set(i + 1, t)?;
                }
                Ok(result)
            })?,
        )?;

        // forget(query) -> count of deleted items
        globals.set(
            "forget",
            lua.create_function(|lua, query: String| {
                let ms: LuaMemoryStore = lua.named_registry_value("_memory_store")?;
                let count = ms.0.forget(&query).map_err(mlua::Error::external)?;
                Ok(count)
            })?,
        )?;

        Ok(())
    }

    fn register_treesitter(lua: &Lua, globals: &Table) -> LuaResult<()> {
        let ts_table = lua.create_table()?;

        // ts.parse(source, grammar) -> LuaTree
        ts_table.set(
            "parse",
            lua.create_function(|lua, (source, grammar): (String, String)| {
                let tree = parsers::parse_with_grammar(&grammar, &source).ok_or_else(|| {
                    mlua::Error::external(format!("Failed to parse with grammar '{}'", grammar))
                })?;

                // Store source alongside tree for text extraction
                let lua_tree = LuaTree {
                    tree: std::sync::Arc::new(tree),
                    source: std::sync::Arc::new(source),
                };
                lua.create_userdata(lua_tree)
            })?,
        )?;

        globals.set("ts", ts_table)?;
        Ok(())
    }

    /// Register builtin Lua modules for require().
    fn register_modules(lua: &Lua) -> LuaResult<()> {
        use crate::commands::script::modules;

        // Get package.preload table
        let package: Table = lua.globals().get("package")?;
        let preload: Table = package.get("preload")?;

        // Register each builtin module
        for name in &[
            "agent",
            "cli",
            "type",
            "type.describe",
            "type.validate",
            "type.generate",
            "test",
            "test.property",
        ] {
            if let Some(src) = modules::get(name) {
                let src = src.to_string();
                let name = *name;
                preload.set(
                    name,
                    lua.create_function(move |lua, ()| lua.load(&src).eval::<Value>())?,
                )?;
            }
        }

        Ok(())
    }
}

/// Wrapper for tree_sitter::Tree with source for Lua.
struct LuaTree {
    tree: std::sync::Arc<tree_sitter::Tree>,
    source: std::sync::Arc<String>,
}

impl UserData for LuaTree {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("root", |lua, this, ()| {
            let node = LuaNode {
                tree: this.tree.clone(),
                source: this.source.clone(),
                path: Vec::new(), // Empty path = root
            };
            lua.create_userdata(node)
        });
    }
}

/// Wrapper for tree_sitter::Node for Lua.
/// Stores tree reference and path from root (list of child indices) to reconstruct the node.
struct LuaNode {
    tree: std::sync::Arc<tree_sitter::Tree>,
    source: std::sync::Arc<String>,
    /// Path from root as child indices
    path: Vec<usize>,
}

impl LuaNode {
    fn get_node(&self) -> Option<tree_sitter::Node<'_>> {
        let mut node = self.tree.root_node();
        for &index in &self.path {
            node = node.child(index)?;
        }
        Some(node)
    }

    fn wrap_child(&self, child_index: usize) -> Self {
        let mut path = self.path.clone();
        path.push(child_index);
        LuaNode {
            tree: self.tree.clone(),
            source: self.source.clone(),
            path,
        }
    }
}

impl UserData for LuaNode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // node:kind() -> string
        methods.add_method("kind", |_, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            Ok(node.kind().to_string())
        });

        // node:text() -> string
        methods.add_method("text", |_, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            let start = node.start_byte();
            let end = node.end_byte();
            Ok(this.source[start..end].to_string())
        });

        // node:start_row() -> number (1-indexed for Lua)
        methods.add_method("start_row", |_, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            Ok(node.start_position().row + 1)
        });

        // node:end_row() -> number (1-indexed)
        methods.add_method("end_row", |_, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            Ok(node.end_position().row + 1)
        });

        // node:child_count() -> number
        methods.add_method("child_count", |_, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            Ok(node.child_count())
        });

        // node:child(index) -> LuaNode (1-indexed for Lua)
        methods.add_method("child", |lua, this, index: usize| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            let child_index = index.saturating_sub(1);
            let _child = node
                .child(child_index)
                .ok_or_else(|| mlua::Error::external("Child not found"))?;
            lua.create_userdata(this.wrap_child(child_index))
        });

        // node:children() -> table of LuaNodes
        methods.add_method("children", |lua, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            let result = lua.create_table()?;
            for i in 0..node.child_count() {
                result.set(i + 1, lua.create_userdata(this.wrap_child(i))?)?;
            }
            Ok(result)
        });

        // node:named_children() -> table of LuaNodes (excludes anonymous nodes)
        methods.add_method("named_children", |lua, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            let result = lua.create_table()?;
            let mut lua_index = 1;
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.is_named() {
                        result.set(lua_index, lua.create_userdata(this.wrap_child(i))?)?;
                        lua_index += 1;
                    }
                }
            }
            Ok(result)
        });

        // node:child_by_field(name) -> LuaNode or nil
        methods.add_method("child_by_field", |lua, this, name: String| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            // Find the index of the child with this field name
            for i in 0..node.child_count() {
                if node.field_name_for_child(i as u32) == Some(&name) {
                    return Ok(Some(lua.create_userdata(this.wrap_child(i))?));
                }
            }
            Ok(None)
        });

        // node:is_named() -> boolean
        methods.add_method("is_named", |_, this, ()| {
            let node = this
                .get_node()
                .ok_or_else(|| mlua::Error::external("Node not found"))?;
            Ok(node.is_named())
        });

        // node:parent() -> LuaNode or nil
        methods.add_method("parent", |lua, this, ()| {
            if this.path.is_empty() {
                return Ok(None); // Root has no parent
            }
            let mut parent_path = this.path.clone();
            parent_path.pop();
            let parent = LuaNode {
                tree: this.tree.clone(),
                source: this.source.clone(),
                path: parent_path,
            };
            Ok(Some(lua.create_userdata(parent)?))
        });

        // node:next_sibling() -> LuaNode or nil
        methods.add_method("next_sibling", |lua, this, ()| {
            if this.path.is_empty() {
                return Ok(None); // Root has no siblings
            }
            let mut sibling_path = this.path.clone();
            let last_index = sibling_path.pop().unwrap();

            // Check if next sibling exists
            let mut parent = this.tree.root_node();
            for &index in &sibling_path {
                parent = parent
                    .child(index)
                    .ok_or_else(|| mlua::Error::external("Path invalid"))?;
            }
            if last_index + 1 >= parent.child_count() {
                return Ok(None);
            }

            sibling_path.push(last_index + 1);
            let sibling = LuaNode {
                tree: this.tree.clone(),
                source: this.source.clone(),
                path: sibling_path,
            };
            Ok(Some(lua.create_userdata(sibling)?))
        });

        // node:prev_sibling() -> LuaNode or nil
        methods.add_method("prev_sibling", |lua, this, ()| {
            if this.path.is_empty() {
                return Ok(None); // Root has no siblings
            }
            let mut sibling_path = this.path.clone();
            let last_index = sibling_path.pop().unwrap();

            if last_index == 0 {
                return Ok(None);
            }

            sibling_path.push(last_index - 1);
            let sibling = LuaNode {
                tree: this.tree.clone(),
                source: this.source.clone(),
                path: sibling_path,
            };
            Ok(Some(lua.create_userdata(sibling)?))
        });
    }
}

/// Convert a Lua value to JSON string using mlua's serde support.
fn value_to_json(lua: &Lua, v: &Value) -> LuaResult<String> {
    let json: serde_json::Value = lua.from_value(v.clone())?;
    Ok(json.to_string())
}

/// Flatten a Lua table into metadata filters.
/// Nested tables become path segments, arrays become JSON strings.
fn flatten_table_to_filters(
    lua: &Lua,
    t: &Table,
    prefix: &str,
) -> LuaResult<Vec<(String, String)>> {
    let mut filters = Vec::new();

    for pair in t.pairs::<String, Value>() {
        let (key, value) = pair?;
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", prefix, key)
        };

        match &value {
            Value::String(s) => {
                filters.push((path, s.to_str()?.to_string()));
            }
            Value::Integer(i) => {
                filters.push((path, i.to_string()));
            }
            Value::Number(n) => {
                filters.push((path, n.to_string()));
            }
            Value::Boolean(b) => {
                filters.push((path, b.to_string()));
            }
            Value::Table(nested) => {
                // Check if array-like (has raw_len > 0)
                if nested.raw_len() > 0 {
                    // Array - serialize to JSON for exact match
                    let json = value_to_json(lua, &value)?;
                    filters.push((path, json));
                } else {
                    // Object - recurse
                    filters.extend(flatten_table_to_filters(lua, nested, &path)?);
                }
            }
            _ => {
                return Err(mlua::Error::external(format!(
                    "unsupported value type in metadata query: {}",
                    value.type_name()
                )));
            }
        }
    }

    Ok(filters)
}

/// Wrapper for ShadowGit to store in Lua registry.
struct LuaShadowGit(std::sync::Arc<ShadowGit>);

impl Clone for LuaShadowGit {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl UserData for LuaShadowGit {}

impl FromLua for LuaShadowGit {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => Ok(ud.borrow::<LuaShadowGit>()?.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaShadowGit".to_string(),
                message: Some("expected ShadowGit userdata".to_string()),
            }),
        }
    }
}

/// Wrapper for MemoryStore to store in Lua registry.
struct LuaMemoryStore(std::sync::Arc<MemoryStore>);

impl Clone for LuaMemoryStore {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl UserData for LuaMemoryStore {}

impl FromLua for LuaMemoryStore {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => Ok(ud.borrow::<LuaMemoryStore>()?.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaMemoryStore".to_string(),
                message: Some("expected MemoryStore userdata".to_string()),
            }),
        }
    }
}

/// Fallback: run moss as subprocess (for commands not yet refactored).
fn run_subprocess(args: &[String]) -> LuaResult<CommandResult> {
    let exe = std::env::current_exe().map_err(mlua::Error::external)?;
    let output = Command::new(&exe)
        .args(args)
        .output()
        .map_err(mlua::Error::external)?;

    Ok(CommandResult {
        output: String::from_utf8_lossy(&output.stdout).to_string(),
        success: output.status.success(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_opts_from_string() {
        let lua = Lua::new();
        let val = lua.create_string("foo.rs").unwrap();
        let opts = ViewOpts::from_lua(Value::String(val), &lua).unwrap();
        assert_eq!(opts.target, Some("foo.rs".to_string()));
    }

    #[test]
    fn test_view_opts_from_table() {
        let lua = Lua::new();
        lua.load(r#"return { target = "bar.rs", context = true }"#)
            .eval::<Value>()
            .and_then(|v| ViewOpts::from_lua(v, &lua))
            .map(|opts| {
                assert_eq!(opts.target, Some("bar.rs".to_string()));
                assert!(opts.context);
            })
            .unwrap();
    }

    #[test]
    fn test_session_prompt() {
        let runtime = LuaRuntime::new(std::path::Path::new(".")).unwrap();
        let session = runtime
            .create_session(r#"local x = prompt("Enter name: ") return x"#)
            .unwrap();

        // Start - should yield waiting for prompt
        match session.step(None).unwrap() {
            RuntimeState::Waiting(RuntimeYield::Prompt { message }) => {
                assert_eq!(message, "Enter name: ");
            }
            other => panic!("Expected Prompt, got {:?}", other),
        }

        // Resume with input - should finish
        match session.step(Some("Alice")).unwrap() {
            RuntimeState::Done(_) => {}
            other => panic!("Expected Done, got {:?}", other),
        }
    }

    #[test]
    fn test_session_menu() {
        let runtime = LuaRuntime::new(std::path::Path::new(".")).unwrap();
        let session = runtime
            .create_session(r#"local x = menu({"a", "b", "c"}) return x"#)
            .unwrap();

        // Start - should yield waiting for menu
        match session.step(None).unwrap() {
            RuntimeState::Waiting(RuntimeYield::Menu { options }) => {
                assert_eq!(options, vec!["a", "b", "c"]);
            }
            other => panic!("Expected Menu, got {:?}", other),
        }

        // Resume with selection - should finish
        match session.step(Some("b")).unwrap() {
            RuntimeState::Done(_) => {}
            other => panic!("Expected Done, got {:?}", other),
        }
    }

    #[test]
    fn test_session_no_yield() {
        let runtime = LuaRuntime::new(std::path::Path::new(".")).unwrap();
        let session = runtime.create_session(r#"return "done""#).unwrap();

        // Start - should finish immediately
        match session.step(None).unwrap() {
            RuntimeState::Done(_) => {}
            other => panic!("Expected Done, got {:?}", other),
        }
    }

    #[test]
    fn test_manual_driver() {
        let runtime = LuaRuntime::new(std::path::Path::new(".")).unwrap();
        let session = runtime
            .create_session(
                r#"manual{
                    actions = {
                        check = function() return analyze() end,
                        find = function() return grep(prompt("Pattern: ")) end,
                    }
                }"#,
            )
            .unwrap();

        // Start - should show menu with defined actions + quit
        match session.step(None).unwrap() {
            RuntimeState::Waiting(RuntimeYield::Menu { options }) => {
                assert!(options.contains(&"quit".to_string()));
                assert!(options.contains(&"check".to_string()));
                assert!(options.contains(&"find".to_string()));
            }
            other => panic!("Expected Menu, got {:?}", other),
        }

        // Select quit - should finish
        match session.step(Some("quit")).unwrap() {
            RuntimeState::Done(_) => {}
            other => panic!("Expected Done after quit, got {:?}", other),
        }
    }

    #[test]
    fn test_manual_driver_requires_actions() {
        let runtime = LuaRuntime::new(std::path::Path::new(".")).unwrap();
        let session = runtime.create_session(r#"manual{}"#).unwrap();

        // Start - should error because no actions provided
        let result = session.step(None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires actions table"), "Error was: {}", err);
    }

    #[test]
    fn test_shadow_lua_api() {
        let tmp = tempfile::TempDir::new().unwrap();
        let runtime = LuaRuntime::new(tmp.path()).unwrap();

        // Open shadow git
        let result = runtime.run_string(
            r#"
            local head = shadow.open()
            assert(head ~= nil, "shadow.open() should return head")
            assert(#head > 0, "head should be non-empty")
            "#,
        );
        assert!(result.is_ok(), "shadow.open failed: {:?}", result);

        // Create a file and snapshot it
        std::fs::write(tmp.path().join("test.txt"), "hello").unwrap();

        let result = runtime.run_string(
            r#"
            local snap = shadow.snapshot({"test.txt"})
            assert(snap ~= nil, "snapshot should return id")
            "#,
        );
        assert!(result.is_ok(), "shadow.snapshot failed: {:?}", result);

        // List snapshots
        let result = runtime.run_string(
            r#"
            local snaps = shadow.list()
            assert(#snaps >= 2, "should have at least 2 snapshots (initial + our snapshot)")
            "#,
        );
        assert!(result.is_ok(), "shadow.list failed: {:?}", result);
    }

    #[test]
    fn test_memory_lua_api() {
        let tmp = tempfile::TempDir::new().unwrap();
        let runtime = LuaRuntime::new(tmp.path()).unwrap();

        // Store some items
        let result = runtime.run_string(
            r#"
            local id1 = store("User prefers tabs", { context = "formatting", weight = 1.0 })
            assert(id1 == 1, "first store should return id 1")

            local id2 = store("auth.py broke tests", { context = "auth.py", weight = 0.8 })
            assert(id2 == 2, "second store should return id 2")

            local id3 = store("system prompt", { metadata = { slot = "system" } })
            assert(id3 == 3, "third store should return id 3")
            "#,
        );
        assert!(result.is_ok(), "store failed: {:?}", result);

        // Recall by content
        let result = runtime.run_string(
            r#"
            local items = recall("tabs")
            assert(#items == 1, "should find 1 item matching 'tabs'")
            assert(items[1].content:find("tabs"), "content should contain 'tabs'")
            "#,
        );
        assert!(result.is_ok(), "recall by content failed: {:?}", result);

        // Recall by context
        let result = runtime.run_string(
            r#"
            local items = recall("auth.py")
            assert(#items == 1, "should find 1 item with auth.py context")
            "#,
        );
        assert!(result.is_ok(), "recall by context failed: {:?}", result);

        // Recall by metadata
        let result = runtime.run_string(
            r#"
            local items = recall({ slot = "system" })
            assert(#items == 1, "should find 1 item with slot=system")
            assert(items[1].content:find("system prompt"), "content should be system prompt")
            "#,
        );
        assert!(result.is_ok(), "recall by metadata failed: {:?}", result);

        // Forget
        let result = runtime.run_string(
            r#"
            local count = forget("auth.py")
            assert(count == 1, "should forget 1 item")

            local items = recall("auth.py")
            assert(#items == 0, "should find no items after forget")
            "#,
        );
        assert!(result.is_ok(), "forget failed: {:?}", result);
    }

    #[test]
    fn test_memory_nested_and_arrays() {
        let tmp = tempfile::TempDir::new().unwrap();
        let runtime = LuaRuntime::new(tmp.path()).unwrap();

        // Store with nested metadata
        let result = runtime.run_string(
            r#"
            store("nested item", { metadata = { author = { name = "Alice", org = "ACME" } } })
            store("array item", { metadata = { tags = {"rust", "lua"} } })
            "#,
        );
        assert!(result.is_ok(), "store failed: {:?}", result);

        // Query with nested object (flattened to dot notation)
        let result = runtime.run_string(
            r#"
            local items = recall({ author = { name = "Alice" } })
            assert(#items == 1, "should find 1 item with author.name=Alice, got " .. #items)
            assert(items[1].content == "nested item", "content mismatch")
            "#,
        );
        assert!(result.is_ok(), "nested query failed: {:?}", result);

        // Query with array (exact match)
        let result = runtime.run_string(
            r#"
            local items = recall({ tags = {"rust", "lua"} })
            assert(#items == 1, "should find 1 item with exact tags array")
            assert(items[1].content == "array item", "content mismatch")
            "#,
        );
        assert!(result.is_ok(), "array query failed: {:?}", result);
    }
}

#[cfg(test)]
#[path = "lua_runtime/test_cli_module.rs"]
mod test_cli_module;
#[cfg(test)]
#[path = "lua_runtime/test_generate_module.rs"]
mod test_generate_module;
#[cfg(test)]
#[path = "lua_runtime/test_property_module.rs"]
mod test_property_module;
#[cfg(test)]
#[path = "lua_runtime/test_test_module.rs"]
mod test_test_module;
#[cfg(test)]
#[path = "lua_runtime/test_type_module.rs"]
mod test_type_module;
#[cfg(test)]
#[path = "lua_runtime/test_validate_module.rs"]
mod test_validate_module;
