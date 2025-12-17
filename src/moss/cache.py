"""Caching layer for parsed AST and CFG data.

This module provides a caching mechanism to avoid re-parsing unchanged files,
improving performance for repeated analysis operations.

Usage:
    from moss.cache import get_cache, cached_parse

    cache = get_cache()

    # Cache AST parse results
    tree = cache.get_ast(path)
    if tree is None:
        tree = ast.parse(source)
        cache.set_ast(path, tree, source)

    # Or use the decorator
    @cached_parse
    def analyze_file(path: Path) -> AnalysisResult:
        ...
"""

from __future__ import annotations

import ast
import hashlib
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class CacheEntry[T]:
    """A cached entry with metadata."""

    value: T
    content_hash: str
    mtime: float
    access_time: float = field(default_factory=time.time)
    hits: int = 0

    def touch(self) -> None:
        """Update access time and hit count."""
        self.access_time = time.time()
        self.hits += 1


@dataclass
class CacheStats:
    """Statistics for cache performance."""

    hits: int = 0
    misses: int = 0
    evictions: int = 0
    entries: int = 0

    @property
    def hit_rate(self) -> float:
        """Calculate cache hit rate."""
        total = self.hits + self.misses
        return self.hits / total if total > 0 else 0.0

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "hits": self.hits,
            "misses": self.misses,
            "evictions": self.evictions,
            "entries": self.entries,
            "hit_rate": f"{self.hit_rate:.2%}",
        }


