"""Tests for DWIM (Do What I Mean) semantic tool routing."""

from moss.dwim import (
    ToolMatch,
    ToolRouter,
    analyze_intent,
    get_tool_info,
    list_tools,
    normalize_parameters,
    resolve_parameter,
    resolve_tool,
    string_similarity,
    suggest_tool,
)


class TestFuzzyMatching:
    """Tests for fuzzy string matching."""

    def test_string_similarity_exact(self):
        """Test exact match similarity."""
        assert string_similarity("skeleton", "skeleton") == 1.0

    def test_string_similarity_similar(self):
        """Test similar strings have high similarity."""
        sim = string_similarity("skeleton", "skelton")  # typo
        assert sim > 0.8

    def test_string_similarity_different(self):
        """Test different strings have low similarity."""
        sim = string_similarity("skeleton", "dependencies")
        assert sim < 0.5


class TestToolResolution:
    """Tests for tool name resolution."""

    def test_resolve_exact_match(self):
        """Test resolving exact tool name."""
        match = resolve_tool("skeleton")
        assert match.tool == "skeleton"
        assert match.confidence == 1.0

    def test_resolve_alias(self):
        """Test resolving tool alias."""
        match = resolve_tool("structure")
        assert match.tool == "skeleton"  # structure → skeleton (code structure)
        assert match.confidence == 1.0

        match = resolve_tool("imports")
        assert match.tool == "deps"

    def test_resolve_typo(self):
        """Test resolving typo in tool name."""
        match = resolve_tool("skelton")  # missing 'e'
        assert match.tool == "skeleton"
        assert match.confidence > 0.8

    def test_resolve_unknown(self):
        """Test resolving completely unknown tool."""
        match = resolve_tool("xyzabc123")
        assert match.confidence < 0.5

    def test_resolve_parameter(self):
        """Test parameter name resolution."""
        assert resolve_parameter("file") == "path"
        assert resolve_parameter("file_path") == "path"
        assert resolve_parameter("glob") == "pattern"
        assert resolve_parameter("base") == "inherits"
        assert resolve_parameter("unknown") == "unknown"

    def test_normalize_parameters(self):
        """Test normalizing parameter dict."""
        params = {"file": "test.py", "glob": "**/*.py"}
        normalized = normalize_parameters(params)
        assert normalized == {"path": "test.py", "pattern": "**/*.py"}


class TestToolRouter:
    """Tests for the ToolRouter class."""

    def test_router_initialization(self):
        """Test router initializes correctly."""
        router = ToolRouter()
        # Router should be able to analyze intent
        matches = router.analyze_intent("test")
        assert isinstance(matches, list)

    def test_analyze_intent_skeleton(self):
        """Test analyzing intent for skeleton."""
        router = ToolRouter()
        matches = router.analyze_intent("show code structure")
        assert len(matches) > 0
        # structure-related tools should be in top results
        tool_names = [m.tool for m in matches[:5]]
        structure_tools = {
            "skeleton",
            "context",
            "search_summarize_module",
            "health_analyze_structure",
        }
        assert any(t in structure_tools for t in tool_names)

    def test_analyze_intent_deps(self):
        """Test analyzing intent for dependencies."""
        router = ToolRouter()
        matches = router.analyze_intent("find imports and dependencies")
        assert len(matches) > 0
        # Should find dependency-related tools in top results
        tool_names = [m.tool for m in matches[:5]]
        dep_related = {
            "deps",
            "dependencies_extract",
            "dependencies_format",
            "dependencies_analyze",
        }
        assert any(t in dep_related for t in tool_names), f"Got: {tool_names}"

    def test_analyze_intent_query(self):
        """Test analyzing intent for query."""
        router = ToolRouter()
        matches = router.analyze_intent("search for classes that inherit from Base")
        assert len(matches) > 0
        # query should be in top results
        tool_names = [m.tool for m in matches[:3]]
        assert "query" in tool_names

    def test_analyze_intent_cfg(self):
        """Test analyzing intent for CFG."""
        router = ToolRouter()
        matches = router.analyze_intent("show control flow graph")
        assert len(matches) > 0
        tool_names = [m.tool for m in matches[:3]]
        assert "cfg" in tool_names

    def test_suggest_tool(self):
        """Test tool suggestion."""
        router = ToolRouter()
        # Use a query that strongly matches tool keywords
        suggestion = router.suggest_tool("show classes functions methods structure outline")
        assert suggestion is not None
        assert suggestion.tool in ("skeleton", "anchors", "context")


class TestModuleFunctions:
    """Tests for module-level convenience functions."""

    def test_analyze_intent(self):
        """Test analyze_intent function."""
        matches = analyze_intent("show file overview")
        assert len(matches) > 0
        assert all(isinstance(m, ToolMatch) for m in matches)

    def test_suggest_tool(self):
        """Test suggest_tool function."""
        suggestion = suggest_tool("find all methods")
        assert suggestion is None or isinstance(suggestion, ToolMatch)

    def test_list_tools(self):
        """Test list_tools function."""
        tools = list_tools()
        assert len(tools) > 0
        assert all("name" in t for t in tools)
        assert all("description" in t for t in tools)

    def test_get_tool_info(self):
        """Test get_tool_info function."""
        info = get_tool_info("skeleton")
        assert info is not None
        assert info["name"] == "skeleton"
        assert "description" in info
        assert "aliases" in info

        # Should also work with alias
        info = get_tool_info("structure")
        assert info is not None
        assert info["name"] == "skeleton"  # structure → skeleton

        # Unknown tool should return None
        info = get_tool_info("xyzunknown123")
        assert info is None


class TestEdgeCases:
    """Tests for edge cases."""

    def test_empty_query(self):
        """Test handling empty query."""
        matches = analyze_intent("")
        # Should still return results (but low confidence)
        assert isinstance(matches, list)

    def test_long_query(self):
        """Test handling very long query."""
        query = "I need to " + "find and analyze " * 100 + "the code structure"
        matches = analyze_intent(query)
        assert isinstance(matches, list)

    def test_special_characters_in_query(self):
        """Test handling special characters."""
        matches = analyze_intent("find def __init__(self):")
        assert isinstance(matches, list)

    def test_available_tools_filter(self):
        """Test filtering available tools."""
        matches = analyze_intent("find code", available_tools=["skeleton", "deps"])
        tool_names = {m.tool for m in matches}
        assert tool_names <= {"skeleton", "deps"}
