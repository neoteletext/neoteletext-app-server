"""The Hackaday channel without the network: reading posts, numbering pages, drawing them, and what viewers do."""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING, Any

import pytest
from hackaday import numbers, views
from hackaday.channel import POLL_OPTIONS, Hackaday, Viewer
from hackaday.layout import JOLLY_WRENCHER, last_top, thumb, wrap
from hackaday.posts import Comment, Post, comment_of, post_of, threaded

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


def test_story_pages_are_numbered_in_decimal_digits() -> None:
    assert numbers.slot_page(numbers.STORY_MAGAZINE, 0) == 0x200
    assert numbers.slot_page(numbers.STORY_MAGAZINE, 42) == 0x242
    assert numbers.slot_page(numbers.LINKS_MAGAZINE, 99) == 0x599
    assert numbers.slot_of(0x342) == (numbers.PICTURE_MAGAZINE, 42)
    assert numbers.slot_of(0x2A0) is None, "a page a remote cannot type is no story"
    assert numbers.slot_of(0x100) is None
    assert [numbers.section_page(index) for index in (0, 9)] == [0x130, 0x139]


def test_text_wraps_and_the_scroll_bar_fits() -> None:
    assert wrap("one two three four", 9) == ["one two", "three", "four"]
    assert wrap("abcdefghijkl", 5) == ["abcde", "fghij", "kl"]
    assert thumb(0, 100, 14, 14) == (0, 2)
    start, length = thumb(last_top(100, 14), 100, 14, 14)
    assert start + length == 14, "the block reaches the bottom at the end"
    assert len({len(line) for line in JOLLY_WRENCHER}) == 1
    assert all(line == line[::-1] for line in JOLLY_WRENCHER), "the emblem is symmetric"


def test_every_page_draws_in_the_page() -> None:
    held = posts()
    screens = [
        views.front(held, NOW, 3),
        views.headlines(held, 0),
        views.sections_index(held),
        views.section(0, held, 0),
        views.story(held[0], 0, 0),
        views.picture(held[0], 0, None),
        views.comments(held[0], 0, 0),
        views.links_page(held[0], 0, None, "The full story"),
        views.reader(held[0], 0),
        views.doom(held, NOW),
        views.statistics(held),
        views.writers(held),
        views.discussed(held),
        views.comments_wall(held, 0),
        views.challenge(held, NOW, NOW + timedelta(days=2)),
        views.then_and_now(held, held),
        views.about(),
        views.quiz(held[0], ["Radio", "Retrocomputing", "Making", "Hardware"], None, (1, 2), 3),
        views.poll(POLL_OPTIONS, [1] * len(POLL_OPTIONS), 2, 0),
        views.mine(views.section_names(), {"Radio"}, 1, held),
        views.search("doom", held, 0, searching=False),
    ]
    for screen in screens:
        assert len(screen.rows) == 25
        assert all(len(row) == 40 and all(code < 0x80 for code in row) for row in screen.rows)
        assert screen.tti(0x100).startswith("PN,10000\n")
    front = screens[0]
    assert "HACKADAY" in front.row_text(1)
    assert "NEWSFLASH" in front.row_text(views.NEWSFLASH_ROW), "the newest post is news for a while"
    assert "201" in front.row_text(14)


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
    assert published(channel, 0x200, "channel"), "story 00"
    assert published(channel, 0x599, "channel") == [], "only held stories"
    channel.publish_all()
    assert len(channel.sent) == first, "nothing changed, nothing sent"


def test_a_story_scrolls_for_its_reader_alone(channel: Recorder) -> None:
    channel.library.posts[0].article.blocks *= 10
    drive(channel, {"type": "open", "session": 1, "page": "200"}, key(1, "200", "PageDown"))
    own = published(channel, 0x200, 1)
    assert len(own) == 1
    assert "Lines 14-27" in own[0]
    drive(channel, key(1, "200", "End"), key(1, "200", "PageDown"))
    assert len(published(channel, 0x200, 1)) == 2, "no further than the end"


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
    assert "IM,16" in published(channel, 0x200, "channel")[-1], "a page with a scroll bar asks for drags"
    first, rows = views.STORY_TEXT_ROW, views.STORY_ROWS
    _, length = thumb(0, total, rows, rows)
    drive(
        channel,
        mouse(1, "200", "press", 39, first, pressed=True, button=0),
        mouse(1, "200", "move", 39, first + rows - length, buttons=1),
        mouse(1, "200", "release", 39, first + rows - length, pressed=False, button=0),
    )
    assert channel.viewers[1].tops[0x200] == last_top(total, rows), "the block dragged to the bottom shows the end"
    assert not channel.viewers[1].grips, "let go on release"
    drive(channel, mouse(1, "200", "move", 39, first, buttons=1))
    assert channel.viewers[1].tops[0x200] == last_top(total, rows), "a move with nothing gripped does nothing"
    drive(channel, mouse(1, "200", "press", 39, first, pressed=True, button=0))
    assert channel.viewers[1].tops[0x200] == last_top(total, rows) - (rows - 1), "a press above the block turns a screen back"


