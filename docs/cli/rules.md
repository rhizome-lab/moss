# moss rules

Manage custom analysis rules - add from URLs, list, update, and remove.

## Subcommands

### add

Add a rule from a URL:

```bash
moss rules add https://example.com/rules/no-console-log.scm
moss rules add https://example.com/rules/require-error-handling.scm --global
```

Options:
- `--global` - Install to global rules (~/.config/moss/rules/) instead of project

The rule file must have TOML frontmatter with an `id` field:

```scheme
# ---
# id = "no-console-log"
# severity = "warning"
# message = "Avoid console.log in production code"
# ---

(call_expression
  function: (member_expression
    object: (identifier) @obj
    property: (property_identifier) @prop)
  (#eq? @obj "console")
  (#eq? @prop "log")) @match
```

### list

List installed rules:

```bash
moss rules list
moss rules list --sources  # Show source URLs
moss rules list --json
```

Output:
```
[project] no-console-log (from https://example.com/rules/no-console-log.scm)
[project] no-todo-comment (local)
[global] require-tests

3 rule(s) installed
```

### update

Update imported rules from their source URLs:

```bash
moss rules update              # Update all imported rules
moss rules update no-console-log  # Update specific rule
```

Only rules with tracked sources (added via URL) will be updated. Local rules are skipped.

### remove

Remove an imported rule:

```bash
moss rules remove no-console-log
```

This removes both the rule file and its entry in the lock file.

## Lock File

Imported rules are tracked in `.moss/rules.lock` (project) or `~/.config/moss/rules.lock` (global):

```toml
[rules.no-console-log]
source = "https://example.com/rules/no-console-log.scm"
sha256 = "abc123..."
added = "2024-01-15"
```

## Examples

Import a rule from GitHub:

```bash
moss rules add https://raw.githubusercontent.com/org/repo/main/rules/security.scm
```

Check what rules are installed:

```bash
moss rules list --sources
```

Update all rules to latest versions:

```bash
moss rules update
```

## See Also

- [analyze](analyze.md) - Run analysis with rules
- [Rule Writing Guide](/guide/rules) - Create custom rules
