"""DWIM (Do What I Mean) - Semantic tool routing for LLM usage.

# See: docs/dwim-architecture.md

This module provides fuzzy matching and semantic routing for tool discovery,
making Moss interfaces robust against minor variations in how tools are invoked.

Key features:
- Semantic aliases: map conceptual names to canonical tools
- Fuzzy matching: handle typos and variations (Levenshtein distance)
- TF-IDF cosine similarity: smarter semantic matching
- Embedding support: optional vector-based similarity (if available)
- Tool routing: find best tool for a natural language description
- Confidence scoring: know when to auto-correct vs suggest

Tools can be registered via entry points or programmatically.

Entry point group: moss.dwim.tools

Example plugin registration in pyproject.toml:
    [project.entry-points."moss.dwim.tools"]
    my_tool = "my_package.tools:MyToolInfo"
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass
from difflib import SequenceMatcher
from importlib.metadata import entry_points
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Sequence

logger = logging.getLogger(__name__)

# =============================================================================
# Core Primitives - Simple Resolution for 4 Primary Tools
# =============================================================================
# The 4 core primitives are the primary CLI/MCP interface. They subsume the
# older tool set (skeleton, anchors, deps, etc.) into a unified tree model.

CORE_PRIMITIVES = {"view", "edit", "analyze", "search"}

# Aliases for core primitives - maps common terms to canonical names
CORE_ALIASES: dict[str, str] = {
    # view aliases
    "show": "view",
    "look": "view",
    "see": "view",
    "display": "view",
    "read": "view",
    "skeleton": "view",
    "tree": "view",
    "expand": "view",
    "symbols": "view",
    # edit aliases
    "modify": "edit",
    "change": "edit",
    "update": "edit",
    "patch": "edit",
    "fix": "edit",
    "replace": "edit",
    "delete": "edit",
    "insert": "edit",
    # analyze aliases
    "check": "analyze",
    "health": "analyze",
    "complexity": "analyze",
    "security": "analyze",
    "lint": "analyze",
    "audit": "analyze",
    # search aliases
    "find": "search",
    "grep": "search",
    "query": "search",
    "locate": "search",
    "lookup": "search",
}


def resolve_core_primitive(name: str) -> tuple[str | None, float]:
    """Resolve a name to one of the 4 core primitives.

    Uses exact match + basic typo correction (Levenshtein).

    Args:
        name: Tool name to resolve

    Returns:
        Tuple of (canonical_name, confidence).
        Returns (None, 0.0) if no match found.
    """
    normalized = name.lower().strip()

    # Exact match
    if normalized in CORE_PRIMITIVES:
        return normalized, 1.0

    # Alias match
    if normalized in CORE_ALIASES:
        return CORE_ALIASES[normalized], 1.0

    # Typo correction via string similarity
    best_match = None
    best_score = 0.0

    for primitive in CORE_PRIMITIVES:
        score = SequenceMatcher(None, normalized, primitive).ratio()
        if score > best_score:
            best_score = score
            best_match = primitive

    for alias, target in CORE_ALIASES.items():
        score = SequenceMatcher(None, normalized, alias).ratio()
        if score > best_score:
            best_score = score
            best_match = target

    # Threshold: 0.7 for auto-correct (e.g., "veiw" -> "view" = 0.75)
    if best_match and best_score >= 0.7:
        return best_match, best_score

    return None, 0.0


# =============================================================================
# Word Forms and Keyword Expansion
# =============================================================================

# Word form mappings for better semantic matching (handles stemming-like behavior)
# Maps variant forms to their canonical form for keyword expansion
WORD_FORMS: dict[str, list[str]] = {
    "summary": ["summarize", "summarizing", "summarization"],
    "analyze": ["analysis", "analyzing", "analyzer"],
    "validate": ["validation", "validating", "validator"],
    "check": ["checking", "checker"],
    "create": ["creating", "creation", "creator"],
    "build": ["building", "builder"],
    "extract": ["extracting", "extraction", "extractor"],
    "find": ["finding", "finder"],
    "search": ["searching", "searcher"],
    "index": ["indexing", "indexed"],
    "format": ["formatting", "formatted", "formatter"],
    "generate": ["generating", "generation", "generator"],
    "list": ["listing", "lister"],
    "get": ["getting", "getter"],
    "resolve": ["resolving", "resolution", "resolver"],
    "apply": ["applying", "application"],
    "patch": ["patching", "patcher"],
    "commit": ["committing", "committed"],
    "merge": ["merging", "merged"],
    "diff": ["diffing", "difference"],
    "tree": ["trees"],
    "health": ["healthy", "healthcheck"],
    "complex": ["complexity"],
    "depend": ["dependency", "dependencies", "dependent"],
    "import": ["imports", "importing", "imported"],
    "export": ["exports", "exporting", "exported"],
    "skeleton": ["skeletons", "skeletal"],
    "anchor": ["anchors", "anchoring"],
}

# Build reverse mapping: variant -> canonical
_CANONICAL_FORMS: dict[str, str] = {}
for canonical, variants in WORD_FORMS.items():
    _CANONICAL_FORMS[canonical] = canonical
    for variant in variants:
        _CANONICAL_FORMS[variant] = canonical


def expand_keywords(keywords: list[str]) -> list[str]:
    """Expand keywords with word form variants for better matching."""
    expanded = set(keywords)
    for kw in keywords:
        kw_lower = kw.lower()
        # Add variants of this keyword
        if kw_lower in WORD_FORMS:
            expanded.update(WORD_FORMS[kw_lower])
        # If keyword is a variant, add the canonical form
        if kw_lower in _CANONICAL_FORMS:
            expanded.add(_CANONICAL_FORMS[kw_lower])
    return list(expanded)


# =============================================================================
# Embedding-based Semantic Matching
# =============================================================================


class EmbeddingMatcher:
    """Stub for removed embedding matcher.

    Embedding-based matching has been removed in favor of simpler TF-IDF
    and fuzzy matching. This class remains for backward compatibility
    but always reports as unavailable.
    """

    _instance: EmbeddingMatcher | None = None

    def __new__(cls) -> EmbeddingMatcher:
        """Create new instance."""
        return super().__new__(cls)

    def __init__(self) -> None:
        """Initialize stub matcher."""
        pass

    @classmethod
    def get(cls) -> EmbeddingMatcher:
        """Get singleton instance."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def match(
        self, query: str, available_tools: set[str] | None = None, top_k: int = 10
    ) -> list[tuple[str, float]]:
        """Stub: always returns empty list (embeddings removed)."""
        return []

    def is_available(self) -> bool:
        """Stub: embeddings are no longer available."""
        return False

    def get_all_embeddings(self) -> dict[str, Any]:
        """Stub: returns empty dict."""
        return {}

    def get_tool_text(self, tool_name: str) -> str | None:
        """Stub: returns None."""
        return None

    def analyze_similarity(self) -> list[tuple[str, str, float]]:
        """Stub: returns empty list."""
        return []

    def find_similar_to(self, tool_name: str, top_k: int = 10) -> list[tuple[str, float]]:
        """Stub: returns empty list."""
        return []

    def embed_query(self, query: str) -> Any | None:
        """Stub: returns None."""
        return None


