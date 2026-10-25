"""Fetching from hackaday.com: politely, only from there, compressed, and every request logged.

- Only `https://hackaday.com/` is fetched; a redirect is followed only
  within it, so nothing a post says can point the channel elsewhere.
- One request at a time, at least `PAUSE_S` apart, over one connection
  kept open, with a User-Agent that says what is asking.
- Answers are asked for gzipped, which the API gives only when asked
  (measured 25 October 2026: 100 posts 1,324,858 bytes plain, 220,306
  gzipped), and both the wire and the unpacked body are bounded.
- A 429 or 503 is waited out for its `Retry-After`; each failure waits
  twice as long as the last, up to `LONGEST_BACKOFF_S`.
- Each request is told to `on_fetch`: the store keeps them with their headers.

The API's answers carry no `ETag` or `Last-Modified` (`Cache-Control:
max-age=60` only), so what has changed is found from the data itself
(`sync.py`).
"""

from __future__ import annotations

import asyncio
import http.client
import json
import time
import urllib.parse
import zlib
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable

#: The only origin fetched.
ORIGIN = "https://hackaday.com/"
HOST = "hackaday.com"
#: Who is asking.
USER_AGENT = "neoteletext-hackaday/0.2 (a private teletext demonstration)"
#: The least time between two requests, in seconds.
PAUSE_S = 1.0
#: How long a request may take, in seconds.
TIMEOUT_S = 60
#: The largest body, unpacked: the REST API's hundred posts with their
#: content are under 2 MB; a lead picture under 1 MB.
LARGEST_BODY = 8 * 1024 * 1024
#: The most redirects followed, all within the origin.
MOST_REDIRECTS = 3
#: The first wait after a failure, and the longest after failures or a `Retry-After`, in seconds.
FIRST_BACKOFF_S = 2
LONGEST_BACKOFF_S = 900
#: HTTP statuses (RFC 9110 section 15).
OK = 200
NOT_MODIFIED = 304
REDIRECTS = frozenset({301, 302, 303, 307, 308})
TOO_MANY = 429
UNAVAILABLE = 503
#: zlib's window size for a gzip stream: 16 added to the window bits says
#: to expect gzip's header (zlib's manual, `inflateInit2`).
GZIP_WINDOW = 16 + zlib.MAX_WBITS


class FetchError(Exception):
    """A fetch that failed or was refused; `retry_after` when the server said how long to wait, in seconds."""

    def __init__(self, message: str, retry_after: float | None = None) -> None:
        """Say what failed, and how long the server asked to be left."""
        super().__init__(message)
        self.retry_after = retry_after


@dataclass(frozen=True)
class Answer:
    """An answer: its status, its unpacked body and its headers, names in lower case."""

    status: int
    body: bytes
    headers: dict[str, str]


@dataclass(frozen=True)
class Fetched:
    """What one request cost, as `on_fetch` is told."""

    kind: str
    url: str
    status: int
    wire_bytes: int
    body_bytes: int
    elapsed_ms: int
    headers: dict[str, str]


def unpacked(body: bytes, encoding: str) -> bytes:
    """Answer a body unpacked, no larger than `LARGEST_BODY`."""
    if encoding in {"", "identity"}:
        return body
    if encoding != "gzip":
        message = f"an encoding not asked for: {encoding}"
        raise FetchError(message)
    inflater = zlib.decompressobj(GZIP_WINDOW)
    try:
        out = inflater.decompress(body, LARGEST_BODY + 1)
    except zlib.error as error:
        message = f"a gzipped body that is not: {error}"
        raise FetchError(message) from error
    if len(out) > LARGEST_BODY or inflater.unconsumed_tail:
        message = f"a body over {LARGEST_BODY} bytes unpacked"
        raise FetchError(message)
    return out


