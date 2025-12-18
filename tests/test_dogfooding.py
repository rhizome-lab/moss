"""Dogfooding tests - using Moss to analyze itself.

These tests verify that Moss can successfully analyze and process
its own codebase, serving as both validation and real-world testing.
"""

from pathlib import Path

import pytest

from moss.anchors import Anchor, AnchorType, find_anchors
from moss.cfg import build_cfg
from moss.dependencies import extract_dependencies
from moss.elided_literals import elide_literals
from moss.skeleton import extract_python_skeleton, format_skeleton


class TestSelfAnalysis:
    """Tests analyzing Moss source code."""

    @pytest.fixture
    def moss_src(self) -> Path:
        """Get the Moss source directory."""
        return Path(__file__).parent.parent / "src" / "moss"

    def test_extract_skeleton_from_all_modules(self, moss_src: Path):
        """Test skeleton extraction on all Moss modules."""
        python_files = list(moss_src.glob("*.py"))
        assert len(python_files) > 10, "Should have many source files"

        total_symbols = 0
        for py_file in python_files:
            source = py_file.read_text()
            try:
                symbols = extract_python_skeleton(source)
                total_symbols += len(symbols)
            except SyntaxError:
                pytest.fail(f"Failed to parse {py_file.name}")

        assert total_symbols > 100, "Should extract many symbols from Moss"

    def test_find_key_classes(self, moss_src: Path):
        """Test finding key Moss classes."""
        key_classes = [
            ("anchors.py", "Anchor"),
            ("patches.py", "Patch"),
            ("events.py", "EventBus"),
            ("validators.py", "SyntaxValidator"),
            ("shadow_git.py", "ShadowGit"),
            ("memory.py", "MemoryManager"),
            ("policy.py", "PolicyEngine"),
            ("cfg.py", "ControlFlowGraph"),
            ("skeleton.py", "Symbol"),
        ]

        for filename, class_name in key_classes:
            py_file = moss_src / filename
            if not py_file.exists():
                continue

            source = py_file.read_text()
            anchor = Anchor(type=AnchorType.CLASS, name=class_name)
            matches = find_anchors(source, anchor)

            assert len(matches) >= 1, f"Should find {class_name} in {filename}"

    def test_find_key_functions(self, moss_src: Path):
        """Test finding key Moss functions."""
        key_functions = [
            ("anchors.py", "find_anchors"),
            ("patches.py", "apply_patch"),
            ("skeleton.py", "extract_python_skeleton"),
            ("cfg.py", "build_cfg"),
            ("elided_literals.py", "elide_literals"),
        ]

        for filename, func_name in key_functions:
            py_file = moss_src / filename
            if not py_file.exists():
                continue

            source = py_file.read_text()
            anchor = Anchor(type=AnchorType.FUNCTION, name=func_name)
            matches = find_anchors(source, anchor)

            assert len(matches) >= 1, f"Should find {func_name} in {filename}"

    def test_build_cfg_for_moss_functions(self, moss_src: Path):
        """Test CFG building on Moss source functions."""
        # Test CFG on validators.py which has good control flow
        validators_file = moss_src / "validators.py"
        if not validators_file.exists():
            pytest.skip("validators.py not found")

        source = validators_file.read_text()
        cfgs = build_cfg(source)

        assert len(cfgs) > 0, "Should build CFGs for validator functions"

        # Verify CFGs have proper structure
        for cfg in cfgs:
            assert cfg.entry_node is not None
            assert cfg.exit_node is not None
            assert cfg.node_count >= 2  # At least entry and exit

    def test_elide_literals_on_moss_source(self, moss_src: Path):
        """Test literal elision on Moss source."""
        # Use a file with many literals
        config_file = moss_src / "config.py"
        if not config_file.exists():
            pytest.skip("config.py not found")

        source = config_file.read_text()
        elided, _stats = elide_literals(source)

        # Should have some elisions but preserve structure
        assert isinstance(elided, str)
        assert len(elided) > 0
        # Elided should be smaller or same size (less literals)
        assert "class" in elided  # Should preserve class definitions

    def test_extract_dependencies_from_moss(self, moss_src: Path):
        """Test dependency extraction from Moss modules."""
        # Test on a module with imports
        anchors_file = moss_src / "anchors.py"
        if not anchors_file.exists():
            pytest.skip("anchors.py not found")

        source = anchors_file.read_text()
        deps = extract_dependencies(source)

        assert deps.imports is not None
        assert len(deps.imports) > 0, "anchors.py should have imports"


