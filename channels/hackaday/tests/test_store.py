"""The archive in SQLite: its schema, what it keeps and what the channel reads from it."""

from __future__ import annotations

import gzip
import sqlite3
from typing import TYPE_CHECKING, Any

import pytest
from hackaday.fetch import LARGEST_BODY, Fetcher, FetchError, unpacked
from hackaday.posts import post_of
from hackaday.store import Store, migrations, search_words

if TYPE_CHECKING:
    from pathlib import Path


def post(ident: int, date: str, *, title: str = "A hack", author: int = 1, tags: tuple[int, ...] = (), **more: Any) -> dict[str, Any]:  # noqa: ANN401 - any other field
    """Answer a post as the API gives it."""
    return {
        "id": ident,
        "type": "post",
        "status": "publish",
        "date_gmt": date,
        "modified_gmt": more.pop("modified_gmt", date),
        "slug": f"hack-{ident}",
        "link": f"https://hackaday.com/{ident}/",
        "title": {"rendered": title},
        "content": {"rendered": more.pop("content", "<p>Words about a hack.</p>")},
        "excerpt": {"rendered": ""},
        "author": author,
        "featured_media": more.pop("featured_media", 0),
        "categories": [1],
        "tags": list(tags),
        "jetpack_featured_media_url": more.pop("picture", ""),
        **more,
    }


def term(ident: int, taxonomy: str, name: str) -> dict[str, Any]:
    """Answer a category or tag as the API gives it."""
    return {"id": ident, "taxonomy": taxonomy, "name": name, "slug": name.lower().replace(" ", "-"), "count": 1, "link": ""}


@pytest.fixture
def store(tmp_path: Path) -> Store:
    made = Store(tmp_path / "archive.sqlite")
    made.put_objects("users", [{"id": 1, "name": "Ada Writer", "slug": "ada"}])
    made.put_objects("terms", [term(1, "category", "Retrocomputing"), term(10, "post_tag", "DOOM"), term(11, "post_tag", "Amiga")])
    return made


def test_every_table_is_strict_and_the_schema_is_at_the_latest_migration(store: Store, tmp_path: Path) -> None:
    tables = store.db.execute("PRAGMA table_list").fetchall()
    ours = [row for row in tables if row["type"] == "table" and not row["name"].startswith(("sqlite_", "posts_search"))]
    assert ours, "the schema made its tables"
    assert {row["name"] for row in ours if not row["strict"]} == set(), "STRICT, every one"
    assert store.db.execute("PRAGMA user_version").fetchone()[0] == len(migrations())
    store.close()
    again = Store(tmp_path / "archive.sqlite")
    assert again.counts()["users"] == 1, "opened again, nothing applied twice, nothing lost"


def test_a_post_is_kept_whole_and_its_columns_come_from_its_json(store: Store) -> None:
    changed = store.put_posts([post(5, "2026-10-25T10:00:00", tags=(10, 11), extra={"kept": True})])
    assert changed == [5]
    row = store.db.execute("SELECT author, date_gmt, type, json_extract(data, '$.extra.kept') AS kept FROM posts WHERE id = 5").fetchone()
    assert (row["author"], row["date_gmt"], row["type"], row["kept"]) == (1, "2026-10-25T10:00:00", "post", 1), "every field kept"
    terms = {row[0] for row in store.db.execute("SELECT term FROM post_terms WHERE post = 5")}
    assert terms == {1, 10, 11}, "its terms, from its JSON's arrays"
    assert store.put_posts([post(5, "2026-10-25T10:00:00", tags=(10, 11))]) == [], "unchanged: not news"
    assert store.put_posts([post(5, "2026-10-25T10:00:00", modified_gmt="2026-10-26T09:00:00")]) == [5], "edited"


def test_a_wrong_type_is_refused_and_nothing_of_the_batch_is_kept(store: Store) -> None:
    with pytest.raises(sqlite3.IntegrityError):
        store.put_posts([post(6, "2026-10-25T10:00:00"), post(7, "2026-10-25T11:00:00", author="seven")])  # type: ignore[arg-type]
    assert store.post_count() == 0, "the transaction undone"


