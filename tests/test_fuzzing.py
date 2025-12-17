"""Fuzzing tests for edge cases in AST parsing and processing.

These tests verify that Moss handles malformed inputs, edge cases,
and unusual code patterns gracefully without crashing.
"""

import pytest

from moss.anchors import Anchor, AnchorType, find_anchors
from moss.cfg import build_cfg
from moss.elided_literals import elide_literals
from moss.patches import Patch, PatchType, apply_patch, apply_text_patch
from moss.skeleton import extract_python_skeleton


class TestSkeletonFuzzing:
    """Fuzzing tests for skeleton extraction."""

    @pytest.mark.parametrize(
        "source",
        [
            "",  # Empty
            "   ",  # Whitespace only
            "\n\n\n",  # Newlines only
            "#comment",  # Comment only
            "# -*- coding: utf-8 -*-",  # Encoding declaration
            "pass",  # Single statement
            "...",  # Ellipsis
            "1 + 1",  # Expression
        ],
    )
    def test_minimal_inputs(self, source):
        """Test skeleton extraction with minimal/degenerate inputs."""
        # Should not crash
        symbols = extract_python_skeleton(source)
        # May return empty list for trivial inputs
        assert isinstance(symbols, list)

    @pytest.mark.parametrize(
        "source",
        [
            "def",  # Incomplete
            "def foo",  # Missing parens
            "def foo(",  # Unclosed paren
            "class",  # Incomplete class
            "class Foo",  # Missing colon
            "class Foo:",  # No body
            "async def",  # Incomplete async
            "def foo():\n",  # No body
            "@",  # Lone decorator
            "@decorator",  # Decorator without function
        ],
    )
    def test_invalid_syntax(self, source):
        """Test skeleton extraction with invalid syntax."""
        # Invalid syntax may raise SyntaxError or return empty list
        try:
            symbols = extract_python_skeleton(source)
            assert isinstance(symbols, list)
        except SyntaxError:
            pass  # Expected for invalid syntax

    def test_deeply_nested_classes(self):
        """Test deeply nested class structures."""
        # Generate deeply nested classes
        nesting = 20
        lines = []
        for i in range(nesting):
            indent = "    " * i
            lines.append(f"{indent}class Nested{i}:")
            lines.append(f"{indent}    pass")

        source = "\n".join(lines)
        symbols = extract_python_skeleton(source)
        # Should extract at least the outermost class
        assert len(symbols) >= 1

    def test_many_methods(self):
        """Test class with many methods."""
        methods = [f"    def method_{i}(self): pass" for i in range(100)]
        source = "class ManyMethods:\n" + "\n".join(methods)

        symbols = extract_python_skeleton(source)
        assert len(symbols) == 1
        assert len(symbols[0].children) == 100

    def test_unicode_identifiers(self):
        """Test Unicode identifiers."""
        source = """
class 日本語クラス:
    def メソッド(self):
        pass

def функция():
    pass

π = 3.14159
"""
        symbols = extract_python_skeleton(source)
        # Should handle Unicode without crashing
        assert isinstance(symbols, list)

    def test_very_long_line(self):
        """Test very long lines."""
        long_name = "a" * 1000
        source = f"def {long_name}(): pass"
        symbols = extract_python_skeleton(source)
        assert len(symbols) == 1
        assert long_name in symbols[0].name


class TestAnchorFuzzing:
    """Fuzzing tests for anchor resolution."""

    @pytest.mark.parametrize(
        "anchor_name",
        [
            "",  # Empty name
            " ",  # Space
            "123",  # Numeric start
            "a" * 1000,  # Very long
            "foo.bar",  # Dotted
            "foo-bar",  # Hyphenated (invalid Python)
            "λ",  # Unicode
            "\n",  # Newline
        ],
    )
    def test_unusual_anchor_names(self, anchor_name):
        """Test anchors with unusual names."""
        source = "def foo(): pass"
        anchor = Anchor(type=AnchorType.FUNCTION, name=anchor_name)

        # Should not crash
        try:
            matches = find_anchors(source, anchor)
            assert isinstance(matches, list)
        except Exception:
            # Some invalid names may raise, which is acceptable
            pass

    def test_duplicate_function_names(self):
        """Test source with duplicate function names."""
        source = """
def foo():
    pass

def foo():  # Redefined
    pass

class Bar:
    def foo(self):  # Different scope
        pass
"""
        anchor = Anchor(type=AnchorType.FUNCTION, name="foo")
        matches = find_anchors(source, anchor)
        # Should find multiple matches
        assert len(matches) >= 2

    def test_empty_source(self):
        """Test anchor resolution on empty source."""
        anchor = Anchor(type=AnchorType.FUNCTION, name="foo")
        matches = find_anchors("", anchor)
        assert matches == []

    def test_syntax_error_source(self):
        """Test anchor resolution on invalid source."""
        anchor = Anchor(type=AnchorType.FUNCTION, name="foo")

        # Invalid Python
        try:
            matches = find_anchors("def foo(", anchor)
            assert isinstance(matches, list)
        except SyntaxError:
            pass  # Acceptable


