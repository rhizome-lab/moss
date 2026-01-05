-- Agent module: autonomous task execution with moss tools
local M = {}

-- Submodules
local risk = require("agent.risk")
local parser = require("agent.parser")
local session = require("agent.session")
local context = require("agent.context")
local commands = require("agent.commands")

-- Seed random on load
math.randomseed(os.time())

-- ID generation (delegated to agent.session module)
M.gen_id = session.gen_id
M.gen_session_id = session.gen_session_id

-- Risk assessment (delegated to agent.risk module)
M.RISK = risk.RISK
M.assess_risk = risk.assess_risk
M.should_auto_approve = risk.should_auto_approve
M.detect_validator = risk.detect_validator

-- Session management (delegated to agent.session module)
M.start_session_log = session.start_session_log
M.json_log_entry = session.json_log_entry
M.list_logs = session.list_logs
M.save_checkpoint = session.save_checkpoint
M.load_checkpoint = session.load_checkpoint
M.parse_checkpoint_json = session.parse_checkpoint_json
M.list_sessions = session.list_sessions

-- JSON utilities (delegated to agent.parser module)
M.json_encode_string = parser.json_encode_string
M.json_decode_string = parser.json_decode_string

-- Memorize a fact to long-term memory (.moss/memory/facts.md)
-- Returns: true on success, false + error message on failure
function M.memorize(fact)
    local memory_dir = _moss_root .. "/.moss/memory"
    local facts_file = memory_dir .. "/facts.md"

    -- Ensure directory exists
    os.execute("mkdir -p " .. memory_dir)

    -- Append fact with timestamp
    local file, err = io.open(facts_file, "a")
    if not file then
        return false, err
    end

    local timestamp = os.date("%Y-%m-%d %H:%M")
    file:write("- " .. fact .. " (" .. timestamp .. ")\n")
    file:close()

    return true
end

-- Batch edit execution (delegated to agent.commands module)
M.execute_batch_edit = commands.execute_batch_edit

local SYSTEM_PROMPT = [[
Respond with commands to accomplish the task.
Conclude with $(done ANSWER) as soon as you have enough evidence.
]]

-- Bootstrap: two exchanges showing (1) exploration (2) reasoning + conclusion
local BOOTSTRAP = {
    -- Exchange 1: ask for help
    {
        role = "assistant",
        content = "I'm unfamiliar with this codebase. Let me see what commands I have available.\n\n$(help)"
    },
    -- Exchange 2: reasoning + conclusion example
    {
        role = "assistant",
        content = "I can see the answer in the results. There are 3 items: A, B, and C.\n\n$(done 3)"
    },
    {
        role = "user",
        content = "Correct!"
    }
}

-- For backwards compat, keep the old format too
local BOOTSTRAP_ASSISTANT = BOOTSTRAP[1].content

local BOOTSTRAP_USER = [[
Available commands:

Exploration:
$(view .) - view current directory
$(view <path>) - view file or directory
$(view <path/Symbol>) - view specific symbol
$(view --types-only <path>) - only type definitions
$(view --deps <path>) - show dependencies/imports
$(view <path>:<start>-<end>) - view line range
$(text-search "<pattern>") - search for text
$(text-search "<pattern>" --only <glob>) - search in specific files

Analysis:
$(analyze complexity) - find complex functions
$(analyze length) - find long functions
$(analyze security) - find security issues
$(analyze duplicate-functions) - find code clones
$(analyze callers <symbol>) - show what calls this
$(analyze callees <symbol>) - show what this calls
$(analyze hotspots) - git history hotspots

Package:
$(package list) - list dependencies
$(package tree) - dependency tree
$(package outdated) - outdated packages
$(package audit) - check vulnerabilities

Editing:
$(edit <path/Symbol> delete) - delete symbol
$(edit <path/Symbol> replace <code>) - replace symbol
$(edit <path/Symbol> insert --before <code>) - insert before
$(edit <path/Symbol> insert --after <code>) - insert after
$(batch-edit <t1> <a1> <c1> | <t2> <a2> <c2>) - multiple edits

Shell:
$(run <shell command>) - execute shell command

Memory:
$(note <finding>) - record finding for session
$(keep) - keep all outputs in working memory
$(keep 1 3) - keep specific outputs by index
$(drop <id>) - remove from working memory
$(memorize <fact>) - save to long-term memory
$(forget <pattern>) - remove notes matching pattern

Session:
$(checkpoint <progress> | <questions>) - save for later
$(ask <question>) - ask user for input
$(done <answer>) - end session with answer

Outputs disappear each turn unless you $(keep) or $(note) them.
]]

-- State machine configuration
-- Three specialized states: planner (optional), explorer, evaluator
-- Role-specific prompts swap out while keeping the same state machine

-- ROLE: investigator (default) - answers questions about the codebase
local INVESTIGATOR_PROMPTS = {
    planner = [[
Create a brief plan to accomplish this task.
List 2-4 concrete steps, then say "Ready to explore."
Do not execute commands - just plan the approach.
]],

    explorer = [[
You are an INVESTIGATOR. Output commands to gather information.

FORMAT: Commands MUST use $(cmd args) syntax exactly. No markdown, no backticks.
CORRECT: $(view src/main.rs) $(analyze length)
WRONG: ```$(view src/main.rs)``` or `view src/main.rs`

Commands:
$(view path) - file structure/symbols
$(view path:start-end) - specific lines
$(text-search "pattern") - search codebase
$(analyze subcommand) - code analysis (complexity, length, security, etc)
$(package subcommand) - dependency analysis
$(run cmd) - shell command (use sparingly)

Do NOT answer - that's the evaluator's job. Just output commands.
]],

    evaluator = [[
You are an EVALUATOR. Judge what we found - do NOT explore further.

FORBIDDEN (will be ignored):
- $(view ...), $(text-search ...), $(run ...) - you don't explore
- Markdown code blocks with commands
- "Let me check", "I need to look" - explorer phrases

ALLOWED:
- $(answer your conclusion here) - final answer
- $(note finding) - record a finding
- $(keep 1 3), $(drop 2) - manage memory

If answer is clear: $(answer The answer based on evidence)
If incomplete: $(note what we found) then explain what's missing

Example:
"Found support_for_extension() in registry.rs maps extensions to languages.
$(note Language detection in moss-languages/registry.rs)
$(answer moss detects language by file extension via support_for_extension())"
]],
}

