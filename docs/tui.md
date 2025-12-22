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

## Modal Keybinds

Context-aware keybinds that change based on mode.

### Design

**Principle:** Mode bindings extend global bindings. Same key in mode overrides global.

**Protocol extension:**
```python
class TUIMode(Protocol):
    name: str
    color: str
    placeholder: str
    bindings: list[Binding]  # NEW: mode-specific bindings (optional, default [])

    async def on_enter(self, app: MossTUI) -> None: ...
```

**MossTUI changes:**
```python
class MossTUI(App):
    BINDINGS: ClassVar[list[Binding]] = [...]  # Global bindings (unchanged)
    _current_mode: TUIMode

    @property
    def active_bindings(self) -> list[Binding]:
        """Merge global + mode bindings. Mode overrides global on key conflict."""
        mode_bindings = getattr(self._current_mode, 'bindings', [])
        if not mode_bindings:
            return self.BINDINGS

        # Build lookup: key -> binding
        result = {b.key: b for b in self.BINDINGS}
        for b in mode_bindings:
            result[b.key] = b  # Override on conflict
        return list(result.values())
```

**KeybindBar changes:**
```python
class KeybindBar(Static):
    def render(self) -> str:
        # Use active_bindings instead of BINDINGS
        for binding in self.app.active_bindings:
            ...

    def watch_mode(self) -> None:
        """Re-render when mode changes."""
        self.refresh()
```

**Reactive update:** MossTUI.set_mode() calls KeybindBar.refresh() after mode change.

### Example Mode Bindings

```python
class DiffMode:
    name = "DIFF"
    color = "magenta"
    placeholder = "Review changes..."
    bindings = [
        Binding("r", "revert_hunk", "Revert"),
        Binding("a", "accept_hunk", "Accept"),
        Binding("n", "next_hunk", "Next"),
        Binding("p", "prev_hunk", "Prev"),
    ]
```

### Implementation Steps

1. Add `bindings: list[Binding] = []` to all mode classes
2. Add `active_bindings` property to MossTUI
3. Update KeybindBar to use `active_bindings`
4. Add refresh call in mode switching logic
5. Define mode-specific bindings for each mode

### Contexts Not Yet Addressed

These could be future extensions:
- **Node type**: Different for file vs directory vs symbol
- **Input focus**: Different when command input is active
- **View state**: Different for expanded vs collapsed

For now, mode-level bindings cover the primary use case.
