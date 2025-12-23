"""Tests for Dependency Graph Provider."""

from pathlib import Path

import pytest

from moss_intelligence.dependencies import (
    PythonDependencyProvider,
    expand_import_context,
    extract_dependencies,
    format_dependencies,
    format_import_context,
    resolve_relative_import,
)
from moss_intelligence.views import ViewTarget, ViewType


class TestExtractDependencies:
    """Tests for extract_dependencies."""

    def test_simple_import(self):
        source = "import os"
        info = extract_dependencies(source)

        assert len(info.imports) == 1
        assert info.imports[0].module == "os"
        assert info.imports[0].names == []
        assert info.imports[0].is_relative is False

    def test_import_with_alias(self):
        source = "import numpy as np"
        info = extract_dependencies(source)

        assert info.imports[0].module == "numpy"
        assert info.imports[0].alias == "np"

    def test_from_import(self):
        source = "from os import path, getcwd"
        info = extract_dependencies(source)

        assert info.imports[0].module == "os"
        assert info.imports[0].names == ["path", "getcwd"]

    def test_relative_import(self):
        source = "from . import module"
        info = extract_dependencies(source)

        assert info.imports[0].is_relative is True
        assert info.imports[0].level == 1

    def test_relative_import_with_module(self):
        source = "from ..package import thing"
        info = extract_dependencies(source)

        assert info.imports[0].is_relative is True
        assert info.imports[0].level == 2
        assert info.imports[0].module == "package"
        assert info.imports[0].names == ["thing"]

    def test_export_function(self):
        source = """
def public_func():
    pass

def _private_func():
    pass
"""
        info = extract_dependencies(source)

        export_names = [e.name for e in info.exports]
        assert "public_func" in export_names
        assert "_private_func" not in export_names

    def test_export_class(self):
        source = """
class MyClass:
    def method(self):
        pass
"""
        info = extract_dependencies(source)

        assert len(info.exports) == 1
        assert info.exports[0].name == "MyClass"
        assert info.exports[0].kind == "class"

    def test_export_variable(self):
        source = """
PUBLIC_CONST = 42
_private = "hidden"
"""
        info = extract_dependencies(source)

        export_names = [e.name for e in info.exports]
        assert "PUBLIC_CONST" in export_names
        assert "_private" not in export_names

    def test_all_exports(self):
        source = """
__all__ = ["foo", "bar"]

def foo(): pass
def bar(): pass
def baz(): pass
"""
        info = extract_dependencies(source)

        assert info.all_exports == ["foo", "bar"]

    def test_multiple_imports(self):
        source = """
import os
import sys
from pathlib import Path
from typing import List, Dict
"""
        info = extract_dependencies(source)

        assert len(info.imports) == 4


class TestFormatDependencies:
    """Tests for format_dependencies."""

    def test_format_imports(self):
        info = extract_dependencies("import os\nfrom pathlib import Path")
        output = format_dependencies(info)

        assert "import os" in output
        assert "from pathlib import Path" in output

    def test_format_relative_imports(self):
        info = extract_dependencies("from . import module")
        output = format_dependencies(info)

        assert "from . import module" in output

    def test_format_exports(self):
        info = extract_dependencies("def hello(): pass\nclass World: pass")
        output = format_dependencies(info)

        assert "function: hello" in output
        assert "class: World" in output


class TestPythonDependencyProvider:
    """Tests for PythonDependencyProvider."""

    @pytest.fixture
    def provider(self):
        return PythonDependencyProvider()

    @pytest.fixture
    def python_file(self, tmp_path: Path) -> Path:
        f = tmp_path / "test.py"
        f.write_text("""
import os
from pathlib import Path

def main():
    pass

class App:
    pass
""")
        return f

    def test_view_type(self, provider: PythonDependencyProvider):
        assert provider.view_type == ViewType.DEPENDENCY

    def test_supported_languages(self, provider: PythonDependencyProvider):
        assert provider.supported_languages == {"python"}

    async def test_render(self, provider: PythonDependencyProvider, python_file: Path):
        target = ViewTarget(path=python_file)
        view = await provider.render(target)

        assert view.view_type == ViewType.DEPENDENCY
        assert "import os" in view.content
        assert "from pathlib import Path" in view.content
        assert "function: main" in view.content
        assert "class: App" in view.content
        assert view.metadata["import_count"] == 2
        assert view.metadata["export_count"] == 2

    async def test_render_syntax_error(self, provider: PythonDependencyProvider, tmp_path: Path):
        f = tmp_path / "broken.py"
        f.write_text("def broken(")
        target = ViewTarget(path=f)

        view = await provider.render(target)

        assert "Parse error" in view.content
        assert "error" in view.metadata


