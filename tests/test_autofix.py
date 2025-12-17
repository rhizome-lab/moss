"""Tests for auto-fix system."""

from pathlib import Path

import pytest

from moss.autofix import (
    Conflict,
    ConflictResolver,
    Fix,
    FixEngine,
    FixResult,
    FixSafety,
    SafetyClassifier,
    create_fix,
)


class TestFix:
    """Tests for Fix dataclass."""

    def test_create_fix(self):
        fix = Fix(
            file_path="src/foo.py",
            old_text="def foo():",
            new_text="def foo() -> None:",
            description="Add return type",
        )

        assert fix.file_path == "src/foo.py"
        assert fix.safety == FixSafety.NEEDS_REVIEW
        assert not fix.is_safe

    def test_safe_fix(self):
        fix = Fix(
            file_path="src/foo.py",
            old_text="x",
            new_text="y",
            description="Safe change",
            safety=FixSafety.SAFE,
        )

        assert fix.is_safe

    def test_fix_with_metadata(self):
        fix = Fix(
            file_path="src/foo.py",
            old_text="x",
            new_text="y",
            description="Fix",
            source="ruff",
            rule_id="E501",
            confidence=0.95,
            line_start=10,
            line_end=12,
        )

        assert fix.source == "ruff"
        assert fix.rule_id == "E501"
        assert fix.confidence == 0.95
        assert fix.line_start == 10
        assert fix.line_end == 12


class TestFixResult:
    """Tests for FixResult dataclass."""

    def test_success_with_applied(self):
        fix = Fix(file_path="f", old_text="a", new_text="b", description="d")
        result = FixResult(applied=[fix])

        assert result.success

    def test_failure_with_errors(self):
        fix = Fix(file_path="f", old_text="a", new_text="b", description="d")
        result = FixResult(errors=[(fix, Exception("error"))])

        assert not result.success

    def test_failure_with_no_applied(self):
        result = FixResult()
        assert not result.success


class TestSafetyClassifier:
    """Tests for SafetyClassifier."""

    @pytest.fixture
    def classifier(self) -> SafetyClassifier:
        return SafetyClassifier()

    def test_classify_type_annotation_as_safe(self, classifier: SafetyClassifier):
        fix = Fix(
            file_path="f.py",
            old_text="def foo():",
            new_text="def foo() -> None:",
            description="Add return type",
        )

        safety = classifier.classify(fix)
        assert safety == FixSafety.SAFE

    def test_classify_deletion_as_unsafe(self, classifier: SafetyClassifier):
        fix = Fix(
            file_path="f.py",
            old_text="important_code()",
            new_text="",
            description="Delete code",
        )

        safety = classifier.classify(fix)
        assert safety == FixSafety.UNSAFE

    def test_classify_security_sensitive_as_unsafe(self, classifier: SafetyClassifier):
        fix = Fix(
            file_path="f.py",
            old_text="password = 'old'",
            new_text="password = 'new'",
            description="Change password",
        )

        safety = classifier.classify(fix)
        assert safety == FixSafety.UNSAFE

    def test_classify_import_as_safe(self, classifier: SafetyClassifier):
        fix = Fix(
            file_path="f.py",
            old_text="import os",
            new_text="import os\nimport sys",
            description="Add import",
        )

        safety = classifier.classify(fix)
        assert safety == FixSafety.SAFE

    def test_respects_existing_classification(self, classifier: SafetyClassifier):
        fix = Fix(
            file_path="f.py",
            old_text="x",
            new_text="y",
            description="d",
            safety=FixSafety.UNSAFE,
        )

        safety = classifier.classify(fix)
        assert safety == FixSafety.UNSAFE

    def test_classify_batch(self, classifier: SafetyClassifier):
        fixes = [
            Fix(
                file_path="f.py",
                old_text="def foo():",
                new_text="def foo() -> None:",
                description="d",
            ),
            Fix(file_path="f.py", old_text="code", new_text="", description="d"),
        ]

        classified = classifier.classify_batch(fixes)

        assert classified[0].safety == FixSafety.SAFE
        assert classified[1].safety == FixSafety.UNSAFE


