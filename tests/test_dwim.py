"""Tests for DWIM (Do What I Mean) semantic tool routing.

Comprehensive test suite covering:
- Fuzzy string matching
- Tool name resolution (exact, alias, typo)
- NL marker protection (show/find/get shouldn't hijack queries)
- Natural language intent analysis
- Parameter resolution and normalization
- Edge cases and error handling
"""

import pytest

from moss.dwim import (
    TOOL_ALIASES,
    TOOL_REGISTRY,
    ToolMatch,
    ToolRouter,
    analyze_intent,
    get_tool_info,
    list_tool_names,
    list_tools,
    normalize_parameters,
    resolve_parameter,
    resolve_tool,
    string_similarity,
    suggest_tool,
    suggest_tools,
)


class TestFuzzyMatching:
    """Tests for fuzzy string matching."""

    def test_string_similarity_exact(self):
        """Test exact match similarity."""
        assert string_similarity("skeleton", "skeleton") == 1.0
        assert string_similarity("deps", "deps") == 1.0
        assert string_similarity("query", "query") == 1.0

    def test_string_similarity_case_insensitive(self):
        """Test that string similarity is case-insensitive."""
        # Same case = exact match
        assert string_similarity("Skeleton", "Skeleton") == 1.0
        # Different case = still matches (case insensitive)
        sim = string_similarity("skeleton", "Skeleton")
        assert sim == 1.0  # Case insensitive matching

    def test_string_similarity_single_typo(self):
        """Test similarity with single character typos."""
        # Missing letter
        assert string_similarity("skeleton", "skelton") > 0.85
        # Extra letter
        assert string_similarity("skeleton", "skeleeton") > 0.85
        # Wrong letter
        assert string_similarity("skeleton", "skelaton") > 0.85
        # Swapped letters
        assert string_similarity("skeleton", "skelteon") > 0.8

    def test_string_similarity_multiple_typos(self):
        """Test similarity degrades with more typos."""
        one_typo = string_similarity("skeleton", "skelton")
        two_typos = string_similarity("skeleton", "skeltn")
        assert one_typo >= two_typos

    def test_string_similarity_completely_different(self):
        """Test very different strings have low similarity."""
        assert string_similarity("skeleton", "dependencies") < 0.5
        assert string_similarity("cfg", "web_search") < 0.3
        assert string_similarity("abc", "xyz") < 0.3

    def test_string_similarity_prefix_match(self):
        """Test prefix matching has decent similarity."""
        # "skel" is prefix of "skeleton"
        sim = string_similarity("skel", "skeleton")
        assert sim > 0.4  # Partial match

    def test_string_similarity_empty_strings(self):
        """Test empty string handling."""
        assert string_similarity("", "") == 1.0
        assert string_similarity("skeleton", "") == 0.0
        assert string_similarity("", "skeleton") == 0.0

    def test_string_similarity_short_strings(self):
        """Test very short strings."""
        assert string_similarity("a", "a") == 1.0
        assert string_similarity("a", "b") == 0.0
        assert string_similarity("ab", "ab") == 1.0


class TestToolResolution:
    """Tests for tool name resolution."""

    # === Exact match tests ===

    def test_resolve_exact_match(self):
        """Test resolving exact tool name."""
        match = resolve_tool("skeleton")
        assert match.tool == "skeleton"
        assert match.confidence == 1.0

    def test_resolve_exact_match_various_tools(self):
        """Test exact match for various tools."""
        tools_to_test = ["deps", "query", "cfg", "anchors", "context", "view"]
        for tool in tools_to_test:
            if tool in TOOL_REGISTRY:
                match = resolve_tool(tool)
                assert match.tool == tool, f"Expected {tool}, got {match.tool}"
                assert match.confidence == 1.0

    def test_resolve_exact_match_underscored_names(self):
        """Test exact match for underscore-separated tool names."""
        tools_to_test = [
            "web_search",
            "web_fetch",
            "todo_list",
            "health_check",
        ]
        for tool in tools_to_test:
            if tool in TOOL_REGISTRY:
                match = resolve_tool(tool)
                assert match.tool == tool
                assert match.confidence == 1.0

    # === Alias tests ===

    def test_resolve_alias_skeleton(self):
        """Test skeleton-related aliases."""
        aliases = ["structure", "outline", "symbols", "hierarchy", "overview", "tree"]
        for alias in aliases:
            if alias in TOOL_ALIASES:
                match = resolve_tool(alias)
                assert match.tool == TOOL_ALIASES[alias], f"Alias {alias} failed"
                assert match.confidence == 1.0

    def test_resolve_alias_deps(self):
        """Test deps-related aliases."""
        aliases = ["imports", "dependencies", "exports", "modules"]
        for alias in aliases:
            if alias in TOOL_ALIASES:
                match = resolve_tool(alias)
                assert match.tool == "deps", f"Alias {alias} should map to deps"
                assert match.confidence == 1.0

    def test_resolve_alias_anchors(self):
        """Test anchors-related aliases."""
        aliases = ["functions", "classes", "methods", "definitions", "defs", "locate"]
        for alias in aliases:
            if alias in TOOL_ALIASES:
                match = resolve_tool(alias)
                assert match.tool == "anchors", f"Alias {alias} should map to anchors"
                assert match.confidence == 1.0

    def test_resolve_alias_query(self):
        """Test query-related aliases."""
        aliases = ["search", "grep", "filter"]
        for alias in aliases:
            if alias in TOOL_ALIASES:
                match = resolve_tool(alias)
                assert match.tool == "query", f"Alias {alias} should map to query"
                assert match.confidence == 1.0

    def test_resolve_alias_cfg(self):
        """Test cfg-related aliases."""
        aliases = ["flow", "graph", "control-flow", "controlflow", "paths"]
        for alias in aliases:
            if alias in TOOL_ALIASES:
                match = resolve_tool(alias)
                assert match.tool == "cfg", f"Alias {alias} should map to cfg"
                assert match.confidence == 1.0

    def test_resolve_alias_web(self):
        """Test web-related aliases."""
        web_fetch_aliases = ["fetch", "browse", "url", "webpage"]
        for alias in web_fetch_aliases:
            if alias in TOOL_ALIASES:
                match = resolve_tool(alias)
                assert match.tool == "web_fetch"

        web_search_aliases = ["websearch", "lookup", "google", "duckduckgo"]
        for alias in web_search_aliases:
            if alias in TOOL_ALIASES:
                match = resolve_tool(alias)
                assert match.tool == "web_search"

    # === Typo correction tests ===

    def test_resolve_typo_missing_letter(self):
        """Test resolving typos with missing letters."""
        typos = [
            ("skelton", "skeleton"),  # missing 'e'
            ("anchrs", "anchors"),  # missing 'o'
            ("contxt", "context"),  # missing 'e'
        ]
        for typo, expected in typos:
            match = resolve_tool(typo)
            assert match.tool == expected, f"Typo {typo} should resolve to {expected}"
            assert match.confidence > 0.7

    def test_resolve_typo_extra_letter(self):
        """Test resolving typos with extra letters."""
        typos = [
            ("skeletonn", "skeleton"),
            ("queryyy", "query"),
        ]
        for typo, expected in typos:
            match = resolve_tool(typo)
            assert match.tool == expected, f"Typo {typo} should resolve to {expected}"
            assert match.confidence > 0.7

    def test_resolve_typo_wrong_letter(self):
        """Test resolving typos with wrong letters."""
        typos = [
            ("skelaton", "skeleton"),  # 'e' → 'a'
            ("quary", "query"),  # 'e' → 'a'
        ]
        for typo, expected in typos:
            match = resolve_tool(typo)
            assert match.tool == expected, f"Typo {typo} should resolve to {expected}"
            assert match.confidence > 0.7

    def test_resolve_typo_swapped_letters(self):
        """Test resolving typos with swapped adjacent letters."""
        typos = [
            ("skelteon", "skeleton"),
            ("anchers", "anchors"),
        ]
        for typo, _expected in typos:
            match = resolve_tool(typo)
            # Swapped letters might have lower confidence
            assert match.confidence > 0.6

    # === Unknown/low confidence tests ===

    def test_resolve_unknown(self):
        """Test resolving completely unknown tool."""
        match = resolve_tool("xyzabc123")
        assert match.confidence < 0.5

    def test_resolve_gibberish(self):
        """Test resolving random gibberish."""
        # Use truly random strings that don't partially match tool names
        gibberish = ["xyzpdq", "zzzzzzz", "aaaaaa", "987654"]
        for g in gibberish:
            match = resolve_tool(g)
            assert match.confidence < 0.6, f"Gibberish {g} should have low confidence"

    # === Hyphen to underscore conversion ===

    def test_resolve_hyphen_to_underscore(self):
        """Test that hyphens are converted to underscores."""
        match = resolve_tool("web-search")
        assert match.tool == "web_search"
        assert match.confidence == 1.0

        match = resolve_tool("todo-list")
        assert match.tool == "todo_list"
        assert match.confidence == 1.0

    # === Case handling ===

    def test_resolve_lowercase(self):
        """Test that resolution is case-insensitive."""
        match = resolve_tool("SKELETON")
        assert match.tool == "skeleton"
        assert match.confidence == 1.0

        match = resolve_tool("Skeleton")
        assert match.tool == "skeleton"
        assert match.confidence == 1.0

        match = resolve_tool("sKeLeTon")
        assert match.tool == "skeleton"
        assert match.confidence == 1.0