def get_embedding_matcher() -> EmbeddingMatcher:
    """Get the global embedding matcher instance (stub, embeddings removed)."""
    return EmbeddingMatcher.get()


# =============================================================================
# Tool Descriptions (for semantic matching)
# =============================================================================


@dataclass
class ToolInfo:
    """Information about a tool for semantic matching."""

    name: str
    description: str
    keywords: list[str]
    parameters: list[str]
    # Weighted example phrases: (phrase, weight). Higher weight = more important.
    # These help distinguish similar tools and improve NL matching.
    examples: list[tuple[str, float]] | None = None


# =============================================================================
# Tool Registry
# =============================================================================

# Registry of tools with semantic information
_TOOLS: dict[str, ToolInfo] = {}


def register_tool(tool: ToolInfo) -> None:
    """Register a tool for semantic routing.

    Args:
        tool: ToolInfo describing the tool
    """
    _TOOLS[tool.name] = tool


def unregister_tool(name: str) -> bool:
    """Unregister a tool.

    Args:
        name: Tool name to remove

    Returns:
        True if tool was removed, False if not found
    """
    if name in _TOOLS:
        del _TOOLS[name]
        return True
    return False


def register_mcp_tool(
    name: str,
    description: str,
    prefix: str = "mcp",
    input_schema: dict | None = None,
) -> ToolInfo:
    """Register an MCP tool into the DWIM registry.

    Extracts keywords from the tool name and description to enable
    natural language routing.

    Args:
        name: MCP tool name (e.g., "read_file", "search")
        description: Tool description from MCP
        prefix: Prefix for the registered tool name (default: "mcp")
        input_schema: Optional JSON schema for parameters

    Returns:
        The created ToolInfo
    """
    # Create prefixed name to avoid collisions with local tools
    full_name = f"{prefix}_{name}" if prefix else name

    # Extract keywords from name and description
    keywords = _extract_keywords_from_mcp(name, description)

    # Extract parameter names from schema
    parameters: list[str] = []
    if input_schema and "properties" in input_schema:
        parameters = list(input_schema["properties"].keys())

    tool = ToolInfo(
        name=full_name,
        description=description,
        keywords=keywords,
        parameters=parameters,
    )
    register_tool(tool)

    # Also add alias from unprefixed name to prefixed name
    if prefix:
        TOOL_ALIASES[name.lower()] = full_name

    return tool