class TestConflictResolver:
    """Tests for ConflictResolver."""

    @pytest.fixture
    def resolver(self) -> ConflictResolver:
        return ConflictResolver()

    def test_no_conflicts_different_files(self, resolver: ConflictResolver):
        fixes = [
            Fix(file_path="a.py", old_text="x", new_text="y", description="d", line_start=1),
            Fix(file_path="b.py", old_text="x", new_text="z", description="d", line_start=1),
        ]

        conflicts = resolver.find_conflicts(fixes)
        assert len(conflicts) == 0

    def test_no_conflicts_non_overlapping(self, resolver: ConflictResolver):
        fixes = [
            Fix(
                file_path="a.py",
                old_text="x",
                new_text="y",
                description="d",
                line_start=1,
                line_end=5,
            ),
            Fix(
                file_path="a.py",
                old_text="z",
                new_text="w",
                description="d",
                line_start=10,
                line_end=15,
            ),
        ]

        conflicts = resolver.find_conflicts(fixes)
        assert len(conflicts) == 0

    def test_finds_overlapping_conflicts(self, resolver: ConflictResolver):
        fixes = [
            Fix(
                file_path="a.py",
                old_text="x",
                new_text="y",
                description="d",
                line_start=1,
                line_end=10,
            ),
            Fix(
                file_path="a.py",
                old_text="z",
                new_text="w",
                description="d",
                line_start=5,
                line_end=15,
            ),
        ]

        conflicts = resolver.find_conflicts(fixes)
        assert len(conflicts) == 1
        assert len(conflicts[0].fixes) == 2

    def test_resolve_first(self, resolver: ConflictResolver):
        fix1 = Fix(file_path="a.py", old_text="x", new_text="y", description="first")
        fix2 = Fix(file_path="a.py", old_text="x", new_text="z", description="second")
        conflict = Conflict(fixes=[fix1, fix2], file_path="a.py", overlap_start=1, overlap_end=10)

        resolved = resolver.resolve(conflict, "first")

        assert len(resolved) == 1
        assert resolved[0] == fix1

    def test_resolve_last(self, resolver: ConflictResolver):
        fix1 = Fix(file_path="a.py", old_text="x", new_text="y", description="first")
        fix2 = Fix(file_path="a.py", old_text="x", new_text="z", description="second")
        conflict = Conflict(fixes=[fix1, fix2], file_path="a.py", overlap_start=1, overlap_end=10)

        resolved = resolver.resolve(conflict, "last")

        assert len(resolved) == 1
        assert resolved[0] == fix2

    def test_resolve_highest_confidence(self, resolver: ConflictResolver):
        fix1 = Fix(file_path="a.py", old_text="x", new_text="y", description="d", confidence=0.8)
        fix2 = Fix(file_path="a.py", old_text="x", new_text="z", description="d", confidence=0.95)
        conflict = Conflict(fixes=[fix1, fix2], file_path="a.py", overlap_start=1, overlap_end=10)

        resolved = resolver.resolve(conflict, "highest_confidence")

        assert len(resolved) == 1
        assert resolved[0] == fix2

    def test_resolve_safest(self, resolver: ConflictResolver):
        fix1 = Fix(
            file_path="a.py", old_text="x", new_text="y", description="d", safety=FixSafety.UNSAFE
        )
        fix2 = Fix(
            file_path="a.py", old_text="x", new_text="z", description="d", safety=FixSafety.SAFE
        )
        conflict = Conflict(fixes=[fix1, fix2], file_path="a.py", overlap_start=1, overlap_end=10)

        resolved = resolver.resolve(conflict, "safest")

        assert len(resolved) == 1
        assert resolved[0] == fix2


