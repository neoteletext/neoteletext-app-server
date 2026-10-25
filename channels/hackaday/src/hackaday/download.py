"""Reading the whole of Hackaday's public archive into the store, by a time: `python -m hackaday.download [DATABASE] [MINUTES]`.

The channel runs it itself while it is not done; the command line is for a
copy made elsewhere. Every page it will read is planned first: each
collection's count (`X-WP-Total`), and for the dated ones (posts, media,
comments) their months from the first, so that no request asks the
server to skip far. Requests then start at an even pace that ends by the
time asked (`DOWNLOAD_WITHIN_S` from the start, the owner's 70 minutes),
the gap worked out again before each from the time and the pages left.
Comment pages take Hackaday about 4 s each (measured 25 October 2026), so
up to `WORKERS` are in flight at once, each over its own connection.

Each page is kept in the same transaction as what it read
(`download_pages`), so a download stopped goes on with only the pages it
had not read. A 429 or 503 holds every request for its `Retry-After`; a
page that fails is tried again, up to `MOST_TRIES`, then left for the
next run.
"""

from __future__ import annotations

import asyncio
import math
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import TYPE_CHECKING, Any

from hackaday.fetch import Fetcher, FetchError, api
from hackaday.store import PageRead, Store
from hackaday.sync import PAGE_SIZE, TIME_FORM, month_after, parse_time, total_pages, zoned

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence

    from hackaday.fetch import Fetched

#: The collections, the table each is kept in, and whether it is read by month.
COLLECTIONS: tuple[tuple[str, str, bool], ...] = (
    ("users", "users", False),
    ("categories", "terms", False),
    ("tags", "terms", False),
    ("posts", "posts", True),
    ("pages", "posts", False),
    ("media", "media", True),
    ("comments", "comments", True),
)
#: How long the whole download is paced to take, in seconds: the owner's 70 minutes.
DOWNLOAD_WITHIN_S = 70 * 60
#: The most requests in flight. A comment page took Hackaday 4 s alone and
#: about 10 s with 24 in flight (measured 26 October 2026), about 2.4 pages
#: a second; the owner's deadline needs more.
WORKERS = 48
#: How often a page that fails is tried before it is left for the next run.
MOST_TRIES = 5
#: How often the progress is kept for page 108, in pages read.
PROGRESS_EVERY = 25


def gap(now: float, deadline: float, remaining: int) -> float:
    """Answer the wait before the next request so the `remaining` end by `deadline`: none once it has passed."""
    return max(0.0, deadline - now) / max(1, remaining)


@dataclass(frozen=True)
class Page:
    """One page to read: its collection, its month ('' for none) and its number."""

    collection: str
    month: str
    page: int


class Pacer:
    """Starts requests at an even pace that ends by a deadline, and holds them all when the server asks."""

    def __init__(self, deadline: float, remaining: Callable[[], int]) -> None:
        """End by `deadline` (the monotonic clock), with `remaining` saying how many requests are left."""
        self.deadline = deadline
        self.remaining = remaining
        self.next = 0.0
        self.held_until = 0.0
        self.lock = asyncio.Lock()

    async def turn(self) -> None:
        """Wait for this request's start."""
        async with self.lock:
            now = time.monotonic()
            start = max(now, self.next, self.held_until)
            self.next = start + gap(start, self.deadline, self.remaining())
        await asyncio.sleep(max(0.0, start - now))

    def hold(self, seconds: float) -> None:
        """Start nothing for `seconds`."""
        self.held_until = max(self.held_until, time.monotonic() + seconds)