def _extract_keywords_from_mcp(name: str, description: str) -> list[str]:
    """Extract keywords from MCP tool name and description.

    Uses simple heuristics:
    - Split name on underscores
    - Extract significant words from description
    - Skip common stopwords
    """
    stopwords = {
        "a",
        "an",
        "the",
        "is",
        "are",
        "to",
        "from",
        "for",
        "with",
        "on",
        "in",
        "of",
        "and",
        "or",
        "this",
        "that",
        "it",
        "be",
        "as",
        "by",
        "at",
        "can",
        "will",
        "may",
        "should",
        "would",
        "could",
        "must",
        "has",
        "have",
        "had",
        "do",
        "does",
        "did",
        "not",
        "but",
        "if",
        "then",
        "else",
        "when",
        "where",
        "what",
        "which",
        "who",
        "how",
        "all",
        "each",
        "every",
        "any",
        "some",
    }

    keywords = set()

    # Add parts of the tool name
    name_parts = name.lower().replace("-", "_").split("_")
    keywords.update(p for p in name_parts if p and len(p) > 2)

    # Extract words from description
    desc_words = description.lower().split()
    for word in desc_words:
        # Clean punctuation
        clean = word.strip(".,;:!?()[]{}\"'")
        if clean and len(clean) > 3 and clean not in stopwords:
            keywords.add(clean)

    return list(keywords)[:20]  # Limit keywords


def unregister_mcp_tools(prefix: str = "mcp") -> int:
    """Unregister all MCP tools with the given prefix.

    Args:
        prefix: Prefix used when registering (default: "mcp")

    Returns:
        Number of tools unregistered
    """
    prefix_with_underscore = f"{prefix}_"
    to_remove = [name for name in _TOOLS if name.startswith(prefix_with_underscore)]
    for name in to_remove:
        unregister_tool(name)
        # Also remove aliases
        original = name[len(prefix_with_underscore) :]
        if original.lower() in TOOL_ALIASES:
            del TOOL_ALIASES[original.lower()]
    return len(to_remove)


def get_tool(name: str) -> ToolInfo | None:
    """Get a tool by name.

    Args:
        name: Tool name

    Returns:
        ToolInfo or None if not found
    """
    return _TOOLS.get(name)


def _discover_entry_points() -> None:
    """Discover and register tools from entry points."""
    try:
        eps = entry_points(group="moss.dwim.tools")
        for ep in eps:
            try:
                tool_info = ep.load()
                if isinstance(tool_info, ToolInfo):
                    if tool_info.name not in _TOOLS:
                        register_tool(tool_info)
                        logger.debug("Discovered tool: %s", tool_info.name)
                elif callable(tool_info):
                    # Factory function
                    info = tool_info()
                    if isinstance(info, ToolInfo) and info.name not in _TOOLS:
                        register_tool(info)
                        logger.debug("Discovered tool: %s", info.name)
            except (ImportError, AttributeError, TypeError) as e:
                logger.warning("Failed to load tool '%s': %s", ep.name, e)
    except (TypeError, StopIteration):
        pass