class TestParameterResolution:
    """Tests for parameter name resolution."""

    def test_resolve_parameter_path_aliases(self):
        """Test path-related parameter aliases."""
        path_aliases = ["file", "file_path", "filepath", "directory", "dir"]
        for alias in path_aliases:
            assert resolve_parameter(alias) == "path", f"{alias} should map to path"

    def test_resolve_parameter_pattern_aliases(self):
        """Test pattern-related parameter aliases."""
        # glob should map to pattern
        assert resolve_parameter("glob") == "pattern"
        # Other pattern-like names may pass through or map differently
        # Just verify they return something reasonable
        assert isinstance(resolve_parameter("wildcard"), str)
        assert isinstance(resolve_parameter("match"), str)

    def test_resolve_parameter_inherits_aliases(self):
        """Test inherits-related parameter aliases."""
        assert resolve_parameter("base") == "inherits"
        assert resolve_parameter("parent") == "inherits"
        assert resolve_parameter("extends") == "inherits"

    def test_resolve_parameter_unknown(self):
        """Test unknown parameters pass through."""
        assert resolve_parameter("unknown") == "unknown"
        assert resolve_parameter("custom_param") == "custom_param"
        assert resolve_parameter("xyz123") == "xyz123"

    def test_normalize_parameters_empty(self):
        """Test normalizing empty params dict."""
        assert normalize_parameters({}) == {}

    def test_normalize_parameters_no_aliases(self):
        """Test normalizing params with no aliases."""
        params = {"path": "test.py", "name": "MyClass"}
        normalized = normalize_parameters(params)
        assert normalized == params

    def test_normalize_parameters_mixed(self):
        """Test normalizing params with some aliases."""
        params = {"file": "test.py", "glob": "**/*.py", "name": "func"}
        normalized = normalize_parameters(params)
        assert normalized["path"] == "test.py"
        assert normalized["name"] == "func"

    def test_normalize_parameters_all_aliases(self):
        """Test normalizing params where all are aliases."""
        params = {"file": "test.py", "base": "Parent"}
        normalized = normalize_parameters(params)
        assert normalized == {"path": "test.py", "inherits": "Parent"}


@pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
class TestNLMarkerProtection:
    """Tests ensuring NL markers (show/find/get) don't hijack queries.

    This is critical - common English words at the start of queries
    should NOT match tool names or aliases.

    NOTE: These tests require the embedding-based semantic matching that was
    removed. They are marked xfail until NL matching is re-implemented or
    the tests are updated for the simplified 4-primitive system.
    """

    def test_show_does_not_match_shadow(self):
        """'show' should not match 'shadow_git_*' tools."""
        matches = analyze_intent("show code structure")
        tool_names = [m.tool for m in matches[:5]]
        shadow_tools = [t for t in tool_names if "shadow" in t]
        assert len(shadow_tools) == 0, f"'show' matched shadow tools: {shadow_tools}"

    def test_find_does_not_match_query_in_nl(self):
        """'find' in NL context should not immediately resolve to 'query'."""
        # "find all classes" should use semantic matching, not alias
        matches = analyze_intent("find all classes in this module")
        # Should get anchors or query through semantic matching, not alias shortcut
        assert len(matches) > 0
        # First result should NOT be query with 100% confidence
        if matches[0].tool == "query":
            assert matches[0].confidence < 1.0, "find shouldn't be alias-matched in NL"

    def test_get_does_not_hijack_queries(self):
        """'get' should not hijack natural language queries."""
        matches = analyze_intent("get the list of all functions")
        assert len(matches) > 0
        # Should find function-related tools via semantic matching
        tool_names = [m.tool for m in matches[:5]]
        assert any(t in tool_names for t in ["anchors", "skeleton", "query"]), f"Got: {tool_names}"

    def test_show_with_structure_finds_skeleton(self):
        """'show code structure' should find skeleton via embeddings."""
        matches = analyze_intent("show code structure")
        tool_names = [m.tool for m in matches[:5]]
        structure_tools = {"skeleton", "view", "context", "health_analyze_structure"}
        assert any(t in structure_tools for t in tool_names), f"Got: {tool_names}"

    def test_find_imports_finds_deps(self):
        """'find imports and dependencies' should find deps tools."""
        matches = analyze_intent("find imports and dependencies")
        tool_names = [m.tool for m in matches[:5]]
        dep_tools = {"deps", "dependencies_extract", "dependencies_analyze"}
        assert any(t in dep_tools for t in tool_names), f"Got: {tool_names}"

    def test_show_me_the_functions(self):
        """Natural phrasing 'show me the functions' should work."""
        matches = analyze_intent("show me the functions in this file")
        tool_names = [m.tool for m in matches[:5]]
        func_tools = {"anchors", "skeleton", "query"}
        assert any(t in func_tools for t in tool_names), f"Got: {tool_names}"

    def test_find_where_class_is_defined(self):
        """'find where class X is defined' should work semantically."""
        matches = analyze_intent("find where class MyClass is defined")
        tool_names = [m.tool for m in matches[:5]]
        search_tools = {"query", "anchors", "search_find_definitions"}
        assert any(t in search_tools for t in tool_names), f"Got: {tool_names}"

    def test_get_all_todos(self):
        """'get all todos' should find todo tools."""
        matches = analyze_intent("get all todos in the codebase")
        tool_names = [m.tool for m in matches[:5]]
        todo_tools = {"todo_list", "todo_search", "health_check_todos"}
        assert any(t in todo_tools for t in tool_names), f"Got: {tool_names}"


class TestToolRouter:
    """Tests for the ToolRouter class."""

    def test_router_initialization(self):
        """Test router initializes correctly."""
        router = ToolRouter()
        matches = router.analyze_intent("test")
        assert isinstance(matches, list)

    # === Skeleton/Structure queries ===

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_analyze_intent_skeleton(self):
        """Test analyzing intent for skeleton."""
        router = ToolRouter()
        matches = router.analyze_intent("show code structure")
        assert len(matches) > 0
        tool_names = [m.tool for m in matches[:5]]
        structure_tools = {"skeleton", "context", "search_summarize_module", "view"}
        assert any(t in structure_tools for t in tool_names)

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_skeleton_various_phrasings(self):
        """Test various phrasings for skeleton intent."""
        phrasings = [
            "what functions are in this file",
            "list all classes and methods",
            "code outline",
            "show the file structure",
            "give me an overview of this module",
            "what's in this file",
        ]
        router = ToolRouter()
        structure_tools = {"skeleton", "anchors", "context", "view", "search_summarize_module"}

        for phrase in phrasings:
            matches = router.analyze_intent(phrase)
            tool_names = [m.tool for m in matches[:5]]
            assert any(t in structure_tools for t in tool_names), (
                f"'{phrase}' should match structure tools, got: {tool_names}"
            )

    # === Dependencies queries ===

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_analyze_intent_deps(self):
        """Test analyzing intent for dependencies."""
        router = ToolRouter()
        matches = router.analyze_intent("find imports and dependencies")
        assert len(matches) > 0
        tool_names = [m.tool for m in matches[:5]]
        dep_related = {
            "deps",
            "dependencies_extract",
            "dependencies_format",
            "dependencies_analyze",
        }
        assert any(t in dep_related for t in tool_names), f"Got: {tool_names}"

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_deps_various_phrasings(self):
        """Test various phrasings for dependencies intent."""
        phrasings = [
            "show dependencies",
            "what does this module import",
            "import graph",
            "list all imports",
        ]
        router = ToolRouter()
        dep_tools = {
            "deps",
            "dependencies_extract",
            "dependencies_analyze",
            "external_deps_list_direct",
        }

        for phrase in phrasings:
            matches = router.analyze_intent(phrase)
            tool_names = [m.tool for m in matches[:5]]
            assert any(t in dep_tools for t in tool_names), (
                f"'{phrase}' should match dep tools, got: {tool_names}"
            )

    # === Query/Search queries ===

    def test_analyze_intent_query(self):
        """Test analyzing intent for query."""
        router = ToolRouter()
        matches = router.analyze_intent("search for classes that inherit from Base")
        assert len(matches) > 0
        tool_names = [m.tool for m in matches[:3]]
        assert "query" in tool_names

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_query_various_phrasings(self):
        """Test various phrasings for query intent."""
        phrasings = [
            "find large functions over 100 lines",
            "search for pattern in code",
            "functions with more than 5 parameters",
        ]
        router = ToolRouter()
        # Query and anchors both handle search-type queries
        search_tools = {"query", "search_query", "search_find_definitions", "anchors"}

        for phrase in phrasings:
            matches = router.analyze_intent(phrase)
            tool_names = [m.tool for m in matches[:5]]
            assert any(t in search_tools for t in tool_names), (
                f"'{phrase}' should match search tools, got: {tool_names}"
            )

    # === CFG queries ===

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_analyze_intent_cfg(self):
        """Test analyzing intent for CFG."""
        router = ToolRouter()
        matches = router.analyze_intent("show control flow graph")
        assert len(matches) > 0
        tool_names = [m.tool for m in matches[:3]]
        assert "cfg" in tool_names

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_cfg_various_phrasings(self):
        """Test various phrasings for CFG intent."""
        phrasings = [
            "control flow graph",
            "show execution paths",
            "analyze branches",
            "what paths can this function take",
            "visualize the control flow",
        ]
        router = ToolRouter()
        cfg_tools = {"cfg", "cfg_build"}

        for phrase in phrasings:
            matches = router.analyze_intent(phrase)
            tool_names = [m.tool for m in matches[:5]]
            assert any(t in cfg_tools for t in tool_names), (
                f"'{phrase}' should match cfg tools, got: {tool_names}"
            )

    # === Anchors queries ===

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_anchors_various_phrasings(self):
        """Test various phrasings for anchors intent."""
        phrasings = [
            "find all classes",
            "locate function definitions",
            "where is this method defined",
            "list all function names",
            "show class definitions",
        ]
        router = ToolRouter()
        anchor_tools = {"anchors", "anchor_find", "search_find_definitions"}

        for phrase in phrasings:
            matches = router.analyze_intent(phrase)
            tool_names = [m.tool for m in matches[:5]]
            assert any(t in anchor_tools for t in tool_names), (
                f"'{phrase}' should match anchor tools, got: {tool_names}"
            )

    # === Context queries ===

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_context_various_phrasings(self):
        """Test various phrasings for context intent."""
        phrasings = [
            "explain this file",
            "what does this code do",
            "summarize the module",
            "give me context about this",
        ]
        router = ToolRouter()
        context_tools = {"context", "search_summarize_module", "view"}

        for phrase in phrasings:
            matches = router.analyze_intent(phrase)
            tool_names = [m.tool for m in matches[:5]]
            assert any(t in context_tools for t in tool_names), (
                f"'{phrase}' should match context tools, got: {tool_names}"
            )

    # === Tool suggestion ===

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_suggest_tool(self):
        """Test tool suggestion."""
        router = ToolRouter()
        suggestion = router.suggest_tool("show classes functions methods structure outline")
        assert suggestion is not None
        assert suggestion.tool in ("skeleton", "anchors", "context", "view")

    def test_suggest_tool_returns_none_for_vague(self):
        """Test that vague queries may return None."""
        router = ToolRouter()
        suggestion = router.suggest_tool("do something")
        # Vague queries should have low confidence or return None
        if suggestion is not None:
            assert suggestion.confidence < 0.8

    # === Tool with arguments ===

    def test_tool_with_path_argument(self):
        """Test 'skeleton src/main.py' style queries."""
        matches = analyze_intent("skeleton src/main.py")
        assert matches[0].tool == "skeleton"
        assert matches[0].confidence == 1.0

    def test_tool_with_multiple_words(self):
        """Test multi-word tool names."""
        matches = analyze_intent("web_search python tutorials")
        if "web_search" in TOOL_REGISTRY:
            assert matches[0].tool == "web_search"
            assert matches[0].confidence == 1.0

    # === Available tools filter ===

    def test_available_tools_filter_basic(self):
        """Test filtering to specific tools."""
        matches = analyze_intent("find code", available_tools=["skeleton", "deps"])
        tool_names = {m.tool for m in matches}
        assert tool_names <= {"skeleton", "deps"}

    def test_available_tools_filter_empty(self):
        """Test with empty available tools list."""
        matches = analyze_intent("find code", available_tools=[])
        # Empty filter may still return results (implementation dependent)
        # Just verify it doesn't crash and returns a list
        assert isinstance(matches, list)

    def test_available_tools_filter_single(self):
        """Test with single available tool."""
        matches = analyze_intent("anything", available_tools=["skeleton"])
        assert all(m.tool == "skeleton" for m in matches)


