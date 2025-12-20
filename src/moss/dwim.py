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
import math
import re
from collections import Counter
from dataclasses import dataclass, field
from difflib import SequenceMatcher
from importlib.metadata import entry_points
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Sequence

logger = logging.getLogger(__name__)

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
# TF-IDF Cosine Similarity (pure Python implementation)
# =============================================================================


def tokenize(text: str) -> list[str]:
    """Tokenize text into lowercase words."""
    return re.findall(r"\b\w+\b", text.lower())


def compute_tf(tokens: list[str]) -> dict[str, float]:
    """Compute term frequency for tokens."""
    counts = Counter(tokens)
    total = len(tokens)
    return {word: count / total for word, count in counts.items()} if total > 0 else {}


def compute_idf(documents: list[list[str]]) -> dict[str, float]:
    """Compute inverse document frequency across documents."""
    n_docs = len(documents)
    if n_docs == 0:
        return {}

    # Count how many documents contain each word
    doc_freq: Counter[str] = Counter()
    for doc in documents:
        doc_freq.update(set(doc))

    # IDF = log(N / df) with smoothing
    return {word: math.log((n_docs + 1) / (df + 1)) + 1 for word, df in doc_freq.items()}


def compute_tfidf(tokens: list[str], idf: dict[str, float]) -> dict[str, float]:
    """Compute TF-IDF vector for tokens."""
    tf = compute_tf(tokens)
    return {word: tf_val * idf.get(word, 1.0) for word, tf_val in tf.items()}


def cosine_similarity(vec1: dict[str, float], vec2: dict[str, float]) -> float:
    """Compute cosine similarity between two sparse vectors."""
    # Find common keys
    common_keys = set(vec1.keys()) & set(vec2.keys())

    if not common_keys:
        return 0.0

    # Dot product
    dot_product = sum(vec1[k] * vec2[k] for k in common_keys)

    # Magnitudes
    mag1 = math.sqrt(sum(v * v for v in vec1.values()))
    mag2 = math.sqrt(sum(v * v for v in vec2.values()))

    if mag1 == 0 or mag2 == 0:
        return 0.0

    return dot_product / (mag1 * mag2)


@dataclass
class TFIDFIndex:
    """TF-IDF index for semantic similarity."""

    documents: list[str] = field(default_factory=list)
    doc_tokens: list[list[str]] = field(default_factory=list)
    idf: dict[str, float] = field(default_factory=dict)
    doc_vectors: list[dict[str, float]] = field(default_factory=list)

    def add_document(self, text: str) -> int:
        """Add a document and return its index."""
        self.documents.append(text)
        tokens = tokenize(text)
        self.doc_tokens.append(tokens)
        # Recompute IDF with new document
        self.idf = compute_idf(self.doc_tokens)
        # Recompute all vectors with new IDF
        self.doc_vectors = [compute_tfidf(t, self.idf) for t in self.doc_tokens]
        return len(self.documents) - 1

    def query(self, text: str, top_k: int = 5) -> list[tuple[int, float]]:
        """Query for most similar documents.

        Returns list of (doc_index, similarity) tuples.
        """
        if not self.documents:
            return []

        query_tokens = tokenize(text)
        query_vec = compute_tfidf(query_tokens, self.idf)

        # Compute similarities
        similarities = [
            (i, cosine_similarity(query_vec, doc_vec)) for i, doc_vec in enumerate(self.doc_vectors)
        ]

        # Sort by similarity descending
        similarities.sort(key=lambda x: x[1], reverse=True)
        return similarities[:top_k]


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
            except Exception as e:
                logger.warning("Failed to load tool '%s': %s", ep.name, e)
    except Exception:
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

# Backwards compatibility alias
TOOL_REGISTRY = _TOOLS

