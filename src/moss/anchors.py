"""Anchor Resolution: Fuzzy AST matching for structural edits."""

from __future__ import annotations

import ast
import difflib
from dataclasses import dataclass, field
from enum import Enum, auto


class AnchorType(Enum):
    """Types of code anchors."""

    FUNCTION = auto()
    CLASS = auto()
    METHOD = auto()
    VARIABLE = auto()
    IMPORT = auto()


@dataclass(frozen=True)
class Anchor:
    """A reference to a code location."""

    type: AnchorType
    name: str
    context: str | None = None  # Parent class/function name
    signature: str | None = None  # For disambiguation (e.g., arg types)


@dataclass
class AnchorMatch:
    """A resolved anchor match in the AST."""

    anchor: Anchor
    node: ast.AST
    lineno: int
    end_lineno: int
    col_offset: int
    end_col_offset: int
    score: float  # 0.0 to 1.0 match confidence
    context_chain: list[str] = field(default_factory=list)  # e.g., ["MyClass", "inner_method"]

    @property
    def span(self) -> tuple[int, int]:
        """Line span (1-indexed, inclusive)."""
        return (self.lineno, self.end_lineno)

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        ctx = ".".join(self.context_chain) + "." if self.context_chain else ""
        return f"{ctx}{self.anchor.name} @ line {self.lineno}-{self.end_lineno} ({self.score:.0%})"


class AmbiguousAnchorError(Exception):
    """Multiple nodes match the anchor."""

    def __init__(self, anchor: Anchor, matches: list[AnchorMatch]):
        self.anchor = anchor
        self.matches = matches
        locations = [f"line {m.lineno}" for m in matches]
        super().__init__(f"Ambiguous anchor '{anchor.name}': matches at {', '.join(locations)}")


class AnchorNotFoundError(Exception):
    """No nodes match the anchor."""

    def __init__(self, anchor: Anchor, suggestions: list[str] | None = None):
        self.anchor = anchor
        self.suggestions = suggestions or []
        msg = f"Anchor '{anchor.name}' not found"
        if suggestions:
            msg += f". Did you mean: {', '.join(suggestions[:3])}?"
        super().__init__(msg)


