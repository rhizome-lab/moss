"""Tests for GBNF grammar support."""

from moss.llm.gbnf import (
    GRAMMARS,
    GBNFBuilder,
    enum_grammar,
    json_schema_to_gbnf,
    validate_grammar,
)


class TestGrammars:
    """Tests for predefined grammars."""

    def test_grammars_are_defined(self):
        """Test that all expected grammars are defined."""
        assert "json" in GRAMMARS
        assert "json_object" in GRAMMARS
        assert "json_array" in GRAMMARS
        assert "boolean" in GRAMMARS
        assert "integer" in GRAMMARS
        assert "identifier" in GRAMMARS

    def test_json_grammar_has_root(self):
        """Test that JSON grammar has root rule."""
        assert "root" in GRAMMARS["json"]
        assert "::=" in GRAMMARS["json"]

    def test_grammars_are_strings(self):
        """Test that all grammars are strings."""
        for name, grammar in GRAMMARS.items():
            assert isinstance(grammar, str), f"Grammar {name} is not a string"


class TestEnumGrammar:
    """Tests for enum grammar builder."""

    def test_single_value(self):
        grammar = enum_grammar(["yes"])
        assert 'root ::= "yes"' in grammar

    def test_multiple_values(self):
        grammar = enum_grammar(["yes", "no", "maybe"])
        assert '"yes"' in grammar
        assert '"no"' in grammar
        assert '"maybe"' in grammar
        assert "|" in grammar


class TestGBNFBuilder:
    """Tests for GBNF builder."""

    def test_add_rule(self):
        builder = GBNFBuilder()
        name = builder.add_rule("test", '"hello"')
        assert name == "test"
        assert "test" in builder.rules

    def test_build(self):
        builder = GBNFBuilder()
        builder.add_rule("root", '"test"')
        grammar = builder.build()
        assert 'root ::= "test"' in grammar

    def test_from_json_schema_string(self):
        builder = GBNFBuilder()
        name = builder.from_json_schema({"type": "string"}, "test")
        assert name == "test"

    def test_from_json_schema_integer(self):
        builder = GBNFBuilder()
        name = builder.from_json_schema({"type": "integer"}, "test")
        assert "integer" in builder.rules.get(name, "")

    def test_from_json_schema_enum(self):
        builder = GBNFBuilder()
        name = builder.from_json_schema({"type": "string", "enum": ["a", "b", "c"]}, "choice")
        defn = builder.rules.get(name, "")
        assert '"a"' in defn
        assert '"b"' in defn
        assert '"c"' in defn


class TestJsonSchemaToGbnf:
    """Tests for JSON schema conversion."""

    def test_simple_object(self):
        schema = {
            "type": "object",
            "properties": {"name": {"type": "string"}},
        }
        grammar = json_schema_to_gbnf(schema)
        assert "root" in grammar
        assert "ws" in grammar

    def test_with_enum(self):
        schema = {
            "type": "object",
            "properties": {"status": {"type": "string", "enum": ["active", "inactive"]}},
        }
        grammar = json_schema_to_gbnf(schema)
        assert '"active"' in grammar or "active" in grammar

    def test_with_array(self):
        schema = {
            "type": "array",
            "items": {"type": "string"},
        }
        grammar = json_schema_to_gbnf(schema)
        assert "root" in grammar


class TestValidateGrammar:
    """Tests for grammar validation."""

    def test_valid_grammar(self):
        grammar = 'root ::= "hello"'
        errors = validate_grammar(grammar)
        assert len(errors) == 0

    def test_missing_separator(self):
        grammar = "root = hello"
        errors = validate_grammar(grammar)
        assert len(errors) > 0
        assert any("::=" in e for e in errors)

    def test_empty_rule_name(self):
        grammar = '::= "hello"'
        errors = validate_grammar(grammar)
        assert len(errors) > 0

    def test_multiple_rules(self):
        grammar = """
root ::= value
value ::= "hello" | "world"
"""
        errors = validate_grammar(grammar)
        assert len(errors) == 0