class Download:
    """The full download: planned, paced and kept page by page."""

    def __init__(self, store: Store, fetchers: Sequence[Fetcher], within_s: float = DOWNLOAD_WITHIN_S) -> None:
        """Read into `store` through `fetchers`, one a worker, paced to take `within_s`."""
        self.store = store
        self.fetchers = fetchers
        self.within_s = within_s
        self.queue: asyncio.Queue[tuple[Page, int]] = asyncio.Queue()
        self.read = self.started = self.failed = 0
        self.planned = 0
        self.months: dict[str, list[str]] = {}

    async def plan(self) -> list[Page]:
        """Count each collection, find each dated one's months, and answer the pages not yet read."""
        done = self.store.downloaded()
        pages: list[Page] = []
        planned = 0
        for collection, _, by_month in COLLECTIONS:
            order = "date" if by_month else "id"
            first, headers = await self.fetchers[0].json(
                api(collection, per_page=1, orderby=order, order="asc", _fields="date_gmt" if by_month else "id"), "plan"
            )
            total = int(headers.get("x-wp-total", "0") or 0)
            months = [""]
            if by_month and first:
                start = parse_time(first[0]["date_gmt"]).replace(day=1, hour=0, minute=0, second=0)
                months = []
                while start <= datetime.now(UTC):
                    months.append(zoned(start))
                    start = month_after(start)
            self.months[collection] = months
            # A month's last page is partly full: one more page a month.
            planned += math.ceil(total / PAGE_SIZE) + (len(months) if by_month else 0)
            for month in months:
                known = done.get((collection, month))
                if known is None:
                    pages.append(Page(collection, month, 1))
                else:
                    total_of, read = known
                    pages.extend(Page(collection, month, number) for number in range(1, total_of + 1) if number not in read)
        read_already = sum(len(read) for _, read in done.values())
        self.planned = max(planned - read_already, len(pages))
        return pages

    def remaining(self) -> int:
        """Answer how many requests are thought to be left."""
        return max(1, self.planned - self.started)

    async def run(self) -> bool:
        """Read every page not read, paced to end within its time; answer whether the whole archive is read."""
        for page in await self.plan():
            self.queue.put_nowait((page, 1))
        pacer = Pacer(time.monotonic() + self.within_s, self.remaining)
        self.progress()
        workers = [asyncio.ensure_future(self.worker(fetcher, pacer)) for fetcher in self.fetchers]
        try:
            await self.queue.join()
        finally:
            for worker in workers:
                worker.cancel()
        complete = self.complete()
        if complete:
            self.store.set_state("backfill", {"done": True, "finished_at": zoned(datetime.now(UTC))})
        self.progress()
        return complete

    async def worker(self, fetcher: Fetcher, pacer: Pacer) -> None:
        """Read pages from the queue until cancelled."""
        while True:
            page, tries = await self.queue.get()
            try:
                await pacer.turn()
                self.started += 1
                await self.read_page(fetcher, page)
            except FetchError as error:
                if error.retry_after:
                    pacer.hold(error.retry_after)
                if tries < MOST_TRIES:
                    self.queue.put_nowait((page, tries + 1))
                else:
                    self.failed += 1
                    print(f"hackaday: left for the next run: {page}: {error}", file=sys.stderr)  # noqa: T201
            finally:
                self.queue.task_done()

    async def read_page(self, fetcher: Fetcher, page: Page) -> None:
        """Read one page and keep it, with its record; a month's first page adds the month's others."""
        collection, table, by_month = next(each for each in COLLECTIONS if each[0] == page.collection)
        query: dict[str, str | int] = {"per_page": PAGE_SIZE, "page": page.page, "orderby": "id", "order": "asc"}
        if by_month:
            start = datetime.strptime(page.month, TIME_FORM + "Z").replace(tzinfo=UTC)
            # `after` leaves out the instant given: a second before the month takes in its first second.
            query |= {"after": zoned(start - timedelta(seconds=1)), "before": zoned(month_after(start))}
        found, headers = await fetcher.json(api(collection, **query), "download")
        items: list[dict[str, Any]] = [item for item in found if isinstance(item, dict)]
        pages = total_pages(headers)
        record = PageRead(collection, page.month, page.page, pages, len(items))
        if table == "posts":
            self.store.put_posts(items, page=record)
        else:
            self.store.put_objects(table, items, page=record)
        if page.page == 1:
            read = self.store.downloaded().get((collection, page.month), (0, set()))[1]
            for number in range(2, pages + 1):
                if number not in read:
                    self.queue.put_nowait((Page(collection, page.month, number), 1))
        self.read += 1
        if self.read % PROGRESS_EVERY == 0:
            self.progress()

    def complete(self) -> bool:
        """Answer whether every month of every collection has every page read."""
        done = self.store.downloaded()
        for collection, months in self.months.items():
            for month in months:
                known = done.get((collection, month))
                if known is None or len(known[1]) < max(1, known[0]):
                    return False
        return True

    def progress(self) -> None:
        """Keep how far the download is, for page 108 and the command line."""
        place = {"read": self.read, "planned": self.read + max(0, self.planned - self.started), "failed": self.failed}
        self.store.set_state("download", place)
        print(f"hackaday: download {place} {self.store.counts()}", file=sys.stderr, flush=True)  # noqa: T201


def fetchers(on_fetch: Callable[[Fetched], None] | None, count: int = WORKERS) -> list[Fetcher]:
    """Answer a client for each worker, with no pause of its own (the pacer spaces them) and a thread each.

    The default pool behind `asyncio.to_thread` holds `min(32, CPUs + 4)`
    threads (24 on the host), which held the download to 24 requests in
    flight however many workers it had (measured 26 October 2026).
    """
    threads = ThreadPoolExecutor(max_workers=count, thread_name_prefix="download")
    return [Fetcher(on_fetch, pause_s=0.0, threads=threads) for _ in range(count)]


def main() -> None:
    """Read until the full download is done, paced to take the minutes given."""
    from hackaday.library import DATABASE  # noqa: PLC0415 - the channel's file, only when none is named

    database = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[2] / "cache" / DATABASE
    within_s = float(sys.argv[2]) * 60 if len(sys.argv) > 2 else DOWNLOAD_WITHIN_S  # noqa: PLR2004 - the second argument
    database.parent.mkdir(parents=True, exist_ok=True)
    store = Store(database)
    asyncio.run(Download(store, fetchers(store.log_fetch), within_s).run())


if __name__ == "__main__":
    main()