def _register_builtin_tools() -> None:
    """Register built-in tools."""
    builtin_tools = [
        ToolInfo(
            name="skeleton",
            description="Extract code structure showing classes, functions, and methods",
            keywords=[
                "outline",
                "hierarchy",
                "overview",
                "symbols",
                "tree",
                "ast",
                "classes",
                "functions",
                "list",
                "show",
                "all",
                "api",
                "definition",
                "public",
            ],
            parameters=["path", "pattern"],
            examples=[
                ("show code structure", 0.5),
                ("what functions are in this file", 0.4),
                ("list all classes and methods", 0.4),
                ("code outline", 0.3),
            ],
        ),
        ToolInfo(
            name="anchors",
            description="Find specific code elements like functions, classes, or methods",
            keywords=[
                "find",
                "locate",
                "definitions",
                "defs",
                "functions",
                "classes",
                "methods",
                "symbols",
                "where",
                "location",
                "named",
            ],
            parameters=["path", "type", "name", "pattern"],
            examples=[
                ("find all classes", 0.5),
                ("locate function definitions", 0.4),
                ("where is this method defined", 0.4),
            ],
        ),
        ToolInfo(
            name="query",
            description="Search and filter code by size, complexity, and inheritance",
            keywords=[
                "search",
                "find",
                "grep",
                "filter",
                "pattern",
                "regex",
                "match",
                "inherits",
                "inherit",
                "subclass",
                "extends",
                "extending",
                "inheritance",
                "base",
                "derived",
                "lines",
                "complex",
                "large",
                "small",
                "size",
                "over",
                "under",
                "long",
                "short",
                "min",
                "max",
                "functions",
                "classes",
                "methods",
            ],
            parameters=["path", "name", "signature", "type", "inherits", "pattern"],
            examples=[
                ("classes that inherit from Base", 0.5),
                ("find large functions over 100 lines", 0.4),
                ("search for pattern in code", 0.3),
            ],
        ),
        ToolInfo(
            name="cfg",
            description="Build control flow graph showing execution paths",
            keywords=[
                "flow",
                "control",
                "graph",
                "execution",
                "branches",
                "paths",
                "complexity",
                "call",
                "logic",
                "branch",
                "loop",
                "if",
                "while",
            ],
            parameters=["path", "function"],
            examples=[
                ("control flow graph", 0.5),
                ("show execution paths", 0.4),
                ("analyze branches", 0.3),
            ],
        ),
        ToolInfo(
            name="deps",
            description="Extract imports and exports showing module dependencies",
            keywords=[
                "imports",
                "exports",
                "dependencies",
                "modules",
                "packages",
                "requires",
                "uses",
                "import",
                "export",
                "dependency",
                "analysis",
                "external",
                "reverse",
            ],
            parameters=["path", "pattern"],
            examples=[
                ("show dependencies", 0.5),
                ("import graph", 0.5),
                ("what does this module import", 0.4),
                ("module dependencies", 0.4),
            ],
        ),
        ToolInfo(
            name="context",
            description="Generate combined view with skeleton, dependencies, and summary",
            keywords=[
                "summary",
                "overview",
                "combined",
                "info",
                "details",
                "comprehensive",
                "explain",
                "about",
                "understand",
                "what",
                "does",
                "describe",
            ],
            parameters=["path"],
            examples=[
                ("explain this file", 0.5),
                ("what does this code do", 0.4),
                ("summarize the module", 0.4),
            ],
        ),
        ToolInfo(
            name="apply_patch",
            description="Apply code changes using anchor-based patching",
            keywords=["edit", "modify", "change", "patch", "update", "fix", "replace"],
            parameters=["file_path", "anchor", "new_content", "edit_type"],
        ),
        # Tree navigation commands
        ToolInfo(
            name="path",
            description="Resolve fuzzy path or symbol to exact file location",
            keywords=["resolve", "find", "where", "location", "file", "fuzzy", "match"],
            parameters=["query"],
        ),
        ToolInfo(
            name="view",
            description="View a node in the codebase tree with structure",
            keywords=["show", "display", "look", "see", "node", "symbol", "class", "function"],
            parameters=["target"],
        ),
        ToolInfo(
            name="search-tree",
            description="Search for symbols in the codebase tree",
            keywords=["search", "find", "symbols", "tree", "grep", "match", "lookup"],
            parameters=["query", "scope"],
        ),
        ToolInfo(
            name="cli_expand",
            description="Show full source code of a symbol",
            keywords=["expand", "source", "code", "full", "body", "implementation", "read"],
            parameters=["target"],
        ),
        ToolInfo(
            name="callers",
            description="Find all callers of a function or method",
            keywords=["callers", "references", "usages", "who", "calls", "uses", "invokes"],
            parameters=["target"],
        ),
        ToolInfo(
            name="callees",
            description="Find what functions a symbol calls",
            keywords=["callees", "calls", "invokes", "uses", "dependencies", "outgoing"],
            parameters=["target"],
        ),
    ]
    for tool in builtin_tools:
        register_tool(tool)


