"""Tests for file watcher module."""

import asyncio
from pathlib import Path

import pytest

from moss.watcher import (
    EventBus,
    EventType,
    FileWatcher,
    IncrementalIndexer,
    WatchConfig,
    WatchEvent,
)


class TestWatchEvent:
    """Tests for WatchEvent dataclass."""

    def test_create_event(self):
        event = WatchEvent(
            path=Path("test.py"),
            event_type=EventType.MODIFIED,
            timestamp=1234567890.0,
        )

        assert event.path == Path("test.py")
        assert event.event_type == EventType.MODIFIED
        assert event.timestamp == 1234567890.0
        assert event.old_path is None

    def test_moved_event(self):
        event = WatchEvent(
            path=Path("new.py"),
            event_type=EventType.MOVED,
            timestamp=1234567890.0,
            old_path=Path("old.py"),
        )

        assert event.old_path == Path("old.py")


class TestEventBus:
    """Tests for EventBus."""

    @pytest.fixture
    def bus(self) -> EventBus:
        return EventBus()

    async def test_subscribe_and_emit(self, bus: EventBus):
        received = []

        async def handler(event: WatchEvent):
            received.append(event)

        bus.subscribe("test.event", handler)
        await bus.start()

        event = WatchEvent(
            path=Path("test.py"),
            event_type=EventType.MODIFIED,
            timestamp=0,
        )
        await bus.emit("test.event", event)

        # Wait for processing
        await asyncio.sleep(0.1)
        await bus.stop()

        assert len(received) == 1
        assert received[0].path == Path("test.py")

    async def test_wildcard_handler(self, bus: EventBus):
        received = []

        async def handler(event: WatchEvent):
            received.append(event)

        bus.subscribe("*", handler)
        await bus.start()

        event = WatchEvent(
            path=Path("test.py"),
            event_type=EventType.CREATED,
            timestamp=0,
        )
        await bus.emit("any.event", event)

        await asyncio.sleep(0.1)
        await bus.stop()

        assert len(received) == 1

    async def test_unsubscribe(self, bus: EventBus):
        received = []

        async def handler(event: WatchEvent):
            received.append(event)

        bus.subscribe("test.event", handler)
        assert bus.unsubscribe("test.event", handler)
        assert not bus.unsubscribe("test.event", handler)  # Already removed

    async def test_multiple_handlers(self, bus: EventBus):
        received1 = []
        received2 = []

        async def handler1(event: WatchEvent):
            received1.append(event)

        async def handler2(event: WatchEvent):
            received2.append(event)

        bus.subscribe("test.event", handler1)
        bus.subscribe("test.event", handler2)
        await bus.start()

        event = WatchEvent(
            path=Path("test.py"),
            event_type=EventType.MODIFIED,
            timestamp=0,
        )
        await bus.emit("test.event", event)

        await asyncio.sleep(0.1)
        await bus.stop()

        assert len(received1) == 1
        assert len(received2) == 1


class TestWatchConfig:
    """Tests for WatchConfig."""

    def test_default_config(self):
        config = WatchConfig()

        assert "*.py" in config.patterns
        assert "__pycache__" in config.ignore_patterns
        assert config.recursive is True
        assert config.debounce_ms == 100

    def test_custom_config(self):
        config = WatchConfig(
            patterns=["*.js"],
            ignore_patterns=["node_modules"],
            debounce_ms=200,
            recursive=False,
        )

        assert config.patterns == ["*.js"]
        assert config.ignore_patterns == ["node_modules"]
        assert config.debounce_ms == 200
        assert config.recursive is False


