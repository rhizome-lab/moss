"""Dependency analysis for project health.

Builds on the dependency graph to detect:
- Circular dependencies
- God modules (high fan-in)
- Orphan modules (nothing imports them)
- Coupling metrics
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


def find_source_dir(root: Path) -> Path | None:
    """Find the main source directory in a project.

    Looks for common source directory patterns:
    - src/<package>/ with __init__.py
    - lib/<package>/ with __init__.py
    - <root>/<package>/ with __init__.py
    - Any directory with .py files

    Args:
        root: Project root directory

    Returns:
        Path to the source directory, or None if not found
    """
    for candidate in [root / "src", root / "lib", root]:
        if candidate.exists():
            for subdir in candidate.iterdir():
                if subdir.is_dir() and (subdir / "__init__.py").exists():
                    return subdir
            if list(candidate.glob("*.py")):
                return candidate
    return None


@dataclass
class CircularDependency:
    """A circular dependency chain."""

    cycle: list[str]  # Module names in the cycle

    @property
    def description(self) -> str:
        return " -> ".join([*self.cycle, self.cycle[0]])


@dataclass
class ModuleMetrics:
    """Metrics for a single module."""

    name: str
    fan_in: int = 0  # How many modules import this
    fan_out: int = 0  # How many modules this imports
    importers: list[str] = field(default_factory=list)
    imports: list[str] = field(default_factory=list)

    @property
    def coupling(self) -> float:
        """Coupling score (fan_in + fan_out)."""
        return self.fan_in + self.fan_out

    @property
    def instability(self) -> float:
        """Instability metric: fan_out / (fan_in + fan_out).

        0 = maximally stable (everything depends on it, it depends on nothing)
        1 = maximally unstable (depends on everything, nothing depends on it)
        """
        total = self.fan_in + self.fan_out
        if total == 0:
            return 0.5  # Isolated module
        return self.fan_out / total


@dataclass
class DependencyAnalysis:
    """Results of dependency analysis."""

    # Raw graph
    graph: dict[str, list[str]] = field(default_factory=dict)
    all_modules: set[str] = field(default_factory=set)

    # Analysis results
    circular_deps: list[CircularDependency] = field(default_factory=list)
    module_metrics: dict[str, ModuleMetrics] = field(default_factory=dict)

    # Summary stats
    total_modules: int = 0
    total_edges: int = 0

    @property
    def god_modules(self) -> list[ModuleMetrics]:
        """Modules with unusually high fan-in (top 10% or fan_in > 5)."""
        if not self.module_metrics:
            return []
        metrics = list(self.module_metrics.values())
        threshold = max(5, sorted(m.fan_in for m in metrics)[-max(1, len(metrics) // 10)])
        return sorted(
            [m for m in metrics if m.fan_in >= threshold],
            key=lambda m: m.fan_in,
            reverse=True,
        )

    @property
    def orphan_modules(self) -> list[str]:
        """Modules that nothing imports (fan_in == 0), excluding entry points."""
        entry_points = {"__main__", "__init__", "cli", "main"}
        return sorted(
            name
            for name, m in self.module_metrics.items()
            if m.fan_in == 0
            and not any(ep in name.lower() for ep in entry_points)
            and m.fan_out > 0  # Has imports, so not just a data file
        )

    @property
    def coupling_density(self) -> float:
        """Overall coupling: edges / (modules * (modules - 1))."""
        n = self.total_modules
        if n <= 1:
            return 0.0
        max_edges = n * (n - 1)
        return self.total_edges / max_edges

    @property
    def has_issues(self) -> bool:
        """Whether there are any dependency issues."""
        return bool(self.circular_deps) or bool(self.god_modules)

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        parts = [f"{self.total_modules} modules, {self.total_edges} edges"]
        parts.append(f"coupling: {self.coupling_density:.1%}")
        if self.circular_deps:
            parts.append(f"{len(self.circular_deps)} circular deps")
        if self.god_modules:
            parts.append(f"{len(self.god_modules)} god modules")
        if self.orphan_modules:
            parts.append(f"{len(self.orphan_modules)} orphans")
        return " | ".join(parts)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON output."""
        return {
            "total_modules": self.total_modules,
            "total_edges": self.total_edges,
            "coupling_density": self.coupling_density,
            "circular_dependencies": [
                {"cycle": cd.cycle, "description": cd.description} for cd in self.circular_deps
            ],
            "god_modules": [
                {"name": m.name, "fan_in": m.fan_in, "importers": m.importers}
                for m in self.god_modules
            ],
            "orphan_modules": self.orphan_modules,
            "module_metrics": {
                name: {
                    "fan_in": m.fan_in,
                    "fan_out": m.fan_out,
                    "instability": m.instability,
                }
                for name, m in self.module_metrics.items()
            },
        }


