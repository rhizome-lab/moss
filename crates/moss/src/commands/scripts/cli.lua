-- CLI parsing library for moss scripts
-- Usage: local cli = require("cli")

local M = {}

-- Parse positional arg spec like "name", "name?", "name..."
local function parse_arg_spec(spec)
    if spec:match("%.%.%.$") then
        return spec:sub(1, -4), true, false  -- name, is_rest, is_optional
    elseif spec:match("%?$") then
        return spec:sub(1, -2), false, true  -- name, is_rest, is_optional
    else
        return spec, false, false
    end
end

-- Check if an option is a flag (boolean, no value)
local function is_flag(opt)
    if opt.flag then return true end
    if opt.type == "boolean" then return true end
    if opt.type and opt.type ~= "boolean" then return false end
    if opt.default ~= nil and type(opt.default) ~= "boolean" then return false end
    return false  -- default: options take values
end

-- Find option spec by long name
local function find_option(options, name)
    if not options then return nil end
    for _, opt in ipairs(options) do
        if opt.name == name then return opt end
    end
    return nil
end

-- Find option spec by short name
local function find_option_short(options, short)
    if not options then return nil end
    for _, opt in ipairs(options) do
        if opt.short == short then return opt end
    end
    return nil
end

-- Parse argv according to command spec
local function parse_args(argv, spec)
    local result = {}
    local arg_specs = spec.args or {}
    local options = spec.options or {}

    -- Apply defaults from options
    for _, opt in ipairs(options) do
        if opt.default ~= nil then
            local key = opt.name:gsub("-", "_")
            result[key] = opt.default
        end
    end

    local positional_idx = 1
    local remaining_positional = {}
    local i = 1

    while i <= #argv do
        local arg = argv[i]

        if arg:match("^%-%-") then
            -- Long option
            local name = arg:sub(3)
            local eq_pos = name:find("=")
            if eq_pos then
                local key = name:sub(1, eq_pos - 1):gsub("-", "_")
                local value = name:sub(eq_pos + 1)
                result[key] = value
            else
                local opt = find_option(options, name)
                local key = name:gsub("-", "_")
                if opt and not is_flag(opt) then
                    i = i + 1
                    result[key] = argv[i]
                else
                    result[key] = true
                end
            end
        elseif arg:match("^%-.$") then
            -- Short option
            local short = arg:sub(2)
            local opt = find_option_short(options, short)
            if opt then
                local key = opt.name:gsub("-", "_")
                if is_flag(opt) then
                    result[key] = true
                else
                    i = i + 1
                    result[key] = argv[i]
                end
            else
                result[short] = true
            end
        else
            -- Positional argument
            if positional_idx <= #arg_specs then
                local spec_str = arg_specs[positional_idx]
                local name, is_rest = parse_arg_spec(spec_str)
                if is_rest then
                    -- Collect remaining args
                    local rest = {}
                    for j = i, #argv do
                        table.insert(rest, argv[j])
                    end
                    result[name] = table.concat(rest, " ")
                    break
                else
                    result[name] = arg
                    positional_idx = positional_idx + 1
                end
            else
                -- Extra positional args go into numeric indices
                table.insert(remaining_positional, arg)
            end
        end
        i = i + 1
    end

    -- Add remaining positional args to result
    for idx, val in ipairs(remaining_positional) do
        result[idx] = val
    end

    return result
end

-- Print main help
local function print_help(config, commands)
    local name = config.name or "script"
    local desc = config.description

    if desc then
        print(name .. " - " .. desc)
    else
        print(name)
    end
    print()

    if commands then
        print("Usage: moss @" .. name .. " <command> [options]")
    else
        print("Usage: moss @" .. name .. " [options]")
    end
    print()

    if commands then
        print("Commands:")
        for _, cmd in ipairs(commands) do
            local suffix = cmd.default and " (default)" or ""
            local desc_str = cmd.description or ""
            print(string.format("  %-12s %s%s", cmd.name, desc_str, suffix))
        end
        print()
    end

    if config.options then
        print("Options:")
        for _, opt in ipairs(config.options) do
            local desc_str = opt.description or ""
            if opt.short then
                print(string.format("  -%s, --%-10s %s", opt.short, opt.name, desc_str))
            else
                print(string.format("      --%-10s %s", opt.name, desc_str))
            end
        end
        print()
    end

    print("  -h, --help       Show this help")
