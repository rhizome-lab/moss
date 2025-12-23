"""API introspection for interface generation.

This module provides tools to introspect the MossAPI and extract
metadata about its structure, methods, and parameters.
"""

from __future__ import annotations

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
    """A method on a sub-API.

    Attributes:
        name: Method name
        description: Description from docstring
        parameters: List of parameters
        return_type: String representation of return type
        is_async: Whether the method is async
    """

    name: str
    description: str = ""
    parameters: list[APIParameter] = field(default_factory=list)
    return_type: str = "Any"
    is_async: bool = False


@dataclass
class SubAPI:
    """A sub-API of MossAPI.

    Attributes:
        name: API name (e.g., "skeleton", "anchor")
        class_name: Class name (e.g., "SkeletonAPI")
        description: Description from docstring
        methods: List of public methods
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
    """Introspect a single method.

    Args:
        method: The method to introspect
        type_hints: Type hints for the method

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
    """Introspect a sub-API class.

    Args:
        api_class: The API class to introspect
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


def introspect_api() -> list[SubAPI]:
    """Introspect the full MossAPI.

    Returns:
        List of SubAPI objects describing each sub-API
    """
    from moss.moss_api import MossAPI

    # Auto-discover sub-APIs from MossAPI properties
    results = []

    # We look for public properties that return a class with 'API' in the name
    # or are explicitly listed in our sub_apis map.
    for name, member in inspect.getmembers(MossAPI):
        if name.startswith("_"):
            continue

        if isinstance(member, property):
            # Try to get the return type from type hints
            try:
                hints = get_type_hints(member.fget)
                return_type = hints.get("return")
                if (
                    return_type
                    and hasattr(return_type, "__name__")
                    and "API" in return_type.__name__
                ):
                    results.append(introspect_subapi(return_type, name))
            except (NameError, AttributeError, TypeError):
                # Fallback: if we can't get hints, we skip it
                # For now, keep the manual list for robustness if needed
                pass

    if not results:
        # Fallback to manual list if auto-discovery fails or is incomplete
        from moss.moss_api import (
            CFGAPI,
            DWIMAPI,
            RAGAPI,
            AgentAPI,
            AnchorAPI,
            ClonesAPI,
            ComplexityAPI,
            ContextAPI,
            DependencyAPI,
            EditAPI,
            ExternalDepsAPI,
            GitAPI,
            GitHotspotsAPI,
            GuessabilityAPI,
            HealthAPI,
            LessonsAPI,
            PatchAPI,
            RefCheckAPI,
            SearchAPI,
            SecurityAPI,
            SkeletonAPI,
            TodoAPI,
            TomlAPI,
            TreeAPI,
            ValidationAPI,
            WeaknessesAPI,
            WebAPI,
        )

        sub_apis = {
            "skeleton": SkeletonAPI,
            "tree": TreeAPI,
            "anchor": AnchorAPI,
            "patch": PatchAPI,
            "dependencies": DependencyAPI,
            "cfg": CFGAPI,
            "validation": ValidationAPI,
            "git": GitAPI,
            "context": ContextAPI,
            "health": HealthAPI,
            "todo": TodoAPI,
            "dwim": DWIMAPI,
            "agent": AgentAPI,
            "edit": EditAPI,
            "complexity": ComplexityAPI,
            "clones": ClonesAPI,
            "security": SecurityAPI,
            "ref_check": RefCheckAPI,
            "git_hotspots": GitHotspotsAPI,
            "external_deps": ExternalDepsAPI,
            "weaknesses": WeaknessesAPI,
            "rag": RAGAPI,
            "web": WebAPI,
            "search": SearchAPI,
            "guessability": GuessabilityAPI,
            "lessons": LessonsAPI,
            "toml": TomlAPI,
        }

        for name, cls in sub_apis.items():
            results.append(introspect_subapi(cls, name))

    return results


__all__ = [
    "APIMethod",
    "APIParameter",
    "SubAPI",
    "introspect_api",
    "introspect_method",
    "introspect_subapi",
]
