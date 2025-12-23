"""Serialization utilities for API result types.

This module provides serialization of MossAPI result types to JSON-compatible
dictionaries. It handles dataclasses, enums, and special types like AST nodes.

Used by HTTP, MCP, and other interface generators to produce consistent output.
"""

from __future__ import annotations

import ast
import dataclasses
from enum import Enum
from pathlib import Path
from typing import Any


def serialize(obj: Any) -> Any:
    """Serialize an object to a JSON-compatible form.

    Handles:
    - Dataclasses: recursively serialized to dicts
    - Enums: converted to their name (string)
    - Paths: converted to strings
    - AST nodes: converted to their type name (not full representation)
    - Lists/tuples: recursively serialized
    - Dicts: recursively serialized (values only)
    - Primitives: passed through unchanged

    Args:
        obj: Object to serialize

    Returns:
        JSON-compatible representation (dict, list, str, int, float, bool, None)
    """
    if obj is None:
        return None

    # Primitives pass through
    if isinstance(obj, (str, int, float, bool)):
        return obj

    # Enums -> name string
    if isinstance(obj, Enum):
        return obj.name

    # Paths -> string
    if isinstance(obj, Path):
        return str(obj)

    # AST nodes -> type name (these aren't serializable in full)
    if isinstance(obj, ast.AST):
        return f"<{obj.__class__.__name__}>"

    # Dataclasses -> dict with recursive serialization
    if dataclasses.is_dataclass(obj) and not isinstance(obj, type):
        return serialize_dataclass(obj)

    # Lists and tuples -> recursively serialize items
    if isinstance(obj, (list, tuple)):
        return [serialize(item) for item in obj]

    # Dicts -> recursively serialize values
    if isinstance(obj, dict):
        return {k: serialize(v) for k, v in obj.items()}

    # Sets -> convert to sorted list
    if isinstance(obj, (set, frozenset)):
        return sorted(serialize(item) for item in obj)

    # Fallback: try str()
    return str(obj)


def serialize_dataclass(obj: Any) -> dict[str, Any]:
    """Serialize a dataclass instance to a dict.

    Recursively serializes all fields, handling nested dataclasses,
    enums, and other special types.

    Args:
        obj: Dataclass instance to serialize

    Returns:
        Dict with all fields serialized
    """
    if not dataclasses.is_dataclass(obj) or isinstance(obj, type):
        raise TypeError(f"Expected dataclass instance, got {type(obj)}")

    result: dict[str, Any] = {}
    for field in dataclasses.fields(obj):
        value = getattr(obj, field.name)
        result[field.name] = serialize(value)

    return result


def serialize_with_renames(obj: Any, renames: dict[str, str]) -> dict[str, Any]:
    """Serialize a dataclass with field name remapping.

    Useful when the API output format differs from internal field names.

    Args:
        obj: Dataclass instance to serialize
        renames: Mapping of {internal_name: output_name}

    Returns:
        Dict with renamed fields

    Example:
        >>> serialize_with_renames(symbol, {"lineno": "line", "end_lineno": "end_line"})
        {"name": "foo", "line": 10, "end_line": 20, ...}
    """
    base = serialize_dataclass(obj)
    result = {}
    for key, value in base.items():
        output_key = renames.get(key, key)
        result[output_key] = value
    return result


# Common rename mappings for API consistency
SYMBOL_RENAMES = {
    "lineno": "line",
    "end_lineno": "end_line",
}

ANCHOR_MATCH_RENAMES = {
    "lineno": "line",
    "end_lineno": "end_line",
    "col_offset": "column",
    "end_col_offset": "end_column",
    "score": "confidence",
}


def serialize_symbol(symbol: Any) -> dict[str, Any]:
    """Serialize a Symbol with consistent field names.

    Args:
        symbol: Symbol instance

    Returns:
        Dict with API-friendly field names
    """
    result = serialize_with_renames(symbol, SYMBOL_RENAMES)
    # Recursively handle children
    if result.get("children"):
        result["children"] = [serialize_symbol(c) for c in symbol.children]
    return result


def serialize_anchor_match(match: Any) -> dict[str, Any]:
    """Serialize an AnchorMatch with consistent field names.

    Excludes the AST node (not JSON-serializable) and anchor object.

    Args:
        match: AnchorMatch instance

    Returns:
        Dict with API-friendly field names
    """
    # Manual serialization to exclude non-serializable fields
    return {
        "name": match.anchor.name,
        "type": match.anchor.type.name,
        "line": match.lineno,
        "end_line": match.end_lineno,
        "column": match.col_offset,
        "end_column": match.end_col_offset,
        "confidence": match.score,
        "context": match.context_chain,
    }


__all__ = [
    "ANCHOR_MATCH_RENAMES",
    "SYMBOL_RENAMES",
    "serialize",
    "serialize_anchor_match",
    "serialize_dataclass",
    "serialize_symbol",
    "serialize_with_renames",
]
