"""Backend implementations for the rules system.

Available backends:
- regex: Simple pattern matching
- ast_grep: Structural AST patterns via ast-grep
- python: Arbitrary Python checks
- pyright: Type-aware analysis (requires pyright)
- deps: Cross-file dependency analysis
"""

from __future__ import annotations

from ..base import Backend, BaseBackend

# Registry of available backends
_BACKENDS: dict[str, type[BaseBackend]] = {}


def register_backend(cls: type[BaseBackend]) -> type[BaseBackend]:
    """Register a backend class."""
    instance = cls()
    _BACKENDS[instance.name] = cls
    return cls


def get_backend(name: str) -> BaseBackend:
    """Get a backend instance by name.

    Args:
        name: Backend name (e.g., "regex", "ast-grep")

    Returns:
        Backend instance

    Raises:
        ValueError: If backend not found
    """
    if name not in _BACKENDS:
        available = ", ".join(_BACKENDS.keys())
        raise ValueError(f"Unknown backend: {name}. Available: {available}")
    return _BACKENDS[name]()


def list_backends() -> list[str]:
    """List available backend names."""
    return list(_BACKENDS.keys())


# Import backends to trigger registration (must be after registry definition)
from . import ast_grep as _ast_grep  # noqa: E402, F401
from . import deps as _deps  # noqa: E402, F401
from . import python as _python  # noqa: E402, F401
from . import regex as _regex  # noqa: E402, F401

__all__ = [
    "Backend",
    "BaseBackend",
    "get_backend",
    "list_backends",
    "register_backend",
]
