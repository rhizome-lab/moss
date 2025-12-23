"""Dependency analysis backend for architectural rules.

ArchUnit-style backend that enables cross-file dependency constraints:
- Layering rules: "UI must not import Infrastructure"
- Module boundaries: "Only X may import internal module Y"
- Circular dependency detection

Usage:
    @rule(backend="deps")
    def no_ui_to_infra(ctx: RuleContext) -> list[Violation]:
        deps_result = ctx.backend("deps")
        imports = deps_result.metadata.get("imports", [])
        # Check architectural constraints
        ...

The backend provides rich metadata about a file's dependencies:
- imports: List of Import objects (module, names, lineno, etc.)
- exports: List of exported symbols
- import_modules: Set of imported module names (for quick checks)
- layers: Optional layer classification if configured

For cross-file analysis, rules can request the full dependency graph:
    @rule(backend="deps", deps_graph=True)
    def no_circular_deps(ctx: RuleContext) -> list[Violation]:
        graph = ctx.backend("deps").metadata.get("graph", {})
        # Analyze full project graph
        ...
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

from moss_intelligence.dependencies import (
    DependencyInfo,
    build_dependency_graph,
    extract_dependencies,
)

from ..base import BackendResult, BaseBackend, Location, Match
from . import register_backend

if TYPE_CHECKING:
    pass


@register_backend
class DepsBackend(BaseBackend):
    """Dependency analysis backend for architectural constraints.

    Provides per-file dependency information and optional project-wide
    dependency graph for cross-file analysis.
    """

    # Cache for dependency graph (built once per analysis run)
    _graph_cache: dict[str, dict[str, list[str]]] | None = None
    _graph_cache_path: str | None = None

    @property
    def name(self) -> str:
        return "deps"

    def analyze(
        self,
        file_path: Path,
        pattern: str | None = None,
        **options: Any,
    ) -> BackendResult:
        """Analyze a file's dependencies.

        Args:
            file_path: File to analyze
            pattern: Optional query pattern:
                - "imports:module" - Check if file imports module
                - "layer:name" - Check if file is in layer (requires layer_map)
                - None - Return all dependency info
            **options:
                - project_root: str - Root for building dependency graph
                - layer_map: dict[str, list[str]] - Module patterns per layer
                - include_graph: bool - Include full project graph in metadata

        Returns:
            BackendResult with dependency metadata and optional matches
        """
        matches: list[Match] = []
        errors: list[str] = []
        metadata: dict[str, Any] = {}

        try:
            source = file_path.read_text()
        except (OSError, UnicodeDecodeError) as e:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"Could not read file: {e}"],
            )

        # Extract per-file dependencies
        try:
            dep_info = extract_dependencies(source)
        except SyntaxError as e:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"Parse error: {e}"],
            )

        # Build metadata from dependency info
        metadata["imports"] = [
            {
                "module": imp.module,
                "names": imp.names,
                "alias": imp.alias,
                "lineno": imp.lineno,
                "is_relative": imp.is_relative,
                "level": imp.level,
            }
            for imp in dep_info.imports
        ]
        metadata["exports"] = [
            {
                "name": exp.name,
                "kind": exp.kind,
                "lineno": exp.lineno,
            }
            for exp in dep_info.exports
        ]
        metadata["all_exports"] = dep_info.all_exports

        # Convenience: set of imported module names
        metadata["import_modules"] = {imp.module for imp in dep_info.imports}

        # Handle pattern queries
        if pattern:
            matches, pattern_errors = self._handle_pattern(pattern, file_path, dep_info, options)
            errors.extend(pattern_errors)

        # Optionally include full project graph
        if options.get("include_graph"):
            project_root = options.get("project_root", str(file_path.parent))
            graph = self._get_or_build_graph(project_root)
            metadata["graph"] = graph

        # Layer classification if configured
        layer_map = options.get("layer_map")
        if layer_map:
            file_layer = self._classify_layer(file_path, layer_map)
            metadata["layer"] = file_layer
            # Also classify imported modules by layer
            import_layers: dict[str, str] = {}
            for imp in dep_info.imports:
                imp_layer = self._module_to_layer(imp.module, layer_map)
                if imp_layer:
                    import_layers[imp.module] = imp_layer
            metadata["import_layers"] = import_layers

        return BackendResult(
            backend_name=self.name,
            matches=matches,
            metadata=metadata,
            errors=errors,
        )

    def _handle_pattern(
        self,
        pattern: str,
        file_path: Path,
        dep_info: DependencyInfo,
        options: dict[str, Any],
    ) -> tuple[list[Match], list[str]]:
        """Handle pattern-based queries.

        Patterns:
            imports:module - Match if file imports module (or submodule)
            imports_from:module - Match if file has 'from module import ...'
            layer:name - Match if file is in the specified layer
        """
        matches: list[Match] = []
        errors: list[str] = []

        if pattern.startswith("imports:"):
            target_module = pattern[8:]  # Remove "imports:" prefix
            for imp in dep_info.imports:
                if self._module_matches(imp.module, target_module):
                    matches.append(
                        Match(
                            location=Location(
                                file_path=file_path,
                                line=imp.lineno,
                                column=1,
                            ),
                            text=f"import {imp.module}",
                            metadata={"import": imp.module, "target": target_module},
                        )
                    )

        elif pattern.startswith("imports_from:"):
            target_module = pattern[13:]  # Remove "imports_from:" prefix
            for imp in dep_info.imports:
                if imp.names and self._module_matches(imp.module, target_module):
                    matches.append(
                        Match(
                            location=Location(
                                file_path=file_path,
                                line=imp.lineno,
                                column=1,
                            ),
                            text=f"from {imp.module} import {', '.join(imp.names)}",
                            metadata={
                                "import": imp.module,
                                "names": imp.names,
                                "target": target_module,
                            },
                        )
                    )

        elif pattern.startswith("layer:"):
            target_layer = pattern[6:]  # Remove "layer:" prefix
            layer_map = options.get("layer_map", {})
            file_layer = self._classify_layer(file_path, layer_map)
            if file_layer == target_layer:
                matches.append(
                    Match(
                        location=Location(file_path=file_path, line=1, column=1),
                        text=str(file_path),
                        metadata={"layer": file_layer},
                    )
                )

        else:
            errors.append(
                f"Unknown pattern format: {pattern}. "
                "Use 'imports:module', 'imports_from:module', or 'layer:name'"
            )

        return matches, errors

    def _module_matches(self, module: str, target: str) -> bool:
        """Check if module matches target (exact or prefix match)."""
        return module == target or module.startswith(target + ".")

    def _classify_layer(self, file_path: Path, layer_map: dict[str, list[str]]) -> str | None:
        """Classify a file into an architectural layer.

        layer_map example:
            {
                "ui": ["src/ui/**", "src/views/**"],
                "domain": ["src/domain/**", "src/models/**"],
                "infrastructure": ["src/db/**", "src/api/**"],
            }
        """
        import fnmatch

        file_str = str(file_path)
        for layer_name, patterns in layer_map.items():
            for pattern in patterns:
                if fnmatch.fnmatch(file_str, pattern):
                    return layer_name
        return None

    def _module_to_layer(self, module: str, layer_map: dict[str, list[str]]) -> str | None:
        """Map a module name to a layer.

        This is a heuristic - converts module name to likely file paths
        and checks against layer patterns.
        """
        import fnmatch

        # Convert module to path-like string
        module_path = module.replace(".", "/")

        for layer_name, patterns in layer_map.items():
            for pattern in patterns:
                # Check if module path matches pattern
                if fnmatch.fnmatch(module_path, pattern.rstrip("*").rstrip("/")):
                    return layer_name
                if fnmatch.fnmatch(f"{module_path}.py", pattern):
                    return layer_name
        return None

    def _get_or_build_graph(self, project_root: str) -> dict[str, list[str]]:
        """Get cached graph or build new one."""
        if self._graph_cache is not None and self._graph_cache_path == project_root:
            return self._graph_cache

        graph = build_dependency_graph(project_root, internal_only=True)
        DepsBackend._graph_cache = graph
        DepsBackend._graph_cache_path = project_root
        return graph

    @classmethod
    def clear_cache(cls) -> None:
        """Clear the dependency graph cache."""
        cls._graph_cache = None
        cls._graph_cache_path = None

    def supports_pattern(self, pattern: str) -> bool:
        """Check if pattern is valid."""
        valid_prefixes = ("imports:", "imports_from:", "layer:")
        return any(pattern.startswith(p) for p in valid_prefixes)


# =============================================================================
# Helper functions for rules
# =============================================================================


def check_layer_violation(
    ctx_metadata: dict[str, Any],
    forbidden_layers: list[str],
) -> list[tuple[str, int]]:
    """Check if any imports violate layer constraints.

    Args:
        ctx_metadata: Metadata from deps backend result
        forbidden_layers: Layers that should not be imported

    Returns:
        List of (module, lineno) tuples for violations
    """
    violations = []
    import_layers = ctx_metadata.get("import_layers", {})
    imports = ctx_metadata.get("imports", [])

    for imp in imports:
        module = imp["module"]
        layer = import_layers.get(module)
        if layer in forbidden_layers:
            violations.append((module, imp["lineno"]))

    return violations


def find_circular_dependencies(graph: dict[str, list[str]]) -> list[list[str]]:
    """Find circular dependencies in a dependency graph.

    Args:
        graph: Module dependency graph (module -> [imports])

    Returns:
        List of cycles (each cycle is a list of modules)
    """
    cycles: list[list[str]] = []
    visited: set[str] = set()
    rec_stack: set[str] = set()

    def dfs(node: str, path: list[str]) -> None:
        visited.add(node)
        rec_stack.add(node)
        path.append(node)

        for neighbor in graph.get(node, []):
            if neighbor not in visited:
                dfs(neighbor, path.copy())
            elif neighbor in rec_stack:
                # Found cycle
                cycle_start = path.index(neighbor)
                cycle = [*path[cycle_start:], neighbor]
                # Normalize cycle (start from smallest element)
                min_idx = cycle.index(min(cycle[:-1]))
                normalized = [*cycle[min_idx:-1], *cycle[:min_idx], cycle[min_idx]]
                if normalized not in cycles:
                    cycles.append(normalized)

        rec_stack.remove(node)

    for node in graph:
        if node not in visited:
            dfs(node, [])

    return cycles
