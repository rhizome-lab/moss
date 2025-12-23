"""Architectural pattern detection in Python codebases.

Detects common patterns:
- Plugin systems (Protocol + Registry + implementations)
- Factory patterns (functions returning different types)
- Strategy patterns (interface + swappable implementations)
- Singleton patterns
- Coupling analysis (module dependencies)

Usage:
    from moss.patterns import PatternAnalyzer

    analyzer = PatternAnalyzer(project_root)
    results = analyzer.analyze()

    # Via CLI:
    # moss patterns [directory] [--pattern plugin,factory]
"""

from __future__ import annotations

import ast
import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


# =============================================================================
# Helpers
# =============================================================================


def get_base_name(node: ast.expr) -> str:
    """Get the name from a base class expression.

    Handles Name, Attribute, and Subscript nodes recursively.
    """
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return node.attr
    if isinstance(node, ast.Subscript):
        return get_base_name(node.value)
    return ""


# =============================================================================
# Data Types
# =============================================================================


@dataclass
class PatternInstance:
    """A detected pattern instance in the codebase."""

    pattern_type: str  # plugin, factory, strategy, singleton, etc.
    name: str  # Pattern name or primary class/function
    file_path: str
    line_start: int
    line_end: int | None = None
    confidence: float = 1.0  # 0.0-1.0
    components: list[str] = field(default_factory=list)  # Related classes/functions
    description: str = ""
    suggestion: str | None = None  # Improvement suggestion


@dataclass
class CouplingInfo:
    """Coupling information for a module."""

    module: str
    imports_from: list[str] = field(default_factory=list)  # Modules this one imports
    imported_by: list[str] = field(default_factory=list)  # Modules that import this


@dataclass
class PatternAnalysis:
    """Results from pattern analysis."""

    root: Path
    patterns: list[PatternInstance] = field(default_factory=list)
    coupling: dict[str, CouplingInfo] = field(default_factory=dict)
    suggestions: list[str] = field(default_factory=list)

    @property
    def plugin_systems(self) -> list[PatternInstance]:
        return [p for p in self.patterns if p.pattern_type == "plugin"]

    @property
    def factories(self) -> list[PatternInstance]:
        return [p for p in self.patterns if p.pattern_type == "factory"]

    @property
    def strategies(self) -> list[PatternInstance]:
        return [p for p in self.patterns if p.pattern_type == "strategy"]

    def to_compact(self) -> str:
        """Token-efficient compact format for AI agents."""
        summary = (
            f"patterns: {len(self.patterns)} "
            f"(plugins={len(self.plugin_systems)}, "
            f"factories={len(self.factories)}, "
            f"strategies={len(self.strategies)})"
        )
        lines = [summary]
        if self.plugin_systems:
            names = [p.name for p in self.plugin_systems[:10]]
            lines.append(f"plugins: {', '.join(names)}")
        if self.factories:
            names = [p.name for p in self.factories[:10]]
            lines.append(f"factories: {', '.join(names)}")
        if self.strategies:
            names = [p.name for p in self.strategies[:10]]
            lines.append(f"strategies: {', '.join(names)}")
        if self.suggestions:
            lines.append(f"suggestions: {len(self.suggestions)}")
        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "root": str(self.root),
            "summary": {
                "total_patterns": len(self.patterns),
                "plugin_systems": len(self.plugin_systems),
                "factories": len(self.factories),
                "strategies": len(self.strategies),
            },
            "patterns": [
                {
                    "type": p.pattern_type,
                    "name": p.name,
                    "file": p.file_path,
                    "line": p.line_start,
                    "confidence": p.confidence,
                    "components": p.components,
                    "description": p.description,
                    "suggestion": p.suggestion,
                }
                for p in self.patterns
            ],
            "suggestions": self.suggestions,
        }


# =============================================================================
# Pattern Detectors
# =============================================================================


