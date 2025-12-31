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

-- Find option spec by long name (handles --no-* prefix if negatable enabled)
local function find_option(options, name, negatable_enabled)
    if not options then return nil, false end
    -- Check exact match first
    for _, opt in ipairs(options) do
        if opt.name == name then return opt, false end
    end
    -- Check --no-* negation (only if enabled globally or per-option)
    if name:match("^no%-") then
        local base = name:sub(4)
        for _, opt in ipairs(options) do
            if opt.name == base then
                local can_negate = opt.negatable or (negatable_enabled and is_flag(opt))
                if can_negate then
                    return opt, true  -- found, is_negated
                end
            end
        end
    end
    return nil, false
end

-- Find option spec by short name
local function find_option_short(options, short)
    if not options then return nil end
    for _, opt in ipairs(options) do
        if opt.short == short then return opt end
    end
    return nil
end

-- Coerce value to type
local function coerce_value(value, opt)
    if not opt.type or opt.type == "string" then
        return value
    elseif opt.type == "number" then
        local n = tonumber(value)
        if n == nil then
            return nil, "expected number, got '" .. tostring(value) .. "'"
        end
        return n
    elseif opt.type == "integer" then
        local n = tonumber(value)
        if n == nil or n % 1 ~= 0 then
            return nil, "expected integer, got '" .. tostring(value) .. "'"
        end
        return math.floor(n)
    elseif opt.type == "boolean" then
        if value == "true" or value == "1" then return true end
        if value == "false" or value == "0" then return false end
        return nil, "expected boolean, got '" .. tostring(value) .. "'"
    end
    return value
end

