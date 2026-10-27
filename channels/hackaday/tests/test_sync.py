"""The sync against a WordPress of its own: the whole archive read once, then only what changes."""

from __future__ import annotations

import asyncio
import math
import urllib.parse
from typing import TYPE_CHECKING, Any, cast

from hackaday.download import COLLECTIONS, MOST_TRIES, Download, gap
from hackaday.fetch import FetchError
from hackaday.store import Store
from hackaday.sync import PAGE_SIZE, WINDOW, Changes, Sync

if TYPE_CHECKING:
    from hackaday.fetch import Fetcher

#: Every poll the sync has: held back, so a test runs only what it means to.
POLLS = ("posts", "comments", "reconcile", "window", "media", "terms")


class FakeWordPress:
    """The REST API's collections and the filters the sync uses, in memory; each request kept."""

    def __init__(self) -> None:
        self.data: dict[str, list[dict[str, Any]]] = {name: [] for name, _, _ in COLLECTIONS}
        self.asked: list[tuple[str, str, dict[str, str]]] = []

    async def json(self, url: str, kind: str) -> tuple[Any, dict[str, str]]:
        parts = urllib.parse.urlsplit(url)
        collection = parts.path.removeprefix("/wp-json/wp/v2/")
        query = dict(urllib.parse.parse_qsl(parts.query))
        self.asked.append((kind, collection, query))
        items = list(self.data[collection])
        if "include" in query:
            wanted = {int(each) for each in query["include"].split(",")}
            items = [item for item in items if item["id"] in wanted]
        if "post" in query:
            items = [item for item in items if item["post"] == int(query["post"])]
        for name, field, keep in (("after", "date_gmt", 1), ("before", "date_gmt", -1), ("modified_after", "modified_gmt", 1)):
            if name in query:
                bound = query[name].removesuffix("Z")
                items = [item for item in items if (item[field] > bound if keep > 0 else item[field] < bound)]
        order = query.get("orderby", "date")
        key = {"id": "id", "date": "date_gmt", "date_gmt": "date_gmt", "modified": "modified_gmt"}.get(order, "id")
        items.sort(key=lambda item: (item.get(key, 0), item["id"]), reverse=query.get("order", "desc") == "desc")
        size = int(query.get("per_page", "10"))
        page = int(query.get("page", "1"))
        chosen = items[(page - 1) * size : page * size]
        if "_fields" in query:
            fields = query["_fields"].split(",")
            chosen = [{name: item[name] for name in fields if name in item} for item in chosen]
        return chosen, {"x-wp-total": str(len(items)), "x-wp-totalpages": str(math.ceil(len(items) / size))}


def post(ident: int, date: str, **more: Any) -> dict[str, Any]:  # noqa: ANN401 - any other field
    return {
        "id": ident,
        "type": "post",
        "status": "publish",
        "date_gmt": date,
        "modified_gmt": more.pop("modified_gmt", date),
        "link": f"https://hackaday.com/{ident}/",
        "title": {"rendered": f"Hack {ident}"},
        "content": {"rendered": "<p>Words.</p>"},
        "excerpt": {"rendered": ""},
        "author": 1,
        "featured_media": 0,
        "categories": [1],
        "tags": [],
        **more,
    }


def comment(ident: int, on: int, date: str) -> dict[str, Any]:
    return {
        "id": ident,
        "post": on,
        "parent": 0,
        "date_gmt": date,
        "status": "approved",
        "author_name": "Reader",
        "content": {"rendered": "Nice."},
    }


def archive() -> FakeWordPress:
    """Make a small Hackaday: posts across three months, with a post at each month's first and last second."""
    api = FakeWordPress()
    api.data["users"] = [{"id": 1, "name": "Ada", "slug": "ada"}]
    api.data["categories"] = [{"id": 1, "taxonomy": "category", "name": "Hacks", "slug": "hacks", "count": 3}]
    api.data["tags"] = [{"id": 100 + n, "taxonomy": "post_tag", "name": f"tag {n}", "slug": f"tag-{n}", "count": 1} for n in range(250)]
    dates = ["2005-01-01T00:00:00", "2005-01-31T23:59:59", "2005-02-01T00:00:00", "2005-03-15T12:00:00"]
    api.data["posts"] = [post(index + 1, date) for index, date in enumerate(dates)]
    api.data["posts"] += [post(1000 + n, f"2005-02-10T{n // 60:02d}:{n % 60:02d}:00") for n in range(230)]
    api.data["pages"] = [post(9000, "2005-01-05T00:00:00", type="page")]
    api.data["media"] = [
        {
            "id": 500,
            "date_gmt": "2005-02-01T00:00:00",
            "modified_gmt": "2005-02-01T00:00:00",
            "source_url": "https://hackaday.com/a.jpg",
            "post": 1,
        }
    ]
    api.data["comments"] = [comment(700 + n, 1000, f"2005-02-11T00:{n:02d}:00") for n in range(30)]
    return api


def syncing(api: FakeWordPress, store: Store) -> Sync:
    sync = Sync(store, cast("Fetcher", api))
    sync.due = dict.fromkeys(POLLS, math.inf)
    return sync


class Flaky(FakeWordPress):
    """A WordPress that refuses some requests: always, or once with a `Retry-After`."""

    def __init__(self, refuse: str, *, once: bool) -> None:
        super().__init__()
        self.refuse = refuse
        self.once = once
        self.refused = 0

    async def json(self, url: str, kind: str) -> tuple[Any, dict[str, str]]:
        if self.refuse in url and not (self.once and self.refused):
            self.refused += 1
            raise FetchError("503 from " + url, retry_after=0.01 if self.once else None)
        return await super().json(url, kind)


def download(api: FakeWordPress, store: Store) -> bool:
    return asyncio.run(Download(store, [cast("Fetcher", api)] * 3, within_s=0).run())