class Fetcher:
    """One polite client."""

    def __init__(self, on_fetch: Callable[[Fetched], None] | None = None, pause_s: float = PAUSE_S) -> None:
        """Tell `on_fetch` of each request made; leave `pause_s` between this client's requests."""
        self.on_fetch = on_fetch
        self.pause_s = pause_s
        self._connection: http.client.HTTPSConnection | None = None
        self._last = 0.0
        self._not_before = 0.0
        self._failures = 0
        self._lock = asyncio.Lock()
        # What the worker thread fetched, told to `on_fetch` back on the
        # event loop's thread, which owns the store.
        self._fetched: list[Fetched] = []

    async def get(self, url: str, kind: str, headers: dict[str, str] | None = None) -> Answer:
        """Answer `url`, waiting its turn; a 304 is an answer, any other status not 200 an error."""
        if not url.startswith(ORIGIN):
            message = f"not {ORIGIN}: {url}"
            raise FetchError(message)
        async with self._lock:
            wait = max(self._last + self.pause_s, self._not_before) - time.monotonic()
            if wait > 0:
                await asyncio.sleep(wait)
            try:
                answer = await asyncio.to_thread(self._get, url, kind, headers or {})
            except FetchError:
                self._failures += 1
                backoff = min(LONGEST_BACKOFF_S, FIRST_BACKOFF_S**self._failures)
                self._not_before = max(self._not_before, time.monotonic() + backoff)
                raise
            finally:
                self._last = time.monotonic()
                self._tell()
            self._failures = 0
            return answer

    async def json(self, url: str, kind: str) -> tuple[Any, dict[str, str]]:
        """Answer `url`'s JSON and the answer's headers."""
        answer = await self.get(url, kind)
        try:
            return json.loads(answer.body), answer.headers
        except ValueError as error:
            message = f"not JSON: {url}"
            raise FetchError(message) from error

    def _tell(self) -> None:
        fetched, self._fetched = self._fetched, []
        if self.on_fetch is not None:
            for each in fetched:
                self.on_fetch(each)

    def close(self) -> None:
        """Close the connection."""
        if self._connection is not None:
            self._connection.close()
            self._connection = None

    def _get(self, url: str, kind: str, extra: dict[str, str]) -> Answer:
        for _ in range(MOST_REDIRECTS + 1):
            answer, wire, elapsed = self._once(url, extra)
            self._fetched.append(Fetched(kind, url, answer.status, wire, len(answer.body), elapsed, answer.headers))
            if answer.status in REDIRECTS:
                target = urllib.parse.urljoin(url, answer.headers.get("location", ""))
                if not target.startswith(ORIGIN):
                    message = f"redirected away from {ORIGIN}: {target}"
                    raise FetchError(message)
                url = target
                continue
            if answer.status in {TOO_MANY, UNAVAILABLE}:
                retry = answer.headers.get("retry-after", "")
                seconds = min(LONGEST_BACKOFF_S, int(retry) if retry.isdigit() else FIRST_BACKOFF_S)
                self._not_before = time.monotonic() + seconds
                message = f"{answer.status} from {url}"
                raise FetchError(message, retry_after=seconds)
            if answer.status not in {OK, NOT_MODIFIED}:
                message = f"{answer.status} from {url}"
                raise FetchError(message)
            return answer
        message = f"more than {MOST_REDIRECTS} redirects: {url}"
        raise FetchError(message)

    def _once(self, url: str, extra: dict[str, str]) -> tuple[Answer, int, int]:
        """Ask once over the kept connection, opening it again once if the server had closed it."""
        parts = urllib.parse.urlsplit(url)
        target = parts.path + (f"?{parts.query}" if parts.query else "")
        request = {"User-Agent": USER_AGENT, "Accept-Encoding": "gzip", **extra}
        for attempt in range(2):
            if self._connection is None:
                self._connection = http.client.HTTPSConnection(HOST, timeout=TIMEOUT_S)
            started = time.monotonic()
            try:
                self._connection.request("GET", target, headers=request)
                response = self._connection.getresponse()
                wire = response.read(LARGEST_BODY + 1)
            except (http.client.HTTPException, OSError) as error:
                self.close()
                stale = isinstance(error, (http.client.RemoteDisconnected, BrokenPipeError, ConnectionResetError))
                if attempt == 0 and stale:
                    continue
                message = f"{url}: {error}"
                raise FetchError(message) from error
            elapsed = int((time.monotonic() - started) * 1000)
            if len(wire) > LARGEST_BODY:
                self.close()
                message = f"over {LARGEST_BODY} bytes: {url}"
                raise FetchError(message)
            headers = {name.lower(): value for name, value in response.getheaders()}
            if headers.get("connection", "").lower() == "close":
                self.close()
            return Answer(response.status, unpacked(wire, headers.get("content-encoding", "").lower()), headers), len(wire), elapsed
        message = f"{url}: the connection closed twice"
        raise FetchError(message)


def api(path: str, **query: str | int) -> str:
    """Answer the address of a WordPress REST API request."""
    encoded = urllib.parse.urlencode(query)
    return f"{ORIGIN}wp-json/wp/v2/{path}" + (f"?{encoded}" if encoded else "")
