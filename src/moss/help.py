"""Help system for moss CLI.

Provides structured command metadata, categories, and examples.
This module is the source of truth for CLI documentation.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    pass


@dataclass
class CommandExample:
    """An example command invocation."""

    command: str
    description: str


@dataclass
class CommandHelp:
    """Help information for a command."""

    name: str
    summary: str
    description: str = ""
    examples: list[CommandExample] = field(default_factory=list)
    see_also: list[str] = field(default_factory=list)


# Command categories - order matters for display
CATEGORIES: dict[str, list[str]] = {
    "Project Setup": ["init", "config", "distros"],
    "Structure Analysis": [
        "skeleton",
        "tree",
        "anchors",
        "query",
        "cfg",
        "deps",
        "context",
    ],
    "Code Quality": [
        "complexity",
        "clones",
        "patterns",
        "weaknesses",
        "lint",
        "security",
        "rules",
    ],
    "Project Health": [
        "health",
        "report",
        "overview",
        "check-docs",
        "check-todos",
        "check-refs",
        "coverage",
    ],
    "Dependencies": ["external-deps"],
    "Git Analysis": ["git-hotspots", "diff", "checkpoint"],
    "Code Generation": ["synthesize", "edit"],
    "Search": ["search", "rag"],
    "Servers & Interfaces": ["mcp-server", "acp-server", "lsp", "tui", "shell", "explore"],
    "Development Tools": ["gen", "watch", "hooks", "mutate"],
    "Task Management": ["run", "status", "pr", "roadmap"],
    "Analysis Tools": [
        "metrics",
        "summarize",
        "analyze-session",
        "telemetry",
        "extract-preferences",
        "diff-preferences",
    ],
    "Evaluation": ["eval"],
}


# Detailed help for each command
COMMANDS: dict[str, CommandHelp] = {
    # === Project Setup ===
    "init": CommandHelp(
        name="init",
        summary="Initialize a moss project",
        description="Create a moss_config.py file and .moss directory for a project.",
        examples=[
            CommandExample("moss init", "Initialize current directory"),
            CommandExample("moss init ~/myproject", "Initialize specific directory"),
            CommandExample("moss init --distro rust", "Use rust distro as base"),
            CommandExample("moss init --force", "Overwrite existing config"),
        ],
        see_also=["config", "distros"],
    ),
    "config": CommandHelp(
        name="config",
        summary="Show or validate configuration",
        description="Display current configuration or validate it for errors.",
        examples=[
            CommandExample("moss config", "Show current config"),
            CommandExample("moss config --validate", "Validate configuration"),
            CommandExample("moss config --list-distros", "List available distros"),
        ],
        see_also=["init", "distros"],
    ),
    "distros": CommandHelp(
        name="distros",
        summary="List available configuration distros",
        description="Show all built-in configuration templates (Python, Rust, etc.).",
        examples=[
            CommandExample("moss distros", "List all distros"),
        ],
        see_also=["init", "config"],
    ),
    # === Structure Analysis ===
    "skeleton": CommandHelp(
        name="skeleton",
        summary="Extract code skeleton (functions, classes, methods)",
        description="Show the structural outline of Python code without implementation details.",
        examples=[
            CommandExample("moss skeleton src/main.py", "Show skeleton of a file"),
            CommandExample("moss skeleton src/", "Show skeleton of all Python files"),
            CommandExample("moss skeleton src/ --public-only", "Only public symbols"),
            CommandExample("moss skeleton src/ --pattern '*.py'", "Custom glob pattern"),
        ],
        see_also=["anchors", "query", "deps"],
    ),
    "tree": CommandHelp(
        name="tree",
        summary="Show git-aware file tree",
        description="Display directory structure, respecting .gitignore by default.",
        examples=[
            CommandExample("moss tree", "Show tree of current directory"),
            CommandExample("moss tree src/", "Show tree of specific directory"),
            CommandExample("moss tree --tracked", "Only git-tracked files"),
            CommandExample("moss tree --all", "Ignore .gitignore"),
        ],
        see_also=["skeleton"],
    ),
    "anchors": CommandHelp(
        name="anchors",
        summary="Find anchors (functions, classes, methods) in code",
        description="List all named code elements with their locations.",
        examples=[
            CommandExample("moss anchors src/api.py", "List anchors in file"),
            CommandExample("moss anchors src/ --type function", "Only functions"),
            CommandExample("moss anchors src/ --name 'test_*'", "Filter by name pattern"),
        ],
        see_also=["skeleton", "query"],
    ),
    "query": CommandHelp(
        name="query",
        summary="Query code with pattern matching and filters",
        description="Find code elements matching various criteria.",
        examples=[
            CommandExample("moss query src/ --name 'process_*'", "Find by name pattern"),
            CommandExample(
                "moss query src/ --type class --inherits BaseAPI", "Classes inheriting from BaseAPI"
            ),
            CommandExample("moss query src/ --min-lines 50", "Functions with 50+ lines"),
            CommandExample("moss query src/ --group-by file", "Group by file"),
        ],
        see_also=["anchors", "skeleton"],
    ),
    "cfg": CommandHelp(
        name="cfg",
        summary="Build and display control flow graph",
        description="Visualize the control flow of functions.",
        examples=[
            CommandExample("moss cfg src/main.py", "Show CFG for all functions"),
            CommandExample("moss cfg src/main.py process", "CFG for specific function"),
            CommandExample("moss cfg src/main.py --dot", "Output in DOT format"),
            CommandExample("moss cfg src/main.py --mermaid", "Output in Mermaid format"),
            CommandExample("moss cfg src/main.py --html -o flow.html", "Save as HTML"),
        ],
        see_also=["skeleton", "complexity"],
    ),
    "deps": CommandHelp(
        name="deps",
        summary="Extract dependencies (imports and exports)",
        description="Analyze import/export relationships between modules.",
        examples=[
            CommandExample("moss deps src/api.py", "Show imports/exports for file"),
            CommandExample("moss deps src/", "Analyze all files in directory"),
            CommandExample("moss deps src/api.py --reverse", "Find what imports this module"),
            CommandExample(
                "moss deps src/ --dot | dot -Tpng > deps.png", "Generate dependency graph"
            ),
        ],
        see_also=["external-deps", "skeleton"],
    ),
    "context": CommandHelp(
        name="context",
        summary="Generate compiled context (skeleton + deps + summary)",
        description="Create a comprehensive context view for a file.",
        examples=[
            CommandExample("moss context src/api.py", "Generate context for file"),
        ],
        see_also=["skeleton", "deps"],
    ),
    # === Code Quality ===
    "complexity": CommandHelp(
        name="complexity",
        summary="Analyze cyclomatic complexity of functions",
        description="Find complex functions that may need refactoring.",
        examples=[
            CommandExample("moss complexity src/", "Analyze all Python files"),
            CommandExample("moss complexity src/ --threshold 10", "Show only complex functions"),
            CommandExample("moss complexity src/ --json", "Output as JSON"),
            CommandExample("moss complexity src/ --sort", "Sort by complexity"),
        ],
        see_also=["cfg", "weaknesses"],
    ),
    "clones": CommandHelp(
        name="clones",
        summary="Detect structural clones via AST hashing",
        description="Find code duplication at the structural level.",
        examples=[
            CommandExample("moss clones src/", "Detect clones in directory"),
            CommandExample("moss clones src/ --level 1", "Include literal variations"),
            CommandExample("moss clones src/ --min-lines 5", "Minimum 5 lines"),
        ],
        see_also=["patterns", "complexity"],
    ),
    "patterns": CommandHelp(
        name="patterns",
        summary="Detect architectural patterns in the codebase",
        description="Find patterns like plugins, factories, singletons, strategies.",
        examples=[
            CommandExample("moss patterns src/", "Detect all patterns"),
            CommandExample("moss patterns src/ --json", "Output as JSON"),
        ],
        see_also=["weaknesses", "clones"],
    ),
    "weaknesses": CommandHelp(
        name="weaknesses",
        summary="Identify architectural weaknesses and gaps",
        description="Find coupling issues, missing abstractions, hardcoded values, etc.",
        examples=[
            CommandExample("moss weaknesses src/", "Analyze for weaknesses"),
            CommandExample("moss weaknesses src/ --fix", "Apply auto-fixes where possible"),
            CommandExample("moss weaknesses src/ --sarif out.sarif", "SARIF output for CI"),
        ],
        see_also=["patterns", "complexity", "lint"],
    ),
    "lint": CommandHelp(
        name="lint",
        summary="Run unified linting across multiple tools",
        description="Run configured linters (ruff, etc.) with unified output.",
        examples=[
            CommandExample("moss lint", "Lint current directory"),
            CommandExample("moss lint src/", "Lint specific directory"),
            CommandExample("moss lint --fix", "Auto-fix issues"),
        ],
        see_also=["rules", "weaknesses"],
    ),
    "security": CommandHelp(
        name="security",
        summary="Run security analysis with multiple tools",
        description="Check for security vulnerabilities using bandit, semgrep, etc.",
        examples=[
            CommandExample("moss security src/", "Run security analysis"),
            CommandExample("moss security src/ --min-severity high", "Only high+ severity"),
            CommandExample("moss security src/ --tools bandit,semgrep", "Specific tools"),
        ],
        see_also=["lint", "weaknesses"],
    ),
    "rules": CommandHelp(
        name="rules",
        summary="Check code against custom rules",
        description="Run user-defined structural rules with multiple backends.",
        examples=[
            CommandExample("moss rules src/", "Check all rules"),
            CommandExample("moss rules src/ --list", "List available rules"),
            CommandExample("moss rules src/ --sarif out.sarif", "SARIF output for CI"),
        ],
        see_also=["lint", "weaknesses"],
    ),
    # === Project Health ===
    "health": CommandHelp(
        name="health",
        summary="Show project health and what needs attention",
        description="Quick health score with actionable next steps.",
        examples=[
            CommandExample("moss health", "Check current project"),
            CommandExample("moss health --json", "JSON output"),
        ],
        see_also=["report", "overview"],
    ),
    "report": CommandHelp(
        name="report",
        summary="Generate comprehensive project report",
        description="Detailed health report with all metrics.",
        examples=[
            CommandExample("moss report", "Full project report"),
            CommandExample("moss report --json", "JSON output"),
        ],
        see_also=["health", "overview"],
    ),
    "overview": CommandHelp(
        name="overview",
        summary="Run multiple checks and output combined results",
        description="Combined view of health, docs, todos, and more.",
        examples=[
            CommandExample("moss overview", "Full overview"),
        ],
        see_also=["health", "report"],
    ),
    "check-docs": CommandHelp(
        name="check-docs",
        summary="Check documentation freshness against codebase",
        description="Find stale references, missing docs, broken links.",
        examples=[
            CommandExample("moss check-docs", "Check current project"),
            CommandExample("moss check-docs --strict", "Fail on warnings"),
            CommandExample("moss check-docs --check-links", "Check internal links"),
        ],
        see_also=["check-todos", "check-refs"],
    ),
    "check-todos": CommandHelp(
        name="check-todos",
        summary="Check TODOs against implementation status",
        description="Find orphaned TODOs, track completion.",
        examples=[
            CommandExample("moss check-todos", "Check current project"),
            CommandExample("moss check-todos --strict", "Fail on orphans"),
        ],
        see_also=["check-docs", "roadmap"],
    ),
    "check-refs": CommandHelp(
        name="check-refs",
        summary="Check bidirectional references between code and docs",
        description="Verify that code references docs and vice versa.",
        examples=[
            CommandExample("moss check-refs", "Check references"),
            CommandExample("moss check-refs --staleness-days 60", "Custom staleness threshold"),
        ],
        see_also=["check-docs"],
    ),
    "coverage": CommandHelp(
        name="coverage",
        summary="Show test coverage statistics",
        description="Display pytest-cov coverage data.",
        examples=[
            CommandExample("moss coverage", "Show coverage stats"),
            CommandExample("moss coverage --json", "JSON output"),
        ],
        see_also=["mutate"],
    ),
    # === Dependencies ===
    "external-deps": CommandHelp(
        name="external-deps",
        summary="Analyze external dependencies",
        description="Check dependencies from pyproject.toml/requirements.txt.",
        examples=[
            CommandExample("moss external-deps", "List dependencies"),
            CommandExample("moss external-deps --vulns", "Check vulnerabilities"),
            CommandExample("moss external-deps --licenses", "Check licenses"),
            CommandExample("moss external-deps --resolve", "Full dependency tree"),
        ],
        see_also=["deps"],
    ),
    # === Git Analysis ===
    "git-hotspots": CommandHelp(
        name="git-hotspots",
        summary="Find frequently changed files in git history",
        description="Identify files that change often (potential refactoring candidates).",
        examples=[
            CommandExample("moss git-hotspots", "Show hotspots (90 days)"),
            CommandExample("moss git-hotspots --days 30", "Last 30 days"),
            CommandExample("moss git-hotspots --limit 20", "Top 20 files"),
        ],
        see_also=["diff", "complexity"],
    ),
    "diff": CommandHelp(
        name="diff",
        summary="Analyze git diff and show symbol changes",
        description="Structural diff showing which functions/classes changed.",
        examples=[
            CommandExample("moss diff", "Diff against HEAD"),
            CommandExample("moss diff HEAD~3", "Diff against 3 commits ago"),
            CommandExample("moss diff main", "Diff against main branch"),
        ],
        see_also=["git-hotspots"],
    ),
    "checkpoint": CommandHelp(
        name="checkpoint",
        summary="Manage checkpoints for safe code modifications",
        description="Create, list, merge, or abort shadow git checkpoints.",
        examples=[
            CommandExample("moss checkpoint create", "Create checkpoint"),
            CommandExample("moss checkpoint list", "List checkpoints"),
            CommandExample("moss checkpoint diff my-checkpoint", "Show changes"),
            CommandExample("moss checkpoint merge my-checkpoint", "Merge changes"),
            CommandExample("moss checkpoint abort my-checkpoint", "Discard changes"),
        ],
        see_also=["git-hotspots"],
    ),
    # === Code Generation ===
    "synthesize": CommandHelp(
        name="synthesize",
        summary="Synthesize code from specification",
        description="Generate code using examples, types, and constraints.",
        examples=[
            CommandExample('moss synthesize "Add two numbers"', "Basic synthesis"),
            CommandExample(
                'moss synthesize "Sort a list" --type "List[int] -> List[int]"',
                "With type signature",
            ),
            CommandExample(
                'moss synthesize "Reverse" -e "hello" "olleh" -e "ab" "ba"',
                "With examples",
            ),
            CommandExample(
                'moss synthesize "Build REST API" --dry-run',
                "Preview decomposition",
            ),
        ],
        see_also=["edit"],
    ),
    "edit": CommandHelp(
        name="edit",
        summary="Edit code with intelligent complexity routing",
        description="Apply structural edits using the appropriate method.",
        examples=[
            CommandExample('moss edit src/api.py "Add error handling"', "Edit with instruction"),
        ],
        see_also=["synthesize"],
    ),
    # === Search ===
    "search": CommandHelp(
        name="search",
        summary="Semantic search across codebase",
        description="Search code using natural language or patterns.",
        examples=[
            CommandExample('moss search -q "error handling"', "Search for concept"),
            CommandExample("moss search -q auth --mode embedding", "Embedding-based search"),
            CommandExample("moss search -i -q api", "Index first, then search"),
        ],
        see_also=["rag", "query"],
    ),
    "rag": CommandHelp(
        name="rag",
        summary="Semantic search with RAG indexing",
        description="Build and query a vector index for semantic search.",
        examples=[
            CommandExample("moss rag index", "Build index"),
            CommandExample('moss rag search "how does auth work"', "Query index"),
        ],
        see_also=["search"],
    ),
    # === Servers & Interfaces ===
    "mcp-server": CommandHelp(
        name="mcp-server",
        summary="Start MCP server for LLM tool access",
        description="Run the MCP server. Default: single-tool. Use --full for multi-tool.",
        examples=[
            CommandExample("moss mcp-server", "Start single-tool MCP server (token-efficient)"),
            CommandExample("moss mcp-server --full", "Start multi-tool MCP server (for IDEs)"),
        ],
        see_also=["acp-server", "lsp"],
    ),
    "acp-server": CommandHelp(
        name="acp-server",
        summary="Start ACP server for IDE integration",
        description="Run Agent Client Protocol server for Zed, JetBrains.",
        examples=[
            CommandExample("moss acp-server", "Start ACP server"),
        ],
        see_also=["mcp-server", "lsp"],
    ),
    "lsp": CommandHelp(
        name="lsp",
        summary="Start the LSP server for IDE integration",
        description="Language Server Protocol server with moss features.",
        examples=[
            CommandExample("moss lsp", "Start LSP server"),
        ],
        see_also=["mcp-server", "acp-server"],
    ),
    "tui": CommandHelp(
        name="tui",
        summary="Start the interactive terminal UI",
        description="Visual interface for exploring moss functionality.",
        examples=[
            CommandExample("moss tui", "Start TUI"),
        ],
        see_also=["shell"],
    ),
    "shell": CommandHelp(
        name="shell",
        summary="Start interactive shell",
        description="REPL for exploring codebases with tab completion and history.",
        examples=[
            CommandExample("moss shell", "Start shell in current directory"),
            CommandExample("moss shell ~/project", "Start in specific directory"),
        ],
        see_also=["explore", "tui"],
    ),
    "explore": CommandHelp(
        name="explore",
        summary="Interactive REPL for codebase exploration",
        description="REPL with tab completion, history, and code exploration commands. "
        "Commands: skeleton, deps, cfg, anchors, query, search, complexity, health, tree.",
        examples=[
            CommandExample("moss explore", "Start explore REPL"),
            CommandExample("moss explore ~/project", "Explore specific project"),
        ],
        see_also=["shell", "tui"],
    ),
    # === Development Tools ===
    "gen": CommandHelp(
        name="gen",
        summary="Generate interface code from MossAPI introspection",
        description="Generate MCP, HTTP, CLI, or other interfaces.",
        examples=[
            CommandExample("moss gen --target mcp", "Generate MCP tools"),
            CommandExample("moss gen --target http", "Generate HTTP routes"),
            CommandExample("moss gen --target openapi -o openapi.json", "Generate OpenAPI spec"),
        ],
        see_also=["mcp-server"],
    ),
    "watch": CommandHelp(
        name="watch",
        summary="Watch for file changes and re-run tests",
        description="Continuous test runner on file changes.",
        examples=[
            CommandExample("moss watch", "Watch and run tests"),
            CommandExample("moss watch --pattern 'tests/*.py'", "Custom pattern"),
        ],
        see_also=["coverage"],
    ),
    "hooks": CommandHelp(
        name="hooks",
        summary="Manage git pre-commit hooks",
        description="Install, remove, or run git hooks.",
        examples=[
            CommandExample("moss hooks install", "Install hooks"),
            CommandExample("moss hooks run", "Run hooks manually"),
        ],
        see_also=["lint"],
    ),
    "mutate": CommandHelp(
        name="mutate",
        summary="Run mutation testing to find undertested code",
        description="Introduce mutations to find weak tests.",
        examples=[
            CommandExample("moss mutate src/", "Run mutation testing"),
        ],
        see_also=["coverage"],
    ),
    # === Task Management ===
    "run": CommandHelp(
        name="run",
        summary="Run a task through moss",
        description="Execute a task with the moss agent system.",
        examples=[
            CommandExample('moss run "Fix the bug in api.py"', "Run a task"),
            CommandExample('moss run "Add tests" --wait', "Wait for completion"),
            CommandExample('moss run "Refactor" --priority high', "High priority"),
        ],
        see_also=["status"],
    ),
    "status": CommandHelp(
        name="status",
        summary="Show status of moss tasks and workers",
        description="Check on running tasks.",
        examples=[
            CommandExample("moss status", "Show status"),
        ],
        see_also=["run"],
    ),
    "pr": CommandHelp(
        name="pr",
        summary="Generate PR review summary",
        description="Analyze a pull request and generate summary.",
        examples=[
            CommandExample("moss pr", "Review current branch"),
            CommandExample("moss pr --base main", "Compare against main"),
        ],
        see_also=["diff"],
    ),
    "roadmap": CommandHelp(
        name="roadmap",
        summary="Show project roadmap and progress from TODO.md",
        description="Parse TODO.md and show progress.",
        examples=[
            CommandExample("moss roadmap", "Show roadmap"),
        ],
        see_also=["check-todos"],
    ),
    # === Analysis Tools ===
    "metrics": CommandHelp(
        name="metrics",
        summary="Generate codebase metrics dashboard",
        description="Comprehensive metrics about the codebase.",
        examples=[
            CommandExample("moss metrics", "Show metrics"),
            CommandExample("moss metrics --json", "JSON output"),
        ],
        see_also=["health", "complexity"],
    ),
    "summarize": CommandHelp(
        name="summarize",
        summary="Generate hierarchical codebase summary",
        description="Create a structural summary of code or documentation.",
        examples=[
            CommandExample("moss summarize", "Summarize current directory"),
            CommandExample("moss summarize src/", "Summarize specific directory"),
            CommandExample("moss summarize --docs", "Summarize documentation"),
            CommandExample("moss summarize --include-private", "Include private symbols"),
        ],
        see_also=["skeleton", "health"],
    ),
    "analyze-session": CommandHelp(
        name="analyze-session",
        summary="Analyze a Claude Code session log",
        description="Parse and analyze session logs for insights.",
        examples=[
            CommandExample("moss analyze-session ~/session.jsonl", "Analyze session"),
        ],
        see_also=["telemetry", "extract-preferences"],
    ),
    "telemetry": CommandHelp(
        name="telemetry",
        summary="Show aggregate telemetry across sessions",
        description="Analyze session telemetry. Supports moss sessions and Claude Code logs.",
        examples=[
            CommandExample("moss telemetry", "Show aggregate stats for all moss sessions"),
            CommandExample("moss telemetry -s abc123", "Show stats for specific session"),
            CommandExample("moss telemetry -l *.jsonl", "Analyze Claude Code session logs"),
            CommandExample("moss telemetry -l logs/ --html", "HTML dashboard output"),
        ],
        see_also=["analyze-session", "metrics"],
    ),
    "extract-preferences": CommandHelp(
        name="extract-preferences",
        summary="Extract user preferences from session logs",
        description="Learn preferences from past sessions.",
        examples=[
            CommandExample("moss extract-preferences ~/sessions/", "Extract from sessions"),
        ],
        see_also=["analyze-session", "diff-preferences"],
    ),
    "diff-preferences": CommandHelp(
        name="diff-preferences",
        summary="Compare two preference extractions",
        description="Show how preferences evolved.",
        examples=[
            CommandExample("moss diff-preferences old.json new.json", "Compare preferences"),
        ],
        see_also=["extract-preferences"],
    ),
    # === Evaluation ===
    "eval": CommandHelp(
        name="eval",
        summary="Run evaluation benchmarks",
        description="Run SWE-bench or other benchmarks.",
        examples=[
            CommandExample("moss eval swebench --subset lite", "Run SWE-bench Lite"),
        ],
        see_also=[],
    ),
}


def get_command_help(name: str) -> CommandHelp | None:
    """Get help for a specific command."""
    return COMMANDS.get(name)


def get_all_commands() -> dict[str, CommandHelp]:
    """Get all command help."""
    return COMMANDS


def get_categories() -> dict[str, list[str]]:
    """Get command categories."""
    return CATEGORIES


def get_commands_by_category() -> dict[str, list[CommandHelp]]:
    """Get commands organized by category."""
    result: dict[str, list[CommandHelp]] = {}
    for category, cmd_names in CATEGORIES.items():
        result[category] = [COMMANDS[name] for name in cmd_names if name in COMMANDS]
    return result


def format_command_help(cmd: CommandHelp, include_examples: bool = True) -> str:
    """Format command help as a string."""
    lines = [f"moss {cmd.name}", "=" * (5 + len(cmd.name)), ""]
    lines.append(cmd.summary)
    if cmd.description:
        lines.append("")
        lines.append(cmd.description)

    if include_examples and cmd.examples:
        lines.append("")
        lines.append("Examples:")
        for ex in cmd.examples:
            lines.append(f"  $ {ex.command}")
            lines.append(f"    {ex.description}")

    if cmd.see_also:
        lines.append("")
        lines.append(f"See also: {', '.join(cmd.see_also)}")

    return "\n".join(lines)


def format_category_list() -> str:
    """Format the category list for display."""
    lines = ["moss - Headless agent orchestration layer for AI engineering", ""]
    lines.append("Commands by category:")

    for category, cmd_names in CATEGORIES.items():
        lines.append("")
        lines.append(f"  {category}:")
        cmds = [COMMANDS.get(name) for name in cmd_names if name in COMMANDS]
        for cmd in cmds:
            if cmd:
                lines.append(f"    {cmd.name:20} {cmd.summary}")

    lines.append("")
    lines.append("Run 'moss help <command>' for detailed help on a command.")
    lines.append("Run 'moss <command> --help' for argument reference.")

    return "\n".join(lines)


def get_mcp_tool_description(api_name: str, method_name: str) -> str:
    """Get enhanced MCP tool description with examples.

    Maps API methods to CLI commands for better discoverability.
    """
    # Map API names to CLI commands
    cli_map = {
        ("skeleton", "extract"): "skeleton",
        ("skeleton", "format"): "skeleton",
        ("tree", "generate"): "tree",
        ("tree", "format"): "tree",
        ("anchor", "find"): "anchors",
        ("anchor", "resolve"): "anchors",
        ("dependencies", "extract"): "deps",
        ("dependencies", "analyze"): "deps",
        ("dependencies", "format"): "deps",
        ("cfg", "build"): "cfg",
        ("health", "check"): "health",
        ("health", "summarize"): "summarize",
        ("health", "check_docs"): "check-docs",
        ("health", "check_todos"): "check-todos",
        ("complexity", "analyze"): "complexity",
        ("complexity", "get_high_risk"): "complexity",
        ("ref_check", "check"): "check-refs",
        ("git_hotspots", "analyze"): "git-hotspots",
        ("git_hotspots", "get_top_hotspots"): "git-hotspots",
        ("external_deps", "analyze"): "external-deps",
        ("external_deps", "list_direct"): "external-deps",
        ("external_deps", "check_security"): "external-deps",
    }

    cli_cmd = cli_map.get((api_name, method_name))
    if cli_cmd and cli_cmd in COMMANDS:
        cmd = COMMANDS[cli_cmd]
        # Return enhanced description with example
        if cmd.examples:
            return f"{cmd.summary}. Example: {cmd.examples[0].command}"
        return cmd.summary

    return ""


__all__ = [
    "CATEGORIES",
    "COMMANDS",
    "CommandExample",
    "CommandHelp",
    "format_category_list",
    "format_command_help",
    "get_all_commands",
    "get_categories",
    "get_command_help",
    "get_commands_by_category",
    "get_mcp_tool_description",
]
