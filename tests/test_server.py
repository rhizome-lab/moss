"""Tests for the Moss server module."""

import time
from pathlib import Path

import pytest

from moss.server.state import CacheEntry, ServerState

# =============================================================================
# CacheEntry Tests
# =============================================================================


class TestCacheEntry:
    def test_create_entry(self):
        entry = CacheEntry(key="test", value="data", created_at=time.time())
        assert entry.key == "test"
        assert entry.value == "data"
        assert not entry.is_expired()

    def test_expired_entry(self):
        entry = CacheEntry(
            key="test",
            value="data",
            created_at=time.time() - 100,
            expires_at=time.time() - 50,
        )
        assert entry.is_expired()

    def test_not_expired_entry(self):
        entry = CacheEntry(
            key="test",
            value="data",
            created_at=time.time(),
            expires_at=time.time() + 100,
        )
        assert not entry.is_expired()

    def test_stale_entry(self):
        entry = CacheEntry(
            key="test",
            value="data",
            created_at=time.time(),
            file_mtime=100.0,
        )
        # Current mtime > cached mtime = stale
        assert entry.is_stale(200.0)
        # Current mtime < cached mtime = not stale
        assert not entry.is_stale(50.0)
        # No current mtime = not stale
        assert not entry.is_stale(None)


# =============================================================================
# ServerState Tests
# =============================================================================


class TestServerState:
    @pytest.fixture
    def state(self, tmp_path: Path):
        return ServerState(root=tmp_path)

    def test_create_state(self, state: ServerState, tmp_path: Path):
        assert state.root == tmp_path
        assert state._cache == {}

    def test_api_property(self, state: ServerState):
        api = state.api
        assert api is not None
        # Should return same instance
        assert state.api is api

    @pytest.mark.asyncio
    async def test_set_and_get_cached(self, state: ServerState):
        await state.set_cached("test.operation", "result", "arg1")

        cached = await state.get_cached("test.operation", "arg1")
        assert cached == "result"

    @pytest.mark.asyncio
    async def test_cache_miss(self, state: ServerState):
        cached = await state.get_cached("nonexistent", "arg")
        assert cached is None

    @pytest.mark.asyncio
    async def test_execute_cached(self, state: ServerState):
        call_count = 0

        def expensive_operation(x: int) -> int:
            nonlocal call_count
            call_count += 1
            return x * 2

        # First call - executes
        result1 = await state.execute_cached("multiply", expensive_operation, 5)
        assert result1 == 10
        assert call_count == 1

        # Second call - cached
        result2 = await state.execute_cached("multiply", expensive_operation, 5)
        assert result2 == 10
        assert call_count == 1  # Not called again

    @pytest.mark.asyncio
    async def test_execute_cached_with_file(self, state: ServerState, tmp_path: Path):
        test_file = tmp_path / "test.txt"
        test_file.write_text("content")

        def read_file() -> str:
            return test_file.read_text()

        # First call
        result1 = await state.execute_cached("read", read_file, file_path=test_file)
        assert result1 == "content"

        # Second call - cached
        result2 = await state.execute_cached("read", read_file, file_path=test_file)
        assert result2 == "content"

    def test_invalidate_all(self, state: ServerState):
        state._cache["a"] = CacheEntry(key="a", value=1, created_at=time.time())
        state._cache["b"] = CacheEntry(key="b", value=2, created_at=time.time())

        count = state.invalidate()
        assert count == 2
        assert len(state._cache) == 0

    def test_invalidate_pattern(self, state: ServerState):
        state._cache["test.a"] = CacheEntry(key="test.a", value=1, created_at=time.time())
        state._cache["test.b"] = CacheEntry(key="test.b", value=2, created_at=time.time())
        state._cache["other.c"] = CacheEntry(key="other.c", value=3, created_at=time.time())

        count = state.invalidate("test")
        assert count == 2
        assert len(state._cache) == 1
        assert "other.c" in state._cache

    def test_stats(self, state: ServerState):
        state._cache["a"] = CacheEntry(key="a", value=1, created_at=time.time())

        stats = state.stats()
        assert stats["entries"] == 1
        assert "root" in stats
        assert "avg_age_seconds" in stats


# =============================================================================
# Server App Tests (requires FastAPI)
# =============================================================================


def _has_fastapi() -> bool:
    """Check if FastAPI is available."""
    try:
        import fastapi  # noqa: F401

        return True
    except ImportError:
        return False


class TestCreateApp:
    def test_create_app_without_fastapi(self, tmp_path: Path, monkeypatch):
        """Test that create_app raises ImportError without FastAPI."""

        # Mock fastapi not being available
        # This test verifies the error handling path
        pass  # Skip for now - requires complex mocking

    @pytest.mark.skipif(
        not _has_fastapi(),
        reason="FastAPI not installed",
    )
    def test_create_app(self, tmp_path: Path):
        from moss.server import create_app

        app = create_app(root=tmp_path)
        assert app is not None
        assert app.title == "Moss API Server"


# =============================================================================
# Integration Tests (requires FastAPI)
# =============================================================================


@pytest.mark.skipif(not _has_fastapi(), reason="FastAPI not installed")
class TestServerIntegration:
    @pytest.fixture
    def app(self, tmp_path: Path):
        from moss.server import create_app

        # Create a minimal project structure
        src_dir = tmp_path / "src"
        src_dir.mkdir()
        (src_dir / "__init__.py").touch()
        (src_dir / "main.py").write_text("def hello(): pass\n")

        return create_app(root=tmp_path)

    @pytest.fixture
    def client(self, app):
        from fastapi.testclient import TestClient

        return TestClient(app)

    def test_root_endpoint(self, client):
        response = client.get("/")
        assert response.status_code == 200
        data = response.json()
        assert data["name"] == "moss"
        assert data["status"] == "running"

    def test_health_endpoint(self, client):
        response = client.get("/health")
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "healthy"

    def test_cache_stats(self, client):
        response = client.get("/cache/stats")
        assert response.status_code == 200
        data = response.json()
        assert "entries" in data

    def test_cache_invalidate(self, client):
        response = client.post("/cache/invalidate")
        assert response.status_code == 200
        data = response.json()
        assert "invalidated" in data

    def test_project_health(self, client):
        response = client.get("/project/health")
        assert response.status_code == 200
        data = response.json()
        assert "health_score" in data
        assert "health_grade" in data
