"""Token-efficient web fetching and search.

Design goal: Minimize tokens while preserving useful information.
Web content is verbose - raw HTML wastes context on noise.

Strategies:
1. Strip HTML to plain text or minimal markdown
2. Extract main content (article, main, .content selectors)
3. Remove nav, footer, ads, scripts, styles
4. Cache results to avoid re-fetching
5. Optional cheap summarization before LLM

Example:
    from moss_orchestration.web import WebFetcher

    fetcher = WebFetcher()
    content = await fetcher.fetch("https://example.com/docs")
    print(content.text)  # Clean, token-efficient content
"""

from __future__ import annotations

import hashlib
import re
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, ClassVar


@dataclass
class WebContent:
    """Fetched web content optimized for tokens."""

    url: str
    title: str = ""
    text: str = ""
    summary: str = ""  # Optional summarized version
    metadata: dict[str, Any] = field(default_factory=dict)
    fetched_at: float = field(default_factory=time.time)

    @property
    def token_estimate(self) -> int:
        """Rough token count (chars / 4)."""
        return len(self.text) // 4

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "url": self.url,
            "title": self.title,
            "text": self.text,
            "summary": self.summary,
            "metadata": self.metadata,
            "fetched_at": self.fetched_at,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> WebContent:
        """Create from dictionary."""
        return cls(
            url=data["url"],
            title=data.get("title", ""),
            text=data.get("text", ""),
            summary=data.get("summary", ""),
            metadata=data.get("metadata", {}),
            fetched_at=data.get("fetched_at", time.time()),
        )


@dataclass
class SearchResult:
    """A single search result."""

    title: str
    url: str
    snippet: str
    rank: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "title": self.title,
            "url": self.url,
            "snippet": self.snippet,
            "rank": self.rank,
        }


@dataclass
class SearchResults:
    """Collection of search results."""

    query: str
    results: list[SearchResult] = field(default_factory=list)
    total: int = 0

    @property
    def token_estimate(self) -> int:
        """Rough token count."""
        total = len(self.query)
        for r in self.results:
            total += len(r.title) + len(r.snippet) + len(r.url)
        return total // 4

    def to_compact(self) -> str:
        """Token-efficient string representation."""
        lines = [f"Query: {self.query}", ""]
        for i, r in enumerate(self.results, 1):
            lines.append(f"{i}. {r.title}")
            lines.append(f"   {r.url}")
            if r.snippet:
                lines.append(f"   {r.snippet[:150]}...")
            lines.append("")
        return "\n".join(lines)