def _register_from_mossapi() -> None:
    """Auto-register tools from MossAPI introspection.

    This discovers all public methods from all sub-APIs and registers them
    as DWIM tools with proper descriptions and keywords extracted from docstrings.
    """
    try:
        from moss.gen.introspect import introspect_api
    except ImportError:
        logger.debug("Could not import introspect_api, skipping MossAPI registration")
        return

    try:
        sub_apis = introspect_api()
    except Exception as e:
        logger.debug("Failed to introspect MossAPI: %s", e)
        return

    for api in sub_apis:
        for method in api.methods:
            # Tool name matches MCP convention: {api}_{method}
            tool_name = f"{api.name}_{method.name}"

            # Skip if already registered (builtin takes precedence)
            if tool_name in _TOOLS:
                continue

            # Extract keywords from method name, API name, and description
            keywords = []

            # Add API name and method name as keywords
            keywords.append(api.name)
            keywords.append(method.name)

            # Split method name on underscores for additional keywords
            keywords.extend(method.name.split("_"))

            # Extract keywords from description (simple word extraction)
            if method.description:
                # Get significant words from description
                desc_words = re.findall(r"\b\w{4,}\b", method.description.lower())
                # Filter common words
                common = {
                    "this",
                    "that",
                    "with",
                    "from",
                    "into",
                    "have",
                    "been",
                    "will",
                    "would",
                    "could",
                    "should",
                }
                keywords.extend(w for w in desc_words if w not in common)

            # Expand keywords with word form variants
            keywords = expand_keywords(keywords)

            # Get parameter names
            params = [p.name for p in method.parameters]

            tool = ToolInfo(
                name=tool_name,
                description=method.description or f"{api.name} {method.name}",
                keywords=list(set(keywords)),  # dedupe
                parameters=params,
            )
            register_tool(tool)
            logger.debug("Registered MossAPI tool: %s", tool_name)


# Auto-register on import
_register_builtin_tools()
_register_from_mossapi()  # Register all MossAPI tools
_discover_entry_points()


def _register_custom_config() -> None:
    """Register custom tools and aliases from user configuration."""
    try:
        from moss.dwim_config import get_config
    except ImportError:
        return

    try:
        config = get_config()
    except Exception as e:
        logger.debug("Failed to load DWIM config: %s", e)
        return

    # Register custom aliases
    for alias, target in config.aliases.items():
        if alias not in TOOL_ALIASES:
            TOOL_ALIASES[alias] = target
            logger.debug("Registered custom alias: %s -> %s", alias, target)

    # Register custom tools
    for tool in config.tools:
        if tool.name not in _TOOLS:
            register_tool(
                ToolInfo(
                    name=tool.name,
                    description=tool.description,
                    keywords=expand_keywords(tool.keywords),
                    parameters=tool.parameters,
                )
            )
            logger.debug("Registered custom tool: %s", tool.name)


_register_custom_config()

# Backwards compatibility alias
TOOL_REGISTRY = _TOOLS

# Semantic aliases: alternative names that map to canonical tools
TOOL_ALIASES: dict[str, str] = {
    # skeleton (most common terms for code structure)
    "symbols": "skeleton",
    "outline": "skeleton",
    "tree": "skeleton",
    "hierarchy": "skeleton",
    "structure": "skeleton",  # Code structure = skeleton
    "overview": "skeleton",  # Code overview = skeleton
    # cli_expand (show full source of a symbol)
    "expand": "cli_expand",
    "fullsource": "cli_expand",
    "source": "cli_expand",
    # skeleton_get_enum_values
    "enum": "skeleton_get_enum_values",
    "enumvalues": "skeleton_get_enum_values",
    "values": "skeleton_get_enum_values",
    # anchors
    "functions": "anchors",
    "classes": "anchors",
    "methods": "anchors",
    "definitions": "anchors",
    "defs": "anchors",
    "locate": "anchors",
    # deps
    "imports": "deps",
    "dependencies": "deps",
    "exports": "deps",
    "modules": "deps",
    # query
    "search": "query",
    "find": "query",
    "grep": "query",
    "filter": "query",
    # cfg
    "flow": "cfg",
    "graph": "cfg",
    "control-flow": "cfg",
    "controlflow": "cfg",
    "paths": "cfg",
    # context
    "summary": "context",
    "info": "context",
    # search_summarize_module
    "module": "search_summarize_module",
    # apply_patch
    "edit": "apply_patch",
    "modify": "apply_patch",
    "patch": "apply_patch",
    "change": "apply_patch",
    # web_fetch
    "fetch": "web_fetch",
    "browse": "web_fetch",
    "url": "web_fetch",
    "webpage": "web_fetch",
    # web_search
    "websearch": "web_search",
    "lookup": "web_search",
    "google": "web_search",
    "duckduckgo": "web_search",
}

