"""Tests for Skeleton Provider."""

from pathlib import Path

import pytest

from moss.skeleton import (
    PythonSkeletonProvider,
    extract_python_skeleton,
    format_skeleton,
)
from moss.views import ViewOptions, ViewTarget, ViewType


class TestPythonSkeletonExtractor:
    """Tests for PythonSkeletonExtractor."""

    def test_extract_function(self):
        source = '''
def hello(name: str) -> str:
    """Greet someone."""
    return f"Hello, {name}"
'''
        symbols = extract_python_skeleton(source)

        assert len(symbols) == 1
        assert symbols[0].name == "hello"
        assert symbols[0].kind == "function"
        assert "def hello(name: str) -> str" in symbols[0].signature
        assert symbols[0].docstring == "Greet someone."

    def test_extract_async_function(self):
        source = '''
async def fetch(url: str) -> bytes:
    """Fetch data from URL."""
    pass
'''
        symbols = extract_python_skeleton(source)

        assert len(symbols) == 1
        assert "async def fetch" in symbols[0].signature

    def test_extract_class(self):
        source = '''
class MyClass:
    """A test class."""

    def method(self) -> None:
        """Do something."""
        pass
'''
        symbols = extract_python_skeleton(source)

        assert len(symbols) == 1
        assert symbols[0].name == "MyClass"
        assert symbols[0].kind == "class"
        assert len(symbols[0].children) == 1
        assert symbols[0].children[0].name == "method"
        assert symbols[0].children[0].kind == "method"

    def test_extract_class_with_bases(self):
        source = """
class Child(Parent, Mixin):
    pass
"""
        symbols = extract_python_skeleton(source)

        assert "class Child(Parent, Mixin)" in symbols[0].signature

    def test_exclude_private_by_default(self):
        source = """
def public(): pass
def _private(): pass
def __dunder__(): pass
"""
        symbols = extract_python_skeleton(source)

        names = [s.name for s in symbols]
        assert "public" in names
        assert "_private" not in names
        assert "__dunder__" in names  # Dunder methods are included

    def test_include_private(self):
        source = """
def public(): pass
def _private(): pass
"""
        symbols = extract_python_skeleton(source, include_private=True)

        names = [s.name for s in symbols]
        assert "public" in names
        assert "_private" in names

    def test_function_with_defaults(self):
        source = """
def func(a, b=10, c="hello"): pass
"""
        symbols = extract_python_skeleton(source)

        assert "b = 10" in symbols[0].signature
        assert "c = 'hello'" in symbols[0].signature

    def test_function_with_args_kwargs(self):
        source = """
def func(*args, **kwargs): pass
"""
        symbols = extract_python_skeleton(source)

        assert "*args" in symbols[0].signature
        assert "**kwargs" in symbols[0].signature


class TestFormatSkeleton:
    """Tests for format_skeleton."""

    def test_format_simple_function(self):
        symbols = extract_python_skeleton("def hello(): pass")
        output = format_skeleton(symbols)

        assert "def hello():" in output
        assert "..." in output

    def test_format_with_docstring(self):
        source = '''
def hello():
    """Say hello."""
    pass
'''
        symbols = extract_python_skeleton(source)
        output = format_skeleton(symbols, include_docstrings=True)

        assert '"""Say hello."""' in output

    def test_format_without_docstring(self):
        source = '''
def hello():
    """Say hello."""
    pass
'''
        symbols = extract_python_skeleton(source)
        output = format_skeleton(symbols, include_docstrings=False)

        assert '"""' not in output

    def test_format_class_with_methods(self):
        source = """
class MyClass:
    def method(self): pass
"""
        symbols = extract_python_skeleton(source)
        output = format_skeleton(symbols)

        assert "class MyClass:" in output
        assert "def method(self):" in output
        # Method should be indented
        lines = output.splitlines()
        method_line = next(line for line in lines if "def method" in line)
        assert method_line.startswith("    ")


class TestPythonSkeletonProvider:
    """Tests for PythonSkeletonProvider."""

    @pytest.fixture
    def provider(self):
        return PythonSkeletonProvider()

    @pytest.fixture
    def python_file(self, tmp_path: Path) -> Path:
        f = tmp_path / "test.py"
        f.write_text('''
class Calculator:
    """A simple calculator."""

    def add(self, a: int, b: int) -> int:
        """Add two numbers."""
        return a + b

    def subtract(self, a: int, b: int) -> int:
        """Subtract two numbers."""
        return a - b


def main():
    """Entry point."""
    calc = Calculator()
    print(calc.add(1, 2))
''')
        return f

    def test_view_type(self, provider: PythonSkeletonProvider):
        assert provider.view_type == ViewType.SKELETON

    def test_supported_languages(self, provider: PythonSkeletonProvider):
        assert provider.supported_languages == {"python"}

    def test_supports_python_file(self, provider: PythonSkeletonProvider, python_file: Path):
        target = ViewTarget(path=python_file, language="python")
        assert provider.supports(target)

    def test_supports_python_extension(self, provider: PythonSkeletonProvider, python_file: Path):
        target = ViewTarget(path=python_file)
        assert provider.supports(target)

    async def test_render(self, provider: PythonSkeletonProvider, python_file: Path):
        target = ViewTarget(path=python_file)
        view = await provider.render(target)

        assert view.view_type == ViewType.SKELETON
        assert "class Calculator" in view.content
        assert "def add" in view.content
        assert "def main" in view.content
        assert view.metadata["symbol_count"] == 2  # Calculator and main
        assert view.metadata["language"] == "python"

    async def test_render_exclude_private(self, provider: PythonSkeletonProvider, tmp_path: Path):
        f = tmp_path / "private.py"
        f.write_text("""
def public(): pass
def _private(): pass
""")
        target = ViewTarget(path=f)
        view = await provider.render(target)

        assert "public" in view.content
        assert "_private" not in view.content

    async def test_render_include_private(self, provider: PythonSkeletonProvider, tmp_path: Path):
        f = tmp_path / "private.py"
        f.write_text("""
def public(): pass
def _private(): pass
""")
        target = ViewTarget(path=f)
        view = await provider.render(target, ViewOptions(include_private=True))

        assert "public" in view.content
        assert "_private" in view.content

    async def test_render_syntax_error(self, provider: PythonSkeletonProvider, tmp_path: Path):
        f = tmp_path / "broken.py"
        f.write_text("def broken(")
        target = ViewTarget(path=f)

        view = await provider.render(target)

        assert "Parse error" in view.content
        assert "error" in view.metadata