end

-- Print command help
local function print_command_help(app_name, cmd)
    local desc = cmd.description or ""
    print(app_name .. " " .. cmd.name .. " - " .. desc)
    print()

    local usage = "Usage: moss @" .. app_name .. " " .. cmd.name
    if cmd.args then
        for _, arg_spec in ipairs(cmd.args) do
            local name, is_rest, is_optional = parse_arg_spec(arg_spec)
            if is_rest then
                usage = usage .. " <" .. name .. ">"
            elseif is_optional then
                usage = usage .. " [" .. name .. "]"
            else
                usage = usage .. " <" .. name .. ">"
            end
        end
    end
    if cmd.options then
        usage = usage .. " [options]"
    end
    print(usage)
    print()

    if cmd.args then
        print("Arguments:")
        for _, arg_spec in ipairs(cmd.args) do
            local name = parse_arg_spec(arg_spec)
            print(string.format("  %-12s", name))
        end
        print()
    end

    if cmd.options then
        print("Options:")
        for _, opt in ipairs(cmd.options) do
            local desc_str = opt.description or ""
            if opt.short then
                print(string.format("  -%s, --%-10s %s", opt.short, opt.name, desc_str))
            else
                print(string.format("      --%-10s %s", opt.name, desc_str))
            end
        end
        print()
    end

    print("  -h, --help       Show this help")
end

-- Check if argv contains help flag
local function has_help_flag(argv)
    for _, arg in ipairs(argv) do
        if arg == "--help" or arg == "-h" then
            return true
        end
    end
    return false
end

-- Find command by name
local function find_command(commands, name)
    if not commands then return nil end
    for _, cmd in ipairs(commands) do
        if cmd.name == name then return cmd end
    end
    return nil
end

-- Find default command
local function find_default_command(commands)
    if not commands then return nil end
    for _, cmd in ipairs(commands) do
        if cmd.default then return cmd end
    end
    return nil
end

-- Main entry point
function M.run(config)
    local argv = args or {}
    local name = config.name or "script"
    local commands = config.commands

    -- Check for top-level help
    if has_help_flag(argv) and (not commands or #argv == 1) then
        print_help(config, commands)
        return
    end

    if commands then
        -- Command routing
        local cmd_name = argv[1]
        local cmd_argv = {}
        for i = 2, #argv do
            table.insert(cmd_argv, argv[i])
        end

        local cmd = find_command(commands, cmd_name)
        if cmd then
            -- Found matching command
            if has_help_flag(cmd_argv) then
                print_command_help(name, cmd)
                return
            end

            local parsed = parse_args(cmd_argv, cmd)
            if cmd.run then
                cmd.run(parsed)
            end
        elseif cmd_name == nil or cmd_name == "" then
            -- No command given
            if config.run then
                -- Top-level run handler
                local parsed = parse_args(argv, config)
                config.run(parsed)
            else
                -- Try default command
                cmd = find_default_command(commands)
                if cmd then
                    local parsed = parse_args(argv, cmd)
                    if cmd.run then
                        cmd.run(parsed)
                    end
                else
                    print_help(config, commands)
                end
            end
        else
            -- Unknown command
            io.stderr:write("Unknown command: " .. tostring(cmd_name) .. "\n")
            print_help(config, commands)
            os.exit(1)
        end
    elseif config.run then
        -- Simple script
        local parsed = parse_args(argv, config)
        config.run(parsed)
    else
        print_help(config, commands)
    end
end

return M