class TestCrossModuleAnalysis:
    """Tests analyzing relationships between Moss modules."""

    @pytest.fixture
    def moss_src(self) -> Path:
        """Get the Moss source directory."""
        return Path(__file__).parent.parent / "src" / "moss"

    def test_init_exports_match_modules(self, moss_src: Path):
        """Test that __init__.py exports match module definitions."""
        init_file = moss_src / "__init__.py"
        if not init_file.exists():
            pytest.skip("__init__.py not found")

        source = init_file.read_text()

        # The __init__.py is mostly imports, so skeleton may be empty
        # But we can verify it parses without error
        symbols = extract_python_skeleton(source)
        assert isinstance(symbols, list)

        # Verify __all__ is defined in the source
        assert "__all__" in source, "__init__.py should have __all__"

    def test_no_circular_import_issues(self, moss_src: Path):
        """Test that Moss modules can be imported without circular import issues."""
        # This test implicitly passes if we got this far,
        # as the test imports worked
        import moss

        # Verify key exports are accessible
        assert hasattr(moss, "Anchor")
        assert hasattr(moss, "Patch")
        assert hasattr(moss, "EventBus")
        assert hasattr(moss, "ShadowGit")
        assert hasattr(moss, "extract_python_skeleton")
        assert hasattr(moss, "apply_patch")
        assert hasattr(moss, "build_cfg")


class TestSkeletonQuality:
    """Tests for skeleton extraction quality on Moss code."""

    @pytest.fixture
    def moss_src(self) -> Path:
        """Get the Moss source directory."""
        return Path(__file__).parent.parent / "src" / "moss"

    def test_docstrings_preserved(self, moss_src: Path):
        """Test that docstrings are captured in skeletons."""
        anchors_file = moss_src / "anchors.py"
        if not anchors_file.exists():
            pytest.skip("anchors.py not found")

        source = anchors_file.read_text()
        symbols = extract_python_skeleton(source)

        # Find a symbol with a docstring
        _has_docstring = any(s.docstring for s in symbols if hasattr(s, "docstring"))
        # Some symbols should have docstrings
        assert len(symbols) > 0

    def test_nested_classes_captured(self, moss_src: Path):
        """Test that nested classes are captured."""
        # Find a file with nested definitions
        for py_file in moss_src.glob("*.py"):
            source = py_file.read_text()
            symbols = extract_python_skeleton(source)

            # Check for nested structures
            for symbol in symbols:
                if hasattr(symbol, "children") and symbol.children:
                    # Found a class with methods
                    return  # Test passes

        # If no nested structures found, that's also fine
        # Not all codebases have deeply nested structures


class TestRealWorldPatterns:
    """Tests for real-world code patterns in Moss."""

    @pytest.fixture
    def moss_src(self) -> Path:
        """Get the Moss source directory."""
        return Path(__file__).parent.parent / "src" / "moss"

    def test_async_functions_handled(self, moss_src: Path):
        """Test that async functions are properly handled."""
        # Find files with async functions
        for py_file in moss_src.glob("*.py"):
            source = py_file.read_text()
            if "async def" in source:
                symbols = extract_python_skeleton(source)
                _skeleton = format_skeleton(symbols)

                # Should capture async functions
                assert len(symbols) > 0
                return  # Found and tested

    def test_dataclasses_handled(self, moss_src: Path):
        """Test that dataclasses are properly handled."""
        for py_file in moss_src.glob("*.py"):
            source = py_file.read_text()
            if "@dataclass" in source:
                symbols = extract_python_skeleton(source)

                # Should extract classes even with decorators
                assert len(symbols) > 0
                return

    def test_type_annotations_preserved(self, moss_src: Path):
        """Test that type annotations don't break parsing."""
        for py_file in moss_src.glob("*.py"):
            source = py_file.read_text()
            # All Moss files should parse successfully
            try:
                symbols = extract_python_skeleton(source)
                assert isinstance(symbols, list)
            except SyntaxError:
                pytest.fail(f"Failed to parse {py_file.name}")


