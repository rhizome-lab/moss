"""Pattern-based decomposition strategy.

Decomposes problems by matching against known patterns and applying
their standard decomposition templates.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from moss.synthesis.strategy import DecompositionStrategy, StrategyMetadata
from moss.synthesis.types import Context, Specification, Subproblem


@dataclass
class Pattern:
    """A known decomposition pattern."""

    name: str
    description: str
    keywords: tuple[str, ...]
    template: tuple[str, ...]
    success_rate: float = 0.8

    def match_score(self, spec: Specification) -> float:
        """Compute how well this pattern matches the specification."""
        desc_lower = spec.description.lower()
        desc_words = set(desc_lower.split())
        score = 0.0

        # Keyword matching - use whole word matching to prevent subproblems
        # from re-matching parent patterns (e.g., "authentication response"
        # should not match "auth" keyword via substring)
        keyword_matches = sum(1 for kw in self.keywords if kw in desc_words)
        score += 0.6 * (keyword_matches / len(self.keywords))

        # Description similarity (simple word overlap)
        desc_words = set(desc_lower.split())
        pattern_words = set(self.description.lower().split())
        overlap = len(desc_words & pattern_words)
        score += 0.4 * (overlap / max(len(pattern_words), 1))

        return min(1.0, score)

    def instantiate(
        self,
        spec: Specification,
        context: Context,
    ) -> list[Subproblem]:
        """Create subproblems from template."""
        subproblems: list[Subproblem] = []

        for i, template in enumerate(self.template):
            # Replace placeholders in template
            description = self._substitute_placeholders(template, spec)

            sub_spec = Specification(
                description=description,
                type_signature=spec.type_signature,
                constraints=spec.constraints,
            )

            # Dependencies: each step depends on previous
            deps = (i - 1,) if i > 0 else ()

            subproblems.append(
                Subproblem(
                    specification=sub_spec,
                    dependencies=deps,
                    priority=i,
                )
            )

        return subproblems

    def _substitute_placeholders(self, template: str, spec: Specification) -> str:
        """Substitute placeholders like {resource} in template."""
        result = template

        # Extract resource name from spec description
        resource = self._extract_resource(spec.description)
        result = result.replace("{resource}", resource)

        return result

    def _extract_resource(self, description: str) -> str:
        """Extract the main resource/entity from a description."""
        # Common patterns: "user management", "for users", "User API"
        patterns = [
            r"(\w+)\s+management",
            r"for\s+(\w+)s?",
            r"(\w+)\s+API",
            r"(\w+)\s+CRUD",
            r"(\w+)\s+endpoint",
        ]

        for pattern in patterns:
            match = re.search(pattern, description, re.IGNORECASE)
            if match:
                return match.group(1).lower()

        # Fallback: extract nouns (simplified)
        words = description.split()
        for word in words:
            if word[0].isupper() and len(word) > 2:
                return word.lower()

        return "resource"


# Built-in pattern library
PATTERNS: list[Pattern] = [
    Pattern(
        name="crud_api",
        description="REST API with Create, Read, Update, Delete operations",
        keywords=("crud", "rest", "api", "endpoint", "resource", "http"),
        template=(
            "Implement Create operation (POST /{resource})",
            "Implement Read operation (GET /{resource}/:id)",
            "Implement List operation (GET /{resource})",
            "Implement Update operation (PUT /{resource}/:id)",
            "Implement Delete operation (DELETE /{resource}/:id)",
        ),
    ),
    Pattern(
        name="authentication",
        description="User authentication with credentials",
        keywords=("auth", "authentication", "login", "credentials", "password"),
        template=(
            "Implement user lookup by username",
            "Implement password validation",
            "Implement session/token generation",
            "Implement authentication response",
        ),
    ),
    Pattern(
        name="etl_pipeline",
        description="Extract, Transform, Load data pipeline",
        keywords=("etl", "pipeline", "extract", "transform", "load", "data"),
        template=(
            "Implement data extraction from source",
            "Implement data transformation rules",
            "Implement data validation",
            "Implement data loading to destination",
        ),
    ),
    Pattern(
        name="validation",
        description="Input validation with error messages",
        keywords=("validate", "validation", "check", "verify", "input", "error"),
        template=(
            "Implement type/format validation",
            "Implement business rule validation",
            "Implement error message generation",
            "Implement validation result aggregation",
        ),
    ),
    Pattern(
        name="search",
        description="Search functionality with filters",
        keywords=("search", "find", "query", "filter", "lookup", "index"),
        template=(
            "Implement query parsing",
            "Implement filter application",
            "Implement result ranking/sorting",
            "Implement pagination",
        ),
    ),
    Pattern(
        name="caching",
        description="Caching layer with expiration",
        keywords=("cache", "caching", "memoize", "store", "expire", "ttl"),
        template=(
            "Implement cache key generation",
            "Implement cache lookup",
            "Implement cache storage",
            "Implement cache invalidation/expiration",
        ),
    ),
]


class PatternBasedDecomposition(DecompositionStrategy):
    """Decompose based on recognized patterns and idioms.

    This strategy works best when:
    - Problem matches a known pattern (CRUD, ETL, auth, etc.)
    - Domain has established solutions
    - Pattern has high historical success rate

    Decomposition approach:
    1. Match specification against known patterns
    2. Select best matching pattern
    3. Instantiate pattern's decomposition template
    """

    def __init__(self, patterns: list[Pattern] | None = None):
        self._patterns = patterns if patterns is not None else PATTERNS
        self._match_threshold = 0.3

    @property
    def metadata(self) -> StrategyMetadata:
        return StrategyMetadata(
            name="pattern_based",
            description="Decompose based on recognized patterns and idioms",
            keywords=(
                "pattern",
                "template",
                "crud",
                "common",
                "standard",
                "idiom",
                "boilerplate",
            ),
        )

    def can_handle(self, spec: Specification, context: Context) -> bool:
        """Check if specification matches a known pattern."""
        return self._find_best_pattern(spec) is not None

    def decompose(
        self,
        spec: Specification,
        context: Context,
    ) -> list[Subproblem]:
        """Decompose based on matched pattern."""
        pattern = self._find_best_pattern(spec)
        if not pattern:
            return []

        return pattern.instantiate(spec, context)

    def estimate_success(self, spec: Specification, context: Context) -> float:
        """Estimate based on pattern match quality."""
        pattern = self._find_best_pattern(spec)
        if not pattern:
            return 0.0

        match_score = pattern.match_score(spec)
        return match_score * pattern.success_rate

    def _find_best_pattern(self, spec: Specification) -> Pattern | None:
        """Find the best matching pattern."""
        best_pattern = None
        best_score = 0.0

        for pattern in self._patterns:
            score = pattern.match_score(spec)
            if score > best_score and score >= self._match_threshold:
                best_score = score
                best_pattern = pattern

        return best_pattern

    def add_pattern(self, pattern: Pattern) -> None:
        """Add a new pattern to the library."""
        self._patterns.append(pattern)