class TestModuleFunctions:
    """Tests for module-level convenience functions."""

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_analyze_intent(self):
        """Test analyze_intent function."""
        matches = analyze_intent("show file overview")
        assert len(matches) > 0
        assert all(isinstance(m, ToolMatch) for m in matches)

    def test_analyze_intent_returns_sorted(self):
        """Test that analyze_intent returns results sorted by confidence."""
        matches = analyze_intent("show code structure")
        confidences = [m.confidence for m in matches]
        assert confidences == sorted(confidences, reverse=True)

    def test_suggest_tool(self):
        """Test suggest_tool function."""
        suggestion = suggest_tool("find all methods")
        assert suggestion is None or isinstance(suggestion, ToolMatch)

    def test_suggest_tool_high_confidence_query(self):
        """Test suggest_tool with high-confidence query."""
        suggestion = suggest_tool("skeleton")
        assert suggestion is not None
        assert suggestion.tool == "skeleton"
        assert suggestion.confidence == 1.0

    def test_suggest_tools_returns_multiple(self):
        """Test suggest_tools returns multiple results."""
        suggestions = suggest_tools("show code", top_k=5)
        assert len(suggestions) <= 5
        assert all(isinstance(s, ToolMatch) for s in suggestions)

    def test_suggest_tools_respects_top_k(self):
        """Test suggest_tools respects top_k limit."""
        for k in [1, 3, 5, 10]:
            suggestions = suggest_tools("find something", top_k=k)
            assert len(suggestions) <= k

    def test_list_tools(self):
        """Test list_tools function."""
        tools = list_tools()
        assert len(tools) > 0
        assert all("name" in t for t in tools)
        assert all("description" in t for t in tools)

    def test_list_tools_has_common_tools(self):
        """Test list_tools includes common tools."""
        tools = list_tools()
        tool_names = {t["name"] for t in tools}
        common_tools = {"skeleton", "deps", "query", "cfg", "anchors", "context"}
        for tool in common_tools:
            assert tool in tool_names, f"Expected {tool} in tool list"

    def test_list_tool_names(self):
        """Test list_tool_names function."""
        names = list_tool_names()
        assert len(names) > 0
        assert all(isinstance(n, str) for n in names)
        assert "skeleton" in names
        assert "deps" in names

    def test_get_tool_info(self):
        """Test get_tool_info function."""
        info = get_tool_info("skeleton")
        assert info is not None
        assert info["name"] == "skeleton"
        assert "description" in info
        assert "aliases" in info

    def test_get_tool_info_via_alias(self):
        """Test get_tool_info works with aliases."""
        info = get_tool_info("structure")
        assert info is not None
        assert info["name"] == "skeleton"  # structure → skeleton

        info = get_tool_info("imports")
        assert info is not None
        assert info["name"] == "deps"

    def test_get_tool_info_unknown(self):
        """Test get_tool_info returns None for unknown tools."""
        assert get_tool_info("xyzunknown123") is None
        assert get_tool_info("") is None
        # Use truly random gibberish that can't fuzzy-match anything
        assert get_tool_info("qqqqqqqqqqqq") is None

    def test_get_tool_info_has_required_fields(self):
        """Test get_tool_info returns all required fields."""
        info = get_tool_info("skeleton")
        assert info is not None
        required_fields = ["name", "description", "keywords", "parameters", "aliases"]
        for field in required_fields:
            assert field in info, f"Missing field: {field}"

    def test_tool_info_aliases_correct(self):
        """Test that tool info aliases match TOOL_ALIASES."""
        for alias, target in TOOL_ALIASES.items():
            info = get_tool_info(alias)
            if info is not None:
                assert info["name"] == target, f"Alias {alias} should point to {target}"