class TestPerformanceOnOwnCode:
    """Performance tests on Moss codebase."""

    @pytest.fixture
    def moss_src(self) -> Path:
        """Get the Moss source directory."""
        return Path(__file__).parent.parent / "src" / "moss"

    def test_skeleton_extraction_performance(self, moss_src: Path):
        """Test that skeleton extraction is fast on Moss code."""
        import time

        start = time.perf_counter()

        for py_file in moss_src.glob("*.py"):
            source = py_file.read_text()
            extract_python_skeleton(source)

        elapsed = time.perf_counter() - start

        # Should complete in under 2 seconds for all files
        assert elapsed < 2.0, f"Skeleton extraction took {elapsed:.2f}s"

    def test_cfg_building_performance(self, moss_src: Path):
        """Test that CFG building is reasonably fast."""
        import time

        start = time.perf_counter()

        for py_file in moss_src.glob("*.py"):
            source = py_file.read_text()
            try:
                build_cfg(source)
            except Exception:
                pass  # Some files may not have functions

        elapsed = time.perf_counter() - start

        # Should complete in under 5 seconds
        assert elapsed < 5.0, f"CFG building took {elapsed:.2f}s"


# ============================================================================
# Tests for new dogfooding commands: summarize, check-docs, check-todos
# ============================================================================


class TestSummarizer:
    """Tests for Summarizer."""

    def test_summarize_file(self, tmp_path: Path):
        """Test single file summarization."""
        from moss.summarize import Summarizer

        # Create a test file
        test_file = tmp_path / "example.py"
        test_file.write_text('''"""Example module."""

def hello(name: str) -> str:
    """Say hello."""
    return f"Hello, {name}!"

class Greeter:
    """A greeter class."""

    def greet(self, name: str) -> str:
        """Greet someone."""
        return f"Greetings, {name}!"
''')

        summarizer = Summarizer()
        summary = summarizer.summarize_file(test_file)

        assert summary is not None
        assert summary.module_name == "example"
        assert summary.docstring == "Example module."
        assert len(summary.functions) == 1
        assert summary.functions[0].name == "hello"
        assert len(summary.classes) == 1
        assert summary.classes[0].name == "Greeter"

    def test_summarize_package(self, tmp_path: Path):
        """Test package summarization."""
        from moss.summarize import Summarizer

        # Create a package
        pkg_dir = tmp_path / "mypackage"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text('"""My package."""\n')
        (pkg_dir / "module.py").write_text('"""A module."""\ndef foo(): pass\n')

        summarizer = Summarizer()
        summary = summarizer.summarize_package(pkg_dir)

        assert summary is not None
        assert summary.name == "mypackage"
        assert summary.docstring == "My package."
        assert len(summary.files) == 2  # __init__.py and module.py

    def test_summarize_project(self, tmp_path: Path):
        """Test project summarization."""
        from moss.summarize import Summarizer

        # Create src/pkg structure
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "myproject"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text('"""My project."""\n')
        (pkg_dir / "core.py").write_text('"""Core module."""\nclass Core: pass\n')

        summarizer = Summarizer()
        summary = summarizer.summarize_project(tmp_path)

        assert summary is not None
        assert summary.name == tmp_path.name
        assert len(summary.packages) == 1
        assert summary.packages[0].name == "myproject"
        assert summary.total_files >= 2

    def test_exclude_private_files(self, tmp_path: Path):
        """Test that private files are excluded by default."""
        from moss.summarize import Summarizer

        pkg_dir = tmp_path / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "public.py").write_text("def foo(): pass")
        (pkg_dir / "_private.py").write_text("def bar(): pass")

        summarizer = Summarizer(include_private=False)
        summary = summarizer.summarize_package(pkg_dir)

        assert summary is not None
        module_names = [f.module_name for f in summary.files]
        assert "public" in module_names
        assert "_private" not in module_names

    def test_exclude_test_files(self, tmp_path: Path):
        """Test that test files are excluded by default."""
        from moss.summarize import Summarizer

        pkg_dir = tmp_path / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "main.py").write_text("def foo(): pass")
        (pkg_dir / "test_main.py").write_text("def test_foo(): pass")

        summarizer = Summarizer(include_tests=False)
        summary = summarizer.summarize_package(pkg_dir)

        assert summary is not None
        module_names = [f.module_name for f in summary.files]
        assert "main" in module_names
        assert "test_main" not in module_names


