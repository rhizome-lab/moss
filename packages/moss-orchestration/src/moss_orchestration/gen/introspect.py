"""API introspection for interface generation.

This module provides tools to introspect moss-intelligence modules and extract
metadata about their structure, functions, and parameters.
"""

from __future__ import annotations

import importlib
import inspect
from dataclasses import dataclass, field
from typing import Any, get_type_hints


@dataclass
class APIParameter:
    """A parameter to an API method.

    Attributes:
        name: Parameter name
        type_hint: String representation of the type
        required: Whether the parameter is required
        default: Default value (if any)
        description: Description from docstring
    """

    name: str
    type_hint: str
    required: bool = True
    default: Any = None
    description: str = ""


@dataclass
class APIMethod:
    """A function in a module.

    Attributes:
        name: Function name
        description: Description from docstring
        parameters: List of parameters
        return_type: String representation of return type
        is_async: Whether the function is async
    """

    name: str
    description: str = ""
    parameters: list[APIParameter] = field(default_factory=list)
    return_type: str = "Any"
    is_async: bool = False


@dataclass
class SubAPI:
    """A module containing API functions.

    Attributes:
        name: Module name (e.g., "skeleton", "anchors")
        class_name: Module path (e.g., "moss_intelligence.skeleton")
        description: Description from module docstring
        methods: List of public functions
    """

    name: str
    class_name: str
    description: str = ""
    methods: list[APIMethod] = field(default_factory=list)


def _parse_docstring(docstring: str | None) -> tuple[str, dict[str, str]]:
    """Parse a Google-style docstring.

    Returns:
        Tuple of (description, {param_name: param_description})
    """
    if not docstring:
        return "", {}

    lines = docstring.strip().split("\n")
    description_lines = []
    param_docs: dict[str, str] = {}

    in_args = False
    current_param = None
    current_desc = []

    for line in lines:
        stripped = line.strip()

        if stripped.lower().startswith("args:"):
            in_args = True
            continue
        elif stripped.lower().startswith(("returns:", "raises:", "example:")):
            # Save last param if any
            if current_param:
                param_docs[current_param] = " ".join(current_desc)
            in_args = False
            continue

        if in_args:
            # Check if this is a new parameter
            if ": " in stripped and not stripped.startswith(" "):
                # Save previous param
                if current_param:
                    param_docs[current_param] = " ".join(current_desc)

                param_name, desc = stripped.split(": ", 1)
                current_param = param_name.strip()
                current_desc = [desc.strip()]
            elif current_param and stripped:
                # Continuation of previous param description
                current_desc.append(stripped)
        else:
            if stripped:
                description_lines.append(stripped)

    # Save last param
    if current_param:
        param_docs[current_param] = " ".join(current_desc)

    return " ".join(description_lines), param_docs


def _get_type_string(type_hint: Any) -> str:
    """Convert a type hint to a string representation."""
    if type_hint is None:
        return "None"
    if type_hint is inspect.Parameter.empty:
        return "Any"

    # Handle string annotations
    if isinstance(type_hint, str):
        return type_hint

    # Handle typing module types
    origin = getattr(type_hint, "__origin__", None)
    if origin is not None:
        args = getattr(type_hint, "__args__", ())
        args_str = ", ".join(_get_type_string(a) for a in args)
        origin_name = getattr(origin, "__name__", str(origin))
        return f"{origin_name}[{args_str}]" if args_str else origin_name

    # Handle regular types
    if hasattr(type_hint, "__name__"):
        return type_hint.__name__

    return str(type_hint)


def introspect_method(method: Any, type_hints: dict[str, Any]) -> APIMethod:
    """Introspect a single function.

    Args:
        method: The function to introspect
        type_hints: Type hints for the function

    Returns:
        APIMethod with extracted metadata
    """
    sig = inspect.signature(method)
    docstring = inspect.getdoc(method)
    description, param_docs = _parse_docstring(docstring)

    parameters = []
    for name, param in sig.parameters.items():
        if name in ("self", "cls"):
            continue

        type_hint = type_hints.get(name, param.annotation)
        has_default = param.default is not inspect.Parameter.empty

        parameters.append(
            APIParameter(
                name=name,
                type_hint=_get_type_string(type_hint),
                required=not has_default,
                default=param.default if has_default else None,
                description=param_docs.get(name, ""),
            )
        )

    return_type = type_hints.get("return", sig.return_annotation)

    return APIMethod(
        name=method.__name__,
        description=description,
        parameters=parameters,
        return_type=_get_type_string(return_type),
        is_async=inspect.iscoroutinefunction(method),
    )