class TestPatchFuzzing:
    """Fuzzing tests for patch application."""

    def test_patch_empty_content(self):
        """Test patching with empty content."""
        source = "def foo(): pass"
        anchor = Anchor(type=AnchorType.FUNCTION, name="foo")
        patch = Patch(anchor=anchor, patch_type=PatchType.DELETE)

        result = apply_patch(source, patch)
        # Should handle deletion
        assert isinstance(result.patched, str)

    def test_patch_nonexistent_anchor(self):
        """Test patching nonexistent anchor."""
        source = "def bar(): pass"
        anchor = Anchor(type=AnchorType.FUNCTION, name="foo")
        patch = Patch(
            anchor=anchor,
            patch_type=PatchType.REPLACE,
            content="def foo(): return 1",
        )

        result = apply_patch(source, patch)
        assert not result.success
        assert result.error is not None

    def test_text_patch_edge_cases(self):
        """Test text-based patching edge cases."""
        source = "hello world"

        # Replace with nothing
        result = apply_text_patch(source, "hello", "")
        assert result.success
        assert result.patched == " world"

        # Replace nothing
        result = apply_text_patch(source, "nonexistent", "replacement")
        assert not result.success

        # Empty source
        result = apply_text_patch("", "foo", "bar")
        assert not result.success

    def test_patch_preserves_structure(self):
        """Test that patch preserves surrounding code."""
        source = """
def before():
    pass

def target():
    return 1

def after():
    pass
"""
        anchor = Anchor(type=AnchorType.FUNCTION, name="target")
        patch = Patch(
            anchor=anchor,
            patch_type=PatchType.REPLACE,
            content="def target():\n    return 2",
        )

        result = apply_patch(source, patch)
        if result.success:
            assert "def before" in result.patched
            assert "def after" in result.patched
            assert "return 2" in result.patched


class TestCFGFuzzing:
    """Fuzzing tests for Control Flow Graph building."""

    @pytest.mark.parametrize(
        "source",
        [
            "",  # Empty
            "pass",  # No function
            "x = 1",  # No function
            "def foo(): ...",  # Ellipsis body
            "async def foo(): pass",  # Async
            "lambda: None",  # Lambda (not a function def)
        ],
    )
    def test_minimal_sources(self, source):
        """Test CFG building with minimal sources."""
        cfgs = build_cfg(source)
        assert isinstance(cfgs, list)

    def test_complex_control_flow(self):
        """Test complex nested control flow."""
        source = """
def complex():
    for i in range(10):
        if i % 2 == 0:
            for j in range(i):
                if j > 5:
                    break
                elif j == 3:
                    continue
                else:
                    try:
                        x = 1 / j
                    except ZeroDivisionError:
                        pass
                    finally:
                        y = 0
        else:
            while True:
                if i > 100:
                    return i
                i += 1
    return 0
"""
        cfgs = build_cfg(source)
        assert len(cfgs) == 1
        cfg = cfgs[0]
        # Should have many nodes due to complexity
        assert cfg.node_count > 5

    def test_many_branches(self):
        """Test function with many branches."""
        conditions = "\n".join([f"    if x == {i}: return {i}" for i in range(50)])
        source = f"def many_branches(x):\n{conditions}\n    return -1"

        cfgs = build_cfg(source)
        assert len(cfgs) == 1

    def test_deeply_nested_try(self):
        """Test deeply nested try-except blocks."""
        nesting = 10
        source = "def nested_try():\n"
        for i in range(nesting):
            indent = "    " * (i + 1)
            source += f"{indent}try:\n"
        source += "    " * (nesting + 1) + "x = 1\n"
        for i in range(nesting - 1, -1, -1):
            indent = "    " * (i + 1)
            source += f"{indent}except:\n{indent}    pass\n"

        cfgs = build_cfg(source)
        assert len(cfgs) == 1


