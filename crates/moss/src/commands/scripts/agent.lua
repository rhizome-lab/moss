-- Agent module: autonomous task execution with moss tools
local M = {}

-- Characters for random IDs (no i,l,o,1,0 to avoid visual confusion)
local ID_CHARS = "abcdefghjkmnpqrstuvwxyz23456789"
local ID_LEN = 4
local SESSION_ID_LEN = 8

-- Seed random on load
math.randomseed(os.time())

-- Generate random short IDs for memory items (avoid sequential to prevent LLM confusion)
function M.gen_id()
    local id = ""
    for _ = 1, ID_LEN do
        local idx = math.random(1, #ID_CHARS)
        id = id .. ID_CHARS:sub(idx, idx)
    end
    return id
end

-- Generate longer session IDs for uniqueness across sessions
function M.gen_session_id()
    local id = ""
    for _ = 1, SESSION_ID_LEN do
        local idx = math.random(1, #ID_CHARS)
        id = id .. ID_CHARS:sub(idx, idx)
    end
    return id
end

-- Session checkpoint directory
local function get_session_dir()
    return _moss_root .. "/.moss/agent"
end

-- Session log directory (for full session recording/replay)
local function get_log_dir()
    return _moss_root .. "/.moss/agent/logs"
end

-- Start recording a session log
-- Returns a logger object with :log(event, data) method
function M.start_session_log(session_id)
    local log_dir = get_log_dir()
    os.execute("mkdir -p " .. log_dir)

    local log_file_path = log_dir .. "/" .. session_id .. ".jsonl"
    local log_file = io.open(log_file_path, "w")
    if not log_file then
        return nil
    end

    local logger = {
        file = log_file,
        session_id = session_id,
        start_time = os.time(),
        turn_count = 0
    }

    -- Write session start event
    logger.file:write(M.json_log_entry("session_start", {
        session_id = session_id,
        timestamp = os.date("!%Y-%m-%dT%H:%M:%SZ"),
        moss_root = _moss_root
    }) .. "\n")
    logger.file:flush()

    function logger:log(event, data)
        self.file:write(M.json_log_entry(event, data) .. "\n")
        self.file:flush()
    end

    function logger:close()
        self:log("session_end", {
            duration_seconds = os.time() - self.start_time,
            total_turns = self.turn_count
        })
        self.file:close()
    end

    return logger
end

-- Format a log entry as JSON
function M.json_log_entry(event, data)
    local parts = {"{"}
    table.insert(parts, string.format('"event": "%s",', event))
    table.insert(parts, string.format('"timestamp": "%s"', os.date("!%Y-%m-%dT%H:%M:%SZ")))

    if data then
        for key, value in pairs(data) do
            if type(value) == "string" then
                table.insert(parts, string.format(', "%s": %s', key, M.json_encode_string(value)))
            elseif type(value) == "number" then
                table.insert(parts, string.format(', "%s": %s', key, tostring(value)))
            elseif type(value) == "boolean" then
                table.insert(parts, string.format(', "%s": %s', key, tostring(value)))
            elseif type(value) == "table" then
                -- Simple array/object serialization for logs
                local json_arr = {}
                for _, v in ipairs(value) do
                    if type(v) == "string" then
                        table.insert(json_arr, M.json_encode_string(v))
                    else
                        table.insert(json_arr, tostring(v))
                    end
                end
                table.insert(parts, string.format(', "%s": [%s]', key, table.concat(json_arr, ", ")))
            end
        end
    end

    table.insert(parts, "}")
    return table.concat(parts, "")
end

-- List available session logs
function M.list_logs()
    local logs = {}
    local log_dir = get_log_dir()
    local handle = io.popen("ls -t " .. log_dir .. "/*.jsonl 2>/dev/null")
    if handle then
        for line in handle:lines() do
            local id = line:match("/([^/]+)%.jsonl$")
            if id then
                table.insert(logs, id)
            end
        end
        handle:close()
    end
    return logs
end

-- Save session checkpoint to JSON file
-- Returns: session_id on success, nil + error on failure
function M.save_checkpoint(session_id, state)
    local session_dir = get_session_dir()
    os.execute("mkdir -p " .. session_dir)

    local checkpoint_file = session_dir .. "/session-" .. session_id .. ".json"
    local file, err = io.open(checkpoint_file, "w")
    if not file then
        return nil, err
    end

    -- Simple JSON serialization (working memory, task, progress)
    local json_parts = {"{"}
    table.insert(json_parts, string.format('"session_id": "%s",', session_id))
    table.insert(json_parts, string.format('"task": %s,', M.json_encode_string(state.task)))
    table.insert(json_parts, string.format('"turn": %d,', state.turn))
    table.insert(json_parts, string.format('"timestamp": "%s",', os.date("%Y-%m-%dT%H:%M:%S")))

    -- Serialize working memory
    table.insert(json_parts, '"working_memory": [')
    for i, item in ipairs(state.working_memory) do
        local item_json = string.format(
            '{"type": "%s", "id": "%s", "content": %s%s}',
            item.type,
            item.id,
            M.json_encode_string(item.content),
            item.cmd and string.format(', "cmd": %s, "success": %s', M.json_encode_string(item.cmd), tostring(item.success)) or ""
        )
        table.insert(json_parts, item_json)
        if i < #state.working_memory then
            table.insert(json_parts, ",")
        end
    end
    table.insert(json_parts, "],")

    -- Serialize progress summary
    table.insert(json_parts, string.format('"progress": %s,', M.json_encode_string(state.progress or "")))
    table.insert(json_parts, string.format('"open_questions": %s', M.json_encode_string(state.open_questions or "")))
    table.insert(json_parts, "}")

    file:write(table.concat(json_parts, "\n"))
    file:close()

    return session_id
end

-- JSON string encoding (handles escapes)
function M.json_encode_string(s)
    if s == nil then return "null" end
    s = tostring(s)
    s = s:gsub('\\', '\\\\')
    s = s:gsub('"', '\\"')
    s = s:gsub('\n', '\\n')
    s = s:gsub('\r', '\\r')
    s = s:gsub('\t', '\\t')
    return '"' .. s .. '"'
end

-- Load session checkpoint from JSON file
-- Returns: state table on success, nil + error on failure
function M.load_checkpoint(session_id)
    local checkpoint_file = get_session_dir() .. "/session-" .. session_id .. ".json"
    local file, err = io.open(checkpoint_file, "r")
    if not file then
        return nil, "Session not found: " .. session_id
    end

    local content = file:read("*a")
    file:close()

    -- Parse JSON (simple parser for our known structure)
    local state = M.parse_checkpoint_json(content)
    if not state then
        return nil, "Failed to parse checkpoint"
    end

    return state
end

-- Simple JSON parser for checkpoint format
function M.parse_checkpoint_json(json)
    local state = {working_memory = {}}

    -- Extract simple fields
    state.session_id = json:match('"session_id":%s*"([^"]*)"')
    state.task = M.json_decode_string(json:match('"task":%s*(".-[^\\]")'))
    state.turn = tonumber(json:match('"turn":%s*(%d+)'))
    state.progress = M.json_decode_string(json:match('"progress":%s*(".-[^\\]")'))
    state.open_questions = M.json_decode_string(json:match('"open_questions":%s*(".-[^\\]")'))

    -- Extract working memory items
    local wm_json = json:match('"working_memory":%s*%[(.-)%]')
    if wm_json then
        for item_json in wm_json:gmatch('{([^}]+)}') do
            local item = {}
            item.type = item_json:match('"type":%s*"([^"]*)"')
            item.id = item_json:match('"id":%s*"([^"]*)"')
            item.content = M.json_decode_string(item_json:match('"content":%s*(".-[^\\]")'))
            local cmd = item_json:match('"cmd":%s*(".-[^\\]")')
            if cmd then
                item.cmd = M.json_decode_string(cmd)
                item.success = item_json:match('"success":%s*true') ~= nil
            end
            table.insert(state.working_memory, item)
        end
    end

    return state
end

-- JSON string decoding
function M.json_decode_string(s)
    if not s or s == "null" then return nil end
    -- Remove surrounding quotes
    s = s:match('^"(.*)"$') or s
    s = s:gsub('\\n', '\n')
    s = s:gsub('\\r', '\r')
    s = s:gsub('\\t', '\t')
    s = s:gsub('\\"', '"')
    s = s:gsub('\\\\', '\\')
    return s
end

-- List available session checkpoints
function M.list_sessions()
    local sessions = {}
    local session_dir = get_session_dir()
    local handle = io.popen("ls -t " .. session_dir .. "/session-*.json 2>/dev/null")
    if handle then
        for line in handle:lines() do
            local id = line:match("session%-(.+)%.json$")
            if id then
                table.insert(sessions, id)
            end
        end
        handle:close()
    end
    return sessions
end

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

-- Execute batch edit from agent command string
-- Format: "target1 action1 content1 | target2 action2 content2"
-- Actions: delete, replace, insert
function M.execute_batch_edit(edits_str)
    local edits = {}
    local outputs = {}

    -- Split by | and parse each edit
    for edit_part in (edits_str .. "|"):gmatch("(.-)%s*|%s*") do
        edit_part = edit_part:match("^%s*(.-)%s*$")  -- trim
        if edit_part ~= "" then
            -- Parse: target action [content]
            local target, rest = edit_part:match("^(%S+)%s+(.+)$")
            if target then
                local action, content = rest:match("^(%S+)%s*(.*)$")
                if action then
                    local edit = { target = target, action = action }
                    if content and content ~= "" then
                        edit.content = content
                    end
                    table.insert(edits, edit)
                end
            end
        end
    end

    if #edits == 0 then
        return { output = "No valid edits parsed", success = false }
    end

    -- Try to use edit.batch if available (Lua runtime context)
    if edit and edit.batch then
        local ok, result = pcall(function()
            return edit.batch(edits, { message = "Agent batch edit" })
        end)
        if ok and result then
            if result.success then
                table.insert(outputs, string.format("Batch edit applied: %d edits", result.edits_applied or #edits))
                if result.files_modified then
                    for _, f in ipairs(result.files_modified) do
                        table.insert(outputs, "  Modified: " .. f)
                    end
                end
                return { output = table.concat(outputs, "\n"), success = true }
            else
                return { output = result.error or "Batch edit failed", success = false }
            end
        end
    end

    -- Fallback: generate JSON and pipe to moss edit --batch -
    local json_edits = {}
    for _, e in ipairs(edits) do
        local json_edit = string.format('{"target": %s, "action": "%s"',
            M.json_encode_string(e.target), e.action)
        if e.content then
            json_edit = json_edit .. string.format(', "content": %s', M.json_encode_string(e.content))
        end
        json_edit = json_edit .. "}"
        table.insert(json_edits, json_edit)
    end
    local json = '{"edits": [' .. table.concat(json_edits, ", ") .. ']}'

    -- Execute via moss CLI
    local cmd = string.format("echo '%s' | ./target/debug/moss edit --batch -", json:gsub("'", "'\\''"))
    return shell(cmd)
end

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
You are an INVESTIGATOR exploring a codebase. Suggest commands to gather information.

Commands:
$(view path) - file structure/symbols
$(view path:start-end) - specific lines
$(text-search "pattern") - search codebase
$(run cmd) - shell command

Output commands directly. Do NOT answer the question - that's the evaluator's job.
Example: $(view src/main.rs) $(text-search "config")
]],

    evaluator = [[
You are an EVALUATOR, not an explorer. Your ONLY job is to judge what we found.

RULES:
1. NEVER output commands (not even in backticks like `view` or `text-search`)
2. NEVER say "I need to", "Let me", or "I will" - those are explorer phrases
3. You MUST either $(answer) or explain what specific info is missing

If results contain the answer: $(answer The complete answer here)
If results are partial: $(note what we found) then explain what's still needed
If results are irrelevant: explain what went wrong

Memory commands: $(keep 1 3), $(keep), $(drop 2), $(note finding)

Example good response:
"The search found `support_for_extension` in registry.rs which maps file extensions to languages.
$(note Language detection uses support_for_extension() in moss-languages/registry.rs)
$(answer moss detects language by file extension via support_for_extension() in the moss-languages registry)"

Example BAD response (DO NOT DO THIS):
"I need to understand more. Let me look at `view registry.rs`" ← WRONG, you're exploring!
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
You are an AUDITOR systematically checking for issues. Run commands to find problems.

Commands:
$(view path) - examine code structure
$(view path:start-end) - inspect specific lines
$(text-search "pattern") - find problematic patterns
$(run cmd) - run analysis tools

Focus on finding concrete issues with file:line locations.
Do NOT conclude yet - that's the evaluator's job.
Example: $(text-search "unwrap()") $(text-search "panic!")
]],

    evaluator = [[
You are an AUDIT EVALUATOR. Assess the findings from exploration.

RULES:
1. NEVER output commands - you evaluate, you don't explore
2. NEVER say "I need to check" or "Let me look" - those are auditor phrases
3. You MUST either $(answer) with findings or explain what areas remain unchecked

For each issue found, note:
- Location (file:line)
- Issue type (security/quality/pattern)
- Severity (critical/high/medium/low)
- Brief description

If audit is complete: $(answer with formatted findings)
If more areas to check: $(note findings so far) then explain what's left

Memory commands: $(keep 1 3), $(keep), $(drop 2), $(note finding)

Example finding format:
$(note SECURITY:HIGH commands/run.rs:45 - unsanitized shell input)
$(note QUALITY:MED lib/parse.rs:120 - unwrap() on user input)

When done:
$(answer
## Audit Findings

### Critical
- None found

### High
- commands/run.rs:45 - SECURITY: unsanitized shell input passed to Command::new()

### Medium
- lib/parse.rs:120 - QUALITY: unwrap() on Result from user-provided data
)
]],
}

-- Role registry
local ROLE_PROMPTS = {
    investigator = INVESTIGATOR_PROMPTS,
    auditor = AUDITOR_PROMPTS,
}

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

-- Build context for planner state (task only)
function M.build_planner_context(task)
    return "**Task:** " .. task .. "\n\nCreate a plan to accomplish this task."
end

-- Build context for explorer state (ephemeral - last outputs only)
function M.build_explorer_context(task, last_outputs, notes, plan)
    local parts = {"**Task:** " .. task}

    if plan then
        table.insert(parts, "\n**Plan:**\n" .. plan)
    end

    if #notes > 0 then
        table.insert(parts, "\n**Notes so far:**")
        for _, note in ipairs(notes) do
            table.insert(parts, "- " .. note)
        end
    end

    if last_outputs and #last_outputs > 0 then
        table.insert(parts, "\n**Last results:**")
        for _, out in ipairs(last_outputs) do
            local status = out.success and "" or " (failed)"
            table.insert(parts, string.format("\n`%s`%s\n```\n%s\n```", out.cmd, status, out.content))
        end
    end

    table.insert(parts, "\nSuggest commands to explore. Do not conclude yet.")
    return table.concat(parts, "\n")
end

-- Build context for evaluator state (working memory + pending outputs)
function M.build_evaluator_context(task, working_memory, last_outputs, notes)
    local parts = {"**Task:** " .. task}

    -- Check for failures in new results
    local has_failures = false
    for _, out in ipairs(last_outputs) do
        if not out.success then
            has_failures = true
            break
        end
    end

    if has_failures then
        table.insert(parts, "\n**WARNING: Some commands failed.** Consider alternative approaches.")
    end

    if #notes > 0 then
        table.insert(parts, "\n**Notes:**")
        for _, note in ipairs(notes) do
            table.insert(parts, "- " .. note)
        end
    end

    if #working_memory > 0 then
        table.insert(parts, "\n**Working memory** (kept from previous turns):")
        for i, item in ipairs(working_memory) do
            local status = item.success and "" or " (FAILED)"
            table.insert(parts, string.format("\n[%d] `%s`%s\n```\n%s\n```", i, item.cmd, status, item.content))
        end
    end

    if #last_outputs > 0 then
        table.insert(parts, "\n**New results** (will be discarded unless you keep them):")
        for i, out in ipairs(last_outputs) do
            local status = out.success and "" or " (FAILED)"
            table.insert(parts, string.format("\n[%d] `%s`%s\n```\n%s\n```", i, out.cmd, status, out.content))
        end
    end

    return table.concat(parts, "\n")
end

-- State machine agent runner (v2)
function M.run_state_machine(opts)
    opts = opts or {}
    local task = opts.prompt or opts.task or "Help with this codebase"
    local max_turns = opts.max_turns or 10
    local provider = opts.provider or "gemini"
    local model = opts.model  -- nil means use provider default
    local use_planner = opts.plan or false
    local role = opts.role or "investigator"

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

        -- Parse commands from response
        local commands = {}
        for cmd_content in response:gmatch('%$%(([^%)]+)%)') do
            local cmd_name, args = cmd_content:match('^(%S+)%s*(.*)$')
            if cmd_name then
                table.insert(commands, {name = cmd_name, args = args or "", full = cmd_name .. " " .. (args or "")})
            end
        end

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
                           cmd.name == "analyze" or cmd.name == "package" then
                        print("[agent-v2] Running: " .. cmd.full)
                        result = shell("./target/debug/moss " .. cmd.full)
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

-- Build error escalation context
-- error_state: {cmd, retries, rolled_back, last_error}
function M.build_error_context(error_state)
    if not error_state then return "" end

    local parts = {"\n[error recovery]"}
    table.insert(parts, string.format("Command '%s' has failed %d time(s).", error_state.cmd, error_state.retries))

    if error_state.last_error then
        table.insert(parts, "Last error: " .. error_state.last_error:sub(1, 200))
    end

    if error_state.rolled_back then
        table.insert(parts, "Status: Already rolled back to pre-edit state.")
        table.insert(parts, "Options:")
        table.insert(parts, "  1. $(ask <question>) - Get user guidance")
        table.insert(parts, "  2. Try a different approach")
        table.insert(parts, "  3. $(done BLOCKED: <explanation>) - Give up with explanation")
    elseif error_state.retries >= 3 then
        table.insert(parts, "Status: Max retries reached, will rollback on next failure.")
        table.insert(parts, "Consider: Try a different approach or $(ask) for help.")
    else
        table.insert(parts, string.format("Status: Retry %d/3", error_state.retries))
    end

    table.insert(parts, "[/error recovery]")
    return table.concat(parts, "\n")
end

-- Build context from working memory
-- working_memory: list of {type="output"|"note", cmd?, content, id}
-- error_state: optional {cmd, retries, rolled_back, last_error}
function M.build_context(task, working_memory, current_outputs, error_state)
    local parts = {"**Task:** " .. task}

    -- Add error recovery context if in error state
    if error_state then
        table.insert(parts, M.build_error_context(error_state))
    end

    -- Add working memory (kept outputs and notes) - markdown format
    if #working_memory > 0 then
        table.insert(parts, "\n**Saved:**")
        for _, item in ipairs(working_memory) do
            if item.type == "note" then
                table.insert(parts, string.format("- [%s] %s", item.id, item.content))
            else
                local status = item.success and "" or " (failed)"
                table.insert(parts, string.format("\n`[%s] %s`%s\n```\n%s\n```", item.id, item.cmd, status, item.content))
            end
        end
    end

    -- Add current turn outputs (ephemeral) - markdown format
    if current_outputs and #current_outputs > 0 then
        table.insert(parts, "\n**Results:**")
        for i, out in ipairs(current_outputs) do
            local status = out.success and "" or " (failed)"
            table.insert(parts, string.format("\n`[%d] %s`%s\n```\n%s\n```", i, out.cmd, status, out.content))
        end
        table.insert(parts, "\n*Results disappear next turn.* Use $(keep), $(note), or $(done ANSWER).")
    end

    return table.concat(parts, "\n")
end

-- Parse keep command: "keep" | "keep all" | "keep 1" | "keep 1 2 3"
function M.parse_keep(cmd, num_outputs)
    local indices = {}
    local args = cmd:match("^keep%s*(.*)")
    if not args or args == "" or args == "all" then
        -- keep all
        for i = 1, num_outputs do
            table.insert(indices, i)
        end
    else
        -- keep specific indices
        for idx in args:gmatch("%d+") do
            local n = tonumber(idx)
            if n and n >= 1 and n <= num_outputs then
                table.insert(indices, n)
            end
        end
    end
    return indices
end

-- Main agent loop
function M.run(opts)
    opts = opts or {}
    local task = opts.prompt or opts.task or "Help with this codebase"
    local max_turns = opts.max_turns or 15
    local provider = opts.provider or "gemini"
    local model = opts.model
    local session_id = opts.resume or M.gen_session_id()
    local start_turn = 1
    local non_interactive = opts.non_interactive or false

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

    -- Start session logging (always enabled for analysis)
    local session_log = M.start_session_log(session_id)
    if session_log then
        session_log:log("task", {
            system_prompt = SYSTEM_PROMPT,
            user_prompt = task,
            provider = provider,
            model = model or "default",
            max_turns = max_turns,
            resumed = opts.resume ~= nil
        })
    end

    -- Build task description
    local task_desc = task
    if opts.explain then
        task_desc = task_desc .. "\nIMPORTANT: Your final answer MUST end with '## Steps' listing each command you ran and why it was needed."
    end
    task_desc = task_desc .. "\nDirectory: " .. _moss_root

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

        -- Extract commands from response
        local commands = {}

        -- Parse $(cmd args) format - the primary format we teach via bootstrap
        for cmd_content in response:gmatch('%$%(([^%)]+)%)') do
            local cmd_name, args = cmd_content:match('^(%S+)%s*(.*)$')
            if cmd_name then
                args = args or ""
                if cmd_name == "view" or cmd_name == "text-search" or cmd_name == "run" or
                   cmd_name == "note" or cmd_name == "done" or cmd_name == "keep" or
                   cmd_name == "drop" or cmd_name == "memorize" or cmd_name == "forget" or
                   cmd_name == "analyze" or cmd_name == "package" or cmd_name == "edit" or
                   cmd_name == "batch-edit" or cmd_name == "checkpoint" or cmd_name == "ask" or
                   cmd_name == "wait" or cmd_name == "help" then
                    table.insert(commands, cmd_name .. " " .. args)
                end
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
                result = shell("./target/debug/moss " .. cmd)
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

-- Parse CLI args
function M.parse_args(args)
    local opts = {}
    local task_parts = {}
    local i = 1
    while i <= #args do
        local arg = args[i]
        if arg == "--provider" and args[i+1] then
            opts.provider = args[i+1]
            i = i + 2
        elseif arg == "--model" and args[i+1] then
            opts.model = args[i+1]
            i = i + 2
        elseif arg == "--max-turns" and args[i+1] then
            opts.max_turns = tonumber(args[i+1])
            i = i + 2
        elseif arg == "--explain" then
            opts.explain = true
            i = i + 1
        elseif arg == "--resume" and args[i+1] then
            opts.resume = args[i+1]
            i = i + 2
        elseif arg == "--list-sessions" then
            opts.list_sessions = true
            i = i + 1
        elseif arg == "--list-logs" then
            opts.list_logs = true
            i = i + 1
        elseif arg == "--non-interactive" or arg == "-n" then
            opts.non_interactive = true
            i = i + 1
        elseif arg == "--v2" or arg == "--state-machine" then
            opts.v2 = true
            i = i + 1
        elseif arg == "--plan" then
            opts.plan = true
            i = i + 1
        elseif arg == "--role" and args[i+1] then
            opts.role = args[i+1]
            i = i + 2
        elseif arg == "--audit" then
            opts.role = "auditor"
            opts.v2 = true  -- auditor requires v2
            i = i + 1
        elseif arg == "--roles" then
            opts.list_roles = true
            i = i + 1
        else
            table.insert(task_parts, arg)
            i = i + 1
        end
    end
    opts.prompt = table.concat(task_parts, " ")
    if opts.prompt == "" then opts.prompt = nil end
    return opts
end

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
        print("")
        print("Usage:")
        print("  moss @agent --v2 --role auditor 'find security issues'")
        print("  moss @agent --audit 'check for unwrap on user input'")
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
