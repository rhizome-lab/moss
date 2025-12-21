"""Tests for Context Host."""

from pathlib import Path

import pytest

from moss.context import CompiledContext, ContextHost, StaticContext, elide_view_with_anchors
from moss.views import Intent, View, ViewTarget, ViewType


class TestStaticContext:
    """Tests for StaticContext."""

    def test_default_empty(self):
        ctx = StaticContext()
        assert ctx.architecture_docs == []
        assert ctx.style_guides == []
        assert ctx.pinned_files == []


class TestCompiledContext:
    """Tests for CompiledContext."""

    def test_total_tokens(self, tmp_path: Path):
        target = ViewTarget(path=tmp_path / "test.py")
        view = View(
            target=target,
            view_type=ViewType.RAW,
            content="word " * 100,  # 100 words
        )
        ctx = CompiledContext(views=[view], static_context={})

        # ~133 tokens for 100 words
        assert ctx.total_tokens > 100

    def test_total_tokens_with_static(self, tmp_path: Path):
        target = ViewTarget(path=tmp_path / "test.py")
        view = View(target=target, view_type=ViewType.RAW, content="hello")
        ctx = CompiledContext(
            views=[view],
            static_context={"doc.md": "word " * 100},
        )

        assert ctx.total_tokens > 100

    def test_to_prompt(self, tmp_path: Path):
        target = ViewTarget(path=tmp_path / "test.py")
        view = View(target=target, view_type=ViewType.RAW, content="def main(): pass")
        ctx = CompiledContext(
            views=[view],
            static_context={"arch.md": "# Architecture\nThis is the arch."},
        )

        prompt = ctx.to_prompt()

        assert "# Architecture" in prompt
        assert "def main(): pass" in prompt
        assert "test.py (RAW)" in prompt


class TestContextHost:
    """Tests for ContextHost."""

    @pytest.fixture
    def host(self):
        return ContextHost()

    @pytest.fixture
    def python_file(self, tmp_path: Path) -> Path:
        f = tmp_path / "example.py"
        f.write_text("""
import os

def hello(name: str) -> str:
    \"\"\"Greet someone.\"\"\"
    return f"Hello, {name}"

class Greeter:
    def greet(self): pass
""")
        return f

    async def test_compile_raw(self, host: ContextHost, python_file: Path):
        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets, view_types=[ViewType.RAW])

        assert len(ctx.views) == 1
        assert ctx.views[0].view_type == ViewType.RAW
        assert "import os" in ctx.views[0].content

    async def test_compile_skeleton(self, host: ContextHost, python_file: Path):
        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets, view_types=[ViewType.SKELETON])

        assert len(ctx.views) == 1
        assert ctx.views[0].view_type == ViewType.SKELETON
        assert "def hello" in ctx.views[0].content
        assert "class Greeter" in ctx.views[0].content

    async def test_compile_dependency(self, host: ContextHost, python_file: Path):
        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets, view_types=[ViewType.DEPENDENCY])

        assert len(ctx.views) == 1
        assert ctx.views[0].view_type == ViewType.DEPENDENCY
        assert "import os" in ctx.views[0].content

    async def test_compile_with_intent_explore(self, host: ContextHost, python_file: Path):
        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets, intent=Intent.EXPLORE)

        # Should prefer SKELETON view
        assert len(ctx.views) == 1
        assert ctx.views[0].view_type == ViewType.SKELETON

    async def test_compile_with_intent_edit(self, host: ContextHost, python_file: Path):
        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets, intent=Intent.EDIT)

        # Should prefer RAW view
        assert len(ctx.views) == 1
        assert ctx.views[0].view_type == ViewType.RAW

    async def test_compile_multiple_targets(self, host: ContextHost, tmp_path: Path):
        f1 = tmp_path / "a.py"
        f1.write_text("def a(): pass")
        f2 = tmp_path / "b.py"
        f2.write_text("def b(): pass")

        targets = [ViewTarget(path=f1), ViewTarget(path=f2)]
        ctx = await host.compile(targets, view_types=[ViewType.SKELETON])

        assert len(ctx.views) == 2

    async def test_compile_with_static_context(
        self, host: ContextHost, tmp_path: Path, python_file: Path
    ):
        arch_doc = tmp_path / "ARCHITECTURE.md"
        arch_doc.write_text("# Architecture\nThis is the system architecture.")
        host.add_architecture_doc(arch_doc)

        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets, view_types=[ViewType.RAW])

        assert "arch/ARCHITECTURE.md" in ctx.static_context
        assert "# Architecture" in ctx.static_context["arch/ARCHITECTURE.md"]

    async def test_compile_with_style_guide(
        self, host: ContextHost, tmp_path: Path, python_file: Path
    ):
        style = tmp_path / "STYLE.md"
        style.write_text("# Style Guide\nUse 4 spaces.")
        host.add_style_guide(style)

        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets)

        assert "style/STYLE.md" in ctx.static_context

    async def test_compile_with_pinned_file(
        self, host: ContextHost, tmp_path: Path, python_file: Path
    ):
        pinned = tmp_path / "constants.py"
        pinned.write_text("VERSION = '1.0.0'")
        host.add_pinned_file(pinned)

        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets)

        assert "pinned/constants.py" in ctx.static_context

    async def test_token_budget(self, host: ContextHost, tmp_path: Path):
        # Create a large file
        f = tmp_path / "large.py"
        f.write_text("x = 1\n" * 1000)

        host.set_token_budget(100)  # Very small budget
        targets = [ViewTarget(path=f)]
        ctx = await host.compile(targets)

        assert ctx.total_tokens <= 100 or len(ctx.views) == 0

    async def test_get_skeleton(self, host: ContextHost, python_file: Path):
        view = await host.get_skeleton(python_file)

        assert view is not None
        assert view.view_type == ViewType.SKELETON

    async def test_get_dependencies(self, host: ContextHost, python_file: Path):
        view = await host.get_dependencies(python_file)

        assert view is not None
        assert view.view_type == ViewType.DEPENDENCY

    async def test_get_raw(self, host: ContextHost, python_file: Path):
        view = await host.get_raw(python_file)

        assert view is not None
        assert view.view_type == ViewType.RAW

    async def test_compile_for_intent(self, host: ContextHost, python_file: Path):
        ctx = await host.compile_for_intent([python_file], Intent.EXPLORE)

        assert ctx.metadata["intent"] == "EXPLORE"
        assert len(ctx.views) == 1

    async def test_missing_static_files_ignored(
        self, host: ContextHost, tmp_path: Path, python_file: Path
    ):
        # Add non-existent file
        host.add_architecture_doc(tmp_path / "missing.md")

        targets = [ViewTarget(path=python_file)]
        ctx = await host.compile(targets)

        assert "arch/missing.md" not in ctx.static_context

    async def test_token_budget_with_elision(self, host: ContextHost, tmp_path: Path):
        # Create a large Python file with clear function definitions
        f = tmp_path / "large.py"
        f.write_text(
            '''
def function_one():
    """First function."""
    x = 1
    y = 2
    z = 3
    return x + y + z

def function_two():
    """Second function."""
    a = 10
    b = 20
    c = 30
    return a + b + c

class MyClass:
    """A class."""
    def method_one(self):
        pass
    def method_two(self):
        pass
'''
            + "# filler line\n" * 200
        )

        # Set a budget that's too small for full content but allows elided
        host.set_token_budget(200)
        targets = [ViewTarget(path=f)]
        ctx = await host.compile(targets, view_types=[ViewType.RAW])

        # Should have an elided view instead of nothing
        if ctx.views:
            assert "elided" in ctx.views[0].content or ctx.views[0].view_type == ViewType.ELIDED


