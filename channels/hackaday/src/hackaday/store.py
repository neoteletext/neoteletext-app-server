"""Hackaday's whole public archive in SQLite: what the channel reads, and what the sync keeps.

The schema is `migrations/NNNN_name.sql`, applied in order, each in a
transaction that also sets `PRAGMA user_version` to its number. Every API
object is kept whole as JSON; the columns looked up by are generated from
it (`0001_archive.sql`). Queries bind every value.
"""

from __future__ import annotations

import json
import re
import sqlite3
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import UTC, datetime
from importlib import resources
from typing import TYPE_CHECKING, Any

from hackaday.html import plain, read

if TYPE_CHECKING:
    from collections.abc import Iterable, Iterator
    from pathlib import Path

    from hackaday.fetch import Fetched

#: How long a write waits for another process's, in milliseconds.
BUSY_TIMEOUT_MS = 30_000
#: A migration's file name: its number, then its name.
MIGRATION = re.compile(r"^(\d{4})_[a-z0-9_]+\.sql$")
#: The longest search, in characters, and the most words it is made of.
LONGEST_SEARCH = 80
MOST_SEARCH_WORDS = 8
#: How a search ranks a title's words against the text's (FTS5 `bm25` weights).
TITLE_WEIGHT = 10.0
TEXT_WEIGHT = 1.0
#: A comment the public sees.
APPROVED = "approved"


def now_text() -> str:
    """Answer the time now as the API writes its times: `2026-10-25T17:00:31`, UTC."""
    return datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%S")


@dataclass(frozen=True)
class PageRead:
    """A page of the full download, kept with what it read: its collection, month (or ''), number, the pages in all and its items."""

    collection: str
    month: str
    page: int
    total_pages: int
    items: int


@dataclass(frozen=True)
class StoredPost:
    """A post as the channel reads it: its whole object, and the names its ids stand for."""

    data: dict[str, Any]
    author: str
    terms: list[tuple[str, str]]
    picture_url: str


def migrations() -> list[tuple[int, str]]:
    """Answer the migrations, in order: each one's number and its SQL."""
    found: list[tuple[int, str]] = []
    for entry in (resources.files("hackaday") / "migrations").iterdir():
        matched = MIGRATION.match(entry.name)
        if matched:
            found.append((int(matched.group(1)), entry.read_text(encoding="utf-8")))
    found.sort()
    numbers = [number for number, _ in found]
    if numbers != list(range(1, len(numbers) + 1)):
        message = f"migrations are not numbered 1 to {len(numbers)}: {numbers}"
        raise ValueError(message)
    return found


def search_words(typed: str) -> str | None:
    """Answer what is typed as an FTS5 query: each word quoted, all of them needed; None for no words.

    Quoting every word leaves nothing of FTS5's own syntax in what a viewer types.
    """
    words = re.findall(r"\w+", typed[:LONGEST_SEARCH])[:MOST_SEARCH_WORDS]
    return " ".join(f'"{word}"' for word in words) or None


def article_text(html: str) -> str:
    """Answer a post's text, for search."""
    return "\n".join(block.text for block in read(html).blocks)


