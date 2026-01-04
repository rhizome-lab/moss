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
Respond with your next command.

$(view <path>) $(view <path/Symbol>) $(view --types-only <path>) $(view --deps <path>)
$(text-search "<pattern>") $(text-search "<pattern>" --only <glob>)
$(analyze complexity) $(analyze callers <symbol>) $(analyze callees <symbol>)
$(package list) $(package tree) $(package outdated) $(package audit)
$(edit <target> delete|replace|insert <code>)
$(run <shell command>)
$(note <finding>) $(keep) $(keep 1 3) $(drop <id>)
$(checkpoint <progress> | <questions>)
$(ask <question>)
$(done <answer>)
]]

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
    local parts = {"Task: " .. task}

    -- Add error recovery context if in error state
    if error_state then
        table.insert(parts, M.build_error_context(error_state))
    end

    -- Add working memory (kept outputs and notes) - stable IDs for $(drop ID)
    if #working_memory > 0 then
        table.insert(parts, "\n[working memory]")
        for _, item in ipairs(working_memory) do
            if item.type == "note" then
                table.insert(parts, string.format("[%s] note: %s", item.id, item.content))
            else
                local header = string.format("[%s] $ %s", item.id, item.cmd)
                if not item.success then header = header .. " (failed)" end
                table.insert(parts, header .. "\n" .. item.content)
            end
        end
        table.insert(parts, "[/working memory]")
    end

    -- Add current turn outputs (ephemeral, indexed)
    if current_outputs and #current_outputs > 0 then
        table.insert(parts, "\n[outputs]")
        for i, out in ipairs(current_outputs) do
            local header = string.format("[%d] $ %s", i, out.cmd)
            if not out.success then header = header .. " (failed)" end
            table.insert(parts, header .. "\n" .. out.content)
        end
        table.insert(parts, "[/outputs]")
        -- Post-history reminder
        table.insert(parts, "\n$(done ANSWER) if ready, otherwise $(note) what you learned and continue.")
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
        local response
        local max_retries = 3
        for attempt = 1, max_retries do
            local ok, result = pcall(function()
                return llm.chat(provider, model, SYSTEM_PROMPT, prompt, {})
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

        -- Extract prose commands from response
        local commands = {}
        local has_next_turn = response:match("next turn:") ~= nil

        -- Parse "I want to view X" -> "view X"
        for target in response:gmatch("I want to view ([^\n]+)") do
            target = target:gsub("^%s+", ""):gsub("%s+$", "")
            if target:match("^only types in") then
                table.insert(commands, "view --types-only " .. target:match("^only types in (.+)"))
            elseif target:match("^dependencies of") then
                table.insert(commands, "view --deps " .. target:match("^dependencies of (.+)"))
            else
                table.insert(commands, "view " .. target)
            end
        end

        -- Parse "I want to search for "X"" or "I want to search for "X" only in Y"
        for pattern, rest in response:gmatch('I want to search for "([^"]+)"([^\n]*)') do
            local only = rest:match("only in ([^\n]+)")
            if only then
                table.insert(commands, "text-search \"" .. pattern .. "\" --only " .. only:gsub("^%s+", ""):gsub("%s+$", ""))
            else
                table.insert(commands, "text-search \"" .. pattern .. "\"")
            end
        end

        -- Parse "I want to analyze X"
        for what in response:gmatch("I want to analyze ([^\n]+)") do
            what = what:gsub("^%s+", ""):gsub("%s+$", "")
            if what:match("^callers of") then
                table.insert(commands, "analyze callers " .. what:match("^callers of (.+)"))
            elseif what:match("^callees of") then
                table.insert(commands, "analyze callees " .. what:match("^callees of (.+)"))
            else
                table.insert(commands, "analyze " .. what)
            end
        end

        -- Parse "I want to list packages" / "I want to see outdated packages"
        if response:match("I want to list packages") then
            table.insert(commands, "package list")
        end
        if response:match("I want to see outdated packages") then
            table.insert(commands, "package outdated")
        end

        -- Parse "I want to run X"
        for cmd in response:gmatch("I want to run ([^\n]+)") do
            table.insert(commands, "run " .. cmd:gsub("^%s+", ""):gsub("%s+$", ""))
        end

        -- Parse "I want to delete/replace/insert"
        for target in response:gmatch("I want to delete ([^\n]+)") do
            table.insert(commands, "edit " .. target:gsub("^%s+", ""):gsub("%s+$", "") .. " delete")
        end
        for target, code in response:gmatch("I want to replace ([^%s]+) with (.+)") do
            table.insert(commands, "edit " .. target .. " replace " .. code)
        end
        for code, target in response:gmatch("I want to insert (.+) before ([^\n]+)") do
            table.insert(commands, "edit " .. target:gsub("^%s+", ""):gsub("%s+$", "") .. " insert --before " .. code)
        end

        -- Parse "I note: X"
        for finding in response:gmatch("I note: ([^\n]+)") do
            table.insert(commands, "note " .. finding:gsub("^%s+", ""):gsub("%s+$", ""))
        end

        -- Parse "I want to ask the user: X"
        for question in response:gmatch("I want to ask the user: ([^\n]+)") do
            table.insert(commands, "ask " .. question:gsub("^%s+", ""):gsub("%s+$", ""))
        end

        -- Parse "My conclusion is: X"
        for answer in response:gmatch("My conclusion is: ([^\n]+)") do
            table.insert(commands, "done " .. answer:gsub("^%s+", ""):gsub("%s+$", ""))
        end

        -- Fallback: also check for $(cmd) syntax for backwards compat
        for cmd in response:gmatch("%$%((.-)%)") do
            table.insert(commands, cmd)
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

            table.insert(current_outputs, {
                cmd = cmd,
                content = result.output,
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

    local result = M.run(opts)
    if not result.success then
        os.exit(1)
    end
else
    return M
end
