-- @todo: TODO file viewer/editor using tree-sitter
-- Usage: moss @todo [list|add|done|rm|clean] [args...]

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

-- Priority section names (lowercase for matching)
local PRIMARY_SECTIONS = {"next up", "next", "todo", "tasks", "in progress", "current", "active"}

-- Check if a section name matches a primary section
local function is_primary_section(name)
    local lower = name:lower()
    for _, pattern in ipairs(PRIMARY_SECTIONS) do
        if lower:find(pattern, 1, true) then
            return true
        end
    end
    return false
end

-- Get heading text from an atx_heading node
local function get_heading_text(heading_node)
    for _, child in ipairs(heading_node:named_children()) do
        if child:kind() == "inline" then
            return child:text()
        end
    end
    return ""
end

-- Check if a list_item is a task (has checkbox)
local function get_task_state(item_node)
    for _, child in ipairs(item_node:children()) do
        local kind = child:kind()
        if kind == "task_list_marker_checked" then
            return "done"
        elseif kind == "task_list_marker_unchecked" then
            return "pending"
        end
    end
    return nil -- not a task item (plain list item)
end

-- Get the text content of a list item (from paragraph/inline)
local function get_item_text(item_node)
    for _, child in ipairs(item_node:named_children()) do
        if child:kind() == "paragraph" then
            for _, gc in ipairs(child:named_children()) do
                if gc:kind() == "inline" then
                    return gc:text():gsub("^%s+", ""):gsub("%s+$", "")
                end
            end
        end
    end
    return ""
end

-- Parse TODO file into structured sections
local function parse_todo(content)
    local tree = ts.parse(content, "markdown")
    local root = tree:root()

    local sections = {}

    local function process_section(section_node)
        local section = {
            name = "",
            level = 0,
            start_row = section_node:start_row(),
            end_row = section_node:end_row(),
            items = {},
            uses_checkbox = false
        }

        for _, child in ipairs(section_node:named_children()) do
            local kind = child:kind()

            if kind == "atx_heading" then
                section.name = get_heading_text(child)
                -- Get level from marker
                for _, hc in ipairs(child:named_children()) do
                    if hc:kind():match("^atx_h%d_marker$") then
                        section.level = tonumber(hc:kind():match("h(%d)")) or 1
                        break
                    end
                end

            elseif kind == "list" then
                for _, item in ipairs(child:named_children()) do
                    if item:kind() == "list_item" then
                        local state = get_task_state(item)
                        local text = get_item_text(item)
                        if state then
                            section.uses_checkbox = true
                        end
                        table.insert(section.items, {
                            text = text,
                            done = state == "done",
                            start_row = item:start_row(),
                            end_row = item:end_row(),
                            is_task = state ~= nil
                        })
                    end
                end

            elseif kind == "section" then
                -- Nested section - recurse
                process_section(child)
            end
        end

        if section.name ~= "" then
            table.insert(sections, section)
        end
    end

    -- Start from document root
    for _, child in ipairs(root:named_children()) do
        if child:kind() == "section" then
            process_section(child)
        end
    end

    return sections
end

-- Find primary section
local function find_primary_section(sections)
    -- First, look for explicitly primary sections
    for _, section in ipairs(sections) do
        if is_primary_section(section.name) then
            return section
        end
    end
    -- Fall back to first section with items
    for _, section in ipairs(sections) do
        if #section.items > 0 then
            return section
        end
    end
    -- Fall back to first section
    return sections[1]
end

-- Find item by fuzzy match
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

-- Read file as lines
local function read_lines(path)
    local content = read_file(path)
    local lines = {}
    for line in (content .. "\n"):gmatch("([^\n]*)\n") do
        table.insert(lines, line)
    end
    return lines
end

-- Write lines to file
local function write_lines(path, lines)
    write_file(path, table.concat(lines, "\n"))
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

    local section = find_primary_section(sections)
    local lines = read_lines(todo_file)

    -- Format new item
    local new_item
    if section.uses_checkbox then
        new_item = "- [ ] " .. text
    else
        new_item = "- " .. text
    end

    -- Find insertion point (after last item or after header)
    local insert_after
    if #section.items > 0 then
        insert_after = section.items[#section.items].end_row
    else
        -- Insert after section header (start_row is the section, need to find header end)
        insert_after = section.start_row
    end

    -- Insert the new item
    table.insert(lines, insert_after + 1, new_item)
    write_lines(todo_file, lines)
    print("Added to " .. section.name .. ": " .. text)

elseif action == "done" then
    local query = table.concat(args, " ", 2)
    if query == "" then
        print("Usage: moss @todo done <query>")
        os.exit(1)
    end

    local content = read_file(todo_file)
    local sections = parse_todo(content)
    local section = find_primary_section(sections)

    if not section then
        print("No sections found")
        os.exit(1)
    end

    local item, err = find_item(section, query)
    if not item then
        print("Error: " .. err)
        os.exit(1)
    end

    local lines = read_lines(todo_file)
    local line = lines[item.start_row]

    -- Mark as done
    if line:match("%- %[ %]") then
        lines[item.start_row] = line:gsub("%- %[ %]", "- [x]")
    elseif line:match("^(%s*)%- ") then
        -- Plain list item - add checkbox
        lines[item.start_row] = line:gsub("^(%s*)%- ", "%1- [x] ")
    end

    write_lines(todo_file, lines)
    print("Marked done: " .. item.text)

elseif action == "rm" then
    local query = table.concat(args, " ", 2)
    if query == "" then
        print("Usage: moss @todo rm <query>")
        os.exit(1)
    end

    local content = read_file(todo_file)
    local sections = parse_todo(content)
    local section = find_primary_section(sections)

    if not section then
        print("No sections found")
        os.exit(1)
    end

    local item, err = find_item(section, query)
    if not item then
        print("Error: " .. err)
        os.exit(1)
    end

    local lines = read_lines(todo_file)

    -- Remove lines for this item
    for i = item.end_row, item.start_row, -1 do
        table.remove(lines, i)
    end

    write_lines(todo_file, lines)
    print("Removed: " .. item.text)

elseif action == "clean" then
    local content = read_file(todo_file)
    local sections = parse_todo(content)

    -- Collect all done items (in reverse order for safe removal)
    local to_remove = {}
    for _, section in ipairs(sections) do
        for _, item in ipairs(section.items) do
            if item.done then
                table.insert(to_remove, item)
            end
        end
    end

    if #to_remove == 0 then
        print("No completed items to remove")
        os.exit(0)
    end

    -- Sort by start_row descending (remove from bottom up)
    table.sort(to_remove, function(a, b) return a.start_row > b.start_row end)

    local lines = read_lines(todo_file)
    for _, item in ipairs(to_remove) do
        for i = item.end_row, item.start_row, -1 do
            table.remove(lines, i)
        end
    end

    write_lines(todo_file, lines)
    print("Removed " .. #to_remove .. " completed item(s)")

else
    print("Unknown action: " .. action)
    print("Usage: moss @todo [list|add|done|rm|clean] [args...]")
    os.exit(1)
end