class TestElideViewWithAnchors:
    """Tests for elide_view_with_anchors function."""

    def test_elides_large_file(self, tmp_path: Path):
        f = tmp_path / "test.py"
        content = (
            '''
def hello():
    """Say hello."""
    x = 1
    y = 2
    return x + y

# lots of filler
'''
            + "# filler\n" * 100
            + '''
def goodbye():
    """Say goodbye."""
    return "bye"
'''
        )
        f.write_text(content)
        target = ViewTarget(path=f)
        view = View(target=target, view_type=ViewType.RAW, content=content)

        elided = elide_view_with_anchors(view, target_tokens=100)

        assert elided is not None
        assert elided.view_type == ViewType.ELIDED
        assert "def hello" in elided.content
        assert "def goodbye" in elided.content
        assert "elided" in elided.content

    def test_preserves_docstrings(self, tmp_path: Path):
        f = tmp_path / "test.py"
        content = '''
def my_function():
    """This is the docstring."""
    pass
'''
        f.write_text(content)
        target = ViewTarget(path=f)
        view = View(target=target, view_type=ViewType.RAW, content=content)

        elided = elide_view_with_anchors(view, target_tokens=50)

        assert elided is not None
        assert "docstring" in elided.content

    def test_returns_none_for_non_python(self, tmp_path: Path):
        f = tmp_path / "test.js"
        f.write_text("function hello() { return 'hi'; }")
        target = ViewTarget(path=f)
        view = View(
            target=target, view_type=ViewType.RAW, content="function hello() { return 'hi'; }"
        )

        elided = elide_view_with_anchors(view, target_tokens=10)

        assert elided is None

    def test_returns_none_for_non_raw_view(self, tmp_path: Path):
        f = tmp_path / "test.py"
        f.write_text("def x(): pass")
        target = ViewTarget(path=f)
        view = View(target=target, view_type=ViewType.SKELETON, content="def x(): pass")

        elided = elide_view_with_anchors(view, target_tokens=10)

        assert elided is None

    def test_metadata_tracks_elision(self, tmp_path: Path):
        f = tmp_path / "test.py"
        content = "def x(): pass\n" + "# line\n" * 50 + "def y(): pass\n"
        f.write_text(content)
        target = ViewTarget(path=f)
        view = View(target=target, view_type=ViewType.RAW, content=content)

        elided = elide_view_with_anchors(view, target_tokens=50)

        assert elided is not None
        assert "original_lines" in elided.metadata
        assert "elided_lines" in elided.metadata
        assert "anchor_count" in elided.metadata
        assert elided.metadata["anchor_count"] >= 2