# Parameter aliases: alternative parameter names that map to canonical ones
PARAM_ALIASES: dict[str, str] = {
    "file": "path",
    "file_path": "path",
    "filepath": "path",
    "directory": "path",
    "dir": "path",
    "source": "path",
    "target": "path",
    "glob": "pattern",
    "filter": "pattern",
    "regex": "name",
    "match": "name",
    "func": "function",
    "fn": "function",
    "method": "function",
    "base": "inherits",
    "parent": "inherits",
    "extends": "inherits",
    "kind": "type",
    "symbol_type": "type",
}


# =============================================================================
# Fuzzy Matching
# =============================================================================

# Confidence thresholds
AUTO_CORRECT_THRESHOLD = 0.85  # Auto-correct if confidence >= this
CLARIFY_THRESHOLD = 0.6  # Below this, ask for clarification (but still suggest)
SUGGEST_THRESHOLD = 0.3  # Suggest if confidence >= this (lowered from 0.5 for better NL support)


def string_similarity(a: str, b: str) -> float:
    """Calculate similarity ratio between two strings."""
    return SequenceMatcher(None, a.lower(), b.lower()).ratio()


# =============================================================================
# Tool Resolution
# =============================================================================


@dataclass
class ToolMatch:
    """Result of matching a query to a tool."""

    tool: str
    confidence: float
    message: str | None = None


def resolve_tool(tool_name: str) -> ToolMatch:
    """Resolve a tool name to its canonical form.

    Handles:
    - Exact matches
    - Hyphen-to-underscore normalization
    - Semantic aliases
    - Fuzzy matching for typos
    """
    # Normalize: hyphens to underscores, lowercase
    normalized = tool_name.replace("-", "_").lower()

    # Exact match
    if normalized in TOOL_REGISTRY:
        return ToolMatch(tool=normalized, confidence=1.0)

    name_lower = normalized

    # Alias match
    if name_lower in TOOL_ALIASES:
        canonical = TOOL_ALIASES[name_lower]
        return ToolMatch(
            tool=canonical,
            confidence=1.0,
            message=f"'{tool_name}' → '{canonical}'",
        )

    # Fuzzy match against tool names
    best_match = None
    best_score = 0.0

    for name in TOOL_REGISTRY:
        score = string_similarity(tool_name, name)
        if score > best_score:
            best_score = score
            best_match = name

    # Fuzzy match against aliases
    for alias, canonical in TOOL_ALIASES.items():
        score = string_similarity(tool_name, alias)
        if score > best_score:
            best_score = score
            best_match = canonical

    if best_match and best_score >= AUTO_CORRECT_THRESHOLD:
        return ToolMatch(
            tool=best_match,
            confidence=best_score,
            message=f"'{tool_name}' → '{best_match}' (auto-corrected)",
        )

    if best_match and best_score >= CLARIFY_THRESHOLD:
        # Confident enough to execute but note uncertainty
        return ToolMatch(
            tool=best_match,
            confidence=best_score,
            message=f"Matched '{tool_name}' → '{best_match}' (confidence: {best_score:.0%})",
        )

    if best_match and best_score >= SUGGEST_THRESHOLD:
        # Low confidence - suggest clarification
        return ToolMatch(
            tool=best_match,
            confidence=best_score,
            message=f"Unclear: '{tool_name}'. Did you mean '{best_match}'? Be more specific.",
        )

    return ToolMatch(tool=tool_name, confidence=0.0, message=f"Unknown tool: {tool_name}")


def resolve_parameter(param_name: str, tool: str | None = None) -> str:
    """Resolve a parameter name to its canonical form."""
    if param_name in PARAM_ALIASES:
        return PARAM_ALIASES[param_name]
    return param_name


def normalize_parameters(params: dict, tool: str | None = None) -> dict:
    """Normalize parameter names to their canonical forms."""
    return {resolve_parameter(k, tool): v for k, v in params.items()}


# =============================================================================
# Semantic Routing with Embeddings
# =============================================================================


