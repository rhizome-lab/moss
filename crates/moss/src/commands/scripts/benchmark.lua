--[[
Agent Benchmark Suite

Runs a set of predefined tasks and measures agent performance.
Metrics: success rate, turns used, time taken.

Usage:
  moss @benchmark                    # Run all benchmarks
  moss @benchmark --task <name>      # Run specific task
  moss @benchmark --list             # List available tasks
  moss @benchmark --provider gemini  # Use specific provider
]]

-- Benchmark task definition
-- Each task has: name, prompt, check (function to verify result), expected_turns (optional)
local TASKS = {
    -- === EXPLORATION ===
    {
        name = "find_main",
        prompt = "What file contains the main function for this project?",
        category = "exploration",
        expected_turns = 4,
        check = function(output)
            return output:lower():find("main.rs") ~= nil
        end
    },
    {
        name = "list_agent_files",
        prompt = "What lua files are in agent/?",
        category = "exploration",
        expected_turns = 2,
        check = function(output)
            -- Should find the agent submodule files
            return output:find("parser") ~= nil and
                   output:find("roles") ~= nil and
                   output:find("commands") ~= nil
        end
    },
    {
        name = "find_usage",
        prompt = "What files use the Extractor struct?",
        category = "exploration",
        expected_turns = 4,
        check = function(output)
            return output:match("%.rs") ~= nil
        end
    },

    -- === ANALYSIS ===
    {
        name = "count_functions",
        prompt = "How many exported functions are in agent.lua?",
        category = "analysis",
        expected_turns = 3,
        check = function(output)
            -- Should find the 3 exported functions
            return output:find("3") ~= nil or
                   (output:find("run_state_machine") ~= nil and
                    output:find("is_looping") ~= nil and
                    output:find("show_help") ~= nil)
        end
    },
    {
        name = "find_complexity",
        prompt = "What is the most complex function in crates/moss/src/path_resolve.rs?",
        category = "analysis",
        expected_turns = 4,
        check = function(output)
            -- Should identify a specific function
            return output:match("fn%s+%w+") ~= nil or output:match("function") ~= nil
        end
    },

    -- === UNDERSTANDING ===
    {
        name = "explain_symbol",
        prompt = "Explain what the Filter struct does in crates/moss/src/filter.rs",
        category = "understanding",
        expected_turns = 4,
        check = function(output)
            return output:lower():find("filter") ~= nil and
                   (output:lower():find("pattern") ~= nil or
                    output:lower():find("match") ~= nil or
                    output:lower():find("glob") ~= nil)
        end
    },
    {
        name = "trace_flow",
        prompt = "How does path_resolve.rs resolve fuzzy paths?",
        category = "understanding",
        expected_turns = 4,
        check = function(output)
            return output:lower():find("fuzzy") ~= nil or
                   output:lower():find("resolve") ~= nil or
                   output:lower():find("match") ~= nil
        end
    },
    {
        name = "cross_layer",
        prompt = "How do the Rust LLM bindings communicate with the Lua agent?",
        category = "understanding",
        expected_turns = 4,
        check = function(output)
            return output:lower():find("mlua") ~= nil or
                   output:lower():find("lua_runtime") ~= nil or
                   (output:lower():find("llm") ~= nil and output:lower():find("chat") ~= nil)
        end
    },

    -- === ARCHITECTURE ===
    {
        name = "find_entry_point",
        prompt = "What is the entry point for the @agent command? Trace from CLI to Lua.",
        category = "architecture",
        expected_turns = 6,
        check = function(output)
            return output:find("script.rs") ~= nil or
                   output:find("agent.lua") ~= nil
        end
    },
}

local M = {}

function M.list_tasks()
    print("Available benchmark tasks:")
    print("")
    for _, task in ipairs(TASKS) do
        local turns_info = task.expected_turns and string.format(" (%d turns)", task.expected_turns) or ""
        print(string.format("  %-20s [%s]%s", task.name, task.category, turns_info))
        print(string.format("    %s", task.prompt:sub(1, 60)))
    end
end

function M.run_task(task, opts)
    local start_time = os.time()

    -- Build agent command
    local cmd = string.format(
        '%s @agent --non-interactive --max-turns %d',
        _moss_bin, opts.max_turns or 10
    )
    if opts.provider then
        cmd = cmd .. ' --provider ' .. opts.provider
    end
    if opts.model then
        cmd = cmd .. ' --model ' .. opts.model
    end
    -- Escape the prompt
    local escaped_prompt = task.prompt:gsub('"', '\\"')
    cmd = cmd .. ' "' .. escaped_prompt .. '"'

    local result = shell(cmd)

    local end_time = os.time()
    local duration = end_time - start_time

    local output = result.output or ""
    local success = result.success and task.check(output)

    -- Count turns from output
    local turns = 0
    for _ in output:gmatch("%[agent%] Turn") do
        turns = turns + 1
    end

    return {
        name = task.name,
        category = task.category,
        success = success,
        duration = duration,
        turns = turns,
        expected_turns = task.expected_turns,
        output_length = #output
    }
