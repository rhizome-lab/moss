"""File watcher for real-time incremental updates.

This module provides:
- FileWatcher: Watch directories for file changes
- WatchEvent: Event types (created, modified, deleted)
- EventBus: Async event notification system

Usage:
    async def on_change(event: WatchEvent):
        print(f"File {event.path} was {event.type.name}")

    watcher = FileWatcher(Path("src"))
    watcher.on_change(on_change)
    await watcher.start()
"""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Callable, Coroutine
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


# =============================================================================
# Event Types
# =============================================================================


class EventType(Enum):
    """Types of file system events."""

    CREATED = auto()
    MODIFIED = auto()
    DELETED = auto()
    MOVED = auto()


@dataclass
class WatchEvent:
    """A file system change event."""

    path: Path
    event_type: EventType
    timestamp: float
    old_path: Path | None = None  # For MOVED events


# Type alias for event handlers
EventHandler = Callable[[WatchEvent], Coroutine[Any, Any, None]]


# =============================================================================
# Event Bus
# =============================================================================


class EventBus:
    """Async event bus for broadcasting events to subscribers."""

    def __init__(self) -> None:
        """Initialize empty event bus."""
        self._handlers: dict[str, list[EventHandler]] = {}
        self._queue: asyncio.Queue[tuple[str, WatchEvent]] = asyncio.Queue()
        self._running = False
        self._task: asyncio.Task[None] | None = None

    def subscribe(self, event_name: str, handler: EventHandler) -> None:
        """Subscribe to an event type.

        Args:
            event_name: Event name to subscribe to (e.g., "file.modified")
            handler: Async function to call when event occurs
        """
        if event_name not in self._handlers:
            self._handlers[event_name] = []
        self._handlers[event_name].append(handler)

    def unsubscribe(self, event_name: str, handler: EventHandler) -> bool:
        """Unsubscribe from an event type.

        Args:
            event_name: Event name to unsubscribe from
            handler: Handler to remove

        Returns:
            True if handler was found and removed
        """
        if event_name not in self._handlers:
            return False
        try:
            self._handlers[event_name].remove(handler)
            return True
        except ValueError:
            return False

    async def emit(self, event_name: str, event: WatchEvent) -> None:
        """Emit an event to all subscribers.

        Args:
            event_name: Event name
            event: Event data
        """
        await self._queue.put((event_name, event))

    async def _process_events(self) -> None:
        """Process events from the queue."""
        while self._running:
            try:
                event_name, event = await asyncio.wait_for(
                    self._queue.get(),
                    timeout=0.5,
                )

                handlers = self._handlers.get(event_name, [])
                handlers.extend(self._handlers.get("*", []))  # Wildcard handlers

                for handler in handlers:
                    try:
                        await handler(event)
                    except Exception as e:
                        logger.exception("Error in event handler: %s", e)

            except TimeoutError:
                continue

    async def start(self) -> None:
        """Start processing events."""
        self._running = True
        self._task = asyncio.create_task(self._process_events())

    async def stop(self) -> None:
        """Stop processing events."""
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass


# =============================================================================
# File Watcher
# =============================================================================


@dataclass
class WatchConfig:
    """Configuration for file watcher."""

    patterns: list[str] = field(default_factory=lambda: ["*.py", "*.js", "*.ts", "*.md"])
    ignore_patterns: list[str] = field(
        default_factory=lambda: [".*", "__pycache__", "node_modules", ".git"]
    )
    debounce_ms: int = 100  # Debounce rapid changes
    recursive: bool = True