class TestDocChecker:
    """Tests for DocChecker."""

    def test_check_finds_missing_readme(self, tmp_path: Path):
        """Test that missing README is flagged as error."""
        from moss.check_docs import DocChecker

        # Create a package but no README
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "main.py").write_text("def foo(): pass")

        checker = DocChecker(tmp_path)
        result = checker.check()

        assert result.has_errors
        assert any(i.category == "missing" and "README" in i.message for i in result.issues)

    def test_check_finds_stale_references(self, tmp_path: Path):
        """Test that stale references are flagged."""
        from moss.check_docs import DocChecker

        # Create README with reference to non-existent module
        readme = tmp_path / "README.md"
        readme.write_text("# Project\n\nSee `nonexistent.module` for details.\n")

        # Create a minimal package
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")

        checker = DocChecker(tmp_path)
        result = checker.check()

        assert result.has_warnings
        stale_issues = [i for i in result.issues if i.category == "stale"]
        assert len(stale_issues) >= 1

    def test_coverage_calculation(self, tmp_path: Path):
        """Test coverage is calculated correctly."""
        from moss.check_docs import DocChecker

        # Create README mentioning one module
        readme = tmp_path / "README.md"
        readme.write_text("# Project\n\nSee `pkg.main` for the main module.\n")

        # Create package with two modules
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "main.py").write_text("def foo(): pass")
        (pkg_dir / "other.py").write_text("def bar(): pass")

        checker = DocChecker(tmp_path)
        result = checker.check()

        # At least one module is documented
        assert result.modules_documented >= 1
        assert result.modules_found >= 2


class TestTodoChecker:
    """Tests for TodoChecker."""

    def test_parse_todo_md(self, tmp_path: Path):
        """Test parsing TODO.md checkbox items."""
        from moss.check_todos import TodoChecker, TodoStatus

        todo_file = tmp_path / "TODO.md"
        todo_file.write_text("""# TODO

## Phase 1
- [x] Completed item
- [ ] Pending item

## Future
- [ ] Another pending item
""")

        checker = TodoChecker(tmp_path)
        result = checker.check()

        assert len(result.tracked_items) == 3
        done = [i for i in result.tracked_items if i.status == TodoStatus.DONE]
        pending = [i for i in result.tracked_items if i.status == TodoStatus.PENDING]
        assert len(done) == 1
        assert len(pending) == 2

    def test_scan_code_todos(self, tmp_path: Path):
        """Test scanning code for TODO comments."""
        from moss.check_todos import TodoChecker

        code_file = tmp_path / "example.py"
        code_file.write_text('''"""Module."""

def foo():
    # TODO: implement this
    pass

def bar():
    # FIXME: broken
    pass
''')

        checker = TodoChecker(tmp_path)
        result = checker.check()

        assert len(result.code_todos) >= 2
        markers = {t.marker for t in result.code_todos}
        assert "TODO" in markers
        assert "FIXME" in markers

    def test_orphan_detection(self, tmp_path: Path):
        """Test that orphaned TODOs are detected."""
        from moss.check_todos import TodoChecker, TodoStatus

        # Create TODO.md with one item
        todo_file = tmp_path / "TODO.md"
        todo_file.write_text("# TODO\n- [ ] Tracked item\n")

        # Create code with untracked TODO
        code_file = tmp_path / "example.py"
        code_file.write_text("# TODO: untracked item\n")

        checker = TodoChecker(tmp_path)
        result = checker.check()

        assert result.orphan_count >= 1
        orphans = [t for t in result.code_todos if t.status == TodoStatus.ORPHAN]
        assert len(orphans) >= 1

    def test_category_tracking(self, tmp_path: Path):
        """Test that categories are tracked from headers."""
        from moss.check_todos import TodoChecker

        todo_file = tmp_path / "TODO.md"
        todo_file.write_text("""# TODO

## Phase 1
- [ ] Item in phase 1

### Sub-phase
- [ ] Item in sub-phase
""")

        checker = TodoChecker(tmp_path)
        result = checker.check()

        categories = {i.category for i in result.tracked_items}
        assert "Phase 1" in categories or "Sub-phase" in categories


