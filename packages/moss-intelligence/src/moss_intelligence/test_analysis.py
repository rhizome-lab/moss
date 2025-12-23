"""Test coverage analysis for project health.

Analyzes test structure (not runtime coverage):
- Module-to-test mapping
- Test-to-code ratio
- Untested public APIs
- Test file organization
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class ModuleTestMapping:
    """Mapping between a source module and its tests."""

    module: str
    module_path: Path
    test_files: list[Path] = field(default_factory=list)
    test_count: int = 0  # Number of test functions/methods

    @property
    def has_tests(self) -> bool:
        return len(self.test_files) > 0 or self.test_count > 0


@dataclass
class UntestedExport:
    """A public export that appears untested."""

    module: str
    name: str
    kind: str  # function, class
    file: Path
    lineno: int


@dataclass
class TestAnalysis:
    """Results of test coverage analysis."""

    # Mappings
    module_mappings: list[ModuleTestMapping] = field(default_factory=list)
    untested_exports: list[UntestedExport] = field(default_factory=list)

    # Stats
    source_files: int = 0
    test_files: int = 0
    source_lines: int = 0
    test_lines: int = 0
    total_exports: int = 0
    tested_exports: int = 0

    @property
    def test_ratio(self) -> float:
        """Ratio of test lines to source lines."""
        if self.source_lines == 0:
            return 0.0
        return self.test_lines / self.source_lines

    @property
    def coverage_estimate(self) -> float:
        """Estimated coverage based on export testing."""
        if self.total_exports == 0:
            return 0.0
        return self.tested_exports / self.total_exports

    @property
    def modules_with_tests(self) -> int:
        return sum(1 for m in self.module_mappings if m.has_tests)

    @property
    def modules_without_tests(self) -> int:
        return sum(1 for m in self.module_mappings if not m.has_tests)

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        parts = [
            f"{self.source_files} src, {self.test_files} test files",
            f"ratio: {self.test_ratio:.1%}",
            f"coverage: ~{self.coverage_estimate:.0%}",
        ]
        if self.untested_exports:
            parts.append(f"{len(self.untested_exports)} untested exports")
        if self.modules_without_tests:
            parts.append(f"{self.modules_without_tests} modules without tests")
        return " | ".join(parts)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON output."""
        return {
            "stats": {
                "source_files": self.source_files,
                "test_files": self.test_files,
                "source_lines": self.source_lines,
                "test_lines": self.test_lines,
                "test_ratio": self.test_ratio,
                "modules_with_tests": self.modules_with_tests,
                "modules_without_tests": self.modules_without_tests,
                "total_exports": self.total_exports,
                "tested_exports": self.tested_exports,
                "coverage_estimate": self.coverage_estimate,
            },
            "untested_exports": [
                {
                    "module": e.module,
                    "name": e.name,
                    "kind": e.kind,
                    "file": str(e.file),
                    "line": e.lineno,
                }
                for e in self.untested_exports[:20]  # Limit output
            ],
            "modules_without_tests": [m.module for m in self.module_mappings if not m.has_tests][
                :20
            ],
        }