class ContentExtractor:
    """Extract main content from HTML, stripping noise."""

    # Tags to remove entirely (including content)
    REMOVE_TAGS: ClassVar[set[str]] = {
        "script",
        "style",
        "nav",
        "footer",
        "header",
        "aside",
        "noscript",
        "iframe",
        "form",
        "button",
        "svg",
        "canvas",
    }

    # Selectors for main content (in priority order)
    MAIN_SELECTORS: ClassVar[list[str]] = [
        "article",
        "main",
        '[role="main"]',
        ".content",
        ".post-content",
        ".article-content",
        ".entry-content",
        "#content",
        "#main",
    ]

    def extract(self, html: str, url: str = "") -> WebContent:
        """Extract clean content from HTML.

        Uses BeautifulSoup if available, falls back to regex.
        """
        try:
            return self._extract_with_bs4(html, url)
        except ImportError:
            return self._extract_with_regex(html, url)

    def _extract_with_bs4(self, html: str, url: str) -> WebContent:
        """Extract using BeautifulSoup (preferred)."""
        from bs4 import BeautifulSoup

        soup = BeautifulSoup(html, "html.parser")

        # Get title
        title = ""
        if soup.title:
            title = soup.title.get_text(strip=True)

        # Remove unwanted tags
        for tag in self.REMOVE_TAGS:
            for el in soup.find_all(tag):
                el.decompose()

        # Find main content
        main_content = None
        for selector in self.MAIN_SELECTORS:
            main_content = soup.select_one(selector)
            if main_content:
                break

        # Fall back to body
        if not main_content:
            main_content = soup.body or soup

        # Extract text
        text = main_content.get_text(separator="\n", strip=True)

        # Clean up whitespace
        text = self._clean_whitespace(text)

        # Extract metadata
        metadata = self._extract_metadata(soup)

        return WebContent(
            url=url,
            title=title,
            text=text,
            metadata=metadata,
        )

    def _extract_with_regex(self, html: str, url: str) -> WebContent:
        """Fallback extraction using regex (no dependencies)."""
        # Extract title
        title_match = re.search(r"<title[^>]*>([^<]+)</title>", html, re.I)
        title = title_match.group(1).strip() if title_match else ""

        # Remove unwanted tags (including content)
        for tag in self.REMOVE_TAGS:
            html = re.sub(rf"<{tag}[^>]*>.*?</{tag}>", "", html, flags=re.I | re.S)

        # Remove tags
        text = re.sub(r"<[^>]+>", " ", html)

        # Decode entities
        text = text.replace("&nbsp;", " ")
        text = text.replace("&amp;", "&")
        text = text.replace("&lt;", "<")
        text = text.replace("&gt;", ">")
        text = text.replace("&quot;", '"')

        # Clean whitespace
        text = self._clean_whitespace(text)

        return WebContent(url=url, title=title, text=text)

    def _clean_whitespace(self, text: str) -> str:
        """Normalize whitespace."""
        # Collapse multiple newlines
        text = re.sub(r"\n{3,}", "\n\n", text)
        # Collapse multiple spaces
        text = re.sub(r"[ \t]+", " ", text)
        # Strip lines
        lines = [line.strip() for line in text.split("\n")]
        # Remove empty lines in sequence
        result = []
        prev_empty = False
        for line in lines:
            if not line:
                if not prev_empty:
                    result.append("")
                prev_empty = True
            else:
                result.append(line)
                prev_empty = False
        return "\n".join(result).strip()

    def _extract_metadata(self, soup: Any) -> dict[str, Any]:
        """Extract useful metadata from HTML head."""
        metadata = {}

        # OpenGraph
        for meta in soup.find_all("meta", property=re.compile(r"^og:")):
            prop = meta.get("property", "").replace("og:", "")
            if prop and meta.get("content"):
                metadata[f"og_{prop}"] = meta["content"]

        # Description
        desc_meta = soup.find("meta", attrs={"name": "description"})
        if desc_meta and desc_meta.get("content"):
            metadata["description"] = desc_meta["content"]

        return metadata


class ContentCache:
    """Simple file-based cache for web content."""

    def __init__(self, cache_dir: Path | None = None, ttl_seconds: int = 3600):
        self.cache_dir = cache_dir or Path.home() / ".cache" / "moss" / "web"
        self.ttl_seconds = ttl_seconds
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    def _key(self, url: str) -> str:
        """Generate cache key from URL."""
        return hashlib.sha256(url.encode()).hexdigest()[:16]

    def get(self, url: str) -> WebContent | None:
        """Get cached content if fresh."""
        import json

        key = self._key(url)
        path = self.cache_dir / f"{key}.json"

        if not path.exists():
            return None

        try:
            data = json.loads(path.read_text())
            content = WebContent.from_dict(data)

            # Check TTL
            if time.time() - content.fetched_at > self.ttl_seconds:
                path.unlink()
                return None

            return content
        except (json.JSONDecodeError, KeyError):
            return None

    def set(self, content: WebContent) -> None:
        """Cache content."""
        import json

        key = self._key(content.url)
        path = self.cache_dir / f"{key}.json"
        path.write_text(json.dumps(content.to_dict()))

    def clear(self) -> int:
        """Clear all cached content. Returns count of items cleared."""
        count = 0
        for path in self.cache_dir.glob("*.json"):
            path.unlink()
            count += 1
        return count


