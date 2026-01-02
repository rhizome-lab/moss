-- Agent module: autonomous task execution with moss tools
local M = {}

local SYSTEM_PROMPT = [[
Commands start with "> ". Output from commands appears after your message.

> view <path>
> grep <pattern>
> edit <path> <task>
> shell <cmd>
> ask <question>
> done <summary>

Example:
  User: fix the typo
  You: Let me find it.
  > grep typo
  [output appears here next turn]
  You: Found it in README.md line 5.
  > edit README.md fix typo on line 5
  [output appears here next turn]
  You: Fixed.
  > done fixed typo in README.md
]]

-- Check if last N commands are identical (loop detection)
function M.is_looping(history, n)
    n = n or 3
    if #history < n then return false end

    local last_cmd = history[#history].cmd
    for i = 1, n - 1 do
        if history[#history - i].cmd ~= last_cmd then
            return false
        end
    end
    return true
end

-- Build prompt from history (keep last N turns, not all)
function M.build_prompt(base_context, history, keep_last)
    keep_last = keep_last or 6
    local prompt = base_context

    local start_idx = math.max(1, #history - keep_last + 1)
    for i = start_idx, #history do
        local h = history[i]
        prompt = prompt .. "\n> " .. h.cmd .. "\n"
        if h.output and #h.output > 0 then
            -- Truncate very long outputs
            local output = h.output
            if #output > 4000 then
                output = output:sub(1, 2000) .. "\n...[truncated]...\n" .. output:sub(-1000)
            end
            prompt = prompt .. output .. "\n"
        end
        if not h.success then
            prompt = prompt .. "(command failed)\n"
        end
    end

    return prompt
end

-- Main agent loop
function M.run(opts)
    opts = opts or {}
    local task = opts.prompt or opts.task or "Help with this codebase"
    local max_turns = opts.max_turns or 1  -- TODO: increase after testing
    local max_tokens = opts.max_tokens or 512  -- TODO: increase after testing
    local provider = opts.provider
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

    for turn = 1, max_turns do
        print(string.format("[agent] Turn %d/%d", turn, max_turns))

        -- Check for loops
        local prompt = M.build_prompt(context, history)
        if M.is_looping(history, 3) then
            prompt = prompt .. "\nWARNING: You've run the same command 3 times. Explain what's wrong and try a different approach.\n"
        end

        -- Get LLM response
        if os.getenv("MOSS_AGENT_DEBUG") then
            print("[DEBUG] Prompt length: " .. #prompt)
            print("[DEBUG] History entries: " .. #history)
            if #history > 0 then
                print("[DEBUG] Last output length: " .. #(history[#history].output or ""))
            end
        end
        io.write("[agent] Thinking... ")
        io.flush()
        local response = llm.complete(provider, model, SYSTEM_PROMPT, prompt, max_tokens)
        io.write("done\n")

        print(response)
        table.insert(all_output, response)

        -- Extract all commands from response
        local commands = {}
        for cmd in response:gmatch("> ([^\n]+)") do
            table.insert(commands, cmd)
        end

        if #commands == 0 then
            print("[agent] No commands found, finishing")
            return { success = true, output = table.concat(all_output, "\n") }
        end

        -- Execute each command
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
            else
                -- Execute command via moss
                print("[agent] Running: " .. cmd)
                result = shell("./target/debug/moss " .. cmd)
            end

            table.insert(history, {
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
    end

    print("[agent] Max turns reached")
    return { success = false, output = table.concat(all_output, "\n") }
end

-- When run as script (moss @agent), execute directly
-- When required as module, return M
if args and #args >= 0 then
    local task = table.concat(args, " ")
    if task == "" then task = nil end
    local result = M.run({ prompt = task })
    if not result.success then
        os.exit(1)
    end
else
    return M
end