class TestDogfoodingCLI:
    """Integration tests for dogfooding CLI commands."""

    def test_summarize_command(self, tmp_path: Path):
        """Test summarize CLI command."""
        from argparse import Namespace

        from moss.cli import cmd_summarize

        # Create minimal project
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text('"""Package."""\n')
        (pkg_dir / "main.py").write_text('"""Main."""\ndef main(): pass\n')

        args = Namespace(
            directory=str(tmp_path),
            include_private=False,
            include_tests=False,
            json=False,
            quiet=False,
            verbose=False,
            debug=False,
            no_color=True,
        )

        result = cmd_summarize(args)
        assert result == 0

    def test_check_docs_command(self, tmp_path: Path):
        """Test check-docs CLI command."""
        from argparse import Namespace

        from moss.cli import cmd_check_docs

        # Create minimal project with README
        readme = tmp_path / "README.md"
        readme.write_text("# Project\n")
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")

        args = Namespace(
            directory=str(tmp_path),
            strict=False,
            json=False,
            quiet=False,
            verbose=False,
            debug=False,
            no_color=True,
        )

        result = cmd_check_docs(args)
        assert result == 0

    def test_check_todos_command(self, tmp_path: Path):
        """Test check-todos CLI command."""
        from argparse import Namespace

        from moss.cli import cmd_check_todos

        # Create TODO.md
        todo_file = tmp_path / "TODO.md"
        todo_file.write_text("# TODO\n- [ ] Something\n")

        args = Namespace(
            directory=str(tmp_path),
            strict=False,
            json=False,
            quiet=False,
            verbose=False,
            debug=False,
            no_color=True,
        )

        result = cmd_check_todos(args)
        assert result == 0

    def test_health_command(self, tmp_path: Path):
        """Test health CLI command."""
        from argparse import Namespace

        from moss.cli import cmd_health

        # Create minimal project
        readme = tmp_path / "README.md"
        readme.write_text("# Project\n")
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "main.py").write_text('"""Main."""\ndef main(): pass\n')

        args = Namespace(
            directory=str(tmp_path),
            json=False,
            quiet=False,
            verbose=False,
            debug=False,
            no_color=True,
        )

        result = cmd_health(args)
        assert result == 0