end

function M.run_all(opts)
    local results = {}
    local passed = 0
    local failed = 0

    print("=" .. string.rep("=", 60))
    print("Agent Benchmark Suite")
    print("=" .. string.rep("=", 60))
    print(string.format("Provider: %s", opts.provider or "gemini"))
    print(string.format("Tasks: %d", #TASKS))
    print("")

    for i, task in ipairs(TASKS) do
        io.write(string.format("[%d/%d] %s... ", i, #TASKS, task.name))
        io.flush()

        local result = M.run_task(task, opts)
        table.insert(results, result)

        local turn_status = ""
        if result.expected_turns then
            if result.turns <= result.expected_turns then
                turn_status = " ok"
            else
                turn_status = string.format(" +%d", result.turns - result.expected_turns)
            end
        end

        if result.success then
            passed = passed + 1
            print(string.format("PASS (%ds, %d turns%s)", result.duration, result.turns, turn_status))
        else
            failed = failed + 1
            print(string.format("FAIL (%ds, %d turns%s)", result.duration, result.turns, turn_status))
        end
    end

    print("")
    print("=" .. string.rep("=", 60))
    print("Results Summary")
    print("=" .. string.rep("=", 60))
    print(string.format("Passed: %d/%d (%.0f%%)", passed, #TASKS, 100 * passed / #TASKS))
    print(string.format("Failed: %d/%d", failed, #TASKS))

    local total_time = 0
    local total_turns = 0
    local total_expected = 0
    local within_expected = 0
    for _, r in ipairs(results) do
        total_time = total_time + r.duration
        total_turns = total_turns + r.turns
        if r.expected_turns then
            total_expected = total_expected + r.expected_turns
            if r.turns <= r.expected_turns then
                within_expected = within_expected + 1
            end
        end
    end
    print(string.format("Total time: %ds", total_time))
    print(string.format("Avg turns: %.1f", total_turns / #TASKS))
    if total_expected > 0 then
        print(string.format("Turn efficiency: %d/%d within expected", within_expected, #TASKS))
    end

    -- Save results as simple text format
    local results_file = ".moss/benchmark-results.txt"
    local f = io.open(results_file, "a")
    if f then
        f:write(string.format("\n=== Benchmark Run: %s ===\n", os.date("%Y-%m-%dT%H:%M:%S")))
        f:write(string.format("Provider: %s\n", opts.provider or "gemini"))
        f:write(string.format("Passed: %d/%d\n", passed, #TASKS))
        f:write(string.format("Total time: %ds\n", total_time))
        f:write(string.format("Avg turns: %.1f\n", total_turns / #TASKS))
        f:write("\nResults:\n")
        for _, r in ipairs(results) do
            f:write(string.format("  %s: %s (%ds, %d turns)\n",
                r.name, r.success and "PASS" or "FAIL", r.duration, r.turns))
        end
        f:close()
        print(string.format("\nResults saved to: %s", results_file))
    end

    return passed == #TASKS and 0 or 1
end

function M.parse_args(args)
    local opts = {}
    local i = 1
    while i <= #args do
        local arg = args[i]
        if arg == "--list" then
            opts.list = true
            i = i + 1
        elseif arg == "--task" and args[i+1] then
            opts.task = args[i+1]
            i = i + 2
        elseif arg == "--provider" and args[i+1] then
            opts.provider = args[i+1]
            i = i + 2
        elseif arg == "--model" and args[i+1] then
            opts.model = args[i+1]
            i = i + 2
        elseif arg == "--max-turns" and args[i+1] then
            opts.max_turns = tonumber(args[i+1])
            i = i + 2
        else
            i = i + 1
        end
    end
    return opts
end

-- Main
if args and #args >= 0 then
    local opts = M.parse_args(args)

    if opts.list then
        M.list_tasks()
        return 0
    end

    if opts.task then
        -- Run specific task
        for _, task in ipairs(TASKS) do
            if task.name == opts.task then
                local result = M.run_task(task, opts)
                if result.success then
                    print("PASS")
                    return 0
                else
                    print("FAIL")
                    return 1
                end
            end
        end
        print("Task not found: " .. opts.task)
        return 1
    end

    -- Run all
    return M.run_all(opts)
end

return M
