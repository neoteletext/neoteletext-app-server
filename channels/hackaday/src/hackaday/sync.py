"""Keeping the store a copy of Hackaday's public archive: all of it once, then what changes.

The REST API's answers carry no validators, so what changed is found from
the data. Each call to `Sync.step` makes the most urgent piece of work
that is due, a request or a few, so the channel's own requests wait for
no more than one:

1. New and edited posts, every `POSTS_EVERY_S`: the newest posts' ids and
   modification times (about 1 KB), and every post modified since the
   latest kept, anywhere in the archive (`modified_after`); only those
   that differ are fetched whole, with any writer, term or media record
   they name that is not kept. A post scheduled ahead is modified before
   it is published, so the newest posts are looked at by date as well.
2. New comments, every `COMMENTS_EVERY_S`, from the whole site at once
   (`after`, with an overlap, kept by id). A comment approved late keeps
   its first date, and comments have no modification time, so:
3. one post of the newest `WINDOW` has all its comments read again every
   `RECONCILE_EVERY_S`, round the window; one that has gone is marked.
4. Posts that have gone: the newest `WINDOW` by date every
   `WINDOW_EVERY_S`, and the whole archive's ids every `SWEEP_EVERY_S`, a
   page at a time.
5. Media records modified, every `MEDIA_EVERY_S`; the categories and the
   writers, every `TERMS_EVERY_S`.
6. Otherwise, the next page of the full download (`BACKFILL`), which
   keeps its place in the same transaction as what it read, so it goes on
   where it stopped. A collection with dates is read a month at a time,
   so no request asks the server to skip far (comments are 1.48 million).

Times are asked with `Z`: without a zone the API reads them as the site's
own (California) time, and `after` and `modified_after` leave out the
instant given (both measured 25 October 2026).
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING, Any

from hackaday.fetch import FetchError, api

if TYPE_CHECKING:
    from hackaday.fetch import Fetcher
    from hackaday.store import Store

#: The REST API's most items a request (WordPress `per_page`).
PAGE_SIZE = 100
#: The newest posts the channel shows, and so watches most closely.
WINDOW = 300
#: How often each poll runs, in seconds.
POSTS_EVERY_S = 300
COMMENTS_EVERY_S = 120
RECONCILE_EVERY_S = 60
WINDOW_EVERY_S = 3600
MEDIA_EVERY_S = 3600
TERMS_EVERY_S = 24 * 3600
SWEEP_EVERY_S = 7 * 24 * 3600
#: How far back a poll looks past the latest time it has, so that what
#: was written in the same second, or whose clock was behind, is not missed.
OVERLAP = timedelta(minutes=10)
#: The most pages one poll reads, against a flood; and the first poll of
#: comments, which reads the whole window's month (about 4,000 comments).
MOST_POLL_PAGES = 20
MOST_FIRST_COMMENT_PAGES = 100
#: The full download's collections, in order: names first, so that posts
#: find their writers and terms kept; each with whether it is read by month.
BACKFILL: tuple[tuple[str, str, bool], ...] = (
    ("users", "users", False),
    ("categories", "terms", False),
    ("tags", "terms", False),
    ("posts", "posts", True),
    ("pages", "posts", False),
    ("media", "media", True),
    ("comments", "comments", True),
)
#: The time form the API writes and is asked in.
TIME_FORM = "%Y-%m-%dT%H:%M:%S"


def parse_time(text: str) -> datetime:
    """Answer one of the API's UTC times."""
    return datetime.strptime(text, TIME_FORM).replace(tzinfo=UTC)


def zoned(when: datetime) -> str:
    """Answer a time as the API is asked it: UTC, with its zone."""
    return when.astimezone(UTC).strftime(TIME_FORM) + "Z"


def month_after(start: datetime) -> datetime:
    """Answer the first of the month after `start`'s."""
    return (start.replace(day=1) + timedelta(days=32)).replace(day=1, hour=0, minute=0, second=0)


def total_pages(headers: dict[str, str]) -> int:
    """Answer how many pages the API says a list has."""
    text = headers.get("x-wp-totalpages", "0")
    return int(text) if text.isdigit() else 0


@dataclass
class Changes:
    """What a step changed: "posts" in the window, "comments" on them, "archive" beyond."""

    kinds: set[str] = field(default_factory=set)