def test_the_whole_archive_is_read_by_many_at_once_without_paging_deep() -> None:
    api = archive()
    store = Store(":memory:")
    assert download(api, store)
    tables = ("posts", "terms", "users", "media", "comments")
    kept = {table: {row[0] for row in store.db.execute(f"SELECT id FROM {table}")} for table in tables}  # noqa: S608 - fixed names
    assert kept["posts"] == {item["id"] for item in (*api.data["posts"], *api.data["pages"])}, "every post and page, the months' edges too"
    assert kept["terms"] == {item["id"] for item in (*api.data["categories"], *api.data["tags"])}
    assert kept["users"] == {1}
    assert kept["media"] == {500}
    assert kept["comments"] == {item["id"] for item in api.data["comments"]}
    asked = [(collection, query) for kind, collection, query in api.asked if kind == "download"]
    pages = [query for _, query in asked]
    assert max(int(query["page"]) for query in pages) <= math.ceil(250 / PAGE_SIZE), "no deep paging: a month at a time"
    assert all(query.get("after", "Z").endswith("Z") for query in pages), "times asked with their zone"
    assert len(asked) == len({(collection, tuple(sorted(query.items()))) for collection, query in asked}), "no page read twice"
    assert store.state("backfill")["done"]


def test_a_download_stopped_goes_on_with_only_the_pages_it_had_not_read() -> None:
    store = Store(":memory:")
    failing = Flaky("tags?per_page=100&page=2", once=False)
    failing.data = archive().data
    assert not download(failing, store), "a page left unread"
    assert failing.refused == MOST_TRIES
    assert not store.state("backfill", {}).get("done")
    api = archive()
    assert download(api, store)
    read = [query for kind, collection, query in api.asked if kind == "download"]
    assert [(query["page"]) for query in read] == ["2"], "only the page it had not read"
    assert store.state("backfill")["done"]


def test_a_retry_after_holds_the_download_and_the_page_is_read() -> None:
    store = Store(":memory:")
    api = Flaky("comments?per_page=100&page=1", once=True)
    api.data = archive().data
    assert download(api, store)
    assert api.refused == 1
    assert store.comment_ids(1000) == {item["id"] for item in api.data["comments"]}


def test_the_pace_ends_by_the_deadline() -> None:
    assert gap(0.0, 10.0, 5) == 2.0
    assert gap(12.0, 10.0, 5) == 0.0, "late: as fast as the workers go"
    assert gap(0.0, 10.0, 0) == 10.0


def test_only_new_and_edited_posts_are_fetched_whole() -> None:
    api = archive()
    store = Store(":memory:")
    sync = syncing(api, store)
    changes = Changes()
    asyncio.run(sync.recent_posts(changes))
    whole = [query for kind, _, query in api.asked if kind == "posts"]
    assert sum(len(query["include"].split(",")) for query in whole) == min(WINDOW, len(api.data["posts"])), "the window, at first"
    api.asked.clear()
    asyncio.run(sync.recent_posts(Changes()))
    assert [kind for kind, _, _ in api.asked if kind == "posts"] == [], "nothing changed, nothing fetched whole"
    api.data["posts"][-1]["modified_gmt"] = "2026-10-25T00:00:00"
    api.data["posts"].append(post(5000, "2026-10-25T00:00:00"))
    api.asked.clear()
    changes = Changes()
    asyncio.run(sync.recent_posts(changes))
    fetched = {int(each) for kind, _, query in api.asked if kind == "posts" for each in query["include"].split(",")}
    assert fetched == {api.data["posts"][-2]["id"], 5000}, "the edited and the new, nothing else"
    assert "posts" in changes.kinds


def test_new_comments_come_from_the_whole_site_and_a_reread_finds_those_gone() -> None:
    api = archive()
    store = Store(":memory:")
    sync = syncing(api, store)
    asyncio.run(sync.recent_posts(Changes()))
    changes = Changes()
    asyncio.run(sync.recent_comments(changes))
    assert store.comment_ids(1000) == {item["id"] for item in api.data["comments"]}, "the window's comments, the first time"
    assert "comments" in changes.kinds
    api.data["comments"].append(comment(900, 1000, "2005-02-12T00:00:00"))
    asyncio.run(sync.recent_comments(Changes()))
    assert 900 in store.comment_ids(1000)
    api.data["comments"] = [item for item in api.data["comments"] if item["id"] != 700]
    window = sorted(store.post_ids_between(*sync.window_dates()), reverse=True)
    store.set_state("reconcile.place", window.index(1000))
    changes = Changes()
    asyncio.run(sync.reconcile_comments(changes))
    assert 700 not in store.comment_ids(1000), "marked gone"
    assert "comments" in changes.kinds


def test_posts_that_have_gone_are_found_by_the_window_and_by_the_sweep() -> None:
    api = archive()
    store = Store(":memory:")
    sync = syncing(api, store)
    assert download(api, store)
    api.data["posts"] = [item for item in api.data["posts"] if item["id"] not in {2, 1005}]
    asyncio.run(sync.check_window(Changes()))
    asyncio.run(sync.sweep_step(Changes()))
    asyncio.run(sync.sweep_step(Changes()))
    asyncio.run(sync.sweep_step(Changes()))
    gone = {row[0] for row in store.db.execute("SELECT id FROM posts WHERE gone_at IS NOT NULL")}
    assert gone == {2, 1005}
    assert not sync.sweeping(), "a sweep a week"


def test_a_step_with_nothing_due_says_so_and_asks_nothing() -> None:
    api = archive()
    sync = syncing(api, Store(":memory:"))
    changes = asyncio.run(sync.step())
    assert not changes.worked, "the channel waits rather than step again at once"
    assert api.asked == []