class TestAnalyzer:
    """Analyze test coverage structure."""

    def __init__(self, root: Path):
        self.root = root.resolve()

    def analyze(self) -> TestAnalysis:
        """Run test analysis."""
        result = TestAnalysis()

        # Find source directory
        src_dir = self._find_source_dir()
        if not src_dir:
            return result

        # Find test directory
        test_dirs = self._find_test_dirs()

        # Collect source modules
        source_modules = self._collect_source_modules(src_dir)
        result.source_files = len(source_modules)

        # Collect test files
        test_files: list[Path] = []
        for test_dir in test_dirs:
            test_files.extend(test_dir.rglob("test_*.py"))
            test_files.extend(test_dir.rglob("*_test.py"))

        result.test_files = len(test_files)

        # Count lines
        for path in source_modules.values():
            try:
                result.source_lines += len(path.read_text().splitlines())
            except (OSError, UnicodeDecodeError):
                pass

        for path in test_files:
            try:
                result.test_lines += len(path.read_text().splitlines())
            except (OSError, UnicodeDecodeError):
                pass

        # Map modules to tests
        result.module_mappings = self._map_modules_to_tests(source_modules, test_files)

        # Find all public exports
        all_exports = self._collect_exports(source_modules)
        result.total_exports = len(all_exports)

        # Find tested exports (mentioned in test files)
        tested_names = self._collect_tested_names(test_files)
        result.tested_exports = sum(1 for e in all_exports if e.name in tested_names)

        # Find untested exports
        for export in all_exports:
            if export.name not in tested_names:
                result.untested_exports.append(export)

        return result

    def _find_source_dir(self) -> Path | None:
        """Find the main source directory."""
        for candidate in [self.root / "src", self.root / "lib", self.root]:
            if candidate.exists():
                for subdir in candidate.iterdir():
                    if subdir.is_dir() and (subdir / "__init__.py").exists():
                        return subdir
                if list(candidate.glob("*.py")):
                    return candidate
        return None

    def _find_test_dirs(self) -> list[Path]:
        """Find test directories."""
        dirs = []
        for name in ["tests", "test"]:
            test_dir = self.root / name
            if test_dir.exists() and test_dir.is_dir():
                dirs.append(test_dir)
        return dirs

    def _collect_source_modules(self, src_dir: Path) -> dict[str, Path]:
        """Collect source modules (module name -> path)."""
        modules = {}
        for py_file in src_dir.rglob("*.py"):
            # Skip test files that might be in src
            if "test" in py_file.name.lower():
                continue
            # Get module name
            rel_path = py_file.relative_to(src_dir)
            module_name = str(rel_path.with_suffix("")).replace("/", ".").replace("\\", ".")
            modules[module_name] = py_file
        return modules

    def _map_modules_to_tests(
        self,
        source_modules: dict[str, Path],
        test_files: list[Path],
    ) -> list[ModuleTestMapping]:
        """Map source modules to their test files."""
        mappings = []

        for module_name, module_path in source_modules.items():
            mapping = ModuleTestMapping(
                module=module_name,
                module_path=module_path,
            )

            # Find matching test files
            # Convention: module "foo.bar" -> test file "test_bar.py" or "test_foo_bar.py"
            base_name = module_name.split(".")[-1]
            full_name = module_name.replace(".", "_")

            for test_file in test_files:
                test_name = test_file.stem
                if (
                    test_name == f"test_{base_name}"
                    or test_name == f"test_{full_name}"
                    or test_name == f"{base_name}_test"
                    or base_name in test_name
                ):
                    mapping.test_files.append(test_file)

            # Count test functions in matched files
            for test_file in mapping.test_files:
                mapping.test_count += self._count_tests(test_file)

            mappings.append(mapping)

        return mappings

    def _count_tests(self, test_file: Path) -> int:
        """Count test functions in a test file."""
        try:
            source = test_file.read_text()
            tree = ast.parse(source)
        except (OSError, SyntaxError, UnicodeDecodeError):
            return 0

        count = 0
        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                if node.name.startswith("test_"):
                    count += 1
        return count

    def _collect_exports(self, source_modules: dict[str, Path]) -> list[UntestedExport]:
        """Collect all public exports from source modules."""
        exports = []

        for module_name, path in source_modules.items():
            try:
                source = path.read_text()
                tree = ast.parse(source)
            except (OSError, SyntaxError, UnicodeDecodeError):
                continue

            for node in ast.iter_child_nodes(tree):
                # Public functions
                if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                    if not node.name.startswith("_"):
                        exports.append(
                            UntestedExport(
                                module=module_name,
                                name=node.name,
                                kind="function",
                                file=path,
                                lineno=node.lineno,
                            )
                        )

                # Public classes
                elif isinstance(node, ast.ClassDef):
                    if not node.name.startswith("_"):
                        exports.append(
                            UntestedExport(
                                module=module_name,
                                name=node.name,
                                kind="class",
                                file=path,
                                lineno=node.lineno,
                            )
                        )

        return exports

    def _collect_tested_names(self, test_files: list[Path]) -> set[str]:
        """Collect names that appear to be tested."""
        tested = set()

        for test_file in test_files:
            try:
                source = test_file.read_text()
            except (OSError, UnicodeDecodeError):
                continue

            # Look for imports from the main package
            # e.g., "from moss.foo import Bar" -> Bar is tested
            import_pattern = re.compile(r"from\s+\S+\s+import\s+([^#\n]+)")
            for match in import_pattern.finditer(source):
                names = match.group(1)
                for name in names.split(","):
                    name = name.strip()
                    if " as " in name:
                        name = name.split(" as ")[0].strip()
                    if name and not name.startswith("_"):
                        tested.add(name)

            # Look for direct references (function calls, class instantiations)
            # Simple heuristic: CamelCase words and function_name patterns
            word_pattern = re.compile(r"\b([A-Z][a-zA-Z0-9]*|[a-z_][a-z0-9_]*)\b")
            for match in word_pattern.finditer(source):
                word = match.group(1)
                if len(word) > 2 and not word.startswith("test"):
                    tested.add(word)

        return tested


def format_test_analysis(analysis: TestAnalysis) -> str:
    """Format analysis results as markdown."""
    lines = ["## Test Analysis", ""]

    # Summary
    lines.append(f"**Source files:** {analysis.source_files}")
    lines.append(f"**Test files:** {analysis.test_files}")
    lines.append(f"**Test ratio:** {analysis.test_ratio:.2f} (test lines / source lines)")
    lines.append(f"**Modules with tests:** {analysis.modules_with_tests}")
    lines.append(f"**Modules without tests:** {analysis.modules_without_tests}")
    lines.append(f"**Coverage estimate:** {analysis.coverage_estimate:.0%} of exports tested")
    lines.append("")

    # Modules without tests
    modules_without = [m for m in analysis.module_mappings if not m.has_tests]
    if modules_without:
        lines.append("### Modules Without Tests")
        lines.append("")
        for m in modules_without[:15]:
            lines.append(f"- `{m.module}`")
        if len(modules_without) > 15:
            lines.append(f"- ... and {len(modules_without) - 15} more")
        lines.append("")

    # Untested exports
    if analysis.untested_exports:
        lines.append("### Untested Exports")
        lines.append("")
        for e in analysis.untested_exports[:15]:
            lines.append(f"- `{e.module}.{e.name}` ({e.kind})")
        if len(analysis.untested_exports) > 15:
            lines.append(f"- ... and {len(analysis.untested_exports) - 15} more")
        lines.append("")

    return "\n".join(lines)
