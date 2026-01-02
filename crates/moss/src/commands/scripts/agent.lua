-- Agent module: autonomous task execution with moss tools
local M = {}

local SYSTEM_PROMPT = [[
Coding session. Output commands in [cmd][/cmd] tags. Multiple commands per turn allowed.
<pre>
[cmd]view [--types-only|--full|--deps] .[/cmd]
[cmd]view src/main.rs[/cmd]
[cmd]view src/main.rs/main[/cmd]
[cmd]text-search "pattern"[/cmd]
[cmd]edit src/lib.rs/foo [delete|replace|insert|move][/cmd]
[cmd]package [list|tree|info|outdated|audit][/cmd]
[cmd]analyze [complexity|security|callers|callees|...][/cmd]
[cmd]run cargo test[/cmd]
[cmd]ask which module?[/cmd]
[cmd]done summary here[/cmd]
</pre>
]]

-- Check if last N turns have identical first command (loop detection)
function M.is_looping(history, n)
    n = n or 3
    if #history < n then return false end

    local last_turn = history[#history]
    if not last_turn.outputs or #last_turn.outputs == 0 then return false end
    local last_cmd = last_turn.outputs[1].cmd

    for i = 1, n - 1 do
        local turn = history[#history - i]
        if not turn.outputs or #turn.outputs == 0 then return false end
        if turn.outputs[1].cmd ~= last_cmd then
            return false
        end
    end
    return true
end

-- Build chat messages from history
-- History is per-turn: {response, outputs: [{cmd, output, success}]}
-- Returns: messages (list of {role, content}), current_prompt
function M.build_messages(base_context, history, keep_last)
    keep_last = keep_last or 10
    local messages = {}

    local start_idx = math.max(1, #history - keep_last + 1)
    for i = start_idx, #history do
        local turn = history[i]
        -- Assistant message: LLM response
        table.insert(messages, {"assistant", turn.response})
        -- User message: all command outputs combined
        local parts = {}
        for _, cmd_result in ipairs(turn.outputs) do
            local output = cmd_result.output or ""
            if #output > 4000 then
                output = output:sub(1, 2000) .. "\n...[truncated]...\n" .. output:sub(-1000)
            end
            local header = "$ " .. cmd_result.cmd
            if not cmd_result.success then
                header = header .. " (failed)"
            end
            table.insert(parts, header .. "\n" .. output)
        end
        table.insert(messages, {"user", table.concat(parts, "\n\n")})
    end

    return messages, base_context
end

-- Main agent loop
function M.run(opts)
    opts = opts or {}
    local task = opts.prompt or opts.task or "Help with this codebase"
    local max_turns = opts.max_turns or 15
    local max_tokens = opts.max_tokens or 4096
    local provider = opts.provider or "gemini"
    local model = opts.model

    -- Build initial context
    local context = "Task: " .. task .. "\n"
    context = context .. "Directory: " .. _moss_root .. "\n\n"

    -- Recall relevant memories
    local ok, memories = pcall(recall, task, 3)
    if ok and memories and #memories > 0 then
        context = context .. "Relevant context from previous sessions:\n"
        for _, m in ipairs(memories) do
            context = context .. "- " .. m.content .. "\n"
        end
        context = context .. "\n"
    end

    -- Initialize shadow git for rollback capability
    local shadow_ok = pcall(function()
        shadow.open()
        shadow.snapshot({})
    end)

    local history = {}
    local all_output = {}

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

        -- Build chat messages
        local messages, prompt = M.build_messages(context, history)

        -- Add loop warning to current prompt if needed
        if M.is_looping(history, 3) then
            prompt = prompt .. "\nWARNING: You've run the same command 3 times. Explain what's wrong and try a different approach.\n"
        end

        -- Get LLM response
        if os.getenv("MOSS_AGENT_DEBUG") then
            print("[DEBUG] Prompt length: " .. #prompt)
            print("[DEBUG] Messages: " .. #messages)
            if #history > 0 then
                local last_turn = history[#history]
                local total_len = 0
                for _, o in ipairs(last_turn.outputs or {}) do
                    total_len = total_len + #(o.output or "")
                end
                print("[DEBUG] Last turn output length: " .. total_len)
            end
        end
        io.write("[agent] Thinking... ")
        io.flush()

        -- Retry logic for intermittent API failures
        local response
        local max_retries = 3
        for attempt = 1, max_retries do
            local ok, result = pcall(function()
                return llm.chat(provider, model, SYSTEM_PROMPT, prompt, messages)
            end)
            if ok then
                response = result
                break
            elseif attempt < max_retries then
                io.write("retry " .. attempt .. "... ")
                io.flush()
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

        -- Extract all commands from response
        local commands = {}
        for cmd in response:gmatch("%[cmd%](.-)%[/cmd%]") do
            table.insert(commands, cmd)
        end

        if #commands == 0 then
            print("[agent] No commands found, finishing")
            return { success = true, output = table.concat(all_output, "\n") }
        end

        -- Execute all commands, collect outputs
        local turn_outputs = {}
        for _, cmd in ipairs(commands) do
            -- Check for done command
            if cmd:match("^done") then
                local summary = cmd:match("^done%s*(.*)") or ""
                print("[agent] Done: " .. summary)
                return { success = true, output = table.concat(all_output, "\n") }
            end

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

            table.insert(turn_outputs, {
                cmd = cmd,
                output = result.output,
                success = result.success
            })

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

        -- Store turn in history (one entry per turn, not per command)
        table.insert(history, {
            response = response,
            outputs = turn_outputs
        })
    end

    print("[agent] Max turns reached")
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