class DependencyAnalyzer:
    """Analyze dependencies in a Python project."""

    def __init__(self, root: Path):
        self.root = root.resolve()

    def analyze(self) -> DependencyAnalysis:
        """Run full dependency analysis."""
        from moss.dependencies import build_dependency_graph

        # Find the source directory
        src_dir = find_source_dir(self.root)
        if not src_dir:
            return DependencyAnalysis()

        # Build the graph
        graph = build_dependency_graph(str(src_dir), internal_only=True)

        # Collect all modules (both importers and imported)
        all_modules: set[str] = set(graph.keys())
        for imports in graph.values():
            all_modules.update(imports)

        # Build metrics for each module
        module_metrics: dict[str, ModuleMetrics] = {}
        for module in all_modules:
            module_metrics[module] = ModuleMetrics(name=module)

        # Calculate fan-in and fan-out
        for module, imports in graph.items():
            if module in module_metrics:
                module_metrics[module].fan_out = len(imports)
                module_metrics[module].imports = imports

            for imp in imports:
                if imp in module_metrics:
                    module_metrics[imp].fan_in += 1
                    module_metrics[imp].importers.append(module)

        # Find circular dependencies
        circular_deps = self._find_cycles(graph)

        # Count total edges
        total_edges = sum(len(imports) for imports in graph.values())

        return DependencyAnalysis(
            graph=graph,
            all_modules=all_modules,
            circular_deps=circular_deps,
            module_metrics=module_metrics,
            total_modules=len(all_modules),
            total_edges=total_edges,
        )

    def _find_cycles(self, graph: dict[str, list[str]]) -> list[CircularDependency]:
        """Find all cycles in the dependency graph using DFS."""
        cycles: list[CircularDependency] = []
        visited: set[str] = set()
        rec_stack: set[str] = set()
        path: list[str] = []

        def dfs(node: str) -> None:
            visited.add(node)
            rec_stack.add(node)
            path.append(node)

            for neighbor in graph.get(node, []):
                # Skip self-loops (usually stdlib shadow imports)
                if neighbor == node:
                    continue
                if neighbor not in visited:
                    dfs(neighbor)
                elif neighbor in rec_stack:
                    # Found a cycle - extract it
                    cycle_start = path.index(neighbor)
                    cycle = path[cycle_start:]
                    # Only report cycles with 2+ distinct modules
                    if len(cycle) >= 2:
                        # Normalize cycle to start with smallest element
                        min_idx = cycle.index(min(cycle))
                        normalized = cycle[min_idx:] + cycle[:min_idx]
                        # Avoid duplicates
                        if not any(c.cycle == normalized for c in cycles):
                            cycles.append(CircularDependency(cycle=normalized))

            path.pop()
            rec_stack.remove(node)

        for node in graph:
            if node not in visited:
                dfs(node)

        return cycles


def format_dependency_analysis(analysis: DependencyAnalysis) -> str:
    """Format analysis results as markdown."""
    lines = ["## Dependency Analysis", ""]

    # Summary
    lines.append(f"**Modules:** {analysis.total_modules}")
    lines.append(f"**Dependencies:** {analysis.total_edges}")
    lines.append(f"**Coupling density:** {analysis.coupling_density:.1%}")
    lines.append("")

    # Circular dependencies
    if analysis.circular_deps:
        lines.append("### Circular Dependencies")
        lines.append("")
        for cd in analysis.circular_deps:
            lines.append(f"- {cd.description}")
        lines.append("")

    # God modules
    god_modules = analysis.god_modules
    if god_modules:
        lines.append("### High Fan-In Modules")
        lines.append("")
        lines.append("| Module | Fan-In | Importers |")
        lines.append("|--------|--------|-----------|")
        for m in god_modules[:10]:  # Top 10
            importers = ", ".join(m.importers[:3])
            if len(m.importers) > 3:
                importers += f" (+{len(m.importers) - 3} more)"
            lines.append(f"| `{m.name}` | {m.fan_in} | {importers} |")
        lines.append("")

    # Orphan modules
    orphans = analysis.orphan_modules
    if orphans:
        lines.append("### Potentially Unused Modules")
        lines.append("")
        for name in orphans[:10]:  # Top 10
            lines.append(f"- `{name}`")
        if len(orphans) > 10:
            lines.append(f"- ... and {len(orphans) - 10} more")
        lines.append("")

    if not analysis.circular_deps and not god_modules and not orphans:
        lines.append("No dependency issues found.")

    return "\n".join(lines)
