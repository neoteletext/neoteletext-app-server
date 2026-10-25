"""What the channel knows of Hackaday, kept up to date in the background.

- The latest `LATEST` posts, with their authors, terms and lead pictures,
  from the REST API; checked every `POSTS_EVERY_S`. A post not seen before
  is news.
- Each post's comments, the newest posts' more often.
- Each post's lead picture as a teletext page, made once.
- Lists from the whole archive (58,711 posts since 2004 on 25 October
  2026): this date in past years, the first posts, searches, a random one.
"""

from __future__ import annotations

import asyncio
import json
import sys
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import TYPE_CHECKING, Any

from hackaday.fetch import Fetcher, FetchError, api
from hackaday.html import plain
from hackaday.posts import Post, comment_of, post_of
from hackaday.tools import ToolError, picture_tti

if TYPE_CHECKING:
    import random
    from collections.abc import Awaitable, Callable

#: Posts the channel holds: one magazine of story pages.
LATEST = 100
#: How often the newest posts are checked, in seconds: Hackaday posts
#: about every three hours (8.9 a day, measured 13 to 25 October 2026).
POSTS_EVERY_S = 300
#: How often the newest posts' comments are read, and how many posts count as newest.
NEW_COMMENTS_EVERY_S = 600
NEWEST = 15
#: How often every post's comments are read.
ALL_COMMENTS_EVERY_S = 3600
#: The most comments read for one post (the most this fortnight was 99).
MOST_COMMENTS = 100
#: The REST API's most items a request (WordPress `per_page`).
PAGE_SIZE = 100
#: What the posts are asked for with.
EMBED = "author,wp:featuredmedia,wp:term"
#: The archive's first year.
FIRST_YEAR = 2004