class ParseCache:
    """Cache for parsed AST and other analysis results.

    Uses content hashing to detect file changes and LRU eviction
    to limit memory usage.
    """

    def __init__(
        self,
        max_entries: int = 1000,
        max_age_seconds: float = 3600,
    ) -> None:
        self.max_entries = max_entries
        self.max_age = max_age_seconds
        self._ast_cache: dict[Path, CacheEntry[ast.AST]] = {}
        self._cfg_cache: dict[tuple[Path, str | None], CacheEntry[Any]] = {}
        self._skeleton_cache: dict[Path, CacheEntry[list]] = {}
        self._deps_cache: dict[Path, CacheEntry[dict]] = {}
        self._stats = CacheStats()

    @property
    def stats(self) -> CacheStats:
        """Get cache statistics."""
        self._stats.entries = (
            len(self._ast_cache)
            + len(self._cfg_cache)
            + len(self._skeleton_cache)
            + len(self._deps_cache)
        )
        return self._stats

    def _hash_content(self, content: str) -> str:
        """Generate a hash of file content."""
        return hashlib.sha256(content.encode()).hexdigest()[:16]

    def _is_valid(self, entry: CacheEntry, path: Path) -> bool:
        """Check if a cache entry is still valid."""
        # Check age
        if time.time() - entry.access_time > self.max_age:
            return False

        # Check if file has been modified
        try:
            current_mtime = path.stat().st_mtime
            if current_mtime != entry.mtime:
                return False
        except OSError:
            return False

        return True

    def _evict_if_needed(self, cache: dict) -> None:
        """Evict old entries if cache is full."""
        if len(cache) < self.max_entries:
            return

        # Sort by access time, remove oldest 10%
        entries = sorted(cache.items(), key=lambda x: x[1].access_time)
        to_remove = max(1, len(entries) // 10)

        for key, _ in entries[:to_remove]:
            del cache[key]
            self._stats.evictions += 1

    # -------------------------------------------------------------------------
    # AST Cache
    # -------------------------------------------------------------------------

    def get_ast(self, path: Path) -> ast.AST | None:
        """Get cached AST for a file."""
        path = path.resolve()
        entry = self._ast_cache.get(path)

        if entry is None:
            self._stats.misses += 1
            return None

        if not self._is_valid(entry, path):
            del self._ast_cache[path]
            self._stats.misses += 1
            return None

        entry.touch()
        self._stats.hits += 1
        return entry.value

    def set_ast(self, path: Path, tree: ast.AST, content: str) -> None:
        """Cache an AST for a file."""
        path = path.resolve()
        self._evict_if_needed(self._ast_cache)

        try:
            mtime = path.stat().st_mtime
        except OSError:
            mtime = time.time()

        self._ast_cache[path] = CacheEntry(
            value=tree,
            content_hash=self._hash_content(content),
            mtime=mtime,
        )

    def get_or_parse_ast(self, path: Path) -> ast.AST:
        """Get cached AST or parse the file."""
        cached = self.get_ast(path)
        if cached is not None:
            return cached

        content = path.read_text()
        tree = ast.parse(content, filename=str(path))
        self.set_ast(path, tree, content)
        return tree

    # -------------------------------------------------------------------------
    # CFG Cache
    # -------------------------------------------------------------------------

    def get_cfg(self, path: Path, function_name: str | None = None) -> Any | None:
        """Get cached CFG for a file/function."""
        path = path.resolve()
        key = (path, function_name)
        entry = self._cfg_cache.get(key)

        if entry is None:
            self._stats.misses += 1
            return None

        if not self._is_valid(entry, path):
            del self._cfg_cache[key]
            self._stats.misses += 1
            return None

        entry.touch()
        self._stats.hits += 1
        return entry.value

    def set_cfg(self, path: Path, cfg: Any, content: str, function_name: str | None = None) -> None:
        """Cache a CFG for a file/function."""
        path = path.resolve()
        key = (path, function_name)
        self._evict_if_needed(self._cfg_cache)

        try:
            mtime = path.stat().st_mtime
        except OSError:
            mtime = time.time()

        self._cfg_cache[key] = CacheEntry(
            value=cfg,
            content_hash=self._hash_content(content),
            mtime=mtime,
        )

    # -------------------------------------------------------------------------
    # Skeleton Cache
    # -------------------------------------------------------------------------

    def get_skeleton(self, path: Path) -> list | None:
        """Get cached skeleton for a file."""
        path = path.resolve()
        entry = self._skeleton_cache.get(path)

        if entry is None:
            self._stats.misses += 1
            return None

        if not self._is_valid(entry, path):
            del self._skeleton_cache[path]
            self._stats.misses += 1
            return None

        entry.touch()
        self._stats.hits += 1
        return entry.value

    def set_skeleton(self, path: Path, skeleton: list, content: str) -> None:
        """Cache a skeleton for a file."""
        path = path.resolve()
        self._evict_if_needed(self._skeleton_cache)

        try:
            mtime = path.stat().st_mtime
        except OSError:
            mtime = time.time()

        self._skeleton_cache[path] = CacheEntry(
            value=skeleton,
            content_hash=self._hash_content(content),
            mtime=mtime,
        )

    def get_or_extract_skeleton(self, path: Path) -> list:
        """Get cached skeleton or extract from file."""
        from moss.skeleton import extract_python_skeleton

        cached = self.get_skeleton(path)
        if cached is not None:
            return cached

        content = path.read_text()
        skeleton = extract_python_skeleton(content)
        self.set_skeleton(path, skeleton, content)
        return skeleton

    # -------------------------------------------------------------------------
    # Dependencies Cache
    # -------------------------------------------------------------------------

    def get_deps(self, path: Path) -> dict | None:
        """Get cached dependencies for a file."""
        path = path.resolve()
        entry = self._deps_cache.get(path)

        if entry is None:
            self._stats.misses += 1
            return None

        if not self._is_valid(entry, path):
            del self._deps_cache[path]
            self._stats.misses += 1
            return None

        entry.touch()
        self._stats.hits += 1
        return entry.value

    def set_deps(self, path: Path, deps: dict, content: str) -> None:
        """Cache dependencies for a file."""
        path = path.resolve()
        self._evict_if_needed(self._deps_cache)

        try:
            mtime = path.stat().st_mtime
        except OSError:
            mtime = time.time()

        self._deps_cache[path] = CacheEntry(
            value=deps,
            content_hash=self._hash_content(content),
            mtime=mtime,
        )

    # -------------------------------------------------------------------------
    # Cache Management
    # -------------------------------------------------------------------------

    def clear(self) -> None:
        """Clear all caches."""
        self._ast_cache.clear()
        self._cfg_cache.clear()
        self._skeleton_cache.clear()
        self._deps_cache.clear()
        self._stats = CacheStats()

    def invalidate(self, path: Path) -> None:
        """Invalidate all cache entries for a path."""
        path = path.resolve()

        if path in self._ast_cache:
            del self._ast_cache[path]

        # Remove all CFG entries for this path
        cfg_keys = [k for k in self._cfg_cache if k[0] == path]
        for key in cfg_keys:
            del self._cfg_cache[key]

        if path in self._skeleton_cache:
            del self._skeleton_cache[path]

        if path in self._deps_cache:
            del self._deps_cache[path]


# =============================================================================
# Global Cache Instance
# =============================================================================

_cache: ParseCache | None = None


def get_cache() -> ParseCache:
    """Get the global cache instance."""
    global _cache
    if _cache is None:
        _cache = ParseCache()
    return _cache


def set_cache(cache: ParseCache) -> None:
    """Set the global cache instance."""
    global _cache
    _cache = cache


def clear_cache() -> None:
    """Clear the global cache."""
    global _cache
    if _cache is not None:
        _cache.clear()


def cache_stats() -> CacheStats:
    """Get statistics from the global cache."""
    return get_cache().stats