-- ROLE: auditor - finds issues (security, quality, patterns)
local AUDITOR_PROMPTS = {
    planner = [[
You are a code AUDITOR. Plan your audit strategy.

Given the audit scope, list 2-4 specific things to check:
- What patterns indicate the issue type?
- Which files/modules are most likely affected?
- What commands will reveal the issues?

Then say "Ready to audit."
Do not execute commands yet - just plan.
]],

    explorer = [[
You are an AUDITOR. Run commands to find issues systematically.

FORMAT: Commands MUST use $(cmd args) syntax exactly. No markdown, no backticks.
CORRECT: $(analyze security) $(view file.rs:10-20)
WRONG: ```$(analyze ...)``` or `analyze security`

Commands:
$(view path) - examine code structure
$(view path:start-end) - inspect specific lines
$(text-search "pattern") - find specific patterns
$(analyze subcommand) - analysis tools (security, length, complexity, duplicate-functions)
$(run cmd) - shell command (use sparingly)

Prefer $(analyze ...) over text-search for structured analysis.
Focus on file:line locations. Do NOT conclude - that's the evaluator's job.
]],

    evaluator = [[
You are an AUDIT EVALUATOR. Assess findings - do NOT explore further.

FORBIDDEN (will be ignored):
- $(view ...), $(text-search ...), $(run ...) - you don't explore
- Markdown code blocks with commands
- "Let me check", "I need to look" - explorer phrases

ALLOWED:
- $(answer formatted findings) - final report
- $(note SEVERITY:TYPE file:line - description) - record finding
- $(keep 1 3), $(drop 2) - manage memory

Finding format: $(note SECURITY:HIGH file.rs:45 - description)
Severities: critical, high, medium, low
Types: SECURITY, QUALITY, PATTERN

If complete:
$(answer
## Findings
### High
- file.rs:45 - SECURITY: description
### Medium
- other.rs:10 - QUALITY: description
)

If incomplete: $(note findings) then explain what areas remain.
]],
}

-- ROLE: refactorer - makes changes to fix issues
local REFACTORER_PROMPTS = {
    planner = [[
You are a code REFACTORER. Plan your changes carefully.

Given the task, outline:
1. What files need to be modified
2. What specific changes are needed
3. How to verify the changes work

Then say "Ready to refactor."
Do not execute commands yet - just plan.
]],

    explorer = [[
You are a REFACTORER. Explore code, then make changes.

FORMAT: Commands MUST use $(cmd args) syntax exactly. No markdown.
CORRECT: $(view file.rs) $(edit file.rs/function_name replace new_code)
WRONG: ```$(edit ...)``` or `edit ...`

Exploration:
$(view path) - examine code structure
$(text-search "pattern") - find specific patterns
$(analyze callers symbol) - find callers before changing
$(analyze callees symbol) - find callees

Editing:
$(edit path/Symbol delete) - delete symbol
$(edit path/Symbol replace new_code) - replace symbol
$(edit path/Symbol insert --before code) - insert before
$(edit path/Symbol insert --after code) - insert after

Validation:
$(run cargo check) - check compilation
$(run cargo test) - run tests

Make ONE change at a time, then validate before continuing.
]],

    evaluator = [[
You are a REFACTOR EVALUATOR. Assess the changes made.

FORBIDDEN (will be ignored):
- $(view ...), $(text-search ...), $(edit ...) - you don't act
- Markdown code blocks with commands

ALLOWED:
- $(answer summary of changes) - when complete
- $(note what was changed) - record progress
- $(keep 1 3), $(drop 2) - manage memory

Check validation results:
- Did the build pass? Did tests pass?
- If validation failed, explain what went wrong

If changes complete and validated:
$(answer
## Changes Made
- file.rs: replaced function X with Y
- other.rs: added error handling

Validation: cargo check passed, cargo test passed
)

If validation failed: $(note what failed) then explain the fix needed.
]],
}

-- Role registry
local ROLE_PROMPTS = {
    investigator = INVESTIGATOR_PROMPTS,
    auditor = AUDITOR_PROMPTS,
    refactorer = REFACTORER_PROMPTS,
}

-- LLM-based auto-dispatch classifier
-- Used for: subagent spawning, dynamic role switching mid-task
local CLASSIFIER_PROMPT = [[
Classify this task into exactly one role. Reply with ONLY the role name.

Roles:
- investigator: questions, exploration, understanding code ("how does X work?", "where is Y?")
- auditor: finding issues, security/quality checks ("find bugs", "check for vulnerabilities")
- refactorer: making changes, fixing, updating code ("rename X to Y", "add error handling")

Task: %s

Role:]]

function M.classify_task(task, provider, model)
    provider = provider or "gemini"
    local prompt = string.format(CLASSIFIER_PROMPT, task)

    -- Lightweight call with no history
    local response = llm.chat(provider, model, "", prompt, {})
    if not response then
        return "investigator"  -- fallback
    end

    -- Extract role from response
    local role_lower = response:lower():gsub("%s+", "")
    if role_lower:find("auditor") then
        return "auditor"
    elseif role_lower:find("refactor") then
        return "refactorer"
    else
        return "investigator"
    end
end

-- Build machine config for a given role
local function build_machine(role)
    local prompts = ROLE_PROMPTS[role] or ROLE_PROMPTS.investigator
    return {
        start = "explorer",  -- can be overridden to "planner"

        states = {
            planner = {
                prompt = prompts.planner,
                context = "task_only",
                next = "explorer",
            },

            explorer = {
                prompt = prompts.explorer,
                context = "last_outputs",
                next = "evaluator",
            },

            evaluator = {
                prompt = prompts.evaluator,
                context = "working_memory",
                next = "explorer",
            },
        },
    }
end

-- Default machine (for backwards compat)
local MACHINE = build_machine("investigator")

-- Context building (delegated to agent.context module)
M.build_planner_context = context.build_planner_context
M.build_explorer_context = context.build_explorer_context
M.build_evaluator_context = context.build_evaluator_context

