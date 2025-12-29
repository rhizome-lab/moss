-- @todo: TODO file viewer/editor
-- Usage: moss @todo [list|add|done|rm] [args...]
-- Note: Uses view() for listing, shell() for mutations (requires moss in PATH)

local action = args[1] or "list"

if action == "list" then
    -- List todos using view (works without moss in PATH)
    local result = view("@todo")
    print(result.output)
elseif action == "add" then
    local text = table.concat(args, " ", 2)
    if text == "" then
        print("Usage: moss @todo add <task>")
        os.exit(1)
    end
    -- Requires moss in PATH
    local result = shell("moss todo add '" .. text:gsub("'", "'\\''") .. "'")
    if not result.success then
        print("Failed to add task. Is moss in PATH?")
        os.exit(1)
    end
elseif action == "done" then
    local text = table.concat(args, " ", 2)
    if text == "" then
        print("Usage: moss @todo done <task>")
        os.exit(1)
    end
    local result = shell("moss todo done '" .. text:gsub("'", "'\\''") .. "'")
    if not result.success then
        print("Failed to mark task done. Is moss in PATH?")
        os.exit(1)
    end
elseif action == "rm" then
    local text = table.concat(args, " ", 2)
    if text == "" then
        print("Usage: moss @todo rm <task>")
        os.exit(1)
    end
    local result = shell("moss todo rm '" .. text:gsub("'", "'\\''") .. "'")
    if not result.success then
        print("Failed to remove task. Is moss in PATH?")
        os.exit(1)
    end
else
    print("Unknown action: " .. action)
    print("Usage: moss @todo [list|add|done|rm] [args...]")
    os.exit(1)
end