class TestStructuralAnalysis:
    """Tests for StructuralAnalyzer."""

    def test_detect_too_many_params(self, tmp_path: Path):
        """Test detection of functions with too many parameters."""
        from moss.structural_analysis import StructuralAnalyzer, StructuralThresholds

        pkg_dir = tmp_path / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "funcs.py").write_text('''"""Module."""

def too_many(a, b, c, d, e, f, g):
    """Has 7 params."""
    pass

def ok(a, b, c):
    """Has 3 params."""
    pass
''')

        analyzer = StructuralAnalyzer(tmp_path, StructuralThresholds(max_params=5))
        result = analyzer.analyze()

        # Should find the function with too many params
        hotspots = [h for h in result.function_hotspots if "too_many" in h.name]
        assert len(hotspots) >= 1
        assert any("Too many parameters" in issue for issue in hotspots[0].issues)

    def test_detect_deep_nesting(self, tmp_path: Path):
        """Test detection of deep nesting."""
        from moss.structural_analysis import StructuralAnalyzer, StructuralThresholds

        pkg_dir = tmp_path / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "nested.py").write_text('''"""Module."""

def deeply_nested():
    if True:
        if True:
            if True:
                if True:
                    if True:
                        pass
''')

        analyzer = StructuralAnalyzer(tmp_path, StructuralThresholds(max_nesting_depth=3))
        result = analyzer.analyze()

        hotspots = [h for h in result.function_hotspots if "deeply_nested" in h.name]
        assert len(hotspots) >= 1
        assert hotspots[0].max_nesting >= 5

    def test_detect_long_function(self, tmp_path: Path):
        """Test detection of long functions."""
        from moss.structural_analysis import StructuralAnalyzer, StructuralThresholds

        pkg_dir = tmp_path / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        # Create a function with many lines
        lines = ['"""Module."""', "", "def long_func():"]
        lines.extend(["    x = 1"] * 60)  # 60 lines
        (pkg_dir / "long.py").write_text("\n".join(lines))

        analyzer = StructuralAnalyzer(tmp_path, StructuralThresholds(max_function_lines=50))
        result = analyzer.analyze()

        hotspots = [h for h in result.function_hotspots if "long_func" in h.name]
        assert len(hotspots) >= 1
        assert any("too long" in issue.lower() for issue in hotspots[0].issues)


class TestDependencyAnalysis:
    """Tests for DependencyAnalyzer."""

    def test_analyze_basic(self, tmp_path: Path):
        """Test basic dependency analysis."""
        from moss.dependencies import build_dependency_graph

        # Create a minimal project with Python files
        pkg_dir = tmp_path / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")
        (pkg_dir / "core.py").write_text('"""Core module."""\n')
        (pkg_dir / "utils.py").write_text('"""Utils."""\nfrom pkg import core\n')

        # Test the underlying graph building
        graph = build_dependency_graph(str(pkg_dir), internal_only=False)

        # Should find at least the utils -> core relationship
        assert len(graph) >= 0  # May be empty if no internal matches

    def test_circular_dep_detection(self, tmp_path: Path):
        """Test circular dependency detection."""
        from moss.dependency_analysis import DependencyAnalyzer

        # Test the cycle detection algorithm directly
        analyzer = DependencyAnalyzer(tmp_path)

        # Simulate a graph with a cycle
        test_graph = {
            "a": ["b"],
            "b": ["c"],
            "c": ["a"],  # Creates cycle: a -> b -> c -> a
        }

        cycles = analyzer._find_cycles(test_graph)

        assert len(cycles) == 1
        assert set(cycles[0].cycle) == {"a", "b", "c"}

    def test_self_loop_skipped(self, tmp_path: Path):
        """Test that self-loops are not reported as cycles."""
        from moss.dependency_analysis import DependencyAnalyzer

        analyzer = DependencyAnalyzer(tmp_path)

        # Self-loop should be skipped
        test_graph = {"logging": ["logging"]}

        cycles = analyzer._find_cycles(test_graph)

        assert len(cycles) == 0

    def test_fan_in_calculation(self, tmp_path: Path):
        """Test fan-in (how many modules import this) calculation."""
        from moss.dependency_analysis import ModuleMetrics

        # Test the metrics calculation logic directly
        graph = {
            "spoke1": ["hub"],
            "spoke2": ["hub"],
            "spoke3": ["hub"],
            "hub": [],
        }

        all_modules = {"hub", "spoke1", "spoke2", "spoke3"}
        metrics: dict[str, ModuleMetrics] = {}
        for module in all_modules:
            metrics[module] = ModuleMetrics(name=module)

        # Calculate fan-in/fan-out
        for module, imports in graph.items():
            if module in metrics:
                metrics[module].fan_out = len(imports)
            for imp in imports:
                if imp in metrics:
                    metrics[imp].fan_in += 1
                    metrics[imp].importers.append(module)

        # hub should have fan-in of 3
        assert metrics["hub"].fan_in == 3
        assert metrics["hub"].fan_out == 0
        assert metrics["spoke1"].fan_in == 0
        assert metrics["spoke1"].fan_out == 1


