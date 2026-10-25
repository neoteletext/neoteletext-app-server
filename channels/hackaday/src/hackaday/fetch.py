"""Fetching from hackaday.com: politely, only from there, and remembered on disk.

- Only `https://hackaday.com/` is fetched, after the redirects, so nothing
  a post says can point the channel elsewhere.
- One request at a time, at least `PAUSE_S` apart, with a User-Agent that
  says what is asking.
- Each answer is kept on disk with its `ETag`; asking again sends
  `If-None-Match`, and a 304 costs nothing (measured on the feed, 25
  October 2026). A restart starts from the disk.
- Bodies are bounded.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from pathlib import Path

#: The only origin fetched.
ORIGIN = "https://hackaday.com/"
#: Who is asking.
USER_AGENT = "neoteletext-hackaday/0.1 (a private teletext demonstration)"
#: The least time between two requests, in seconds.
PAUSE_S = 1.0
#: How long a request may take, in seconds.
TIMEOUT_S = 30
#: The largest body read: the REST API's hundred posts with their content
#: are under 2 MB; a lead picture under 1 MB.
LARGEST_BODY = 8 * 1024 * 1024
#: HTTP's Not Modified status (RFC 9110 section 15.4.5).
NOT_MODIFIED = 304


class FetchError(Exception):
    """A fetch that failed or was refused."""


@dataclass(frozen=True)
class Answer:
    """A body and what came with it."""

    body: bytes
    headers: dict[str, str]
    fresh: bool


class Fetcher:
    """One polite client, its answers kept in `cache`."""

    def __init__(self, cache: Path) -> None:
        """Keep answers in `cache`."""
        self.cache = cache
        self.cache.mkdir(parents=True, exist_ok=True)
        self._last = 0.0
        self._lock = asyncio.Lock()

    def _paths(self, url: str) -> tuple[Path, Path]:
        name = hashlib.sha256(url.encode()).hexdigest()[:32]
        return self.cache / f"{name}.body", self.cache / f"{name}.json"

    def cached(self, url: str) -> Answer | None:
        """Answer what is kept for `url`, without asking."""
        body, meta = self._paths(url)
        if not body.exists() or not meta.exists():
            return None
        return Answer(body.read_bytes(), json.loads(meta.read_text()), fresh=False)

    async def get(self, url: str) -> Answer:
        """Answer `url`'s body, asking again only when it may have changed."""
        if not url.startswith(ORIGIN):
            message = f"not {ORIGIN}: {url}"
            raise FetchError(message)
        async with self._lock:
            wait = self._last + PAUSE_S - time.monotonic()
            if wait > 0:
                await asyncio.sleep(wait)
            try:
                answer = await asyncio.to_thread(self._get, url)
            finally:
                self._last = time.monotonic()
            return answer

    def _get(self, url: str) -> Answer:
        body_path, meta_path = self._paths(url)
        kept = self.cached(url)
        request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})  # noqa: S310 - the origin is checked
        if kept is not None and "etag" in kept.headers:
            request.add_header("If-None-Match", kept.headers["etag"])
        try:
            with urllib.request.urlopen(request, timeout=TIMEOUT_S) as response:  # noqa: S310 - the origin is checked
                if not response.geturl().startswith(ORIGIN):
                    message = f"redirected away from {ORIGIN}: {response.geturl()}"
                    raise FetchError(message)
                body = response.read(LARGEST_BODY + 1)
                if len(body) > LARGEST_BODY:
                    message = f"over {LARGEST_BODY} bytes: {url}"
                    raise FetchError(message)
                headers = {name.lower(): value for name, value in response.headers.items()}
        except urllib.error.HTTPError as error:
            if error.code == NOT_MODIFIED and kept is not None:
                return Answer(kept.body, kept.headers, fresh=False)
            message = f"{error.code} from {url}"
            raise FetchError(message) from error
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            message = f"{url}: {error}"
            raise FetchError(message) from error
        body_path.write_bytes(body)
        meta_path.write_text(json.dumps(headers))
        return Answer(body, headers, fresh=True)

    async def json(self, url: str) -> Any:  # noqa: ANN401 - JSON is any shape; callers check it
        """Answer `url`'s JSON."""
        return json.loads((await self.get(url)).body)


def api(path: str, **query: str | int) -> str:
    """Answer the address of a WordPress REST API request."""
    encoded = urllib.parse.urlencode(query)
    return f"{ORIGIN}wp-json/wp/v2/{path}" + (f"?{encoded}" if encoded else "")