-- State machine agent runner (v2)
function M.run_state_machine(opts)
    opts = opts or {}
    local task = opts.prompt or opts.task or "Help with this codebase"
    local max_turns = opts.max_turns or 10
    local provider = opts.provider or "gemini"
    local model = opts.model  -- nil means use provider default
    local use_planner = opts.plan or false
    local role = opts.role
    if not role then
        if opts.auto_dispatch and task then
            print("[agent-v2] Classifying task...")
            role = M.classify_task(task, provider, model)
            print(string.format("[agent-v2] Auto-dispatch → %s", role))
        else
            role = "investigator"
        end
    end

    -- Refactorer always plans first
    if role == "refactorer" then
        use_planner = true
    end

    -- Initialize shadow worktree for safe editing (--shadow flag or auto for refactorer)
    local shadow_enabled = opts.shadow
    if shadow_enabled then
        print("[agent-v2] Initializing shadow worktree for safe editing...")
        local ok, err = pcall(function()
            shadow.worktree.open()
            shadow.worktree.sync()
            shadow.worktree.enable()
        end)
        if ok then
            print("[agent-v2] Shadow mode enabled - edits go to .moss/shadow/worktree/")
        else
            print("[agent-v2] Warning: Failed to initialize shadow worktree: " .. tostring(err))
            shadow_enabled = false
        end
    end

    -- Auto-detect validator if:
    -- 1. Shadow enabled and no explicit --validate, OR
    -- 2. --auto-validate flag is set
    if (shadow_enabled or opts.auto_validate) and not opts.validate_cmd then
        local detected_cmd, detected_type = M.detect_validator()
        if detected_cmd then
            opts.validate_cmd = detected_cmd
            print("[agent-v2] Auto-detected validator: " .. detected_cmd .. " (" .. detected_type .. ")")
        end
    end

    -- Handle --diff: get changed files and add to task context
    if opts.diff_base ~= nil then
        local base = opts.diff_base
        if base == "" then
            local detect = shell("git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null || git rev-parse --verify origin/main 2>/dev/null || git rev-parse --verify origin/master 2>/dev/null || git rev-parse --verify main 2>/dev/null || git rev-parse --verify master 2>/dev/null")
            if detect.success and detect.output and detect.output ~= "" then
                base = detect.output:match("refs/remotes/(.+)") or detect.output:gsub("%s+", "")
            else
                base = "HEAD~10"
            end
        end

        local merge_base_result = shell("git merge-base " .. base .. " HEAD 2>/dev/null")
        local effective_base = merge_base_result.success and merge_base_result.output:gsub("%s+", "") or base

        local diff_result = shell("git diff --name-only " .. effective_base)
        if diff_result.success and diff_result.output and diff_result.output ~= "" then
            local diff_files = {}
            for file in diff_result.output:gmatch("[^\n]+") do
                table.insert(diff_files, file)
            end
            print("[agent-v2] Focusing on " .. #diff_files .. " changed files (vs " .. base .. ")")
            task = task .. "\n\nFOCUS: Only analyze these changed files:\n"
            for _, f in ipairs(diff_files) do
                task = task .. "  - " .. f .. "\n"
            end
            task = task .. "\nIgnore unchanged files unless directly relevant to changes."
        end
    end

    -- Build machine config for this role
    local machine = build_machine(role)

    local session_id = M.gen_session_id()
    print(string.format("[agent-v2:%s] Session: %s", role, session_id))

    -- Start session logging
    local session_log = M.start_session_log(session_id)
    local start_state = use_planner and "planner" or "explorer"
    if session_log then
        session_log:log("task", {
            system_prompt = "state_machine_v2",
            user_prompt = task,
            provider = provider,
            model = model or "default",
            max_turns = max_turns,
            machine_start = start_state,
            use_planner = use_planner,
            role = role
        })
    end

    local state = start_state
    local notes = {}           -- accumulated notes
    local working_memory = {}  -- curated outputs kept by evaluator
    local last_outputs = {}    -- most recent turn's outputs (pending curation)
    local plan = nil           -- plan from planner state
    local recent_cmds = {}     -- for loop detection
    local turn = 0
    local validation_retry_count = 0  -- track validation retries

    while turn < max_turns do
        turn = turn + 1
        local state_config = machine.states[state]

        -- Build context based on state
        local context
        if state == "planner" then
            context = M.build_planner_context(task)
        elseif state == "explorer" then
            context = M.build_explorer_context(task, last_outputs, notes, plan)
        else
            context = M.build_evaluator_context(task, working_memory, last_outputs, notes)
        end

        print(string.format("[agent-v2] Turn %d/%d (%s)", turn, max_turns, state))
        io.write("[agent-v2] Thinking... ")
        io.flush()

        -- Log turn start
        if session_log then
            session_log.turn_count = turn
            session_log:log("turn_start", {
                turn = turn,
                state = state,
                working_memory_count = #working_memory,
                notes_count = #notes,
                pending_outputs = #last_outputs
            })
        end

        -- LLM call with optional bootstrap
        local history = {}
        if state_config.bootstrap then
            -- Inject bootstrap as fake assistant turn
            table.insert(history, {role = "assistant", content = state_config.bootstrap})
        end
        local response = llm.chat(provider, model, state_config.prompt, context, history)
        io.write("done\n")
        print(response)

        -- Log LLM response
        if session_log then
            session_log:log("llm_response", {
                turn = turn,
                state = state,
                response = response:sub(1, 500)  -- truncate for log
            })
        end

        -- Handle planner state - save plan and transition
        if state == "planner" then
            plan = response
            if session_log then
                session_log:log("plan_created", { plan = response:sub(1, 500) })
            end
            state = state_config.next
            goto continue
        end

        -- Parse commands from response (handles quoted strings properly)
        local commands = M.parse_commands(response)

        -- Handle $(done) or $(answer) - only valid in evaluator state
        for _, cmd in ipairs(commands) do
            if cmd.name == "done" or cmd.name == "answer" then
                if state == "evaluator" then
                    local final_answer = cmd.args
                    -- Models often output "$(done ANSWER) - actual answer"
                    -- If args is just "ANSWER", look for text after the $(done ...) in response
                    if final_answer == "ANSWER" then
                        local after = response:match('%$%(done%s+ANSWER%)%s*[-:]?%s*(.+)')
                        if after then
                            final_answer = after:gsub('\n.*', '')  -- first line only
                        end
                    end
                    -- Handle shadow mode finalization
                    if shadow_enabled then
                        print("[agent-v2] Finalizing shadow edits...")
                        local diff = shadow.worktree.diff()
                        if diff and #diff > 0 then
                            print("[agent-v2] Changes in shadow worktree:")
                            print(diff)

                            -- Validate if validate_cmd is set
                            local should_apply = true
                            local validation_error = nil
                            if opts.validate_cmd then
                                print("[agent-v2] Validating: " .. opts.validate_cmd)
                                local validation = shadow.worktree.validate(opts.validate_cmd)
                                if validation.success then
                                    print("[agent-v2] Validation passed ✓")
                                else
                                    print("[agent-v2] Validation FAILED:")
                                    validation_error = validation.stdout or validation.stderr or "Unknown error"
                                    print(validation_error)
                                    should_apply = false
                                end
                            end

                            if should_apply then
                                print("[agent-v2] Applying shadow changes to real repo...")
                                local applied = shadow.worktree.apply()
                                print("[agent-v2] Applied " .. #applied .. " file(s)")

                                -- Auto-commit if --commit flag is set
                                if opts.commit and #applied > 0 then
                                    print("[agent-v2] Creating git commit...")
                                    -- Stage all applied files
                                    for _, file in ipairs(applied) do
                                        shell("git add " .. file)
                                    end
                                    -- Generate commit message from task
                                    local commit_msg = task:sub(1, 50)
                                    if #task > 50 then
                                        commit_msg = commit_msg .. "..."
                                    end
                                    local result = shell("git commit -m '[moss agent] " .. commit_msg:gsub("'", "'\\''") .. "'")
                                    if result.success then
                                        print("[agent-v2] Committed changes ✓")
                                    else
                                        print("[agent-v2] Warning: git commit failed - " .. (result.output or ""))
                                    end
                                end
                            else
                                -- Validation failed - retry if allowed
                                local max_retries = opts.retry_on_failure or 0
                                if validation_retry_count < max_retries then
                                    validation_retry_count = validation_retry_count + 1
                                    print("[agent-v2] Retrying (" .. validation_retry_count .. "/" .. max_retries .. ")...")
                                    -- Reset shadow and inject error into working memory
                                    shadow.worktree.reset()
                                    shadow.worktree.sync()  -- Resync to clean state
                                    table.insert(working_memory, {
                                        id = M.gen_id(),
                                        type = "error",
                                        content = "VALIDATION FAILED. Please fix this error and try again:\n" .. validation_error,
                                    })
                                    -- Don't return - continue the state machine
                                    goto continue
                                else
                                    print("[agent-v2] Discarding shadow changes (validation failed" ..
                                        (max_retries > 0 and ", max retries reached" or "") .. ")")
                                    shadow.worktree.reset()
                                end
                            end
                        else
                            print("[agent-v2] No shadow changes to apply")
                        end
                        shadow.worktree.disable()
                    end

                    print("[agent-v2] Answer: " .. final_answer)
                    if session_log then
                        session_log:log("done", { answer = final_answer:sub(1, 200), turn = turn })
                        session_log:close()
                    end
                    return {success = true, answer = final_answer, turns = turn}
                else
                    print("[agent-v2] Warning: $(" .. cmd.name .. ") ignored in explorer state")
                end
            end
        end

        -- Handle $(note) commands
        for _, cmd in ipairs(commands) do
            if cmd.name == "note" then
                table.insert(notes, cmd.args)
                print("[agent-v2] Noted: " .. cmd.args)
            end
        end

        -- Handle $(keep) and $(drop) - only in evaluator state
        if state == "evaluator" then
            for _, cmd in ipairs(commands) do
                if cmd.name == "keep" then
                    local indices = M.parse_keep("keep " .. cmd.args, #last_outputs)
                    for _, idx in ipairs(indices) do
                        if last_outputs[idx] then
                            table.insert(working_memory, last_outputs[idx])
                            print("[agent-v2] Kept: " .. last_outputs[idx].cmd)
                        end
                    end
                elseif cmd.name == "drop" then
                    local idx = tonumber(cmd.args)
                    if idx and working_memory[idx] then
                        print("[agent-v2] Dropped: " .. working_memory[idx].cmd)
                        table.remove(working_memory, idx)
                    end
                end
            end
        end

        -- Execute exploration commands (only in explorer state)
        if state == "explorer" then
            last_outputs = {}
            for _, cmd in ipairs(commands) do
                if cmd.name ~= "note" and cmd.name ~= "done" and cmd.name ~= "answer" then
                    local result
                    if cmd.name == "run" then
                        print("[agent-v2] Running: " .. cmd.args)
                        result = shell(cmd.args)
                    elseif cmd.name == "view" or cmd.name == "text-search" or
                           cmd.name == "analyze" or cmd.name == "package" or
                           cmd.name == "edit" then
                        print("[agent-v2] Running: " .. cmd.full)
                        result = shell(_moss_bin .. " " .. cmd.full)
                    else
                        -- Unknown command, skip
                        print("[agent-v2] Skipping unknown: " .. cmd.name)
                        result = nil
                    end
                    if result then
                        -- Truncate large outputs to prevent context bloat
                        local content = result.output or ""
                        local MAX_OUTPUT = 10000  -- ~10KB per command output
                        local truncated = false
                        if #content > MAX_OUTPUT then
                            content = content:sub(1, MAX_OUTPUT)
                            content = content .. "\n... [OUTPUT TRUNCATED - " .. (#(result.output or "") - MAX_OUTPUT) .. " more bytes]\n"
                            truncated = true
                        end
                        table.insert(last_outputs, {
                            cmd = cmd.full,
                            content = content,
                            success = result.success,
                            truncated = truncated
                        })
                        if session_log then
                            session_log:log("command", {
                                cmd = cmd.full,
                                success = result.success,
                                output_length = (result.output or ""):len(),
                                turn = turn
                            })
                        end
                    end
                end
            end

            -- Track commands for loop detection
            for _, out in ipairs(last_outputs) do
                table.insert(recent_cmds, out.cmd)
            end

            -- Check for loops (same command 3+ times in a row)
            if M.is_looping(recent_cmds, 3) then
                print("[agent-v2] Loop detected, bailing out")
                if shadow_enabled then
                    print("[agent-v2] Discarding shadow changes (loop detected)")
                    shadow.worktree.reset()
                    shadow.worktree.disable()
                end
                if session_log then
                    session_log:log("loop_detected", { cmd = recent_cmds[#recent_cmds], turn = turn })
                    session_log:close()
                end
                return {success = false, reason = "loop_detected", turns = turn}
            end
        end

        -- Transition to next state
        state = state_config.next
        ::continue::
    end

    print("[agent-v2] Max turns reached")
    if shadow_enabled then
        print("[agent-v2] Discarding shadow changes (max turns)")
        shadow.worktree.reset()
        shadow.worktree.disable()
    end
    if session_log then
        session_log:log("max_turns_reached", { turn = turn })
        session_log:close()
    end
    return {success = false, reason = "max_turns", turns = turn}
end

-- Check if last N commands are identical (loop detection)
-- recent_cmds is a list of recent command strings
function M.is_looping(recent_cmds, n)
    n = n or 3
    if #recent_cmds < n then return false end

    local last_cmd = recent_cmds[#recent_cmds]
    for i = 1, n - 1 do
        if recent_cmds[#recent_cmds - i] ~= last_cmd then
            return false
        end
    end
    return true
end

-- Context building continued (delegated to agent.context module)
M.build_error_context = context.build_error_context
M.build_context = context.build_context

-- Command parsing (delegated to agent.parser module)
M.parse_commands = parser.parse_commands
M.parse_keep = parser.parse_keep

-- Main agent loop
function M.show_help()
    print([[Usage: moss @agent [options] <task>

Options:
  --provider <name>   LLM provider (gemini, openrouter, ollama)
  --model <name>      Model name for the provider
  --max-turns <n>     Maximum conversation turns (default: 15)
  --explain           Trace: show full tool outputs
  --resume <id>       Resume from a previous session
  --list-sessions     List available sessions to resume
  --list-logs         List session log files
  -n, --non-interactive  Skip user input prompts
  --v2                Use state machine agent (auto for --audit, --refactor)
  --role <name>       Agent role (explorer, auditor, refactorer, investigator)
  --audit             Shorthand for --role auditor --v2
  --refactor          Shorthand for --role refactorer --v2 --plan
  --plan              Enable planning mode
  --validate <cmd>    Run validation command after edits (e.g., "cargo check")
  --auto-validate     Auto-detect validation command (cargo check, tsc, etc.)
  --shadow            Edit in shadow worktree, validate before applying (enables --auto-validate)
  --auto-approve [LEVEL]  Auto-approve edits up to risk level (low/medium/high, default: low)
  --commit            Auto-commit changes after successful validation
  --retry-on-failure [N]  Retry up to N times on validation failure (default: 1)
  --diff [base]       Focus on git diff (auto-detects main/master if base omitted)
  --auto              Auto-dispatch based on task analysis
  --roles             List available roles and descriptions
  -h, --help          Show this help message

Examples:
  moss @agent "add error handling to parse_config"
  moss @agent --refactor --validate "cargo check" "extract helper function"
  moss @agent --refactor --shadow "rename foo to bar safely"
  moss @agent --audit "review security of auth module"
  moss @agent --resume abc123
]])
end

function M.run(opts)
    opts = opts or {}
    if opts.help then
        M.show_help()
        return { success = true }
    end
    local task = opts.prompt or opts.task or "Help with this codebase"
    local max_turns = opts.max_turns or 15
    local provider = opts.provider or "gemini"
    local model = opts.model
    local session_id = opts.resume or M.gen_session_id()
    local start_turn = 1
    local non_interactive = opts.non_interactive or false
    local validate_cmd = opts.validate_cmd  -- Auto-validate after edits

    -- Resume from checkpoint if specified
    local working_memory = {}
    if opts.resume then
        local state, err = M.load_checkpoint(opts.resume)
        if state then
            print("[agent] Resuming session: " .. opts.resume)
            task = state.task or task
            working_memory = state.working_memory or {}
            start_turn = (state.turn or 0) + 1
            if state.progress then
                print("[agent] Previous progress: " .. state.progress)
            end
            if state.open_questions then
                print("[agent] Open questions: " .. state.open_questions)
            end
        else
            print("[agent] Warning: " .. (err or "Failed to load checkpoint"))
            print("[agent] Starting fresh session")
            session_id = M.gen_session_id()
        end
    else
        -- Recall relevant memories into working memory (only for new sessions)
        local ok, memories = pcall(recall, task, 3)
        if ok and memories and #memories > 0 then
            for _, m in ipairs(memories) do
                table.insert(working_memory, {type = "note", id = M.gen_id(), content = "(recalled) " .. m.content})
            end
        end
    end

    print("[agent] Session: " .. session_id)
    if validate_cmd then
        print("[agent] Auto-validation enabled: " .. validate_cmd)
    end

    -- Start session logging (always enabled for analysis)
    local session_log = M.start_session_log(session_id)
    if session_log then
        session_log:log("task", {
            system_prompt = SYSTEM_PROMPT,
            user_prompt = task,
            provider = provider,
            model = model or "default",
            max_turns = max_turns,
            resumed = opts.resume ~= nil,
            validate_cmd = validate_cmd
        })
    end

    -- Build task description
    local task_desc = task
    if opts.explain then
        task_desc = task_desc .. "\nIMPORTANT: Your final answer MUST end with '## Steps' listing each command you ran and why it was needed."
    end
    task_desc = task_desc .. "\nDirectory: " .. _moss_root

    -- Add diff context if --diff specified
    local diff_files = nil
    if opts.diff_base ~= nil then
        local base = opts.diff_base
        if base == "" then
            -- Auto-detect default branch
            local detect = shell("git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null || git rev-parse --verify origin/main 2>/dev/null || git rev-parse --verify origin/master 2>/dev/null || git rev-parse --verify main 2>/dev/null || git rev-parse --verify master 2>/dev/null")
            if detect.success and detect.output and detect.output ~= "" then
                base = detect.output:match("refs/remotes/(.+)") or detect.output:gsub("%s+", "")
            else
                print("[agent] Warning: Could not detect default branch, using HEAD~10")
                base = "HEAD~10"
            end
        end

        -- Get merge-base for proper comparison
        local merge_base_result = shell("git merge-base " .. base .. " HEAD 2>/dev/null")
        local effective_base = merge_base_result.success and merge_base_result.output:gsub("%s+", "") or base

        -- Get changed files
        local diff_result = shell("git diff --name-only " .. effective_base)
        if diff_result.success and diff_result.output and diff_result.output ~= "" then
            diff_files = {}
            for file in diff_result.output:gmatch("[^\n]+") do
                table.insert(diff_files, file)
            end

            print("[agent] Focusing on " .. #diff_files .. " changed files (vs " .. base .. ")")
            task_desc = task_desc .. "\n\nFOCUS: Only analyze these changed files:\n"
            for _, f in ipairs(diff_files) do
                task_desc = task_desc .. "  - " .. f .. "\n"
            end
            task_desc = task_desc .. "\nIgnore unchanged files unless directly relevant to changes."
        else
            print("[agent] No changed files found relative to " .. base)
        end
    end

    -- Initialize shadow git for rollback capability
    local shadow_ok = pcall(function()
        shadow.open()
        shadow.snapshot({})
    end)

    local recent_cmds = {}  -- for loop detection
    local all_output = {}
    local total_retries = 0
    local current_outputs = nil  -- outputs from last turn (ephemeral)
    local error_state = nil  -- tracks escalation: {cmd, retries, rolled_back}

    -- Open log file if debug mode
    local log_file = nil
    if os.getenv("MOSS_AGENT_DEBUG") then
        log_file = io.open("/tmp/moss-agent.log", "w")
        if log_file then
            log_file:write("=== Agent session: " .. task .. " ===\n\n")
        end
    end

    for turn = start_turn, max_turns do
        print(string.format("[agent] Turn %d/%d (session: %s)", turn, max_turns, session_id))

        -- Build context from working memory + current outputs + error state
        local prompt = M.build_context(task_desc, working_memory, current_outputs, error_state)

        -- Log turn start
        if session_log then
            session_log.turn_count = turn
            session_log:log("turn_start", {
                turn = turn,
                prompt = prompt,
                working_memory_count = #working_memory,
                has_error_state = error_state ~= nil
            })
        end

        -- Add loop warning if needed
        if M.is_looping(recent_cmds, 3) then
            prompt = prompt .. "\nWARNING: You've run the same command 3 times. Explain what's wrong and try a different approach.\n"
        end

        -- Debug output
        if os.getenv("MOSS_AGENT_DEBUG") then
            print("[DEBUG] Prompt length: " .. #prompt)
            print("[DEBUG] Working memory items: " .. #working_memory)
        end
        io.write("[agent] Thinking... ")
        io.flush()

        -- Retry logic with exponential backoff
        -- Bootstrap: inject a fake exchange where the model "asked" for help
        -- This establishes $(cmd) syntax through example rather than instruction
        -- Bootstrap history: teach $(cmd) syntax + reasoning + conclusion
        local bootstrap_history = {
            {"assistant", BOOTSTRAP[1].content},  -- "Let me see what commands..."
            {"user", BOOTSTRAP_USER},              -- Command list
            {"assistant", BOOTSTRAP[2].content},  -- "I can see the answer... $(done 3)"
            {"user", BOOTSTRAP[3].content}        -- "Correct!"
        }

        -- Gemini ignores system prompts - prepend to first user message instead
        local effective_system = SYSTEM_PROMPT
        local effective_history = bootstrap_history
        if provider == "gemini" then
            effective_system = ""
            -- Prepend system prompt as user message
            effective_history = {{"user", SYSTEM_PROMPT}}
            for _, msg in ipairs(bootstrap_history) do
                table.insert(effective_history, msg)
            end
        end

        local response
        local max_retries = 3
        for attempt = 1, max_retries do
            local ok, result = pcall(function()
                return llm.chat(provider, model, effective_system, prompt, effective_history)
            end)
            if ok then
                response = result
                break
            elseif attempt < max_retries then
                total_retries = total_retries + 1
                local delay = 2 ^ (attempt - 1)  -- 1s, 2s, 4s
                io.write("retry in " .. delay .. "s... ")
                io.flush()
                os.execute("sleep " .. delay)
            else
                error(result)
            end
        end
        io.write("done\n")

        -- Log LLM response
        if log_file then
            log_file:write("\n=== Turn " .. turn .. " LLM Response ===\n")
            log_file:write(response)
            log_file:write("\n")
            log_file:flush()
        end

        print(response)
        table.insert(all_output, response)

        -- Log LLM response
        if session_log then
            session_log:log("llm_response", {
                turn = turn,
                response = response,
                retries = total_retries
            })
        end

        -- Extract commands from response (handles quoted strings properly)
        local commands = {}
        local parsed = M.parse_commands(response)
        for _, cmd in ipairs(parsed) do
            if cmd.name == "view" or cmd.name == "text-search" or cmd.name == "run" or
               cmd.name == "note" or cmd.name == "done" or cmd.name == "keep" or
               cmd.name == "drop" or cmd.name == "memorize" or cmd.name == "forget" or
               cmd.name == "analyze" or cmd.name == "package" or cmd.name == "edit" or
               cmd.name == "batch-edit" or cmd.name == "checkpoint" or cmd.name == "ask" or
               cmd.name == "wait" or cmd.name == "help" then
                table.insert(commands, cmd.name .. " " .. cmd.args)
            end
        end

        -- Fallback: Python-style syntax for models that use it
        if #commands == 0 then
            for path in response:gmatch('view%s*%(%s*["\']([^"\']+)["\']%s*%)') do
                table.insert(commands, "view " .. path)
            end
            for path in response:gmatch('view%s*%(%s*["\']([^"\']+)["\']%s*,%s*types_only%s*=%s*True%s*%)') do
                table.insert(commands, "view --types-only " .. path)
            end
            for pattern in response:gmatch('text_search%s*%(%s*["\']([^"\']+)["\']%s*%)') do
                table.insert(commands, "text-search \"" .. pattern .. "\"")
            end
            for cmd in response:gmatch('run%s*%(%s*["\']([^"\']+)["\']%s*%)') do
                table.insert(commands, "run " .. cmd)
            end
            for finding in response:gmatch('note%s*%(%s*["\']([^"\']+)["\']%s*%)') do
                table.insert(commands, "note " .. finding)
            end
            for answer in response:gmatch('done%s*%(%s*["\']([^"\']+)["\']%s*%)') do
                table.insert(commands, "done " .. answer)
            end
        end

        if #commands == 0 then
            print("[agent] No commands found, finishing")
            if session_log then
                session_log:close()
            end
            if total_retries > 0 then
                print("[agent] API retries: " .. total_retries)
            end
            return { success = true, output = table.concat(all_output, "\n") }
        end

        -- Guard against runaway model output
        local max_commands_per_turn = 10
        if #commands > max_commands_per_turn then
            print(string.format("[agent] WARNING: Model output %d commands, limiting to %d", #commands, max_commands_per_turn))
            local limited = {}
            for i = 1, max_commands_per_turn do
                limited[i] = commands[i]
            end
            commands = limited
        end

        -- Separate execution commands from memory commands
        local exec_commands = {}
        local keep_commands = {}
        local note_commands = {}
        local drop_commands = {}
        local forget_commands = {}
        local memorize_commands = {}
        local checkpoint_cmd = nil
        local done_summary = nil

        local wait_flag = false
        for _, cmd in ipairs(commands) do
            if cmd:match("^done") then
                done_summary = cmd:match("^done%s*(.*)") or ""
            elseif cmd:match("^wait") then
                wait_flag = true
            elseif cmd:match("^checkpoint") then
                checkpoint_cmd = cmd
            elseif cmd:match("^keep") then
                table.insert(keep_commands, cmd)
            elseif cmd:match("^note ") then
                table.insert(note_commands, cmd)
            elseif cmd:match("^drop ") then
                table.insert(drop_commands, cmd)
            elseif cmd:match("^forget ") then
                table.insert(forget_commands, cmd)
            elseif cmd:match("^memorize ") then
                table.insert(memorize_commands, cmd)
            else
                table.insert(exec_commands, cmd)
            end
        end

        -- If $(wait) present, treat $(done) as invalid (prevent pre-answering)
        if wait_flag and done_summary then
            print("[agent] WARNING: $(wait) and $(done) in same turn - ignoring $(done)")
            done_summary = nil
        end

        -- If ONLY done (no exec commands), return immediately
        if done_summary and #exec_commands == 0 then
            print("[agent] Done: " .. done_summary)
            if session_log then
                session_log:log("done", { summary = done_summary:sub(1, 200) })
                session_log:close()
            end
            if total_retries > 0 then
                print("[agent] API retries: " .. total_retries)
            end
            return { success = true, output = table.concat(all_output, "\n") }
        end

        -- Process drop commands FIRST (removes from working memory by ID)
        for _, cmd in ipairs(drop_commands) do
            local id = cmd:match("^drop%s+(%S+)")
            if id then
                for i = #working_memory, 1, -1 do
                    if working_memory[i].id == id then
                        table.remove(working_memory, i)
                        print("[agent] Dropped: " .. id)
                        break
                    end
                end
            end
        end

        -- Process forget commands (removes notes matching pattern)
        for _, cmd in ipairs(forget_commands) do
            local pattern = cmd:match("^forget%s+(.+)")
            if pattern then
                local removed = {}
                for i = #working_memory, 1, -1 do
                    if working_memory[i].type == "note" and working_memory[i].content:find(pattern, 1, true) then
                        table.insert(removed, working_memory[i])
                        table.remove(working_memory, i)
                    end
                end
                if #removed > 0 then
                    print("[agent] Forgot " .. #removed .. " note(s) matching '" .. pattern .. "':")
                    for _, item in ipairs(removed) do
                        print("  - [" .. item.id .. "] " .. item.content)
                    end
                else
                    print("[agent] No notes matching: " .. pattern)
                end
            end
        end

        -- Process keep commands (refers to previous turn's outputs)
        local prev_outputs = current_outputs or {}
        for _, cmd in ipairs(keep_commands) do
            local indices = M.parse_keep(cmd, #prev_outputs)
            for _, idx in ipairs(indices) do
                local out = prev_outputs[idx]
                if out then
                    table.insert(working_memory, {
                        type = "output",
                        id = M.gen_id(),
                        cmd = out.cmd,
                        content = out.content,
                        success = out.success
                    })
                    print("[agent] Kept output " .. idx)
                end
            end
        end

        -- Process note commands
        for _, cmd in ipairs(note_commands) do
            local fact = cmd:match("^note (.+)")
            if fact then
                table.insert(working_memory, {type = "note", id = M.gen_id(), content = fact})
                print("[agent] Noted: " .. fact)
            end
        end

        -- Process memorize commands (long-term, version-controlled)
        for _, cmd in ipairs(memorize_commands) do
            local fact = cmd:match("^memorize (.+)")
            if fact then
                local ok, err = M.memorize(fact)
                if ok then
                    print("[agent] Memorized: " .. fact)
                else
                    print("[agent] Failed to memorize: " .. (err or "unknown error"))
                end
            end
        end

        -- Process checkpoint command (saves session state and exits)
        if checkpoint_cmd then
            local args = checkpoint_cmd:match("^checkpoint%s*(.*)")
            local progress, open_questions = "", ""
            if args then
                -- Parse "progress | open questions" format
                local p, q = args:match("^(.-)%s*|%s*(.*)$")
                if p then
                    progress = p
                    open_questions = q
                else
                    progress = args
                end
            end

            local state = {
                task = task,
                turn = turn,
                working_memory = working_memory,
                progress = progress,
                open_questions = open_questions
            }

            local saved_id, err = M.save_checkpoint(session_id, state)
            if saved_id then
                print("[agent] Session checkpointed: " .. saved_id)
                print("[agent] Resume with: moss @agent --resume " .. saved_id)
                if progress ~= "" then
                    print("[agent] Progress: " .. progress)
                end
                if open_questions ~= "" then
                    print("[agent] Open questions: " .. open_questions)
                end
            else
                print("[agent] Failed to checkpoint: " .. (err or "unknown error"))
            end

            if session_log then
                session_log:log("checkpoint", { progress = progress, open_questions = open_questions })
                session_log:close()
            end
            return { success = true, output = table.concat(all_output, "\n"), session_id = session_id, checkpointed = true }
        end

        -- Clear for this turn's outputs
        current_outputs = {}

        -- Execute commands
        for _, cmd in ipairs(exec_commands) do
            -- Snapshot before edits (including batch-edit)
            if (cmd:match("^edit") or cmd:match("^batch%-edit")) and shadow_ok then
                pcall(function() shadow.snapshot({}) end)
            end

            -- Handle ask specially - read from user
            local result
            if cmd:match("^ask ") then
                local question = cmd:match("^ask (.+)")
                if non_interactive then
                    -- In non-interactive mode, log the question and return a special response
                    print("[agent] BLOCKED: " .. question .. " (non-interactive mode)")
                    result = { output = "ERROR: Cannot ask user in non-interactive mode. Question was: " .. question, success = false }
                    -- Log the block for analysis
                    if session_log then
                        session_log:log("blocked_ask", { question = question })
                    end
                else
                    io.write("[agent] " .. question .. "\n> ")
                    io.flush()
                    local answer = io.read("*l") or ""
                    result = { output = "User: " .. answer, success = true }
                end
            elseif cmd:match("^batch%-edit ") then
                -- Parse and execute batch edit via edit.batch()
                local edits_str = cmd:match("^batch%-edit (.+)")
                print("[agent] Batch editing: " .. edits_str:sub(1, 60) .. (edits_str:len() > 60 and "..." or ""))
                result = M.execute_batch_edit(edits_str)
            elseif cmd:match("^run ") then
                -- Execute raw shell command
                local raw_cmd = cmd:match("^run (.+)")
                print("[agent] Running: " .. raw_cmd)
                result = shell(raw_cmd)
            else
                -- Execute command via moss
                print("[agent] Running: " .. cmd)
                result = shell(_moss_bin .. " " .. cmd)
            end

            -- Auto-validation after successful edits
            if validate_cmd and result.success and (cmd:match("^edit") or cmd:match("^batch%-edit")) then
                print("[agent] Validating: " .. validate_cmd)
                local validation = shell(validate_cmd)
                if not validation.success then
                    print("[agent] Validation FAILED - rolling back")
                    -- Rollback to last snapshot
                    if shadow_ok then
                        local snapshots = shadow.list()
                        if #snapshots > 1 then
                            shadow.restore(snapshots[#snapshots - 1].id)
                            print("[agent] Rolled back to previous state")
                        end
                    end
                    -- Override result to show validation failure
                    result = {
                        success = false,
                        output = "Edit applied but validation failed (auto-rollback):\n" .. (validation.output or "")
                    }
                else
                    print("[agent] Validation passed ✓")
                    result.output = (result.output or "") .. "\nValidation passed: " .. validate_cmd
                end
            end

            -- Log to file if debug mode
            if log_file then
                log_file:write("\n--- " .. cmd .. " ---\n")
                log_file:write(result.output)
                log_file:write("\n")
                log_file:flush()
            end

            -- Truncate large outputs to prevent context bloat
            local content = result.output or ""
            local MAX_OUTPUT = 10000  -- ~10KB per command output
            if #content > MAX_OUTPUT then
                content = content:sub(1, MAX_OUTPUT)
                content = content .. "\n... [OUTPUT TRUNCATED - " .. (#(result.output or "") - MAX_OUTPUT) .. " more bytes]\n"
            end
            table.insert(current_outputs, {
                cmd = cmd,
                content = content,
                success = result.success
            })

            -- Log command execution
            if session_log then
                session_log:log("command", {
                    turn = turn,
                    cmd = cmd,
                    success = result.success,
                    output = result.output
                })
            end

            -- Track for loop detection
            table.insert(recent_cmds, cmd)
            if #recent_cmds > 10 then
                table.remove(recent_cmds, 1)
            end

            -- Error escalation tracking
            if not result.success then
                -- Check if this is a validation-like command (run, edit, batch-edit)
                local is_validation = cmd:match("^run ") or cmd:match("^edit") or cmd:match("^batch%-edit")
                if is_validation then
                    if error_state and error_state.cmd == cmd then
                        -- Same command failed again, increment retries
                        error_state.retries = error_state.retries + 1
                        error_state.last_error = result.output:sub(1, 500)
                    else
                        -- New error, start tracking
                        error_state = {
                            cmd = cmd,
                            retries = 1,
                            rolled_back = false,
                            last_error = result.output:sub(1, 500)
                        }
                    end

                    print(string.format("[agent] Error on '%s' (attempt %d/3)", cmd:sub(1, 40), error_state.retries))

                    -- Escalation logic
                    if error_state.retries >= 3 and not error_state.rolled_back and shadow_ok then
                        print("[agent] Max retries reached, rolling back...")
                        local rollback_ok = pcall(function()
                            local snapshots = shadow.list()
                            if #snapshots > 1 then
                                shadow.restore(snapshots[#snapshots - 1].id)
                            end
                        end)
                        if rollback_ok then
                            error_state.rolled_back = true
                            print("[agent] Rolled back to pre-edit state")
                            -- Add note about rollback
                            table.insert(working_memory, {
                                type = "note",
                                id = M.gen_id(),
                                content = "ROLLBACK: " .. cmd .. " failed 3 times, reverted changes"
                            })
                        end
                    end
                end
            else
                -- Command succeeded, clear error state if it was for this command
                if error_state and error_state.cmd == cmd then
                    print("[agent] Error resolved for: " .. cmd:sub(1, 40))
                    error_state = nil
                end
            end
        end

        -- If done was requested along with commands, return after executing them
        if done_summary then
            print("[agent] Done: " .. done_summary)
            if session_log then
                session_log:log("done", { summary = done_summary:sub(1, 200) })
                session_log:close()
            end
            if total_retries > 0 then
                print("[agent] API retries: " .. total_retries)
            end
            return { success = true, output = table.concat(all_output, "\n") }
        end
    end

    -- Auto-checkpoint on max turns reached
    print("[agent] Max turns reached, auto-checkpointing...")
    local state = {
        task = task,
        turn = max_turns,
        working_memory = working_memory,
        progress = "Session ended at max turns",
        open_questions = "Review working memory for context"
    }
    local saved_id, err = M.save_checkpoint(session_id, state)
    if saved_id then
        print("[agent] Session auto-checkpointed: " .. saved_id)
        print("[agent] Resume with: moss @agent --resume " .. saved_id)
    else
        print("[agent] Warning: Failed to auto-checkpoint: " .. (err or "unknown error"))
    end

    if session_log then
        session_log:log("max_turns_reached", { turn = max_turns })
        session_log:close()
    end

    if total_retries > 0 then
        print("[agent] API retries: " .. total_retries)
    end
    return { success = false, output = table.concat(all_output, "\n"), session_id = session_id }
end

-- CLI argument parsing (delegated to agent.parser module)
M.parse_args = parser.parse_args

-- When run as script (moss @agent), execute directly
-- When required as module, return M
if args and #args >= 0 then
    local opts = M.parse_args(args)

    -- Handle --list-sessions
    if opts.list_sessions then
        local sessions = M.list_sessions()
        if #sessions == 0 then
            print("No saved sessions found.")
        else
            print("Available sessions (checkpoints):")
            for _, id in ipairs(sessions) do
                local state = M.load_checkpoint(id)
                if state then
                    local task_preview = (state.task or ""):sub(1, 50)
                    if #(state.task or "") > 50 then task_preview = task_preview .. "..." end
                    print(string.format("  %s  turn %d  %s", id, state.turn or 0, task_preview))
                else
                    print(string.format("  %s  (failed to load)", id))
                end
            end
        end
        os.exit(0)
    end

    -- Handle --list-logs
    if opts.list_logs then
        local logs = M.list_logs()
        if #logs == 0 then
            print("No session logs found.")
        else
            print("Available session logs:")
            for _, id in ipairs(logs) do
                local log_path = _moss_root .. "/.moss/agent/logs/" .. id .. ".jsonl"
                local handle = io.popen("wc -l < " .. log_path .. " 2>/dev/null")
                local line_count = handle and handle:read("*n") or 0
                if handle then handle:close() end
                print(string.format("  %s  (%d events)", id, line_count))
            end
            print("\nView with: cat .moss/agent/logs/<session-id>.jsonl | jq")
        end
        os.exit(0)
    end

    -- Handle --roles
    if opts.list_roles then
        print("Available roles:")
        print("  investigator  (default) Answer questions about the codebase")
        print("  auditor       Find issues: security, quality, patterns")
        print("  refactorer    Make code changes with validation")
        print("")
        print("Usage:")
        print("  moss @agent --v2 'how does X work?'")
        print("  moss @agent --audit 'find unwrap on user input'")
        print("  moss @agent --refactor 'rename foo to bar'")
        print("  moss @agent --refactor --shadow 'rename foo to bar'  # safe editing via shadow worktree")
        print("  moss @agent --auto 'task'  # LLM picks the role")
        os.exit(0)
    end

    local result
    if opts.v2 then
        result = M.run_state_machine(opts)
    else
        result = M.run(opts)
    end
    if not result.success then
        os.exit(1)
    end
else
    return M
end