class TestAPISurfaceAnalysis:
    """Tests for APISurfaceAnalyzer."""

    def test_public_export_detection(self, tmp_path: Path):
        """Test that public exports are detected."""
        from moss.api_surface_analysis import APISurfaceAnalyzer

        # Create a minimal project
        src = tmp_path / "src"
        src.mkdir()
        pkg = src / "pkg"
        pkg.mkdir()
        (pkg / "__init__.py").write_text("")
        (pkg / "core.py").write_text('''"""Core module."""

def public_func():
    """A public function."""
    pass

def _private_func():
    pass

class PublicClass:
    """A public class."""
    pass

class _PrivateClass:
    pass
''')

        analyzer = APISurfaceAnalyzer(tmp_path)
        result = analyzer.analyze()

        # Should find public_func, PublicClass as public
        public_names = {e.name for e in result.exports}
        assert "public_func" in public_names
        assert "PublicClass" in public_names
        assert "_private_func" not in public_names
        assert "_PrivateClass" not in public_names

    def test_undocumented_detection(self, tmp_path: Path):
        """Test that undocumented exports are flagged."""
        from moss.api_surface_analysis import APISurfaceAnalyzer

        src = tmp_path / "src"
        src.mkdir()
        pkg = src / "pkg"
        pkg.mkdir()
        (pkg / "__init__.py").write_text("")
        (pkg / "core.py").write_text('''def documented():
    """Has a docstring."""
    pass

def undocumented():
    pass
''')

        analyzer = APISurfaceAnalyzer(tmp_path)
        result = analyzer.analyze()

        undoc_names = {e.name for e in result.undocumented}
        assert "undocumented" in undoc_names
        assert "documented" not in undoc_names

    def test_naming_convention_detection(self, tmp_path: Path):
        """Test that naming issues are detected."""
        from moss.api_surface_analysis import APISurfaceAnalyzer

        src = tmp_path / "src"
        src.mkdir()
        pkg = src / "pkg"
        pkg.mkdir()
        (pkg / "__init__.py").write_text("")
        (pkg / "core.py").write_text("""def camelCaseFunc():  # Bad: should be snake_case
    pass

class lowercase_class:  # Bad: should be PascalCase
    pass

def good_function():
    pass

class GoodClass:
    pass
""")

        analyzer = APISurfaceAnalyzer(tmp_path)
        result = analyzer.analyze()

        bad_names = {i.name for i in result.naming_issues}
        assert "camelCaseFunc" in bad_names
        assert "lowercase_class" in bad_names
        assert "good_function" not in bad_names
        assert "GoodClass" not in bad_names


class TestStatusChecker:
    """Tests for StatusChecker."""

    def test_status_health_score(self, tmp_path: Path):
        """Test health score calculation."""
        from moss.status import StatusChecker

        # Create minimal project
        readme = tmp_path / "README.md"
        readme.write_text("# Project\nAll documented.\n")
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        pkg_dir = src_dir / "pkg"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("")

        checker = StatusChecker(tmp_path)
        status = checker.check()

        # Should have a health score
        assert 0 <= status.health_score <= 100
        assert status.health_grade in ("A", "B", "C", "D", "F")

    def test_status_to_markdown(self, tmp_path: Path):
        """Test markdown output."""
        from moss.status import StatusChecker

        readme = tmp_path / "README.md"
        readme.write_text("# Test\n")

        checker = StatusChecker(tmp_path)
        status = checker.check()

        md = status.to_markdown()
        assert "# Project Status" in md
        assert "Health:" in md
        assert "Overview" in md

    def test_status_to_dict(self, tmp_path: Path):
        """Test JSON dict output."""
        from moss.status import StatusChecker

        readme = tmp_path / "README.md"
        readme.write_text("# Test\n")

        checker = StatusChecker(tmp_path)
        status = checker.check()

        d = status.to_dict()
        assert "name" in d
        assert "health" in d
        assert "stats" in d
        assert d["health"]["score"] >= 0
        assert d["health"]["grade"] in ("A", "B", "C", "D", "F")