class ProtocolDetector(ast.NodeVisitor):
    """Detect Protocol definitions and their implementations."""

    def __init__(self, source: str, file_path: str) -> None:
        self.source = source
        self.file_path = file_path
        self.protocols: list[dict[str, Any]] = []
        self.protocol_impls: list[dict[str, Any]] = []
        self.registries: list[dict[str, Any]] = []

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        # Check if this is a Protocol definition
        for base in node.bases:
            base_name = get_base_name(base)
            if base_name == "Protocol":
                self.protocols.append(
                    {
                        "name": node.name,
                        "line": node.lineno,
                        "end_line": node.end_lineno,
                        "methods": self._get_methods(node),
                    }
                )
            elif base_name in [p["name"] for p in self.protocols]:
                # This class implements a Protocol we found
                self.protocol_impls.append(
                    {
                        "name": node.name,
                        "protocol": base_name,
                        "line": node.lineno,
                    }
                )

        # Check for registry patterns (dict with type as key)
        for stmt in node.body:
            if isinstance(stmt, ast.AnnAssign) and stmt.annotation:
                annotation = ast.unparse(stmt.annotation)
                if "dict" in annotation.lower() and (
                    "type" in annotation.lower() or "str" in annotation.lower()
                ):
                    if stmt.target and isinstance(stmt.target, ast.Name):
                        name = stmt.target.id
                        registry_words = ["registry", "plugins", "handlers"]
                        if any(word in name.lower() for word in registry_words):
                            self.registries.append(
                                {
                                    "name": name,
                                    "class": node.name,
                                    "line": stmt.lineno,
                                }
                            )

        self.generic_visit(node)

    def _get_methods(self, node: ast.ClassDef) -> list[str]:
        """Get method names from a class."""
        methods = []
        for item in node.body:
            if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                if not item.name.startswith("_") or item.name.startswith("__"):
                    methods.append(item.name)
        return methods