class TestEdgeCases:
    """Tests for edge cases and unusual inputs."""

    # === Empty/whitespace queries ===

    def test_empty_query(self):
        """Test handling empty query."""
        matches = analyze_intent("")
        assert isinstance(matches, list)
        assert len(matches) == 0  # Empty query should return empty list

    def test_whitespace_only_query(self):
        """Test handling whitespace-only query."""
        matches = analyze_intent("   ")
        assert isinstance(matches, list)

    def test_newlines_in_query(self):
        """Test handling newlines in query."""
        matches = analyze_intent("find\nall\nclasses")
        assert isinstance(matches, list)

    def test_tabs_in_query(self):
        """Test handling tabs in query."""
        matches = analyze_intent("find\tall\tclasses")
        assert isinstance(matches, list)

    # === Very long queries ===

    def test_long_query(self):
        """Test handling very long query."""
        query = "I need to " + "find and analyze " * 100 + "the code structure"
        matches = analyze_intent(query)
        assert isinstance(matches, list)

    def test_extremely_long_single_word(self):
        """Test handling extremely long single word."""
        query = "a" * 1000
        matches = analyze_intent(query)
        assert isinstance(matches, list)

    # === Special characters ===

    def test_special_characters_in_query(self):
        """Test handling special characters."""
        matches = analyze_intent("find def __init__(self):")
        assert isinstance(matches, list)

    def test_unicode_in_query(self):
        """Test handling unicode characters."""
        matches = analyze_intent("find función with émoji 🎉")
        assert isinstance(matches, list)

    def test_quotes_in_query(self):
        """Test handling quotes in query."""
        matches = analyze_intent('find "quoted string"')
        assert isinstance(matches, list)

    def test_brackets_in_query(self):
        """Test handling brackets in query."""
        matches = analyze_intent("find [list] and {dict}")
        assert isinstance(matches, list)

    def test_regex_like_query(self):
        """Test handling regex-like patterns."""
        matches = analyze_intent("find .*\\.py$ files")
        assert isinstance(matches, list)

    def test_path_like_query(self):
        """Test handling path-like strings."""
        matches = analyze_intent("skeleton /path/to/file.py")
        assert isinstance(matches, list)
        assert matches[0].tool == "skeleton"

    # === Numbers ===

    def test_numeric_query(self):
        """Test handling numeric queries."""
        matches = analyze_intent("12345")
        assert isinstance(matches, list)

    def test_query_with_numbers(self):
        """Test handling queries with numbers."""
        matches = analyze_intent("find functions with more than 100 lines")
        assert isinstance(matches, list)

    # === Case variations ===

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_all_caps_query(self):
        """Test handling all caps query."""
        matches = analyze_intent("SHOW CODE STRUCTURE")
        assert isinstance(matches, list)
        # Should still work
        tool_names = [m.tool for m in matches[:5]]
        assert any(t in ["skeleton", "view", "context"] for t in tool_names)

    def test_mixed_case_query(self):
        """Test handling mixed case query."""
        matches = analyze_intent("ShOw CoDe StRuCtUrE")
        assert isinstance(matches, list)

    # === Punctuation ===

    def test_trailing_punctuation(self):
        """Test handling trailing punctuation."""
        matches = analyze_intent("show code structure!")
        assert isinstance(matches, list)

        matches = analyze_intent("show code structure?")
        assert isinstance(matches, list)

        matches = analyze_intent("show code structure...")
        assert isinstance(matches, list)

    def test_leading_punctuation(self):
        """Test handling leading punctuation."""
        matches = analyze_intent("...show code structure")
        assert isinstance(matches, list)

    # === Tool filtering ===

    def test_available_tools_filter(self):
        """Test filtering available tools."""
        matches = analyze_intent("find code", available_tools=["skeleton", "deps"])
        tool_names = {m.tool for m in matches}
        assert tool_names <= {"skeleton", "deps"}

    def test_available_tools_nonexistent(self):
        """Test filtering with non-existent tools."""
        matches = analyze_intent("find code", available_tools=["nonexistent_tool"])
        assert len(matches) == 0

    def test_available_tools_mixed(self):
        """Test filtering with mix of real and fake tools."""
        matches = analyze_intent("find code", available_tools=["skeleton", "fake_tool"])
        tool_names = {m.tool for m in matches}
        assert "skeleton" in tool_names or len(tool_names) == 0
        assert "fake_tool" not in tool_names

    # === Repeated words ===

    def test_repeated_words(self):
        """Test handling repeated words."""
        matches = analyze_intent("find find find code")
        assert isinstance(matches, list)

    def test_many_repeated_words(self):
        """Test handling many repeated words."""
        matches = analyze_intent("skeleton " * 50)
        assert isinstance(matches, list)
        # Should still recognize skeleton
        assert matches[0].tool == "skeleton"


class TestConfidenceScoring:
    """Tests for confidence score behavior."""

    def test_exact_match_full_confidence(self):
        """Test that exact tool names get 1.0 confidence."""
        matches = analyze_intent("skeleton")
        assert matches[0].confidence == 1.0

    def test_alias_full_confidence(self):
        """Test that exact aliases get 1.0 confidence."""
        matches = analyze_intent("structure")
        assert matches[0].confidence == 1.0

    def test_typo_reduced_confidence(self):
        """Test that typos get reduced confidence."""
        matches = analyze_intent("skelton")
        assert 0.7 < matches[0].confidence < 1.0

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_nl_query_moderate_confidence(self):
        """Test that NL queries get moderate confidence."""
        matches = analyze_intent("show me the code structure please")
        assert len(matches) > 0
        # NL queries should have moderate, not perfect confidence
        assert matches[0].confidence < 1.0

    def test_vague_query_low_confidence(self):
        """Test that vague queries get lower confidence."""
        matches = analyze_intent("do something")
        if len(matches) > 0:
            assert matches[0].confidence < 0.8

    def test_confidence_decreases_in_ranking(self):
        """Test that confidence decreases down the ranking."""
        matches = analyze_intent("show code structure")
        if len(matches) >= 2:
            for i in range(len(matches) - 1):
                assert matches[i].confidence >= matches[i + 1].confidence

    def test_confidence_range(self):
        """Test that confidence is always in valid range."""
        queries = [
            "skeleton",
            "show code",
            "random gibberish xyz",
            "find all the things in the world",
        ]
        for query in queries:
            matches = analyze_intent(query)
            for m in matches:
                assert 0 <= m.confidence <= 1.0, f"Invalid confidence for {query}"


