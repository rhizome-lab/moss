"""DWIM (Do What I Mean) - Semantic tool routing for LLM usage.

This module provides fuzzy matching and semantic routing for tool discovery,
making Moss interfaces robust against minor variations in how tools are invoked.

Key features:
- Semantic aliases: map conceptual names to canonical tools
- Fuzzy matching: handle typos and variations (Levenshtein distance)
- TF-IDF cosine similarity: smarter semantic matching
- Embedding support: optional vector-based similarity (if available)
- Tool routing: find best tool for a natural language description
- Confidence scoring: know when to auto-correct vs suggest
"""

from __future__ import annotations

import math
import re
from collections import Counter
from dataclasses import dataclass, field
from difflib import SequenceMatcher
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Sequence


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


# Registry of tools with semantic information
TOOL_REGISTRY: dict[str, ToolInfo] = {
    "skeleton": ToolInfo(
        name="skeleton",
        description="Extract code structure showing classes, functions, and methods",
        keywords=[
            "structure",
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
    "anchors": ToolInfo(
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
    "query": ToolInfo(
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
    "cfg": ToolInfo(
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
    "deps": ToolInfo(
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
    "context": ToolInfo(
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
    "apply_patch": ToolInfo(
        name="apply_patch",
        description="Apply code changes using anchor-based patching",
        keywords=["edit", "modify", "change", "patch", "update", "fix", "replace"],
        parameters=["file_path", "anchor", "new_content", "edit_type"],
    ),
}

# Semantic aliases: alternative names that map to canonical tools
TOOL_ALIASES: dict[str, str] = {
    # skeleton
    "structure": "skeleton",
    "symbols": "skeleton",
    "outline": "skeleton",
    "tree": "skeleton",
    "hierarchy": "skeleton",
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
    # apply_patch
    "edit": "apply_patch",
    "modify": "apply_patch",
    "patch": "apply_patch",
    "change": "apply_patch",
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


def keyword_match_score(query: str, keywords: list[str]) -> float:
    """Score how well a query matches a list of keywords."""
    query_lower = query.lower()
    query_words = set(query_lower.split())

    # Direct word match
    direct_matches = sum(1 for kw in keywords if kw in query_lower)

    # Word overlap
    keyword_words = set(kw.lower() for kw in keywords)
    overlap = len(query_words & keyword_words)

    # Partial matches
    partial = sum(max(string_similarity(qw, kw) for kw in keywords) for qw in query_words)

    # Combine scores (weighted)
    total_keywords = len(keywords)
    if total_keywords == 0:
        return 0.0

    score = (
        (direct_matches / total_keywords) * 0.5
        + (overlap / max(len(query_words), 1)) * 0.3
        + (partial / max(len(query_words), 1)) * 0.2
    )

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

        # Get TF-IDF similarities
        tfidf_results = self._index.query(query, top_k=len(self._tool_names))
        tfidf_scores = {
            self._tool_names[idx]: score
            for idx, score in tfidf_results
            if self._tool_names[idx] in tools
        }

        matches = []
        for tool_name in tools:
            if tool_name not in TOOL_REGISTRY:
                continue

            tool_info = TOOL_REGISTRY[tool_name]

            # TF-IDF cosine similarity (primary signal)
            tfidf_score = tfidf_scores.get(tool_name, 0.0)

            # Keyword matching (secondary signal)
            keyword_score = keyword_match_score(query, tool_info.keywords)

            # Fuzzy string matching (for typos)
            name_score = string_similarity(query, tool_name)
            desc_score = string_similarity(query, tool_info.description)

            # Combined score (weighted)
            # TF-IDF is the primary signal, keywords provide domain knowledge
            confidence = (
                tfidf_score * 0.4 + keyword_score * 0.35 + desc_score * 0.15 + name_score * 0.1
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
        for info in TOOL_REGISTRY.values()
    ]


def get_tool_info(tool_name: str) -> dict | None:
    """Get detailed information about a tool."""
    # Resolve to canonical name
    match = resolve_tool(tool_name)
    if match.confidence < SUGGEST_THRESHOLD:
        return None

    info = TOOL_REGISTRY.get(match.tool)
    if not info:
        return None

    return {
        "name": info.name,
        "description": info.description,
        "keywords": info.keywords,
        "parameters": info.parameters,
        "aliases": [alias for alias, target in TOOL_ALIASES.items() if target == info.name],
    }