# Semantic aliases: alternative names that map to canonical tools
TOOL_ALIASES: dict[str, str] = {
    # skeleton
    "symbols": "skeleton",
    "outline": "skeleton",
    "tree": "skeleton",
    "hierarchy": "skeleton",
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
    "overview": "context",
    "info": "context",
    # search_summarize_module
    "structure": "search_summarize_module",
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
SUGGEST_THRESHOLD = 0.3  # Suggest if confidence >= this (lowered from 0.5 for better NL support)


def string_similarity(a: str, b: str) -> float:
    """Calculate similarity ratio between two strings."""
    return SequenceMatcher(None, a.lower(), b.lower()).ratio()


def normalize_word(word: str) -> str:
    """Normalize a word to its canonical form using word form mappings."""
    return _CANONICAL_FORMS.get(word.lower(), word.lower())


def keyword_match_score(query: str, keywords: list[str]) -> float:
    """Score how well a query matches a list of keywords.

    Uses word form normalization so "summarize" matches "summary".
    Optimized to reward matched keywords rather than penalize tools with many keywords.
    """
    if not keywords:
        return 0.0

    query_lower = query.lower()
    query_words = query_lower.split()
    query_words_set = set(query_words)

    # Normalize query words to canonical forms
    query_canonical = {normalize_word(w) for w in query_words_set}

    # Normalize keywords to canonical forms
    keyword_canonical = {normalize_word(kw) for kw in keywords}

    # Overlap score: proportion of query words matched
    overlap = len(query_canonical & keyword_canonical)
    overlap_score = overlap / len(query_words_set) if query_words_set else 0.0

    # First word (action) bonus: if first word matches, it's very likely the right tool
    first_word = normalize_word(query_words[0]) if query_words else ""
    first_word_match = 0.3 if first_word in keyword_canonical else 0.0

    # Best partial match score (for typo tolerance)
    best_partial = 0.0
    if keywords:
        for qw in query_words_set:
            for kw in keywords:
                sim = string_similarity(qw, kw)
                if sim > best_partial:
                    best_partial = sim

    # Combine scores
    # - Overlap: 50% weight (how many query words match keywords)
    # - First word: 30% bonus if action word matches
    # - Best partial: 20% for typo tolerance
    score = overlap_score * 0.5 + first_word_match + best_partial * 0.2

    return min(score, 1.0)


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
    - Semantic aliases
    - Fuzzy matching for typos
    """
    # Exact match
    if tool_name in TOOL_REGISTRY:
        return ToolMatch(tool=tool_name, confidence=1.0)

    name_lower = tool_name.lower()

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

    if best_match and best_score >= SUGGEST_THRESHOLD:
        return ToolMatch(
            tool=best_match,
            confidence=best_score,
            message=f"Unknown tool '{tool_name}'. Did you mean '{best_match}'?",
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
# Semantic Routing with TF-IDF
# =============================================================================


class ToolRouter:
    """Smart tool router using TF-IDF cosine similarity.

    Combines multiple signals for robust matching:
    - TF-IDF cosine similarity on tool descriptions
    - Keyword matching
    - Fuzzy string matching for typos
    - Alias resolution
    """

    def __init__(self) -> None:
        """Initialize the router with tool descriptions."""
        self._index = TFIDFIndex()
        self._tool_names: list[str] = []

        # Build index from tool descriptions and keywords
        for tool_name, info in TOOL_REGISTRY.items():
            # Combine description and keywords for richer matching
            full_text = f"{info.name} {info.description} {' '.join(info.keywords)}"
            self._index.add_document(full_text)
            self._tool_names.append(tool_name)

    def analyze_intent(
        self, query: str, available_tools: Sequence[str] | None = None
    ) -> list[ToolMatch]:
        """Analyze a natural language query to find the best matching tools.

        Uses TF-IDF cosine similarity combined with keyword matching
        and fuzzy string matching for comprehensive coverage.

        Args:
            query: Natural language description of what the user wants
            available_tools: Limit search to these tools (default: all)

        Returns:
            List of ToolMatch sorted by confidence (highest first)
        """
        tools = set(available_tools) if available_tools else set(TOOL_REGISTRY.keys())

        # Fast path: if first word is a recognized tool name or alias, use it directly
        # This prevents "expand Patch" from matching patch tools due to the second word
        # But skip this for natural language queries (detected by common words)
        query_words = query.lower().split()
        natural_lang_words = {
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
        }
        is_natural_language = any(w in natural_lang_words for w in query_words[1:])

        if query_words and not is_natural_language:
            first_word = query_words[0]
            # Check for alias match
            if first_word in TOOL_ALIASES:
                tool = TOOL_ALIASES[first_word]
                if tool in tools:
                    return [ToolMatch(tool=tool, confidence=1.0)]
            # Check for direct tool name match (first word matches tool base name)
            for tool_name in tools:
                if tool_name not in TOOL_REGISTRY:
                    continue
                tool_base = tool_name.split("_")[0]
                if first_word == tool_base or first_word == tool_name:
                    return [ToolMatch(tool=tool_name, confidence=1.0)]

        # Expand query with word form variants for better TF-IDF matching
        # e.g., "summarize" -> "summarize summary summarization summarizing"
        expanded_words = set(query_words)
        for word in query_words:
            if word in WORD_FORMS:
                expanded_words.update(WORD_FORMS[word])
            if word in _CANONICAL_FORMS:
                canonical = _CANONICAL_FORMS[word]
                expanded_words.add(canonical)
                if canonical in WORD_FORMS:
                    expanded_words.update(WORD_FORMS[canonical])
        expanded_query = " ".join(expanded_words)

        # Get TF-IDF similarities using expanded query
        tfidf_results = self._index.query(expanded_query, top_k=len(self._tool_names))
        tfidf_scores = {
            self._tool_names[idx]: score
            for idx, score in tfidf_results
            if self._tool_names[idx] in tools
        }

        # Extract first word (usually the action) for special handling
        query_words = query.lower().split()
        first_word = normalize_word(query_words[0]) if query_words else ""

        matches = []
        for tool_name in tools:
            if tool_name not in TOOL_REGISTRY:
                continue

            tool_info = TOOL_REGISTRY[tool_name]

            # TF-IDF cosine similarity
            tfidf_score = tfidf_scores.get(tool_name, 0.0)

            # Keyword matching (includes first-word bonus)
            keyword_score = keyword_match_score(query, tool_info.keywords)

            # Fuzzy string matching (for typos)
            name_score = string_similarity(query, tool_name)
            desc_score = string_similarity(query, tool_info.description)

            # Exact name match boost: if first word IS the tool name, strong signal
            # e.g., "skeleton foo.py" -> skeleton tool gets boost
            # Common verbs like "search", "find" don't give boost because they're generic
            common_verbs = {"search", "find", "show", "get", "list", "check", "run", "what"}
            tool_base = tool_name.split("_")[0]  # "skeleton_format" -> "skeleton"
            exact_name_boost = 0.0
            if first_word == tool_base or first_word == tool_name:
                if first_word not in common_verbs:
                    exact_name_boost = 0.4  # Full boost for specific tool names
            elif tool_base in query.lower() and tool_base not in common_verbs:
                exact_name_boost = 0.2

            # Combined score (weighted)
            confidence = (
                tfidf_score * 0.20
                + keyword_score * 0.40
                + desc_score * 0.10
                + name_score * 0.10
                + exact_name_boost
            )

            if confidence > 0.1:  # Minimum threshold
                matches.append(ToolMatch(tool=tool_name, confidence=confidence))

        # Sort by confidence
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
    "PARAM_ALIASES",
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
    "register_tool",
    "resolve_parameter",
    "resolve_tool",
    "suggest_tool",
    "suggest_tools",
    "unregister_tool",
]