class WebFetcher:
    """Token-efficient web content fetcher.

    Example:
        fetcher = WebFetcher()
        content = await fetcher.fetch("https://docs.python.org/3/")
        print(f"Title: {content.title}")
        print(f"Tokens: ~{content.token_estimate}")
        print(content.text[:500])
    """

    def __init__(
        self,
        cache: ContentCache | None = None,
        extractor: ContentExtractor | None = None,
        timeout: float = 30.0,
        max_content_length: int = 500_000,  # 500KB
    ):
        self.cache = cache or ContentCache()
        self.extractor = extractor or ContentExtractor()
        self.timeout = timeout
        self.max_content_length = max_content_length

    async def fetch(
        self,
        url: str,
        *,
        use_cache: bool = True,
        extract_main: bool = True,
    ) -> WebContent:
        """Fetch and extract content from URL.

        Args:
            url: URL to fetch
            use_cache: Check cache first
            extract_main: Extract main content vs full page

        Returns:
            WebContent with clean, token-efficient text
        """
        # Check cache
        if use_cache:
            cached = self.cache.get(url)
            if cached:
                return cached

        # Fetch
        html = await self._fetch_html(url)

        # Extract
        if extract_main:
            content = self.extractor.extract(html, url)
        else:
            content = WebContent(url=url, text=html)

        # Cache
        if use_cache:
            self.cache.set(content)

        return content

    async def _fetch_html(self, url: str) -> str:
        """Fetch raw HTML from URL."""
        try:
            import aiohttp

            async with aiohttp.ClientSession() as session:
                async with session.get(
                    url,
                    timeout=aiohttp.ClientTimeout(total=self.timeout),
                    headers={"User-Agent": "Mozilla/5.0 (moss web fetcher)"},
                ) as response:
                    response.raise_for_status()
                    return await response.text()
        except ImportError:
            # Fallback to urllib (sync)
            import urllib.request

            req = urllib.request.Request(
                url, headers={"User-Agent": "Mozilla/5.0 (moss web fetcher)"}
            )
            with urllib.request.urlopen(req, timeout=self.timeout) as response:
                return response.read().decode("utf-8", errors="replace")

    def fetch_sync(
        self,
        url: str,
        *,
        use_cache: bool = True,
        extract_main: bool = True,
    ) -> WebContent:
        """Synchronous version of fetch."""
        import asyncio

        return asyncio.get_event_loop().run_until_complete(
            self.fetch(url, use_cache=use_cache, extract_main=extract_main)
        )


class WebSearcher:
    """Web search with token-efficient results.

    Uses DuckDuckGo by default (no API key required).
    """

    def __init__(self, max_results: int = 5):
        self.max_results = max_results

    async def search(self, query: str) -> SearchResults:
        """Search the web and return results.

        Args:
            query: Search query

        Returns:
            SearchResults with titles, URLs, and snippets
        """
        try:
            return await self._search_duckduckgo(query)
        except (OSError, TimeoutError, ConnectionError, ValueError):
            # Return empty results on error
            return SearchResults(query=query, results=[], total=0)

    async def _search_duckduckgo(self, query: str) -> SearchResults:
        """Search using DuckDuckGo HTML interface."""
        try:
            from duckduckgo_search import DDGS
        except ImportError as e:
            raise ImportError("duckduckgo-search required: pip install duckduckgo-search") from e

        results = []
        with DDGS() as ddgs:
            for i, r in enumerate(ddgs.text(query, max_results=self.max_results)):
                results.append(
                    SearchResult(
                        title=r.get("title", ""),
                        url=r.get("href", ""),
                        snippet=r.get("body", ""),
                        rank=i + 1,
                    )
                )

        return SearchResults(query=query, results=results, total=len(results))

    def search_sync(self, query: str) -> SearchResults:
        """Synchronous version of search."""
        import asyncio

        return asyncio.get_event_loop().run_until_complete(self.search(query))


# Convenience functions


async def fetch_url(url: str, use_cache: bool = True) -> WebContent:
    """Fetch URL with token-efficient extraction."""
    fetcher = WebFetcher()
    return await fetcher.fetch(url, use_cache=use_cache)


async def search_web(query: str, max_results: int = 5) -> SearchResults:
    """Search web with token-efficient results."""
    searcher = WebSearcher(max_results=max_results)
    return await searcher.search(query)


def extract_content(html: str, url: str = "") -> WebContent:
    """Extract clean content from HTML string."""
    extractor = ContentExtractor()
    return extractor.extract(html, url)
