"""The Hackaday channel without the network: reading posts, numbering pages, drawing them, and what viewers do."""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING, Any

import pytest
from hackaday import channel as channel_module
from hackaday import numbers, site, views
from hackaday.channel import POLL_OPTIONS, SPLASH_S, Hackaday, Viewer
from hackaday.layout import JOLLY_WRENCHER, JOLLY_WRENCHER_LARGE, headline, last_top, thumb, wrap
from hackaday.posts import Comment, Post, comment_of, post_of, threaded
from neoteletext_app.screen import Colour

if TYPE_CHECKING:
    from pathlib import Path

NOW = datetime(2026, 10, 25, 12, 0, tzinfo=UTC)


def raw_post(number: int, *, category: str = "Retrocomputing", tags: tuple[str, ...] = ("doom",), hours_ago: int = 0) -> dict[str, Any]:
    """Answer a post as the REST API gives it with `_embed`."""
    date = (NOW - timedelta(hours=hours_ago)).replace(tzinfo=None).isoformat()
    return {
        "id": number,
        "date_gmt": date,
        "title": {"rendered": f"Hack number {number} &#8211; with a &#8220;title&#8221;"},
        "link": f"https://hackaday.com/2026/10/25/hack-{number}/",
        "content": {
            "rendered": (
                "<p>First paragraph with <a href='https://example.com/x'>a link</a>.</p>"
                "<h2>A heading</h2><ul><li>An item</li></ul>"
                "<iframe src='https://www.youtube.com/embed/abc'></iframe><script>ignored()</script><p>Last words.</p>"
            )
        },
        "excerpt": {"rendered": "<p>The excerpt &hellip;read more</p>"},
        "_embedded": {
            "author": [{"name": "Writer A"}],
            "wp:term": [
                [{"taxonomy": "category", "name": category}, {"taxonomy": "category", "name": "Slider"}],
                [{"taxonomy": "post_tag", "name": tag} for tag in tags],
            ],
            "wp:featuredmedia": [{"source_url": f"https://hackaday.com/wp-content/uploads/{number}.jpg"}],
        },
    }


def posts(count: int = 12) -> list[Post]:
    """Answer `count` posts, newest first, in turns of two sections."""
    made = [
        post_of(raw_post(number, category="Radio Hacks" if number % 2 else "Retrocomputing", hours_ago=number)) for number in range(count)
    ]
    return [post for post in made if post is not None]


def test_a_post_reads_with_its_terms_text_links_and_videos() -> None:
    post = post_of(raw_post(7, tags=("doom", "Does It Run Doom")))
    assert post is not None
    assert post.title == "Hack number 7 \u2013 with a \u201ctitle\u201d", "entities decoded; the page folds them when drawn"
    assert post.categories == ["Retrocomputing"], "the site's own Slider category is dropped"
    assert post.tags == ["doom", "Does It Run Doom"]
    assert [block.kind for block in post.article.blocks] == ["text", "heading", "item", "text"]
    assert post.article.links == [("a link", "https://example.com/x")]
    assert post.article.videos == ["https://www.youtube.com/embed/abc"]
    assert "ignored" not in " ".join(block.text for block in post.article.blocks)
    assert post.sections == ["Retrocomputing"]
    assert post.has("DOES IT RUN DOOM")


def test_what_is_not_a_post_is_refused() -> None:
    assert post_of({"id": 1}) is None
    assert comment_of({"content": {}}) is None


def test_comments_thread_replies_under_what_they_answer() -> None:
    def comment(number: int, parent: int, minutes: int) -> Comment:
        return Comment(number, parent, f"person {number}", NOW + timedelta(minutes=minutes), "words")

    ordered = threaded([comment(1, 0, 0), comment(2, 0, 1), comment(3, 1, 2), comment(4, 3, 3), comment(5, 5, 4)])
    assert [(depth, item.id) for depth, item in ordered] == [(0, 1), (1, 3), (2, 4), (0, 2)], "a comment answering itself is not shown"


