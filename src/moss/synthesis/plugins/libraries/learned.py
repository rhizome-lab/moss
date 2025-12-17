"""Learned library plugin with frequency-based abstraction learning.

Learns reusable abstractions from successful synthesis runs by:
1. Tracking common patterns across solutions
2. Extracting repeated code structures (functions, expressions)
3. Creating abstractions for frequently occurring patterns

Inspired by DreamCoder's library learning, but uses frequency-based
detection rather than compression-based optimization.

Usage:
    from moss.synthesis.plugins.libraries.learned import LearnedLibrary

    library = LearnedLibrary()

    # Record solutions as they are generated
    library.record_solution("def add(a, b): return a + b", spec)
    library.record_solution("def sub(a, b): return a - b", spec2)

    # Learn abstractions from recorded patterns
    abstraction = await library.learn_abstraction(solutions, spec)

    # Search for relevant abstractions
    matches = library.search_abstractions(spec, context)
"""

from __future__ import annotations

import ast
import hashlib
import json
import re
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

from moss.synthesis.plugins.protocols import (
    Abstraction,
    LibraryMetadata,
    LibraryPlugin,
)

if TYPE_CHECKING:
    from moss.synthesis.types import Context, Specification


# =============================================================================
# Pattern Types
# =============================================================================


@dataclass
class CodePattern:
    """A detected code pattern that may become an abstraction.

    Attributes:
        template: The pattern template (code with holes)
        examples: Example code snippets matching this pattern
        frequency: How often this pattern has been seen
        category: Pattern category (function, expression, statement, etc.)
        signature: Optional type signature pattern
    """

    template: str
    examples: list[str] = field(default_factory=list)
    frequency: int = 0
    category: str = "expression"
    signature: str | None = None

    @property
    def hash(self) -> str:
        """Compute hash for pattern deduplication."""
        return hashlib.md5(self.template.encode()).hexdigest()[:12]


@dataclass
class PatternMatch:
    """A match between a pattern and some code."""

    pattern: CodePattern
    bindings: dict[str, str] = field(default_factory=dict)
    score: float = 0.0


# =============================================================================
# Pattern Extraction
# =============================================================================


