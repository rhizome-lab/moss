"""GBNF Grammar Support for Constrained LLM Inference.

GBNF (GGML BNF) is a grammar format used by llama.cpp to constrain LLM outputs
to match a specific syntax. This module provides:

1. Grammar definitions for common output formats
2. Utilities for building custom grammars
3. JSON schema to GBNF conversion

Example:
    from moss.llm.gbnf import GRAMMARS, json_schema_to_gbnf

    # Use predefined grammar
    grammar = GRAMMARS["json"]

    # Or convert from JSON schema
    grammar = json_schema_to_gbnf({
        "type": "object",
        "properties": {"name": {"type": "string"}}
    })
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

# =============================================================================
# Core GBNF Grammar Definitions
# =============================================================================

# Basic JSON grammar - constrains output to valid JSON
JSON_GRAMMAR = r"""
root   ::= value
value  ::= object | array | string | number | ("true" | "false" | "null")
object ::= "{" ws (pair ("," ws pair)*)? ws "}"
pair   ::= string ws ":" ws value
array  ::= "[" ws (value ("," ws value)*)? ws "]"
string ::= "\"" ([^"\\] | "\\" .)* "\""
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)?
ws     ::= [ \t\n\r]*
"""

# JSON object only (no arrays or primitives at root)
JSON_OBJECT_GRAMMAR = r"""
root   ::= object
object ::= "{" ws (pair ("," ws pair)*)? ws "}"
pair   ::= string ws ":" ws value
value  ::= object | array | string | number | ("true" | "false" | "null")
array  ::= "[" ws (value ("," ws value)*)? ws "]"
string ::= "\"" ([^"\\] | "\\" .)* "\""
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)?
ws     ::= [ \t\n\r]*
"""

# JSON array only
JSON_ARRAY_GRAMMAR = r"""
root   ::= array
array  ::= "[" ws (value ("," ws value)*)? ws "]"
value  ::= object | array | string | number | ("true" | "false" | "null")
object ::= "{" ws (pair ("," ws pair)*)? ws "}"
pair   ::= string ws ":" ws value
string ::= "\"" ([^"\\] | "\\" .)* "\""
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)?
ws     ::= [ \t\n\r]*
"""

# Boolean only (for yes/no questions)
BOOLEAN_GRAMMAR = r"""
root ::= "true" | "false"
"""

# Integer only
INTEGER_GRAMMAR = r"""
root ::= "-"? ("0" | [1-9] [0-9]*)
"""

# Identifier (Python/programming variable names)
IDENTIFIER_GRAMMAR = r"""
root ::= [a-zA-Z_] [a-zA-Z0-9_]*
"""

# Comma-separated list of identifiers
IDENTIFIER_LIST_GRAMMAR = r"""
root   ::= ident ("," ws ident)*
ident  ::= [a-zA-Z_] [a-zA-Z0-9_]*
ws     ::= [ \t]*
"""

# Edit plan format (for architect_editor.py)
EDIT_PLAN_GRAMMAR = r"""
root       ::= "{" ws pairs ws "}"
pairs      ::= pair ("," ws pair)*
pair       ::= key ws ":" ws value
key        ::= "\"task\"" | "\"file_path\"" | "\"approach\"" | "\"steps\"" | "\"risks\""
value      ::= string | array
string     ::= "\"" [^"]* "\""
array      ::= "[" ws (string ("," ws string)*)? ws "]"
ws         ::= [ \t\n\r]*
"""

# Docstring output format (for agent_loop.py FUNC: format)
DOCSTRING_GRAMMAR = r"""
root   ::= (func ws)*
func   ::= "FUNC:" ident "|" desc "\n"
ident  ::= [a-zA-Z_] [a-zA-Z0-9_]*
desc   ::= [^\n]*
ws     ::= [ \t]*
"""

# Yes/No/Maybe responses
YESNO_GRAMMAR = r"""
root ::= "yes" | "no" | "maybe"
"""


# Enum grammar builder
def enum_grammar(values: list[str]) -> str:
    """Build grammar that matches exactly one of the given values."""
    quoted = [f'"{v}"' for v in values]
    return f"root ::= {' | '.join(quoted)}"


# =============================================================================
# Grammar Registry
# =============================================================================

GRAMMARS: dict[str, str] = {
    "json": JSON_GRAMMAR,
    "json_object": JSON_OBJECT_GRAMMAR,
    "json_array": JSON_ARRAY_GRAMMAR,
    "boolean": BOOLEAN_GRAMMAR,
    "integer": INTEGER_GRAMMAR,
    "identifier": IDENTIFIER_GRAMMAR,
    "identifier_list": IDENTIFIER_LIST_GRAMMAR,
    "edit_plan": EDIT_PLAN_GRAMMAR,
    "docstring": DOCSTRING_GRAMMAR,
    "yesno": YESNO_GRAMMAR,
}


# =============================================================================
# JSON Schema to GBNF Conversion
# =============================================================================


@dataclass
class GBNFBuilder:
    """Builder for constructing GBNF grammars from JSON schemas."""

    rules: dict[str, str]
    _counter: int = 0

    def __init__(self) -> None:
        self.rules = {}
        self._counter = 0

    def _unique_name(self, prefix: str = "rule") -> str:
        """Generate unique rule name."""
        self._counter += 1
        return f"{prefix}{self._counter}"

    def add_rule(self, name: str, definition: str) -> str:
        """Add a rule and return its name."""
        self.rules[name] = definition
        return name

    def build(self) -> str:
        """Build final grammar string."""
        lines = [f"{name} ::= {defn}" for name, defn in self.rules.items()]
        return "\n".join(lines)

    def from_json_schema(self, schema: dict[str, Any], name: str = "root") -> str:
        """Convert JSON schema to GBNF rule."""
        schema_type = schema.get("type")

        if schema_type == "object":
            return self._object_rule(schema, name)
        elif schema_type == "array":
            return self._array_rule(schema, name)
        elif schema_type == "string":
            if "enum" in schema:
                values = " | ".join(f'"{v}"' for v in schema["enum"])
                return self.add_rule(name, values)
            return self.add_rule(name, "string")
        elif schema_type == "integer":
            return self.add_rule(name, "integer")
        elif schema_type == "number":
            return self.add_rule(name, "number")
        elif schema_type == "boolean":
            return self.add_rule(name, '("true" | "false")')
        elif schema_type == "null":
            return self.add_rule(name, '"null"')
        elif "oneOf" in schema or "anyOf" in schema:
            options = schema.get("oneOf") or schema.get("anyOf")
            sub_rules = []
            for i, opt in enumerate(options):
                sub_name = self.from_json_schema(opt, f"{name}_opt{i}")
                sub_rules.append(sub_name)
            return self.add_rule(name, " | ".join(sub_rules))
        else:
            # Fallback to generic value
            return self.add_rule(name, "value")

    def _object_rule(self, schema: dict[str, Any], name: str) -> str:
        """Build rule for JSON object."""
        props = schema.get("properties", {})
        required = set(schema.get("required", []))

        if not props:
            return self.add_rule(name, "object")

        pairs = []
        for prop_name, prop_schema in props.items():
            prop_rule = self.from_json_schema(prop_schema, f"{name}_{prop_name}")
            pair = f'""{prop_name}"" ws ":" ws {prop_rule}'
            if prop_name not in required:
                pair = f"({pair})?"
            pairs.append(pair)

        # Simplified: require all properties in order
        defn = '"{" ws ' + ' ws "," ws '.join(pairs) + ' ws "}"'
        return self.add_rule(name, defn)

    def _array_rule(self, schema: dict[str, Any], name: str) -> str:
        """Build rule for JSON array."""
        items = schema.get("items", {})
        item_rule = self.from_json_schema(items, f"{name}_item")
        defn = f'"[" ws ({item_rule} ("," ws {item_rule})*)? ws "]"'
        return self.add_rule(name, defn)


def json_schema_to_gbnf(schema: dict[str, Any]) -> str:
    """Convert a JSON schema to GBNF grammar.

    Args:
        schema: JSON Schema dictionary

    Returns:
        GBNF grammar string

    Example:
        schema = {
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name"]
        }
        grammar = json_schema_to_gbnf(schema)
    """
    builder = GBNFBuilder()

    # Add common primitives
    builder.add_rule("ws", "[ \\t\\n\\r]*")
    builder.add_rule("string", '"\\"" [^"\\\\]* "\\"" ')
    builder.add_rule("integer", '"-"? ("0" | [1-9] [0-9]*)')
    builder.add_rule("number", 'integer ("." [0-9]+)? ([eE] [+-]? [0-9]+)?')
    builder.add_rule("object", '"{" ws (pair ("," ws pair)*)? ws "}"')
    builder.add_rule("pair", 'string ws ":" ws value')
    builder.add_rule("array", '"[" ws (value ("," ws value)*)? ws "]"')
    builder.add_rule("value", 'object | array | string | number | ("true" | "false" | "null")')

    # Build schema-specific rules
    builder.from_json_schema(schema)

    return builder.build()


# =============================================================================
# Grammar Validation
# =============================================================================


def validate_grammar(grammar: str) -> list[str]:
    """Validate GBNF grammar syntax.

    Returns list of errors (empty if valid).
    """
    errors = []
    lines = grammar.strip().split("\n")

    defined_rules: set[str] = set()
    referenced_rules: set[str] = set()

    for i, line in enumerate(lines, 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue

        if "::=" not in line:
            errors.append(f"Line {i}: Missing '::=' in rule definition")
            continue

        parts = line.split("::=", 1)
        rule_name = parts[0].strip()
        rule_body = parts[1].strip() if len(parts) > 1 else ""

        if not rule_name:
            errors.append(f"Line {i}: Empty rule name")
            continue

        defined_rules.add(rule_name)

        # Extract referenced rule names (simple heuristic)
        import re

        refs = re.findall(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\b", rule_body)
        for ref in refs:
            if ref not in ("ws", "true", "false", "null"):
                referenced_rules.add(ref)

    # Check for undefined rules
    undefined = referenced_rules - defined_rules
    # Filter out common terminals
    undefined = {r for r in undefined if r not in ("ws", "string", "number", "integer", "value")}

    if undefined and "root" not in defined_rules:
        errors.append("Missing 'root' rule (entry point)")

    return errors
