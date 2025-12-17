"""Tests for Elided Literals module."""

from moss.elided_literals import (
    ElidedLiteralsProvider,
    ElisionConfig,
    ElisionStats,
    elide_literals,
    elide_literals_regex,
)


class TestElisionStats:
    """Tests for ElisionStats."""

    def test_default_stats(self):
        stats = ElisionStats()
        assert stats.total == 0

    def test_total(self):
        stats = ElisionStats(strings=5, numbers=3, lists=2)
        assert stats.total == 10


class TestElisionConfig:
    """Tests for ElisionConfig."""

    def test_default_config(self):
        config = ElisionConfig()
        assert config.elide_strings is True
        assert config.elide_numbers is True
        assert config.preserve_docstrings is True

    def test_custom_config(self):
        config = ElisionConfig(elide_strings=False, elide_numbers=True)
        assert config.elide_strings is False
        assert config.elide_numbers is True


class TestElideLiterals:
    """Tests for elide_literals function."""

    def test_elide_string(self):
        source = 'x = "hello world"'
        elided, stats = elide_literals(source)

        assert '"..."' in elided or "'...'" in elided
        assert stats.strings >= 1

    def test_preserve_empty_string(self):
        source = 'x = ""'
        config = ElisionConfig(preserve_empty_strings=True)
        elided, _stats = elide_literals(source, config)

        assert '""' in elided or "''" in elided

    def test_preserve_single_char_string(self):
        source = 'x = "a"'
        config = ElisionConfig(preserve_single_char_strings=True)
        elided, _stats = elide_literals(source, config)

        assert '"a"' in elided or "'a'" in elided

    def test_elide_number(self):
        source = "x = 12345"
        elided, stats = elide_literals(source)

        assert "12345" not in elided
        assert stats.numbers >= 1

    def test_preserve_small_int(self):
        source = "x = 5"
        config = ElisionConfig(preserve_small_ints=True)
        elided, _stats = elide_literals(source, config)

        assert "5" in elided

    def test_preserve_zero_one(self):
        source = "x = 0; y = 1"
        config = ElisionConfig(preserve_zero_one=True)
        elided, _stats = elide_literals(source, config)

        assert "0" in elided
        assert "1" in elided

    def test_elide_list(self):
        source = "x = [1, 2, 3, 4, 5]"
        elided, _stats = elide_literals(source)

        # Should keep first element hint
        assert "[" in elided
        assert "...]" in elided or "..." in elided

    def test_preserve_empty_list(self):
        source = "x = []"
        elided, _stats = elide_literals(source)

        assert "[]" in elided

    def test_elide_dict(self):
        source = 'x = {"a": 1, "b": 2, "c": 3}'
        elided, _stats = elide_literals(source)

        assert "{" in elided
        assert "..." in elided

    def test_preserve_empty_dict(self):
        source = "x = {}"
        elided, _stats = elide_literals(source)

        assert "{}" in elided

    def test_elide_f_string(self):
        source = 'x = f"Hello {name}!"'
        _elided, stats = elide_literals(source)

        assert stats.f_strings >= 1

    def test_preserve_docstring(self):
        source = '''
def foo():
    """This is a docstring."""
    pass
'''
        config = ElisionConfig(preserve_docstrings=True)
        elided, _stats = elide_literals(source, config)

        # Docstring should be preserved
        if '"""' in elided:
            assert "docstring" in elided.lower() or "..." not in elided.split('"""')[1]

    def test_preserve_type_annotation(self):
        source = "x: int = 5"
        config = ElisionConfig(preserve_type_annotations=True)
        elided, _stats = elide_literals(source, config)

        assert "int" in elided

    def test_multiple_statements(self):
        source = """
x = "hello"
y = 12345
z = [1, 2, 3]
"""
        _elided, stats = elide_literals(source)

        assert stats.strings >= 1
        assert stats.numbers >= 1
        assert stats.lists >= 1

    def test_complex_code(self):
        source = '''
def process(data):
    """Process the data."""
    result = []
    for item in data:
        if item["value"] > 100:
            result.append({"processed": True, "item": item})
    return result
'''
        elided, stats = elide_literals(source)

        # Should have some elisions but preserve structure
        assert "def process" in elided
        assert "return result" in elided
        assert stats.total >= 1

    def test_invalid_syntax_returns_original(self):
        source = "def foo( # incomplete"
        elided, stats = elide_literals(source)

        assert elided == source
        assert stats.total == 0


class TestElideLiteralsRegex:
    """Tests for elide_literals_regex function."""

    def test_elide_double_quoted_string(self):
        source = 'x = "hello world"'
        elided = elide_literals_regex(source)

        assert '"..."' in elided

    def test_elide_single_quoted_string(self):
        source = "x = 'hello world'"
        elided = elide_literals_regex(source)

        assert "'...'" in elided

    def test_elide_triple_quoted_string(self):
        source = 'x = """multi\nline"""'
        elided = elide_literals_regex(source)

        # Should have elided the content
        assert "multi" not in elided or "..." in elided

    def test_elide_number(self):
        source = "x = 12345"
        elided = elide_literals_regex(source)

        assert "12345" not in elided


class TestElidedLiteralsProvider:
    """Tests for ElidedLiteralsProvider."""

    def test_elide_directly(self):
        source = """
def foo():
    x = "hello world"
    y = 12345
    return x
"""
        provider = ElidedLiteralsProvider()
        elided, stats = elide_literals(source, provider.config)

        assert "def foo" in elided
        assert "return x" in elided
        assert stats.total >= 1

    def test_custom_config(self):
        source = 'x = "hello"'
        config = ElisionConfig(elide_strings=False)

        elided, _stats = elide_literals(source, config)

        # String should NOT be elided
        assert "hello" in elided
