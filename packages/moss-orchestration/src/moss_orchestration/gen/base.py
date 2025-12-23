"""Base classes for interface generators."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from moss import MossAPI


class LazyAPIExecutor:
    """Base class for executors with lazy MossAPI initialization.

    Provides a cached `api` property that creates the MossAPI instance
    on first access.
    """

    _root: Path
    _api: MossAPI | None

    def __init__(self, root: str | Path = "."):
        """Initialize the executor.

        Args:
            root: Project root directory
        """
        self._root = Path(root).resolve()
        self._api = None

    @property
    def api(self) -> MossAPI:
        """Lazy-initialize and cache the MossAPI instance."""
        if self._api is None:
            from moss import MossAPI

            self._api = MossAPI.for_project(self._root)
        return self._api
