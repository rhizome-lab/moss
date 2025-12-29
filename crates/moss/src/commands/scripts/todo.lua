-- @todo: TODO file viewer/editor
-- Usage: moss @todo [list|add|done|rm|clean] [args...]
-- Pure Lua implementation

-- Find todo file
local function find_todo_file()
    local names = {"TODO.md", "TODO.txt", "TODO", "TASKS.md", "TASKS.txt", "TASKS",
                   "todo.md", "todo.txt", "todo", "tasks.md", "tasks.txt", "tasks"}
    for _, name in ipairs(names) do
        if file_exists(name) then
            return name
        end
    end
    return nil
end

-- Parse a line as an item, returns {text, done, raw} or nil
local function parse_item(line)
    local trimmed = line:match("^%s*(.-)%s*$")
    if trimmed == "" then return nil end

    -- Checkbox done: - [x] or - [X]
    local text = trimmed:match("^%- %[x%] (.+)$") or trimmed:match("^%- %[X%] (.+)$")
    if text then
        return {text = text, done = true, raw = line}
    end

    -- Checkbox pending: - [ ]
    text = trimmed:match("^%- %[ %] (.+)$")
    if text then
        return {text = text, done = false, raw = line}
    end

    -- Bullet: - item
    text = trimmed:match("^%- (.+)$")
    if text then
        return {text = text, done = false, raw = line}
    end

    -- Numbered: 1. item
    text = trimmed:match("^%d+%. (.+)$")
    if text then
        return {text = text, done = false, raw = line}
    end

    return nil
end

