"""moss-intelligence: Code understanding and analysis.

Stateless, pure code intelligence. No LLM, no memory, no side effects.

Example:
    from moss_intelligence import Intelligence

    intel = Intelligence("/path/to/project")

    # Views
    skeleton = intel.skeleton("src/main.py")
    tree = intel.tree("src/", depth=2)

    # Analysis
    complexity = intel.complexity("src/")
    deps = intel.dependencies("src/main.py")
"""

from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .skeleton import Symbol
    from .complexity import ComplexityReport
    from .dependency_analysis import DependencyAnalysis


class Intelligence:
    """Main entry point for code intelligence.

    Provides stateless, pure code understanding:
    - Views: skeleton, tree, source
    - Analysis: complexity, security, dependencies
    - Search: symbols, references
    """

    def __init__(self, root: str | Path):
        """Initialize intelligence for a project.

        Args:
            root: Path to project root directory
        """
        self.root = Path(root).resolve()
        if not self.root.is_dir():
            raise ValueError(f"Not a directory: {self.root}")

    def _resolve_path(self, path: str | Path) -> Path:
        """Resolve path relative to project root."""
        p = Path(path)
        if not p.is_absolute():
            p = self.root / p
        return p

    # === Views ===

    def skeleton(self, path: str | Path, include_private: bool = False) -> "list[Symbol]":
        """Extract skeleton (signatures only) from a file.

        Args:
            path: Path to file (relative to root or absolute)
            include_private: Include private (_prefixed) symbols

        Returns:
            List of Symbol objects
        """
        from .skeleton import extract_python_skeleton

        resolved = self._resolve_path(path)
        source = resolved.read_text()
        return extract_python_skeleton(source, include_private=include_private)

    def tree(self, path: str | Path = ".", depth: int = 2) -> str:
        """Get tree view of directory structure.

        Args:
            path: Directory path
            depth: Maximum depth to traverse

        Returns:
            Formatted tree string
        """
        from .tree import format_tree

        resolved = self._resolve_path(path)
        return format_tree(resolved, max_depth=depth)

    # === Analysis ===

    def complexity(self, path: str | Path = ".") -> "ComplexityReport":
        """Analyze cyclomatic complexity.

        Args:
            path: File or directory to analyze

        Returns:
            ComplexityReport with per-function metrics
        """
        from .complexity import analyze_complexity

        resolved = self._resolve_path(path)
        return analyze_complexity(resolved)

    def security(self, path: str | Path = ".") -> dict:
        """Run security analysis.

        Args:
            path: File or directory to analyze

        Returns:
            Security findings
        """
        from .security import SecurityAnalyzer

        resolved = self._resolve_path(path)
        analyzer = SecurityAnalyzer(resolved)
        return analyzer.analyze()

    def dependencies(self, path: str | Path) -> "DependencyAnalysis":
        """Analyze dependencies and imports.

        Args:
            path: File or directory to analyze

        Returns:
            DependencyAnalysis with graph and metrics
        """
        from .dependency_analysis import DependencyAnalyzer

        resolved = self._resolve_path(path)
        analyzer = DependencyAnalyzer(resolved)
        return analyzer.analyze()

    def clones(self, path: str | Path = ".", level: int = 0) -> dict:
        """Detect code clones/duplicates.

        Args:
            path: Directory to analyze
            level: Elision level (0-3)

        Returns:
            Clone analysis results
        """
        from .clones import detect_clones, ElisionLevel

        resolved = self._resolve_path(path)
        return detect_clones(resolved, level=ElisionLevel(level))

    # === Summaries ===

    def summarize(self, path: str | Path = ".") -> dict:
        """Generate project summary.

        Args:
            path: Directory to summarize

        Returns:
            ProjectSummary with hierarchical structure
        """
        from .summarize import Summarizer

        resolved = self._resolve_path(path)
        summarizer = Summarizer()
        return summarizer.summarize_project(resolved)

    # === Structural Edit ===

    def edit(self, path: str | Path, changes: dict) -> dict:
        """Apply structural edits to a file.

        Args:
            path: File to edit
            changes: Structured change specification

        Returns:
            Edit result with diff
        """
        from .edit import EditAPI

        resolved = self._resolve_path(path)
        api = EditAPI(self.root)
        # TODO: translate changes dict to EditAPI calls
        raise NotImplementedError("Structural edit API pending")


# Re-export key types
from .skeleton import Symbol
from .complexity import ComplexityReport, FunctionComplexity
from .dependency_analysis import DependencyAnalysis

__all__ = [
    "Intelligence",
    "Symbol",
    "ComplexityReport",
    "FunctionComplexity",
    "DependencyAnalysis",
]
