"""Tests for the caching module."""

import ast
import time
from pathlib import Path

from moss.cache import CacheEntry, CacheStats, ParseCache, clear_cache, get_cache


class TestCacheEntry:
    """Tests for CacheEntry."""

    def test_create_entry(self):
        entry = CacheEntry(
            value="test",
            content_hash="abc123",
            mtime=time.time(),
        )

        assert entry.value == "test"
        assert entry.content_hash == "abc123"
        assert entry.hits == 0

    def test_touch_updates_stats(self):
        entry = CacheEntry(
            value="test",
            content_hash="abc123",
            mtime=time.time(),
        )

        original_time = entry.access_time
        entry.touch()

        assert entry.hits == 1
        assert entry.access_time >= original_time


class TestCacheStats:
    """Tests for CacheStats."""

    def test_hit_rate_zero(self):
        stats = CacheStats()
        assert stats.hit_rate == 0.0

    def test_hit_rate_calculation(self):
        stats = CacheStats(hits=7, misses=3)
        assert stats.hit_rate == 0.7

    def test_to_dict(self):
        stats = CacheStats(hits=10, misses=5, evictions=2, entries=15)
        result = stats.to_dict()

        assert result["hits"] == 10
        assert result["misses"] == 5
        assert result["evictions"] == 2
        assert result["entries"] == 15
        assert "hit_rate" in result


class TestParseCache:
    """Tests for ParseCache."""

    def test_create_cache(self):
        cache = ParseCache()

        assert cache.max_entries == 1000
        assert cache.stats.entries == 0

    def test_get_ast_miss(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        path.write_text("x = 1")

        result = cache.get_ast(path)

        assert result is None
        assert cache.stats.misses == 1

    def test_set_and_get_ast(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        content = "x = 1"
        path.write_text(content)

        tree = ast.parse(content)
        cache.set_ast(path, tree, content)

        result = cache.get_ast(path)

        assert result is not None
        assert cache.stats.hits == 1

    def test_get_or_parse_ast(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        path.write_text("def foo(): pass")

        # First call should parse
        tree1 = cache.get_or_parse_ast(path)
        assert tree1 is not None

        # Second call should use cache
        tree2 = cache.get_or_parse_ast(path)
        assert tree2 is not None
        assert cache.stats.hits == 1

    def test_cache_invalidation_on_mtime_change(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        path.write_text("x = 1")

        tree = ast.parse("x = 1")
        cache.set_ast(path, tree, "x = 1")

        # Simulate file modification
        time.sleep(0.01)
        path.write_text("x = 2")

        result = cache.get_ast(path)

        assert result is None  # Should be invalidated
        assert cache.stats.misses == 1

    def test_skeleton_cache(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        content = "def foo(): pass"
        path.write_text(content)

        cache.set_skeleton(path, [{"name": "foo"}], content)
        result = cache.get_skeleton(path)

        assert result is not None
        assert result == [{"name": "foo"}]
        assert cache.stats.hits == 1

    def test_get_or_extract_skeleton(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        path.write_text("def bar(): pass")

        result = cache.get_or_extract_skeleton(path)

        assert result is not None
        assert len(result) >= 1

        # Second call should use cache
        _ = cache.get_or_extract_skeleton(path)
        assert cache.stats.hits == 1

    def test_cfg_cache(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        content = "def foo(): pass"
        path.write_text(content)

        cfg_data = {"nodes": [], "edges": []}
        cache.set_cfg(path, cfg_data, content, "foo")

        result = cache.get_cfg(path, "foo")

        assert result is not None
        assert result == cfg_data

    def test_deps_cache(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        content = "import os"
        path.write_text(content)

        deps = {"imports": ["os"], "exports": []}
        cache.set_deps(path, deps, content)

        result = cache.get_deps(path)

        assert result is not None
        assert result == deps

    def test_clear_cache(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        content = "x = 1"
        path.write_text(content)

        cache.set_ast(path, ast.parse(content), content)
        cache.set_skeleton(path, [], content)

        cache.clear()

        assert cache.get_ast(path) is None
        assert cache.get_skeleton(path) is None

    def test_invalidate_path(self, tmp_path: Path):
        cache = ParseCache()
        path = tmp_path / "test.py"
        content = "x = 1"
        path.write_text(content)

        cache.set_ast(path, ast.parse(content), content)
        cache.set_skeleton(path, [], content)
        cache.set_cfg(path, {}, content, "func")

        cache.invalidate(path)

        assert cache.get_ast(path) is None
        assert cache.get_skeleton(path) is None
        assert cache.get_cfg(path, "func") is None

    def test_eviction(self, tmp_path: Path):
        cache = ParseCache(max_entries=5)

        for i in range(10):
            path = tmp_path / f"test{i}.py"
            content = f"x = {i}"
            path.write_text(content)
            cache.set_ast(path, ast.parse(content), content)

        # Should have evicted some entries
        assert cache.stats.evictions > 0


class TestGlobalCache:
    """Tests for global cache functions."""

    def test_get_cache(self):
        cache = get_cache()
        assert cache is not None

    def test_clear_cache(self, tmp_path: Path):
        cache = get_cache()
        path = tmp_path / "test.py"
        path.write_text("x = 1")
        cache.get_or_parse_ast(path)

        clear_cache()

        assert cache.get_ast(path) is None