class TestFileWatcher:
    """Tests for FileWatcher."""

    @pytest.fixture
    def watch_dir(self, tmp_path: Path) -> Path:
        """Create a test directory with files."""
        src = tmp_path / "src"
        src.mkdir()
        (src / "main.py").write_text("print('hello')")
        (src / "utils.py").write_text("def helper(): pass")
        return tmp_path

    def test_create_watcher(self, watch_dir: Path):
        watcher = FileWatcher(watch_dir)

        assert watcher.path == watch_dir.resolve()
        assert watcher.config is not None

    def test_should_include(self, watch_dir: Path):
        watcher = FileWatcher(watch_dir)

        assert watcher._should_include(Path("test.py"))
        assert watcher._should_include(Path("app.js"))
        assert not watcher._should_include(Path(".hidden"))
        assert not watcher._should_include(Path("test.xyz"))

    def test_should_include_dir(self, watch_dir: Path):
        watcher = FileWatcher(watch_dir)

        assert watcher._should_include_dir(Path("src"))
        assert not watcher._should_include_dir(Path("__pycache__"))
        assert not watcher._should_include_dir(Path(".git"))
        assert not watcher._should_include_dir(Path("node_modules"))

    async def test_scan_files(self, watch_dir: Path):
        watcher = FileWatcher(watch_dir)

        hashes = await watcher._scan_files()

        # Should find the Python files
        py_files = [p for p in hashes.keys() if p.endswith(".py")]
        assert len(py_files) == 2

    async def test_start_stop(self, watch_dir: Path):
        watcher = FileWatcher(watch_dir)

        await watcher.start()
        assert watcher._running

        await watcher.stop()
        assert not watcher._running

    async def test_register_handlers(self, watch_dir: Path):
        watcher = FileWatcher(watch_dir)
        received = []

        async def handler(event: WatchEvent):
            received.append(event)

        watcher.on_change(handler)
        watcher.on_created(handler)
        watcher.on_modified(handler)
        watcher.on_deleted(handler)

        # Handlers should be registered
        assert len(watcher._event_bus._handlers) == 4


class TestIncrementalIndexer:
    """Tests for IncrementalIndexer."""

    @pytest.fixture
    def watch_dir(self, tmp_path: Path) -> Path:
        src = tmp_path / "src"
        src.mkdir()
        (src / "main.py").write_text("def main(): pass")
        return tmp_path

    @pytest.fixture
    def mock_indexer(self):
        """Create a mock indexer."""

        class MockIndexer:
            def __init__(self):
                self.indexed = []
                self.removed = []

            async def index_file(self, path: Path):
                self.indexed.append(path)

            async def remove_file(self, path: Path):
                self.removed.append(path)

        return MockIndexer()

    async def test_creates_incremental_indexer(self, watch_dir: Path, mock_indexer):
        watcher = FileWatcher(watch_dir)
        indexer = IncrementalIndexer(watcher, mock_indexer)

        assert indexer.watcher is watcher
        assert indexer.indexer is mock_indexer

    async def test_processes_change_events(self, watch_dir: Path, mock_indexer):
        watcher = FileWatcher(watch_dir)
        indexer = IncrementalIndexer(watcher, mock_indexer)

        # Simulate a change event
        event = WatchEvent(
            path=Path(watch_dir / "src" / "main.py"),
            event_type=EventType.MODIFIED,
            timestamp=0,
        )
        await indexer._on_change(event)

        # Wait for debounce
        await asyncio.sleep(0.3)

        assert len(mock_indexer.indexed) == 1

    async def test_processes_delete_events(self, watch_dir: Path, mock_indexer):
        watcher = FileWatcher(watch_dir)
        indexer = IncrementalIndexer(watcher, mock_indexer)

        event = WatchEvent(
            path=Path(watch_dir / "src" / "main.py"),
            event_type=EventType.DELETED,
            timestamp=0,
        )
        await indexer._on_change(event)

        await asyncio.sleep(0.3)

        assert len(mock_indexer.removed) == 1

    async def test_debounces_rapid_changes(self, watch_dir: Path, mock_indexer):
        watcher = FileWatcher(watch_dir)
        indexer = IncrementalIndexer(watcher, mock_indexer)

        # Rapid changes to same file
        for _ in range(5):
            event = WatchEvent(
                path=Path(watch_dir / "src" / "main.py"),
                event_type=EventType.MODIFIED,
                timestamp=0,
            )
            await indexer._on_change(event)

        await asyncio.sleep(0.3)

        # Should only index once due to debouncing
        assert len(mock_indexer.indexed) == 1