class ToolRouter:
    """Smart tool router using sentence embeddings.

    Combines:
    - Fast path: exact name/alias matching
    - Embedding similarity: semantic matching for natural language
    - Fuzzy matching: typo tolerance
    """

    def __init__(self) -> None:
        """Initialize the router."""
        self._matcher = get_embedding_matcher()

    def analyze_intent(
        self, query: str, available_tools: Sequence[str] | None = None
    ) -> list[ToolMatch]:
        """Analyze a natural language query to find the best matching tools.

        Matching strategy:
        1. Tool-like queries (1-2 words, no articles): exact → typo → alias
        2. Natural language queries: straight to semantic embedding matching

        Args:
            query: Natural language description of what the user wants
            available_tools: Limit search to these tools (default: all)

        Returns:
            List of ToolMatch sorted by confidence (highest first)
        """
        tools = set(available_tools) if available_tools else set(TOOL_REGISTRY.keys())

        # Normalize: hyphens to underscores, lowercase
        normalized = query.replace("-", "_").lower()
        words = normalized.split()

        if not words:
            return []

        # NL markers that shouldn't be treated as tool names/aliases
        nl_markers = {
            "and",
            "or",
            "but",
            "the",
            "a",
            "an",
            "for",
            "to",
            "in",
            "of",
            "with",
            "what",
            "how",
            "where",
            "why",
            "which",
            "that",
            "this",
            "is",
            "are",
            "my",
            "all",
            "show",
            "find",
            "get",
        }

        # === STEP 1: Try exact matching first (works for any query) ===
        joined = "_".join(words)

        # 1a. Exact tool name match
        if joined in tools:
            return [ToolMatch(tool=joined, confidence=1.0)]

        # 1b. Exact alias match
        if joined in TOOL_ALIASES:
            target = TOOL_ALIASES[joined]
            if target in tools:
                return [ToolMatch(tool=target, confidence=1.0)]

        # 1c. First word is exact tool name (e.g., "skeleton src/main.py")
        # Skip if first word is an NL marker like "find", "show", "get"
        if words[0] not in nl_markers and words[0] in tools:
            return [ToolMatch(tool=words[0], confidence=1.0)]

        # 1d. First word is exact alias (skip if NL marker)
        if words[0] not in nl_markers and words[0] in TOOL_ALIASES:
            target = TOOL_ALIASES[words[0]]
            if target in tools:
                return [ToolMatch(tool=target, confidence=1.0)]

        # === STEP 2: Typo correction for tool-like queries ===
        # Tool-like = short query without many NL markers
        nl_word_count = sum(1 for w in words if w in nl_markers)
        is_tool_like = len(words) <= 3 and nl_word_count <= 1

        if is_tool_like:
            typo_matches = []

            # Check full query as tool name typo
            for tool_name in tools:
                if tool_name not in TOOL_REGISTRY:
                    continue
                sim = string_similarity(joined, tool_name)
                if sim >= 0.7:
                    typo_matches.append((tool_name, sim))

            # Check reversed word order (e.g., "list todo" -> "todo_list")
            if len(words) >= 2:
                reversed_joined = "_".join(reversed(words[:2]))
                for tool_name in tools:
                    if tool_name not in TOOL_REGISTRY:
                        continue
                    sim = string_similarity(reversed_joined, tool_name)
                    if sim >= 0.8:  # Higher threshold for reversed
                        typo_matches.append((tool_name, sim * 0.95))

            # Check first word as tool base typo (skip if first word is NL marker)
            if words[0] not in nl_markers:
                for tool_name in tools:
                    if tool_name not in TOOL_REGISTRY:
                        continue
                    base = tool_name.split("_")[0]
                    sim = string_similarity(words[0], base)
                    if sim >= 0.75 and sim < 1.0:  # Typo, not exact
                        typo_matches.append((tool_name, sim * 0.9))

            # Check aliases for typos
            for alias, target in TOOL_ALIASES.items():
                if target not in tools:
                    continue
                sim = string_similarity(joined, alias)
                if sim >= 0.7:
                    typo_matches.append((target, sim))

            if typo_matches:
                typo_matches.sort(key=lambda x: (-x[1], len(x[0])))
                seen = set()
                results = []
                for tool, score in typo_matches:
                    if tool not in seen:
                        seen.add(tool)
                        results.append(ToolMatch(tool=tool, confidence=score))
                return results[:10]

        # === NATURAL LANGUAGE QUERIES ===
        # Use semantic embedding matching
        embedding_results = self._matcher.match(query, tools)

        if embedding_results:
            matches = []
            for tool_name, sim in embedding_results:
                # Scale: sim 0.65 → conf 0.30, sim 0.9 → conf 0.95
                # Below 0.65: very low confidence
                if sim < 0.5:
                    confidence = 0.1
                elif sim < 0.65:
                    confidence = 0.1 + (sim - 0.5) * 1.33  # 0.5→0.1, 0.65→0.3
                else:
                    confidence = min(0.95, 0.3 + (sim - 0.65) * 2.6)  # 0.65→0.3, 0.9→0.95
                matches.append(ToolMatch(tool=tool_name, confidence=confidence))
            return matches

        # Fallback: fuzzy string matching (if embeddings unavailable)
        matches = []
        for tool_name in tools:
            if tool_name not in TOOL_REGISTRY:
                continue
            sim = string_similarity(normalized, tool_name)
            if sim > 0.5:
                matches.append(ToolMatch(tool=tool_name, confidence=sim * 0.8))

        matches.sort(key=lambda m: m.confidence, reverse=True)
        return matches

    def suggest_tool(self, query: str) -> ToolMatch | None:
        """Suggest the best tool for a query.

        Returns the top match if confidence is above threshold, None otherwise.
        """
        matches = self.analyze_intent(query)
        if matches and matches[0].confidence >= SUGGEST_THRESHOLD:
            return matches[0]
        return None