def test_pages_are_numbered_in_decimal_digits() -> None:
    assert numbers.page(2, 10) == 0x210
    assert numbers.page(8, 99) == 0x899
    assert numbers.decimal(0x342) == (3, 42)
    assert numbers.decimal(0x2A0) is None, "a page a remote cannot type is none of ours"
    assert numbers.is_story(0x210)
    assert numbers.is_story(0x160)
    assert not numbers.is_story(0x209), "N00 to N09 are the magazine's own pages"
    assert not numbers.is_story(0x150)


def test_each_post_goes_to_the_magazine_its_labels_name_first() -> None:
    held = posts()
    placed = site.place(held)
    assert placed.page_of(held[0]) == 0x210, "Retrocomputing, 2xx, newest first"
    assert placed.page_of(held[1]) == 0x510, "Radio, 5xx"
    assert placed.page_of(held[2]) == 0x211
    assert placed.post_at(0x511) is held[3]
    assert placed.post_at(0x299) is None
    column = post_of(raw_post(99, category="Hackaday Columns", tags=()))
    assert column is not None
    assert site.magazines_of(column)[0] is site.HOME, "a column that names no subject is magazine 1's"
    both = post_of(raw_post(98, category="Security Hacks", tags=("retrocomputing",)))
    assert both is not None
    assert [mag.number for mag in site.magazines_of(both)] == [2, 7], "the Challenge's subject comes first"
    nothing = post_of(raw_post(97, category="Slider", tags=()))
    assert nothing is not None
    assert site.magazines_of(nothing)[0].number == site.CATCH_ALL
    assert placed.tag_pages(2) == [(0x204, "doom", 6)], "a magazine's busiest tag has its page"


def test_text_wraps_and_the_scroll_bar_fits() -> None:
    assert wrap("one two three four", 9) == ["one two", "three", "four"]
    assert wrap("abcdefghijkl", 5) == ["abcde", "fghij", "kl"]
    assert thumb(0, 100, 14, 14) == (0, 2)
    start, length = thumb(last_top(100, 14), 100, 14, 14)
    assert start + length == 14, "the block reaches the bottom at the end"
    for bitmap, size in ((JOLLY_WRENCHER, (26, 21)), (JOLLY_WRENCHER_LARGE, (70, 63))):
        assert {len(line) for line in bitmap} == {size[0]}
        assert len(bitmap) == size[1]


def test_a_headline_takes_two_lines_and_is_cut_only_past_them() -> None:
    assert headline("A short one", 20) == ["A short one"]
    assert headline("A title that takes two lines", 20) == ["A title that takes", "two lines"], "two lines, whole"
    long = headline("A title of a post that runs to well past two lines of twenty", 20)
    assert len(long) == 2
    assert long[1].endswith("..."), long
    assert all(len(line) <= 20 for line in long), long
    assert long[0] == "A title of a post", "the first line whole"