class Sync:
    """The reading of the API into the store."""

    def __init__(self, store: Store, fetcher: Fetcher) -> None:
        """Read through `fetcher` into `store`."""
        self.store = store
        self.fetcher = fetcher
        # When each poll is next due, on the monotonic clock; all at once at the start.
        self.due: dict[str, float] = {}

    def is_due(self, name: str, every_s: int) -> bool:
        """Answer whether poll `name` is due, and if so set when it is next."""
        now = time.monotonic()
        if now < self.due.get(name, 0.0):
            return False
        self.due[name] = now + every_s
        return True

    async def step(self) -> Changes:  # noqa: PLR0911 - one answer per kind of work, in order of urgency
        """Do the most urgent work that is due, and answer what it changed."""
        changes = Changes()
        if self.is_due("posts", POSTS_EVERY_S):
            await self.recent_posts(changes)
            return changes
        if self.is_due("comments", COMMENTS_EVERY_S):
            await self.recent_comments(changes)
            return changes
        if self.is_due("reconcile", RECONCILE_EVERY_S):
            await self.reconcile_comments(changes)
            return changes
        if self.is_due("window", WINDOW_EVERY_S):
            await self.check_window(changes)
            return changes
        if self.is_due("media", MEDIA_EVERY_S):
            await self.recent_media()
            return changes
        if self.is_due("terms", TERMS_EVERY_S):
            await self.refresh_names()
            return changes
        if await self.backfill_step(changes):
            return changes
        await self.sweep_step(changes)
        return changes

    @property
    def idle(self) -> bool:
        """Answer whether there was nothing to do last time: the full download and the sweep are done."""
        return bool(self.store.state("backfill", {}).get("done")) and not self.sweeping()

    # New and edited posts.

    async def recent_posts(self, changes: Changes) -> None:
        """Fetch whole the posts that are new or edited, and what they name."""
        # With fewer than the window kept, the whole window is looked at, so the channel has it at once.
        pages = 1 if self.store.post_count() >= WINDOW else WINDOW // PAGE_SIZE
        seen: dict[int, str] = {}
        for page in range(1, pages + 1):
            index, headers = await self.fetcher.json(
                api("posts", per_page=PAGE_SIZE, page=page, orderby="date", order="desc", _fields="id,modified_gmt"), "index"
            )
            self.store.set_state("posts.total", int(headers.get("x-wp-total", "0") or 0))
            seen.update({item["id"]: item["modified_gmt"] for item in index if isinstance(item, dict)})
        latest = self.store.latest_time("posts", "modified_gmt")
        if latest is not None:
            since = zoned(parse_time(latest) - OVERLAP)
            for page in range(1, MOST_POLL_PAGES + 1):
                edited, headers = await self.fetcher.json(
                    api(
                        "posts",
                        modified_after=since,
                        orderby="modified",
                        order="asc",
                        per_page=PAGE_SIZE,
                        page=page,
                        _fields="id,modified_gmt",
                    ),
                    "changes",
                )
                seen.update({item["id"]: item["modified_gmt"] for item in edited if isinstance(item, dict)})
                if page >= total_pages(headers):
                    break
        kept = self.store.modified("posts", seen)
        wanted = [ident for ident, modified in seen.items() if kept.get(ident) != modified]
        await self.fetch_posts(wanted, changes)

    async def fetch_posts(self, ids: list[int], changes: Changes) -> None:
        """Fetch posts whole by id, then the writers, terms and media records they name that are not kept."""
        window = set(self.store.post_ids_between(*self.window_dates()))
        objects: list[dict[str, Any]] = []
        for start in range(0, len(ids), PAGE_SIZE):
            chunk = ids[start : start + PAGE_SIZE]
            found, _ = await self.fetcher.json(api("posts", include=",".join(map(str, chunk)), per_page=PAGE_SIZE), "posts")
            objects.extend(item for item in found if isinstance(item, dict))
        await self.fetch_named(objects)
        changed = self.store.put_posts(objects)
        if changed:
            changes.kinds.add("posts" if set(changed) & window or not window else "archive")
            newest = self.window_dates()
            if any(newest[0] <= str(obj.get("date_gmt", "")) for obj in objects):
                changes.kinds.add("posts")

    async def fetch_named(self, posts: list[dict[str, Any]]) -> None:
        """Fetch the writers, categories, tags and media records `posts` name that are not kept."""
        named = {
            "users": [post.get("author", 0) for post in posts],
            "terms": [term for post in posts for term in (*post.get("categories", []), *post.get("tags", []))],
            "media": [post.get("featured_media", 0) for post in posts],
        }
        for table, ids in named.items():
            missing = self.store.missing(table, ids)
            collections = ["categories", "tags"] if table == "terms" else [table]
            for collection in collections:
                for start in range(0, len(missing), PAGE_SIZE):
                    chunk = missing[start : start + PAGE_SIZE]
                    found, _ = await self.fetcher.json(api(collection, include=",".join(map(str, chunk)), per_page=PAGE_SIZE), collection)
                    self.store.put_objects(table, [item for item in found if isinstance(item, dict)])

    def window_dates(self) -> tuple[str, str]:
        """Answer the dates of the window's oldest and newest kept posts."""
        posts = self.store.db.execute(
            "SELECT min(date_gmt), max(date_gmt) FROM (SELECT date_gmt FROM posts WHERE type = 'post' AND gone_at IS NULL "
            "ORDER BY date_gmt DESC LIMIT ?)",
            (WINDOW,),
        ).fetchone()
        return (str(posts[0] or ""), str(posts[1] or ""))

    async def check_window(self, changes: Changes) -> None:
        """Mark gone the posts among the newest `WINDOW` that the API no longer lists."""
        listed: dict[int, str] = {}
        for page in range(1, WINDOW // PAGE_SIZE + 1):
            index, _ = await self.fetcher.json(
                api("posts", per_page=PAGE_SIZE, page=page, orderby="date", order="desc", _fields="id,date_gmt"), "window"
            )
            listed.update({item["id"]: item["date_gmt"] for item in index if isinstance(item, dict)})
        if len(listed) < WINDOW:
            return
        kept = self.store.post_ids_between(min(listed.values()), max(listed.values()))
        if self.store.mark_gone("posts", kept - set(listed)):
            changes.kinds.add("posts")

    # Comments.

    async def recent_comments(self, changes: Changes) -> None:
        """Read the comments made since the latest kept, across the whole site."""
        latest = self.store.state("comments.after")
        oldest, _ = self.window_dates()
        # The first time, every comment on the window's posts.
        first = parse_time(oldest) if oldest else datetime.now(UTC) - timedelta(days=1)
        since = parse_time(latest) - OVERLAP if latest else first
        newest = latest
        window = self.store.post_ids_between(*self.window_dates())
        for page in range(1, (MOST_POLL_PAGES if latest else MOST_FIRST_COMMENT_PAGES) + 1):
            found, headers = await self.fetcher.json(
                api("comments", after=zoned(since), orderby="date_gmt", order="asc", per_page=PAGE_SIZE, page=page), "comments"
            )
            comments = [item for item in found if isinstance(item, dict)]
            if comments:
                newest = max(str(newest or ""), *(str(item.get("date_gmt", "")) for item in comments))
            self.store.put_objects("comments", comments, ("comments.after", newest))
            if any(item.get("post") in window for item in comments):
                changes.kinds.add("comments")
            if page >= total_pages(headers):
                break

    async def reconcile_comments(self, changes: Changes) -> None:
        """Read again every comment of the next post in the window, and mark gone those no longer there."""
        window = sorted(self.store.post_ids_between(*self.window_dates()), reverse=True)
        if not window:
            return
        place = int(self.store.state("reconcile.place", 0)) % len(window)
        post = window[place]
        found: list[dict[str, Any]] = []
        for page in range(1, MOST_POLL_PAGES + 1):
            items, headers = await self.fetcher.json(
                api("comments", post=post, orderby="id", order="asc", per_page=PAGE_SIZE, page=page), "reconcile"
            )
            found.extend(item for item in items if isinstance(item, dict))
            if page >= total_pages(headers):
                break
        before = self.store.comment_ids(post)
        listed = {item["id"] for item in found}
        self.store.put_objects("comments", found, ("reconcile.place", place + 1))
        gone = self.store.mark_gone("comments", before - listed)
        if gone or listed - before:
            changes.kinds.add("comments")

    # Media, categories and writers.

    async def recent_media(self) -> None:
        """Read the media records modified since the latest kept."""
        latest = self.store.latest_time("media", "modified_gmt")
        if latest is None:
            return
        since = zoned(parse_time(latest) - OVERLAP)
        for page in range(1, MOST_POLL_PAGES + 1):
            found, headers = await self.fetcher.json(
                api("media", modified_after=since, orderby="modified", order="asc", per_page=PAGE_SIZE, page=page), "media"
            )
            self.store.put_objects("media", [item for item in found if isinstance(item, dict)])
            if page >= total_pages(headers):
                break

    async def refresh_names(self) -> None:
        """Read the categories and the writers again: few, and their counts change."""
        for collection, table in (("categories", "terms"), ("users", "users")):
            for page in range(1, MOST_POLL_PAGES + 1):
                found, headers = await self.fetcher.json(
                    api(collection, per_page=PAGE_SIZE, page=page, orderby="id", order="asc"), collection
                )
                self.store.put_objects(table, [item for item in found if isinstance(item, dict)])
                if page >= total_pages(headers):
                    break

    # The full download.

    async def backfill_step(self, changes: Changes) -> bool:
        """Read the next page of the full download; answer False when it is done."""
        place: dict[str, Any] = self.store.state("backfill", {"stage": 0, "page": 1, "window": None})
        if place.get("done"):
            return False
        stage = int(place["stage"])
        collection, table, by_month = BACKFILL[stage]
        query: dict[str, str | int] = {"per_page": PAGE_SIZE, "page": int(place["page"]), "orderby": "id", "order": "asc"}
        if by_month:
            if place.get("window") is None:
                first, _ = await self.fetcher.json(api(collection, per_page=1, orderby="date", order="asc", _fields="date_gmt"), "backfill")
                start = parse_time(first[0]["date_gmt"]) if first else datetime.now(UTC)
                place = {**place, "window": zoned(start.replace(day=1, hour=0, minute=0, second=0))}
                self.store.set_state("backfill", place)
            start = datetime.strptime(str(place["window"]), TIME_FORM + "Z").replace(tzinfo=UTC)
            # `after` leaves out the instant given: a second before the month takes in its first second.
            query |= {"after": zoned(start - timedelta(seconds=1)), "before": zoned(month_after(start))}
        found, headers = await self.fetcher.json(api(collection, **query), "backfill")
        items = [item for item in found if isinstance(item, dict)]
        place = self.next_place(place, by_month=by_month, finished=int(place["page"]) >= total_pages(headers))
        if table == "posts":
            self.store.put_posts(items, ("backfill", place))
        else:
            self.store.put_objects(table, items, ("backfill", place))
        changes.kinds.add("archive")
        return True

    @staticmethod
    def next_place(place: dict[str, Any], *, by_month: bool, finished: bool) -> dict[str, Any]:
        """Answer the full download's place after a page: the next page, month or collection, or done."""
        if not finished:
            return {**place, "page": int(place["page"]) + 1}
        if by_month:
            start = datetime.strptime(str(place["window"]), TIME_FORM + "Z").replace(tzinfo=UTC)
            following = month_after(start)
            if following <= datetime.now(UTC):
                return {**place, "page": 1, "window": zoned(following)}
        stage = int(place["stage"]) + 1
        if stage >= len(BACKFILL):
            return {"stage": stage, "page": 1, "window": None, "done": True, "finished_at": zoned(datetime.now(UTC))}
        return {"stage": stage, "page": 1, "window": None}

    # The sweep for posts that have gone.

    def sweeping(self) -> bool:
        """Answer whether a sweep is under way or due."""
        place = self.store.state("sweep", {})
        if place.get("page"):
            return True
        last = place.get("finished_at")
        return last is None or datetime.now(UTC) - datetime.strptime(last, TIME_FORM + "Z").replace(tzinfo=UTC) > timedelta(
            seconds=SWEEP_EVERY_S
        )

    async def sweep_step(self, changes: Changes) -> None:
        """Read the next page of every post's id, and mark gone the kept posts between them that it lacks."""
        if not self.store.state("backfill", {}).get("done") or not self.sweeping():
            return
        place: dict[str, Any] = self.store.state("sweep", {})
        page = int(place.get("page") or 1)
        after = int(place.get("after_id") or 0)
        found, headers = await self.fetcher.json(
            api("posts", _fields="id", orderby="id", order="asc", per_page=PAGE_SIZE, page=page), "sweep"
        )
        ids = {item["id"] for item in found if isinstance(item, dict)}
        last = max(ids) if ids else after
        finished = page >= total_pages(headers)
        upto = 2**62 if finished else last
        if self.store.mark_gone("posts", self.store.post_ids_from(after + 1, upto) - ids):
            changes.kinds.add("archive")
        if finished:
            self.store.set_state("sweep", {"finished_at": zoned(datetime.now(UTC))})
        else:
            self.store.set_state("sweep", {"page": page + 1, "after_id": last})


async def run_until_done(sync: Sync) -> None:
    """Step until the full download is done: the command line's use, without the channel."""
    told = None
    while not sync.store.state("backfill", {}).get("done"):
        try:
            await sync.step()
        except FetchError as error:
            print(f"hackaday: {error}", flush=True)  # noqa: T201 - the command line's progress
        place = sync.store.state("backfill", {})
        where = (place.get("stage"), place.get("window"))
        if where != told:
            told = where
            print(f"hackaday: {place} {sync.store.counts()}", flush=True)  # noqa: T201 - the command line's progress