-- Parse argv according to command spec
-- config flags: bundling, negatable, strict
local function parse_args(argv, spec, global_options, config)
    config = config or {}
    local result = {}
    local arg_specs = spec.args or {}
    local options = spec.options or {}
    local errors = {}

    -- Merge global options
    if global_options then
        for k, v in pairs(global_options) do
            result[k] = v
        end
    end

    -- Build combined options list for lookups
    local all_options = {}
    if spec._parent_options then
        for _, opt in ipairs(spec._parent_options) do
            table.insert(all_options, opt)
        end
    end
    for _, opt in ipairs(options) do
        table.insert(all_options, opt)
    end

    -- Apply environment variable fallbacks, then defaults
    for _, opt in ipairs(all_options) do
        local key = opt.name:gsub("-", "_")
        if opt.env then
            local env_val = os.getenv(opt.env)
            if env_val then
                local coerced, err = coerce_value(env_val, opt)
                if err then
                    table.insert(errors, opt.name .. " (from $" .. opt.env .. "): " .. err)
                else
                    result[key] = coerced
                end
            end
        end
        if result[key] == nil and opt.default ~= nil then
            result[key] = opt.default
        end
    end

    local positional_idx = 1
    local remaining_positional = {}
    local provided_options = {}  -- track which options were explicitly provided
    local i = 1

    while i <= #argv do
        local arg = argv[i]

        if arg == "--" then
            -- Everything after -- is positional
            for j = i + 1, #argv do
                table.insert(remaining_positional, argv[j])
            end
            break
        elseif arg:match("^%-%-") then
            -- Long option
            local name = arg:sub(3)
            local eq_pos = name:find("=")
            local value = nil
            if eq_pos then
                value = name:sub(eq_pos + 1)
                name = name:sub(1, eq_pos - 1)
            end

            local opt, is_negated = find_option(all_options, name, config.negatable)
            local key = (opt and opt.name or name):gsub("-", "_")

            if is_negated then
                result[key] = false
                provided_options[key] = true
            elseif opt and is_flag(opt) then
                result[key] = true
                provided_options[key] = true
            elseif value then
                local coerced, err = coerce_value(value, opt or {})
                if err then
                    table.insert(errors, name .. ": " .. err)
                else
                    result[key] = coerced
                    provided_options[key] = true
                end
            elseif opt and not is_flag(opt) then
                i = i + 1
                if argv[i] then
                    local coerced, err = coerce_value(argv[i], opt)
                    if err then
                        table.insert(errors, name .. ": " .. err)
                    else
                        result[key] = coerced
                        provided_options[key] = true
                    end
                else
                    table.insert(errors, "option --" .. name .. " requires a value")
                end
            else
                result[key] = true
                provided_options[key] = true
            end
        elseif arg:match("^%-[^%-]") then
            -- Short option(s) - handle bundling like -abc only if enabled
            local shorts = arg:sub(2)
            if config.bundling and #shorts > 1 then
                -- Bundling mode: -abc = -a -b -c
                for ci = 1, #shorts do
                    local short = shorts:sub(ci, ci)
                    local opt = find_option_short(all_options, short)
                    if opt then
                        local key = opt.name:gsub("-", "_")
                        if is_flag(opt) then
                            result[key] = true
                            provided_options[key] = true
                        elseif ci == #shorts then
                            -- Last char in bundle can take a value
                            i = i + 1
                            if argv[i] then
                                local coerced, err = coerce_value(argv[i], opt)
                                if err then
                                    table.insert(errors, "-" .. short .. ": " .. err)
                                else
                                    result[key] = coerced
                                    provided_options[key] = true
                                end
                            else
                                table.insert(errors, "option -" .. short .. " requires a value")
                            end
                        else
                            -- Non-last char in bundle that needs value is an error
                            table.insert(errors, "option -" .. short .. " requires a value (cannot be bundled)")
                        end
                    else
                        result[short] = true
                    end
                end
            else
                -- No bundling: treat as single short option
                local short = shorts:sub(1, 1)
                local opt = find_option_short(all_options, short)
                if opt then
                    local key = opt.name:gsub("-", "_")
                    if is_flag(opt) then
                        result[key] = true
                        provided_options[key] = true
                    else
                        -- Value is either rest of arg or next arg
                        local value_part = shorts:sub(2)
                        if #value_part > 0 then
                            local coerced, err = coerce_value(value_part, opt)
                            if err then
                                table.insert(errors, "-" .. short .. ": " .. err)
                            else
                                result[key] = coerced
                                provided_options[key] = true
                            end
                        else
                            i = i + 1
                            if argv[i] then
                                local coerced, err = coerce_value(argv[i], opt)
                                if err then
                                    table.insert(errors, "-" .. short .. ": " .. err)
                                else
                                    result[key] = coerced
                                    provided_options[key] = true
                                end
                            else
                                table.insert(errors, "option -" .. short .. " requires a value")
                            end
                        end
                    end
                else
                    result[short] = true
                end
            end
        else
            -- Positional argument
            if positional_idx <= #arg_specs then
                local spec_str = arg_specs[positional_idx]
                local name, is_rest = parse_arg_spec(spec_str)
                if is_rest then
                    -- Collect remaining args as array
                    local rest = {}
                    for j = i, #argv do
                        if argv[j] == "--" then break end
                        table.insert(rest, argv[j])
                    end
                    result[name] = rest
                    break
                else
                    result[name] = arg
                    positional_idx = positional_idx + 1
                end
            else
                table.insert(remaining_positional, arg)
            end
        end
        i = i + 1
    end

    -- Add remaining positional args to result
    for idx, val in ipairs(remaining_positional) do
        result[idx] = val
    end

    -- Validation (only if strict mode enabled)
    if config.strict then
        -- Validate required positional args
        for idx, spec_str in ipairs(arg_specs) do
            local name, is_rest, is_optional = parse_arg_spec(spec_str)
            if not is_optional and not is_rest and result[name] == nil then
                table.insert(errors, "missing required argument: " .. name)
            end
        end

        -- Validate required options
        for _, opt in ipairs(all_options) do
            if opt.required then
                local key = opt.name:gsub("-", "_")
                if result[key] == nil then
                    table.insert(errors, "missing required option: --" .. opt.name)
                end
            end
        end

        -- Check mutually exclusive options
        for _, opt in ipairs(all_options) do
            if opt.conflicts and provided_options[opt.name:gsub("-", "_")] then
                for _, conflict_name in ipairs(opt.conflicts) do
                    local conflict_key = conflict_name:gsub("-", "_")
                    if provided_options[conflict_key] then
                        table.insert(errors, "options --" .. opt.name .. " and --" .. conflict_name .. " are mutually exclusive")
                    end
                end
            end
        end
    end

    return result, errors
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
        local usage = "Usage: moss @" .. name
        if config.args then
            for _, arg_spec in ipairs(config.args) do
                local arg_name, is_rest, is_optional = parse_arg_spec(arg_spec)
                if is_rest then
                    usage = usage .. " [" .. arg_name .. "...]"
                elseif is_optional then
                    usage = usage .. " [" .. arg_name .. "]"
                else
                    usage = usage .. " <" .. arg_name .. ">"
                end
            end
        end
        if config.options then
            usage = usage .. " [options]"
        end
        print(usage)
    end
    print()

    if commands then
        print("Commands:")
        for _, cmd in ipairs(commands) do
            local suffix = cmd.default and " (default)" or ""
            local desc_str = cmd.description or ""
            local aliases_str = ""
            if cmd.aliases then
                aliases_str = " (alias: " .. table.concat(cmd.aliases, ", ") .. ")"
            end
            print(string.format("  %-12s %s%s%s", cmd.name, desc_str, suffix, aliases_str))
        end
        print()
    end

    if config.options then
        print("Options:")
        for _, opt in ipairs(config.options) do
            local desc_str = opt.description or ""
            local suffix = ""
            if opt.required then suffix = suffix .. " (required)" end
            if opt.env then suffix = suffix .. " [$" .. opt.env .. "]" end
            if opt.default ~= nil then suffix = suffix .. " [default: " .. tostring(opt.default) .. "]" end
            if opt.short then
                print(string.format("  -%s, --%-10s %s%s", opt.short, opt.name, desc_str, suffix))
            else
                print(string.format("      --%-10s %s%s", opt.name, desc_str, suffix))
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
                usage = usage .. " [" .. name .. "...]"
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
            local name, is_rest, is_optional = parse_arg_spec(arg_spec)
            local suffix = ""
            if is_optional then suffix = " (optional)" end
            if is_rest then suffix = " (multiple)" end
            print(string.format("  %-12s%s", name, suffix))
        end
        print()
    end

    if cmd.options then
        print("Options:")
        for _, opt in ipairs(cmd.options) do
            local desc_str = opt.description or ""
            local suffix = ""
            if opt.required then suffix = suffix .. " (required)" end
            if opt.env then suffix = suffix .. " [$" .. opt.env .. "]" end
            if opt.default ~= nil then suffix = suffix .. " [default: " .. tostring(opt.default) .. "]" end
            if opt.short then
                print(string.format("  -%s, --%-10s %s%s", opt.short, opt.name, desc_str, suffix))
            else
                print(string.format("      --%-10s %s%s", opt.name, desc_str, suffix))
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