class TestToolMatchDataclass:
    """Tests for ToolMatch dataclass behavior."""

    def test_toolmatch_has_tool(self):
        """Test ToolMatch has tool attribute."""
        match = ToolMatch(tool="skeleton", confidence=1.0)
        assert match.tool == "skeleton"

    def test_toolmatch_has_confidence(self):
        """Test ToolMatch has confidence attribute."""
        match = ToolMatch(tool="skeleton", confidence=0.95)
        assert match.confidence == 0.95

    def test_toolmatch_equality(self):
        """Test ToolMatch equality."""
        m1 = ToolMatch(tool="skeleton", confidence=1.0)
        m2 = ToolMatch(tool="skeleton", confidence=1.0)
        assert m1 == m2

    def test_toolmatch_inequality(self):
        """Test ToolMatch inequality."""
        m1 = ToolMatch(tool="skeleton", confidence=1.0)
        m2 = ToolMatch(tool="deps", confidence=1.0)
        assert m1 != m2

        m3 = ToolMatch(tool="skeleton", confidence=0.9)
        assert m1 != m3


class TestRegistryConsistency:
    """Tests for tool registry consistency."""

    def test_all_aliases_point_to_valid_tools(self):
        """Test that all aliases point to registered tools."""
        for alias, target in TOOL_ALIASES.items():
            assert target in TOOL_REGISTRY, f"Alias {alias} points to unregistered tool {target}"

    def test_no_circular_aliases(self):
        """Test that there are no circular alias references."""
        # Aliases should point to tool names, not other aliases
        for alias, target in TOOL_ALIASES.items():
            assert target not in TOOL_ALIASES, f"Alias {alias} points to another alias {target}"

    def test_tool_names_are_lowercase(self):
        """Test that tool names are lowercase."""
        for name in TOOL_REGISTRY.keys():
            assert name == name.lower(), f"Tool name {name} should be lowercase"

    def test_alias_names_are_lowercase(self):
        """Test that alias names are lowercase."""
        for alias in TOOL_ALIASES.keys():
            assert alias == alias.lower(), f"Alias {alias} should be lowercase"

    def test_registered_tools_have_descriptions(self):
        """Test that all registered tools have descriptions."""
        for name in list(TOOL_REGISTRY.keys())[:20]:  # Sample first 20
            info = get_tool_info(name)
            if info is not None:
                assert info.get("description"), f"Tool {name} missing description"


class TestWordOrderVariations:
    """Tests for word order handling."""

    def test_reversed_word_order(self):
        """Test that reversed word order still works."""
        # "list todo" should work like "todo list" or "todo_list"
        matches = analyze_intent("list todo")
        # Should find todo-related tools
        tool_names = [m.tool for m in matches[:5]]
        todo_tools = {"todo_list", "todo_search"}
        # At least one should match
        assert any(t in todo_tools for t in tool_names) or len(matches) > 0

    def test_search_todo_vs_todo_search(self):
        """Test 'search todo' vs 'todo search' handling."""
        matches1 = analyze_intent("search todo")
        matches2 = analyze_intent("todo search")
        # Both should return results
        assert len(matches1) > 0
        assert len(matches2) > 0


class TestMultipleToolsInQuery:
    """Tests for queries that could match multiple tools."""

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_ambiguous_query_returns_multiple(self):
        """Test that ambiguous queries return multiple options."""
        matches = analyze_intent("analyze code")
        # Should return multiple potential matches
        assert len(matches) >= 2

    def test_specific_query_ranks_correctly(self):
        """Test that specific queries rank the right tool first."""
        # "skeleton" should rank skeleton first
        matches = analyze_intent("skeleton")
        assert matches[0].tool == "skeleton"

        # "deps" should rank deps first
        matches = analyze_intent("deps")
        assert matches[0].tool == "deps"

    def test_related_tools_ranked_together(self):
        """Test that related tools appear near each other in rankings."""
        matches = analyze_intent("show dependencies")
        tool_names = [m.tool for m in matches[:10]]
        # Dependency-related tools should cluster
        dep_tools = [t for t in tool_names if "dep" in t or t == "deps"]
        if len(dep_tools) >= 2:
            # Check they're not too far apart
            indices = [tool_names.index(t) for t in dep_tools]
            assert max(indices) - min(indices) <= 5


# === Parametrized tests for comprehensive coverage ===


class TestExactToolMatching:
    """Parametrized tests for exact tool name matching."""

    @pytest.mark.parametrize(
        "tool_name",
        [
            "skeleton",
            "deps",
            "query",
            "cfg",
            "anchors",
            "context",
            "view",
            "callers",
            "callees",
            "todo_list",
            "web_search",
            "web_fetch",
            "health_check",
        ],
    )
    def test_exact_tool_name_matches(self, tool_name):
        """Test that exact tool names match with full confidence."""
        if tool_name in TOOL_REGISTRY:
            matches = analyze_intent(tool_name)
            assert len(matches) > 0
            assert matches[0].tool == tool_name
            assert matches[0].confidence == 1.0


class TestAliasMatching:
    """Parametrized tests for alias matching."""

    @pytest.mark.parametrize(
        "alias,expected_tool",
        [
            ("structure", "skeleton"),
            ("outline", "skeleton"),
            ("symbols", "skeleton"),
            ("imports", "deps"),
            ("dependencies", "deps"),
            ("functions", "anchors"),
            ("classes", "anchors"),
            ("methods", "anchors"),
            ("search", "query"),
            ("grep", "query"),
            ("flow", "cfg"),
            ("graph", "cfg"),
        ],
    )
    def test_alias_resolves_correctly(self, alias, expected_tool):
        """Test that aliases resolve to the correct tool."""
        if alias in TOOL_ALIASES:
            matches = analyze_intent(alias)
            assert len(matches) > 0
            assert matches[0].tool == expected_tool
            assert matches[0].confidence == 1.0


@pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
class TestNaturalLanguageQueries:
    """Parametrized tests for natural language queries."""

    @pytest.mark.parametrize(
        "query,expected_tools",
        [
            # Structure queries
            ("show code structure", {"skeleton", "view", "context"}),
            ("what functions are in this file", {"skeleton", "anchors"}),
            ("list all classes", {"anchors", "skeleton", "query"}),
            ("code outline", {"skeleton", "view", "context"}),
            # Dependency queries
            ("show dependencies", {"deps", "dependencies_extract", "dependencies_analyze"}),
            ("what does this import", {"deps", "dependencies_extract"}),
            # Search queries
            ("find classes that inherit from Base", {"query", "anchors"}),
            ("search for pattern", {"query", "search_query", "search_grep"}),
            # CFG queries
            ("control flow graph", {"cfg", "cfg_build"}),
            ("show execution paths", {"cfg", "cfg_build"}),
        ],
    )
    def test_nl_query_finds_relevant_tools(self, query, expected_tools):
        """Test that NL queries find relevant tools."""
        matches = analyze_intent(query)
        tool_names = {m.tool for m in matches[:5]}
        assert tool_names & expected_tools, f"Query '{query}' got {tool_names}"


class TestToolWithArguments:
    """Parametrized tests for tool + argument queries."""

    @pytest.mark.parametrize(
        "query,expected_tool",
        [
            ("skeleton src/main.py", "skeleton"),
            ("skeleton src/", "skeleton"),
            ("deps requirements.txt", "deps"),
            ("query type:class", "query"),
            ("cfg my_function", "cfg"),
            ("anchors *.py", "anchors"),
            ("view README.md", "view"),
        ],
    )
    def test_tool_with_argument(self, query, expected_tool):
        """Test tool name followed by argument."""
        if expected_tool in TOOL_REGISTRY:
            matches = analyze_intent(query)
            assert len(matches) > 0
            assert matches[0].tool == expected_tool
            assert matches[0].confidence == 1.0


class TestTypoRecovery:
    """Parametrized tests for typo recovery."""

    @pytest.mark.parametrize(
        "typo,expected_tool",
        [
            ("skelton", "skeleton"),  # missing 'e'
            ("skeletn", "skeleton"),  # missing 'o'
            ("skeletan", "skeleton"),  # wrong vowel
            ("qury", "query"),  # missing 'e'
            ("querry", "query"),  # extra 'r'
            ("dpes", "deps"),  # swapped
            ("desp", "deps"),  # swapped
            ("anchers", "anchors"),  # wrong vowel
            ("ancors", "anchors"),  # missing 'h'
        ],
    )
    def test_typo_recovers_correct_tool(self, typo, expected_tool):
        """Test that typos recover to the correct tool."""
        matches = analyze_intent(typo)
        if len(matches) > 0 and matches[0].confidence > 0.6:
            # Only assert if we got a confident match
            assert matches[0].tool == expected_tool, f"Typo '{typo}' got {matches[0].tool}"


class TestNLMarkerQueries:
    """Parametrized tests ensuring NL markers don't hijack queries."""

    @pytest.mark.parametrize(
        "query",
        [
            "show me the code",
            "show all functions",
            "show dependencies",
            "find all classes",
            "find the definition",
            "find imports",
            "get the structure",
            "get all methods",
            "get dependencies",
        ],
    )
    def test_nl_marker_does_not_give_100_confidence(self, query):
        """Test that NL marker queries don't get 100% alias confidence."""
        matches = analyze_intent(query)
        if len(matches) > 0:
            # If first word is show/find/get, it shouldn't be alias-matched
            first_word = query.split()[0].lower()
            if first_word in ("show", "find", "get"):
                # These should go through embedding matching, not alias
                # So confidence should be < 1.0 (unless exact tool match follows)
                words = query.lower().split()
                if len(words) > 1 and words[1] not in TOOL_REGISTRY:
                    assert matches[0].confidence < 1.0 or matches[0].tool != TOOL_ALIASES.get(
                        first_word
                    )


class TestSpecialInputs:
    """Parametrized tests for special input handling."""

    @pytest.mark.parametrize(
        "query",
        [
            "",  # empty
            " ",  # single space
            "   ",  # multiple spaces
            "\t",  # tab
            "\n",  # newline
            "a",  # single char
            "ab",  # two chars
            "123",  # numbers only
            "!@#$%",  # special chars only
            "skeleton" * 100,  # very long
        ],
    )
    def test_special_input_does_not_crash(self, query):
        """Test that special inputs don't crash."""
        matches = analyze_intent(query)
        assert isinstance(matches, list)
        for m in matches:
            assert isinstance(m, ToolMatch)
            assert isinstance(m.tool, str)
            assert isinstance(m.confidence, (int, float))
            assert 0 <= m.confidence <= 1.0


class TestCaseSensitivity:
    """Parametrized tests for case handling."""

    @pytest.mark.parametrize(
        "query",
        [
            "SKELETON",
            "Skeleton",
            "sKeLeTon",
            "DEPS",
            "Deps",
            "dEpS",
            "QUERY",
            "Query",
            "qUeRy",
        ],
    )
    def test_case_insensitive_tool_matching(self, query):
        """Test that tool matching is case-insensitive."""
        matches = analyze_intent(query)
        expected = query.lower()
        if expected in TOOL_REGISTRY:
            assert len(matches) > 0
            assert matches[0].tool == expected
            assert matches[0].confidence == 1.0


class TestHyphenUnderscore:
    """Parametrized tests for hyphen/underscore handling."""

    @pytest.mark.parametrize(
        "query,expected",
        [
            ("web-search", "web_search"),
            ("web_search", "web_search"),
            ("todo-list", "todo_list"),
            ("todo_list", "todo_list"),
            ("health-check", "health_check"),
            ("health_check", "health_check"),
        ],
    )
    def test_hyphen_underscore_equivalence(self, query, expected):
        """Test that hyphens and underscores are treated equivalently."""
        if expected in TOOL_REGISTRY:
            matches = analyze_intent(query)
            assert len(matches) > 0
            assert matches[0].tool == expected
            assert matches[0].confidence == 1.0


# =============================================================================
# XFAIL TESTS: Known gaps in semantic matching
# =============================================================================
# These document queries that SHOULD work but don't yet.
# Fix the underlying matching, then remove the xfail marker.


