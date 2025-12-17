"""Tests for the plugin architecture."""

from pathlib import Path

import pytest

from moss.plugins import (
    PluginMetadata,
    PluginRegistry,
    ViewPlugin,
    detect_language,
    get_registry,
    reset_registry,
)
from moss.views import ViewOptions, ViewTarget

# =============================================================================
# PluginMetadata Tests
# =============================================================================


class TestPluginMetadata:
    def test_create_with_defaults(self):
        meta = PluginMetadata(name="test", view_type="skeleton")
        assert meta.name == "test"
        assert meta.view_type == "skeleton"
        assert meta.languages == frozenset()
        assert meta.priority == 0
        assert meta.version == "0.1.0"
        assert meta.description == ""

    def test_create_with_all_fields(self):
        meta = PluginMetadata(
            name="test-plugin",
            view_type="cfg",
            languages=frozenset(["python", "typescript"]),
            priority=10,
            version="1.2.3",
            description="A test plugin",
        )
        assert meta.name == "test-plugin"
        assert meta.view_type == "cfg"
        assert "python" in meta.languages
        assert "typescript" in meta.languages
        assert meta.priority == 10
        assert meta.version == "1.2.3"
        assert meta.description == "A test plugin"

    def test_is_frozen(self):
        meta = PluginMetadata(name="test", view_type="skeleton")
        with pytest.raises(AttributeError):
            meta.name = "changed"  # type: ignore


# =============================================================================
# PluginRegistry Tests
# =============================================================================


class MockPlugin:
    """Mock plugin for testing."""

    def __init__(
        self,
        name: str = "mock",
        view_type: str = "skeleton",
        languages: frozenset[str] = frozenset(["python"]),
        priority: int = 0,
    ):
        self._metadata = PluginMetadata(
            name=name,
            view_type=view_type,
            languages=languages,
            priority=priority,
        )

    @property
    def metadata(self) -> PluginMetadata:
        return self._metadata

    def supports(self, target: ViewTarget) -> bool:
        return target.path.suffix == ".py"

    async def render(self, target: ViewTarget, options: ViewOptions | None = None):
        from moss.views import View, ViewType

        return View(
            target=target,
            view_type=ViewType.SKELETON,
            content="mock content",
            metadata={},
        )


class TestPluginRegistry:
    @pytest.fixture
    def registry(self):
        return PluginRegistry()

    def test_register_plugin(self, registry: PluginRegistry):
        plugin = MockPlugin()
        registry.register(plugin)

        assert registry.get_plugin("mock") is plugin
        assert len(registry.get_all_plugins()) == 1

    def test_register_duplicate_raises(self, registry: PluginRegistry):
        plugin1 = MockPlugin(name="test")
        plugin2 = MockPlugin(name="test")

        registry.register(plugin1)
        with pytest.raises(ValueError, match="already registered"):
            registry.register(plugin2)

    def test_unregister_plugin(self, registry: PluginRegistry):
        plugin = MockPlugin()
        registry.register(plugin)

        assert registry.unregister("mock") is True
        assert registry.get_plugin("mock") is None
        assert registry.unregister("mock") is False  # Already removed

    def test_get_plugins_for_view_type(self, registry: PluginRegistry):
        plugin1 = MockPlugin(name="skel1", view_type="skeleton", priority=5)
        plugin2 = MockPlugin(name="skel2", view_type="skeleton", priority=10)
        plugin3 = MockPlugin(name="deps", view_type="dependency")

        registry.register(plugin1)
        registry.register(plugin2)
        registry.register(plugin3)

        skeleton_plugins = registry.get_plugins_for_view_type("skeleton")
        assert len(skeleton_plugins) == 2
        # Should be sorted by priority (highest first)
        assert skeleton_plugins[0].metadata.name == "skel2"
        assert skeleton_plugins[1].metadata.name == "skel1"

        deps_plugins = registry.get_plugins_for_view_type("dependency")
        assert len(deps_plugins) == 1

    def test_find_plugin(self, registry: PluginRegistry, tmp_path: Path):
        plugin = MockPlugin()
        registry.register(plugin)

        py_file = tmp_path / "test.py"
        py_file.write_text("# test")

        target = ViewTarget(path=py_file)
        found = registry.find_plugin(target, "skeleton")

        assert found is plugin

    def test_find_plugin_priority(self, registry: PluginRegistry, tmp_path: Path):
        """Higher priority plugins should be preferred."""
        low_priority = MockPlugin(name="low", priority=5)
        high_priority = MockPlugin(name="high", priority=10)

        # Register in reverse order
        registry.register(low_priority)
        registry.register(high_priority)

        py_file = tmp_path / "test.py"
        py_file.write_text("# test")

        target = ViewTarget(path=py_file)
        found = registry.find_plugin(target, "skeleton")

        assert found is high_priority

    def test_find_plugin_no_match(self, registry: PluginRegistry, tmp_path: Path):
        plugin = MockPlugin()
        registry.register(plugin)

        # Non-Python file
        txt_file = tmp_path / "test.txt"
        txt_file.write_text("hello")

        target = ViewTarget(path=txt_file)
        found = registry.find_plugin(target, "skeleton")

        assert found is None

    def test_get_supported_view_types(self, registry: PluginRegistry):
        registry.register(MockPlugin(name="p1", view_type="skeleton"))
        registry.register(MockPlugin(name="p2", view_type="dependency"))
        registry.register(MockPlugin(name="p3", view_type="skeleton"))

        types = registry.get_supported_view_types()
        assert types == {"skeleton", "dependency"}


