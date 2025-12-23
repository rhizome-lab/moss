"""Decomposition strategies for code synthesis.

This module provides domain-specific strategies for decomposing
code synthesis problems.

Available strategies:
- TypeDrivenDecomposition: Decompose based on type signatures
- TestDrivenDecomposition: Decompose based on test analysis
- PatternBasedDecomposition: Decompose based on known patterns
"""

from .pattern_based import PatternBasedDecomposition
from .test_driven import TestDrivenDecomposition
from .type_driven import TypeDrivenDecomposition

__all__ = [
    "PatternBasedDecomposition",
    "TestDrivenDecomposition",
    "TypeDrivenDecomposition",
]