-- Parse header, returns {level, name} or nil
local function parse_header(line)
    local hashes, name = line:match("^(#+)%s+(.+)$")
    if hashes and name then
        return {level = #hashes, name = name}
    end
    return nil
end

-- Priority section names
local PRIMARY_SECTIONS = {"next up", "next", "todo", "tasks", "in progress", "current", "active"}

-- Find primary section index
local function find_primary_section(sections)
    for _, priority in ipairs(PRIMARY_SECTIONS) do
        for i, section in ipairs(sections) do
            if section.name:lower():find(priority, 1, true) then
                return i
            end
        end
    end
    -- Fall back to first section with items
    for i, section in ipairs(sections) do
        if #section.items > 0 then
            return i
        end
    end
    return 1
end

-- Parse the entire TODO file
local function parse_todo(content)
    local sections = {}
    local current_section = nil
    local line_num = 0

    for line in content:gmatch("([^\n]*)\n?") do
        line_num = line_num + 1

        local header = parse_header(line)
        if header then
            if current_section then
                table.insert(sections, current_section)
            end
            current_section = {
                name = header.name,
                level = header.level,
                header_line = line_num,
                items = {},
                uses_checkbox = false
            }
        elseif current_section then
            local item = parse_item(line)
            if item then
                item.line_num = line_num
                table.insert(current_section.items, item)
                if line:match("%[[ xX]%]") then
                    current_section.uses_checkbox = true
                end
            end
        end
    end

    if current_section then
        table.insert(sections, current_section)
    end

    return sections
end

-- Format a new item based on section format
local function format_item(text, uses_checkbox)
    if uses_checkbox then
        return "- [ ] " .. text
    else
        return "- " .. text
    end
end

-- Find item by fuzzy text match
local function find_item(section, query)
    local query_lower = query:lower()
    local matches = {}

    for _, item in ipairs(section.items) do
        if item.text:lower():find(query_lower, 1, true) then
            table.insert(matches, item)
        end
    end

    if #matches == 0 then
        return nil, "No item matching '" .. query .. "' found"
    elseif #matches > 1 then
        local msg = "Multiple items match '" .. query .. "'. Be more specific:\n"
        for i, item in ipairs(matches) do
            msg = msg .. "  " .. i .. ". " .. item.text .. "\n"
        end
        return nil, msg
    end

    return matches[1], nil
end

-- Main
local action = args[1] or "list"
local todo_file = find_todo_file()

if not todo_file then
    print("No todo file found. Looked for: TODO.md, TASKS.md, etc.")
    os.exit(1)
end

if action == "list" then
    local result = view("@todo")
    print(result.output)

elseif action == "add" then
    local text = table.concat(args, " ", 2)
    if text == "" then
        print("Usage: moss @todo add <task>")
        os.exit(1)
    end

    local content = read_file(todo_file)
    local sections = parse_todo(content)

    if #sections == 0 then
        print("No sections found in " .. todo_file)
        os.exit(1)
    end

    local section = sections[find_primary_section(sections)]
    local new_item = format_item(text, section.uses_checkbox)

    -- Find insertion point (after last item or after header)
    local insert_after = section.header_line
    if #section.items > 0 then
        insert_after = section.items[#section.items].line_num
    end

    -- Build new content
    local lines = {}
    local line_num = 0
    for line in content:gmatch("([^\n]*)\n?") do
        line_num = line_num + 1
        table.insert(lines, line)
        if line_num == insert_after then
            table.insert(lines, new_item)
        end
    end

    write_file(todo_file, table.concat(lines, "\n"))
    print("Added to " .. section.name .. ": " .. text)

elseif action == "done" then
    local query = table.concat(args, " ", 2)
    if query == "" then
        print("Usage: moss @todo done <query>")
        os.exit(1)
    end

    local content = read_file(todo_file)
    local sections = parse_todo(content)
    local section = sections[find_primary_section(sections)]

    local item, err = find_item(section, query)
    if not item then
        print("Error: " .. err)
        os.exit(1)
    end

    -- Build new content with item marked done
    local lines = {}
    local line_num = 0
    for line in content:gmatch("([^\n]*)\n?") do
        line_num = line_num + 1
        if line_num == item.line_num then
            -- Transform line to mark as done
            local new_line = line
            if line:match("%- %[ %]") then
                new_line = line:gsub("%- %[ %]", "- [x]")
            elseif line:match("^(%s*)%- ") then
                local indent = line:match("^(%s*)")
                local rest = line:match("^%s*%- (.+)$")
                new_line = indent .. "- [x] " .. rest
            end
            table.insert(lines, new_line)
        else
            table.insert(lines, line)
        end
    end

    write_file(todo_file, table.concat(lines, "\n"))
    print("Marked done: " .. item.text)

elseif action == "rm" then
    local query = table.concat(args, " ", 2)
    if query == "" then
        print("Usage: moss @todo rm <query>")
        os.exit(1)
    end

    local content = read_file(todo_file)
    local sections = parse_todo(content)
    local section = sections[find_primary_section(sections)]

    local item, err = find_item(section, query)
    if not item then
        print("Error: " .. err)
        os.exit(1)
    end

    -- Build new content without the item
    local lines = {}
    local line_num = 0
    for line in content:gmatch("([^\n]*)\n?") do
        line_num = line_num + 1
        if line_num ~= item.line_num then
            table.insert(lines, line)
        end
    end

    write_file(todo_file, table.concat(lines, "\n"))
    print("Removed: " .. item.text)

elseif action == "clean" then
    local content = read_file(todo_file)
    local sections = parse_todo(content)

    -- Collect line numbers of done items
    local done_lines = {}
    for _, section in ipairs(sections) do
        for _, item in ipairs(section.items) do
            if item.done then
                done_lines[item.line_num] = true
            end
        end
    end

    local count = 0
    for _ in pairs(done_lines) do count = count + 1 end

    if count == 0 then
        print("No completed items to remove")
        os.exit(0)
    end

    -- Build new content without done items
    local lines = {}
    local line_num = 0
    for line in content:gmatch("([^\n]*)\n?") do
        line_num = line_num + 1
        if not done_lines[line_num] then
            table.insert(lines, line)
        end
    end

    write_file(todo_file, table.concat(lines, "\n"))
    print("Removed " .. count .. " completed item(s)")

else
    print("Unknown action: " .. action)
    print("Usage: moss @todo [list|add|done|rm|clean] [args...]")
    os.exit(1)
end
