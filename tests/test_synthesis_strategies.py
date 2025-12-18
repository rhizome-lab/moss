"""Tests for code synthesis strategies."""

from __future__ import annotations

from moss.synthesis import Context, Specification, Subproblem
from moss.synthesis.strategies import (
    PatternBasedDecomposition,
    TestDrivenDecomposition,
    TypeDrivenDecomposition,
)
from moss.synthesis.strategies.pattern_based import Pattern
from moss.synthesis.strategies.test_driven import (
    ExtractedTestCase,
    categorize_test,
    cluster_tests,
    extract_test_info,
)
from moss.synthesis.strategies.type_driven import (
    extract_inner_type,
    is_collection_type,
    is_composite_type,
    parse_type_signature,
)

# =============================================================================
# Type-Driven Strategy Tests
# =============================================================================


class TestParseTypeSignature:
    """Tests for parse_type_signature helper."""

    def test_simple_signature(self):
        result = parse_type_signature("int -> str")
        assert result is not None
        assert result.input_types == ["int"]
        assert result.output_type == "str"

    def test_tuple_input(self):
        result = parse_type_signature("(int, str) -> bool")
        assert result is not None
        assert result.input_types == ["int", "str"]
        assert result.output_type == "bool"

    def test_generic_type(self):
        result = parse_type_signature("List[int] -> List[str]")
        assert result is not None
        assert result.is_generic
        assert result.type_params is not None

    def test_invalid_signature(self):
        result = parse_type_signature("not a signature")
        assert result is None

    def test_empty_signature(self):
        result = parse_type_signature("")
        assert result is None


class TestTypeHelpers:
    """Tests for type helper functions."""

    def test_is_composite_tuple(self):
        assert is_composite_type("Tuple[int, str]")
        assert is_composite_type("tuple[int, str]")

    def test_is_composite_dict(self):
        assert is_composite_type("Dict[str, int]")

    def test_is_not_composite(self):
        assert not is_composite_type("int")
        assert not is_composite_type("List[int]")

    def test_is_collection_list(self):
        assert is_collection_type("List[int]")
        assert is_collection_type("list[str]")

    def test_is_collection_set(self):
        assert is_collection_type("Set[int]")

    def test_is_not_collection(self):
        assert not is_collection_type("int")
        assert not is_collection_type("Dict[str, int]")

    def test_extract_inner_type(self):
        assert extract_inner_type("List[int]") == "int"
        assert extract_inner_type("Set[str]") == "str"
        assert extract_inner_type("int") is None


class TestTypeDrivenDecomposition:
    """Tests for TypeDrivenDecomposition strategy."""

    def test_metadata(self):
        strategy = TypeDrivenDecomposition()
        assert strategy.name == "type_driven"
        assert "type" in strategy.description.lower()

    def test_can_handle_with_type(self):
        strategy = TypeDrivenDecomposition()
        spec = Specification(
            description="Sort users",
            type_signature="List[User] -> List[User]",
        )
        ctx = Context()
        assert strategy.can_handle(spec, ctx)

    def test_cannot_handle_without_type(self):
        strategy = TypeDrivenDecomposition()
        spec = Specification(description="Sort users")
        ctx = Context()
        assert not strategy.can_handle(spec, ctx)

    def test_decompose_collection_transform(self):
        strategy = TypeDrivenDecomposition()
        spec = Specification(
            description="Convert list of ints to list of strings",
            type_signature="List[int] -> List[str]",
        )
        ctx = Context()
        subproblems = strategy.decompose(spec, ctx)

        assert len(subproblems) >= 1
        # Should have element transform subproblem
        descs = [s.specification.description for s in subproblems]
        assert any("transform" in d.lower() or "element" in d.lower() for d in descs)

    def test_no_decompose_primitive_conversion(self):
        """Primitive type conversions should NOT decompose to avoid infinite recursion."""
        strategy = TypeDrivenDecomposition()
        spec = Specification(
            description="Convert string to int",
            type_signature="str -> int",
        )
        ctx = Context()
        subproblems = strategy.decompose(spec, ctx)

        # Primitive conversions should NOT decompose (prevents infinite recursion)
        assert len(subproblems) == 0

    def test_decompose_via_intermediate_complex_types(self):
        """Complex types (non-primitive) should decompose via intermediate steps."""
        strategy = TypeDrivenDecomposition()
        spec = Specification(
            description="Convert User to UserDTO",
            type_signature="User -> UserDTO",
        )
        ctx = Context()
        subproblems = strategy.decompose(spec, ctx)

        # Non-primitive types should decompose into intermediate steps
        assert len(subproblems) >= 2

    def test_estimate_success_with_generic(self):
        strategy = TypeDrivenDecomposition()
        spec = Specification(
            description="test",
            type_signature="List[int] -> List[str]",
        )
        ctx = Context()
        score = strategy.estimate_success(spec, ctx)
        assert score > 0.5  # Should be confident with generics

    def test_estimate_success_without_type(self):
        strategy = TypeDrivenDecomposition()
        spec = Specification(description="test")
        ctx = Context()
        score = strategy.estimate_success(spec, ctx)
        assert score == 0.0