def test_the_channel_reads_posts_with_their_names_and_pictures(store: Store) -> None:
    store.put_objects(
        "media",
        [{"id": 70, "date_gmt": "2026-01-01T00:00:00", "modified_gmt": "2026-01-01T00:00:00", "source_url": "https://hackaday.com/m.jpg"}],
    )
    store.put_posts(
        [
            post(1, "2005-10-25T09:00:00", title="The first &amp; best"),
            post(2, "2024-10-25T08:00:00", tags=(10,), picture="https://hackaday.com/j.jpg"),
            post(3, "2024-10-25T20:00:00", tags=(10,), featured_media=70),
            post(4, "2026-10-25T10:00:00", title="This Week in Security: 100%"),
            post(5, "2026-10-26T10:00:00", title="This Week in Security: later", author=2),
        ]
    )
    latest = store.latest(2)
    assert [each.data["id"] for each in latest] == [5, 4]
    made = post_of(store.by_ids([3])[0])
    assert made is not None
    assert made.author == "Ada Writer"
    assert made.categories == ["Retrocomputing"]
    assert made.tags == ["DOOM"]
    assert made.picture_url == "https://hackaday.com/m.jpg", "the media record's file when the post names none"
    assert store.by_ids([2])[0].picture_url == "https://hackaday.com/j.jpg"
    assert [each.data["id"] for each in store.on_this_day("10-25", "2026")] == [3, 1], "the latest of each past year"
    assert [each.data["id"] for each in store.tagged("doom", 5)] == [3, 2]
    assert [each.data["id"] for each in store.first(1)] == [1]
    count, theirs = store.by_author(1, 2)
    assert (count, [each.data["id"] for each in theirs]) == (4, [4, 3])
    found = store.latest_titled("This Week in Security: 1")
    assert found is not None
    assert found.data["id"] == 4
    assert store.latest_titled("100_") is None, "LIKE's own characters are only themselves"
    assert [each.data["id"] for each in store.search("first best", 5)] == [1]
    first = store.post_at(0)
    assert first is not None
    assert first.data["id"] == 1


def test_a_search_is_only_words(store: Store) -> None:
    store.put_posts([post(1, "2026-10-25T10:00:00", title="NEAR the toaster", content="<p>It runs DOOM.</p>")])
    assert search_words('NEAR(toaster "doom" OR *') == '"NEAR" "toaster" "doom" "OR"'
    assert search_words("  ") is None
    for typed in ('"', "NEAR(", "*", "a:b", "toaster AND"):
        store.search(typed, 5)
    assert [each.data["id"] for each in store.search("doom", 5)] == [1], "its text is searched too"
    assert [each.data["id"] for each in store.search("toasters", 5)] == [1], "words stemmed"


def test_what_has_gone_is_kept_but_not_shown(store: Store) -> None:
    store.put_posts([post(1, "2026-10-24T10:00:00"), post(2, "2026-10-25T10:00:00")])
    store.put_objects("comments", [{"id": 9, "post": 1, "parent": 0, "date_gmt": "2026-10-25T11:00:00", "status": "approved"}])
    assert store.mark_gone("posts", [2]) == 1
    assert store.mark_gone("comments", [9]) == 1
    assert [each.data["id"] for each in store.latest(5)] == [1]
    assert store.comments_of([1]) == {}
    assert store.counts()["posts"] == 2, "kept"
    assert store.put_posts([post(2, "2026-10-25T10:00:00")]) == [2], "back again is a change"


def test_a_gzipped_answer_is_unpacked_within_its_bound() -> None:
    assert unpacked(gzip.compress(b"hello"), "gzip") == b"hello"
    with pytest.raises(FetchError):
        unpacked(gzip.compress(bytes(LARGEST_BODY + 1)), "gzip")
    with pytest.raises(FetchError):
        unpacked(b"not gzip", "gzip")
    with pytest.raises(FetchError):
        unpacked(b"x", "br")


def test_only_hackaday_is_fetched() -> None:
    import asyncio  # noqa: PLC0415 - only this test runs a loop

    with pytest.raises(FetchError):
        asyncio.run(Fetcher().get("https://example.com/", "test"))