class Store:
    """The database, open."""

    def __init__(self, path: Path | str) -> None:
        """Open the database at `path` (":memory:" for none), and bring its schema up to date."""
        self.db = sqlite3.connect(str(path), autocommit=True)
        self.db.row_factory = sqlite3.Row
        self.db.execute(f"PRAGMA busy_timeout = {BUSY_TIMEOUT_MS}")
        self.db.execute("PRAGMA foreign_keys = ON")
        if str(path) != ":memory:":
            self.db.execute("PRAGMA journal_mode = WAL")
            # WAL's NORMAL loses at most the last commits on a power cut, never the database.
            self.db.execute("PRAGMA synchronous = NORMAL")
        self.migrate()

    def close(self) -> None:
        """Close the database."""
        self.db.close()

    def migrate(self) -> None:
        """Apply each migration newer than the database, each in its own transaction."""
        current = int(self.db.execute("PRAGMA user_version").fetchone()[0])
        for number, sql in migrations():
            if number > current:
                self.db.executescript(f"BEGIN IMMEDIATE;\n{sql}\nPRAGMA user_version = {number};\nCOMMIT;")

    @contextmanager
    def transaction(self) -> Iterator[sqlite3.Connection]:
        """Do the work inside in one transaction, undone if it fails."""
        self.db.execute("BEGIN IMMEDIATE")
        try:
            yield self.db
        except BaseException:
            self.db.execute("ROLLBACK")
            raise
        self.db.execute("COMMIT")

    # Keeping what the API gave.

    def put_posts(self, objects: Iterable[dict[str, Any]], state: tuple[str, Any] | None = None, page: PageRead | None = None) -> list[int]:
        """Keep posts or pages whole, their terms and their search text; answer the ids that were new or changed."""
        rows = [
            (obj, plain(obj["title"]["rendered"]), article_text(obj.get("content", {}).get("rendered", "")))
            for obj in objects
            if isinstance(obj, dict) and isinstance(obj.get("id"), int)
        ]
        changed: list[int] = []
        with self.transaction() as db:
            for obj, title, text in rows:
                ident = obj["id"]
                before = db.execute("SELECT modified_gmt, gone_at FROM posts WHERE id = ?", (ident,)).fetchone()
                if before is None or before["modified_gmt"] != obj.get("modified_gmt") or before["gone_at"] is not None:
                    changed.append(ident)
                encoded = json.dumps(obj, ensure_ascii=False, separators=(",", ":"))
                db.execute(
                    "INSERT INTO posts (id, data, title) VALUES (?, ?, ?) "
                    "ON CONFLICT (id) DO UPDATE SET data = excluded.data, title = excluded.title, "
                    "fetched_at = excluded.fetched_at, gone_at = NULL",
                    (ident, encoded, title),
                )
                db.execute("DELETE FROM post_terms WHERE post = ?", (ident,))
                db.execute(
                    "INSERT OR IGNORE INTO post_terms (post, term) "
                    "SELECT ?, value FROM json_each(?, '$.categories') UNION SELECT ?, value FROM json_each(?, '$.tags')",
                    (ident, encoded, ident, encoded),
                )
                db.execute("INSERT OR REPLACE INTO posts_search (rowid, title, body) VALUES (?, ?, ?)", (ident, title, text))
            self._state(db, state)
            self._page(db, page)
        return changed

    def put_objects(
        self, table: str, objects: Iterable[dict[str, Any]], state: tuple[str, Any] | None = None, page: PageRead | None = None
    ) -> int:
        """Keep terms, users, media or comments whole; answer how many."""
        if table not in {"terms", "users", "media", "comments"}:
            message = f"no such table: {table}"
            raise ValueError(message)
        kept = 0
        gone = ", gone_at = NULL" if table in {"media", "comments"} else ""
        with self.transaction() as db:
            for obj in objects:
                if not isinstance(obj, dict) or not isinstance(obj.get("id"), int):
                    continue
                db.execute(
                    f"INSERT INTO {table} (id, data) VALUES (?, ?) "  # noqa: S608 - the table is one of four names above
                    f"ON CONFLICT (id) DO UPDATE SET data = excluded.data, fetched_at = excluded.fetched_at{gone}",
                    (obj["id"], json.dumps(obj, ensure_ascii=False, separators=(",", ":"))),
                )
                kept += 1
            self._state(db, state)
            self._page(db, page)
        return kept

    def mark_gone(self, table: str, ids: Iterable[int]) -> int:
        """Mark posts, media or comments the API no longer has; they are kept. Answer how many were marked."""
        if table not in {"posts", "media", "comments"}:
            message = f"no such table: {table}"
            raise ValueError(message)
        marked = 0
        with self.transaction() as db:
            for ident in ids:
                marked += db.execute(
                    f"UPDATE {table} SET gone_at = ? WHERE id = ? AND gone_at IS NULL",  # noqa: S608 - the table is one of three names above
                    (now_text(), ident),
                ).rowcount
        return marked

    @staticmethod
    def _page(db: sqlite3.Connection, page: PageRead | None) -> None:
        if page is not None:
            db.execute(
                "INSERT OR REPLACE INTO download_pages (collection, month, page, total_pages, items, read_at) VALUES (?, ?, ?, ?, ?, ?)",
                (page.collection, page.month, page.page, page.total_pages, page.items, now_text()),
            )

    def downloaded(self) -> dict[tuple[str, str], tuple[int, set[int]]]:
        """Answer the full download's pages read: for each collection and month, its pages in all and those read."""
        found: dict[tuple[str, str], tuple[int, set[int]]] = {}
        for row in self.db.execute("SELECT collection, month, page, total_pages FROM download_pages"):
            key = (row["collection"], row["month"])
            total, pages = found.get(key, (0, set()))
            pages.add(row["page"])
            found[key] = (max(total, row["total_pages"]), pages)
        return found

    @staticmethod
    def _state(db: sqlite3.Connection, state: tuple[str, Any] | None) -> None:
        if state is not None:
            db.execute(
                "INSERT INTO sync_state (name, value, updated_at) VALUES (?, ?, ?) "
                "ON CONFLICT (name) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                (state[0], json.dumps(state[1]), now_text()),
            )

    def set_state(self, name: str, value: object) -> None:
        """Keep a reading's place."""
        with self.transaction() as db:
            self._state(db, (name, value))

    def state(self, name: str, default: object = None) -> Any:  # noqa: ANN401 - a place is any JSON
        """Answer a reading's place, or `default`."""
        row = self.db.execute("SELECT value FROM sync_state WHERE name = ?", (name,)).fetchone()
        return default if row is None else json.loads(row["value"])

    def log_fetch(self, fetched: Fetched) -> None:
        """Keep a request made, and its answer's headers."""
        with self.transaction() as db:
            db.execute(
                "INSERT INTO fetches (at, kind, url, status, wire_bytes, body_bytes, elapsed_ms, headers) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    now_text(),
                    fetched.kind,
                    fetched.url,
                    fetched.status,
                    fetched.wire_bytes,
                    fetched.body_bytes,
                    fetched.elapsed_ms,
                    json.dumps(fetched.headers),
                ),
            )

    # What the sync asks.

    def modified(self, table: str, ids: Iterable[int]) -> dict[int, str]:
        """Answer the kept modification time of each of `ids` that is kept and not gone."""
        if table not in {"posts", "media"}:
            message = f"no such table: {table}"
            raise ValueError(message)
        wanted = json.dumps(list(ids))
        rows = self.db.execute(
            f"SELECT id, modified_gmt FROM {table} WHERE gone_at IS NULL AND id IN (SELECT value FROM json_each(?))",  # noqa: S608 - one of two names above
            (wanted,),
        )
        return {row["id"]: row["modified_gmt"] for row in rows}

    def missing(self, table: str, ids: Iterable[int]) -> list[int]:
        """Answer which of `ids` the table does not hold."""
        if table not in {"terms", "users", "media", "posts"}:
            message = f"no such table: {table}"
            raise ValueError(message)
        wanted = json.dumps(sorted({ident for ident in ids if ident}))
        rows = self.db.execute(
            f"SELECT value FROM json_each(?) WHERE value NOT IN (SELECT id FROM {table})",  # noqa: S608 - one of four names above
            (wanted,),
        )
        return [int(row[0]) for row in rows]

    def latest_time(self, table: str, column: str) -> str | None:
        """Answer the latest of a table's times: posts' or media's `modified_gmt`, comments' `date_gmt`."""
        if (table, column) not in {("posts", "modified_gmt"), ("media", "modified_gmt"), ("comments", "date_gmt")}:
            message = f"no such time: {table}.{column}"
            raise ValueError(message)
        row = self.db.execute(f"SELECT max({column}) FROM {table}").fetchone()  # noqa: S608 - one of three pairs above
        return None if row[0] is None else str(row[0])

    def comment_ids(self, post: int) -> set[int]:
        """Answer the ids of a post's kept comments that are not gone."""
        return {row["id"] for row in self.db.execute("SELECT id FROM comments WHERE post = ? AND gone_at IS NULL", (post,))}

    def post_ids_between(self, first: str, last: str) -> set[int]:
        """Answer the ids of posts dated `first` to `last` that are not gone."""
        rows = self.db.execute("SELECT id FROM posts WHERE type = 'post' AND gone_at IS NULL AND date_gmt BETWEEN ? AND ?", (first, last))
        return {row["id"] for row in rows}

    def post_ids_from(self, first: int, last: int) -> set[int]:
        """Answer the ids of posts, not pages, from `first` to `last` that are not gone."""
        rows = self.db.execute("SELECT id FROM posts WHERE type = 'post' AND gone_at IS NULL AND id BETWEEN ? AND ?", (first, last))
        return {row["id"] for row in rows}

    def picture_page(self, url: str) -> str | None:
        """Answer the teletext page made from a picture, if one was."""
        row = self.db.execute("SELECT tti FROM picture_pages WHERE url = ?", (url,)).fetchone()
        return None if row is None else str(row["tti"])

    def put_picture(self, url: str, headers: dict[str, str], body: bytes, level: str, tti: str) -> None:
        """Keep a picture's file and the page made from it."""
        with self.transaction() as db:
            db.execute(
                "INSERT INTO pictures (url, headers, body) VALUES (?, ?, ?) "
                "ON CONFLICT (url) DO UPDATE SET headers = excluded.headers, body = excluded.body, fetched_at = excluded.fetched_at",
                (url, json.dumps(headers), body),
            )
            db.execute(
                "INSERT INTO picture_pages (url, level, tti) VALUES (?, ?, ?) "
                "ON CONFLICT (url) DO UPDATE SET level = excluded.level, tti = excluded.tti, made_at = excluded.made_at",
                (url, level, tti),
            )

    # What the channel reads.

    def _posts(self, where: str, parameters: tuple[object, ...], order: str, limit: int, offset: int = 0) -> list[StoredPost]:
        # `where` and `order` are this module's own text; every value is bound.
        chosen = f"WHERE p.type = 'post' AND p.gone_at IS NULL AND {where} ORDER BY {order} LIMIT ? OFFSET ?"
        rows = self.db.execute(
            "SELECT p.id, p.data, coalesce(u.name, '') AS author, "
            "coalesce(nullif(json_extract(p.data, '$.jetpack_featured_media_url'), ''), m.source_url, '') AS picture "
            "FROM posts p LEFT JOIN users u ON u.id = p.author LEFT JOIN media m ON m.id = p.featured_media " + chosen,
            (*parameters, limit, offset),
        ).fetchall()
        ids = json.dumps([row["id"] for row in rows])
        terms: dict[int, list[tuple[str, str]]] = {}
        for term in self.db.execute(
            "SELECT pt.post, t.taxonomy, t.name FROM post_terms pt JOIN terms t ON t.id = pt.term "
            "WHERE pt.post IN (SELECT value FROM json_each(?)) ORDER BY t.taxonomy, t.name",
            (ids,),
        ):
            terms.setdefault(term["post"], []).append((term["taxonomy"], term["name"]))
        return [StoredPost(json.loads(row["data"]), row["author"], terms.get(row["id"], []), row["picture"]) for row in rows]

    def latest(self, count: int) -> list[StoredPost]:
        """Answer the newest `count` posts, newest first."""
        return self._posts("1", (), "p.date_gmt DESC", count)

    def first(self, count: int) -> list[StoredPost]:
        """Answer the archive's first `count` posts."""
        return self._posts("1", (), "p.date_gmt ASC", count)

    def by_ids(self, ids: list[int]) -> list[StoredPost]:
        """Answer the posts of `ids`, newest first."""
        return self._posts("p.id IN (SELECT value FROM json_each(?))", (json.dumps(ids),), "p.date_gmt DESC", len(ids))

    def on_this_day(self, month_day: str, before_year: str) -> list[StoredPost]:
        """Answer the latest post of `month_day` (`10-25`) in each year before `before_year`, newest year first."""
        rows = self.db.execute(
            "SELECT id FROM (SELECT id, date_gmt, row_number() OVER (PARTITION BY substr(date_gmt, 1, 4) ORDER BY date_gmt DESC) AS place "
            "FROM posts WHERE type = 'post' AND gone_at IS NULL AND substr(date_gmt, 6, 5) = ? AND date_gmt < ?) "
            "WHERE place = 1 ORDER BY date_gmt DESC",
            (month_day, before_year),
        )
        return self.by_ids([row["id"] for row in rows])

    def search(self, typed: str, count: int) -> list[StoredPost]:
        """Answer the posts that have every word typed, the best first: a title's words count most."""
        query = search_words(typed)
        if query is None:
            return []
        rows = self.db.execute(
            "SELECT rowid FROM posts_search WHERE posts_search MATCH ? ORDER BY bm25(posts_search, ?, ?) LIMIT ?",
            (query, TITLE_WEIGHT, TEXT_WEIGHT, count),
        )
        ranked = [row[0] for row in rows]
        found = {post.data["id"]: post for post in self.by_ids(ranked)}
        return [found[ident] for ident in ranked if ident in found]

    def latest_titled(self, start: str) -> StoredPost | None:
        """Answer the newest post whose title starts with `start`."""
        escaped = start.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
        found = self._posts("p.title LIKE ? ESCAPE '\\'", (f"{escaped}%",), "p.date_gmt DESC", 1)
        return found[0] if found else None

    def tagged(self, slug: str, count: int) -> list[StoredPost]:
        """Answer the newest `count` posts with a tag, by its slug."""
        return self._posts(
            "p.id IN (SELECT pt.post FROM post_terms pt JOIN terms t ON t.id = pt.term WHERE t.taxonomy = 'post_tag' AND t.slug = ?)",
            (slug,),
            "p.date_gmt DESC",
            count,
        )

    def by_author(self, author: int, count: int) -> tuple[int, list[StoredPost]]:
        """Answer how many posts a writer has, and their newest `count`."""
        row = self.db.execute("SELECT count(*) FROM posts WHERE type = 'post' AND gone_at IS NULL AND author = ?", (author,)).fetchone()
        return int(row[0]), self._posts("p.author = ?", (author,), "p.date_gmt DESC", count)

    def post_count(self) -> int:
        """Answer how many posts are kept and not gone."""
        return int(self.db.execute("SELECT count(*) FROM posts WHERE type = 'post' AND gone_at IS NULL").fetchone()[0])

    def post_at(self, offset: int) -> StoredPost | None:
        """Answer the post at `offset` in date order."""
        found = self._posts("1", (), "p.date_gmt", 1, offset)
        return found[0] if found else None

    def comments_of(self, posts: list[int]) -> dict[int, list[dict[str, Any]]]:
        """Answer the public comments of each of `posts`, oldest first."""
        found: dict[int, list[dict[str, Any]]] = {}
        for row in self.db.execute(
            "SELECT post, data FROM comments WHERE post IN (SELECT value FROM json_each(?)) AND gone_at IS NULL AND status = ? "
            "ORDER BY date_gmt, id",
            (json.dumps(posts), APPROVED),
        ):
            found.setdefault(row["post"], []).append(json.loads(row["data"]))
        return found

    def counts(self) -> dict[str, int]:
        """Answer how many of each kind are kept."""
        return {
            table: int(self.db.execute(f"SELECT count(*) FROM {table}").fetchone()[0])  # noqa: S608 - fixed names
            for table in ("posts", "comments", "terms", "users", "media", "pictures")
        }

    def fetched_since(self, since: str) -> list[tuple[str, int, int]]:
        """Answer, for each kind of request since `since`, how many and their bytes on the wire."""
        rows = self.db.execute(
            "SELECT kind, count(*), coalesce(sum(wire_bytes), 0) FROM fetches WHERE at >= ? GROUP BY kind ORDER BY 3 DESC", (since,)
        )
        return [(row[0], int(row[1]), int(row[2])) for row in rows]
