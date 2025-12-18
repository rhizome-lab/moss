"""Plugin registry for synthesis components.

This module provides the SynthesisRegistry for discovering and managing
synthesis plugins (generators, validators, libraries).

The registry follows the same pattern as moss.plugins.PluginRegistry:
- Manual plugin registration
- Automatic discovery via entry points
- Priority-based plugin selection
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, TypeVar

if TYPE_CHECKING:
    from moss.synthesis.types import Context, Specification

from .protocols import (
    CodeGenerator,
    LibraryPlugin,
    SynthesisValidator,
)

logger = logging.getLogger(__name__)

T = TypeVar("T")


# =============================================================================
# Sub-Registries
# =============================================================================


class GeneratorRegistry:
    """Registry for CodeGenerator plugins."""

    def __init__(self) -> None:
        self._plugins: dict[str, CodeGenerator] = {}
        self._by_type: dict[str, list[CodeGenerator]] = {}

    def register(self, plugin: CodeGenerator) -> None:
        """Register a code generator."""
        meta = plugin.metadata

        if meta.name in self._plugins:
            raise ValueError(f"Generator '{meta.name}' is already registered")

        self._plugins[meta.name] = plugin

        # Index by generator type
        type_key = meta.generator_type.value
        if type_key not in self._by_type:
            self._by_type[type_key] = []
        self._by_type[type_key].append(plugin)

        # Keep sorted by priority (descending)
        self._by_type[type_key].sort(key=lambda p: p.metadata.priority, reverse=True)

        logger.debug(
            "Registered generator: %s (type=%s, priority=%d)",
            meta.name,
            meta.generator_type.value,
            meta.priority,
        )

    def unregister(self, name: str) -> bool:
        """Unregister a generator by name."""
        plugin = self._plugins.pop(name, None)
        if plugin is None:
            return False

        type_key = plugin.metadata.generator_type.value
        if type_key in self._by_type:
            self._by_type[type_key] = [
                p for p in self._by_type[type_key] if p.metadata.name != name
            ]
        return True

    def get(self, name: str) -> CodeGenerator | None:
        """Get a generator by name."""
        return self._plugins.get(name)

    def find_best(
        self,
        spec: Specification,
        context: Context,
    ) -> CodeGenerator | None:
        """Find the best generator for a specification.

        Selection based on:
        1. can_generate() returning True
        2. Priority (highest wins)
        """
        # Flatten all plugins sorted by priority
        all_plugins = sorted(
            self._plugins.values(),
            key=lambda p: p.metadata.priority,
            reverse=True,
        )

        for plugin in all_plugins:
            if plugin.can_generate(spec, context):
                return plugin

        return None

    def get_all(self) -> list[CodeGenerator]:
        """Get all registered generators."""
        return list(self._plugins.values())


class ValidatorRegistry:
    """Registry for SynthesisValidator plugins."""

    def __init__(self) -> None:
        self._plugins: dict[str, SynthesisValidator] = {}
        self._by_type: dict[str, list[SynthesisValidator]] = {}

    def register(self, plugin: SynthesisValidator) -> None:
        """Register a synthesis validator."""
        meta = plugin.metadata

        if meta.name in self._plugins:
            raise ValueError(f"Validator '{meta.name}' is already registered")

        self._plugins[meta.name] = plugin

        # Index by validator type
        type_key = meta.validator_type.value
        if type_key not in self._by_type:
            self._by_type[type_key] = []
        self._by_type[type_key].append(plugin)

        # Keep sorted by priority (descending)
        self._by_type[type_key].sort(key=lambda p: p.metadata.priority, reverse=True)

        logger.debug(
            "Registered validator: %s (type=%s, priority=%d)",
            meta.name,
            meta.validator_type.value,
            meta.priority,
        )

    def unregister(self, name: str) -> bool:
        """Unregister a validator by name."""
        plugin = self._plugins.pop(name, None)
        if plugin is None:
            return False

        type_key = plugin.metadata.validator_type.value
        if type_key in self._by_type:
            self._by_type[type_key] = [
                p for p in self._by_type[type_key] if p.metadata.name != name
            ]
        return True

    def get(self, name: str) -> SynthesisValidator | None:
        """Get a validator by name."""
        return self._plugins.get(name)

    def find_all_applicable(
        self,
        spec: Specification,
        code: str,
    ) -> list[SynthesisValidator]:
        """Find all validators applicable for a specification and code.

        Returns validators sorted by priority.
        """
        applicable = [
            plugin for plugin in self._plugins.values() if plugin.can_validate(spec, code)
        ]
        return sorted(
            applicable,
            key=lambda p: p.metadata.priority,
            reverse=True,
        )

    def get_all(self) -> list[SynthesisValidator]:
        """Get all registered validators."""
        return list(self._plugins.values())


class LibraryRegistry:
    """Registry for LibraryPlugin plugins."""

    def __init__(self) -> None:
        self._plugins: dict[str, LibraryPlugin] = {}

    def register(self, plugin: LibraryPlugin) -> None:
        """Register a library plugin."""
        meta = plugin.metadata

        if meta.name in self._plugins:
            raise ValueError(f"Library '{meta.name}' is already registered")

        self._plugins[meta.name] = plugin

        logger.debug(
            "Registered library: %s (supports_learning=%s)",
            meta.name,
            meta.supports_learning,
        )

    def unregister(self, name: str) -> bool:
        """Unregister a library by name."""
        return self._plugins.pop(name, None) is not None

    def get(self, name: str) -> LibraryPlugin | None:
        """Get a library by name."""
        return self._plugins.get(name)

    def get_all(self) -> list[LibraryPlugin]:
        """Get all registered libraries."""
        return sorted(
            self._plugins.values(),
            key=lambda p: p.metadata.priority,
            reverse=True,
        )


# =============================================================================
# Main Registry
# =============================================================================


class SynthesisRegistry:
    """Unified registry for all synthesis plugins.

    Sub-registries:
    - generators: CodeGenerator plugins
    - validators: SynthesisValidator plugins
    - libraries: LibraryPlugin plugins

    Entry point groups:
    - moss.synthesis.generators
    - moss.synthesis.validators
    - moss.synthesis.libraries
    """

    def __init__(self) -> None:
        self.generators = GeneratorRegistry()
        self.validators = ValidatorRegistry()
        self.libraries = LibraryRegistry()
        self._discovered = False

    def discover_plugins(self) -> dict[str, int]:
        """Discover and register plugins via entry points.

        Returns:
            Dict mapping plugin type to count discovered
        """
        if self._discovered:
            return {"generators": 0, "validators": 0, "libraries": 0}

        counts = {"generators": 0, "validators": 0, "libraries": 0}

        try:
            from importlib.metadata import entry_points

            # Discover generators
            for ep in entry_points(group="moss.synthesis.generators"):
                try:
                    plugin = ep.load()()
                    if isinstance(plugin, CodeGenerator):
                        self.generators.register(plugin)
                        counts["generators"] += 1
                        logger.info("Discovered generator: %s", ep.name)
                except Exception as e:
                    logger.warning("Failed to load generator '%s': %s", ep.name, e)

            # Discover validators
            for ep in entry_points(group="moss.synthesis.validators"):
                try:
                    plugin = ep.load()()
                    if isinstance(plugin, SynthesisValidator):
                        self.validators.register(plugin)
                        counts["validators"] += 1
                        logger.info("Discovered validator: %s", ep.name)
                except Exception as e:
                    logger.warning("Failed to load validator '%s': %s", ep.name, e)

            # Discover libraries
            for ep in entry_points(group="moss.synthesis.libraries"):
                try:
                    plugin = ep.load()()
                    if isinstance(plugin, LibraryPlugin):
                        self.libraries.register(plugin)
                        counts["libraries"] += 1
                        logger.info("Discovered library: %s", ep.name)
                except Exception as e:
                    logger.warning("Failed to load library '%s': %s", ep.name, e)

        except ImportError:
            logger.debug("importlib.metadata not available, skipping discovery")

        self._discovered = True
        return counts

    def register_builtins(self) -> None:
        """Register built-in plugins.

        Provides fallback registration for plugins not installed via entry points.
        """
        # Import built-in generators
        try:
            from moss.synthesis.plugins.generators import PlaceholderGenerator

            if "placeholder" not in [g.metadata.name for g in self.generators.get_all()]:
                self.generators.register(PlaceholderGenerator())
        except ImportError:
            logger.debug("PlaceholderGenerator not available")

        try:
            from moss.synthesis.plugins.generators import TemplateGenerator

            if "template" not in [g.metadata.name for g in self.generators.get_all()]:
                self.generators.register(TemplateGenerator())
        except ImportError:
            logger.debug("TemplateGenerator not available")

        try:
            from moss.synthesis.plugins.generators import LLMGenerator, MockLLMProvider

            if "llm" not in [g.metadata.name for g in self.generators.get_all()]:
                # Register with mock provider by default (safe for testing)
                self.generators.register(LLMGenerator(provider=MockLLMProvider()))
        except ImportError:
            logger.debug("LLMGenerator not available")

        try:
            from moss.synthesis.plugins.generators import ComponentGenerator

            if "component" not in [g.metadata.name for g in self.generators.get_all()]:
                self.generators.register(ComponentGenerator())
        except ImportError:
            logger.debug("ComponentGenerator not available")

        try:
            from moss.synthesis.plugins.generators import SMTGenerator

            if "smt" not in [g.metadata.name for g in self.generators.get_all()]:
                self.generators.register(SMTGenerator())
        except ImportError:
            logger.debug("SMTGenerator not available")

        try:
            from moss.synthesis.plugins.generators import PBEGenerator

            if "pbe" not in [g.metadata.name for g in self.generators.get_all()]:
                self.generators.register(PBEGenerator())
        except ImportError:
            logger.debug("PBEGenerator not available")

        # Import built-in validators
        try:
            from moss.synthesis.plugins.validators import PytestValidator

            if "pytest" not in [v.metadata.name for v in self.validators.get_all()]:
                self.validators.register(PytestValidator())
        except ImportError:
            logger.debug("PytestValidator not available")

        # Import built-in libraries
        try:
            from moss.synthesis.plugins.libraries import MemoryLibrary

            if "memory" not in [lib.metadata.name for lib in self.libraries.get_all()]:
                self.libraries.register(MemoryLibrary())
        except ImportError:
            logger.debug("MemoryLibrary not available")

        try:
            from moss.synthesis.plugins.libraries import LearnedLibrary

            if "learned" not in [lib.metadata.name for lib in self.libraries.get_all()]:
                self.libraries.register(LearnedLibrary())
        except ImportError:
            logger.debug("LearnedLibrary not available")

    def ensure_initialized(self) -> None:
        """Ensure the registry has discovered plugins."""
        if not self._discovered:
            self.discover_plugins()
            self.register_builtins()


# =============================================================================
# Global Registry
# =============================================================================

_global_synthesis_registry: SynthesisRegistry | None = None


def get_synthesis_registry() -> SynthesisRegistry:
    """Get the global synthesis plugin registry.

    Creates and initializes the registry on first call.
    """
    global _global_synthesis_registry

    if _global_synthesis_registry is None:
        _global_synthesis_registry = SynthesisRegistry()
        _global_synthesis_registry.ensure_initialized()

    return _global_synthesis_registry


def reset_synthesis_registry() -> None:
    """Reset the global synthesis registry (mainly for testing)."""
    global _global_synthesis_registry
    _global_synthesis_registry = None


__all__ = [
    "GeneratorRegistry",
    "LibraryRegistry",
    "SynthesisRegistry",
    "ValidatorRegistry",
    "get_synthesis_registry",
    "reset_synthesis_registry",
]