class FileWatcher:
    """Watch directories for file changes.

    Uses watchfiles if available, otherwise falls back to polling.
    """

    def __init__(
        self,
        path: Path | str,
        config: WatchConfig | None = None,
    ) -> None:
        """Initialize file watcher.

        Args:
            path: Directory to watch
            config: Watch configuration
        """
        self.path = Path(path).resolve()
        self.config = config or WatchConfig()
        self._event_bus = EventBus()
        self._running = False
        self._task: asyncio.Task[None] | None = None
        self._file_hashes: dict[str, str] = {}

    def on_change(self, handler: EventHandler) -> None:
        """Register a handler for any file change.

        Args:
            handler: Async function to call on change
        """
        self._event_bus.subscribe("*", handler)

    def on_created(self, handler: EventHandler) -> None:
        """Register a handler for file creation."""
        self._event_bus.subscribe("file.created", handler)

    def on_modified(self, handler: EventHandler) -> None:
        """Register a handler for file modification."""
        self._event_bus.subscribe("file.modified", handler)

    def on_deleted(self, handler: EventHandler) -> None:
        """Register a handler for file deletion."""
        self._event_bus.subscribe("file.deleted", handler)

    async def start(self) -> None:
        """Start watching for changes."""
        if self._running:
            return

        self._running = True
        await self._event_bus.start()

        # Try to use watchfiles, fall back to polling
        try:
            import watchfiles  # noqa: F401

            self._task = asyncio.create_task(self._watch_with_watchfiles())
        except ImportError:
            logger.info("watchfiles not available, using polling")
            self._task = asyncio.create_task(self._watch_with_polling())

    async def stop(self) -> None:
        """Stop watching for changes."""
        self._running = False
        await self._event_bus.stop()
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass

    async def _watch_with_watchfiles(self) -> None:
        """Watch using watchfiles library."""
        import watchfiles

        async for changes in watchfiles.awatch(
            self.path,
            recursive=self.config.recursive,
            stop_event=asyncio.Event(),  # Will be set when we stop
        ):
            if not self._running:
                break

            for change_type, path_str in changes:
                path = Path(path_str)

                if not self._should_include(path):
                    continue

                event_type = self._map_watchfiles_change(change_type)
                event = WatchEvent(
                    path=path,
                    event_type=event_type,
                    timestamp=asyncio.get_event_loop().time(),
                )

                event_name = f"file.{event_type.name.lower()}"
                await self._event_bus.emit(event_name, event)

    def _map_watchfiles_change(self, change: Any) -> EventType:
        """Map watchfiles change type to our EventType."""
        import watchfiles

        if change == watchfiles.Change.added:
            return EventType.CREATED
        elif change == watchfiles.Change.modified:
            return EventType.MODIFIED
        elif change == watchfiles.Change.deleted:
            return EventType.DELETED
        return EventType.MODIFIED

    async def _watch_with_polling(self) -> None:
        """Watch using polling (fallback when watchfiles unavailable)."""

        # Initial scan
        self._file_hashes = await self._scan_files()

        while self._running:
            await asyncio.sleep(self.config.debounce_ms / 1000)

            new_hashes = await self._scan_files()

            # Check for changes
            old_paths = set(self._file_hashes.keys())
            new_paths = set(new_hashes.keys())

            # Deleted files
            for path_str in old_paths - new_paths:
                path = Path(path_str)
                event = WatchEvent(
                    path=path,
                    event_type=EventType.DELETED,
                    timestamp=asyncio.get_event_loop().time(),
                )
                await self._event_bus.emit("file.deleted", event)

            # Created files
            for path_str in new_paths - old_paths:
                path = Path(path_str)
                event = WatchEvent(
                    path=path,
                    event_type=EventType.CREATED,
                    timestamp=asyncio.get_event_loop().time(),
                )
                await self._event_bus.emit("file.created", event)

            # Modified files
            for path_str in old_paths & new_paths:
                if self._file_hashes[path_str] != new_hashes[path_str]:
                    path = Path(path_str)
                    event = WatchEvent(
                        path=path,
                        event_type=EventType.MODIFIED,
                        timestamp=asyncio.get_event_loop().time(),
                    )
                    await self._event_bus.emit("file.modified", event)

            self._file_hashes = new_hashes

    async def _scan_files(self) -> dict[str, str]:
        """Scan directory and compute file hashes."""

        hashes = {}

        def scan_dir(directory: Path) -> None:
            try:
                for entry in directory.iterdir():
                    if entry.is_dir():
                        if self.config.recursive and self._should_include_dir(entry):
                            scan_dir(entry)
                    elif entry.is_file() and self._should_include(entry):
                        try:
                            stat = entry.stat()
                            # Use mtime + size as quick hash
                            hash_key = f"{stat.st_mtime}:{stat.st_size}"
                            hashes[str(entry)] = hash_key
                        except OSError:
                            pass
            except OSError:
                pass

        scan_dir(self.path)
        return hashes

    def _should_include(self, path: Path) -> bool:
        """Check if a file should be watched based on patterns."""
        import fnmatch

        name = path.name

        # Check ignore patterns
        for pattern in self.config.ignore_patterns:
            if fnmatch.fnmatch(name, pattern):
                return False

        # Check include patterns
        for pattern in self.config.patterns:
            if fnmatch.fnmatch(name, pattern):
                return True

        return False

    def _should_include_dir(self, path: Path) -> bool:
        """Check if a directory should be descended into."""
        import fnmatch

        name = path.name

        for pattern in self.config.ignore_patterns:
            if fnmatch.fnmatch(name, pattern):
                return False

        return True


# =============================================================================
# Integration with CodeIndexer
# =============================================================================


class IncrementalIndexer:
    """Incrementally update code index on file changes."""

    def __init__(
        self,
        watcher: FileWatcher,
        indexer: Any,  # CodeIndexer
    ) -> None:
        """Initialize incremental indexer.

        Args:
            watcher: File watcher to use
            indexer: CodeIndexer to update
        """
        self.watcher = watcher
        self.indexer = indexer
        self._pending: dict[str, WatchEvent] = {}
        self._debounce_task: asyncio.Task[None] | None = None

    async def start(self) -> None:
        """Start watching and indexing."""
        self.watcher.on_change(self._on_change)
        await self.watcher.start()

    async def stop(self) -> None:
        """Stop watching and indexing."""
        await self.watcher.stop()
        if self._debounce_task:
            self._debounce_task.cancel()

    async def _on_change(self, event: WatchEvent) -> None:
        """Handle file change event."""
        # Debounce: collect changes and process in batch
        self._pending[str(event.path)] = event

        # Cancel existing debounce task
        if self._debounce_task:
            self._debounce_task.cancel()

        # Schedule processing after debounce period
        self._debounce_task = asyncio.create_task(self._process_pending())

    async def _process_pending(self) -> None:
        """Process pending file changes."""
        await asyncio.sleep(0.2)  # 200ms debounce

        events = list(self._pending.values())
        self._pending.clear()

        for event in events:
            try:
                if event.event_type == EventType.DELETED:
                    await self.indexer.remove_file(event.path)
                    logger.debug("Removed from index: %s", event.path)
                else:
                    await self.indexer.index_file(event.path)
                    logger.debug("Indexed: %s", event.path)
            except Exception as e:
                logger.warning("Failed to process %s: %s", event.path, e)
