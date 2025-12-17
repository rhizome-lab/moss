"""Built-in library plugins (DreamCoder-style abstraction management).

Libraries:
- MemoryLibrary: In-memory abstraction storage
- LearnedLibrary: Frequency-based abstraction learning
"""

from .learned import (
    CodePattern,
    LearnedLibrary,
    PatternExtractor,
    PatternMatch,
    SolutionRecord,
)
from .memory import MemoryLibrary

__all__ = [
    "CodePattern",
    "LearnedLibrary",
    "MemoryLibrary",
    "PatternExtractor",
    "PatternMatch",
    "SolutionRecord",
]