# =============================================================================
# Global Registry Tests
# =============================================================================


class TestGlobalRegistry:
    def setup_method(self):
        reset_registry()

    def teardown_method(self):
        reset_registry()

    def test_get_registry_creates_singleton(self):
        registry1 = get_registry()
        registry2 = get_registry()
        assert registry1 is registry2

    def test_get_registry_has_builtin_plugins(self):
        registry = get_registry()

        # Should have Python plugins
        assert registry.get_plugin("python-skeleton") is not None
        assert registry.get_plugin("python-dependency") is not None
        assert registry.get_plugin("python-cfg") is not None

    def test_reset_registry_clears_singleton(self):
        registry1 = get_registry()
        reset_registry()
        registry2 = get_registry()
        assert registry1 is not registry2


# =============================================================================
# Language Detection Tests
# =============================================================================


class TestDetectLanguage:
    @pytest.mark.parametrize(
        "filename,expected",
        [
            ("test.py", "python"),
            ("test.pyi", "python"),
            ("test.js", "javascript"),
            ("test.mjs", "javascript"),
            ("test.jsx", "javascript"),
            ("test.ts", "typescript"),
            ("test.tsx", "typescript"),
            ("test.go", "go"),
            ("test.rs", "rust"),
            ("test.java", "java"),
            ("test.c", "c"),
            ("test.h", "c"),
            ("test.cpp", "cpp"),
            ("test.hpp", "cpp"),
            ("test.rb", "ruby"),
            ("test.md", "markdown"),
            ("test.json", "json"),
            ("test.yaml", "yaml"),
            ("test.yml", "yaml"),
            ("test.toml", "toml"),
            ("test.unknown", "unknown"),
        ],
    )
    def test_detect_language(self, filename: str, expected: str):
        assert detect_language(Path(filename)) == expected


# =============================================================================
# Builtin Plugin Tests
# =============================================================================


class TestPythonSkeletonPlugin:
    @pytest.fixture
    def plugin(self):
        from moss.skeleton import PythonSkeletonPlugin

        return PythonSkeletonPlugin()

    @pytest.fixture
    def python_file(self, tmp_path: Path) -> Path:
        f = tmp_path / "test.py"
        f.write_text("""
class Foo:
    def bar(self): pass

def baz(): pass
""")
        return f

    def test_metadata(self, plugin):
        meta = plugin.metadata
        assert meta.name == "python-skeleton"
        assert meta.view_type == "skeleton"
        assert "python" in meta.languages
        assert meta.priority == 5

    def test_supports_python(self, plugin, python_file: Path):
        target = ViewTarget(path=python_file)
        assert plugin.supports(target) is True

    def test_not_supports_non_python(self, plugin, tmp_path: Path):
        txt_file = tmp_path / "test.txt"
        txt_file.write_text("hello")
        target = ViewTarget(path=txt_file)
        assert plugin.supports(target) is False

    async def test_render(self, plugin, python_file: Path):
        target = ViewTarget(path=python_file)
        view = await plugin.render(target)

        assert "class Foo" in view.content
        assert "def bar" in view.content
        assert "def baz" in view.content
        assert view.metadata.get("symbol_count") == 2  # Foo and baz


class TestPythonDependencyPlugin:
    @pytest.fixture
    def plugin(self):
        from moss.dependencies import PythonDependencyPlugin

        return PythonDependencyPlugin()

    @pytest.fixture
    def python_file(self, tmp_path: Path) -> Path:
        f = tmp_path / "test.py"
        f.write_text("""
import os
from pathlib import Path

def main(): pass
""")
        return f

    def test_metadata(self, plugin):
        meta = plugin.metadata
        assert meta.name == "python-dependency"
        assert meta.view_type == "dependency"
        assert "python" in meta.languages

    async def test_render(self, plugin, python_file: Path):
        target = ViewTarget(path=python_file)
        view = await plugin.render(target)

        assert "import os" in view.content
        assert view.metadata.get("import_count") == 2


class TestPythonCFGPlugin:
    @pytest.fixture
    def plugin(self):
        from moss.cfg import PythonCFGPlugin

        return PythonCFGPlugin()

    @pytest.fixture
    def python_file(self, tmp_path: Path) -> Path:
        f = tmp_path / "test.py"
        f.write_text("""
def foo():
    if True:
        return 1
    return 2
""")
        return f

    def test_metadata(self, plugin):
        meta = plugin.metadata
        assert meta.name == "python-cfg"
        assert meta.view_type == "cfg"
        assert "python" in meta.languages

    async def test_render(self, plugin, python_file: Path):
        target = ViewTarget(path=python_file)
        view = await plugin.render(target)

        assert view.metadata.get("function_count") == 1
        assert len(view.metadata.get("cfgs", [])) == 1


# =============================================================================
# ViewPlugin Protocol Tests
# =============================================================================


class TestViewPluginProtocol:
    def test_mock_plugin_is_view_plugin(self):
        """Verify MockPlugin satisfies ViewPlugin protocol."""
        plugin = MockPlugin()
        assert isinstance(plugin, ViewPlugin)

    def test_builtin_plugins_are_view_plugin(self):
        """Verify builtin plugins satisfy ViewPlugin protocol."""
        from moss.cfg import PythonCFGPlugin
        from moss.dependencies import PythonDependencyPlugin
        from moss.skeleton import PythonSkeletonPlugin

        assert isinstance(PythonSkeletonPlugin(), ViewPlugin)
        assert isinstance(PythonDependencyPlugin(), ViewPlugin)
        assert isinstance(PythonCFGPlugin(), ViewPlugin)