class FactoryDetector(ast.NodeVisitor):
    """Detect factory patterns - functions that create different types."""

    def __init__(self, source: str, file_path: str) -> None:
        self.source = source
        self.file_path = file_path
        self.factories: list[dict[str, Any]] = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self._check_factory(node)
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self._check_factory(node)
        self.generic_visit(node)

    def _check_factory(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> None:
        """Check if a function is a factory."""
        # Look for factory naming patterns
        name_hints = ["create", "make", "build", "get", "factory", "new"]
        has_factory_name = any(hint in node.name.lower() for hint in name_hints)

        # Look for conditional returns of different types
        return_types = self._find_return_types(node)

        if has_factory_name and len(return_types) > 1:
            self.factories.append(
                {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": node.end_lineno,
                    "return_types": return_types,
                }
            )
        elif len(return_types) >= 3:  # Multiple return types even without factory name
            self.factories.append(
                {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": node.end_lineno,
                    "return_types": return_types,
                }
            )

    def _find_return_types(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> list[str]:
        """Find all return statement types in a function."""
        types = set()

        class ReturnVisitor(ast.NodeVisitor):
            def visit_Return(self, ret_node: ast.Return) -> None:
                if ret_node.value:
                    if isinstance(ret_node.value, ast.Call):
                        if isinstance(ret_node.value.func, ast.Name):
                            types.add(ret_node.value.func.id)
                        elif isinstance(ret_node.value.func, ast.Attribute):
                            types.add(ret_node.value.func.attr)

        ReturnVisitor().visit(node)
        return list(types)


class SingletonDetector(ast.NodeVisitor):
    """Detect singleton patterns."""

    def __init__(self, source: str, file_path: str) -> None:
        self.source = source
        self.file_path = file_path
        self.singletons: list[dict[str, Any]] = []

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        # Look for classic singleton patterns
        has_instance_attr = False
        has_new_override = False

        for stmt in node.body:
            # Class-level _instance attribute
            if isinstance(stmt, ast.AnnAssign) and stmt.target:
                if isinstance(stmt.target, ast.Name) and "_instance" in stmt.target.id:
                    has_instance_attr = True
            elif isinstance(stmt, ast.Assign):
                for target in stmt.targets:
                    if isinstance(target, ast.Name) and "_instance" in target.id:
                        has_instance_attr = True

            # __new__ override
            if isinstance(stmt, ast.FunctionDef) and stmt.name == "__new__":
                has_new_override = True

        if has_instance_attr and has_new_override:
            self.singletons.append(
                {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": node.end_lineno,
                }
            )

        self.generic_visit(node)


class StrategyDetector(ast.NodeVisitor):
    """Detect strategy patterns - interface with multiple swappable implementations.

    Strategy pattern indicators:
    - A Protocol/ABC with 2+ implementations
    - Classes that hold a reference to the interface (composition)
    - Methods that swap/set the strategy at runtime
    """

    def __init__(self, source: str, file_path: str) -> None:
        self.source = source
        self.file_path = file_path
        self.interfaces: list[dict[str, Any]] = []  # Protocols/ABCs
        self.implementations: dict[str, list[dict[str, Any]]] = {}  # interface -> impls
        self.strategy_holders: list[dict[str, Any]] = []  # Classes holding strategies

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        # Check if this is an interface (Protocol or ABC)
        is_interface = False
        interface_type = None

        for base in node.bases:
            base_name = get_base_name(base)
            if base_name == "Protocol":
                is_interface = True
                interface_type = "Protocol"
            elif base_name == "ABC":
                is_interface = True
                interface_type = "ABC"

        if is_interface:
            methods = self._get_abstract_methods(node)
            self.interfaces.append(
                {
                    "name": node.name,
                    "type": interface_type,
                    "line": node.lineno,
                    "end_line": node.end_lineno,
                    "methods": methods,
                }
            )
        else:
            # Check if it implements any known interface
            for base in node.bases:
                base_name = get_base_name(base)
                if base_name not in self.implementations:
                    self.implementations[base_name] = []
                self.implementations[base_name].append(
                    {
                        "name": node.name,
                        "line": node.lineno,
                        "interface": base_name,
                    }
                )

            # Check if this class holds a strategy reference
            strategy_fields = self._find_strategy_fields(node)
            if strategy_fields:
                self.strategy_holders.append(
                    {
                        "name": node.name,
                        "line": node.lineno,
                        "end_line": node.end_lineno,
                        "strategy_fields": strategy_fields,
                    }
                )

        self.generic_visit(node)

    def _get_abstract_methods(self, node: ast.ClassDef) -> list[str]:
        """Get abstract method names from a class."""
        methods = []
        for item in node.body:
            if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                # Check for @abstractmethod decorator
                is_abstract = any(
                    self._get_decorator_name(d) == "abstractmethod" for d in item.decorator_list
                )
                # Or just public methods in Protocol
                if is_abstract or not item.name.startswith("_"):
                    methods.append(item.name)
        return methods

    def _get_decorator_name(self, node: ast.expr) -> str:
        """Get decorator name."""
        if isinstance(node, ast.Name):
            return node.id
        if isinstance(node, ast.Attribute):
            return node.attr
        return ""

    def _find_strategy_fields(self, node: ast.ClassDef) -> list[str]:
        """Find fields that likely hold strategy references."""
        strategy_fields = []
        strategy_hints = ["strategy", "handler", "processor", "provider", "policy", "algorithm"]

        for stmt in node.body:
            # Check annotated assignments
            if isinstance(stmt, ast.AnnAssign) and stmt.target:
                if isinstance(stmt.target, ast.Name):
                    name = stmt.target.id.lower()
                    if any(hint in name for hint in strategy_hints):
                        strategy_fields.append(stmt.target.id)

            # Check __init__ assignments
            if isinstance(stmt, ast.FunctionDef) and stmt.name == "__init__":
                for init_stmt in ast.walk(stmt):
                    if isinstance(init_stmt, ast.Assign):
                        for target in init_stmt.targets:
                            if isinstance(target, ast.Attribute) and isinstance(
                                target.value, ast.Name
                            ):
                                if target.value.id == "self":
                                    attr_name = target.attr.lower()
                                    if any(hint in attr_name for hint in strategy_hints):
                                        strategy_fields.append(target.attr)

        return strategy_fields


class CouplingAnalyzer(ast.NodeVisitor):
    """Analyze module coupling via imports."""

    def __init__(self, source: str, module_name: str) -> None:
        self.source = source
        self.module_name = module_name
        self.imports: list[str] = []

    def visit_Import(self, node: ast.Import) -> None:
        for alias in node.names:
            self.imports.append(alias.name)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        if node.module:
            self.imports.append(node.module)


# =============================================================================
# Main Analyzer
# =============================================================================


class PatternAnalyzer:
    """Analyzes a codebase for architectural patterns."""

    def __init__(
        self,
        root: Path,
        patterns: list[str] | None = None,
    ) -> None:
        """Initialize the analyzer.

        Args:
            root: Project root directory
            patterns: List of patterns to detect (None = all)
        """
        self.root = Path(root).resolve()
        self.requested_patterns = patterns or [
            "plugin",
            "factory",
            "singleton",
            "strategy",
            "coupling",
        ]

    def analyze(self) -> PatternAnalysis:
        """Run pattern analysis on the codebase."""
        result = PatternAnalysis(root=self.root)

        # Find all Python files
        python_files = list(self.root.rglob("*.py"))
        exclude_parts = [".venv", "venv", "node_modules", ".git", "__pycache__", "dist", "build"]
        python_files = [
            f for f in python_files if not any(part in str(f) for part in exclude_parts)
        ]

        # First pass: collect all protocols and interfaces for cross-file analysis
        all_protocols: list[dict[str, Any]] = []
        all_interfaces: dict[str, dict[str, Any]] = {}  # name -> interface info
        all_implementations: dict[str, list[dict[str, Any]]] = {}  # interface -> impls

        for file_path in python_files:
            try:
                source = file_path.read_text()
                tree = ast.parse(source)
                rel_path = str(file_path.relative_to(self.root))

                if "plugin" in self.requested_patterns:
                    detector = ProtocolDetector(source, str(file_path))
                    detector.visit(tree)
                    all_protocols.extend(detector.protocols)

                if "strategy" in self.requested_patterns:
                    detector = StrategyDetector(source, rel_path)
                    detector.visit(tree)

                    # Collect interfaces
                    for iface in detector.interfaces:
                        all_interfaces[iface["name"]] = {**iface, "file": rel_path}

                    # Collect implementations
                    for iface_name, impls in detector.implementations.items():
                        if iface_name not in all_implementations:
                            all_implementations[iface_name] = []
                        for impl in impls:
                            all_implementations[iface_name].append({**impl, "file": rel_path})

            except (OSError, UnicodeDecodeError, SyntaxError) as e:
                logger.debug("Failed to parse %s: %s", file_path, e)

        # Second pass: analyze each file
        coupling_data: dict[str, list[str]] = {}

        for file_path in python_files:
            try:
                source = file_path.read_text()
                tree = ast.parse(source)
                rel_path = str(file_path.relative_to(self.root))

                # Plugin/Protocol detection
                if "plugin" in self.requested_patterns:
                    detector = ProtocolDetector(source, rel_path)
                    detector.visit(tree)

                    for protocol in detector.protocols:
                        result.patterns.append(
                            PatternInstance(
                                pattern_type="plugin",
                                name=protocol["name"],
                                file_path=rel_path,
                                line_start=protocol["line"],
                                line_end=protocol.get("end_line"),
                                components=protocol.get("methods", []),
                                description=f"{len(protocol.get('methods', []))} methods",
                            )
                        )

                    for registry in detector.registries:
                        result.patterns.append(
                            PatternInstance(
                                pattern_type="plugin",
                                name=f"{registry['class']}.{registry['name']}",
                                file_path=rel_path,
                                line_start=registry["line"],
                                description="Plugin registry",
                            )
                        )

                # Factory detection
                if "factory" in self.requested_patterns:
                    detector = FactoryDetector(source, rel_path)
                    detector.visit(tree)

                    for factory in detector.factories:
                        result.patterns.append(
                            PatternInstance(
                                pattern_type="factory",
                                name=factory["name"],
                                file_path=rel_path,
                                line_start=factory["line"],
                                line_end=factory.get("end_line"),
                                components=factory.get("return_types", []),
                                description=f"Creates {len(factory.get('return_types', []))} types",
                            )
                        )

                # Singleton detection
                if "singleton" in self.requested_patterns:
                    detector = SingletonDetector(source, rel_path)
                    detector.visit(tree)

                    for singleton in detector.singletons:
                        result.patterns.append(
                            PatternInstance(
                                pattern_type="singleton",
                                name=singleton["name"],
                                file_path=rel_path,
                                line_start=singleton["line"],
                                line_end=singleton.get("end_line"),
                                description="Singleton pattern with _instance + __new__",
                            )
                        )

                # Strategy holder detection (classes that use strategies)
                if "strategy" in self.requested_patterns:
                    detector = StrategyDetector(source, rel_path)
                    detector.visit(tree)

                    for holder in detector.strategy_holders:
                        result.patterns.append(
                            PatternInstance(
                                pattern_type="strategy",
                                name=holder["name"],
                                file_path=rel_path,
                                line_start=holder["line"],
                                line_end=holder.get("end_line"),
                                components=holder.get("strategy_fields", []),
                                description=f"Strategy holder with fields: "
                                f"{', '.join(holder.get('strategy_fields', []))}",
                            )
                        )

                # Coupling analysis
                if "coupling" in self.requested_patterns:
                    module_name = rel_path.replace("/", ".").replace(".py", "")
                    analyzer = CouplingAnalyzer(source, module_name)
                    analyzer.visit(tree)
                    coupling_data[module_name] = analyzer.imports

            except (OSError, UnicodeDecodeError, SyntaxError) as e:
                logger.debug("Failed to analyze %s: %s", file_path, e)

        # Build coupling graph
        if "coupling" in self.requested_patterns:
            for module, imports in coupling_data.items():
                result.coupling[module] = CouplingInfo(
                    module=module,
                    imports_from=imports,
                )

            # Calculate imported_by (reverse edges)
            for module, info in result.coupling.items():
                for imported in info.imports_from:
                    if imported in result.coupling:
                        result.coupling[imported].imported_by.append(module)

            # Generate suggestions for highly coupled modules
            for module, info in result.coupling.items():
                if len(info.imported_by) > 10:
                    result.suggestions.append(
                        f"{module} is imported by {len(info.imported_by)} modules - "
                        "consider if it's doing too much"
                    )
                if len(info.imports_from) > 15:
                    result.suggestions.append(
                        f"{module} imports {len(info.imports_from)} modules - "
                        "may have too many dependencies"
                    )

        # Strategy pattern detection: interface + 2+ implementations
        if "strategy" in self.requested_patterns:
            for iface_name, iface_info in all_interfaces.items():
                impls = all_implementations.get(iface_name, [])
                if len(impls) >= 2:
                    # This is a strategy pattern: interface with multiple implementations
                    impl_names = [impl["name"] for impl in impls]
                    result.patterns.append(
                        PatternInstance(
                            pattern_type="strategy",
                            name=iface_name,
                            file_path=iface_info["file"],
                            line_start=iface_info["line"],
                            line_end=iface_info.get("end_line"),
                            components=impl_names,
                            description=f"{iface_info.get('type', 'Interface')} with "
                            f"{len(impls)} implementations: {', '.join(impl_names[:5])}"
                            + ("..." if len(impl_names) > 5 else ""),
                            confidence=0.9,  # High confidence when we find explicit pattern
                        )
                    )

        return result


def format_pattern_analysis(analysis: PatternAnalysis) -> str:
    """Format pattern analysis as markdown."""
    lines = ["## Pattern Analysis", ""]

    # Summary
    lines.append("### Summary")
    lines.append(f"- Plugin systems: {len(analysis.plugin_systems)}")
    lines.append(f"- Factories: {len(analysis.factories)}")
    lines.append(f"- Strategies: {len(analysis.strategies)}")
    lines.append(f"- Total patterns: {len(analysis.patterns)}")
    lines.append("")

    # Patterns by type
    if analysis.plugin_systems:
        lines.append("### Plugin Systems")
        for p in analysis.plugin_systems:
            lines.append(f"- **{p.name}** (`{p.file_path}:{p.line_start}`)")
            if p.description:
                lines.append(f"  {p.description}")
            if p.components:
                lines.append(f"  Components: {', '.join(p.components[:5])}")
        lines.append("")

    if analysis.factories:
        lines.append("### Factories")
        for p in analysis.factories:
            lines.append(f"- **{p.name}** (`{p.file_path}:{p.line_start}`)")
            if p.description:
                lines.append(f"  {p.description}")
            if p.components:
                lines.append(f"  Returns: {', '.join(p.components)}")
        lines.append("")

    if analysis.strategies:
        lines.append("### Strategy Patterns")
        for p in analysis.strategies:
            lines.append(f"- **{p.name}** (`{p.file_path}:{p.line_start}`)")
            if p.description:
                lines.append(f"  {p.description}")
            if p.components:
                lines.append(f"  Implementations: {', '.join(p.components[:5])}")
                if len(p.components) > 5:
                    lines.append(f"  ... and {len(p.components) - 5} more")
        lines.append("")

    # Suggestions
    if analysis.suggestions:
        lines.append("### Suggestions")
        for s in analysis.suggestions:
            lines.append(f"- {s}")
        lines.append("")

    return "\n".join(lines)


def analyze_patterns(
    root: Path | str,
    patterns: list[str] | None = None,
) -> PatternAnalysis:
    """Convenience function to analyze patterns.

    Args:
        root: Project root directory
        patterns: Patterns to detect (None = all)

    Returns:
        PatternAnalysis with detected patterns
    """
    analyzer = PatternAnalyzer(Path(root), patterns=patterns)
    return analyzer.analyze()