-- Find command by name or alias
local function find_command(commands, name)
    if not commands then return nil end
    for _, cmd in ipairs(commands) do
        if cmd.name == name then return cmd end
        if cmd.aliases then
            for _, alias in ipairs(cmd.aliases) do
                if alias == name then return cmd end
            end
        end
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

-- Report errors and exit
local function report_errors(errors)
    for _, err in ipairs(errors) do
        io.stderr:write("Error: " .. err .. "\n")
    end
    os.exit(1)
end

-- Main entry point
-- Config flags:
--   bundling: enable short option bundling (-abc = -a -b -c)
--   negatable: enable --no-* for all flags
--   strict: enable validation errors (required args/options, conflicts)
function M.run(config)
    local argv = args or {}
    local name = config.name or "script"
    local commands = config.commands
    local parse_config = {
        bundling = config.bundling,
        negatable = config.negatable,
        strict = config.strict,
    }

    -- Check for top-level help
    if has_help_flag(argv) and (not commands or #argv == 1) then
        print_help(config, commands)
        return
    end

    if commands then
        -- Parse global options first (options before command)
        local global_parsed = {}
        local cmd_start = 1
        if config.options then
            -- Find where the command starts
            for i, arg in ipairs(argv) do
                if not arg:match("^%-") then
                    cmd_start = i
                    break
                end
            end
            -- Parse global args (before command)
            local global_argv = {}
            for i = 1, cmd_start - 1 do
                table.insert(global_argv, argv[i])
            end
            global_parsed = parse_args(global_argv, { options = config.options }, nil, parse_config)
        end

        -- Command routing
        local cmd_name = argv[cmd_start]
        local cmd_argv = {}
        for i = cmd_start + 1, #argv do
            table.insert(cmd_argv, argv[i])
        end

        local cmd = find_command(commands, cmd_name)
        if cmd then
            -- Found matching command
            if has_help_flag(cmd_argv) then
                print_command_help(name, cmd)
                return
            end

            -- Inherit parent options for lookups
            cmd._parent_options = config.options

            local parsed, errors = parse_args(cmd_argv, cmd, global_parsed, parse_config)
            if #errors > 0 then
                report_errors(errors)
            end
            if cmd.run then
                cmd.run(parsed)
            end
        elseif cmd_name == nil or cmd_name == "" then
            -- No command given
            if config.run then
                -- Top-level run handler
                local parsed, errors = parse_args(argv, config, nil, parse_config)
                if #errors > 0 then
                    report_errors(errors)
                end
                config.run(parsed)
            else
                -- Try default command
                cmd = find_default_command(commands)
                if cmd then
                    cmd._parent_options = config.options
                    local parsed, errors = parse_args(argv, cmd, global_parsed, parse_config)
                    if #errors > 0 then
                        report_errors(errors)
                    end
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
        local parsed, errors = parse_args(argv, config, nil, parse_config)
        if #errors > 0 then
            report_errors(errors)
        end
        config.run(parsed)
    else
        print_help(config, commands)
    end
end

return M