class TestResolveRelativeImport:
    """Tests for resolve_relative_import."""

    @pytest.fixture
    def project(self, tmp_path: Path) -> Path:
        """Create a sample project structure."""
        pkg = tmp_path / "mypackage"
        pkg.mkdir()
        (pkg / "__init__.py").write_text("")
        (pkg / "models.py").write_text("class User: pass")
        (pkg / "api.py").write_text("from .models import User")

        subpkg = pkg / "sub"
        subpkg.mkdir()
        (subpkg / "__init__.py").write_text("")
        (subpkg / "utils.py").write_text("from ..models import User")

        return tmp_path

    def test_resolve_same_package(self, project: Path):
        """Resolve from .models import User in api.py."""
        info = extract_dependencies("from .models import User")
        current_file = project / "mypackage" / "api.py"

        resolved = resolve_relative_import(info.imports[0], current_file, project)

        assert resolved is not None
        assert resolved.name == "models.py"
        assert resolved.exists()

    def test_resolve_parent_package(self, project: Path):
        """Resolve from ..models import User in sub/utils.py."""
        info = extract_dependencies("from ..models import User")
        current_file = project / "mypackage" / "sub" / "utils.py"

        resolved = resolve_relative_import(info.imports[0], current_file, project)

        assert resolved is not None
        assert resolved.name == "models.py"

    def test_resolve_non_relative_returns_none(self, project: Path):
        """Non-relative imports return None."""
        info = extract_dependencies("import os")
        current_file = project / "mypackage" / "api.py"

        resolved = resolve_relative_import(info.imports[0], current_file, project)

        assert resolved is None

    def test_resolve_missing_module(self, project: Path):
        """Missing modules return None."""
        info = extract_dependencies("from .nonexistent import Foo")
        current_file = project / "mypackage" / "api.py"

        resolved = resolve_relative_import(info.imports[0], current_file, project)

        assert resolved is None


class TestExpandImportContext:
    """Tests for expand_import_context."""

    @pytest.fixture
    def project(self, tmp_path: Path) -> Path:
        """Create a sample project with imports."""
        pkg = tmp_path / "mypackage"
        pkg.mkdir()
        (pkg / "__init__.py").write_text("")
        (pkg / "models.py").write_text("""
class User:
    id: int
    email: str

    def validate(self) -> bool:
        return True

class Session:
    token: str
""")
        (pkg / "api.py").write_text("""
from .models import User, Session

def get_user(user_id: int) -> User:
    pass
""")
        return tmp_path

    def test_expands_imported_symbols(self, project: Path):
        """Expands imported symbols to their skeletons."""
        api_file = project / "mypackage" / "api.py"

        context = expand_import_context(api_file, project)

        # Should have entries for User and Session
        assert len(context) == 2
        assert any("User" in k for k in context.keys())
        assert any("Session" in k for k in context.keys())

    def test_skeleton_contains_signature(self, project: Path):
        """Skeleton includes class signature."""
        api_file = project / "mypackage" / "api.py"

        context = expand_import_context(api_file, project)

        # Find User skeleton
        user_key = next(k for k in context if "User" in k)
        assert "class User" in context[user_key]

    def test_handles_nonexistent_file(self, tmp_path: Path):
        """Returns empty dict for nonexistent files."""
        context = expand_import_context(tmp_path / "nonexistent.py", tmp_path)

        assert context == {}

    def test_handles_no_imports(self, tmp_path: Path):
        """Returns empty dict for files with no relative imports."""
        f = tmp_path / "standalone.py"
        f.write_text("import os\ndef hello(): pass")

        context = expand_import_context(f, tmp_path)

        assert context == {}


class TestFormatImportContext:
    """Tests for format_import_context."""

    def test_formats_context(self):
        """Formats context as commented text."""
        context = {
            "models:User": "class User:\n    ...",
            "auth:Token": "class Token:\n    ...",
        }

        output = format_import_context(context)

        assert "# Imported Types:" in output
        assert "# models:User" in output
        assert "# auth:Token" in output

    def test_empty_context(self):
        """Empty context returns empty string."""
        output = format_import_context({})

        assert output == ""
