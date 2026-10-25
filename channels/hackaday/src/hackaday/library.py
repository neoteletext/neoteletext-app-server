"""What the channel knows of Hackaday: the store's copy of the whole archive, kept up to date.

- The store (`store.py`) holds every post, comment, tag, category,
  writer and media record the REST API publishes, and every request made.
- The sync (`sync.py`) reads all of it once, then what changes, in the
  background, a request at a time.
- The channel shows the newest `LATEST` posts with their comments; every
  list from the archive (this date in past years, the first posts, a tag's
  posts, a writer's, searches, a random post) is read from the store.
- Each shown post's lead picture is fetched once and made into a teletext
  page once, both kept in the store.
"""

from __future__ import annotations

import asyncio
import itertools
import sys
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import TYPE_CHECKING

from hackaday.download import Download, fetchers
from hackaday.fetch import Fetcher, FetchError
from hackaday.posts import Post, comment_of, post_of
from hackaday.store import Store
from hackaday.sync import Sync
from hackaday.tools import PICTURE_LEVEL, ToolError, picture_tti

if TYPE_CHECKING:
    import random
    from collections.abc import Awaitable, Callable

    from hackaday.store import StoredPost

#: Posts the channel shows: about a month's (300 posts were 21 September to
#: 25 October 2026), enough to fill each subject magazine's 90 story pages
#: no further than its busiest needs.
LATEST = 300
#: The width pictures are asked for, in pixels: WordPress scales on
#: request, and the converter needs no more for a page's mosaic cells.
PICTURE_WIDTH = 400
#: The store's file, in the channel's cache.
DATABASE = "hackaday.sqlite"
#: How long after a download that left pages unread another starts, in seconds.
DOWNLOAD_AGAIN_S = 300
#: How long the sync waits when it has nothing to do, in seconds.
IDLE_S = 5.0
#: How long the picture maker waits when every shown picture is made, in seconds.
PICTURES_IDLE_S = 30.0


