"""Hackaday's posts as the channel shows them, from the store's copy of the WordPress REST API.

The API separates a post's categories (a fixed set of about 100) from its
tags (a long tail), which the RSS feed mixes (measured 25 October 2026).
The store names a post's writer, terms and lead picture from their own
collections (`store.py`).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import TYPE_CHECKING, Any

from hackaday import site
from hackaday.html import Article, plain, read
from hackaday.site import HOUSEKEEPING

if TYPE_CHECKING:
    from hackaday.store import StoredPost


@dataclass(frozen=True)
class Comment:
    """One comment: who, when, what, and which it answers."""

    id: int
    parent: int
    author: str
    date: datetime
    text: str


@dataclass
class Post:
    """One post."""

    id: int
    date: datetime
    title: str
    link: str
    author: str
    categories: list[str]
    tags: list[str]
    article: Article
    excerpt: str
    picture_url: str
    comments: list[Comment] = field(default_factory=list)
    comment_count: int = 0

    @property
    def sections(self) -> list[str]:
        """Answer the names of the magazines the post's labels name, its own first (`site.py`)."""
        return [magazine.name for magazine in site.magazines_of(self)]

    def has(self, label: str) -> bool:
        """Answer whether a category or tag of the post is `label`, in any case."""
        wanted = label.lower()
        return any(name.lower() == wanted for name in (*self.categories, *self.tags))


def _date(text: str) -> datetime:
    return datetime.fromisoformat(text).replace(tzinfo=UTC)


def post_of(stored: StoredPost) -> Post | None:
    """Answer a post from the store, or None when it is not one."""
    raw = stored.data
    try:
        categories = [plain(name) for taxonomy, name in stored.terms if taxonomy == "category" and plain(name).lower() not in HOUSEKEEPING]
        tags = [plain(name) for taxonomy, name in stored.terms if taxonomy == "post_tag"]
        return Post(
            id=int(raw["id"]),
            date=_date(raw["date_gmt"]),
            title=plain(raw["title"]["rendered"]),
            link=str(raw["link"]),
            author=plain(stored.author or "Hackaday"),
            categories=categories,
            tags=tags,
            article=read(raw["content"]["rendered"]),
            excerpt=plain(raw.get("excerpt", {}).get("rendered", "")).removesuffix("...read more").removesuffix("read more").strip(),
            picture_url=stored.picture_url,
        )
    except (KeyError, TypeError, ValueError):
        return None


def comment_of(raw: dict[str, Any]) -> Comment | None:
    """Answer a comment from the REST API, or None when it is not one."""
    try:
        return Comment(
            id=int(raw["id"]),
            parent=int(raw.get("parent") or 0),
            author=plain(raw.get("author_name") or "Someone"),
            date=_date(raw["date_gmt"]),
            text=" ".join(block.text for block in read(raw["content"]["rendered"]).blocks),
        )
    except (KeyError, TypeError, ValueError):
        return None


def threaded(comments: list[Comment]) -> list[tuple[int, Comment]]:
    """Answer the comments in thread order, each with its depth: replies under what they answer."""
    children: dict[int, list[Comment]] = {}
    known = {comment.id for comment in comments}
    for comment in sorted(comments, key=lambda each: each.date):
        parent = comment.parent if comment.parent in known else 0
        children.setdefault(parent, []).append(comment)
    ordered: list[tuple[int, Comment]] = []
    placed: set[int] = set()
    stack = [(0, comment) for comment in reversed(children.get(0, []))]
    while stack:
        depth, comment = stack.pop()
        # A comment that answers itself, or a loop of them, is shown once.
        if comment.id in placed:
            continue
        placed.add(comment.id)
        ordered.append((depth, comment))
        stack.extend((depth + 1, child) for child in reversed(children.get(comment.id, [])))
    return ordered