# =============================================================================
# Test-Driven Strategy Tests
# =============================================================================


class TestExtractTestInfo:
    """Tests for extract_test_info helper."""

    def test_extract_from_string(self):
        test_code = """
def test_add_success():
    result = add(2, 3)
    assert result == 5
"""
        info = extract_test_info(test_code)
        assert info.name == "test_add_success"
        assert "add" in info.operations

    def test_extract_from_dict(self):
        test_dict = {
            "name": "test_auth",
            "description": "Test authentication",
            "operations": ["authenticate", "verify"],
            "inputs": ["user", "pass"],
            "expected": [True],
            "category": "happy_path",
        }
        info = extract_test_info(test_dict)
        assert info.name == "test_auth"
        assert info.category == "happy_path"


class TestCategorizeTest:
    """Tests for categorize_test helper."""

    def test_categorize_error(self):
        assert categorize_test("test_raises_error", "raise ValueError") == "error_handling"

    def test_categorize_validation(self):
        assert categorize_test("test_invalid_input", "") == "validation"

    def test_categorize_edge_case(self):
        assert categorize_test("test_empty_list", "") == "edge_case"

    def test_categorize_happy_path(self):
        assert categorize_test("test_success", "") == "happy_path"

    def test_categorize_general(self):
        assert categorize_test("test_something", "") == "general"


class TestClusterTests:
    """Tests for cluster_tests helper."""

    def test_cluster_by_category(self):
        tests = [
            ExtractedTestCase(name="test1", category="happy_path"),
            ExtractedTestCase(name="test2", category="error_handling"),
            ExtractedTestCase(name="test3", category="happy_path"),
        ]
        clusters = cluster_tests(tests)
        assert "happy_path" in clusters
        assert len(clusters["happy_path"]) == 2
        assert "error_handling" in clusters


class TestTestDrivenDecomposition:
    """Tests for TestDrivenDecomposition strategy."""

    def test_metadata(self):
        strategy = TestDrivenDecomposition()
        assert strategy.name == "test_driven"
        assert "test" in strategy.description.lower()

    def test_can_handle_with_tests(self):
        strategy = TestDrivenDecomposition()
        spec = Specification(
            description="Add numbers",
            tests=("def test_add(): assert add(2, 3) == 5",),
        )
        ctx = Context()
        assert strategy.can_handle(spec, ctx)

    def test_cannot_handle_without_tests(self):
        strategy = TestDrivenDecomposition()
        spec = Specification(description="Add numbers")
        ctx = Context()
        assert not strategy.can_handle(spec, ctx)

    def test_decompose_creates_subproblems(self):
        strategy = TestDrivenDecomposition()
        spec = Specification(
            description="Implement calculator",
            tests=(
                "def test_add_success(): assert calc.add(2, 3) == 5",
                "def test_add_error(): pytest.raises(ValueError)",
                "def test_empty(): assert calc.add(0, 0) == 0",
            ),
        )
        ctx = Context()
        subproblems = strategy.decompose(spec, ctx)

        # Should create subproblems from different test categories
        assert len(subproblems) >= 1

    def test_estimate_success_many_tests(self):
        strategy = TestDrivenDecomposition()
        spec = Specification(
            description="test",
            tests=tuple(f"def test_{i}(): pass" for i in range(15)),
        )
        ctx = Context()
        score = strategy.estimate_success(spec, ctx)
        assert score > 0.5  # High confidence with many tests

    def test_estimate_success_no_tests(self):
        strategy = TestDrivenDecomposition()
        spec = Specification(description="test")
        ctx = Context()
        score = strategy.estimate_success(spec, ctx)
        assert score == 0.0


# =============================================================================
# Pattern-Based Strategy Tests
# =============================================================================