def test_every_page_draws_in_the_page() -> None:
    held = posts()
    placed = site.place(held)
    retro = site.magazine(2)
    screens = [
        views.front(held, placed, NOW, 3, NOW + timedelta(days=2)),
        views.splash(),
        views.headlines(held, placed, 0),
        views.magazines_index(placed),
        views.popular_tags(held, placed),
        views.columns_index(held, placed),
        views.magazine_front(retro, placed, "Retrocomputing Challenge"),
        views.magazine_stories(retro, placed, 0),
        views.tag_page(retro, "doom", held, placed, 0),
        views.story(held[0], retro, 0x210, 0x211, 0),
        views.picture(held[0], retro, 0x210, 0x211, None),
        views.comments(held[0], retro, 0x210, 0x211, 0),
        views.links_page(held[0], retro, 0x210, 0x211, None, "The full story"),
        views.reader(held[0], 0),
        views.doom(held, NOW),
        views.statistics(held),
        views.writers(held),
        views.discussed(held, placed),
        views.comments_wall(held, placed, 0),
        views.challenge(held, NOW, NOW + timedelta(days=2)),
        views.then_and_now(held, held),
        views.about(),
        views.quiz(held[0], ["Radio", "Retrocomputing", "Making", "Hardware"], None, (1, 2), 3),
        views.poll(POLL_OPTIONS, [1] * len(POLL_OPTIONS), 2, 0),
        views.mine({5}, 1, held, placed),
        views.search("doom", held, 0, searching=False),
    ]
    for screen in screens:
        assert len(screen.rows) == 25
        assert all(len(row) == 40 and all(code < 0x80 for code in row) for row in screen.rows)
        assert screen.tti(0x100).startswith("PN,10000\n")
    front = screens[0]
    assert "H A C K A D A Y" in front.row_text(1), "double width, a letter to every other cell"
    assert "NEWSFLASH" in front.row_text(views.NEWSFLASH_ROW), "the newest post is news for a while"
    assert "200 Retro" in front.row_text(14), "the magazines, each with its page"
    assert "510" in front.row_text(18), "the latest, each with its page"


class Recorder(Hackaday):
    """The channel, its lines kept and its library filled by hand."""

    def __init__(self, cache: Path) -> None:
        super().__init__(cache)
        self.sent: list[dict[str, Any]] = []
        self.library.posts = posts()

    def send(self, message: dict[str, Any]) -> None:
        self.sent.append(message)

    def spawn(self, work: object) -> None:
        """Run nothing in the background: the tests drive what happens."""
        if asyncio.iscoroutine(work):
            work.close()


def drive(channel: Hackaday, *messages: dict[str, Any]) -> None:
    async def run() -> None:
        for message in messages:
            await channel.receive(message)

    asyncio.run(run())


def key(session: int, page: str, name: str) -> dict[str, Any]:
    return {"type": "input", "session": session, "page": page, "event": {"kind": "key", "action": "press", "key": name}}


def published(channel: Recorder, page: int, owner: object) -> list[str]:
    return [
        message["tti"]
        for message in channel.sent
        if message["type"] == "publish" and message["page"] == f"{page:03X}" and message["owner"] == owner
    ]


@pytest.fixture
def channel(tmp_path: Path) -> Recorder:
    made = Recorder(tmp_path / "cache")
    made.publish_all()
    return made


def test_everything_is_published_once_and_unchanged_pages_not_again(channel: Recorder) -> None:
    first = len(channel.sent)
    assert published(channel, 0x210, "channel"), "the newest Retrocomputing story"
    assert published(channel, 0x299, "channel") == [], "only held stories"
    channel.publish_all()
    assert len(channel.sent) == first, "nothing changed, nothing sent"


def test_a_story_scrolls_for_its_reader_alone(channel: Recorder) -> None:
    channel.library.posts[0].article.blocks *= 10
    drive(channel, {"type": "open", "session": 1, "page": "210"}, key(1, "210", "PageDown"))
    own = published(channel, 0x210, 1)
    assert len(own) == 1
    assert "Lines 14-27" in own[0]
    drive(channel, key(1, "210", "End"), key(1, "210", "PageDown"))
    assert len(published(channel, 0x210, 1)) == 2, "no further than the end"


def mouse(session: int, page: str, action: str, column: int, row: int, **more: Any) -> dict[str, Any]:  # noqa: ANN401 - the event's other fields
    return {
        "type": "input",
        "session": session,
        "page": page,
        "event": {"kind": "mouse", "action": action, "column": column, "row": row, **more},
    }