@dataclass
class Library:
    """The channel's knowledge, and who to tell when it changes."""

    fetcher: Fetcher
    pictures: Path
    posts: list[Post] = field(default_factory=list)
    picture_pages: dict[int, str] = field(default_factory=dict)
    total: int = 0
    on_change: Callable[[str], Awaitable[None]] | None = None
    on_news: Callable[[Post], Awaitable[None]] | None = None

    def post(self, post_id: int) -> Post | None:
        """Answer a post the channel holds."""
        return next((post for post in self.posts if post.id == post_id), None)

    async def changed(self, what: str) -> None:
        """Tell the channel that `what` changed: "posts", "comments" or "pictures"."""
        if self.on_change is not None:
            await self.on_change(what)

    async def load_posts(self, count: int = LATEST) -> list[Post]:
        """Read the newest `count` posts."""
        answer = await self.fetcher.get(api("posts", per_page=min(count, PAGE_SIZE), _embed=EMBED))
        self.total = int(answer.headers.get("x-wp-total", "0") or 0)
        raw = json.loads(answer.body)
        return [post for item in raw if isinstance(item, dict) and (post := post_of(item)) is not None]

    async def refresh_posts(self) -> None:
        """Bring the posts up to date, telling of each new one."""
        fresh = await self.load_posts()
        known = {post.id for post in self.posts}
        news = [post for post in fresh if post.id not in known] if known else []
        kept = {post.id: post for post in self.posts}
        for post in fresh:
            old = kept.get(post.id)
            if old is not None:
                post.comments, post.comment_count = old.comments, old.comment_count
        self.posts = sorted(fresh, key=lambda post: post.date, reverse=True)[:LATEST]
        await self.changed("posts")
        for post in sorted(news, key=lambda post: post.date):
            if self.on_news is not None:
                await self.on_news(post)

    async def load_comments(self, post: Post) -> None:
        """Read one post's comments."""
        url = api("comments", post=post.id, per_page=MOST_COMMENTS, order="asc", _fields="id,parent,author_name,date_gmt,content")
        answer = await self.fetcher.get(url)
        raw: list[Any] = json.loads(answer.body)
        post.comments = [comment for item in raw if isinstance(item, dict) and (comment := comment_of(item)) is not None]
        post.comment_count = int(answer.headers.get("x-wp-total", len(post.comments)) or 0)

    async def load_picture(self, post: Post) -> None:
        """Make the post's lead picture into a teletext page, once; kept on disk."""
        kept = self.pictures / f"{post.id}.tti"
        if kept.exists():
            self.picture_pages[post.id] = kept.read_text(encoding="latin-1")
            return
        if not post.picture_url:
            return
        answer = await self.fetcher.get(post.picture_url.split("?")[0] + "?w=400")
        suffix = Path(post.picture_url.split("?")[0]).suffix or ".jpg"
        page = await picture_tti(answer.body, suffix)
        kept.write_text(page, encoding="latin-1")
        self.picture_pages[post.id] = page

    async def on_this_day(self, today: datetime) -> list[Post]:
        """Answer one post from this date in each past year, newest year first."""
        found: list[Post] = []
        for year in range(today.year - 1, FIRST_YEAR - 1, -1):
            try:
                day = today.replace(year=year)
            except ValueError:
                continue
            start = day.replace(hour=0, minute=0, second=0, microsecond=0)
            url = api("posts", after=start.isoformat(), before=(start + timedelta(days=1)).isoformat(), per_page=1, _embed=EMBED)
            raw = await self.fetcher.json(url)
            found.extend(post for item in raw if isinstance(item, dict) and (post := post_of(item)) is not None)
        return found

    async def first_posts(self, count: int = 10) -> list[Post]:
        """Answer the archive's first posts."""
        raw = await self.fetcher.json(api("posts", per_page=count, order="asc", _embed=EMBED))
        return [post for item in raw if isinstance(item, dict) and (post := post_of(item)) is not None]

    async def search(self, words: str, count: int = 20) -> list[Post]:
        """Answer the archive's posts for `words`."""
        raw = await self.fetcher.json(api("posts", search=words[:80], per_page=count, _embed=EMBED))
        return [post for item in raw if isinstance(item, dict) and (post := post_of(item)) is not None]

    async def tagged(self, tag: str, count: int = 30) -> list[Post]:
        """Answer the archive's posts with a tag, by its slug."""
        tags = await self.fetcher.json(api("tags", slug=tag, _fields="id"))
        if not tags:
            return []
        raw = await self.fetcher.json(api("posts", tags=tags[0]["id"], per_page=count, _embed=EMBED))
        return [post for item in raw if isinstance(item, dict) and (post := post_of(item)) is not None]

    async def author(self, author: int, count: int = 3) -> tuple[int, list[str]]:
        """Answer how many posts an author has written, and the titles of their latest."""
        answer = await self.fetcher.get(api("posts", author=author, per_page=count, _fields="title"))
        titles = [plain(item["title"]["rendered"]) for item in json.loads(answer.body) if isinstance(item, dict) and "title" in item]
        return int(answer.headers.get("x-wp-total", "0") or 0), titles

    async def random_post(self, generator: random.Random) -> Post | None:
        """Answer a post from anywhere in the archive."""
        if not self.total:
            return None
        raw = await self.fetcher.json(api("posts", per_page=1, offset=generator.randrange(self.total), _embed=EMBED))
        return next((post for item in raw if isinstance(item, dict) and (post := post_of(item)) is not None), None)

    async def run(self) -> None:
        """Keep everything up to date until cancelled."""
        loop = asyncio.get_running_loop()
        next_posts = next_new_comments = next_all_comments = loop.time()
        pictures_done: set[int] = set()
        while True:
            now = loop.time()
            try:
                if now >= next_posts:
                    await self.refresh_posts()
                    next_posts = now + POSTS_EVERY_S
                if now >= next_new_comments:
                    for post in self.posts[:NEWEST]:
                        await self.load_comments(post)
                    await self.changed("comments")
                    next_new_comments = now + NEW_COMMENTS_EVERY_S
                if now >= next_all_comments:
                    for post in self.posts[NEWEST:]:
                        await self.load_comments(post)
                    await self.changed("comments")
                    next_all_comments = now + ALL_COMMENTS_EVERY_S
                waiting = [post for post in self.posts if post.id not in pictures_done]
                if waiting:
                    post = waiting[0]
                    pictures_done.add(post.id)
                    try:
                        await self.load_picture(post)
                        await self.changed("pictures")
                    except (ToolError, FetchError) as error:
                        print(f"hackaday: no picture for {post.id}: {error}", file=sys.stderr)  # noqa: T201
                    continue
            except FetchError as error:
                print(f"hackaday: {error}", file=sys.stderr)  # noqa: T201
            await asyncio.sleep(max(1.0, min(next_posts, next_new_comments, next_all_comments) - loop.time()))


def now_utc() -> datetime:
    """Answer the time now."""
    return datetime.now(UTC)
