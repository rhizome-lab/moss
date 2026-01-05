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
-- Each task has: name, prompt, check (function to verify result)
local TASKS = {
    {
        name = "find_main",
        prompt = "What file contains the main function for this project?",
        category = "exploration",
        check = function(output)
            return output:lower():find("main.rs") ~= nil
        end
    },
    {
        name = "count_functions",
        prompt = "How many functions are in crates/moss/src/filter.rs?",
        category = "exploration",
        check = function(output)
            -- Just verify it gives a number
            return output:match("%d+") ~= nil
        end
    },
    {
        name = "find_complexity",
        prompt = "What is the most complex function in the codebase?",
        category = "analysis",
        check = function(output)
            -- Should find and name a function
            return output:match("function") ~= nil or output:match("fn ") ~= nil
        end
    },
    {
        name = "explain_symbol",
        prompt = "Explain what the Filter struct does in src/filter.rs",
        category = "understanding",
        check = function(output)
            return output:lower():find("filter") ~= nil and
                   (output:lower():find("pattern") ~= nil or output:lower():find("match") ~= nil)
        end
    },
    {
        name = "find_usage",
        prompt = "What files use the Extractor struct?",
        category = "exploration",
        check = function(output)
            return output:match("%.rs") ~= nil
        end
    },
}

local M = {}

function M.list_tasks()
    print("Available benchmark tasks:")
    print("")
    for _, task in ipairs(TASKS) do
        print(string.format("  %-20s [%s]", task.name, task.category))
        print(string.format("    %s", task.prompt:sub(1, 60)))
    end
end

function M.run_task(task, opts)
    local start_time = os.time()

    -- Build agent command
    local cmd = string.format(
        '%s @agent --v2 --non-interactive --max-turns %d',
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

        if result.success then
            passed = passed + 1
            print(string.format("PASS (%ds, %d turns)", result.duration, result.turns))
        else
            failed = failed + 1
            print(string.format("FAIL (%ds, %d turns)", result.duration, result.turns))
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
    for _, r in ipairs(results) do
        total_time = total_time + r.duration
        total_turns = total_turns + r.turns
    end
    print(string.format("Total time: %ds", total_time))
    print(string.format("Avg turns: %.1f", total_turns / #TASKS))

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