def introspect_subapi(api_class: type, api_name: str) -> SubAPI:
    """Introspect a class for its public methods.

    Args:
        api_class: The class to introspect
        api_name: The name of the API (e.g., "skeleton")

    Returns:
        SubAPI with extracted metadata
    """
    docstring = inspect.getdoc(api_class)
    description, _ = _parse_docstring(docstring)

    methods = []
    for name, member in inspect.getmembers(api_class, predicate=inspect.isfunction):
        # Skip private methods
        if name.startswith("_"):
            continue

        try:
            type_hints = get_type_hints(member)
        except (NameError, AttributeError, TypeError):
            type_hints = {}

        methods.append(introspect_method(member, type_hints))

    return SubAPI(
        name=api_name,
        class_name=api_class.__name__,
        description=description,
        methods=methods,
    )


def introspect_module(module_name: str, display_name: str) -> SubAPI:
    """Introspect a module for its public functions.

    Args:
        module_name: Full module path (e.g., "moss_intelligence.skeleton")
        display_name: Display name for the API (e.g., "skeleton")

    Returns:
        SubAPI with extracted metadata
    """
    try:
        module = importlib.import_module(module_name)
    except ImportError as e:
        return SubAPI(
            name=display_name,
            class_name=module_name,
            description=f"Import error: {e}",
            methods=[],
        )

    docstring = inspect.getdoc(module)
    description, _ = _parse_docstring(docstring)

    methods = []
    for name in dir(module):
        # Skip private items
        if name.startswith("_"):
            continue

        member = getattr(module, name)

        # Only include functions defined in this module
        if not inspect.isfunction(member):
            continue
        if getattr(member, "__module__", None) != module_name:
            continue

        try:
            type_hints = get_type_hints(member)
        except (NameError, AttributeError, TypeError):
            type_hints = {}

        methods.append(introspect_method(member, type_hints))

    return SubAPI(
        name=display_name,
        class_name=module_name,
        description=description,
        methods=methods,
    )


# Modules to introspect from moss-intelligence
INTELLIGENCE_MODULES = {
    "skeleton": "moss_intelligence.skeleton",
    "anchors": "moss_intelligence.anchors",
    "patches": "moss_intelligence.patches",
    "tree": "moss_intelligence.tree",
    "views": "moss_intelligence.views",
    "cfg": "moss_intelligence.cfg",
    "dependencies": "moss_intelligence.dependencies",
    "complexity": "moss_intelligence.complexity",
    "security": "moss_intelligence.security",
    "clones": "moss_intelligence.clones",
    "diagnostics": "moss_intelligence.diagnostics",
    "weaknesses": "moss_intelligence.weaknesses",
    "edit": "moss_intelligence.edit",
    "summarize": "moss_intelligence.summarize",
    "scopes": "moss_intelligence.scopes",
    "test_gaps": "moss_intelligence.test_gaps",
    "test_health": "moss_intelligence.test_health",
}

# Modules to introspect from moss-orchestration
ORCHESTRATION_MODULES = {
    "dwim": "moss_orchestration.dwim",
    "validators": "moss_orchestration.validators",
    "events": "moss_orchestration.events",
}


def introspect_api() -> list[SubAPI]:
    """Introspect the moss-intelligence and moss-orchestration APIs.

    Returns:
        List of SubAPI objects describing each module's public functions
    """
    results = []

    # Introspect moss-intelligence modules
    for name, module_path in INTELLIGENCE_MODULES.items():
        results.append(introspect_module(module_path, name))

    # Introspect moss-orchestration modules
    for name, module_path in ORCHESTRATION_MODULES.items():
        results.append(introspect_module(module_path, name))

    return results


__all__ = [
    "APIMethod",
    "APIParameter",
    "SubAPI",
    "introspect_api",
    "introspect_method",
    "introspect_module",
    "introspect_subapi",
]