# Global router instance (lazy initialization)
_router: ToolRouter | None = None


def get_router() -> ToolRouter:
    """Get the global tool router instance."""
    global _router
    if _router is None:
        _router = ToolRouter()
    return _router


def analyze_intent(query: str, available_tools: Sequence[str] | None = None) -> list[ToolMatch]:
    """Analyze a natural language query to find the best matching tools.

    This is the main entry point for semantic routing.

    Args:
        query: Natural language description of what the user wants
        available_tools: Limit search to these tools (default: all)

    Returns:
        List of ToolMatch sorted by confidence (highest first)
    """
    return get_router().analyze_intent(query, available_tools)


def suggest_tool(query: str) -> ToolMatch | None:
    """Suggest the best tool for a query, with confidence scoring.

    Returns the top match if confidence is above threshold, None otherwise.
    """
    return get_router().suggest_tool(query)


def suggest_tools(query: str, top_k: int = 3) -> list[ToolMatch]:
    """Suggest top-k tools for a query, regardless of threshold.

    Always returns up to top_k results, even if confidence is low.
    This is useful for showing alternatives when no confident match exists.

    Args:
        query: Natural language description of what the user wants
        top_k: Maximum number of suggestions to return (default: 3)

    Returns:
        List of ToolMatch sorted by confidence (highest first).
        Each match has a message indicating confidence level.
    """
    matches = analyze_intent(query)

    # Add messages based on confidence
    results = []
    for match in matches[:top_k]:
        if match.confidence >= AUTO_CORRECT_THRESHOLD:
            match.message = "High confidence match"
        elif match.confidence >= SUGGEST_THRESHOLD:
            match.message = "Suggested match"
        else:
            match.message = "Low confidence - consider alternatives"
        results.append(match)

    return results


# =============================================================================
# Introspection
# =============================================================================


def list_tools() -> list[dict]:
    """List all available tools with their descriptions."""
    return [
        {
            "name": info.name,
            "description": info.description,
            "keywords": info.keywords,
            "parameters": info.parameters,
        }
        for info in _TOOLS.values()
    ]


def list_tool_names() -> list[str]:
    """List all registered tool names."""
    return list(_TOOLS.keys())


def get_tool_info(tool_name: str) -> dict | None:
    """Get detailed information about a tool."""
    # Resolve to canonical name
    match = resolve_tool(tool_name)
    if match.confidence < SUGGEST_THRESHOLD:
        return None

    info = _TOOLS.get(match.tool)
    if not info:
        return None

    return {
        "name": info.name,
        "description": info.description,
        "keywords": info.keywords,
        "parameters": info.parameters,
        "aliases": [alias for alias, target in TOOL_ALIASES.items() if target == info.name],
    }


__all__ = [
    "AUTO_CORRECT_THRESHOLD",
    "CLARIFY_THRESHOLD",
    "CORE_ALIASES",
    "CORE_PRIMITIVES",
    "PARAM_ALIASES",
    "SUGGEST_THRESHOLD",
    "TOOL_ALIASES",
    "TOOL_REGISTRY",
    "ToolInfo",
    "ToolMatch",
    "ToolRouter",
    "analyze_intent",
    "get_router",
    "get_tool",
    "get_tool_info",
    "list_tool_names",
    "list_tools",
    "normalize_parameters",
    "register_mcp_tool",
    "register_tool",
    "resolve_core_primitive",
    "resolve_parameter",
    "resolve_tool",
    "suggest_tool",
    "suggest_tools",
    "unregister_mcp_tools",
    "unregister_tool",
]
