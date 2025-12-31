-- @config: Config file viewer/editor using tree-sitter
-- Usage: moss @config [get|set] [key] [value]
--   moss @config              - view full config
--   moss @config get key      - get specific key (e.g., daemon.port)
--   moss @config set key val  - set key to value

local CONFIG_PATH = ".moss/config.toml"

-- Parse TOML into a table structure using tree-sitter
local function parse_toml(content)
    local tree = ts.parse(content, "toml")
    local root = tree:root()
    local result = {}

    local function get_value(node)
        local kind = node:kind()
        if kind == "string" then
            -- Remove quotes
            local text = node:text()
            return text:sub(2, -2)
        elseif kind == "integer" then
            return tonumber(node:text())
        elseif kind == "float" then
            return tonumber(node:text())
        elseif kind == "boolean" then
            return node:text() == "true"
        elseif kind == "array" then
            local arr = {}
            for _, child in ipairs(node:named_children()) do
                table.insert(arr, get_value(child))
            end
            return arr
        else
            return node:text()
        end
    end

    local function process_table(table_node, target)
        -- Get table name from bare_key
        local table_name = nil
        for _, child in ipairs(table_node:named_children()) do
            if child:kind() == "bare_key" or child:kind() == "dotted_key" then
                table_name = child:text()
                break
            end
        end

        if table_name then
            target[table_name] = target[table_name] or {}
            -- Process pairs in this table
            for _, child in ipairs(table_node:named_children()) do
                if child:kind() == "pair" then
                    local key, value
                    for _, pc in ipairs(child:named_children()) do
                        if pc:kind() == "bare_key" or pc:kind() == "dotted_key" then
                            key = pc:text()
                        else
                            value = get_value(pc)
                        end
                    end
                    if key then
                        target[table_name][key] = value
                    end
                end
            end
        end
    end

    for _, child in ipairs(root:named_children()) do
        if child:kind() == "table" then
            process_table(child, result)
        end
    end

    return result
end

-- Get a value by dotted key (e.g., "daemon.port")
local function get_key(config, key)
    local parts = {}
    for part in key:gmatch("[^.]+") do
        table.insert(parts, part)
    end

    local current = config
    for _, part in ipairs(parts) do
        if type(current) ~= "table" then
            return nil
        end
        current = current[part]
    end
    return current
end

-- Format a value for display
local function format_value(val)
    if type(val) == "table" then
        local items = {}
        for i, v in ipairs(val) do
            table.insert(items, format_value(v))
        end
        return "[" .. table.concat(items, ", ") .. "]"
    elseif type(val) == "string" then
        return '"' .. val .. '"'
    else
        return tostring(val)
    end
end

-- Format a value for TOML
local function toml_value(val)
    if type(val) == "table" then
        local items = {}
        for _, v in ipairs(val) do
            table.insert(items, toml_value(v))
        end
        return "[" .. table.concat(items, ", ") .. "]"
    elseif type(val) == "string" then
        return '"' .. val:gsub('\\', '\\\\'):gsub('"', '\\"') .. '"'
    elseif type(val) == "boolean" then
        return val and "true" or "false"
    else
        return tostring(val)
    end
end

-- Parse a value from command line
local function parse_value(str)
    -- Boolean
    if str == "true" then return true end
    if str == "false" then return false end
    -- Number
    local num = tonumber(str)
    if num then return num end
    -- Array (simple: comma-separated)
    if str:match("^%[.*%]$") then
        local items = {}
        for item in str:sub(2, -2):gmatch("[^,]+") do
            table.insert(items, parse_value(item:match("^%s*(.-)%s*$")))
        end
        return items
    end
    -- String (remove quotes if present)
    if str:match('^".*"$') or str:match("^'.*'$") then
        return str:sub(2, -2)
    end
    return str
end

-- Set a value in the config file
local function set_key(key, value)
    local parts = {}
    for part in key:gmatch("[^.]+") do
        table.insert(parts, part)
    end

    if #parts < 2 then
        print("Error: key must be section.key (e.g., daemon.port)")
        os.exit(1)
    end

    local section = parts[1]
    local field = table.concat(parts, ".", 2)

    -- Read current content
    local content = ""
    if file_exists(CONFIG_PATH) then
        content = read_file(CONFIG_PATH)
    end

    local tree = ts.parse(content, "toml")
    local root = tree:root()

    -- Find the section and key
    local section_found = false
    local key_line = nil
    local section_end_line = nil

    for _, child in ipairs(root:named_children()) do
        if child:kind() == "table" then
            local table_name = nil
            for _, tc in ipairs(child:named_children()) do
                if tc:kind() == "bare_key" then
                    table_name = tc:text()
                    break
                end
            end

            if table_name == section then
                section_found = true
                section_end_line = child:end_row()

                -- Find the key
                for _, tc in ipairs(child:named_children()) do
                    if tc:kind() == "pair" then
                        for _, pc in ipairs(tc:named_children()) do
                            if (pc:kind() == "bare_key" or pc:kind() == "dotted_key") and pc:text() == field then
                                key_line = tc:start_row()
                                break
                            end
                        end
                    end
                end
            end
        end
    end

    -- Read as lines
    local lines = {}
    for line in (content .. "\n"):gmatch("([^\n]*)\n") do
        table.insert(lines, line)
    end

    local new_line = field .. " = " .. toml_value(value)

    if key_line then
        -- Replace existing line
        lines[key_line] = new_line
    elseif section_found then
        -- Add to existing section
        table.insert(lines, section_end_line, new_line)
    else
        -- Add new section
        if #lines > 0 and lines[#lines] ~= "" then
            table.insert(lines, "")
        end
        table.insert(lines, "[" .. section .. "]")
        table.insert(lines, new_line)
    end

    write_file(CONFIG_PATH, table.concat(lines, "\n"))
    print(key .. " = " .. format_value(value))
end

-- Help text
local function print_help()
    print([[moss @config - Config file viewer/editor

Usage: moss @config [command] [args...]

Commands:
  (none)        View full config (default)
  get <key>     Get a specific key (e.g., daemon.port)
  set <key> <v> Set key to value

Examples:
  moss @config                      # view full config
  moss @config get daemon.port      # get specific key
  moss @config set daemon.port 8080 # set daemon port to 8080
  moss @config set view.tests true  # enable test display

Config is stored at: .moss/config.toml]])
end

-- Main
local action = args[1]

if action == "--help" or action == "-h" or action == "help" then
    print_help()
    os.exit(0)
elseif not action or action == "view" then
    -- View full config
    if file_exists(CONFIG_PATH) then
        local result = view("@config")
        print(result.output)
    else
        print("No config file at " .. CONFIG_PATH)
        print("Create one with: moss @config set <key> <value>")
    end

elseif action == "get" then
    local key = args[2]
    if not key then
        print("Usage: moss @config get <key>")
        print("Example: moss @config get daemon.port")
        os.exit(1)
    end

    if not file_exists(CONFIG_PATH) then
        print("No config file")
        os.exit(1)
    end

    local content = read_file(CONFIG_PATH)
    local config = parse_toml(content)
    local value = get_key(config, key)

    if value == nil then
        print("Key not found: " .. key)
        os.exit(1)
    end

    print(format_value(value))

elseif action == "set" then
    local key = args[2]
    local value_str = args[3]

    if not key or not value_str then
        print("Usage: moss @config set <key> <value>")
        print("Example: moss @config set daemon.port 8080")
        os.exit(1)
    end

    local value = parse_value(value_str)
    set_key(key, value)

else
    print("Unknown action: " .. action)
    print("Run 'moss @config --help' for usage")
    os.exit(1)
end