class TestElidedLiteralsFuzzing:
    """Fuzzing tests for literal elision."""

    @pytest.mark.parametrize(
        "source",
        [
            "",  # Empty
            "   ",  # Whitespace
            "# comment",  # Comment only
            "pass",  # No literals
        ],
    )
    def test_minimal_sources(self, source):
        """Test elision on minimal sources."""
        elided, stats = elide_literals(source)
        assert isinstance(elided, str)
        assert stats.total >= 0

    def test_many_string_literals(self):
        """Test source with many string literals."""
        strings = [f's{i} = "string_{i}"' for i in range(100)]
        source = "\n".join(strings)

        _elided, stats = elide_literals(source)
        assert stats.strings >= 50  # At least half should be elided

    def test_mixed_literal_types(self):
        """Test source with all literal types."""
        source = """
s = "string"
n = 12345
f = 3.14159
c = 1 + 2j
b = b"bytes"
l = [1, 2, 3]
d = {"key": "value"}
t = (1, 2, 3, 4, 5)
fs = f"formatted {s}"
"""
        _elided, stats = elide_literals(source)
        assert stats.total > 0

    def test_preserves_docstrings(self):
        """Test that docstrings are preserved."""
        source = '''
"""Module docstring."""

def foo():
    """Function docstring."""
    return "not a docstring"

class Bar:
    """Class docstring."""
    pass
'''
        elided, _stats = elide_literals(source)
        # Docstrings should be preserved
        assert "Module docstring" in elided or "docstring" in elided.lower()

    def test_unicode_strings(self):
        """Test Unicode string handling."""
        source = """
s1 = "Hello, 世界"
s2 = "مرحبا"
s3 = "🎉🎊🎈"
"""
        elided, _stats = elide_literals(source)
        assert isinstance(elided, str)

    def test_raw_and_formatted_strings(self):
        """Test raw and formatted strings."""
        source = """
r = r"raw\\nstring"
f = f"formatted {value}"
rf = rf"raw formatted {value}"
fr = fr"also raw formatted {value}"
"""
        elided, _stats = elide_literals(source)
        assert isinstance(elided, str)


class TestInputBoundaries:
    """Tests for input boundary conditions."""

    def test_max_recursion_depth(self):
        """Test that deeply recursive structures don't cause stack overflow."""
        # Create deeply nested expression
        depth = 100
        expr = "x"
        for _ in range(depth):
            expr = f"({expr})"

        source = f"result = {expr}"

        # Should not crash due to recursion
        symbols = extract_python_skeleton(source)
        assert isinstance(symbols, list)

    def test_large_source_file(self):
        """Test handling of large source files."""
        # Generate a moderately large source
        classes = []
        for i in range(50):
            methods = "\n".join([f"    def method_{j}(self): return {j}" for j in range(10)])
            classes.append(f"class Class{i}:\n{methods}")

        source = "\n\n".join(classes)

        symbols = extract_python_skeleton(source)
        assert len(symbols) == 50

    def test_binary_content(self):
        """Test handling of non-UTF8 content."""
        # This would typically be caught before skeleton extraction,
        # but test graceful handling
        try:
            symbols = extract_python_skeleton("\x00\x01\x02\x03")
            assert isinstance(symbols, list)
        except Exception:
            # Expected to fail gracefully
            pass

    def test_mixed_indentation(self):
        """Test handling of mixed tabs/spaces."""
        source = "def foo():\n\treturn 1\n    x = 2"  # Mixed indentation

        # Should handle without crashing (may fail to parse)
        try:
            symbols = extract_python_skeleton(source)
            assert isinstance(symbols, list)
        except Exception:
            pass

    def test_unusual_decorators(self):
        """Test unusual decorator patterns."""
        source = """
@decorator1
@decorator2(arg1, arg2)
@decorator3(
    multiline,
    argument,
)
@module.submodule.decorator
def decorated():
    pass
"""
        symbols = extract_python_skeleton(source)
        assert len(symbols) == 1
        assert symbols[0].name == "decorated"