def test_the_scroll_bar_is_pulled_and_pages_on_either_side(channel: Recorder) -> None:
    channel.library.posts[0].article.blocks *= 10
    channel.publish_all()
    total = len(views.article_lines(channel.posts[0]))
    assert "IM,16" in published(channel, 0x210, "channel")[-1], "a page with a scroll bar asks for drags"
    first, rows = views.STORY_TEXT_ROW, views.STORY_ROWS
    _, length = thumb(0, total, rows, rows)
    drive(
        channel,
        mouse(1, "210", "press", 39, first, pressed=True, button=0),
        mouse(1, "210", "move", 39, first + rows - length, buttons=1),
        mouse(1, "210", "release", 39, first + rows - length, pressed=False, button=0),
    )
    assert channel.viewers[1].tops[0x210] == last_top(total, rows), "the block dragged to the bottom shows the end"
    assert not channel.viewers[1].grips, "let go on release"
    drive(channel, mouse(1, "210", "move", 39, first, buttons=1))
    assert channel.viewers[1].tops[0x210] == last_top(total, rows), "a move with nothing gripped does nothing"
    drive(channel, mouse(1, "210", "press", 39, first, pressed=True, button=0))
    assert channel.viewers[1].tops[0x210] == last_top(total, rows) - (rows - 1), "a press above the block turns a screen back"


def test_a_storys_keys_and_prompts_show_its_views_and_the_next(channel: Recorder) -> None:
    drive(channel, {"type": "open", "session": 2, "page": "210"}, key(2, "210", "F1"))
    assert channel.viewers[2].views[0x210] == views.PICTURE, "red, the text's first prompt, is its picture"
    assert published(channel, 0x210, 2), "the picture is the viewer's own copy"
    drive(channel, key(2, "210", "F1"))
    assert 0x210 not in channel.viewers[2].views, "red on the picture is the text"
    assert [message for message in channel.sent if message["type"] == "release"] == [{"type": "release", "session": 2, "page": "210"}], (
        "the text is the channel's copy again"
    )
    drive(channel, key(2, "210", "ColorF1Green"))
    assert channel.viewers[2].views[0x210] == views.COMMENTS, "green on the text is its comments"
    drive(channel, key(2, "210", "F3"))
    assert channel.viewers[2].views[0x210] == views.LINKS, "yellow on the comments is its links"
    shown = channel.shown(2, 0x210)
    assert shown is not None
    cyan = next(column for column, code in enumerate(shown.rows[24]) if code == Colour.CYAN.text)
    drive(channel, mouse(2, "210", "press", cyan + 1, 24, pressed=True, button=0))
    assert [message["page"] for message in channel.sent if message["type"] == "goto"] == ["211"], (
        "the cyan prompt, clicked, is the next story"
    )
    drive(channel, {"type": "leave", "session": 2, "page": "210"})
    assert 0x210 not in channel.viewers[2].views, "one who leaves finds the text when they come back"


def test_story_pages_are_titled_with_their_posts(channel: Recorder) -> None:
    titles = {message["page"]: message["title"] for message in channel.sent if message["type"] == "title"}
    assert titles["210"] == 'Hack number 0 - with a "title"'
    long = "A title of a post that runs to well past what the contents holds"
    channel.library.posts[1].title = long
    channel.publish_all()
    titles = {message["page"]: message["title"] for message in channel.sent if message["type"] == "title"}
    assert titles["510"] == long[:29].rstrip() + "...", titles["510"]
    assert len(titles["510"]) <= 32
    assert titles["204"] == "doom (6)", "a tag page is titled by its tag"
    assert "205" not in titles, "a tag page without a tag keeps the title it was declared with"
    assert published(channel, 0x205, "channel") == [], "and is not published, so no list shows it"


def test_the_magazines_are_named_in_the_contents(channel: Recorder) -> None:
    declared = channel.declaration()["pages"]
    sections = {entry["page"]: entry["section"] for entry in declared if "section" in entry}
    assert sections == {f"{mag.front:03X}": mag.name for mag in site.SUBJECTS}
    assert len(declared) <= 800, "the server's most"
    assert all(len(entry["title"]) <= 32 for entry in declared)