def test_a_click_on_a_prompt_follows_its_link(channel: Recorder) -> None:
    channel.library.posts[0].article.blocks *= 10
    drive(channel, mouse(2, "200", "press", 1, 24, pressed=True, button=0))
    screen = channel.shown(2, 0x200)
    assert screen is not None
    assert [message["page"] for message in channel.sent if message["type"] == "goto"] == [f"{screen.fastext_links[0]:03X}"]


def test_story_pages_are_titled_with_their_posts(channel: Recorder) -> None:
    titles = {message["page"]: message["title"] for message in channel.sent if message["type"] == "title"}
    assert titles["200"] == 'Hack number 0 - with a "title"'
    long = "A title of a post that runs to well past what the contents holds"
    channel.library.posts[1].title = long
    channel.publish_all()
    titles = {message["page"]: message["title"] for message in channel.sent if message["type"] == "title"}
    assert titles["201"] == long[:29].rstrip() + "...", titles["201"]
    assert len(titles["201"]) <= 32


def test_the_poll_counts_one_vote_each(channel: Recorder) -> None:
    drive(channel, key(1, "116", "ArrowDown"), key(1, "116", "Enter"), key(1, "116", "Enter"), key(2, "116", "Enter"))
    assert channel.votes[:2] == [1, 1]
    drive(channel, {"type": "close", "session": 1})
    assert channel.votes[:2] == [1, 0], "a viewer who goes takes their vote"


def test_the_quiz_scores_and_sounds(channel: Recorder) -> None:
    drive(channel, {"type": "open", "session": 3, "page": "108"})
    viewer = channel.viewers[3]
    assert viewer.question is not None
    right = viewer.answers.index(viewer.question.sections[0])
    names = ("F1", "F2", "F3", "F4")
    drive(channel, key(3, "108", names[right]))
    assert viewer.score == (1, 1)
    sounds = [message for message in channel.sent if message["type"] == "sound"]
    assert {"object", "play"} <= {member for message in sounds for member in message if member in {"object", "play"}}


def test_search_takes_typing_but_colour_keys_still_leave(channel: Recorder) -> None:
    drive(
        channel,
        {"type": "input", "session": 4, "page": "104", "event": {"kind": "text", "text": "200"}},
        key(4, "104", "F1"),
    )
    assert channel.viewers[4].typed == "200", "digits typed into the search are not a page number"
    assert [message["page"] for message in channel.sent if message["type"] == "goto"] == ["100"]


def test_digits_select_a_page_on_a_page_that_does_not_take_them(channel: Recorder) -> None:
    drive(channel, *({"type": "input", "session": 5, "page": "107", "event": {"kind": "text", "text": digit}} for digit in "242"))
    assert [message["page"] for message in channel.sent if message["type"] == "goto"] == ["242"]


def test_my_hackaday_stars_sections(channel: Recorder) -> None:
    drive(channel, key(6, "115", "ArrowDown"), key(6, "115", "Enter"))
    assert channel.viewers[6].favourites == {"Radio"}
    assert Viewer().favourites == set()


def test_the_servers_figures_show_on_page_121(channel: Recorder) -> None:
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
    assert "242 Story 42" in text
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
    drive(made, key(9, "122", "Enter"), key(9, "122", "Enter"), {"type": "open", "session": 9, "page": "113"})
    sounds_sent = [message for message in made.sent if message["type"] == "sound"]
    assert [next(iter(set(message) & {"object", "play"})) for message in sounds_sent] == ["object", "play", "play", "play"], (
        "sent once, played thrice"
    )
    assert sounds_sent[-1]["play"]["page"] == "113", "the welcome belongs to the podcast page"


def test_each_person_has_a_page_and_says_hello(tmp_path: Path) -> None:
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
    assert made.pages[0x141] == "Ada Example"
    first = views.profile(made.people[0], 0, 2)
    assert first.rows[2][:3] == bytes([0x17, 0x7F, 0x7F]), "the portrait's rows"
    assert first.packets == ["OL,26,@iAKA"], "its colours"
    assert "Ada Example" in first.row_text(17)
    assert "no photo" in views.profile(made.people[1], 1, 2).row_text(12), "the Jolly Wrencher stands in"
    drive(made, {"type": "open", "session": 3, "page": "141"}, {"type": "open", "session": 3, "page": "142"})
    plays = [message["play"] for message in made.sent if message["type"] == "sound" and "play" in message]
    assert [play["page"] for play in plays] == ["141"], "Ada says hello on her page; Bo has no clip on this server"


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
