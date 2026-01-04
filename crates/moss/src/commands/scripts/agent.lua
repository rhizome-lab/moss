-- Agent module: autonomous task execution with moss tools
local M = {}

-- Generate random short IDs for memory items (avoid sequential to prevent LLM confusion)
local id_chars = "abcdefghjkmnpqrstuvwxyz23456789"  -- no i,l,o,1,0 to avoid confusion
function M.gen_id()
    local id = ""
    for _ = 1, 4 do
        local idx = math.random(1, #id_chars)
        id = id .. id_chars:sub(idx, idx)
    end
    return id
end

-- Seed random on first load
math.randomseed(os.time())

local SYSTEM_PROMPT = [[
Coding session. Output commands in $(cmd) syntax. Multiple per turn OK.

Command outputs disappear after each turn. To manage context:
- $(keep) or $(keep 1 3) saves outputs to working memory
- $(note key fact here) records insights
- $(drop xk7f) removes item from working memory by ID
- $(forget pattern) removes notes matching pattern
- $(done YOUR FINAL ANSWER) ends the session

$(done The answer is X because Y)
$(keep)
$(note uses clap for CLI)
$(view .)
$(view --types-only .)
$(view --deps .)
$(view src/main.rs)
$(view src/main.rs/main)
$(text-search "pattern")
$(edit src/lib.rs/foo delete|replace|insert|move)
$(package list|tree|info|outdated|audit)
$(analyze complexity|security|callers|callees)
$(run cargo test)
$(ask which module?)
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

-- Build context from working memory
-- working_memory: list of {type="output"|"note", cmd?, content, id}
function M.build_context(task, working_memory, current_outputs)
    local parts = {"Task: " .. task}

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

    -- Build task description
    local task_desc = task
    if opts.explain then
        task_desc = task_desc .. "\nIMPORTANT: Your final answer MUST end with '## Steps' listing each command you ran and why it was needed."
    end
    task_desc = task_desc .. "\nDirectory: " .. _moss_root

    -- Recall relevant memories into working memory
    local working_memory = {}
    local ok, memories = pcall(recall, task, 3)
    if ok and memories and #memories > 0 then
        for _, m in ipairs(memories) do
            table.insert(working_memory, {type = "note", id = M.gen_id(), content = "(recalled) " .. m.content})
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

    -- Open log file if debug mode
    local log_file = nil
    if os.getenv("MOSS_AGENT_DEBUG") then
        log_file = io.open("/tmp/moss-agent.log", "w")
        if log_file then
            log_file:write("=== Agent session: " .. task .. " ===\n\n")
        end
    end

    for turn = 1, max_turns do
        print(string.format("[agent] Turn %d/%d", turn, max_turns))

        -- Build context from working memory + current outputs
        local prompt = M.build_context(task_desc, working_memory, current_outputs)

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

        -- Extract all commands from response: $(cmd here)
        local commands = {}
        for cmd in response:gmatch("%$%((.-)%)") do
            table.insert(commands, cmd)
        end

        if #commands == 0 then
            print("[agent] No commands found, finishing")
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
        local done_summary = nil

        for _, cmd in ipairs(commands) do
            if cmd:match("^done") then
                done_summary = cmd:match("^done%s*(.*)") or ""
            elseif cmd:match("^keep") then
                table.insert(keep_commands, cmd)
            elseif cmd:match("^note ") then
                table.insert(note_commands, cmd)
            elseif cmd:match("^drop ") then
                table.insert(drop_commands, cmd)
            elseif cmd:match("^forget ") then
                table.insert(forget_commands, cmd)
            else
                table.insert(exec_commands, cmd)
            end
        end

        -- If ONLY done (no exec commands), return immediately
        if done_summary and #exec_commands == 0 then
            print("[agent] Done: " .. done_summary)
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
                local removed = 0
                for i = #working_memory, 1, -1 do
                    if working_memory[i].type == "note" and working_memory[i].content:find(pattern, 1, true) then
                        table.remove(working_memory, i)
                        removed = removed + 1
                    end
                end
                print("[agent] Forgot " .. removed .. " note(s) matching: " .. pattern)
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

        -- Clear for this turn's outputs
        current_outputs = {}

        -- Execute commands
        for _, cmd in ipairs(exec_commands) do
            -- Snapshot before edits
            if cmd:match("^edit") and shadow_ok then
                pcall(function() shadow.snapshot({}) end)
            end

            -- Handle ask specially - read from user
            local result
            if cmd:match("^ask ") then
                local question = cmd:match("^ask (.+)")
                io.write("[agent] " .. question .. "\n> ")
                io.flush()
                local answer = io.read("*l") or ""
                result = { output = "User: " .. answer, success = true }
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

            -- Track for loop detection
            table.insert(recent_cmds, cmd)
            if #recent_cmds > 10 then
                table.remove(recent_cmds, 1)
            end

            -- Rollback on edit failure
            if cmd:match("^edit") and not result.success and shadow_ok then
                print("[agent] Edit failed, rolling back")
                pcall(function()
                    local snapshots = shadow.list()
                    if #snapshots > 1 then
                        shadow.restore(snapshots[#snapshots - 1].id)
                    end
                end)
            end
        end

        -- If done was requested along with commands, return after executing them
        if done_summary then
            print("[agent] Done: " .. done_summary)
            if total_retries > 0 then
                print("[agent] API retries: " .. total_retries)
            end
            return { success = true, output = table.concat(all_output, "\n") }
        end
    end

    print("[agent] Max turns reached")
    if total_retries > 0 then
        print("[agent] API retries: " .. total_retries)
    end
    return { success = false, output = table.concat(all_output, "\n") }
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
    local result = M.run(opts)
    if not result.success then
        os.exit(1)
    end
else
    return M
end
