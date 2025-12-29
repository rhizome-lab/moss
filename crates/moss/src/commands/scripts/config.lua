-- @config: Config file viewer/editor
-- Usage: moss @config [view|edit] [args...]

local action = args[1] or "view"

if action == "view" then
    local result = view("@config")
    print(result.output)
elseif action == "edit" then
    -- Open in editor
    local editor = os.getenv("EDITOR") or "nano"
    os.execute(editor .. " .moss/config.toml")
else
    print("Unknown action: " .. action)
    print("Usage: moss @config [view|edit]")
    os.exit(1)
end
