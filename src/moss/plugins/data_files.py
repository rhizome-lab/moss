"""Data file schema extraction plugins.

Extracts structural information from data files:
- JSON schema structure
- YAML schema structure
- TOML configuration structure
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from moss.plugins import PluginMetadata
    from moss.views import View, ViewOptions, ViewTarget


@dataclass
class SchemaNode:
    """A node in the schema tree."""

    name: str
    value_type: str  # object, array, string, number, boolean, null
    path: str  # JSON path like "root.foo.bar"
    children: list[SchemaNode] = field(default_factory=list)
    array_item_type: str | None = None  # For arrays, the type of items
    sample_value: Any = None  # Sample value for leaf nodes


def infer_schema(data: Any, path: str = "root", name: str = "root") -> SchemaNode:
    """Infer schema structure from data.

    Args:
        data: The data to analyze
        path: Current JSON path
        name: Current node name

    Returns:
        SchemaNode representing the structure
    """
    if data is None:
        return SchemaNode(name=name, value_type="null", path=path)

    if isinstance(data, bool):
        return SchemaNode(name=name, value_type="boolean", path=path, sample_value=data)

    if isinstance(data, int | float):
        return SchemaNode(name=name, value_type="number", path=path, sample_value=data)

    if isinstance(data, str):
        # Truncate long strings
        sample = data[:50] + "..." if len(data) > 50 else data
        return SchemaNode(name=name, value_type="string", path=path, sample_value=sample)

    if isinstance(data, list):
        node = SchemaNode(name=name, value_type="array", path=path)
        if data:
            # Infer type from first item
            first_item = data[0]
            item_schema = infer_schema(first_item, f"{path}[]", "[item]")
            node.array_item_type = item_schema.value_type
            if item_schema.value_type == "object":
                node.children = item_schema.children
        return node

    if isinstance(data, dict):
        node = SchemaNode(name=name, value_type="object", path=path)
        for key, value in data.items():
            child_path = f"{path}.{key}"
            child = infer_schema(value, child_path, key)
            node.children.append(child)
        return node

    return SchemaNode(name=name, value_type="unknown", path=path)


def format_schema(node: SchemaNode, indent: int = 0) -> str:
    """Format schema as readable text.

    Args:
        node: Schema node to format
        indent: Current indentation level

    Returns:
        Formatted string representation
    """
    lines = []
    prefix = "  " * indent

    type_str = node.value_type
    if node.array_item_type:
        type_str = f"array<{node.array_item_type}>"

    if node.sample_value is not None and node.value_type not in ("object", "array"):
        sample = repr(node.sample_value)
        if len(sample) > 30:
            sample = sample[:30] + "..."
        lines.append(f"{prefix}{node.name}: {type_str} = {sample}")
    else:
        lines.append(f"{prefix}{node.name}: {type_str}")

    for child in node.children:
        lines.append(format_schema(child, indent + 1))

    return "\n".join(lines)


def schema_to_dict(node: SchemaNode) -> dict:
    """Convert schema node to dictionary for metadata.

    Args:
        node: Schema node to convert

    Returns:
        Dictionary representation
    """
    result = {
        "name": node.name,
        "type": node.value_type,
        "path": node.path,
    }
    if node.array_item_type:
        result["item_type"] = node.array_item_type
    if node.children:
        result["children"] = [schema_to_dict(c) for c in node.children]
    return result


class JSONSchemaPlugin:
    """Plugin for extracting schema from JSON files."""

    @property
    def metadata(self) -> PluginMetadata:
        from moss.plugins import PluginMetadata

        return PluginMetadata(
            name="json-schema",
            view_type="skeleton",
            languages=frozenset(["json"]),
            priority=5,
            version="0.1.0",
            description="JSON schema extraction",
        )

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the target."""
        from moss.plugins import detect_language

        if not target.path.exists():
            return False
        return detect_language(target.path) == "json"

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a schema view for a JSON file."""
        from moss.views import View, ViewType

        source = target.path.read_text()

        try:
            data = json.loads(source)
        except json.JSONDecodeError as e:
            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content=f"# JSON parse error: {e}",
                metadata={"error": str(e)},
            )

        schema = infer_schema(data)
        content = format_schema(schema)

        return View(
            target=target,
            view_type=ViewType.SKELETON,
            content=content,
            metadata={
                "schema": schema_to_dict(schema),
                "root_type": schema.value_type,
                "language": "json",
            },
        )


class YAMLSchemaPlugin:
    """Plugin for extracting schema from YAML files."""

    @property
    def metadata(self) -> PluginMetadata:
        from moss.plugins import PluginMetadata

        return PluginMetadata(
            name="yaml-schema",
            view_type="skeleton",
            languages=frozenset(["yaml"]),
            priority=5,
            version="0.1.0",
            description="YAML schema extraction",
        )

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the target."""
        from moss.plugins import detect_language

        if not target.path.exists():
            return False
        return detect_language(target.path) == "yaml"

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a schema view for a YAML file."""
        from moss.views import View, ViewType

        source = target.path.read_text()

        try:
            import yaml

            data = yaml.safe_load(source)
        except ImportError:
            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content="# PyYAML not installed",
                metadata={"error": "PyYAML not installed"},
            )
        except yaml.YAMLError as e:
            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content=f"# YAML parse error: {e}",
                metadata={"error": str(e)},
            )

        schema = infer_schema(data)
        content = format_schema(schema)

        return View(
            target=target,
            view_type=ViewType.SKELETON,
            content=content,
            metadata={
                "schema": schema_to_dict(schema),
                "root_type": schema.value_type,
                "language": "yaml",
            },
        )


class TOMLSchemaPlugin:
    """Plugin for extracting schema from TOML files."""

    @property
    def metadata(self) -> PluginMetadata:
        from moss.plugins import PluginMetadata

        return PluginMetadata(
            name="toml-schema",
            view_type="skeleton",
            languages=frozenset(["toml"]),
            priority=5,
            version="0.1.0",
            description="TOML schema extraction",
        )

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the target."""
        from moss.plugins import detect_language

        if not target.path.exists():
            return False
        return detect_language(target.path) == "toml"

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a schema view for a TOML file."""
        from moss.views import View, ViewType

        source = target.path.read_text()

        try:
            # Python 3.11+ has tomllib in stdlib
            import tomllib

            data = tomllib.loads(source)
        except ImportError:
            try:
                import tomli as tomllib

                data = tomllib.loads(source)
            except ImportError:
                return View(
                    target=target,
                    view_type=ViewType.SKELETON,
                    content="# TOML parser not available (need Python 3.11+ or tomli)",
                    metadata={"error": "TOML parser not available"},
                )
        except tomllib.TOMLDecodeError as e:
            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content=f"# TOML parse error: {e}",
                metadata={"error": str(e)},
            )

        schema = infer_schema(data)
        content = format_schema(schema)

        return View(
            target=target,
            view_type=ViewType.SKELETON,
            content=content,
            metadata={
                "schema": schema_to_dict(schema),
                "root_type": schema.value_type,
                "language": "toml",
            },
        )
