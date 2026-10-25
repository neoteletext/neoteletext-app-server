"""Hackaday's posts as the channel keeps them, from the WordPress REST API.

The API separates a post's categories (a fixed set of about 100) from its
tags (a long tail), which the RSS feed mixes (measured 25 October 2026).
`_embed` brings the author, the terms and the lead picture in the same
answer.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any

from hackaday.html import Article, plain, read

#: Categories the site uses for its own pages, not subjects: never shown.
HOUSEKEEPING = frozenset({"slider", "featured", "original art", "uncategorized"})

#: The channel's sections, each a page, and the categories in each (by
#: name, lower case). A post is in every section one of its categories is.
SECTIONS: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("Retrocomputing", ("retrocomputing", "classic hacks", "history", "retrotechtacular")),
    ("Radio", ("radio hacks", "wireless hacks", "sdr")),
    (
        "Making",
        (
            "3d printer hacks",
            "tool hacks",
            "cnc hacks",
            "repair hacks",
            "led hacks",
            "home hacks",
            "clock hacks",
            "wearable hacks",
            "how-to",
        ),
    ),
    (
        "Hardware",
        (
            "microcontrollers",
            "raspberry pi",
            "arduino hacks",
            "hardware",
            "parts",
            "peripherals hacks",
            "computer hacks",
            "cyberdecks",
            "teardown",
            "reverse engineering",
        ),
    ),
    ("Science and space", ("science", "space", "chemistry hacks", "medical hacks", "green hacks", "engineering", "solar hacks")),
    (
        "Games and music",
        (
            "games",
            "toy hacks",
            "nintendo ds hacks",
            "home entertainment hacks",
            "musical hacks",
            "digital audio hacks",
            "video hacks",
            "art",
        ),
    ),
    (
        "Security and software",
        (
            "security hacks",
            "software hacks",
            "software development",
            "linux hacks",
            "artificial intelligence",
            "internet hacks",
            "phone hacks",
        ),
    ),
    ("Getting about", ("transportation hacks", "car hacks", "drone hacks", "robots hacks", "weapons hacks")),
    (
        "News and columns",
        ("hackaday columns", "news", "current events", "cons", "contests", "interest", "rants", "reviews", "podcasts", "hackaday links"),
    ),
)
#: The section of posts no other section holds.
OTHER_SECTION = "Everything else"


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
        """Answer the channel's sections the post is in."""
        names = {category.lower() for category in self.categories} | {tag.lower() for tag in self.tags}
        found = [name for name, members in SECTIONS if names & set(members)]
        return found or [OTHER_SECTION]

    def has(self, label: str) -> bool:
        """Answer whether a category or tag of the post is `label`, in any case."""
        wanted = label.lower()
        return any(name.lower() == wanted for name in (*self.categories, *self.tags))


def _date(text: str) -> datetime:
    return datetime.fromisoformat(text).replace(tzinfo=UTC)


def post_of(raw: dict[str, Any]) -> Post | None:
    """Answer a post from one REST API answer with `_embed`, or None when it is not one."""
    try:
        embedded = raw.get("_embedded") or {}
        authors = embedded.get("author") or [{}]
        terms = [term for group in embedded.get("wp:term") or [] for term in group]
        media = (embedded.get("wp:featuredmedia") or [{}])[0] or {}
        categories = [
            plain(term["name"]) for term in terms if term.get("taxonomy") == "category" and term.get("name", "").lower() not in HOUSEKEEPING
        ]
        tags = [plain(term["name"]) for term in terms if term.get("taxonomy") == "post_tag"]
        return Post(
            id=int(raw["id"]),
            date=_date(raw["date_gmt"]),
            title=plain(raw["title"]["rendered"]),
            link=str(raw["link"]),
            author=plain(authors[0].get("name") or "Hackaday"),
            categories=categories,
            tags=tags,
            article=read(raw["content"]["rendered"]),
            excerpt=plain(raw.get("excerpt", {}).get("rendered", "")).removesuffix("...read more").removesuffix("read more").strip(),
            picture_url=str(media.get("source_url") or raw.get("jetpack_featured_media_url") or ""),
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