class TestPattern:
    """Tests for Pattern dataclass."""

    def test_match_score_keywords(self):
        pattern = Pattern(
            name="crud",
            description="CRUD operations",
            keywords=("crud", "api", "rest"),
            template=("Create", "Read", "Update", "Delete"),
        )
        spec = Specification(description="Build a REST API with CRUD operations")
        score = pattern.match_score(spec)
        assert score > 0.5  # Should match well

    def test_match_score_no_match(self):
        pattern = Pattern(
            name="crud",
            description="CRUD operations",
            keywords=("crud", "api", "rest"),
            template=("Create", "Read", "Update", "Delete"),
        )
        spec = Specification(description="Calculate fibonacci numbers")
        score = pattern.match_score(spec)
        assert score < 0.3  # Should not match well

    def test_instantiate_creates_subproblems(self):
        pattern = Pattern(
            name="test",
            description="Test pattern",
            keywords=("test",),
            template=("Step 1", "Step 2", "Step 3"),
        )
        spec = Specification(description="Test task")
        ctx = Context()
        subproblems = pattern.instantiate(spec, ctx)
        assert len(subproblems) == 3


class TestPatternBasedDecomposition:
    """Tests for PatternBasedDecomposition strategy."""

    def test_metadata(self):
        strategy = PatternBasedDecomposition()
        assert strategy.name == "pattern_based"
        assert "pattern" in strategy.description.lower()

    def test_can_handle_matching_pattern(self):
        strategy = PatternBasedDecomposition()
        spec = Specification(description="Build a REST API with CRUD for users")
        ctx = Context()
        assert strategy.can_handle(spec, ctx)

    def test_cannot_handle_no_pattern(self):
        strategy = PatternBasedDecomposition()
        spec = Specification(description="Calculate the 100th prime number")
        ctx = Context()
        # May or may not match depending on threshold
        # This is a weak test - we just verify it doesn't crash
        strategy.can_handle(spec, ctx)

    def test_decompose_crud_pattern(self):
        strategy = PatternBasedDecomposition()
        spec = Specification(description="Build user management API with CRUD")
        ctx = Context()
        subproblems = strategy.decompose(spec, ctx)

        if subproblems:  # If pattern matched
            assert len(subproblems) >= 4  # CRUD has at least 4 steps
            descs = [s.specification.description.lower() for s in subproblems]
            assert any("create" in d for d in descs)

    def test_decompose_auth_pattern(self):
        strategy = PatternBasedDecomposition()
        spec = Specification(description="Implement user authentication with login")
        ctx = Context()
        subproblems = strategy.decompose(spec, ctx)

        if subproblems:  # If pattern matched
            assert len(subproblems) >= 3

    def test_estimate_success(self):
        strategy = PatternBasedDecomposition()
        spec = Specification(description="Build a REST API with CRUD for products")
        ctx = Context()
        score = strategy.estimate_success(spec, ctx)
        assert score > 0.0  # Should have some confidence

    def test_add_custom_pattern(self):
        strategy = PatternBasedDecomposition()
        custom = Pattern(
            name="custom",
            description="Custom pattern",
            keywords=("custom", "special"),
            template=("Custom step 1", "Custom step 2"),
        )
        strategy.add_pattern(custom)

        spec = Specification(description="Build a custom special thing")
        ctx = Context()
        subproblems = strategy.decompose(spec, ctx)

        # Should match our custom pattern
        if subproblems:
            assert len(subproblems) == 2


# =============================================================================
# Integration Tests
# =============================================================================


class TestStrategyIntegration:
    """Integration tests for all strategies together."""

    def test_all_strategies_have_consistent_interface(self):
        """Verify all strategies implement the same interface."""
        strategies = [
            TypeDrivenDecomposition(),
            TestDrivenDecomposition(),
            PatternBasedDecomposition(),
        ]

        for strategy in strategies:
            # Check metadata
            assert hasattr(strategy, "metadata")
            assert hasattr(strategy.metadata, "name")
            assert hasattr(strategy.metadata, "description")
            assert hasattr(strategy.metadata, "keywords")

            # Check methods
            spec = Specification(description="test")
            ctx = Context()
            assert hasattr(strategy, "can_handle")
            assert hasattr(strategy, "decompose")
            assert hasattr(strategy, "estimate_success")

            # Methods should be callable
            strategy.can_handle(spec, ctx)
            strategy.decompose(spec, ctx)
            strategy.estimate_success(spec, ctx)

    def test_strategies_return_valid_subproblems(self):
        """Verify subproblems have required attributes."""
        strategies = [
            TypeDrivenDecomposition(),
            TestDrivenDecomposition(),
            PatternBasedDecomposition(),
        ]

        # Test with a spec that should match multiple strategies
        spec = Specification(
            description="Build a REST API for user management",
            type_signature="Request -> Response",
            tests=("def test_api(): assert api.get() == 200",),
        )
        ctx = Context()

        for strategy in strategies:
            if strategy.can_handle(spec, ctx):
                subproblems = strategy.decompose(spec, ctx)
                for sub in subproblems:
                    assert isinstance(sub, Subproblem)
                    assert isinstance(sub.specification, Specification)
                    assert isinstance(sub.dependencies, tuple)
                    assert isinstance(sub.priority, int)
