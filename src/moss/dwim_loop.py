"""DWIM-Driven Agent Loop: LLM outputs terse intents, DWIM routes to tools.

Design principle: No tool schemas in LLM prompts. LLM outputs terse commands
like "skeleton foo.py" or "fix: add null check", DWIM interprets and routes.

Context model: Path-based, not conversation history. Each turn gets:
- System prompt
- Task path (root → current leaf)
- Active notes
- Last result preview

See: docs/agentic-loop.md
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass
from datetime import UTC, datetime
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING, Any

from moss.cache import EphemeralCache
from moss.dwim import CLARIFY_THRESHOLD, ToolMatch, analyze_intent, resolve_tool
from moss.task_tree import NoteExpiry, TaskTree

if TYPE_CHECKING:
    from moss.moss_api import MossAPI
    from moss.session import Session

logger = logging.getLogger(__name__)

# Max chars for result preview in context
DEFAULT_RESULT_PREVIEW_LIMIT = 500
MAX_RESULT_PREVIEW_LIMIT = 2000


class LoopState(Enum):
    """State of the agent loop."""

    RUNNING = auto()
    DONE = auto()  # Agent signaled completion
    FAILED = auto()  # Unrecoverable error
    STALLED = auto()  # No progress
    MAX_TURNS = auto()  # Hit turn limit


@dataclass
class ParsedIntent:
    """Result of parsing a terse LLM intent."""

    verb: str  # Action: skeleton, expand, fix, validate, grep, done
    target: str | None  # File path, symbol name, or None
    content: str | None  # For "fix: ..." commands, the fix description
    raw: str  # Original input
    confidence: float = 1.0


@dataclass
class TurnResult:
    """Result of a single agent turn."""

    intent: ParsedIntent
    tool_match: ToolMatch | None
    tool_output: Any
    error: str | None = None
    duration_ms: int = 0


@dataclass
class LoopConfig:
    """Configuration for the DWIM loop."""

    max_turns: int = 50
    stall_threshold: int = 5  # Max turns without progress
    confidence_threshold: float = CLARIFY_THRESHOLD  # Below this, ask for clarification
    model: str = "gemini/gemini-2.0-flash"
    temperature: float = 0.0
    system_prompt: str = ""


@dataclass
class LoopResult:
    """Result of running the DWIM loop."""

    state: LoopState
    turns: list[TurnResult]
    final_output: Any
    error: str | None = None
    total_duration_ms: int = 0


class TaskType(Enum):
    """Classification of task intent."""

    READ_ONLY = auto()  # show, explain, find, search - answer a question
    WRITE = auto()  # fix, edit, patch - modify code
    UNKNOWN = auto()  # unclear, needs more context


# Patterns that indicate read-only tasks
READ_ONLY_PATTERNS = [
    r"\b(show|display|print|list)\b",
    r"\b(what|where|which|who|how|why)\b.*\?",
    r"\b(find|search|look\s*for|locate)\b",
    r"\b(explain|describe|summarize)\b",
    r"\b(count|how\s*many)\b",
    r"\b(check|verify|confirm)\b.*\?",
    r"\b(is|are|does|do|has|have|can|will)\b.*\?",
]

# Patterns that indicate write tasks
WRITE_PATTERNS = [
    r"\b(fix|repair|correct)\b",
    r"\b(add|insert|create|implement)\b",
    r"\b(remove|delete|drop)\b",
    r"\b(change|modify|update|edit|patch)\b",
    r"\b(refactor|rewrite|restructure)\b",
    r"\b(rename|move)\b",
]


def classify_task(task: str) -> TaskType:
    """Classify a task as read-only or write.

    Args:
        task: Task description

    Returns:
        TaskType indicating the nature of the task
    """
    task_lower = task.lower()

    # Check write patterns first (more specific)
    for pattern in WRITE_PATTERNS:
        if re.search(pattern, task_lower):
            return TaskType.WRITE

    # Check read-only patterns
    for pattern in READ_ONLY_PATTERNS:
        if re.search(pattern, task_lower):
            return TaskType.READ_ONLY

    return TaskType.UNKNOWN


# Common action verbs and their canonical forms
ACTION_VERBS = {
    # Code exploration
    "skeleton": "skeleton",
    "skel": "skeleton",
    "structure": "skeleton",
    "outline": "skeleton",
    "expand": "expand",
    "show": "expand",
    "read": "expand",
    "view": "view",
    # Search
    "grep": "grep",
    "search": "search",
    "find": "find",
    "query": "query",
    # Code modification
    "fix": "fix",
    "patch": "patch",
    "edit": "edit",
    "write": "write",
    "replace": "replace",
    "insert": "insert",
    "revert": "revert",
    "diff": "diff",
    "branch": "branch",
    "analyze": "analyze",
    # Validation
    "validate": "validate",
    "check": "validate",
    "lint": "validate",
    # Dependencies
    "deps": "deps",
    "imports": "deps",
    "dependencies": "deps",
    # Call graph
    "callers": "callers",
    "callees": "callees",
    "calls": "callees",
    # Task management (meta-commands)
    "breakdown": "breakdown",
    "split": "breakdown",
    "decompose": "breakdown",
    "note": "note",
    "remember": "note",
    "fetch": "fetch",
    "getresult": "fetch",
    # Termination
    "done": "done",
    "finished": "done",
    "complete": "done",
}


def parse_intent(text: str) -> ParsedIntent:
    """Parse a terse LLM intent into structured form.

    Handles formats like:
    - "skeleton foo.py"
    - "expand Patch.apply"
    - "fix: add null check"
    - "grep 'def main' src/"
    - "done"

    Args:
        text: Raw LLM output

    Returns:
        ParsedIntent with verb, target, and optional content
    """
    text = text.strip()
    if not text:
        return ParsedIntent(verb="", target=None, content=None, raw=text, confidence=0.0)

    # Check for "fix: ..." format (verb with content)
    if ":" in text:
        parts = text.split(":", 1)
        verb_candidate = parts[0].strip().lower()
        if verb_candidate in ACTION_VERBS:
            return ParsedIntent(
                verb=ACTION_VERBS[verb_candidate],
                target=None,
                content=parts[1].strip(),
                raw=text,
            )

    # Split on whitespace
    parts = text.split(None, 1)
    first_word = parts[0].lower()

    # Check if first word is an action verb
    if first_word in ACTION_VERBS:
        verb = ACTION_VERBS[first_word]
        target = parts[1].strip() if len(parts) > 1 else None
        return ParsedIntent(verb=verb, target=target, content=None, raw=text)

    # Fallback: treat entire text as a natural language query
    # Let DWIM handle the routing
    return ParsedIntent(
        verb="query",
        target=None,
        content=text,
        raw=text,
        confidence=0.5,  # Lower confidence for unparsed input
    )


def build_tool_call(intent: ParsedIntent, api: MossAPI) -> tuple[str, dict[str, Any]]:
    """Build a tool call from parsed intent.

    Args:
        intent: Parsed intent from LLM
        api: MossAPI instance for context

    Returns:
        Tuple of (tool_name, parameters)
    """
    verb = intent.verb
    target = intent.target

    # Handle termination
    if verb == "done":
        return ("done", {})

    # Handle fix/edit (needs special treatment)
    if verb in ("fix", "patch", "edit"):
        return ("patch.apply", {"content": intent.content, "target": target})

    # Map verbs to tools
    verb_to_tool = {
        "skeleton": "skeleton.format",
        "expand": "skeleton.expand",
        "view": "view",
        "grep": "search.grep",
        "search": "search.find_symbols",
        "find": "search.find_definitions",
        "query": "query",
        "validate": "validation.validate",
        "deps": "dependencies.format",
        "callers": "callers",
        "callees": "callees",
        "write": "edit.write_file",
        "replace": "edit.replace_text",
        "insert": "edit.insert_line",
        "revert": "shadow_git.rollback_hunk",
        "diff": "shadow_git.get_diff",
        "branch": "shadow_git.switch_branch",
        "analyze": "telemetry.analyze_all_sessions",
    }

    tool_name = verb_to_tool.get(verb, verb)
    params: dict[str, Any] = {}

    if target:
        # Parse target - could be "file", "symbol", or "file symbol" / "symbol file"
        parts = target.split()

        if tool_name == "shadow_git.rollback_hunk":
            # revert <file> <line>
            if len(parts) >= 2:
                params["file_path"] = parts[0]
                try:
                    params["line"] = int(parts[1])
                except ValueError:
                    params["line"] = 0
                # Use current shadow branch if any, or default
                params["branch_name"] = "shadow/current"
        elif tool_name == "shadow_git.get_diff":
            params["branch_name"] = target if target else "shadow/current"
        elif tool_name == "skeleton.expand":
            # expand needs both file_path and symbol_name
            # Accept: "Symbol file.py" or "file.py Symbol" or just "Symbol"
            file_parts = []
            symbol_part = None
            for p in parts:
                if "/" in p or p.endswith(".py"):
                    file_parts.append(p)
                else:
                    symbol_part = p

            if file_parts and symbol_part:
                # Standard case: symbol + file(s)
                if len(file_parts) == 1:
                    params["file_path"] = file_parts[0]
                else:
                    # Multi-file: search in specified files
                    params["file_paths"] = file_parts
                params["symbol_name"] = symbol_part
            elif symbol_part and not file_parts:
                # Only symbol given: search across codebase
                tool_name = "skeleton.expand_search"
                params["symbol_name"] = symbol_part
        elif tool_name == "skeleton.format" or tool_name == "dependencies.format":
            # These take file_path
            for p in parts:
                if "/" in p or p.endswith(".py"):
                    params["file_path"] = p
                    break
            if not params:
                params["file_path"] = target
        elif tool_name == "search.grep":
            # grep <pattern> [path]
            if len(parts) >= 2:
                params["pattern"] = parts[0]
                params["path"] = parts[1]
            else:
                params["pattern"] = target
        else:
            # Generic handling
            is_file_path = "/" in target or target.endswith(".py")
            param_name = "path" if is_file_path else "symbol"
            params[param_name] = target

    return (tool_name, params)


class DWIMLoop:
    """DWIM-driven agent loop with context-excluded model.

    Context model:
    - No conversation history accumulation
    - Each turn gets: system + task path + notes + last result preview
    - State lives in TaskTree, not message list
    - ~300 tokens per turn instead of unbounded growth

    The loop:
    1. Build context from TaskTree path
    2. Get terse intent from LLM
    3. Parse intent, route via DWIM
    4. Execute tool, store result
    5. Update TaskTree state
    6. Repeat until "done" or limit
    """

    def __init__(
        self,
        api: MossAPI,
        config: LoopConfig | None = None,
        session: Session | None = None,
    ):
        self.api = api
        self.config = config or LoopConfig()
        self.session = session
        self._turns: list[TurnResult] = []
        self._task_tree: TaskTree | None = None
        self._last_result: str | None = None
        self._task_type: TaskType = TaskType.UNKNOWN
        self._successful_results: int = 0  # Count of successful tool results
        self._ephemeral_cache = EphemeralCache(
            max_entries=50,
            default_ttl=600.0,  # 10 minutes for agent session
            preview_lines=30,
        )

    def _check_mail(self) -> str | None:
        """Check session inbox for unread user feedback."""
        if not self.session:
            return None

        unread = self.session.get_unread_messages()
        if not unread:
            return None

        feedback = "\n".join(f"- {m.content}" for m in unread)
        return f"\nUser Feedback (Mid-task correction):\n{feedback}"

    def interrupt(self, reason: str = "") -> None:
        """Interrupt the running loop."""
        if self.session:
            self.session.pause(reason)

    def send_feedback(self, message: str) -> None:
        """Send feedback to the agent while it's running."""
        if self.session:
            self.session.send_message(message)

    def _build_system_prompt(self) -> str:
        """Build the system prompt for terse agent mode."""
        if self.config.system_prompt:
            return self.config.system_prompt

        return """You are a code assistant. Output ONE terse command per response.

Commands:
- skeleton <file> - show file structure
- expand <symbol> - show full source
- grep <pattern> <path> - search
- deps <file> - imports/dependencies
- callers <symbol> - who calls this
- callees <symbol> - what this calls
- validate - run linters
- write <file>:<content> - overwrite file
- replace <file> <search> <replace> - replace text
- insert <file> <content> - append or insert line
- diff [branch] - show changes
- branch [name] - list or switch branch
- revert <file> <line> - undo change at line
- analyze [session] - show telemetry
- fix: <description> - describe fix
- breakdown: <step1>, <step2>, ... - split current task
- note: <content> - remember for this task
- done [summary] - task complete, include brief summary

IMPORTANT: When the requested info is in "Last result", say "done <summary>".
For read-only tasks (show, explain, find), say "done" after getting the answer.
Do NOT repeat the same command. Never output prose."""

    def _get_adaptive_preview_limit(self) -> int:
        """Determine preview limit based on task complexity/type."""
        if self._task_type == TaskType.WRITE:
            return MAX_RESULT_PREVIEW_LIMIT
        return DEFAULT_RESULT_PREVIEW_LIMIT

    def _preview_result(self, result: str) -> tuple[str, str | None]:
        """Create preview of result, cache full if large.

        Uses EphemeralCache for TTL-based storage and proper preview generation.
        Implements adaptive pruning based on content importance.

        Returns:
            (preview_text, result_id or None)
        """
        base_limit = self._get_adaptive_preview_limit()

        # Adaptive adjustment based on importance
        importance = self._ephemeral_cache.score_content(
            result, task_context=self._task_tree.root.goal if self._task_tree else ""
        )

        # High importance content gets more context (up to 2x)
        # Low importance gets less (down to 0.5x)
        adjusted_limit = int(base_limit * (0.5 + importance))

        if len(result) <= adjusted_limit:
            return result, None

        # Store in ephemeral cache, get preview
        result_id = self._ephemeral_cache.store(result)
        preview = self._ephemeral_cache.generate_preview(result, adjusted_limit)
        # Add result ID reference
        preview = preview.replace(
            "available via resource link",
            f"fetch {result_id}",
        )
        return preview, result_id

    def _build_turn_context(self) -> str:
        """Build context for current turn from TaskTree.

        Returns minimal context: path + notes + last result preview.
        For read-only tasks with results, adds completion hint.
        """
        parts = []

        # Task path
        if self._task_tree:
            parts.append(self._task_tree.format_context())

        # Mid-task feedback
        mail = self._check_mail()
        if mail:
            parts.append(mail)

        # Last result
        if self._last_result:
            parts.append(f"\nLast result:\n{self._last_result}")

        # Add completion hint for read-only tasks with successful results
        if self._task_type == TaskType.READ_ONLY and self._successful_results > 0:
            parts.append(
                "\n[READ-ONLY TASK: You have the answer above. Say 'done <brief summary>' now.]"
            )

        return "\n".join(parts) if parts else "(no context)"

    async def _get_llm_response(self) -> str:
        """Get next intent from LLM using context-excluded model.

        No conversation history - just current state.
        """
        try:
            import litellm
        except ImportError as e:
            msg = "litellm required for DWIMLoop. Install with: pip install litellm"
            raise ImportError(msg) from e

        # Build minimal context
        context = self._build_turn_context()

        messages = [
            {"role": "system", "content": self._build_system_prompt()},
            {"role": "user", "content": context},
        ]

        response = await litellm.acompletion(
            model=self.config.model,
            messages=messages,
            temperature=self.config.temperature,
        )

        return (response.choices[0].message.content or "").strip()

    def _validate_sandbox_scope(
        self, tool_name: str, params: dict[str, Any], scope: Path
    ) -> str | None:
        """Validate that tool parameters respect the sandbox scope.

        Args:
            tool_name: Name of tool being executed
            params: Tool parameters
            scope: Allowed sandbox path (must be absolute or relative to root)

        Returns:
            Error message if invalid, None if allowed
        """
        # Collect potential paths from params
        paths_to_check = []

        if "file_path" in params:
            paths_to_check.append(str(params["file_path"]))
        if "file_paths" in params:
            paths_to_check.extend(str(p) for p in params["file_paths"])
        if "path" in params:
            paths_to_check.append(str(params["path"]))
        if "target" in params:
            # Check if target looks like a file path (has extension or separator)
            target = str(params["target"])
            if "/" in target or "." in target:
                paths_to_check.append(target)

        # Resolve scope to absolute path
        try:
            if scope.is_absolute():
                abs_scope = scope.resolve()
            else:
                abs_scope = (self.api.root / scope).resolve()
        except OSError as e:
            return f"Invalid sandbox scope '{scope}': {e}"

        for p_str in paths_to_check:
            try:
                # Handle relative paths (relative to api root)
                path = Path(p_str)
                if not path.is_absolute():
                    path = self.api.root / path

                # Resolve path
                # Note: path.resolve() might fail if file doesn't exist.
                # Use parent directory for non-existent files.
                try:
                    abs_path = path.resolve()
                except OSError:
                    # Fallback to resolving parent if file doesn't exist
                    abs_path = path.parent.resolve() / path.name

                if not str(abs_path).startswith(str(abs_scope)):
                    return (
                        f"Access denied: '{p_str}' is outside sandbox scope '{scope}' "
                        f"(resolved: {abs_path})"
                    )

            except Exception as e:
                return f"Path validation failed for '{p_str}': {e}"

        return None

    async def _execute_tool(self, tool_name: str, params: dict[str, Any], _depth: int = 0) -> Any:
        """Execute a tool and return result.

        Args:
            tool_name: Name of tool to execute
            params: Tool parameters
            _depth: Recursion depth (internal)

        Returns:
            Tool output (string or structured data)
        """
        # Prevent infinite recursion
        if _depth > 3:
            return f"Unknown tool: {tool_name}"

        # Handle termination
        if tool_name == "done":
            return None

        # Check sandbox scope
        if self._task_tree and self._task_tree.current.sandbox_scope:
            scope = self._task_tree.current.sandbox_scope
            error = self._validate_sandbox_scope(tool_name, params, scope)
            if error:
                return f"Sandbox Error: {error}"

        # Check triggers before execution
        triggers = await self._check_memory_triggers(tool_name, params)

        result = await self._run_tool_logic(tool_name, params, _depth)

        # Append triggers to result if any
        if triggers and isinstance(result, str):
            result += f"\n\n[Memory Triggers]\n{triggers}"

        return result

    async def _check_memory_triggers(self, tool_name: str, params: dict[str, Any]) -> str | None:
        """Check for memory triggers based on tool and params."""
        try:
            # Extract files from params
            files = []
            if "file_path" in params:
                files.append(str(params["file_path"]))
            if "file_paths" in params:
                files.extend(str(p) for p in params["file_paths"])
            if "path" in params:
                path_str = str(params["path"])
                if path_str.endswith(".py") or "/" in path_str:
                    files.append(path_str)

            if not files:
                return None

            from moss.memory import StateSnapshot

            # Create minimal state
            state = StateSnapshot.create(files=files, context="")

            # Check triggers on memory layer
            # Access memory_layer from api (added in previous step)
            if hasattr(self.api, "memory_layer"):
                warnings = await self.api.memory_layer.check_triggers(state)
                if warnings:
                    return "\n".join(warnings)

            return None
        except Exception as e:
            logger.warning("Failed to check memory triggers: %s", e)
            return None

    async def _run_tool_logic(self, tool_name: str, params: dict[str, Any], _depth: int) -> Any:
        """Core logic to run the tool (extracted from _execute_tool)."""
        # Handle multi-file/search expand
        if tool_name == "skeleton.expand_search":
            return self._expand_search(params.get("symbol_name", ""))

        if tool_name == "skeleton.expand" and "file_paths" in params:
            return self._expand_multi_file(
                params.get("symbol_name", ""),
                params.get("file_paths", []),
            )

        # Route through MossAPI
        parts = tool_name.split(".")
        if len(parts) == 2:
            api_name, method_name = parts
            sub_api = getattr(self.api, api_name, None)
            if sub_api:
                method = getattr(sub_api, method_name, None)
                if method:
                    # Handle async methods
                    result = method(**params)
                    if hasattr(result, "__await__"):
                        result = await result
                    return result

        # Fallback: try DWIM routing (with depth limit)
        matches = analyze_intent(tool_name)
        if matches and matches[0].confidence > self.config.confidence_threshold:
            best = matches[0]
            if best.tool != tool_name:  # Avoid infinite loop
                return await self._execute_tool(best.tool, params, _depth + 1)

        return f"Unknown tool: {tool_name}"

    def _expand_search(self, symbol_name: str) -> str:
        """Search for symbol across codebase and expand all matches.

        Args:
            symbol_name: Name of the symbol to find and expand

        Returns:
            Formatted string with all matching expansions
        """
        if not symbol_name:
            return "Error: No symbol name provided"

        # Use search API to find definitions
        matches = self.api.search.find_definitions(symbol_name)
        if not matches:
            return f"No definitions found for: {symbol_name}"

        results = []
        for match in matches[:5]:  # Limit to 5 matches
            try:
                content = self.api.skeleton.expand(match.file_path, symbol_name)
                if content:
                    results.append(f"# {match.file_path}\n{content}")
            except Exception as e:
                results.append(f"# {match.file_path}\nError: {e}")

        if not results:
            return f"Symbol found but could not expand: {symbol_name}"

        return "\n\n".join(results)

    def _expand_multi_file(self, symbol_name: str, file_paths: list[str]) -> str:
        """Expand symbol in multiple specified files.

        Args:
            symbol_name: Name of the symbol to expand
            file_paths: List of file paths to search in

        Returns:
            Formatted string with expansions from each file
        """
        if not symbol_name:
            return "Error: No symbol name provided"

        results = []
        for file_path in file_paths:
            try:
                content = self.api.skeleton.expand(file_path, symbol_name)
                if content:
                    results.append(f"# {file_path}\n{content}")
                else:
                    results.append(f"# {file_path}\nNot found: {symbol_name}")
            except Exception as e:
                results.append(f"# {file_path}\nError: {e}")

        if not results:
            return f"Could not expand {symbol_name} in any file"

        return "\n\n".join(results)

    def _handle_meta_command(self, intent: ParsedIntent) -> str | None:
        """Handle meta-commands that modify TaskTree state.

        Returns output string, or None if not a meta-command.
        """
        if not self._task_tree:
            return None

        verb = intent.verb

        # breakdown: step1, step2, step3
        if verb == "breakdown" and intent.content:
            steps = [s.strip() for s in intent.content.split(",") if s.strip()]
            if steps:
                self._task_tree.breakdown(steps)
                return f"Split into {len(steps)} subtasks"
            return "No steps provided"

        # note: content
        if verb == "note" and intent.content:
            self._task_tree.add_note(intent.content, NoteExpiry.ON_DONE)
            return "Note added"

        # done [summary]
        if verb == "done":
            summary = intent.target or intent.content or "completed"
            result = self._task_tree.complete(summary)
            if result is None:
                return None  # Root complete - will exit loop
            return f"Completed, now at: {result.goal}"

        # fetch: result_id - expand cached result
        if verb == "fetch" and intent.target:
            result_id = intent.target
            content = self._ephemeral_cache.get_content(result_id)
            if content:
                return content
            return f"Unknown or expired result ID: {result_id}"

        return None

    def _detect_stall(self, intent: ParsedIntent) -> bool:
        """Detect if agent is stalled (repeating same command).

        Returns True if we should force exit due to stall.
        """
        if len(self._turns) < self.config.stall_threshold:
            return False

        # Check last N intents for repetition
        recent = self._turns[-self.config.stall_threshold :]
        raw_intents = [t.intent.raw for t in recent]

        # If all recent intents are identical, we're stalled
        if len(set(raw_intents)) == 1 and raw_intents[0] == intent.raw:
            return True

        return False

    async def run(self, task: str) -> LoopResult:
        """Run the DWIM loop on a task.

        Args:
            task: Initial task description

        Returns:
            LoopResult with final state and all turns
        """
        self._turns = []
        self._task_tree = TaskTree(task)
        self._last_result = None
        self._task_type = classify_task(task)
        self._successful_results = 0
        self._ephemeral_cache.clear()  # Fresh cache for new run
        start_time = datetime.now(UTC)

        logger.debug(f"Task classified as: {self._task_type.name}")

        try:
            for _turn_num in range(self.config.max_turns):
                turn_start = datetime.now(UTC)

                # Tick note counters
                self._task_tree.tick_notes()

                # Get LLM response (context built from TaskTree)
                llm_response = await self._get_llm_response()

                # Parse intent
                intent = parse_intent(llm_response)

                # Stall detection - repeated identical commands
                if self._detect_stall(intent):
                    total_duration = int((datetime.now(UTC) - start_time).total_seconds() * 1000)
                    return LoopResult(
                        state=LoopState.STALLED,
                        turns=self._turns,
                        final_output=self._turns[-1].tool_output if self._turns else None,
                        error=f"Stalled: repeated '{intent.raw}' {self.config.stall_threshold}x",
                        total_duration_ms=total_duration,
                    )

                # Handle meta-commands first
                meta_output = self._handle_meta_command(intent)
                if meta_output is not None:
                    self._last_result, _ = self._preview_result(meta_output)
                    duration = int((datetime.now(UTC) - turn_start).total_seconds() * 1000)
                    self._turns.append(
                        TurnResult(
                            intent=intent,
                            tool_match=None,
                            tool_output=meta_output,
                            duration_ms=duration,
                        )
                    )
                    continue

                # Check for root completion
                if intent.verb == "done":
                    duration = int((datetime.now(UTC) - turn_start).total_seconds() * 1000)
                    self._turns.append(
                        TurnResult(
                            intent=intent,
                            tool_match=None,
                            tool_output=None,
                            duration_ms=duration,
                        )
                    )
                    total_duration = int((datetime.now(UTC) - start_time).total_seconds() * 1000)
                    return LoopResult(
                        state=LoopState.DONE,
                        turns=self._turns,
                        final_output=self._turns[-2].tool_output if len(self._turns) > 1 else None,
                        total_duration_ms=total_duration,
                    )

                # Build and execute tool call
                tool_name, params = build_tool_call(intent, self.api)
                tool_match = resolve_tool(tool_name) if tool_name != "done" else None

                # Check confidence - ask for clarification if low
                if tool_match and tool_match.confidence < self.config.confidence_threshold:
                    # Low confidence - return clarification message instead of executing
                    output = None
                    error = tool_match.message or f"Unclear command: {intent.raw}"
                    self._last_result = f"Clarification needed: {error}"
                    duration = int((datetime.now(UTC) - turn_start).total_seconds() * 1000)
                    self._turns.append(
                        TurnResult(
                            intent=intent,
                            tool_match=tool_match,
                            tool_output=None,
                            error=error,
                            duration_ms=duration,
                        )
                    )
                    continue

                try:
                    output = await self._execute_tool(tool_name, params)
                    error = None
                except Exception as e:
                    output = None
                    error = str(e)

                # Store result with preview
                if output:
                    result_str = str(output) if not isinstance(output, str) else output
                    self._last_result, _ = self._preview_result(result_str)
                    self._successful_results += 1
                elif error:
                    self._last_result = f"Error: {error}"
                else:
                    self._last_result = "(no output)"

                # Record turn
                duration = int((datetime.now(UTC) - turn_start).total_seconds() * 1000)
                self._turns.append(
                    TurnResult(
                        intent=intent,
                        tool_match=tool_match,
                        tool_output=output,
                        error=error,
                        duration_ms=duration,
                    )
                )

            # Max turns reached
            total_duration = int((datetime.now(UTC) - start_time).total_seconds() * 1000)
            return LoopResult(
                state=LoopState.MAX_TURNS,
                turns=self._turns,
                final_output=self._turns[-1].tool_output if self._turns else None,
                error=f"Max turns ({self.config.max_turns}) reached",
                total_duration_ms=total_duration,
            )

        except Exception as e:
            total_duration = int((datetime.now(UTC) - start_time).total_seconds() * 1000)
            return LoopResult(
                state=LoopState.FAILED,
                turns=self._turns,
                final_output=None,
                error=str(e),
                total_duration_ms=total_duration,
            )


__all__ = [
    "DWIMLoop",
    "LoopConfig",
    "LoopResult",
    "LoopState",
    "ParsedIntent",
    "TaskType",
    "TurnResult",
    "classify_task",
    "parse_intent",
]