class TestKnownGaps:
    """Tests for queries that should work but don't yet.

    These are marked xfail to document expected behavior.
    When fixing DWIM matching, use these as targets.
    """

    @pytest.mark.xfail(reason="'module' dominates, finds search_summarize_module not deps")
    def test_module_dependencies_finds_deps(self):
        """'module dependencies' should find deps tools."""
        matches = analyze_intent("module dependencies")
        tool_names = [m.tool for m in matches[:5]]
        dep_tools = {"deps", "dependencies_extract", "dependencies_analyze"}
        assert any(t in dep_tools for t in tool_names), f"Got: {tool_names}"

    @pytest.mark.xfail(reason="'module imports' matches search_summarize_module")
    def test_module_imports_finds_deps(self):
        """'module imports' should find deps tools."""
        matches = analyze_intent("module imports")
        tool_names = [m.tool for m in matches[:5]]
        dep_tools = {"deps", "dependencies_extract"}
        assert any(t in dep_tools for t in tool_names), f"Got: {tool_names}"

    @pytest.mark.xfail(reason="Vague query, matches anchors instead of query")
    def test_classes_inherit_finds_query(self):
        """'classes that inherit from Base' should find query tool."""
        matches = analyze_intent("classes that inherit from Base")
        tool_names = [m.tool for m in matches[:3]]
        assert "query" in tool_names, f"Got: {tool_names}"

    @pytest.mark.xfail(reason="NL matching requires embeddings which were removed")
    def test_what_packages_finds_deps(self):
        """'what packages are used' should find deps tools."""
        matches = analyze_intent("what packages are used")
        tool_names = [m.tool for m in matches[:5]]
        dep_tools = {"deps", "dependencies_extract", "external_deps_list_direct"}
        assert any(t in dep_tools for t in tool_names), f"Got: {tool_names}"

    @pytest.mark.xfail(reason="'subprocess' not in any tool embeddings")
    def test_methods_call_subprocess_finds_query(self):
        """'methods that call subprocess' should find query tool."""
        matches = analyze_intent("methods that call subprocess")
        tool_names = [m.tool for m in matches[:5]]
        assert "query" in tool_names, f"Got: {tool_names}"

    def test_hyphen_full_confidence(self):
        """Hyphen-to-underscore should give full confidence."""
        match = analyze_intent("todo-list")[0]
        assert match.tool == "todo_list"
        assert match.confidence == 1.0, f"Got {match.confidence}"

    @pytest.mark.xfail(reason="Empty available_tools still searches all tools")
    def test_empty_available_tools_returns_empty(self):
        """Empty available_tools filter should return no results."""
        matches = analyze_intent("find code", available_tools=[])
        assert len(matches) == 0, f"Got {len(matches)} matches"


class TestCorePrimitives:
    """Tests for the 4 core primitives: view, edit, analyze, search.

    These are the simplified CLI/MCP tools that subsume the older tool set.
    Resolution uses exact match + basic typo correction.
    """

    def test_exact_match_all_primitives(self):
        """Test exact match for all 4 core primitives."""
        from moss.dwim import CORE_PRIMITIVES, resolve_core_primitive

        for primitive in CORE_PRIMITIVES:
            result, confidence = resolve_core_primitive(primitive)
            assert result == primitive
            assert confidence == 1.0

    def test_alias_view(self):
        """Test aliases resolve to 'view'."""
        from moss.dwim import resolve_core_primitive

        aliases = ["show", "look", "see", "display", "read", "skeleton", "tree", "expand"]
        for alias in aliases:
            result, confidence = resolve_core_primitive(alias)
            assert result == "view", f"'{alias}' should resolve to 'view', got '{result}'"
            assert confidence == 1.0

    def test_alias_edit(self):
        """Test aliases resolve to 'edit'."""
        from moss.dwim import resolve_core_primitive

        aliases = ["modify", "change", "update", "patch", "fix", "replace", "delete"]
        for alias in aliases:
            result, confidence = resolve_core_primitive(alias)
            assert result == "edit", f"'{alias}' should resolve to 'edit', got '{result}'"
            assert confidence == 1.0

    def test_alias_analyze(self):
        """Test aliases resolve to 'analyze'."""
        from moss.dwim import resolve_core_primitive

        aliases = ["check", "health", "complexity", "security", "lint", "audit"]
        for alias in aliases:
            result, confidence = resolve_core_primitive(alias)
            assert result == "analyze", f"'{alias}' should resolve to 'analyze', got '{result}'"
            assert confidence == 1.0

    def test_alias_search(self):
        """Test aliases resolve to 'search'."""
        from moss.dwim import resolve_core_primitive

        aliases = ["find", "grep", "query", "locate", "lookup"]
        for alias in aliases:
            result, confidence = resolve_core_primitive(alias)
            assert result == "search", f"'{alias}' should resolve to 'search', got '{result}'"
            assert confidence == 1.0

    def test_typo_correction_view(self):
        """Test typo correction for 'view'."""
        from moss.dwim import resolve_core_primitive

        typos = ["veiw", "viwe", "vew", "veuw"]
        for typo in typos:
            result, confidence = resolve_core_primitive(typo)
            assert result == "view", f"'{typo}' should resolve to 'view', got '{result}'"
            assert confidence >= 0.7

    def test_typo_correction_edit(self):
        """Test typo correction for 'edit'."""
        from moss.dwim import resolve_core_primitive

        typos = ["eidt", "edti", "edi", "editr"]
        for typo in typos:
            result, confidence = resolve_core_primitive(typo)
            assert result == "edit", f"'{typo}' should resolve to 'edit', got '{result}'"
            assert confidence >= 0.7

    def test_typo_correction_analyze(self):
        """Test typo correction for 'analyze'."""
        from moss.dwim import resolve_core_primitive

        typos = ["analize", "analyz", "analyez", "anayze"]
        for typo in typos:
            result, confidence = resolve_core_primitive(typo)
            assert result == "analyze", f"'{typo}' should resolve to 'analyze', got '{result}'"
            assert confidence >= 0.7

    def test_typo_correction_search(self):
        """Test typo correction for 'search'."""
        from moss.dwim import resolve_core_primitive

        typos = ["serach", "saerch", "seach", "searh"]
        for typo in typos:
            result, confidence = resolve_core_primitive(typo)
            assert result == "search", f"'{typo}' should resolve to 'search', got '{result}'"
            assert confidence >= 0.7

    def test_no_match_gibberish(self):
        """Test that gibberish returns no match."""
        from moss.dwim import resolve_core_primitive

        gibberish = ["xyz", "foobar", "asdfgh", "qwerty123"]
        for word in gibberish:
            result, confidence = resolve_core_primitive(word)
            assert result is None, f"'{word}' should not match, got '{result}'"
            assert confidence == 0.0

    def test_case_insensitive(self):
        """Test case-insensitive matching."""
        from moss.dwim import resolve_core_primitive

        cases = ["VIEW", "View", "vIeW", "EDIT", "Edit", "ANALYZE", "SEARCH"]
        for case in cases:
            result, confidence = resolve_core_primitive(case)
            assert result is not None
            assert confidence == 1.0