class TestFixEngine:
    """Tests for FixEngine."""

    @pytest.fixture
    def repo(self, tmp_path: Path) -> Path:
        """Create a test repository."""
        (tmp_path / "src").mkdir()
        (tmp_path / "src" / "foo.py").write_text("def foo():\n    pass\n")
        (tmp_path / "src" / "bar.py").write_text("x = 1\ny = 2\n")
        return tmp_path

    @pytest.fixture
    def engine(self, repo: Path) -> FixEngine:
        return FixEngine(repo)

    def test_preview_single_fix(self, engine: FixEngine):
        fix = Fix(
            file_path="src/foo.py",
            old_text="def foo():",
            new_text="def foo() -> None:",
            description="Add return type",
        )

        diff = engine.preview([fix])

        assert "--- a/src/foo.py" in diff
        assert "+++ b/src/foo.py" in diff
        assert "-def foo():" in diff
        assert "+def foo() -> None:" in diff

    def test_preview_multiple_fixes(self, engine: FixEngine):
        fixes = [
            Fix(
                file_path="src/foo.py",
                old_text="def foo():",
                new_text="def foo() -> None:",
                description="d",
            ),
            Fix(
                file_path="src/bar.py",
                old_text="x = 1",
                new_text="x: int = 1",
                description="d",
            ),
        ]

        diff = engine.preview(fixes)

        assert "src/foo.py" in diff
        assert "src/bar.py" in diff

    async def test_apply_fix(self, engine: FixEngine, repo: Path):
        fix = Fix(
            file_path="src/foo.py",
            old_text="def foo():",
            new_text="def foo() -> None:",
            description="Add return type",
        )

        result = await engine.apply([fix], use_shadow_branch=False)

        assert result.success
        assert len(result.applied) == 1

        content = (repo / "src" / "foo.py").read_text()
        assert "def foo() -> None:" in content

    async def test_apply_fix_not_found(self, engine: FixEngine):
        fix = Fix(
            file_path="src/foo.py",
            old_text="nonexistent text",
            new_text="replacement",
            description="d",
        )

        result = await engine.apply([fix], use_shadow_branch=False)

        assert len(result.skipped) == 1
        assert "not found" in result.skipped[0][1].lower()

    async def test_apply_fix_file_not_found(self, engine: FixEngine):
        fix = Fix(
            file_path="nonexistent.py",
            old_text="x",
            new_text="y",
            description="d",
        )

        result = await engine.apply([fix], use_shadow_branch=False)

        assert len(result.errors) == 1
        assert isinstance(result.errors[0][1], FileNotFoundError)

    async def test_apply_with_conflict_resolution(self, engine: FixEngine, repo: Path):
        # Two fixes targeting the same text
        fixes = [
            Fix(
                file_path="src/foo.py",
                old_text="def foo():",
                new_text="def foo() -> None:",
                description="first",
                line_start=1,
            ),
            Fix(
                file_path="src/foo.py",
                old_text="def foo():",
                new_text="def bar():",
                description="second",
                line_start=1,
            ),
        ]

        result = await engine.apply(fixes, auto_resolve_conflicts=True, use_shadow_branch=False)

        # First fix should be applied, second skipped
        assert len(result.applied) == 1
        assert len(result.skipped) == 1


class TestCreateFix:
    """Tests for create_fix helper."""

    def test_creates_fix_with_classification(self):
        fix = create_fix(
            file_path="foo.py",
            old_text="def foo():",
            new_text="def foo() -> None:",
            description="Add return type",
        )

        assert fix.file_path == "foo.py"
        assert fix.safety == FixSafety.SAFE  # Type annotations are safe

    def test_creates_fix_with_metadata(self):
        fix = create_fix(
            file_path="foo.py",
            old_text="x",
            new_text="y",
            description="Fix",
            source="mypy",
            rule_id="error",
            line_start=10,
        )

        assert fix.source == "mypy"
        assert fix.rule_id == "error"
        assert fix.line_start == 10
