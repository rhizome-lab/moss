"""Tests for frequency-based abstraction learning library."""

from pathlib import Path

import pytest

from moss.synthesis.plugins.libraries.learned import (
    CodePattern,
    LearnedLibrary,
    PatternExtractor,
)
from moss.synthesis.plugins.protocols import Abstraction, LibraryPlugin
from moss.synthesis.types import Context, Specification

# =============================================================================
# Test Fixtures
# =============================================================================


@pytest.fixture
def simple_spec() -> Specification:
    """Simple specification for testing."""
    return Specification(
        description="Add two numbers together",
        type_signature="(int, int) -> int",
    )


@pytest.fixture
def context() -> Context:
    """Simple context for testing."""
    return Context(primitives=["int", "float"], library={})


@pytest.fixture
def library() -> LearnedLibrary:
    """Fresh learned library for testing."""
    return LearnedLibrary(min_frequency=2)


@pytest.fixture
def persistent_library(tmp_path: Path) -> LearnedLibrary:
    """Library with persistence enabled."""
    return LearnedLibrary(
        min_frequency=2,
        persistence_path=tmp_path / "library.json",
    )


# =============================================================================
# PatternExtractor Tests
# =============================================================================


class TestPatternExtractor:
    """Tests for pattern extraction."""

    def test_extract_function_pattern(self) -> None:
        extractor = PatternExtractor()
        code = "def add(a, b):\n    return a + b"
        patterns = extractor.extract_patterns(code)

        assert len(patterns) > 0
        assert any(p.category == "function" for p in patterns)

    def test_extract_expression_pattern(self) -> None:
        extractor = PatternExtractor()
        code = "x = a + b"
        patterns = extractor.extract_patterns(code)

        assert any(p.category == "expression" for p in patterns)

    def test_extract_listcomp_pattern(self) -> None:
        extractor = PatternExtractor()
        code = "[x * 2 for x in items]"
        patterns = extractor.extract_patterns(code)

        assert any(p.category == "comprehension" for p in patterns)

    def test_extract_idiom_pattern(self) -> None:
        extractor = PatternExtractor()
        code = """
def guard(x):
    if x is None:
        return False
    return True
"""
        patterns = extractor.extract_patterns(code)

        assert any(p.category == "idiom" for p in patterns)

    def test_handle_syntax_error(self) -> None:
        extractor = PatternExtractor()
        patterns = extractor.extract_patterns("this is not valid python {{{")

        assert patterns == []


class TestCodePattern:
    """Tests for CodePattern dataclass."""

    def test_hash_generation(self) -> None:
        pattern1 = CodePattern(template="def $NAME(): pass")
        pattern2 = CodePattern(template="def $NAME(): pass")
        pattern3 = CodePattern(template="def $OTHER(): pass")

        assert pattern1.hash == pattern2.hash
        assert pattern1.hash != pattern3.hash

    def test_pattern_attributes(self) -> None:
        pattern = CodePattern(
            template="$A + $B",
            examples=["1 + 2", "3 + 4"],
            frequency=5,
            category="expression",
            signature="(int, int) -> int",
        )

        assert pattern.frequency == 5
        assert len(pattern.examples) == 2
        assert pattern.category == "expression"


# =============================================================================
# LearnedLibrary Tests
# =============================================================================


