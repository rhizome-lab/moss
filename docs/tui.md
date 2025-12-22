# TUI Development Notes

Textual framework conventions and lessons learned.

## Markup Escaping

Textual uses Rich markup but with its own escaping rules:
- Escape literal brackets with `\[` not `[[` (Rich style) or `rich.markup.escape()`
- Example: `\[Q]uit` displays as `[Q]uit`

## Actions and Click Handlers

- `@click=app.foo` calls method `action_foo` on the app
- Always use `action_` prefix for methods called via `@click`
- Action methods can take string arguments: `@click=app.cd_to('/path')`

## Styling Clickable Text

- Bold/underline markup inside `@click` regions splits the hover background
- Don't style text inside click handlers: `[@click=app.quit]Quit[/]` not `[@click=app.quit][b]Q[/b]uit[/]`
- Each markup tag creates a separate hover region

## Theme System

- Use `self.theme` for Textual's built-in theme system
- Don't hardcode theme names like "github-dark"
- Watch theme changes with `watch_theme(self, theme: str)` method

## Tree Widget

- `on_tree_node_selected` fires on single click/enter, not double-click
- Implement double-click manually with timestamp tracking
- `on_tree_node_highlighted` fires on cursor movement (hover/arrow keys)
- Indentation controlled by `guide_depth` property (default 4), not CSS
- Set `self.guide_depth = 2` in `__init__` for minimal indent

## Command Palette

- Built-in action is `command_palette` not `action_command_palette`
- Add custom commands via `get_system_commands(self, screen)` method
- Yields `SystemCommand(title, help, callback)` from `textual.app`
- NOT `DiscoveryHit` - that's for search providers
- CSS selectors: `CommandInput` (not `Input`), `#--container`, `#--input`, `#--results`

## Markdown Files

- ViewAPI doesn't handle markdown symbols - need TUI-side handling
- Extract headings as pseudo-symbols with `kind="heading"`
- Build nested tree based on heading levels (h1 > h2 > h3)
- Store symbol object when selected for later use (can't recover from path string)
- Show section content from heading line to next same-or-higher level heading