class PatternExtractor:
    """Extract reusable patterns from code solutions.

    Identifies:
    - Common function structures
    - Repeated expressions
    - Idioms (list comprehensions, conditionals, etc.)
    """

    def __init__(self) -> None:
        self._pattern_counts: Counter[str] = Counter()

    def extract_patterns(self, code: str) -> list[CodePattern]:
        """Extract patterns from code.

        Args:
            code: Python source code

        Returns:
            List of detected patterns
        """
        patterns: list[CodePattern] = []

        try:
            tree = ast.parse(code)
        except SyntaxError:
            return patterns

        # Extract function patterns
        patterns.extend(self._extract_function_patterns(tree))

        # Extract expression patterns
        patterns.extend(self._extract_expression_patterns(tree))

        # Extract idiom patterns
        patterns.extend(self._extract_idiom_patterns(tree))

        return patterns

    def _extract_function_patterns(self, tree: ast.AST) -> list[CodePattern]:
        """Extract function-level patterns."""
        patterns: list[CodePattern] = []

        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                pattern = self._function_to_pattern(node)
                if pattern:
                    patterns.append(pattern)

        return patterns

    def _function_to_pattern(
        self,
        node: ast.FunctionDef | ast.AsyncFunctionDef,
    ) -> CodePattern | None:
        """Convert a function to a pattern template."""
        # Get function body as template
        if not node.body:
            return None

        # Simple case: single return statement
        if len(node.body) == 1 and isinstance(node.body[0], ast.Return):
            return_stmt = node.body[0]
            if return_stmt.value:
                expr_pattern = self._expr_to_template(return_stmt.value)
                if expr_pattern:
                    # Build type signature from args
                    args = [arg.arg for arg in node.args.args]
                    sig = f"({', '.join(args)}) -> $RESULT"

                    template = f"def $NAME({', '.join(args)}):\n    return {expr_pattern}"
                    return CodePattern(
                        template=template,
                        examples=[],
                        frequency=1,
                        category="function",
                        signature=sig,
                    )

        return None

    def _extract_expression_patterns(self, tree: ast.AST) -> list[CodePattern]:
        """Extract expression patterns."""
        patterns: list[CodePattern] = []

        for node in ast.walk(tree):
            if isinstance(node, ast.BinOp):
                template = self._binop_to_template(node)
                if template:
                    patterns.append(
                        CodePattern(
                            template=template,
                            examples=[],
                            frequency=1,
                            category="expression",
                        )
                    )

            elif isinstance(node, ast.ListComp):
                template = self._listcomp_to_template(node)
                if template:
                    patterns.append(
                        CodePattern(
                            template=template,
                            examples=[],
                            frequency=1,
                            category="comprehension",
                        )
                    )

        return patterns

    def _extract_idiom_patterns(self, tree: ast.AST) -> list[CodePattern]:
        """Extract common Python idioms."""
        patterns: list[CodePattern] = []

        for node in ast.walk(tree):
            # Guard clause pattern: if X: return Y
            if isinstance(node, ast.If):
                if len(node.body) == 1 and isinstance(node.body[0], ast.Return):
                    patterns.append(
                        CodePattern(
                            template="if $COND:\n    return $VALUE",
                            examples=[],
                            frequency=1,
                            category="idiom",
                        )
                    )

            # None check pattern
            if isinstance(node, ast.Compare):
                if any(isinstance(op, ast.Is) for op in node.ops):
                    is_none_check = any(
                        isinstance(c, ast.Constant) and c.value is None for c in node.comparators
                    )
                    if is_none_check:
                        patterns.append(
                            CodePattern(
                                template="$VAR is None",
                                examples=[],
                                frequency=1,
                                category="idiom",
                            )
                        )

        return patterns

    def _expr_to_template(self, node: ast.expr) -> str | None:
        """Convert expression to template string."""
        if isinstance(node, ast.Name):
            return f"${node.id.upper()}"
        elif isinstance(node, ast.BinOp):
            left = self._expr_to_template(node.left)
            right = self._expr_to_template(node.right)
            op = self._op_to_str(node.op)
            if left and right and op:
                return f"({left} {op} {right})"
        elif isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name):
                args = ", ".join(f"$ARG{i}" for i in range(len(node.args)))
                return f"{node.func.id}({args})"
        elif isinstance(node, ast.Constant):
            return repr(node.value)

        return None

    def _binop_to_template(self, node: ast.BinOp) -> str | None:
        """Convert binary operation to template."""
        op = self._op_to_str(node.op)
        if op:
            return f"($LEFT {op} $RIGHT)"
        return None

    def _listcomp_to_template(self, node: ast.ListComp) -> str | None:
        """Convert list comprehension to template."""
        if len(node.generators) == 1:
            gen = node.generators[0]
            if isinstance(gen.target, ast.Name):
                return f"[$EXPR for {gen.target.id} in $ITER]"
        return None

    def _op_to_str(self, op: ast.operator) -> str | None:
        """Convert operator to string."""
        op_map = {
            ast.Add: "+",
            ast.Sub: "-",
            ast.Mult: "*",
            ast.Div: "/",
            ast.Mod: "%",
            ast.Pow: "**",
            ast.FloorDiv: "//",
        }
        return op_map.get(type(op))


# =============================================================================
# Learned Library
# =============================================================================


@dataclass
class SolutionRecord:
    """Record of a solution and its specification."""

    code: str
    spec_description: str
    spec_type: str | None = None
    patterns: list[str] = field(default_factory=list)
    timestamp: float = 0.0