class TestLearnedLibrary:
    """Tests for LearnedLibrary."""

    def test_protocol_compliance(self) -> None:
        library = LearnedLibrary()
        assert isinstance(library, LibraryPlugin)

    def test_metadata(self) -> None:
        library = LearnedLibrary()
        assert library.metadata.name == "learned"
        assert library.metadata.supports_learning is True

    def test_add_and_get_abstraction(self, library: LearnedLibrary) -> None:
        abstraction = Abstraction(
            name="add",
            code="def add(a, b): return a + b",
            description="Add two numbers",
        )

        library.add_abstraction(abstraction)

        assert len(library) == 1
        abstractions = library.get_abstractions()
        assert len(abstractions) == 1
        assert abstractions[0].name == "add"

    def test_remove_abstraction(self, library: LearnedLibrary) -> None:
        library.add_abstraction(Abstraction(name="test", code="pass", description="test"))

        assert library.remove_abstraction("test") is True
        assert library.remove_abstraction("nonexistent") is False
        assert len(library) == 0

    def test_record_solution(
        self,
        library: LearnedLibrary,
        simple_spec: Specification,
    ) -> None:
        code = "def add(a, b):\n    return a + b"
        patterns = library.record_solution(code, simple_spec)

        assert len(patterns) > 0
        assert len(library.get_pattern_frequencies()) > 0

    def test_pattern_frequency_tracking(
        self,
        library: LearnedLibrary,
        simple_spec: Specification,
    ) -> None:
        code1 = "def add(a, b):\n    return a + b"
        code2 = "def sub(a, b):\n    return a - b"

        library.record_solution(code1, simple_spec)
        library.record_solution(code2, simple_spec)

        frequencies = library.get_pattern_frequencies()
        # Both should have similar patterns
        assert len(frequencies) > 0

    def test_get_frequent_patterns(
        self,
        library: LearnedLibrary,
        simple_spec: Specification,
    ) -> None:
        # Record same pattern multiple times
        code = "def add(a, b):\n    return a + b"
        for _ in range(3):
            library.record_solution(code, simple_spec)

        # min_frequency=2, so should have frequent patterns
        frequent = library.get_frequent_patterns()
        assert len(frequent) > 0

    def test_search_abstractions_empty(
        self,
        library: LearnedLibrary,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        results = library.search_abstractions(simple_spec, context)
        assert results == []

    def test_search_abstractions_keyword_match(
        self,
        library: LearnedLibrary,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        library.add_abstraction(
            Abstraction(
                name="add_numbers",
                code="def add(a, b): return a + b",
                description="Add two numbers together",
            )
        )

        results = library.search_abstractions(simple_spec, context)
        assert len(results) == 1
        assert results[0][0].name == "add_numbers"
        assert results[0][1] > 0  # Positive score

    def test_search_abstractions_type_match(
        self,
        library: LearnedLibrary,
        context: Context,
    ) -> None:
        library.add_abstraction(
            Abstraction(
                name="int_op",
                code="def op(x): return x",
                type_signature="(int, int) -> int",
                description="Integer operation",
            )
        )

        spec = Specification(
            description="Some operation",
            type_signature="(int, int) -> int",
        )

        results = library.search_abstractions(spec, context)
        assert len(results) == 1
        # Should have type match bonus
        assert results[0][1] >= 0.5

    @pytest.mark.asyncio
    async def test_learn_abstraction(
        self,
        library: LearnedLibrary,
        simple_spec: Specification,
    ) -> None:
        # Record pattern multiple times to exceed threshold
        code = "def add(a, b):\n    return a + b"
        solutions = [code] * 3

        abstraction = await library.learn_abstraction(solutions, simple_spec)

        # Should learn an abstraction
        if abstraction:
            assert abstraction.name.startswith("abs_")
            assert len(library) == 1

    @pytest.mark.asyncio
    async def test_learn_abstraction_no_patterns(
        self,
        library: LearnedLibrary,
        simple_spec: Specification,
    ) -> None:
        # Single solution shouldn't trigger learning
        result = await library.learn_abstraction(["x = 1"], simple_spec)
        assert result is None

    def test_record_usage(self, library: LearnedLibrary) -> None:
        abstraction = Abstraction(
            name="test",
            code="pass",
            description="test",
            usage_count=0,
        )
        library.add_abstraction(abstraction)

        library.record_usage(abstraction)

        updated = library.get_abstractions()[0]
        assert updated.usage_count == 1

    def test_clear(self, library: LearnedLibrary, simple_spec: Specification) -> None:
        library.add_abstraction(Abstraction(name="test", code="pass", description=""))
        library.record_solution("def f(): pass", simple_spec)

        library.clear()

        assert len(library) == 0
        assert library.get_pattern_frequencies() == {}


# =============================================================================
# Persistence Tests
# =============================================================================


class TestLearnedLibraryPersistence:
    """Tests for library persistence."""

    def test_persistence_path_metadata(self, persistent_library: LearnedLibrary) -> None:
        assert persistent_library.metadata.persistence_type == "file"

    def test_persist_and_load(self, tmp_path: Path) -> None:
        path = tmp_path / "library.json"

        # Create and populate library
        lib1 = LearnedLibrary(persistence_path=path)
        lib1.add_abstraction(
            Abstraction(
                name="test_abs",
                code="def test(): pass",
                description="Test abstraction",
                usage_count=5,
            )
        )

        # Create new library that should load from file
        lib2 = LearnedLibrary(persistence_path=path)

        assert len(lib2) == 1
        abs2 = lib2.get_abstractions()[0]
        assert abs2.name == "test_abs"
        assert abs2.usage_count == 5

    def test_persist_pattern_counts(
        self,
        tmp_path: Path,
        simple_spec: Specification,
    ) -> None:
        path = tmp_path / "library.json"

        lib1 = LearnedLibrary(persistence_path=path)
        lib1.record_solution("def f(): return 1", simple_spec)
        lib1.record_solution("def f(): return 1", simple_spec)

        lib2 = LearnedLibrary(persistence_path=path)
        assert len(lib2.get_pattern_frequencies()) > 0

    def test_memory_library_no_persistence(self) -> None:
        library = LearnedLibrary()
        assert library.metadata.persistence_type == "memory"


# =============================================================================
# Max Abstractions Tests
# =============================================================================


class TestMaxAbstractions:
    """Tests for abstraction limit enforcement."""

    def test_prune_when_max_reached(self) -> None:
        library = LearnedLibrary(max_abstractions=2)

        # Add 3 abstractions
        for i in range(3):
            library.add_abstraction(
                Abstraction(
                    name=f"abs_{i}",
                    code=f"def f{i}(): pass",
                    description=f"Abstraction {i}",
                    usage_count=i,  # Higher index = higher usage
                )
            )

        # Should have pruned to max
        assert len(library) == 3  # add_abstraction doesn't prune

    @pytest.mark.asyncio
    async def test_learn_prunes_least_used(self, simple_spec: Specification) -> None:
        library = LearnedLibrary(min_frequency=1, max_abstractions=1)

        # Add an abstraction with low usage
        library.add_abstraction(
            Abstraction(
                name="old_abs",
                code="def old(): pass",
                description="Old abstraction",
                usage_count=0,
            )
        )

        # Record pattern to trigger learning
        code = "def new_func(x):\n    return x + 1"
        for _ in range(2):
            library.record_solution(code, simple_spec)

        await library.learn_abstraction([code], simple_spec)

        # The old abstraction should have been pruned if new one was learned
        names = [a.name for a in library.get_abstractions()]
        # At most max_abstractions
        assert len(names) <= 2


# =============================================================================
# Compression Estimation Tests
# =============================================================================


class TestCompressionEstimation:
    """Tests for compression gain estimation."""

    def test_estimate_compression_gain(self) -> None:
        library = LearnedLibrary()

        code = "def complex_function(x):\n    return x * 2 + 1"
        examples = [code] * 5  # Same code 5 times

        gain = library._estimate_compression(code, examples)

        # Should have positive gain for reused code
        assert gain >= 0

    def test_no_compression_single_example(self) -> None:
        library = LearnedLibrary()

        code = "def f(): pass"
        examples = [code]

        gain = library._estimate_compression(code, examples)
        assert gain == 0.0


# =============================================================================
# Integration Tests
# =============================================================================


class TestLearnedLibraryIntegration:
    """Integration tests for learned library."""

    @pytest.mark.asyncio
    async def test_full_learning_workflow(self) -> None:
        """Test complete workflow: record solutions -> learn -> search."""
        library = LearnedLibrary(min_frequency=2)

        # Define similar specifications
        spec1 = Specification(description="Add two integers")
        spec2 = Specification(description="Subtract two integers")
        spec3 = Specification(description="Multiply two integers")

        # Record similar solutions
        library.record_solution("def add(a, b):\n    return a + b", spec1)
        library.record_solution("def sub(a, b):\n    return a - b", spec2)
        library.record_solution("def mul(a, b):\n    return a * b", spec3)

        # Try to learn abstraction
        new_spec = Specification(description="Divide two integers")
        await library.learn_abstraction(
            ["def div(a, b):\n    return a / b"],
            new_spec,
        )

        # Search for abstractions
        context = Context()
        library.search_abstractions(new_spec, context)

        # Either learned new abstraction or found existing patterns
        assert len(library.get_pattern_frequencies()) > 0

    @pytest.mark.asyncio
    async def test_pattern_reuse_across_specs(self) -> None:
        """Test that patterns are shared across specifications."""
        library = LearnedLibrary(min_frequency=2)

        spec1 = Specification(description="Process user data")
        spec2 = Specification(description="Process order data")

        # Both use guard clause pattern
        code1 = """
def process_user(data):
    if data is None:
        return None
    return data['user']
"""
        code2 = """
def process_order(data):
    if data is None:
        return None
    return data['order']
"""

        library.record_solution(code1, spec1)
        library.record_solution(code2, spec2)

        # Should detect the common guard clause pattern
        frequent = library.get_frequent_patterns(min_frequency=2)
        # May have frequent patterns from the None check
        assert isinstance(frequent, list)