def test_a_viewer_arriving_sees_the_emblem_for_two_seconds(channel: Recorder) -> None:
    async def arrive() -> None:
        await channel.receive({"type": "open", "session": 7, "page": "100"})
        assert published(channel, 0x100, 7) == [views.splash().tti(0x100)], "the whole-page emblem, theirs alone"
        await asyncio.sleep(SPLASH_S + 0.1)
        await channel.receive({"type": "open", "session": 7, "page": "100"})

    asyncio.run(arrive())
    assert [message for message in channel.sent if message["type"] == "release"] == [{"type": "release", "session": 7, "page": "100"}]
    assert len(published(channel, 0x100, 7)) == 1, "once a visit"


def test_the_roulette_wheel_turns_then_lands_on_its_post(channel: Recorder, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(channel_module, "SPIN_STEPS_S", (0.0, 0.0, 0.0))
    landed = channel.posts[3]

    async def random_post(_generator: object) -> Post:
        return landed

    monkeypatch.setattr(channel.library, "random_post", random_post)
    drive(channel, {"type": "open", "session": 4, "page": "131"})
    asyncio.run(channel.spin(4))
    frames = published(channel, 0x131, 4)
    assert len(frames) == 4, "a frame a step, then the post"
    assert len(set(frames[:3])) == 3, "the wheel moves on each step"
    assert "SPIN" in frames[0]
    assert channel.viewers[4].roulette is landed
    assert "SPIN" not in frames[-1]


def test_a_listing_places_items_of_one_or_two_rows() -> None:
    laid = views.listing(["one", "a title long enough to need a second line of its own here", "three", "four"], 5, 8)
    assert [(index, row, len(lines)) for index, row, lines in laid.shown(0)] == [(0, 5, 1), (1, 6, 2), (2, 8, 1)]
    assert laid.item_at(0, 7) == 1, "either row of an item is it"
    assert laid.item_at(0, 9) is None
    assert laid.keeping(0, 3) == 1, "scrolled no further than to bring the chosen into view"
    assert laid.last_top() == 1, "from item 1 the last item shows"


def test_every_list_is_worked_with_keys_wheel_clicks_and_its_bar(channel: Recorder) -> None:
    channel.on_this_day = posts(30)
    channel.publish_on_this_day()
    shared = published(channel, 0x130, "channel")[-1]
    assert "IM,16" in shared, "a list longer than its page has a bar to pull"
    laid = views.choices_listing(channel.on_this_day_items(), channel.on_this_day_intro())
    drive(
        channel,
        {"type": "open", "session": 8, "page": "130"},
        {"type": "input", "session": 8, "page": "130", "event": {"kind": "wheel", "dx": 0, "dy": 240}},
    )
    assert channel.viewers[8].tops[0x130] == 2, "two notches, two items on"
    assert channel.viewers[8].chosen.get(0x130, 0) == 0, "the wheel moves the list, not the choice"
    _, row, _ = laid.shown(2)[1]
    drive(channel, mouse(8, "130", "press", 5, row, button=0))
    assert channel.viewers[8].chosen[0x130] == 3, "a click chooses the item under it"
    held = channel.on_this_day[3]
    gotos = [message["page"] for message in channel.sent if message["type"] == "goto"]
    assert gotos == [f"{channel.placed.page_of(held):03X}"] if channel.placed.page_of(held) else gotos == ["133"], "and opens it"
    drive(channel, mouse(8, "130", "press", 39, laid.first_row + laid.rows - 1, button=0))
    assert channel.viewers[8].tops[0x130] > 2, "a press under the bar's block turns a screen"


def test_the_poll_counts_one_vote_each(channel: Recorder) -> None:
    drive(channel, key(1, "141", "ArrowDown"), key(1, "141", "Enter"), key(1, "141", "Enter"), key(2, "141", "Enter"))
    assert channel.votes[:2] == [1, 1]
    drive(channel, {"type": "close", "session": 1})
    assert channel.votes[:2] == [1, 0], "a viewer who goes takes their vote"


def test_the_quiz_scores_and_sounds(channel: Recorder) -> None:
    drive(channel, {"type": "open", "session": 3, "page": "140"})
    viewer = channel.viewers[3]
    assert viewer.question is not None
    right = viewer.answers.index(viewer.question.sections[0])
    names = ("F1", "F2", "F3", "F4")
    drive(channel, key(3, "140", names[right]))
    assert viewer.score == (1, 1)
    sounds = [message for message in channel.sent if message["type"] == "sound"]
    assert {"object", "play"} <= {member for message in sounds for member in message if member in {"object", "play"}}


def test_search_takes_typing_but_colour_keys_still_leave(channel: Recorder) -> None:
    drive(
        channel,
        {"type": "input", "session": 4, "page": "105", "event": {"kind": "text", "text": "210"}},
        key(4, "105", "F1"),
    )
    assert channel.viewers[4].typed == "210", "digits typed into the search are not a page number"
    assert [message["page"] for message in channel.sent if message["type"] == "goto"] == ["100"]


def test_digits_select_a_page_on_a_page_that_does_not_take_them(channel: Recorder) -> None:
    drive(channel, *({"type": "input", "session": 5, "page": "107", "event": {"kind": "text", "text": digit}} for digit in "242"))
    assert [message["page"] for message in channel.sent if message["type"] == "goto"] == ["242"]


def test_my_hackaday_stars_magazines(channel: Recorder) -> None:
    drive(channel, key(6, "142", "ArrowDown"), key(6, "142", "Enter"))
    assert channel.viewers[6].favourites == {3}, "the second subject, Hardware"
    assert Viewer().favourites == set()


def test_the_servers_figures_show_on_page_150(channel: Recorder) -> None:
    stats = {
        "type": "stats",
        "server": {
            "uptime_s": 3_725,
            "bytes_sent": 3 * 1024 * 1024,
            "messages_sent": 900,
            "memory_kb": 20_480,
            "viewers": {"hackaday": 4, "showcase": 2},
        },
        "channel": {"viewers": 4, "viewers_peak": 9, "viewers_total": 31, "opens": 120, "keys": 77, "most_opened": {"100": 50, "242": 12}},
    }
    drive(channel, stats)
    screen = views.server_stats(stats, channel.pages)
    text = "\n".join(screen.row_text(row) for row in range(25))
    assert "Peak 9, 31 visits" in text
    assert "3.0 MB" in text
    assert "1h 02m, memory 20.0 MB" in text
    assert "242 Retro story 33" in text
    assert "showcase 2" in text
    assert published(channel, numbers.SERVER_STATS, "channel")[-1] == screen.tti(numbers.SERVER_STATS)


def test_podcast_grabs_play_from_the_sound_board(tmp_path: Path) -> None:
    import json  # noqa: PLC0415 - only this test writes a manifest
    import wave  # noqa: PLC0415

    sounds = tmp_path / "sounds"
    (sounds / "neoteletext").mkdir(parents=True)
    (sounds / "clips.json").write_text(
        json.dumps([{"name": "welcome", "title": "Welcome to the Hackaday Podcast", "episode": "392"}, {"name": "bad", "title": "Bad"}])
    )
    with wave.open(str(sounds / "neoteletext" / "welcome.wav"), "wb") as sound:
        sound.setnchannels(1)
        sound.setsampwidth(2)
        sound.setframerate(22_050)
        sound.writeframes(bytes(22_050 * 2))
    with wave.open(str(sounds / "neoteletext" / "bad.wav"), "wb") as sound:
        sound.setnchannels(2)
        sound.setsampwidth(2)
        sound.setframerate(22_050)
        sound.writeframes(bytes(400))
    made = Recorder(tmp_path / "cache")
    assert [clip.name for clip in made.clips] == ["welcome"], "a stereo clip is refused"
    assert made.clips[0].seconds == 1.0
    drive(made, key(9, "113", "Enter"), key(9, "113", "Enter"), {"type": "open", "session": 9, "page": "112"})
    sounds_sent = [message for message in made.sent if message["type"] == "sound"]
    assert [next(iter(set(message) & {"object", "play"})) for message in sounds_sent] == ["object", "play", "play", "play"], (
        "sent once, played thrice"
    )
    assert sounds_sent[-1]["play"]["page"] == "112", "the welcome belongs to the podcast page"


def test_each_person_has_a_page_and_says_hello(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    import json  # noqa: PLC0415 - only this test writes the manifests
    import wave  # noqa: PLC0415

    folder = tmp_path / "people"
    folder.mkdir()
    portrait = "PN,10000\nOL,2,\x1bW\x7f\x7f\nOL,26,@iAKA\nOL,24,not taken\n"
    (folder / "ada.tti").write_text(portrait, encoding="latin-1")
    people_list = [
        {
            "slug": "ada",
            "name": "Ada Example",
            "author": 1,
            "title": "Editor",
            "podcast": "Hosts",
            "bio": "Writes things.",
            "clip": "ada",
            "portrait": "ada.tti",
        },
        {
            "slug": "bo",
            "name": "Bo Example",
            "author": 2,
            "title": "Writer",
            "podcast": "Co-hosts",
            "bio": "",
            "clip": "bo",
            "portrait": None,
        },
    ]
    (folder / "people.json").write_text(json.dumps(people_list))
    sounds = tmp_path / "sounds"
    (sounds / "neoteletext").mkdir(parents=True)
    (sounds / "clips.json").write_text(json.dumps([{"name": "ada", "title": "I'm Ada"}]))
    with wave.open(str(sounds / "neoteletext" / "ada.wav"), "wb") as sound:
        sound.setnchannels(1)
        sound.setsampwidth(2)
        sound.setframerate(22_050)
        sound.writeframes(bytes(2_000))
    made = Recorder(tmp_path / "cache")
    made.publish_all()
    assert [person.name for person in made.people] == ["Ada Example", "Bo Example"]
    assert made.pages[0x121] == "Ada Example"
    first = views.profile(made.people[0], 0, 2)
    assert first.rows[2][:3] == bytes([0x17, 0x7F, 0x7F]), "the portrait's rows"
    assert first.packets == ["OL,26,@iAKA"], "its colours"
    assert "Ada Example" in first.row_text(17)
    assert "no photo" in views.profile(made.people[1], 1, 2).row_text(12), "the Jolly Wrencher stands in"
    monkeypatch.setattr(channel_module, "NAME_DELAY_S", 0.05)

    def plays() -> list[str]:
        return [message["play"]["page"] for message in made.sent if message["type"] == "sound" and "play" in message]

    async def visit(*pages: str, linger: float) -> None:
        for page in pages:
            await made.receive({"type": "open", "session": 3, "page": page})
        await asyncio.sleep(linger)

    asyncio.run(visit("121", linger=0.01))
    assert plays() == [], "the page shows first: the name comes after the delay"
    assert [message for message in made.sent if message["type"] == "sound" and "object" in message], "the sound is sent at once, ready"
    asyncio.run(visit("121", linger=0.2))
    assert plays() == ["121"], "then Ada says hello on her page"
    asyncio.run(visit("121", "122", linger=0.2))
    assert plays() == ["121"], "not once she is left behind; and Bo has no clip on this server"


def test_a_long_biography_ends_on_a_whole_sentence_or_a_mark() -> None:
    assert views.two_lines("Short.") == ["Short."]
    long_one = (
        "Tom was weaned on the flickering light of the MS-DOS prompt. "
        "But he went on and on and on about things for a very long while after that."
    )
    assert views.two_lines(long_one) == ["Tom was weaned on the flickering light", "of the MS-DOS prompt."]
    endless = " ".join(["word"] * 40)
    cut = views.two_lines(endless)
    assert len(cut) == 2
    assert cut[-1].endswith("..")
