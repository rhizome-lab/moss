"""Canonical API surface for Moss.

This module provides the primary entry point for using Moss as a library.
Import MossAPI for organized access to all functionality.

Example:
    from moss import MossAPI

    # Create API instance
    api = MossAPI.for_project("/path/to/project")

    # Use various capabilities
    skeleton = api.skeleton.extract("src/main.py")
    deps = api.dependencies.analyze("src/")
    health = api.health.check()
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from moss.anchors import AnchorMatch
    from moss.cfg import ControlFlowGraph
    from moss.check_docs import DocCheckResult
    from moss.check_refs import RefCheckResult
    from moss.check_todos import TodoCheckResult
    from moss.complexity import ComplexityReport
    from moss.context import CompiledContext, ContextHost
    from moss.dependencies import DependencyInfo
    from moss.dependency_analysis import DependencyAnalysis
    from moss.external_deps import DependencyAnalysisResult
    from moss.git_hotspots import GitHotspotAnalysis
    from moss.patches import Patch, PatchResult
    from moss.rag import IndexStats, RAGIndex, SearchResult
    from moss.shadow_git import CommitHandle, ShadowBranch, ShadowGit
    from moss.skeleton import Symbol
    from moss.status import ProjectStatus
    from moss.structural_analysis import StructuralAnalysis
    from moss.summarize import ProjectSummary
    from moss.test_analysis import TestAnalysis
    from moss.validators import ValidationResult, ValidatorChain
    from moss.weaknesses import WeaknessAnalysis


@dataclass
class SkeletonAPI:
    """API for code skeleton extraction.

    Extracts structural summaries of code (classes, functions, signatures)
    without implementation details. Supports multiple languages via plugin system.
    """

    root: Path

    def extract(self, file_path: str | Path) -> list[Symbol]:
        """Extract skeleton from a Python file.

        Args:
            file_path: Path to the Python file (relative to root or absolute)

        Returns:
            List of Symbol objects representing the code structure

        Note:
            For non-Python files, use format() which routes through the plugin system.
        """
        from moss.skeleton import extract_python_skeleton

        path = self._resolve_path(file_path)
        source = path.read_text()
        return extract_python_skeleton(source)

    def format(self, file_path: str | Path, include_docstrings: bool = True) -> str:
        """Extract and format skeleton as readable text.

        Uses the plugin registry to support multiple file types (Python, Markdown, etc.).

        Args:
            file_path: Path to the file
            include_docstrings: Whether to include docstrings in output (Python only)

        Returns:
            Formatted string representation of the skeleton
        """
        import asyncio
        import concurrent.futures

        from moss.plugins import get_registry
        from moss.views import ViewOptions, ViewTarget

        path = self._resolve_path(file_path)
        if not path.exists():
            return f"File not found: {path}"
        target = ViewTarget(path=path)
        registry = get_registry()
        plugin = registry.find_plugin(target, "skeleton")

        if plugin is None:
            return f"No skeleton plugin found for: {path.suffix}"

        options = ViewOptions(include_private=True)

        async def render() -> str:
            view = await plugin.render(target, options)
            if "error" in view.metadata:
                return f"Error: {view.metadata['error']}"
            return view.content

        def run_in_new_loop() -> str:
            return asyncio.run(render())

        # Check if we're already in an async context
        try:
            asyncio.get_running_loop()
            # Already in async context - run in a thread with its own loop
            with concurrent.futures.ThreadPoolExecutor() as executor:
                future = executor.submit(run_in_new_loop)
                return future.result()
        except RuntimeError:
            # No running loop - just use asyncio.run
            return asyncio.run(render())

    def expand(self, file_path: str | Path, symbol_name: str) -> str | None:
        """Get the full source code of a named symbol.

        Useful for getting complete enum definitions, class bodies, or function
        implementations when the skeleton isn't enough.

        Args:
            file_path: Path to the Python file
            symbol_name: Name of the symbol to expand (e.g., "StepType", "my_function")

        Returns:
            Full source code of the symbol, or None if not found

        Example:
            # Get full enum definition
            content = api.skeleton.expand("src/agent_loop.py", "StepType")
            # Returns complete enum with all values
        """
        from moss.skeleton import expand_symbol

        path = self._resolve_path(file_path)
        source = path.read_text()
        return expand_symbol(source, symbol_name)

    def get_enum_values(self, file_path: str | Path, enum_name: str) -> list[str] | None:
        """Extract enum member names from an Enum class.

        Args:
            file_path: Path to the Python file
            enum_name: Name of the Enum class

        Returns:
            List of enum member names, or None if not found

        Example:
            values = api.skeleton.get_enum_values("src/agent_loop.py", "StepType")
            # Returns: ["TOOL", "LLM", "HYBRID"]
        """
        from moss.skeleton import get_enum_values

        path = self._resolve_path(file_path)
        source = path.read_text()
        return get_enum_values(source, enum_name)

    def _resolve_path(self, file_path: str | Path) -> Path:
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class TreeAPI:
    """API for git-aware file tree visualization.

    Shows project structure with awareness of git tracking status.
    """

    root: Path

    def generate(
        self,
        path: str | Path | None = None,
        tracked_only: bool = False,
        gitignore: bool = True,
    ) -> Any:  # TreeResult, but avoid circular import
        """Generate a tree visualization of a directory.

        Args:
            path: Directory to visualize (default: project root)
            tracked_only: If True, only show git-tracked files
            gitignore: If True, respect .gitignore when showing all files

        Returns:
            TreeResult with tree visualization and file counts
        """
        from moss.tree import generate_tree

        target = self._resolve_path(path) if path else self.root
        return generate_tree(target, tracked_only=tracked_only, gitignore=gitignore)

    def format(
        self,
        path: str | Path | None = None,
        tracked_only: bool = False,
        compact: bool = False,
    ) -> str:
        """Generate and format tree as readable text.

        Args:
            path: Directory to visualize (default: project root)
            tracked_only: If True, only show git-tracked files
            compact: If True, use token-efficient format

        Returns:
            Formatted tree visualization
        """
        from moss.tree import generate_tree

        target = self._resolve_path(path) if path else self.root
        result = generate_tree(target, tracked_only=tracked_only)

        return result.to_compact() if compact else result.to_text()

    def _resolve_path(self, file_path: str | Path) -> Path:
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class AnchorAPI:
    """API for finding code locations using fuzzy anchors.

    Anchors identify code elements (functions, classes, variables) by name
    and type, with fuzzy matching support.
    """

    root: Path

    def find(
        self,
        file_path: str | Path,
        name: str,
        anchor_type: str = "function",
    ) -> list[AnchorMatch]:
        """Find anchors matching a name in a file.

        Args:
            file_path: Path to search in
            name: Name to search for (supports fuzzy matching)
            anchor_type: Type filter - "function", "class", "variable", "method", "import"

        Returns:
            List of AnchorMatch objects with locations and confidence scores
        """
        from moss.anchors import Anchor, AnchorType, find_anchors

        path = self._resolve_path(file_path)
        source = path.read_text()

        type_map = {
            "function": AnchorType.FUNCTION,
            "class": AnchorType.CLASS,
            "variable": AnchorType.VARIABLE,
            "method": AnchorType.METHOD,
            "import": AnchorType.IMPORT,
        }
        anchor = Anchor(type=type_map.get(anchor_type, AnchorType.FUNCTION), name=name)
        return find_anchors(source, anchor)

    def resolve(
        self,
        file_path: str | Path,
        name: str,
        anchor_type: str = "function",
    ) -> AnchorMatch:
        """Resolve a single anchor (raises if ambiguous or not found).

        Args:
            file_path: Path to search in
            name: Name to search for
            anchor_type: Type filter

        Returns:
            Single best AnchorMatch

        Raises:
            AnchorNotFoundError: If no match found
            AmbiguousAnchorError: If multiple matches with equal confidence
        """
        from moss.anchors import Anchor, AnchorType, resolve_anchor

        path = self._resolve_path(file_path)
        source = path.read_text()

        type_map = {
            "function": AnchorType.FUNCTION,
            "class": AnchorType.CLASS,
            "variable": AnchorType.VARIABLE,
            "method": AnchorType.METHOD,
            "import": AnchorType.IMPORT,
        }
        anchor = Anchor(type=type_map.get(anchor_type, AnchorType.FUNCTION), name=name)
        return resolve_anchor(source, anchor)

    def _resolve_path(self, file_path: str | Path) -> Path:
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class PatchAPI:
    """API for applying code patches.

    Supports AST-aware patching with automatic fallback to text-based
    patching when AST parsing fails.
    """

    root: Path

    def apply(
        self,
        file_path: str | Path,
        patch: Patch,
        write: bool = True,
    ) -> PatchResult:
        """Apply a patch to a file.

        Args:
            file_path: Path to the file to patch
            patch: Patch object describing the change
            write: Whether to write changes to disk

        Returns:
            PatchResult with success status and modified content
        """
        from moss.patches import apply_patch

        path = self._resolve_path(file_path)
        source = path.read_text()
        result = apply_patch(source, patch)

        if write and result.success:
            path.write_text(result.patched)

        return result

    def apply_with_fallback(
        self,
        file_path: str | Path,
        patch: Patch,
        write: bool = True,
    ) -> PatchResult:
        """Apply a patch with automatic text fallback.

        First tries AST-aware patching, falls back to text-based
        if that fails.

        Args:
            file_path: Path to the file to patch
            patch: Patch object describing the change
            write: Whether to write changes to disk

        Returns:
            PatchResult with success status and modified content
        """
        from moss.patches import apply_patch_with_fallback

        path = self._resolve_path(file_path)
        source = path.read_text()
        result = apply_patch_with_fallback(source, patch)

        if write and result.success:
            path.write_text(result.patched)

        return result

    def create(
        self,
        patch_type: str,
        anchor_name: str,
        content: str,
        **kwargs: Any,
    ) -> Patch:
        """Create a Patch object.

        Args:
            patch_type: Type of patch - "insert_before", "insert_after", "replace", "delete"
            anchor_name: Name of the anchor to target
            content: New content for the patch
            **kwargs: Additional patch options (anchor_type for anchor construction)

        Returns:
            Patch object ready for application
        """
        from moss.anchors import Anchor, AnchorType
        from moss.patches import Patch, PatchType

        type_map = {
            "insert_before": PatchType.INSERT_BEFORE,
            "insert_after": PatchType.INSERT_AFTER,
            "replace": PatchType.REPLACE,
            "delete": PatchType.DELETE,
        }

        anchor_type_map = {
            "function": AnchorType.FUNCTION,
            "class": AnchorType.CLASS,
            "variable": AnchorType.VARIABLE,
            "method": AnchorType.METHOD,
            "import": AnchorType.IMPORT,
        }

        anchor_type = anchor_type_map.get(
            kwargs.get("anchor_type", "function"), AnchorType.FUNCTION
        )
        anchor = Anchor(type=anchor_type, name=anchor_name)

        return Patch(
            anchor=anchor,
            patch_type=type_map.get(patch_type, PatchType.REPLACE),
            content=content,
        )

    def _resolve_path(self, file_path: str | Path) -> Path:
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class DependencyAPI:
    """API for dependency analysis.

    Analyzes import/export relationships, detects circular dependencies,
    and provides coupling metrics.
    """

    root: Path

    def extract(self, file_path: str | Path) -> DependencyInfo:
        """Extract imports and exports from a file.

        Args:
            file_path: Path to analyze

        Returns:
            DependencyInfo with imports and exports
        """
        from moss.dependencies import extract_dependencies

        path = self._resolve_path(file_path)
        source = path.read_text()
        return extract_dependencies(source)

    def analyze(self) -> DependencyAnalysis:
        """Run full dependency analysis on the project.

        Returns:
            DependencyAnalysis with circular deps, god modules, orphans, etc.
        """
        from moss.dependency_analysis import DependencyAnalyzer

        analyzer = DependencyAnalyzer(self.root)
        return analyzer.analyze()

    def format(self, file_path: str | Path) -> str:
        """Extract and format dependencies as readable text.

        Args:
            file_path: Path to analyze

        Returns:
            Formatted string with imports and exports
        """
        from moss.dependencies import format_dependencies

        info = self.extract(file_path)
        return format_dependencies(info)

    def _resolve_path(self, file_path: str | Path) -> Path:
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class CFGAPI:
    """API for control flow graph analysis.

    Builds control flow graphs showing execution paths through functions.
    """

    root: Path

    def build(self, file_path: str | Path) -> list[ControlFlowGraph]:
        """Build CFGs for all functions in a file.

        Args:
            file_path: Path to the Python file

        Returns:
            List of ControlFlowGraph objects for each function
        """
        from moss.cfg import build_cfg

        path = self._resolve_path(file_path)
        source = path.read_text()
        return build_cfg(source)

    def _resolve_path(self, file_path: str | Path) -> Path:
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class ValidationAPI:
    """API for code validation.

    Runs validators (syntax, linting, tests) and reports issues.
    """

    root: Path

    def create_chain(self) -> ValidatorChain:
        """Create a standard Python validator chain.

        Returns:
            ValidatorChain configured for Python (syntax + ruff + pytest)
        """
        from moss.validators import create_python_validator_chain

        return create_python_validator_chain()

    async def validate(self, file_path: str | Path) -> ValidationResult:
        """Validate a Python file with the default chain.

        Args:
            file_path: Path to validate

        Returns:
            ValidationResult with any issues found
        """
        chain = self.create_chain()
        path = self._resolve_path(file_path)
        return await chain.validate(path)

    def _resolve_path(self, file_path: str | Path) -> Path:
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class GitAPI:
    """API for shadow git operations.

    Provides atomic commit/rollback operations for safe code modifications.
    """

    root: Path
    _shadow_git: ShadowGit | None = None

    def init(self) -> ShadowGit:
        """Initialize shadow git for the project.

        Returns:
            ShadowGit instance for managing branches
        """
        from moss.shadow_git import ShadowGit

        if self._shadow_git is None:
            self._shadow_git = ShadowGit(self.root)
        return self._shadow_git

    async def create_branch(self, name: str | None = None) -> ShadowBranch:
        """Create an isolated shadow branch for agent work.

        Args:
            name: Optional branch name (auto-generated if not provided)

        Returns:
            ShadowBranch for managing the branch
        """
        git = self.init()
        return await git.create_shadow_branch(name)

    async def commit(self, branch: ShadowBranch, message: str) -> CommitHandle:
        """Create a commit on the specified shadow branch.

        Args:
            branch: ShadowBranch to commit on
            message: Commit message

        Returns:
            CommitHandle referencing the new commit
        """
        git = self.init()
        return await git.commit(branch, message)

    async def create_checkpoint(
        self, name: str | None = None, message: str | None = None
    ) -> dict[str, str]:
        """Create a checkpoint with current changes.

        Checkpoints are shadow branches that capture current work state,
        allowing safe experimentation with easy rollback.

        Args:
            name: Optional checkpoint name (auto-generated if not provided)
            message: Optional commit message for initial checkpoint state

        Returns:
            Dict with 'branch' name and 'commit' SHA
        """
        import time

        git = self.init()
        branch_name = name or f"checkpoint/{int(time.time())}"
        branch = await git.create_shadow_branch(branch_name)
        commit_msg = message or f"Checkpoint: {branch_name}"
        handle = await git.commit(branch, commit_msg)
        return {"branch": branch.name, "commit": handle.sha}

    async def list_checkpoints(self) -> list[dict[str, str]]:
        """List active checkpoints.

        Returns:
            List of checkpoint info dicts with 'name' and 'type' keys
        """
        git = self.init()
        result = await git._run_git("branch", "--list", "shadow/*", "checkpoint/*")
        branches = [b.strip() for b in result.stdout.strip().split("\n") if b.strip()]
        checkpoints = []
        for branch in branches:
            # Remove leading * and whitespace for current branch
            branch = branch.lstrip("* ").strip()
            checkpoint_type = "checkpoint" if branch.startswith("checkpoint/") else "shadow"
            checkpoints.append({"name": branch, "type": checkpoint_type})
        return checkpoints

    async def diff_checkpoint(self, name: str) -> dict[str, str]:
        """Show changes in a checkpoint.

        Args:
            name: Checkpoint branch name

        Returns:
            Dict with 'diff' (full diff) and 'stat' (summary stats)
        """
        git = self.init()
        # Create a temporary branch object for the diff
        branch = ShadowBranch(name, git.root)
        diff = await git.diff(branch)
        stat = await git.diff_stat(branch)
        return {"diff": diff, "stat": stat}

    async def merge_checkpoint(self, name: str, message: str | None = None) -> dict[str, str]:
        """Merge checkpoint changes into base branch.

        Args:
            name: Checkpoint branch name to merge
            message: Optional merge commit message

        Returns:
            Dict with 'commit' SHA of merge commit
        """
        git = self.init()
        branch = ShadowBranch(name, git.root)
        merge_msg = message or f"Merge checkpoint {name}"
        handle = await git.squash_merge(branch, merge_msg)
        return {"commit": handle.sha}

    async def abort_checkpoint(self, name: str) -> dict[str, bool]:
        """Abandon a checkpoint and delete its branch.

        Args:
            name: Checkpoint branch name to abort

        Returns:
            Dict with 'success' boolean
        """
        git = self.init()
        # Get current branch to check if we're on the checkpoint
        current = await git._get_current_branch()
        if current == name:
            # Switch to main/master before deleting
            try:
                await git._run_git("checkout", "main")
            except Exception:
                await git._run_git("checkout", "master")

        branch = ShadowBranch(name, git.root)
        await git.abort(branch)
        return {"success": True}


@dataclass
class ContextAPI:
    """API for context compilation.

    Compiles code views (skeletons, CFGs, dependencies) into structured
    context for AI consumption.
    """

    root: Path
    _host: ContextHost | None = None

    def init(self) -> ContextHost:
        """Initialize the context host with default view providers.

        Returns:
            ContextHost instance
        """
        from moss.context import ContextHost
        from moss.views import create_default_registry

        if self._host is None:
            registry = create_default_registry()
            self._host = ContextHost(registry)
        return self._host

    async def compile(
        self,
        file_paths: list[str | Path],
        view_types: list[str] | None = None,
    ) -> CompiledContext:
        """Compile context for the given files.

        Args:
            file_paths: Files to include in context
            view_types: View types to generate (default: skeleton, dependency)

        Returns:
            CompiledContext with rendered views
        """
        from moss.views import ViewTarget, ViewType

        host = self.init()

        targets = []
        for path in file_paths:
            p = Path(path)
            if not p.is_absolute():
                p = self.root / p
            targets.append(ViewTarget(path=p))

        types = view_types or ["skeleton", "dependency"]
        type_map = {
            "skeleton": ViewType.SKELETON,
            "dependency": ViewType.DEPENDENCY,
            "cfg": ViewType.CFG,
            "raw": ViewType.RAW,
            "elided": ViewType.ELIDED,
        }
        view_type_enums = [type_map.get(t, ViewType.SKELETON) for t in types]

        return await host.compile(targets, view_types=view_type_enums)


@dataclass
class HealthAPI:
    """API for project health analysis.

    Provides comprehensive project health metrics and reports.
    """

    root: Path

    def check(self) -> ProjectStatus:
        """Run full health analysis on the project.

        Returns:
            ProjectStatus with health score, grade, and detailed metrics
        """
        from moss.status import StatusChecker

        checker = StatusChecker(self.root)
        return checker.check()

    def summarize(self) -> ProjectSummary:
        """Generate a project summary.

        Returns:
            ProjectSummary with module information
        """
        from moss.summarize import Summarizer

        summarizer = Summarizer(include_private=False, include_tests=False)
        return summarizer.summarize_project(self.root)

    def check_docs(self) -> DocCheckResult:
        """Check documentation health.

        Returns:
            DocCheckResult with coverage and issues
        """
        from moss.check_docs import DocChecker

        checker = DocChecker(self.root, check_links=True)
        return checker.check()

    def check_todos(self) -> TodoCheckResult:
        """Check TODO tracking health.

        Returns:
            TodoCheckResult with tracked and orphaned TODOs
        """
        from moss.check_todos import TodoChecker

        checker = TodoChecker(self.root)
        return checker.check()

    def analyze_structure(self) -> StructuralAnalysis:
        """Analyze structural code quality.

        Returns:
            StructuralAnalysis with hotspots and metrics
        """
        from moss.structural_analysis import StructuralAnalyzer

        analyzer = StructuralAnalyzer(self.root)
        return analyzer.analyze()

    def analyze_tests(self) -> TestAnalysis:
        """Analyze test coverage structure.

        Returns:
            TestAnalysis with module-test mappings
        """
        from moss.test_analysis import TestAnalyzer

        analyzer = TestAnalyzer(self.root)
        return analyzer.analyze()


@dataclass
class TodoSearchResult:
    """Result of a TODO search."""

    text: str
    status: str  # "pending" or "done"
    section: str | None
    line: int
    source: str  # "todo.md" or file path for code TODOs

    def to_compact(self, include_section: bool = True) -> str:
        """Format as compact single line."""
        prefix = "[x]" if self.status == "done" else "- "
        section = f" ({self.section})" if include_section and self.section else ""
        return f"{prefix}{self.text}{section}"


@dataclass
class TodoListResult:
    """Result of listing TODOs with smart formatting."""

    items: list[TodoSearchResult]
    truncated: bool = False
    per_section_limit: int | None = None
    max_sections: int | None = None

    def to_compact(self) -> str:
        """Format as compact text, auto-truncated by section."""
        if not self.items:
            return "(no TODOs found)"

        # Group by section
        by_section: dict[str | None, list[TodoSearchResult]] = {}
        for item in self.items:
            by_section.setdefault(item.section, []).append(item)

        lines = []
        limit = self.per_section_limit or 5  # Default: 5 per section
        max_sec = self.max_sections or 5  # Default: 5 sections

        sections_shown = 0
        for section, items in by_section.items():
            if sections_shown >= max_sec:
                break
            sections_shown += 1

            section_name = section or "Uncategorized"
            shown = items[:limit]
            remaining = len(items) - len(shown)

            lines.append(f"## {section_name}")
            for item in shown:
                lines.append(item.to_compact(include_section=False))
            if remaining > 0:
                lines.append(f"  ... and {remaining} more")
            lines.append("")

        remaining_sections = len(by_section) - sections_shown
        if remaining_sections > 0:
            lines.append(f"... and {remaining_sections} more sections")
            lines.append("")

        total = len(self.items)
        lines.append(f"({total} total items)")
        return "\n".join(lines)


@dataclass
class TodoAPI:
    """API for TODO management and search.

    Search and browse TODOs from TODO.md. Useful for finding
    relevant work items and understanding project priorities.

    Example: "Find TODOs about authentication" → search("authentication")
    """

    root: Path

    def list(
        self,
        section: str | None = None,
        include_done: bool = False,
        per_section_limit: int = 5,
    ) -> TodoListResult:
        """List TODOs, optionally filtered by section.

        Args:
            section: Filter to specific section (case-insensitive partial match)
            include_done: Include completed TODOs (default: False)
            per_section_limit: Max items to show per section (default: 5)

        Returns:
            TodoListResult with items grouped by section
        """
        from moss.check_todos import TodoChecker, TodoStatus

        checker = TodoChecker(self.root)
        result = checker.check()

        items = []
        for todo in result.tracked_items:
            # Filter by status
            if not include_done and todo.status == TodoStatus.DONE:
                continue

            # Filter by section
            if section and todo.category:
                if section.lower() not in todo.category.lower():
                    continue
            elif section and not todo.category:
                continue

            items.append(
                TodoSearchResult(
                    text=todo.text,
                    status=todo.status.value,
                    section=todo.category,
                    line=todo.line,
                    source=todo.source,
                )
            )

        return TodoListResult(items=items, per_section_limit=per_section_limit)

    def search(self, query: str, include_done: bool = False) -> list[TodoSearchResult]:
        """Search TODOs by keyword.

        Args:
            query: Search query (case-insensitive, matches text and section)
            include_done: Include completed TODOs (default: False)

        Returns:
            List of TodoSearchResult with matching items, sorted by relevance
        """
        from moss.check_todos import TodoChecker, TodoStatus

        checker = TodoChecker(self.root)
        result = checker.check()

        query_lower = query.lower()
        matches = []

        for todo in result.tracked_items:
            # Filter by status
            if not include_done and todo.status == TodoStatus.DONE:
                continue

            # Score by match quality
            score = 0
            text_lower = todo.text.lower()
            section_lower = (todo.category or "").lower()

            if query_lower in text_lower:
                # Direct match in text
                score = 2
            elif query_lower in section_lower:
                # Match in section name
                score = 1
            else:
                # Check for word-level match
                query_words = set(query_lower.split())
                text_words = set(text_lower.split())
                if query_words & text_words:
                    score = 1

            if score > 0:
                matches.append(
                    (
                        score,
                        TodoSearchResult(
                            text=todo.text,
                            status=todo.status.value,
                            section=todo.category,
                            line=todo.line,
                            source=todo.source,
                        ),
                    )
                )

        # Sort by score descending, then by line number
        matches.sort(key=lambda x: (-x[0], x[1].line))
        return [m[1] for m in matches]

    def sections(self) -> list[dict[str, Any]]:
        """List all TODO sections with counts.

        Returns:
            List of dicts with section name, pending count, and done count
        """
        from collections import defaultdict

        from moss.check_todos import TodoChecker, TodoStatus

        checker = TodoChecker(self.root)
        result = checker.check()

        # Count by section
        section_counts: dict[str, dict[str, int]] = defaultdict(lambda: {"pending": 0, "done": 0})

        for todo in result.tracked_items:
            section = todo.category or "Uncategorized"
            if todo.status == TodoStatus.DONE:
                section_counts[section]["done"] += 1
            else:
                section_counts[section]["pending"] += 1

        return [
            {"section": name, "pending": counts["pending"], "done": counts["done"]}
            for name, counts in sorted(section_counts.items())
        ]


@dataclass
class ComplexityAPI:
    """API for cyclomatic complexity analysis.

    Calculates McCabe cyclomatic complexity for Python functions,
    helping identify code that may be difficult to test or maintain.
    """

    root: Path

    def analyze(self, pattern: str = "**/*.py") -> ComplexityReport:
        """Analyze cyclomatic complexity of all Python files.

        Args:
            pattern: Glob pattern for files to analyze (default: all Python files)

        Returns:
            ComplexityReport with complexity metrics for all functions
        """
        from moss.complexity import analyze_complexity

        return analyze_complexity(self.root, pattern=pattern)

    def get_high_risk(self, threshold: int = 10) -> list[dict[str, Any]]:
        """Get functions exceeding a complexity threshold.

        Args:
            threshold: Complexity threshold (default: 10)

        Returns:
            List of function details for high-complexity functions
        """
        report = self.analyze()
        return [f.to_dict() for f in report.functions if f.complexity > threshold]


@dataclass
class ClonesAPI:
    """API for structural clone detection.

    Detects structurally similar code by normalizing AST subtrees and
    comparing hashes. Helps identify code that could potentially be
    abstracted into shared functions.
    """

    root: Path

    def detect(self, level: int = 0, min_lines: int = 3) -> Any:
        """Detect structural clones in the codebase.

        Args:
            level: Elision level (0-3) controlling normalization:
                   0 = names only (exact structural clones)
                   1 = + literals (same structure, different constants)
                   2 = + calls (same pattern, different functions)
                   3 = control flow skeleton only
            min_lines: Minimum function lines to consider (default: 3)

        Returns:
            CloneAnalysis with clone groups and statistics
        """
        from moss.clones import ElisionLevel, detect_clones

        return detect_clones(self.root, level=ElisionLevel(level), min_lines=min_lines)

    def get_groups(self, level: int = 0, min_count: int = 2) -> list[dict[str, Any]]:
        """Get clone groups with at least min_count members.

        Args:
            level: Elision level (0-3)
            min_count: Minimum clones per group (default: 2)

        Returns:
            List of clone group details
        """
        analysis = self.detect(level=level)
        result = analysis.to_dict()
        return [g for g in result.get("groups", []) if g.get("count", 0) >= min_count]


@dataclass
class SecurityAPI:
    """API for security analysis.

    Orchestrates multiple security tools (bandit, semgrep) to detect
    vulnerabilities and security issues in the codebase.
    """

    root: Path

    def analyze(
        self,
        tools: list[str] | None = None,
        min_severity: str = "low",
    ) -> Any:
        """Run security analysis.

        Args:
            tools: List of tools to use (None = all available)
            min_severity: Minimum severity to report ("low", "medium", "high", "critical")

        Returns:
            SecurityAnalysis with findings and summary
        """
        from moss.security import analyze_security

        return analyze_security(self.root, tools=tools, min_severity=min_severity)

    def get_high_severity(self) -> list[dict[str, Any]]:
        """Get high and critical severity findings.

        Returns:
            List of high/critical security findings
        """
        analysis = self.analyze(min_severity="high")
        return [f.to_dict() for f in analysis.findings]


@dataclass
class RefCheckAPI:
    """API for bidirectional reference checking.

    Validates that code files reference their documentation and
    documentation references implementation files. Detects stale
    references where targets have been modified after sources.
    """

    root: Path

    def check(self, staleness_days: int = 30) -> RefCheckResult:
        """Run bidirectional reference check.

        Args:
            staleness_days: Warn if target modified more than N days after source

        Returns:
            RefCheckResult with valid, broken, and stale references
        """
        from moss.check_refs import RefChecker

        checker = RefChecker(self.root, staleness_days=staleness_days)
        return checker.check()

    def check_code_to_docs(self) -> list[dict[str, Any]]:
        """Check only code-to-documentation references.

        Returns:
            List of broken code->doc references
        """
        result = self.check()
        return [r.to_dict() for r in result.code_to_docs_broken]

    def check_docs_to_code(self) -> list[dict[str, Any]]:
        """Check only documentation-to-code references.

        Returns:
            List of broken doc->code references
        """
        result = self.check()
        return [r.to_dict() for r in result.docs_to_code_broken]


@dataclass
class GitHotspotsAPI:
    """API for git hotspot analysis.

    Identifies frequently changed files in the git repository.
    High churn areas may indicate code that needs refactoring.
    """

    root: Path

    def analyze(self, days: int = 90) -> GitHotspotAnalysis:
        """Analyze git history for hot spots.

        Args:
            days: Number of days to analyze (default: 90)

        Returns:
            GitHotspotAnalysis with frequently changed files
        """
        from moss.git_hotspots import analyze_hotspots

        return analyze_hotspots(self.root, days=days)

    def get_top_hotspots(self, days: int = 90, limit: int = 10) -> list[dict[str, Any]]:
        """Get the top N most frequently changed files.

        Args:
            days: Number of days to analyze
            limit: Maximum number of files to return

        Returns:
            List of hotspot details for most frequently changed files
        """
        result = self.analyze(days=days)
        return [h.to_dict() for h in result.hotspots[:limit]]


@dataclass
class ExternalDepsAPI:
    """API for external dependency analysis.

    Analyzes PyPI/npm dependencies including transitive dependencies,
    security vulnerabilities, and license compatibility.
    """

    root: Path

    def analyze(
        self,
        resolve: bool = False,
        check_vulns: bool = False,
        check_licenses: bool = False,
    ) -> DependencyAnalysisResult:
        """Analyze project dependencies.

        Args:
            resolve: If True, resolve full transitive dependency tree
            check_vulns: If True, check for known vulnerabilities via OSV API
            check_licenses: If True, check license compatibility

        Returns:
            DependencyAnalysisResult with dependency information
        """
        from moss.external_deps import ExternalDependencyAnalyzer

        analyzer = ExternalDependencyAnalyzer(self.root)
        return analyzer.analyze(
            resolve=resolve,
            check_vulns=check_vulns,
            check_licenses=check_licenses,
        )

    def list_direct(self) -> list[dict[str, Any]]:
        """List direct dependencies.

        Returns:
            List of direct dependency details
        """
        result = self.analyze()
        return [d.to_dict() for d in result.dependencies]

    def check_security(self) -> list[dict[str, Any]]:
        """Check for security vulnerabilities.

        Returns:
            List of vulnerability details
        """
        result = self.analyze(check_vulns=True)
        return [v.to_dict() for v in result.vulnerabilities]


@dataclass
class WeaknessesAPI:
    """API for architectural weakness analysis.

    Identifies potential issues in codebase architecture:
    - Tight coupling between components
    - Missing abstractions
    - Inconsistent patterns
    - Technical debt indicators
    """

    root: Path

    def analyze(
        self,
        categories: list[str] | None = None,
    ) -> WeaknessAnalysis:
        """Analyze codebase for architectural weaknesses.

        Args:
            categories: Categories to check (None = all)
                Valid categories: coupling, abstraction, pattern,
                hardcoded, error_handling, complexity, duplication

        Returns:
            WeaknessAnalysis with detected weaknesses
        """
        from moss.weaknesses import WeaknessAnalyzer

        analyzer = WeaknessAnalyzer(self.root, categories=categories)
        return analyzer.analyze()

    def format(self, analysis: WeaknessAnalysis) -> str:
        """Format weakness analysis as markdown.

        Args:
            analysis: WeaknessAnalysis to format

        Returns:
            Markdown-formatted report
        """
        from moss.weaknesses import format_weakness_analysis

        return format_weakness_analysis(analysis)


@dataclass
class GuessabilityAPI:
    """API for evaluating codebase structure quality.

    Measures how intuitive and predictable the codebase structure is.
    Can you guess where to find functionality based on its name?
    """

    root: Path

    def analyze(self) -> Any:
        """Analyze codebase guessability.

        Evaluates:
        - Name-content alignment: Do module names reflect their contents?
        - Predictability: Are similar things in similar places?
        - Pattern consistency: Are conventions followed?

        Returns:
            GuessabilityReport with scores and recommendations
        """
        from moss.guessability import GuessabilityAnalyzer

        analyzer = GuessabilityAnalyzer(self.root)
        return analyzer.analyze()

    def score(self) -> Any:
        """Get overall guessability score.

        Returns:
            GuessabilityScore with 'score' (0.0-1.0) and 'grade' (A-F)
        """
        from moss.guessability import GuessabilityScore

        report = self.analyze()
        return GuessabilityScore(score=report.overall_score, grade=report.grade)

    def recommendations(self) -> list[str]:
        """Get guessability improvement recommendations.

        Returns:
            List of actionable recommendations
        """
        report = self.analyze()
        return report.recommendations


@dataclass
class WebAPI:
    """API for token-efficient web fetching and search.

    Provides web content extraction optimized for LLM context.
    Strips HTML noise, extracts main content, caches results.

    Example:
        api = MossAPI.for_project(".")
        content = await api.web.fetch("https://docs.python.org/3/")
        print(f"Tokens: ~{content.token_estimate}")

        results = await api.web.search("python asyncio tutorial")
        print(results.to_compact())
    """

    root: Path
    _fetcher: Any = None
    _searcher: Any = None

    def _get_fetcher(self) -> Any:
        """Get or create the web fetcher."""
        if self._fetcher is None:
            from moss.web import WebFetcher

            self._fetcher = WebFetcher()
        return self._fetcher

    def _get_searcher(self) -> Any:
        """Get or create the web searcher."""
        if self._searcher is None:
            from moss.web import WebSearcher

            self._searcher = WebSearcher()
        return self._searcher

    async def fetch(
        self,
        url: str,
        *,
        use_cache: bool = True,
        extract_main: bool = True,
    ) -> dict[str, Any]:
        """Fetch and extract content from URL.

        Args:
            url: URL to fetch
            use_cache: Check cache first (default: True)
            extract_main: Extract main content vs full page (default: True)

        Returns:
            Dict with url, title, text, token_estimate, metadata
        """
        fetcher = self._get_fetcher()
        content = await fetcher.fetch(url, use_cache=use_cache, extract_main=extract_main)
        return {
            "url": content.url,
            "title": content.title,
            "text": content.text,
            "token_estimate": content.token_estimate,
            "metadata": content.metadata,
        }

    async def search(
        self,
        query: str,
        max_results: int = 5,
    ) -> dict[str, Any]:
        """Search the web with token-efficient results.

        Args:
            query: Search query
            max_results: Maximum number of results (default: 5)

        Returns:
            Dict with query, results list, and compact string representation
        """
        from moss.web import WebSearcher

        searcher = WebSearcher(max_results=max_results)
        results = await searcher.search(query)
        return {
            "query": results.query,
            "results": [r.to_dict() for r in results.results],
            "total": results.total,
            "compact": results.to_compact(),
            "token_estimate": results.token_estimate,
        }

    def extract_content(self, html: str, url: str = "") -> dict[str, Any]:
        """Extract clean content from HTML string.

        Args:
            html: Raw HTML content
            url: Optional URL for metadata

        Returns:
            Dict with title, text, token_estimate, metadata
        """
        from moss.web import extract_content

        content = extract_content(html, url)
        return {
            "url": content.url,
            "title": content.title,
            "text": content.text,
            "token_estimate": content.token_estimate,
            "metadata": content.metadata,
        }

    def clear_cache(self) -> int:
        """Clear the web content cache.

        Returns:
            Number of cached items cleared
        """
        fetcher = self._get_fetcher()
        return fetcher.cache.clear()


class RAGAPI:
    """API for RAG (Retrieval-Augmented Generation) semantic search.

    Provides semantic code search capabilities using vector embeddings.
    Index your codebase once, then search with natural language queries.
    """

    root: Path
    _index: RAGIndex | None = None

    def _get_index(self) -> RAGIndex:
        """Get or create the RAG index."""
        if self._index is None:
            from moss.rag import RAGIndex

            self._index = RAGIndex(self.root)
        return self._index

    async def index(
        self,
        path: str | Path | None = None,
        patterns: list[str] | None = None,
        force: bool = False,
    ) -> int:
        """Index files for semantic search.

        Args:
            path: Directory to index (defaults to project root)
            patterns: Glob patterns to include (default: code and docs)
            force: Re-index even if content hasn't changed

        Returns:
            Number of chunks indexed
        """
        idx = self._get_index()
        target = Path(path) if path else None
        return await idx.index(path=target, patterns=patterns, force=force)

    async def search(
        self,
        query: str,
        limit: int = 10,
        mode: str = "hybrid",
        kind: str | None = None,
    ) -> list[SearchResult]:
        """Search the index with natural language or code queries.

        Args:
            query: Natural language or code query
            limit: Maximum results to return
            mode: Search mode - "hybrid", "embedding", or "tfidf"
            kind: Filter by symbol kind (e.g., "function", "class", "module")

        Returns:
            List of SearchResult objects with file paths, scores, and snippets
        """
        idx = self._get_index()
        return await idx.search(query, limit=limit, mode=mode, kind=kind)

    async def stats(self) -> IndexStats:
        """Get index statistics.

        Returns:
            IndexStats with document count, files indexed, and backend info
        """
        idx = self._get_index()
        return await idx.stats()

    async def clear(self) -> dict[str, bool]:
        """Clear the index.

        Returns:
            Dict with 'success' boolean
        """
        idx = self._get_index()
        await idx.clear()
        return {"success": True}


@dataclass
class SymbolMatch:
    """A symbol found in the codebase.

    Attributes:
        name: Symbol name
        kind: Symbol kind (function, class, method, variable)
        file_path: Path to the file containing the symbol
        line: Line number where the symbol is defined
        signature: Function/method signature if applicable
    """

    name: str
    kind: str
    file_path: str
    line: int
    signature: str = ""

    def to_compact(self) -> str:
        """Return a compact string representation."""
        sig = f" {self.signature}" if self.signature else ""
        return f"[{self.kind}] {self.name}{sig} ({self.file_path}:{self.line})"


@dataclass
class GrepMatch:
    """A text match from grep search.

    Attributes:
        file_path: Path to the file containing the match
        line_number: Line number of the match
        line_content: Content of the matching line
        match_start: Start position of the match within the line
        match_end: End position of the match within the line
    """

    file_path: str
    line_number: int
    line_content: str
    match_start: int = 0
    match_end: int = 0

    def to_compact(self) -> str:
        """Return a compact string representation."""
        return f"{self.file_path}:{self.line_number}: {self.line_content.strip()}"


@dataclass
class GrepResult:
    """Result of a grep search.

    Attributes:
        matches: List of matches found
        total_matches: Total number of matches
        files_searched: Number of files searched
    """

    matches: list[GrepMatch]
    total_matches: int
    files_searched: int

    def to_compact(self) -> str:
        """Return a compact string representation."""
        lines = [f"Found {self.total_matches} matches in {self.files_searched} files:"]
        for m in self.matches[:20]:
            lines.append(f"  {m.to_compact()}")
        if len(self.matches) > 20:
            lines.append(f"  ... and {len(self.matches) - 20} more")
        return "\n".join(lines)


@dataclass
class FileMatch:
    """Result of resolving a file name with DWIM.

    Attributes:
        path: Resolved file path (relative to project root)
        confidence: Match confidence (0.0 to 1.0)
        message: Optional explanation of the match
    """

    path: str
    confidence: float
    message: str | None = None

    def to_compact(self) -> str:
        """Return a compact string representation."""
        if self.message:
            return f"{self.path} ({self.message})"
        return self.path


@dataclass
class SearchAPI:
    """API for codebase search operations.

    Provides unified search across the codebase using structural
    analysis (skeleton/anchors), text patterns (grep), and file
    patterns (glob). Use this instead of raw grep/glob for better
    integration with Moss.
    """

    root: Path

    def find_symbols(
        self,
        name: str,
        kind: str | None = None,
        fuzzy: bool = True,
        limit: int = 50,
    ) -> list[SymbolMatch]:
        """Find symbols by name across the codebase.

        Searches all Python files for functions, classes, methods,
        and variables matching the given name.

        Args:
            name: Symbol name to search for (supports partial matching)
            kind: Filter by kind: function, class, method, variable
            fuzzy: If True, match partial names; if False, exact match only
            limit: Maximum number of results to return

        Returns:
            List of SymbolMatch objects sorted by relevance
        """
        import fnmatch

        from moss.skeleton import extract_python_skeleton

        results: list[SymbolMatch] = []
        pattern = name.lower()

        # Find all Python files
        for py_file in self.root.rglob("*.py"):
            # Skip hidden dirs, venv, etc
            parts = py_file.parts
            skip_dirs = ("venv", "node_modules", "__pycache__")
            if any(p.startswith(".") or p in skip_dirs for p in parts):
                continue

            try:
                source = py_file.read_text()
                symbols = extract_python_skeleton(source)
            except Exception:
                continue

            for sym in symbols:
                sym_name_lower = sym.name.lower()

                # Check match
                if fuzzy:
                    in_name = pattern in sym_name_lower
                    matched = in_name or fnmatch.fnmatch(sym_name_lower, f"*{pattern}*")
                else:
                    matched = sym_name_lower == pattern

                if not matched:
                    continue

                # Filter by kind if specified
                if kind and sym.kind != kind:
                    continue

                # Build signature for functions/methods
                sig = ""
                if sym.kind in ("function", "method") and sym.signature:
                    sig = sym.signature

                rel_path = str(py_file.relative_to(self.root))
                results.append(
                    SymbolMatch(
                        name=sym.name,
                        kind=sym.kind,
                        file_path=rel_path,
                        line=sym.lineno,
                        signature=sig,
                    )
                )

                if len(results) >= limit:
                    return results

        # Sort by relevance: exact matches first, then by name length
        results.sort(key=lambda m: (m.name.lower() != pattern, len(m.name), m.name))
        return results

    def grep(
        self,
        pattern: str,
        path: str | None = None,
        glob: str | None = None,
        context_lines: int = 0,
        limit: int = 100,
        ignore_case: bool = False,
    ) -> GrepResult:
        """Search for text patterns in files.

        Uses regex pattern matching to find text in files.
        Similar to grep/ripgrep but integrated with Moss.

        Args:
            pattern: Regex pattern to search for
            path: Directory to search in (defaults to project root)
            glob: Glob pattern to filter files (e.g., "*.py", "**/*.ts")
            context_lines: Number of context lines before/after match
            limit: Maximum number of matches to return
            ignore_case: If True, perform case-insensitive matching

        Returns:
            GrepResult with matches and statistics
        """
        import re

        search_path = Path(path) if path else self.root
        if not search_path.is_absolute():
            search_path = self.root / search_path

        flags = re.IGNORECASE if ignore_case else 0
        try:
            regex = re.compile(pattern, flags)
        except re.error:
            return GrepResult(matches=[], total_matches=0, files_searched=0)

        matches: list[GrepMatch] = []
        files_searched = 0
        total_matches = 0

        # Determine file pattern
        file_pattern = glob or "**/*"

        for file_path in search_path.glob(file_pattern):
            if not file_path.is_file():
                continue

            # Skip binary files and hidden dirs
            parts = file_path.parts
            skip_dirs = ("venv", "node_modules", "__pycache__")
            if any(p.startswith(".") or p in skip_dirs for p in parts):
                continue

            # Skip common binary extensions
            binary_exts = (
                ".pyc",
                ".pyo",
                ".so",
                ".dll",
                ".exe",
                ".bin",
                ".jpg",
                ".png",
                ".gif",
                ".pdf",
            )
            if file_path.suffix.lower() in binary_exts:
                continue

            try:
                content = file_path.read_text(errors="ignore")
            except Exception:
                continue

            files_searched += 1
            lines = content.split("\n")

            for i, line in enumerate(lines, 1):
                match = regex.search(line)
                if match:
                    total_matches += 1
                    if len(matches) < limit:
                        rel_path = str(file_path.relative_to(self.root))
                        matches.append(
                            GrepMatch(
                                file_path=rel_path,
                                line_number=i,
                                line_content=line,
                                match_start=match.start(),
                                match_end=match.end(),
                            )
                        )

        return GrepResult(
            matches=matches,
            total_matches=total_matches,
            files_searched=files_searched,
        )

    def find_files(
        self,
        pattern: str,
        path: str | None = None,
        limit: int = 100,
    ) -> list[str]:
        """Find files matching a glob pattern.

        Args:
            pattern: Glob pattern (e.g., "**/*.py", "src/**/*.ts")
            path: Directory to search in (defaults to project root)
            limit: Maximum number of results to return

        Returns:
            List of relative file paths matching the pattern
        """
        search_path = Path(path) if path else self.root
        if not search_path.is_absolute():
            search_path = self.root / search_path

        results: list[str] = []
        for file_path in search_path.glob(pattern):
            if not file_path.is_file():
                continue

            # Skip hidden dirs
            parts = file_path.parts
            if any(p.startswith(".") for p in parts):
                continue

            rel_path = str(file_path.relative_to(self.root))
            results.append(rel_path)

            if len(results) >= limit:
                break

        return sorted(results)

    def find_definitions(
        self,
        name: str,
        kind: str | None = None,
    ) -> list[SymbolMatch]:
        """Find where a symbol is defined.

        Shortcut for find_symbols with exact matching.

        Args:
            name: Exact symbol name to find
            kind: Filter by kind: function, class, method

        Returns:
            List of SymbolMatch objects for definitions
        """
        return self.find_symbols(name, kind=kind, fuzzy=False)

    def find_usages(
        self,
        name: str,
        exclude_definitions: bool = True,
        limit: int = 100,
    ) -> GrepResult:
        """Find usages of a symbol in the codebase.

        Searches for references to a symbol name, optionally
        excluding the definition sites.

        Args:
            name: Symbol name to find usages of
            exclude_definitions: If True, exclude definition lines
            limit: Maximum number of results to return

        Returns:
            GrepResult with usage locations
        """
        import re

        # Search for the name as a word boundary match
        pattern = rf"\b{re.escape(name)}\b"
        result = self.grep(pattern, glob="**/*.py", limit=limit * 2)

        if not exclude_definitions:
            return result

        # Filter out definitions (lines with def/class followed by the name)
        def_pattern = rf"^\s*(def|class|async\s+def)\s+{re.escape(name)}\b"
        def_regex = re.compile(def_pattern)

        filtered = [m for m in result.matches if not def_regex.match(m.line_content)]

        return GrepResult(
            matches=filtered[:limit],
            total_matches=len(filtered),
            files_searched=result.files_searched,
        )

    def _get_all_files(self) -> list[str]:
        """Get all files in the project, excluding common ignored directories."""
        all_files: list[str] = []
        skip_dirs = ("venv", "node_modules", "__pycache__", ".git")
        for file_path in self.root.rglob("*"):
            if not file_path.is_file():
                continue
            parts = file_path.parts
            if any(p.startswith(".") or p in skip_dirs for p in parts):
                continue
            all_files.append(str(file_path.relative_to(self.root)))
        return all_files

    def _fuzzy_match_file(self, name: str, all_files: list[str]) -> tuple[str | None, float]:
        """Find best fuzzy match for a filename."""
        from difflib import SequenceMatcher

        def similarity(a: str, b: str) -> float:
            return SequenceMatcher(None, a.lower(), b.lower()).ratio()

        name_lower = name.lower()
        name_no_ext = name.rsplit(".", 1)[0] if "." in name else name
        best_match = None
        best_score = 0.0

        for f in all_files:
            fname = f.split("/")[-1]
            fname_no_ext = fname.rsplit(".", 1)[0] if "." in fname else fname

            # Best of: full name, without extension, substring boost
            score = max(
                similarity(name, fname),
                similarity(name_no_ext, fname_no_ext),
                0.8 if name_lower in fname.lower() else 0.0,
            )

            if score > best_score:
                best_score = score
                best_match = f

        return best_match, best_score

    def resolve_file(
        self,
        name: str,
        extensions: list[str] | None = None,
    ) -> FileMatch:
        """Resolve a file name with DWIM (Do What I Mean).

        Handles typos, missing extensions, and partial paths.

        Args:
            name: File name, partial path, or module name to resolve
            extensions: Extensions to try if name has none (default: .py, .ts, .js, .tsx, .jsx)

        Returns:
            FileMatch with resolved path and confidence
        """
        if extensions is None:
            extensions = [".py", ".ts", ".js", ".tsx", ".jsx", ".go", ".rs", ".md"]

        all_files = self._get_all_files()
        name_lower = name.lower()

        # 1. Exact match
        if name in all_files:
            return FileMatch(path=name, confidence=1.0)

        # 2. Try with extensions (if no extension provided)
        has_ext = "." in name.split("/")[-1]
        if not has_ext:
            for ext in extensions:
                candidate = name + ext
                if candidate in all_files:
                    return FileMatch(path=candidate, confidence=1.0, message=f"added {ext}")
                # Case-insensitive with extension
                for f in all_files:
                    if f.lower() == candidate.lower():
                        return FileMatch(path=f, confidence=0.95, message="case-insensitive match")

        # 3. Case-insensitive match
        for f in all_files:
            if f.lower() == name_lower:
                return FileMatch(path=f, confidence=0.95, message="case-insensitive")

        # 4. Partial path match (prefer non-test, shorter paths)
        partial = [f for f in all_files if f.endswith("/" + name) or f.endswith(name)]
        if partial:
            partial.sort(key=lambda p: (p.startswith("tests/"), len(p)))
            return FileMatch(path=partial[0], confidence=0.9, message="partial path")

        # 5. Fuzzy match
        best_match, best_score = self._fuzzy_match_file(name, all_files)
        if best_match and best_score >= 0.85:
            return FileMatch(path=best_match, confidence=best_score, message="auto-corrected")
        if best_match and best_score >= 0.5:
            msg = f"did you mean '{best_match}'?"
            return FileMatch(path=best_match, confidence=best_score, message=msg)

        return FileMatch(path=name, confidence=0.0, message="no match found")

    def _resolve_path(self, file_path: str | Path) -> Path:
        """Resolve a file path relative to the project root."""
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path


@dataclass
class ToolMatchResult:
    """Result of matching a query to a tool.

    Attributes:
        tool: Canonical tool name
        confidence: Match confidence (0.0 to 1.0)
        message: Optional explanation of the match
    """

    tool: str
    confidence: float
    message: str | None = None

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        msg = f" ({self.message})" if self.message else ""
        return f"{self.tool} ({self.confidence:.0%} confidence){msg}"


@dataclass
class ToolInfoResult:
    """Information about a tool.

    Attributes:
        name: Tool name
        description: Human-readable description
        keywords: Search keywords for this tool
        parameters: Parameter names
        aliases: Alternative names that map to this tool
    """

    name: str
    description: str
    keywords: list[str]
    parameters: list[str]
    aliases: list[str]

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        aliases = f" (aka: {', '.join(self.aliases)})" if self.aliases else ""
        params = f" [{', '.join(self.parameters)}]" if self.parameters else ""
        return f"{self.name}{aliases}: {self.description}{params}"


@dataclass
class ToolListResult:
    """Result of listing available tools.

    Attributes:
        tools: List of tool information
    """

    tools: list[ToolInfoResult]

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        lines = []
        for t in self.tools:
            aliases = f" (aka: {', '.join(t.aliases)})" if t.aliases else ""
            lines.append(f"- {t.name}{aliases}: {t.description}")
        return f"{len(self.tools)} tools:\n" + "\n".join(lines)

    def to_dict(self) -> dict:
        """Return dict representation."""
        return {
            "tools": [
                {
                    "name": t.name,
                    "description": t.description,
                    "keywords": t.keywords,
                    "parameters": t.parameters,
                    "aliases": t.aliases,
                }
                for t in self.tools
            ],
            "count": len(self.tools),
        }


@dataclass
class DWIMAPI:
    """START HERE - Tool discovery and routing for Moss.

    Don't know which Moss tool to use? Ask DWIM! This API helps you find
    the right tool for any task using natural language queries.

    Example: "summarize the codebase" → health_summarize
    Example: "check for TODOs" → health_check_todos
    Example: "find complex functions" → complexity_analyze

    Features:
    - Natural language queries: describe what you want, get tool suggestions
    - Semantic aliases: map conceptual names to canonical tools
    - Fuzzy matching: handle typos and variations
    - Confidence scoring: know when to auto-correct vs suggest
    """

    def resolve_tool(self, tool_name: str) -> ToolMatchResult:
        """Resolve a tool name to its canonical form.

        Handles exact matches, semantic aliases, and fuzzy matching
        for typos.

        Args:
            tool_name: Tool name to resolve (may be misspelled or alias)

        Returns:
            ToolMatchResult with canonical name and confidence
        """
        from moss.dwim import resolve_tool

        match = resolve_tool(tool_name)
        return ToolMatchResult(
            tool=match.tool,
            confidence=match.confidence,
            message=match.message,
        )

    def analyze_intent(self, query: str, top_k: int = 3) -> list[ToolMatchResult]:
        """Find the right Moss tool for any task using natural language.

        USE THIS FIRST when you don't know which tool to use! Describe what
        you want to do and get ranked suggestions.

        Examples:
        - "summarize the codebase" → health_summarize
        - "show file structure" → skeleton_format
        - "find TODOs" → health_check_todos
        - "check code complexity" → complexity_analyze

        Args:
            query: Natural language description of what you want to do
            top_k: Maximum number of suggestions to return

        Returns:
            List of ToolMatchResult sorted by confidence (highest first)
        """
        from moss.dwim import analyze_intent

        matches = analyze_intent(query)[:top_k]
        return [
            ToolMatchResult(
                tool=m.tool,
                confidence=m.confidence,
                message=m.message,
            )
            for m in matches
        ]

    def list_tools(self) -> ToolListResult:
        """List all available tools with their metadata.

        Returns:
            ToolListResult with descriptions, keywords, etc.
        """
        from moss.dwim import TOOL_ALIASES, TOOL_REGISTRY

        results = []
        for name, info in TOOL_REGISTRY.items():
            aliases = [alias for alias, target in TOOL_ALIASES.items() if target == name]
            results.append(
                ToolInfoResult(
                    name=info.name,
                    description=info.description,
                    keywords=info.keywords,
                    parameters=info.parameters,
                    aliases=aliases,
                )
            )
        return ToolListResult(tools=results)

    def get_tool_info(self, tool_name: str) -> ToolInfoResult | None:
        """Get detailed information about a specific tool.

        Args:
            tool_name: Tool name (can be alias or misspelled)

        Returns:
            ToolInfoResult or None if tool not found
        """
        from moss.dwim import get_tool_info

        info = get_tool_info(tool_name)
        if info is None:
            return None

        return ToolInfoResult(
            name=info["name"],
            description=info["description"],
            keywords=info["keywords"],
            parameters=info["parameters"],
            aliases=info.get("aliases", []),
        )


@dataclass
class MossAPI:
    """Unified API for Moss functionality.

    Provides organized access to all Moss capabilities through
    domain-specific sub-APIs.

    Example:
        api = MossAPI.for_project("/path/to/project")

        # Extract code structure
        skeleton = api.skeleton.extract("src/main.py")

        # Analyze dependencies
        deps = api.dependencies.analyze()

        # Check project health
        health = api.health.check()
        print(f"Health grade: {health.health_grade}")
    """

    root: Path

    # Sub-APIs (initialized lazily)
    _skeleton: SkeletonAPI | None = None
    _tree: TreeAPI | None = None
    _anchor: AnchorAPI | None = None
    _patch: PatchAPI | None = None
    _dependencies: DependencyAPI | None = None
    _cfg: CFGAPI | None = None
    _validation: ValidationAPI | None = None
    _git: GitAPI | None = None
    _context: ContextAPI | None = None
    _health: HealthAPI | None = None
    _todo: TodoAPI | None = None
    _dwim: DWIMAPI | None = None
    _complexity: ComplexityAPI | None = None
    _clones: ClonesAPI | None = None
    _security: SecurityAPI | None = None
    _ref_check: RefCheckAPI | None = None
    _git_hotspots: GitHotspotsAPI | None = None
    _external_deps: ExternalDepsAPI | None = None
    _weaknesses: WeaknessesAPI | None = None
    _rag: RAGAPI | None = None
    _web: WebAPI | None = None
    _search: SearchAPI | None = None
    _guessability: GuessabilityAPI | None = None

    @classmethod
    def for_project(cls, path: str | Path) -> MossAPI:
        """Create a MossAPI instance for a project directory.

        Args:
            path: Path to the project root

        Returns:
            MossAPI instance configured for the project
        """
        return cls(root=Path(path).resolve())

    @property
    def skeleton(self) -> SkeletonAPI:
        """Access skeleton extraction functionality."""
        if self._skeleton is None:
            self._skeleton = SkeletonAPI(root=self.root)
        return self._skeleton

    @property
    def tree(self) -> TreeAPI:
        """Access file tree visualization functionality."""
        if self._tree is None:
            self._tree = TreeAPI(root=self.root)
        return self._tree

    @property
    def anchor(self) -> AnchorAPI:
        """Access anchor finding functionality."""
        if self._anchor is None:
            self._anchor = AnchorAPI(root=self.root)
        return self._anchor

    @property
    def patch(self) -> PatchAPI:
        """Access patching functionality."""
        if self._patch is None:
            self._patch = PatchAPI(root=self.root)
        return self._patch

    @property
    def dependencies(self) -> DependencyAPI:
        """Access dependency analysis functionality."""
        if self._dependencies is None:
            self._dependencies = DependencyAPI(root=self.root)
        return self._dependencies

    @property
    def cfg(self) -> CFGAPI:
        """Access control flow graph functionality."""
        if self._cfg is None:
            self._cfg = CFGAPI(root=self.root)
        return self._cfg

    @property
    def validation(self) -> ValidationAPI:
        """Access validation functionality."""
        if self._validation is None:
            self._validation = ValidationAPI(root=self.root)
        return self._validation

    @property
    def git(self) -> GitAPI:
        """Access shadow git functionality."""
        if self._git is None:
            self._git = GitAPI(root=self.root)
        return self._git

    @property
    def context(self) -> ContextAPI:
        """Access context compilation functionality."""
        if self._context is None:
            self._context = ContextAPI(root=self.root)
        return self._context

    @property
    def health(self) -> HealthAPI:
        """Access health analysis functionality."""
        if self._health is None:
            self._health = HealthAPI(root=self.root)
        return self._health

    @property
    def todo(self) -> TodoAPI:
        """Access TODO search and management functionality."""
        if self._todo is None:
            self._todo = TodoAPI(root=self.root)
        return self._todo

    @property
    def dwim(self) -> DWIMAPI:
        """Access semantic tool routing functionality."""
        if self._dwim is None:
            self._dwim = DWIMAPI()
        return self._dwim

    @property
    def complexity(self) -> ComplexityAPI:
        """Access cyclomatic complexity analysis functionality."""
        if self._complexity is None:
            self._complexity = ComplexityAPI(root=self.root)
        return self._complexity

    @property
    def clones(self) -> ClonesAPI:
        """Access structural clone detection functionality."""
        if self._clones is None:
            self._clones = ClonesAPI(root=self.root)
        return self._clones

    @property
    def security(self) -> SecurityAPI:
        """Access security analysis functionality."""
        if self._security is None:
            self._security = SecurityAPI(root=self.root)
        return self._security

    @property
    def ref_check(self) -> RefCheckAPI:
        """Access bidirectional reference checking functionality."""
        if self._ref_check is None:
            self._ref_check = RefCheckAPI(root=self.root)
        return self._ref_check

    @property
    def git_hotspots(self) -> GitHotspotsAPI:
        """Access git hotspot analysis functionality."""
        if self._git_hotspots is None:
            self._git_hotspots = GitHotspotsAPI(root=self.root)
        return self._git_hotspots

    @property
    def external_deps(self) -> ExternalDepsAPI:
        """Access external dependency analysis functionality."""
        if self._external_deps is None:
            self._external_deps = ExternalDepsAPI(root=self.root)
        return self._external_deps

    @property
    def weaknesses(self) -> WeaknessesAPI:
        """Access architectural weakness analysis functionality."""
        if self._weaknesses is None:
            self._weaknesses = WeaknessesAPI(root=self.root)
        return self._weaknesses

    @property
    def rag(self) -> RAGAPI:
        """Access RAG (semantic search) functionality."""
        if self._rag is None:
            self._rag = RAGAPI(root=self.root)
        return self._rag

    @property
    def web(self) -> WebAPI:
        """Access token-efficient web fetching and search."""
        if self._web is None:
            self._web = WebAPI(root=self.root)
        return self._web

    @property
    def search(self) -> SearchAPI:
        """Access codebase search functionality."""
        if self._search is None:
            self._search = SearchAPI(root=self.root)
        return self._search

    @property
    def guessability(self) -> GuessabilityAPI:
        """Access codebase guessability analysis."""
        if self._guessability is None:
            self._guessability = GuessabilityAPI(root=self.root)
        return self._guessability


# Convenience alias
API = MossAPI