@dataclass
class Library:
    """The channel's knowledge, and who to tell when it changes."""

    store: Store
    fetcher: Fetcher
    posts: list[Post] = field(default_factory=list)
    picture_pages: dict[int, str] = field(default_factory=dict)
    on_change: Callable[[str], Awaitable[None]] | None = None
    on_news: Callable[[Post], Awaitable[None]] | None = None
    # Posts made from the store, kept while their modification time and picture stand.
    _made: dict[int, tuple[str, Post]] = field(default_factory=dict)
    # Pictures that could not be made, not tried again until a restart.
    _failed: set[str] = field(default_factory=set)

    @classmethod
    def open(cls, database: Path | str) -> Library:
        """Open the library on its database, every request kept in it."""
        library = cls(Store(database), Fetcher())
        library.fetcher.on_fetch = library.store.log_fetch
        return library

    @property
    def total(self) -> int:
        """How many posts the archive has: as the API last said, or as many as are kept."""
        return max(int(self.store.state("posts.total", 0)), self.store.post_count())

    def post(self, post_id: int) -> Post | None:
        """Answer a post the channel shows."""
        return next((post for post in self.posts if post.id == post_id), None)

    async def changed(self, what: str) -> None:
        """Tell the channel that `what` changed: "posts", "comments" or "pictures"."""
        if self.on_change is not None:
            await self.on_change(what)

    def made(self, stored: list[StoredPost]) -> list[Post]:
        """Answer the store's posts as the channel shows them, each made once while it is unchanged."""
        posts: list[Post] = []
        for each in stored:
            ident, modified = int(each.data["id"]), str(each.data.get("modified_gmt", ""))
            kept = self._made.get(ident)
            if kept is not None and kept[0] == modified and kept[1].picture_url == each.picture_url:
                posts.append(kept[1])
                continue
            post = post_of(each)
            if post is not None:
                self._made[ident] = (modified, post)
                posts.append(post)
        return posts

    def load(self) -> list[Post]:
        """Read the newest posts and their comments from the store; answer those not shown before, if any were."""
        known = {post.id for post in self.posts}
        posts = self.made(self.store.latest(LATEST))
        shown = {post.id for post in posts}
        self._made = {ident: made for ident, made in self._made.items() if ident in shown}
        self.posts = posts
        self.load_comments()
        self.picture_pages = {
            post.id: page for post in posts if post.picture_url and (page := self.store.picture_page(post.picture_url)) is not None
        }
        return [post for post in posts if post.id not in known] if known else []

    def load_comments(self) -> None:
        """Give each shown post its comments from the store."""
        found = self.store.comments_of([post.id for post in self.posts])
        for post in self.posts:
            comments = [comment for raw in found.get(post.id, []) if (comment := comment_of(raw)) is not None]
            post.comments, post.comment_count = comments, len(comments)

    # The archive's lists, read from the store.

    async def on_this_day(self, today: datetime) -> list[Post]:
        """Answer one post from this date in each past year, newest year first."""
        return self.made(self.store.on_this_day(today.strftime("%m-%d"), today.strftime("%Y")))

    async def first_posts(self, count: int = 10) -> list[Post]:
        """Answer the archive's first posts."""
        return self.made(self.store.first(count))

    async def search(self, words: str, count: int = 20) -> list[Post]:
        """Answer the archive's posts with every word of `words`, the best first."""
        return self.made(self.store.search(words, count))

    async def latest_titled(self, start: str) -> Post | None:
        """Answer the newest post whose title starts with `start`: a column's latest edition."""
        stored = self.store.latest_titled(start)
        found = self.made([stored]) if stored is not None else []
        return found[0] if found else None

    async def tagged(self, tag: str, count: int = 30) -> list[Post]:
        """Answer the newest posts with a tag, by its slug."""
        return self.made(self.store.tagged(tag, count))

    async def author(self, author: int, count: int = 3) -> tuple[int, list[str]]:
        """Answer how many posts a writer has, and the titles of their latest."""
        total, latest = self.store.by_author(author, count)
        return total, [post.title for post in self.made(latest)]

    async def random_post(self, generator: random.Random) -> Post | None:
        """Answer a post from anywhere in the archive."""
        count = self.store.post_count()
        stored = self.store.post_at(generator.randrange(count)) if count else None
        found = self.made([stored]) if stored is not None else []
        return found[0] if found else None

    # Keeping up to date.

    async def run(self) -> None:
        """Keep the store up to date and the channel told, and make the shown posts' pictures, until cancelled."""
        self.load()
        await self.changed("posts")
        pictures = asyncio.ensure_future(self.make_pictures())
        downloading = None if self.store.state("backfill", {}).get("done") else asyncio.ensure_future(self.download())
        sync = Sync(self.store, self.fetcher)
        try:
            while True:
                try:
                    changes = await sync.step()
                except FetchError as error:
                    print(f"hackaday: {error}", file=sys.stderr)  # noqa: T201
                    continue
                if "posts" in changes.kinds:
                    fresh = self.load()
                    await self.changed("posts")
                    for post in sorted(fresh, key=lambda post: post.date):
                        if self.on_news is not None:
                            await self.on_news(post)
                elif "comments" in changes.kinds:
                    self.load_comments()
                    await self.changed("comments")
                if sync.idle and not changes.kinds:
                    await asyncio.sleep(IDLE_S)
        finally:
            pictures.cancel()
            if downloading is not None:
                downloading.cancel()

    async def download(self) -> None:
        """Read the whole archive beside the polls, paced (`download.py`), until it is all read."""
        for _ in itertools.count():
            if await Download(self.store, fetchers(self.store.log_fetch)).run():
                return
            # Pages left unread are tried again after a pause, as many times as it takes.
            await asyncio.sleep(DOWNLOAD_AGAIN_S)

    async def make_pictures(self) -> None:
        """Make the shown posts' pictures into pages, one at a time, newest first."""
        while True:
            waiting = [
                post
                for post in self.posts
                if post.picture_url and post.id not in self.picture_pages and post.picture_url not in self._failed
            ]
            if not waiting:
                await asyncio.sleep(PICTURES_IDLE_S)
                continue
            post = waiting[0]
            try:
                await self.make_picture(post)
            except (ToolError, FetchError) as error:
                self._failed.add(post.picture_url)
                print(f"hackaday: no picture for {post.id}: {error}", file=sys.stderr)  # noqa: T201
                continue
            await self.changed("pictures")

    async def make_picture(self, post: Post) -> None:
        """Make a post's picture into a page, fetching the file only if the store lacks the page."""
        made = self.store.picture_page(post.picture_url)
        if made is None:
            answer = await self.fetcher.get(f"{post.picture_url.split('?')[0]}?w={PICTURE_WIDTH}", "picture")
            suffix = Path(post.picture_url.split("?")[0]).suffix or ".jpg"
            made = await picture_tti(answer.body, suffix)
            self.store.put_picture(post.picture_url, answer.headers, answer.body, PICTURE_LEVEL, made)
        self.picture_pages[post.id] = made


def now_utc() -> datetime:
    """Answer the time now."""
    return datetime.now(UTC)
