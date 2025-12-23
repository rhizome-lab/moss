"""Server state management.

This module provides persistent state for the Moss server,
enabling parse-once-query-many semantics and caching.
"""

from __future__ import annotations

import asyncio
import hashlib
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class CacheEntry:
    """A cached result with metadata.

    Attributes:
        key: Cache key
        value: Cached value
        created_at: Timestamp when cached
        expires_at: Timestamp when entry expires (None = no expiry)
        file_mtime: File modification time (for invalidation)
    """

    key: str
    value: Any
    created_at: float
    expires_at: float | None = None
    file_mtime: float | None = None

    def is_expired(self) -> bool:
        """Check if the entry has expired."""
        if self.expires_at is None:
            return False
        return time.time() > self.expires_at

    def is_stale(self, current_mtime: float | None) -> bool:
        """Check if the entry is stale based on file modification time."""
        if self.file_mtime is None or current_mtime is None:
            return False
        return current_mtime > self.file_mtime


@dataclass
class ServerState:
    """Persistent state for the Moss server.

    Manages:
    - Cached parse results (skeletons, CFGs, etc.)
    - Active MossAPI instances
    - File watchers for cache invalidation
    - Idle timeout tracking for daemon auto-shutdown
    """

    root: Path
    _cache: dict[str, CacheEntry] = field(default_factory=dict)
    _locks: dict[str, asyncio.Lock] = field(default_factory=dict)
    _api: Any = None  # MossAPI instance
    _default_ttl: float = 300.0  # 5 minutes
    _start_time: float = field(default_factory=time.time)
    _last_activity: float = field(default_factory=time.time)
    _query_count: int = 0

    def __post_init__(self):
        """Initialize the state."""
        self.root = Path(self.root).resolve()
        self._start_time = time.time()
        self._last_activity = time.time()

    def touch(self) -> None:
        """Update last activity time. Call on each request."""
        self._last_activity = time.time()
        self._query_count += 1

    def idle_seconds(self) -> float:
        """Get seconds since last activity."""
        return time.time() - self._last_activity

    def uptime_seconds(self) -> float:
        """Get server uptime in seconds."""
        return time.time() - self._start_time

    @property
    def api(self) -> Any:
        """Get or create the MossAPI instance.

        Note: MossAPI was removed during package restructuring.
        Use moss_intelligence modules directly instead.
        """
        raise NotImplementedError(
            "MossAPI removed - use moss_intelligence modules directly"
        )

    def _get_lock(self, key: str) -> asyncio.Lock:
        """Get or create a lock for a cache key."""
        if key not in self._locks:
            self._locks[key] = asyncio.Lock()
        return self._locks[key]

    def _make_key(self, operation: str, *args: Any, **kwargs: Any) -> str:
        """Create a cache key from operation and arguments."""
        parts = [operation]
        parts.extend(str(a) for a in args)
        parts.extend(f"{k}={v}" for k, v in sorted(kwargs.items()))
        key_str = ":".join(parts)
        return hashlib.sha256(key_str.encode()).hexdigest()[:16]

    def _get_file_mtime(self, file_path: Path | str | None) -> float | None:
        """Get file modification time."""
        if file_path is None:
            return None
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        try:
            return path.stat().st_mtime
        except OSError:
            return None

    async def get_cached(
        self,
        operation: str,
        *args: Any,
        file_path: Path | str | None = None,
        **kwargs: Any,
    ) -> Any | None:
        """Get a cached result if available and valid.

        Args:
            operation: Operation name (e.g., "skeleton.extract")
            *args: Operation arguments
            file_path: File path for mtime-based invalidation
            **kwargs: Operation keyword arguments

        Returns:
            Cached value if available and valid, None otherwise
        """
        key = self._make_key(operation, *args, **kwargs)
        entry = self._cache.get(key)

        if entry is None:
            return None

        # Check expiration
        if entry.is_expired():
            del self._cache[key]
            return None

        # Check file staleness
        current_mtime = self._get_file_mtime(file_path)
        if entry.is_stale(current_mtime):
            del self._cache[key]
            return None

        return entry.value

    async def set_cached(
        self,
        operation: str,
        value: Any,
        *args: Any,
        file_path: Path | str | None = None,
        ttl: float | None = None,
        **kwargs: Any,
    ) -> None:
        """Cache a result.

        Args:
            operation: Operation name
            value: Value to cache
            *args: Operation arguments
            file_path: File path for mtime-based invalidation
            ttl: Time to live in seconds (None = use default)
            **kwargs: Operation keyword arguments
        """
        key = self._make_key(operation, *args, **kwargs)
        ttl = ttl if ttl is not None else self._default_ttl

        self._cache[key] = CacheEntry(
            key=key,
            value=value,
            created_at=time.time(),
            expires_at=time.time() + ttl if ttl > 0 else None,
            file_mtime=self._get_file_mtime(file_path),
        )

    async def execute_cached(
        self,
        operation: str,
        func: Any,
        *args: Any,
        file_path: Path | str | None = None,
        ttl: float | None = None,
        **kwargs: Any,
    ) -> Any:
        """Execute an operation with caching.

        Args:
            operation: Operation name
            func: Function to execute if not cached
            *args: Function arguments
            file_path: File path for mtime-based invalidation
            ttl: Time to live in seconds
            **kwargs: Function keyword arguments

        Returns:
            Cached or fresh result
        """
        # Check cache first
        cached = await self.get_cached(operation, *args, file_path=file_path, **kwargs)
        if cached is not None:
            return cached

        # Use lock to prevent duplicate computation
        key = self._make_key(operation, *args, **kwargs)
        async with self._get_lock(key):
            # Double-check after acquiring lock
            cached = await self.get_cached(operation, *args, file_path=file_path, **kwargs)
            if cached is not None:
                return cached

            # Execute and cache
            if asyncio.iscoroutinefunction(func):
                result = await func(*args, **kwargs)
            else:
                result = func(*args, **kwargs)

            await self.set_cached(
                operation,
                result,
                *args,
                file_path=file_path,
                ttl=ttl,
                **kwargs,
            )
            return result

    def invalidate(self, pattern: str | None = None) -> int:
        """Invalidate cached entries.

        Args:
            pattern: Key pattern to invalidate (None = all)

        Returns:
            Number of entries invalidated
        """
        if pattern is None:
            count = len(self._cache)
            self._cache.clear()
            return count

        to_remove = [k for k in self._cache if pattern in k]
        for k in to_remove:
            del self._cache[k]
        return len(to_remove)

    def stats(self) -> dict[str, Any]:
        """Get cache statistics.

        Returns:
            Dict with cache stats
        """
        now = time.time()
        expired = sum(1 for e in self._cache.values() if e.is_expired())
        total_age = sum(now - e.created_at for e in self._cache.values())

        return {
            "entries": len(self._cache),
            "expired": expired,
            "avg_age_seconds": total_age / len(self._cache) if self._cache else 0,
            "root": str(self.root),
            "uptime_seconds": self.uptime_seconds(),
            "idle_seconds": self.idle_seconds(),
            "query_count": self._query_count,
        }


__all__ = [
    "CacheEntry",
    "ServerState",
]