class LearnedLibrary:
    """Library that learns abstractions from synthesis history.

    Uses frequency-based pattern detection to identify common code
    structures that should be abstracted and reused.

    Features:
    - Tracks solutions and their patterns
    - Learns abstractions when patterns exceed threshold
    - Supports optional persistence to file
    - Compatible with LibraryPlugin protocol

    Args:
        min_frequency: Minimum pattern occurrences before creating abstraction
        max_abstractions: Maximum abstractions to store
        persistence_path: Optional path for persisting library state
    """

    def __init__(
        self,
        min_frequency: int = 3,
        max_abstractions: int = 100,
        persistence_path: Path | None = None,
    ) -> None:
        self._min_frequency = min_frequency
        self._max_abstractions = max_abstractions
        self._persistence_path = persistence_path

        self._abstractions: dict[str, Abstraction] = {}
        self._pattern_counts: Counter[str] = Counter()
        self._pattern_examples: dict[str, list[str]] = {}
        self._solution_history: list[SolutionRecord] = []
        self._extractor = PatternExtractor()

        self._metadata = LibraryMetadata(
            name="learned",
            priority=10,  # Higher priority than MemoryLibrary
            description="Frequency-based abstraction learning",
            supports_learning=True,
            persistence_type="file" if persistence_path else "memory",
        )

        # Load from persistence if available
        if persistence_path and persistence_path.exists():
            self._load()

    @property
    def metadata(self) -> LibraryMetadata:
        """Return library metadata."""
        return self._metadata

    def get_abstractions(self) -> list[Abstraction]:
        """Get all learned abstractions."""
        return list(self._abstractions.values())

    def add_abstraction(self, abstraction: Abstraction) -> None:
        """Manually add an abstraction.

        Args:
            abstraction: The abstraction to add
        """
        self._abstractions[abstraction.name] = abstraction
        self._persist()

    def remove_abstraction(self, name: str) -> bool:
        """Remove an abstraction by name.

        Args:
            name: Abstraction name

        Returns:
            True if removed, False if not found
        """
        removed = self._abstractions.pop(name, None) is not None
        if removed:
            self._persist()
        return removed

    def record_solution(
        self,
        code: str,
        spec: Specification,
    ) -> list[CodePattern]:
        """Record a solution and extract its patterns.

        Call this for every successful synthesis result to build
        the pattern database for learning.

        Args:
            code: The generated code
            spec: The specification it satisfies

        Returns:
            List of patterns extracted from the code
        """
        import time

        patterns = self._extractor.extract_patterns(code)

        # Record patterns
        pattern_hashes = []
        for pattern in patterns:
            hash_key = pattern.hash
            self._pattern_counts[hash_key] += 1
            pattern_hashes.append(hash_key)

            if hash_key not in self._pattern_examples:
                self._pattern_examples[hash_key] = []
            if len(self._pattern_examples[hash_key]) < 5:  # Keep up to 5 examples
                self._pattern_examples[hash_key].append(code)

        # Store solution record
        record = SolutionRecord(
            code=code,
            spec_description=spec.description,
            spec_type=spec.type_signature,
            patterns=pattern_hashes,
            timestamp=time.time(),
        )
        self._solution_history.append(record)

        # Limit history size
        if len(self._solution_history) > 1000:
            self._solution_history = self._solution_history[-500:]

        self._persist()
        return patterns

    def get_pattern_frequencies(self) -> dict[str, int]:
        """Get current pattern frequency counts.

        Returns:
            Dict mapping pattern hash to frequency count
        """
        return dict(self._pattern_counts)

    def get_frequent_patterns(self, min_frequency: int | None = None) -> list[tuple[str, int]]:
        """Get patterns that meet the frequency threshold.

        Args:
            min_frequency: Override default minimum frequency

        Returns:
            List of (pattern_hash, count) pairs above threshold
        """
        threshold = min_frequency or self._min_frequency
        return [
            (pattern, count)
            for pattern, count in self._pattern_counts.most_common()
            if count >= threshold
        ]

    def _extract_keywords(self, text: str) -> set[str]:
        """Extract keywords from text for matching."""
        words = re.findall(r"\w+", text.lower())
        stopwords = {"a", "an", "the", "is", "are", "to", "for", "of", "in", "on", "and", "or"}
        return {w for w in words if len(w) > 2 and w not in stopwords}

    def search_abstractions(
        self,
        spec: Specification,
        context: Context,
    ) -> list[tuple[Abstraction, float]]:
        """Search for relevant abstractions.

        Scoring based on:
        - Keyword overlap with description
        - Type signature compatibility
        - Usage frequency (learned patterns)
        - Compression gain

        Args:
            spec: The specification to match
            context: Available resources

        Returns:
            List of (abstraction, score) pairs, sorted by relevance
        """
        if not self._abstractions:
            return []

        spec_keywords = self._extract_keywords(spec.description)
        if spec.type_signature:
            spec_keywords.update(self._extract_keywords(spec.type_signature))

        results: list[tuple[Abstraction, float]] = []

        for abstraction in self._abstractions.values():
            score = 0.0

            # Keyword overlap
            abs_keywords = self._extract_keywords(abstraction.description)
            abs_keywords.update(self._extract_keywords(abstraction.name))

            if spec_keywords and abs_keywords:
                overlap = len(spec_keywords & abs_keywords)
                score += overlap / max(len(spec_keywords), len(abs_keywords))

            # Type signature match
            if spec.type_signature and abstraction.type_signature:
                if spec.type_signature == abstraction.type_signature:
                    score += 0.5
                elif self._types_compatible(spec.type_signature, abstraction.type_signature):
                    score += 0.25

            # Boost for frequently used
            if abstraction.usage_count > 0:
                score += min(0.1 * abstraction.usage_count, 0.3)

            # Boost for high compression gain
            if abstraction.compression_gain > 0:
                score += min(abstraction.compression_gain * 0.1, 0.2)

            if score > 0:
                results.append((abstraction, score))

        results.sort(key=lambda x: x[1], reverse=True)
        return results

    def _types_compatible(self, type1: str, type2: str) -> bool:
        """Check if two type signatures are compatible."""

        def get_return_type(sig: str) -> str | None:
            match = re.search(r"->\s*(\S+)", sig)
            return match.group(1) if match else None

        ret1 = get_return_type(type1)
        ret2 = get_return_type(type2)

        if ret1 and ret2:
            return ret1.lower() == ret2.lower()
        return False

    async def learn_abstraction(
        self,
        solutions: list[str],
        spec: Specification,
    ) -> Abstraction | None:
        """Learn a new abstraction from solutions.

        Analyzes the provided solutions plus historical patterns
        to identify abstractions worth creating.

        Args:
            solutions: List of successful solutions
            spec: The specification they solve

        Returns:
            New abstraction if one was learned, None otherwise
        """
        # Record new solutions
        for solution in solutions:
            self.record_solution(solution, spec)

        # Find frequent patterns that aren't yet abstracted
        frequent = self.get_frequent_patterns()
        if not frequent:
            return None

        for pattern_hash, _count in frequent:
            # Check if we already have this pattern as an abstraction
            if pattern_hash in self._abstractions:
                continue

            # Get examples for this pattern
            examples = self._pattern_examples.get(pattern_hash, [])
            if not examples:
                continue

            # Try to create abstraction from pattern
            abstraction = self._create_abstraction_from_pattern(pattern_hash, examples, spec)
            if abstraction:
                # Enforce max abstractions limit
                if len(self._abstractions) >= self._max_abstractions:
                    # Remove least used abstraction
                    self._prune_least_used()

                self._abstractions[abstraction.name] = abstraction
                self._persist()
                return abstraction

        return None

    def _create_abstraction_from_pattern(
        self,
        pattern_hash: str,
        examples: list[str],
        spec: Specification,
    ) -> Abstraction | None:
        """Create an abstraction from a pattern and its examples.

        Args:
            pattern_hash: Pattern identifier
            examples: Example code using this pattern
            spec: Current specification

        Returns:
            New abstraction or None
        """
        if not examples:
            return None

        # Use the first example as the basis
        code = examples[0]

        # Try to extract a function from the code
        try:
            tree = ast.parse(code)
        except SyntaxError:
            return None

        # Look for function definitions
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                # Extract function as abstraction
                name = f"abs_{pattern_hash}"
                description = f"Learned from pattern: {spec.description[:50]}"

                # Get type signature if available
                sig = None
                if node.returns:
                    try:
                        sig = ast.unparse(node.returns)
                    except Exception:
                        pass

                # Extract just this function
                try:
                    func_code = ast.unparse(node)
                except Exception:
                    func_code = code

                return Abstraction(
                    name=name,
                    code=func_code,
                    type_signature=sig,
                    description=description,
                    usage_count=self._pattern_counts.get(pattern_hash, 0),
                    compression_gain=self._estimate_compression(func_code, examples),
                )

        return None

    def _estimate_compression(self, abstraction_code: str, examples: list[str]) -> float:
        """Estimate compression gain from using an abstraction.

        Simplified version of DreamCoder's MDL-based scoring.
        """
        abstraction_len = len(abstraction_code)

        # Estimate savings from reusing abstraction
        total_example_len = sum(len(e) for e in examples)
        call_len = 20  # Approximate length of calling the abstraction

        if len(examples) > 1:
            # Savings = total code replaced - (abstraction + calls)
            savings = total_example_len - (abstraction_len + call_len * len(examples))
            # Normalize to 0-1 range
            return max(0.0, min(1.0, savings / total_example_len))

        return 0.0

    def _prune_least_used(self) -> None:
        """Remove the least-used abstraction to make room."""
        if not self._abstractions:
            return

        least_used = min(
            self._abstractions.values(),
            key=lambda a: (a.usage_count, a.compression_gain),
        )
        del self._abstractions[least_used.name]

    def record_usage(self, abstraction: Abstraction) -> None:
        """Record that an abstraction was used."""
        if abstraction.name in self._abstractions:
            old = self._abstractions[abstraction.name]
            self._abstractions[abstraction.name] = Abstraction(
                name=old.name,
                code=old.code,
                type_signature=old.type_signature,
                description=old.description,
                usage_count=old.usage_count + 1,
                compression_gain=old.compression_gain,
            )
            self._persist()

    def clear(self) -> None:
        """Clear all learned data."""
        self._abstractions.clear()
        self._pattern_counts.clear()
        self._pattern_examples.clear()
        self._solution_history.clear()
        self._persist()

    def __len__(self) -> int:
        """Return number of abstractions."""
        return len(self._abstractions)

    # =========================================================================
    # Persistence
    # =========================================================================

    def _persist(self) -> None:
        """Persist library state to file if configured."""
        if not self._persistence_path:
            return

        data = self._to_dict()
        try:
            self._persistence_path.parent.mkdir(parents=True, exist_ok=True)
            with open(self._persistence_path, "w") as f:
                json.dump(data, f, indent=2)
        except Exception:
            pass  # Silently ignore persistence errors

    def _load(self) -> None:
        """Load library state from file."""
        if not self._persistence_path or not self._persistence_path.exists():
            return

        try:
            with open(self._persistence_path) as f:
                data = json.load(f)
            self._from_dict(data)
        except Exception:
            pass  # Silently ignore load errors

    def _to_dict(self) -> dict[str, Any]:
        """Convert library state to dictionary."""
        return {
            "abstractions": {
                name: {
                    "name": a.name,
                    "code": a.code,
                    "type_signature": a.type_signature,
                    "description": a.description,
                    "usage_count": a.usage_count,
                    "compression_gain": a.compression_gain,
                }
                for name, a in self._abstractions.items()
            },
            "pattern_counts": dict(self._pattern_counts),
            "pattern_examples": {
                k: v[:5]
                for k, v in self._pattern_examples.items()  # Limit examples
            },
        }

    def _from_dict(self, data: dict[str, Any]) -> None:
        """Restore library state from dictionary."""
        # Restore abstractions
        for name, a_data in data.get("abstractions", {}).items():
            self._abstractions[name] = Abstraction(
                name=a_data["name"],
                code=a_data["code"],
                type_signature=a_data.get("type_signature"),
                description=a_data.get("description", ""),
                usage_count=a_data.get("usage_count", 0),
                compression_gain=a_data.get("compression_gain", 0.0),
            )

        # Restore pattern counts
        self._pattern_counts = Counter(data.get("pattern_counts", {}))

        # Restore pattern examples
        self._pattern_examples = data.get("pattern_examples", {})


# Protocol compliance check
assert isinstance(LearnedLibrary(), LibraryPlugin)


__all__ = [
    "CodePattern",
    "LearnedLibrary",
    "PatternExtractor",
    "PatternMatch",
    "SolutionRecord",
]
