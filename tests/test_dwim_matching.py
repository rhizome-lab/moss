"""Comprehensive tests for DWIM tool matching behavior.

These tests define the expected matching behavior for common query patterns.
The goal is to ensure users can find tools naturally regardless of:
- Word order (todo list vs list todos)
- Separator style (todo-list vs todo_list vs todo list)
- Synonyms and related terms
- Typos and minor variations
"""

import pytest

from moss.dwim import analyze_intent

# Expected matches: (query, expected_tool, min_confidence)
# These define the contract for how DWIM should behave
EXACT_MATCH_CASES = [
    # Exact tool names (various formats)
    ("skeleton", "skeleton", 0.95),
    ("todo_list", "todo_list", 0.95),
    ("todo-list", "todo_list", 0.95),
    ("todo list", "todo_list", 0.95),
    ("deps", "deps", 0.95),
    ("cfg", "cfg", 0.95),
    ("anchors", "anchors", 0.95),
    ("query", "query", 0.95),
]

WORD_ORDER_CASES = [
    # Word order shouldn't matter much
    ("todo list", "todo_list", 0.90),
    ("list todo", "todo_list", 0.80),
    ("list todos", "todo_list", 0.80),
    ("todos list", "todo_list", 0.80),
    ("search todo", "todo_search", 0.80),
    ("todo search", "todo_search", 0.90),
]

NATURAL_LANGUAGE_CASES = [
    # Natural language queries
    ("show code structure", "skeleton", 0.60),
    ("what functions are in this file", "skeleton", 0.50),
    ("find all classes", "anchors", 0.50),
    ("show dependencies", "deps", 0.60),
    ("import graph", "deps", 0.50),
    ("control flow", "cfg", 0.60),
    ("complexity analysis", "complexity_analyze", 0.50),
    ("find todos", "todo_list", 0.50),
    ("show my todos", "todo_list", 0.50),
    ("list all todos", "todo_list", 0.60),
]

TYPO_CASES = [
    # Common typos should still match
    ("skelton", "skeleton", 0.70),  # missing 'e'
    ("skeletn", "skeleton", 0.70),  # missing 'o'
    ("depss", "deps", 0.70),  # extra 's'
    ("todoo", "todo_list", 0.50),  # extra 'o'
]

SYNONYM_CASES = [
    # Synonyms and related terms
    ("outline", "skeleton", 0.50),
    ("structure", "skeleton", 0.50),
    ("overview", "skeleton", 0.50),
    ("imports", "deps", 0.50),
    ("dependencies", "deps", 0.60),
    ("flow graph", "cfg", 0.50),
    ("branches", "cfg", 0.40),
]

NEGATIVE_CASES = [
    # These should NOT match the given tool with high confidence
    ("random gibberish xyz", None, 0.30),  # Should be below threshold
    ("make coffee", None, 0.30),
    ("send email", None, 0.30),
]

# Queries that include file paths or targets - should still match the tool
TARGET_IN_QUERY_CASES = [
    ("skeleton src/main.py", "skeleton", 0.90),
    ("skeleton src/foo/bar/baz.py", "skeleton", 0.90),
    ("deps package.json", "deps", 0.80),
    ("deps for package.json", "deps", 0.80),
    ("anchors in src/lib.rs", "anchors", 0.80),
    ("find anchors src/", "anchors", 0.70),
    ("summarize README.md", "search_summarize_file", 0.70),
    ("todo list TODO.md", "todo_list", 0.80),
    ("todos in TODO.md", "todo_list", 0.70),
    ("cfg main.py:process", "cfg", 0.80),
    ("complexity src/heavy.py", "complexity_analyze", 0.70),
]


class TestExactMatches:
    """Test exact tool name matching."""

    @pytest.mark.parametrize("query,expected_tool,min_confidence", EXACT_MATCH_CASES)
    def test_exact_match(self, query: str, expected_tool: str, min_confidence: float):
        results = analyze_intent(query)
        assert len(results) > 0, f"No results for query: {query}"
        top = results[0]
        assert top.tool == expected_tool, (
            f"Query '{query}' matched '{top.tool}' instead of '{expected_tool}'"
        )
        assert top.confidence >= min_confidence, (
            f"Query '{query}' confidence {top.confidence:.2f} < {min_confidence}"
        )


@pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
class TestWordOrder:
    """Test that word order doesn't significantly affect matching."""

    @pytest.mark.parametrize("query,expected_tool,min_confidence", WORD_ORDER_CASES)
    def test_word_order(self, query: str, expected_tool: str, min_confidence: float):
        results = analyze_intent(query)
        assert len(results) > 0, f"No results for query: {query}"

        # Check if expected tool is in top 3 results
        top_tools = [r.tool for r in results[:3]]
        assert expected_tool in top_tools, (
            f"Query '{query}' didn't have '{expected_tool}' in top 3. Got: {top_tools}"
        )

        # Find the match for expected tool
        match = next((r for r in results if r.tool == expected_tool), None)
        assert match is not None
        assert match.confidence >= min_confidence, (
            f"Query '{query}' confidence for '{expected_tool}' was "
            f"{match.confidence:.2f} < {min_confidence}"
        )


@pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
class TestNaturalLanguage:
    """Test natural language query understanding."""

    @pytest.mark.parametrize("query,expected_tool,min_confidence", NATURAL_LANGUAGE_CASES)
    def test_natural_language(self, query: str, expected_tool: str, min_confidence: float):
        results = analyze_intent(query)
        assert len(results) > 0, f"No results for query: {query}"

        # Check if expected tool is in top 5 results
        top_tools = [r.tool for r in results[:5]]
        assert expected_tool in top_tools, (
            f"Query '{query}' didn't have '{expected_tool}' in top 5. Got: {top_tools}"
        )

        match = next((r for r in results if r.tool == expected_tool), None)
        assert match is not None
        assert match.confidence >= min_confidence, (
            f"Query '{query}' confidence for '{expected_tool}' was "
            f"{match.confidence:.2f} < {min_confidence}"
        )


class TestTypoTolerance:
    """Test that typos are handled gracefully."""

    @pytest.mark.parametrize("query,expected_tool,min_confidence", TYPO_CASES)
    def test_typo(self, query: str, expected_tool: str, min_confidence: float):
        results = analyze_intent(query)
        assert len(results) > 0, f"No results for query: {query}"

        # Check if expected tool is in top 3 results
        top_tools = [r.tool for r in results[:3]]
        assert expected_tool in top_tools, (
            f"Typo query '{query}' didn't have '{expected_tool}' in top 3. Got: {top_tools}"
        )


@pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
class TestSynonyms:
    """Test synonym and related term matching."""

    @pytest.mark.parametrize("query,expected_tool,min_confidence", SYNONYM_CASES)
    def test_synonym(self, query: str, expected_tool: str, min_confidence: float):
        results = analyze_intent(query)
        assert len(results) > 0, f"No results for query: {query}"

        # Check if expected tool is in top 5 results
        top_tools = [r.tool for r in results[:5]]
        assert expected_tool in top_tools, (
            f"Synonym query '{query}' didn't have '{expected_tool}' in top 5. Got: {top_tools}"
        )


class TestNegativeCases:
    """Test that irrelevant queries don't get high confidence matches."""

    @pytest.mark.parametrize("query,expected_tool,max_confidence", NEGATIVE_CASES)
    def test_negative(self, query: str, expected_tool, max_confidence: float):
        results = analyze_intent(query)
        if not results:
            return  # No results is fine for gibberish

        top = results[0]
        assert top.confidence <= max_confidence, (
            f"Irrelevant query '{query}' got confidence {top.confidence:.2f} "
            f"for '{top.tool}' (expected <= {max_confidence})"
        )


@pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
class TestTargetInQuery:
    """Test queries that include file paths or targets."""

    @pytest.mark.parametrize("query,expected_tool,min_confidence", TARGET_IN_QUERY_CASES)
    def test_target_in_query(self, query: str, expected_tool: str, min_confidence: float):
        results = analyze_intent(query)
        assert len(results) > 0, f"No results for query: {query}"

        # Expected tool should be top result
        top = results[0]
        assert top.tool == expected_tool, (
            f"Query '{query}' matched '{top.tool}' instead of '{expected_tool}'"
        )
        assert top.confidence >= min_confidence, (
            f"Query '{query}' confidence {top.confidence:.2f} < {min_confidence}"
        )


class TestConsistency:
    """Test matching consistency across equivalent queries."""

    def test_separator_equivalence(self):
        """Different separators should give same results."""
        queries = ["todo_list", "todo-list", "todo list"]
        results = [analyze_intent(q) for q in queries]

        # All should have same top tool
        top_tools = [r[0].tool for r in results if r]
        assert len(set(top_tools)) == 1, f"Inconsistent top tools: {top_tools}"

    def test_case_insensitivity(self):
        """Matching should be case-insensitive."""
        queries = ["skeleton", "Skeleton", "SKELETON", "SkElEtOn"]
        results = [analyze_intent(q) for q in queries]

        top_tools = [r[0].tool for r in results if r]
        assert len(set(top_tools)) == 1, f"Case sensitivity issue: {top_tools}"

    def test_plural_equivalence(self):
        """Singular and plural should match similarly."""
        pairs = [
            ("todo", "todos"),
            ("dependency", "dependencies"),
            ("anchor", "anchors"),
        ]
        for singular, plural in pairs:
            r1 = analyze_intent(singular)
            r2 = analyze_intent(plural)
            if r1 and r2:
                # Top tools should be related (same base or similar)
                t1, t2 = r1[0].tool, r2[0].tool
                # At minimum, same prefix
                assert t1.split("_")[0] == t2.split("_")[0] or t1 == t2, (
                    f"'{singular}' -> '{t1}' but '{plural}' -> '{t2}'"
                )