class AnchorResolver(ast.NodeVisitor):
    """Resolve anchors to AST nodes using fuzzy matching."""

    def __init__(self, source: str, min_score: float = 0.6):
        self.source = source
        self.lines = source.splitlines()
        self.min_score = min_score
        self._context_stack: list[str] = []
        self._matches: list[AnchorMatch] = []
        self._all_names: list[str] = []  # For suggestions
        self._target_anchor: Anchor | None = None

    def _similarity(self, a: str, b: str) -> float:
        """Calculate string similarity ratio."""
        return difflib.SequenceMatcher(None, a.lower(), b.lower()).ratio()

    def _match_score(self, node_name: str, node_context: list[str]) -> float:
        """Calculate match score for a node against target anchor."""
        if self._target_anchor is None:
            return 0.0

        anchor = self._target_anchor

        # Name similarity
        name_score = self._similarity(node_name, anchor.name)
        if name_score < self.min_score:
            return 0.0

        # Context matching - weighted more heavily when context is specified
        context_score = 1.0
        if anchor.context:
            if node_context:
                # Check if anchor.context matches any context in chain
                best_ctx = max(self._similarity(anchor.context, ctx) for ctx in node_context)
                context_score = best_ctx
            else:
                context_score = 0.3  # Strong penalty for missing context

            # When context is specified, weight it more heavily
            return name_score * 0.5 + context_score * 0.5

        # No context specified - just use name
        return name_score

    def _create_match(self, node: ast.AST, name: str) -> AnchorMatch | None:
        """Create a match if score is sufficient."""
        if self._target_anchor is None:
            return None

        context = list(self._context_stack)
        score = self._match_score(name, context)

        if score >= self.min_score:
            # All nodes passed to _create_match have location info
            lineno = getattr(node, "lineno", 0)
            col_offset = getattr(node, "col_offset", 0)
            return AnchorMatch(
                anchor=self._target_anchor,
                node=node,
                lineno=lineno,
                end_lineno=getattr(node, "end_lineno", lineno),
                col_offset=col_offset,
                end_col_offset=getattr(node, "end_col_offset", col_offset),
                score=score,
                context_chain=context,
            )
        return None

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self._all_names.append(node.name)

        if self._target_anchor and self._target_anchor.type in (
            AnchorType.FUNCTION,
            AnchorType.METHOD,
        ):
            match = self._create_match(node, node.name)
            if match:
                self._matches.append(match)

        self._context_stack.append(node.name)
        self.generic_visit(node)
        self._context_stack.pop()

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self._all_names.append(node.name)

        if self._target_anchor and self._target_anchor.type in (
            AnchorType.FUNCTION,
            AnchorType.METHOD,
        ):
            match = self._create_match(node, node.name)
            if match:
                self._matches.append(match)

        self._context_stack.append(node.name)
        self.generic_visit(node)
        self._context_stack.pop()

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        self._all_names.append(node.name)

        if self._target_anchor and self._target_anchor.type == AnchorType.CLASS:
            match = self._create_match(node, node.name)
            if match:
                self._matches.append(match)

        self._context_stack.append(node.name)
        self.generic_visit(node)
        self._context_stack.pop()

    def visit_Assign(self, node: ast.Assign) -> None:
        for target in node.targets:
            if isinstance(target, ast.Name):
                self._all_names.append(target.id)

                if self._target_anchor and self._target_anchor.type == AnchorType.VARIABLE:
                    match = self._create_match(node, target.id)
                    if match:
                        self._matches.append(match)

        self.generic_visit(node)

    def visit_Import(self, node: ast.Import) -> None:
        if self._target_anchor and self._target_anchor.type == AnchorType.IMPORT:
            for alias in node.names:
                name = alias.asname or alias.name
                self._all_names.append(name)
                match = self._create_match(node, name)
                if match:
                    self._matches.append(match)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        if self._target_anchor and self._target_anchor.type == AnchorType.IMPORT:
            for alias in node.names:
                name = alias.asname or alias.name
                self._all_names.append(name)
                match = self._create_match(node, name)
                if match:
                    self._matches.append(match)

    def resolve(self, anchor: Anchor) -> AnchorMatch:
        """Resolve an anchor to a single AST node.

        Raises:
            AnchorNotFoundError: If no matches found
            AmbiguousAnchorError: If multiple matches found
        """
        self._target_anchor = anchor
        self._matches = []
        self._all_names = []

        tree = ast.parse(self.source)
        self.visit(tree)

        if not self._matches:
            # Find suggestions
            suggestions = difflib.get_close_matches(anchor.name, self._all_names, n=3, cutoff=0.4)
            raise AnchorNotFoundError(anchor, suggestions)

        if len(self._matches) == 1:
            return self._matches[0]

        # Multiple matches - check if one is clearly best
        self._matches.sort(key=lambda m: m.score, reverse=True)
        best = self._matches[0]
        second = self._matches[1]

        # If best is significantly better (>0.2 difference), use it
        if best.score - second.score > 0.2:
            return best

        raise AmbiguousAnchorError(anchor, self._matches)

    def resolve_all(self, anchor: Anchor) -> list[AnchorMatch]:
        """Resolve an anchor to all matching nodes."""
        self._target_anchor = anchor
        self._matches = []
        self._all_names = []

        tree = ast.parse(self.source)
        self.visit(tree)

        return sorted(self._matches, key=lambda m: m.score, reverse=True)


def resolve_anchor(source: str, anchor: Anchor, min_score: float = 0.6) -> AnchorMatch:
    """Convenience function to resolve an anchor in source code."""
    resolver = AnchorResolver(source, min_score)
    return resolver.resolve(anchor)


def find_anchors(source: str, anchor: Anchor, min_score: float = 0.6) -> list[AnchorMatch]:
    """Find all matches for an anchor in source code."""
    resolver = AnchorResolver(source, min_score)
    return resolver.resolve_all(anchor)
