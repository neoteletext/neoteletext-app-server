"""The Hackaday channel: the library's knowledge as pages, and what each viewer does with them.

Shared pages (the front page, the stories, the figures) are the channel's
copy, published when what they show changes, whatever the number of
viewers. What one viewer does (scroll a story, search, play the quiz,
vote) makes that viewer's own copy of the page (docs/applications.md
section 4).
"""

from __future__ import annotations

import asyncio
import random
import sys
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

from neoteletext_app.app import App, Event, page_hears, session_hears
from neoteletext_app.screen import Colour, Screen, tti
from neoteletext_app.sound import RATE, chime, noise, sequence, tone

from hackaday import numbers, views
from hackaday.fetch import Fetcher, FetchError
from hackaday.layout import BODY_ROW, TEXT_WIDTH, flashing, last_top, page, wrap
from hackaday.library import Library, now_utc
from hackaday.tools import ToolError, qr_tti

if TYPE_CHECKING:
    from array import array
    from collections.abc import Callable

    from hackaday.posts import Post

#: The ticker moves on this often, in seconds.
TICKER_EVERY_S = 7
#: Clocks and countdowns are redrawn this often.
MINUTE_S = 60
#: The archive's lists are read again this often.
ARCHIVE_EVERY_S = 6 * 3600
#: Lines a wheel notch scrolls, as desktop browsers do, and a notch in the
#: protocol's units (docs/applications.md section 2: 1/120ths of a line).
WHEEL_LINES = 3
NOTCH = 120
#: The Retrocomputing Challenge closes on Tuesday 27 October 2026 (the
#: announcement, hackaday.com, 18 August 2026); the end of that day in
#: California, where Hackaday is.
CHALLENGE_CLOSES = datetime(2026, 10, 28, 7, 0, tzinfo=UTC)
#: The poll's choices.
POLL_OPTIONS = ["Commodore 64", "ZX Spectrum", "Amiga", "Apple II", "BBC Micro", "Atari 800", "IBM PC", "MSX"]
#: Sounds, by handle, and the profile channel they play on.
CHIME, RIGHT, WRONG, SPIN = 1, 2, 3, 4
SOUND_CHANNEL = 0
#: The searchable length of what is typed.
LONGEST_SEARCH = 30
#: The letters that start every tag group on the tags page.
LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


def sounds() -> dict[int, array[int]]:
    """Answer the channel's sounds: a newsflash chime, right, wrong and a roulette spin."""
    return {
        CHIME: chime([72, 76, 79, 84], 110),
        RIGHT: chime([76, 84], 120),
        WRONG: sequence(tone(220, 180, wave="square", gain=0.5), tone(185, 260, wave="square", gain=0.5)),
        SPIN: noise(240, gain=0.4),
    }


@dataclass
class Viewer:
    """What one viewer is doing."""

    tops: dict[int, int] = field(default_factory=dict)
    wheel: dict[int, int] = field(default_factory=dict)
    chosen: dict[int, int] = field(default_factory=dict)
    reading: Post | None = None
    typed: str = ""
    results: list[Post] = field(default_factory=list)
    searching: bool = False
    roulette: Post | None = None
    question: Post | None = None
    answers: list[str] = field(default_factory=list)
    picked: int | None = None
    score: tuple[int, int] = (0, 0)
    vote: int | None = None
    favourites: set[str] = field(default_factory=set)
    tag: str | None = None
    heard: set[int] = field(default_factory=set)


class Hackaday(App):
    """The channel."""

    def __init__(self, cache: Path) -> None:
        """Serve every page, the library kept in `cache`."""
        pages = {
            numbers.FRONT: "Hackaday",
            numbers.HEADLINES: "Headlines",
            numbers.SECTIONS: "Sections",
            numbers.TAGS: "Tags A to Z",
            numbers.SEARCH: "Search the archive",
            numbers.ON_THIS_DAY: "On this day",
            numbers.ROULETTE: "Hack roulette",
            numbers.DOOM: "DOOM watch",
            numbers.QUIZ: "Guess the section",
            numbers.STATISTICS: "Statistics",
            numbers.WRITERS: "Writers",
            numbers.SECURITY: "This Week in Security",
            numbers.LINKS_COLUMN: "Hackaday Links",
            numbers.PODCAST: "Hackaday Podcast",
            numbers.CHALLENGE: "Retrocomputing Challenge",
            numbers.MINE: "My Hackaday",
            numbers.POLL: "Live poll",
            numbers.COMMENTS_WALL: "Latest comments",
            numbers.DISCUSSED: "Most discussed",
            numbers.THEN_AND_NOW: "Then and now",
            numbers.ABOUT: "How it works",
            numbers.SERVER_STATS: "Behind the scenes",
            numbers.READER: "From the archive",
        }
        for index, name in enumerate(views.section_names()):
            pages[numbers.section_page(index)] = name[:32]
        for slot in range(numbers.STORIES):
            pages[numbers.slot_page(numbers.STORY_MAGAZINE, slot)] = f"Story {slot}"
            pages[numbers.slot_page(numbers.PICTURE_MAGAZINE, slot)] = f"Picture {slot}"
            pages[numbers.slot_page(numbers.COMMENTS_MAGAZINE, slot)] = f"Comments {slot}"
            pages[numbers.slot_page(numbers.LINKS_MAGAZINE, slot)] = f"Links {slot}"
        super().__init__(pages)
        self.library = Library(Fetcher(cache / "http"), cache / "pictures")
        self.library.pictures.mkdir(parents=True, exist_ok=True)
        self.library.on_change = self.changed
        self.library.on_news = self.news
        self.viewers: dict[int, Viewer] = {}
        self.published: dict[tuple[int | str, int], str] = {}
        self.qr: dict[str, str] = {}
        self.ticker = 0
        self.on_this_day: list[Post] = []
        self.doom_ports: list[Post] = []
        self.entries: list[Post] = []
        self.first_posts: list[Post] = []
        self.columns: dict[int, Post | None] = {numbers.SECURITY: None, numbers.LINKS_COLUMN: None, numbers.PODCAST: None}
        self.votes = [0] * len(POLL_OPTIONS)
        self.figures: dict[str, Any] = {}
        self.best = 0
        self.sounds = sounds()
        self.random = random.Random()  # noqa: S311 - a game's choices, not a secret
        self.tasks: set[asyncio.Task[None]] = set()

    # Publishing.

    def put(self, page: int, screens: Screen | list[Screen], owner: int | None = None) -> None:
        """Publish `page` when it differs from what was last published there."""
        listed = screens if isinstance(screens, list) else [screens]
        key: tuple[int | str, int] = ("channel" if owner is None else owner, page)
        text = tti(page, listed)
        if self.published.get(key) == text:
            return
        self.published[key] = text
        self.publish(page, listed, "channel" if owner is None else owner)

    def spawn(self, work: asyncio.Future[None] | asyncio.Task[None] | object) -> None:
        """Run `work` in the background, keeping it until it ends."""
        if asyncio.iscoroutine(work):
            task: asyncio.Task[None] = asyncio.ensure_future(work)
            self.tasks.add(task)
            task.add_done_callback(self.tasks.discard)

    @property
    def posts(self) -> list[Post]:
        """The stories the channel holds, newest first."""
        return self.library.posts

    def slot(self, post: Post) -> int | None:
        """Answer the slot of a post the channel holds."""
        return next((slot for slot, held in enumerate(self.posts) if held.id == post.id), None)

    # The channel's pages.

    async def on_start(self) -> None:
        """Publish what is known, then keep it up to date."""
        self.published.clear()
        self.publish_all()
        self.spawn(self.library.run())
        self.spawn(self.every(TICKER_EVERY_S, self.tick))
        self.spawn(self.every(MINUTE_S, self.minute))
        self.spawn(self.every(ARCHIVE_EVERY_S, self.read_archive))

    async def every(self, seconds: int, work: Callable[[], object]) -> None:
        """Do `work` now and every `seconds`."""
        while True:
            try:
                result = work()
                if asyncio.iscoroutine(result):
                    await result
            except (FetchError, ToolError) as error:
                print(f"hackaday: {error}", file=sys.stderr)  # noqa: T201
            await asyncio.sleep(seconds)

    def tick(self) -> None:
        """Move the ticker on."""
        self.ticker += 1
        self.put(numbers.FRONT, views.front(self.posts, now_utc(), self.ticker))

    def minute(self) -> None:
        """Redraw what shows the time."""
        now = now_utc()
        self.put(numbers.FRONT, views.front(self.posts, now, self.ticker))
        self.put(numbers.CHALLENGE, views.challenge(self.entries, now, CHALLENGE_CLOSES))
        self.put(numbers.DOOM, views.doom(self.doom_ports, now))

    async def read_archive(self) -> None:
        """Read the lists from the whole archive, each published as it comes."""
        self.doom_ports = sorted(
            {post.id: post for tag in ("doom", "does-it-run-doom") for post in await self.library.tagged(tag)}.values(),
            key=lambda post: post.date,
            reverse=True,
        )
        self.put(numbers.DOOM, views.doom(self.doom_ports, now_utc()))
        self.entries = await self.library.tagged("2026-retrocomputing-challenge")
        self.put(numbers.CHALLENGE, views.challenge(self.entries, now_utc(), CHALLENGE_CLOSES))
        self.first_posts = await self.library.first_posts()
        self.put(numbers.THEN_AND_NOW, views.then_and_now(self.first_posts, self.posts))
        for number, words in (
            (numbers.SECURITY, "This Week in Security"),
            (numbers.LINKS_COLUMN, "Hackaday Links"),
            (numbers.PODCAST, "Hackaday Podcast Episode"),
        ):
            if self.columns.get(number) is None:
                found = [post for post in await self.library.search(words, 5) if post.title.startswith(words)]
                self.columns[number] = found[0] if found else None
        self.publish_columns()
        self.on_this_day = await self.library.on_this_day(now_utc())
        self.publish_on_this_day()

    def publish_columns(self) -> None:
        """Publish the regular columns' latest editions."""
        titles = {
            numbers.SECURITY: ("This Week in Security", Colour.RED),
            numbers.LINKS_COLUMN: ("Hackaday Links", Colour.BLUE),
            numbers.PODCAST: ("Podcast", Colour.MAGENTA),
        }
        for number, (title, colour) in titles.items():
            self.put(number, views.column_page(title, colour, self.columns.get(number), 0, self.column_links(number)))

    def column_links(self, page: int) -> list[tuple[str, int | None]]:
        """Answer a column's Fastext: the other columns and the front."""
        return (
            [("Front", numbers.FRONT), ("Security", numbers.SECURITY), ("Links", numbers.LINKS_COLUMN), ("Podcast", numbers.PODCAST)]
            if page
            else []
        )

    def publish_on_this_day(self) -> None:
        """Publish page 105's list for viewers who have not chosen."""
        self.put(numbers.ON_THIS_DAY, self.on_this_day_screen(0))

    def on_this_day_screen(self, chosen: int) -> Screen:
        """Draw page 105 with line `chosen` picked."""
        items = [views.Choice(f"{post.date.year} {post.title}", post) for post in self.on_this_day]
        today = now_utc().strftime("%d %B")
        intro = [f"Hackaday on {today}, every year", "since 2004"]
        if not items:
            intro.append("Reading the archive...")
        return views.choices(
            "On this day",
            Colour.BLUE,
            items,
            chosen,
            [("Front", numbers.FRONT), ("Roulette", numbers.ROULETTE), ("Then+now", numbers.THEN_AND_NOW), ("Search", numbers.SEARCH)],
            intro,
        )

    def publish_all(self) -> None:
        """Publish every shared page from what the library holds."""
        now = now_utc()
        posts = self.posts
        self.put(numbers.FRONT, views.front(posts, now, self.ticker))
        self.put(numbers.HEADLINES, views.headlines(posts, 0))
        self.put(numbers.SECTIONS, views.sections_index(posts))
        for index in range(len(views.section_names())):
            self.put(numbers.section_page(index), views.section(index, posts, 0))
        self.put(numbers.TAGS, self.tags_screen(Viewer()))
        self.put(numbers.SEARCH, views.search("", [], 0, searching=False))
        self.publish_on_this_day()
        self.put(numbers.ROULETTE, self.roulette_screen(None))
        self.put(numbers.DOOM, views.doom(self.doom_ports, now))
        self.put(numbers.QUIZ, views.quiz(None, [], None, (0, 0), self.best))
        self.put(numbers.STATISTICS, views.statistics(posts))
        self.put(numbers.WRITERS, views.writers(posts))
        self.publish_columns()
        self.put(numbers.CHALLENGE, views.challenge(self.entries, now, CHALLENGE_CLOSES))
        self.put(numbers.MINE, views.mine(views.section_names(), set(), 0, posts))
        self.put(numbers.POLL, views.poll(POLL_OPTIONS, self.votes, None, 0))
        self.put(numbers.COMMENTS_WALL, views.comments_wall(posts, 0))
        self.put(numbers.DISCUSSED, views.discussed(posts))
        self.put(numbers.THEN_AND_NOW, views.then_and_now(self.first_posts, posts))
        self.put(numbers.ABOUT, views.about())
        self.put(numbers.SERVER_STATS, views.server_stats(self.figures, self.pages))
        if not posts:
            return
        self.put(numbers.READER, views.reader(posts[0], 0))
        for slot, post in enumerate(posts):
            self.publish_story(slot, post)

    def publish_story(self, slot: int, post: Post) -> None:
        """Publish story `slot`'s four pages."""
        self.put(views.story_page(slot), views.story(post, slot, 0))
        self.put(numbers.slot_page(numbers.PICTURE_MAGAZINE, slot), views.picture(post, slot, self.library.picture_pages.get(post.id)))
        self.put(numbers.slot_page(numbers.COMMENTS_MAGAZINE, slot), views.comments(post, slot, 0))
        self.put(numbers.slot_page(numbers.LINKS_MAGAZINE, slot), self.links_screens(post, slot))
        if post.link not in self.qr:
            self.spawn(self.make_qr(post))

    def links_screens(self, post: Post, slot: int) -> list[Screen]:
        """Draw a story's link pages: the article, then each video, as sub-pages."""
        screens = [views.links_page(post, slot, self.qr.get(post.link), "The full story")]
        screens.extend(
            views.links_page(post, slot, self.qr.get(video), f"Video {number}")
            for number, video in enumerate(post.article.videos[:3], start=1)
        )
        return screens

    async def make_qr(self, post: Post) -> None:
        """Make the QR codes of a story's links, then publish its link pages again."""
        for target in [post.link, *post.article.videos[:3]]:
            if target in self.qr:
                continue
            try:
                self.qr[target] = await qr_tti(target, BODY_ROW, 1)
            except ToolError as error:
                print(f"hackaday: no QR code for {target}: {error}", file=sys.stderr)  # noqa: T201
                return
        slot = self.slot(post)
        if slot is not None:
            self.put(numbers.slot_page(numbers.LINKS_MAGAZINE, slot), self.links_screens(post, slot))

    async def changed(self, what: str) -> None:
        """Publish again what the library's change shows."""
        if what == "pictures":
            for slot, post in enumerate(self.posts):
                self.put(
                    numbers.slot_page(numbers.PICTURE_MAGAZINE, slot), views.picture(post, slot, self.library.picture_pages.get(post.id))
                )
            return
        self.publish_all()

    async def news(self, post: Post) -> None:
        """Announce a new post: the newsflash on the front page, and a chime for whoever is watching it."""
        self.log(f"news: {post.title}")
        self.put(numbers.FRONT, views.front(self.posts, now_utc(), self.ticker))
        self.sound_object(page_hears(numbers.FRONT), CHIME, RATE, self.sounds[CHIME])
        self.play(page_hears(numbers.FRONT), SOUND_CHANNEL, CHIME, gain=70, page=numbers.FRONT)

    async def on_stats(self, stats: dict[str, Any]) -> None:
        """Show the server's figures on page 121 as they come."""
        self.figures = stats
        self.put(numbers.SERVER_STATS, views.server_stats(stats, self.pages))

    # Viewers.

    def viewer(self, session: int) -> Viewer:
        """Answer what `session` is doing, made when first needed."""
        return self.viewers.setdefault(session, Viewer())

    async def on_close(self, session: int) -> None:
        """Forget the viewer."""
        viewer = self.viewers.pop(session, None)
        if viewer is not None and viewer.vote is not None:
            self.votes[viewer.vote] = max(0, self.votes[viewer.vote] - 1)
            self.put(numbers.POLL, views.poll(POLL_OPTIONS, self.votes, None, 0))
        self.published = {key: text for key, text in self.published.items() if key[0] != session}

    def hear(self, session: int, sound: int) -> None:
        """Play one of the channel's sounds to one viewer, sending it the first time."""
        viewer = self.viewer(session)
        if sound not in viewer.heard:
            self.sound_object(session_hears(session), sound, RATE, self.sounds[sound])
            viewer.heard.add(sound)
        self.play(session_hears(session), SOUND_CHANNEL, sound, gain=70)

    async def on_input(self, session: int, page: int, event: Event) -> bool:  # noqa: PLR0911 - one dispatch by page
        """Answer whether the page took the input."""
        if page == numbers.SEARCH:
            return self.search_input(session, event)
        if page == numbers.QUIZ:
            return self.quiz_input(session, event)
        if page == numbers.ROULETTE:
            return self.roulette_input(session, event)
        if page == numbers.POLL:
            return self.poll_input(session, event)
        if page == numbers.MINE:
            return self.mine_input(session, event)
        if page == numbers.TAGS:
            return self.tags_input(session, event)
        if page == numbers.ON_THIS_DAY:
            return self.list_input(session, page, event, self.on_this_day, self.on_this_day_screen)
        return self.scroll_input(session, page, event)

    # Scrolling.

    def scrolled_story(self, page: int, top: int) -> tuple[Screen, int] | None:
        """Answer a story's or its comments' page drawn from line `top`, and its last top line."""
        found = numbers.slot_of(page)
        if found is None or found[1] >= len(self.posts):
            return None
        magazine, slot = found
        post = self.posts[slot]
        if magazine == numbers.STORY_MAGAZINE:
            return views.story(post, slot, top), last_top(len(views.article_lines(post)), views.STORY_ROWS)
        if magazine == numbers.COMMENTS_MAGAZINE:
            return views.comments(post, slot, top), last_top(len(views.comment_lines(post)), views.LIST_ROWS)
        return None

    def scrolled(self, session: int, page: int, top: int) -> tuple[Screen, int] | None:  # noqa: PLR0911 - one answer per kind of page
        """Answer `page` drawn from line `top`, and its last top line, if it scrolls."""
        posts = self.posts
        if numbers.slot_of(page) is not None:
            return self.scrolled_story(page, top)
        if page == numbers.HEADLINES:
            return views.headlines(posts, top), last_top(len(posts), views.LIST_ROWS)
        if page == numbers.COMMENTS_WALL:
            return views.comments_wall(posts, top), last_top(len(views.comments_wall_lines(posts)), views.LIST_ROWS)
        if page in self.columns:
            column = self.columns[page]
            lines = 3 + len(views.article_lines(column)) if column else 1
            titles = {
                numbers.SECURITY: ("This Week in Security", Colour.RED),
                numbers.LINKS_COLUMN: ("Hackaday Links", Colour.BLUE),
                numbers.PODCAST: ("Podcast", Colour.MAGENTA),
            }
            title, colour = titles[page]
            return views.column_page(title, colour, column, top, self.column_links(page)), last_top(lines, views.LIST_ROWS)
        if page == numbers.READER:
            reading = self.viewer(session).reading
            if reading is None:
                return None
            return views.reader(reading, top), last_top(len(views.article_lines(reading)), views.STORY_ROWS)
        sections = views.section_names()
        for index in range(len(sections)):
            if page == numbers.section_page(index):
                count = len([post for post in posts if sections[index] in post.sections])
                return views.section(index, posts, top), last_top(count, views.LIST_ROWS)
        return None

    @staticmethod
    def movement(viewer: Viewer, page: int, event: Event, *, top: int, last: int, screen_lines: int) -> int | None:  # noqa: PLR0911, PLR0913 - one answer per key; the view's state
        """Answer where `event` scrolls a view at `top` to, or None when it does not scroll."""
        if event.pressed_key("PageDown") or event.character == " ":
            return top + screen_lines
        if event.pressed_key("PageUp"):
            return top - screen_lines
        if event.pressed_key("ArrowDown"):
            return top + 1
        if event.pressed_key("ArrowUp"):
            return top - 1
        if event.pressed_key("Home"):
            return 0
        if event.pressed_key("End"):
            return last
        if event.kind == "wheel":
            # Whole lines; what is left of a notch waits for the next turn.
            turned = viewer.wheel.get(page, 0) + event.dy * WHEEL_LINES
            lines = int(turned / NOTCH)
            viewer.wheel[page] = turned - lines * NOTCH
            return top + lines
        return None

    def scroll_input(self, session: int, page: int, event: Event) -> bool:
        """Scroll a page that scrolls: a screen, a line, the ends, the wheel."""
        viewer = self.viewer(session)
        top = viewer.tops.get(page, 0)
        drawn = self.scrolled(session, page, top)
        if drawn is None:
            return False
        _, last = drawn
        screen_lines = views.STORY_ROWS - 1 if numbers.slot_of(page) or page == numbers.READER else views.LIST_ROWS - 1
        moved = self.movement(viewer, page, event, top=top, last=last, screen_lines=screen_lines)
        if moved is None:
            return False
        moved = max(0, min(last, moved))
        if moved != top:
            viewer.tops[page] = moved
            drawn = self.scrolled(session, page, moved)
            if drawn is not None:
                self.put(page, drawn[0], session)
        return True

    # Lists to choose from.

    def list_input(self, session: int, page: int, event: Event, posts: list[Post], draw: Callable[[int], Screen]) -> bool:
        """Choose from a list of posts with the arrows, the wheel or a click; read the chosen one with Enter."""
        viewer = self.viewer(session)
        chosen = viewer.chosen.get(page, 0)
        if not posts:
            return False
        if event.pressed_key("ArrowDown") or (event.kind == "wheel" and event.dy > 0):
            chosen = min(len(posts) - 1, chosen + 1)
        elif event.pressed_key("ArrowUp") or (event.kind == "wheel" and event.dy < 0):
            chosen = max(0, chosen - 1)
        elif event.pressed_key("Enter"):
            self.read(session, posts[chosen])
            return True
        else:
            return False
        viewer.chosen[page] = chosen
        self.put(page, draw(chosen), session)
        return True

    def read(self, session: int, post: Post) -> None:
        """Show a post: its story page when the channel holds it, else the archive reader."""
        slot = self.slot(post)
        if slot is not None:
            self.goto(session, views.story_page(slot))
            return
        viewer = self.viewer(session)
        viewer.reading = post
        viewer.tops[numbers.READER] = 0
        self.put(numbers.READER, views.reader(post, 0), session)
        self.goto(session, numbers.READER)

    # Search.

    def search_input(self, session: int, event: Event) -> bool:
        """Type words; Enter searches the archive; the arrows choose a result; Enter on it reads it."""
        viewer = self.viewer(session)
        chosen = viewer.chosen.get(numbers.SEARCH, 0)
        if event.kind in {"text", "paste"}:
            viewer.typed = (viewer.typed + event.text)[:LONGEST_SEARCH]
        elif event.pressed_key("Backspace"):
            viewer.typed = viewer.typed[:-1]
        elif event.pressed_key("Enter"):
            if viewer.results and viewer.chosen.get(numbers.SEARCH) is not None and not viewer.typed:
                self.read(session, viewer.results[chosen])
                return True
            if viewer.typed.strip():
                viewer.searching = True
                self.spawn(self.run_search(session, viewer.typed.strip()))
        elif event.pressed_key("ArrowDown") and viewer.results:
            chosen = min(len(viewer.results) - 1, chosen + 1)
        elif event.pressed_key("ArrowUp") and viewer.results:
            chosen = max(0, chosen - 1)
        elif event.pressed_key("Escape"):
            viewer.typed = ""
        else:
            return False
        viewer.chosen[numbers.SEARCH] = chosen
        self.put(numbers.SEARCH, views.search(viewer.typed, viewer.results, chosen, searching=viewer.searching), session)
        return True

    async def run_search(self, session: int, words: str) -> None:
        """Search the archive for `words`, and show the viewer what came back."""
        viewer = self.viewer(session)
        try:
            viewer.results = await self.library.search(words)
        except FetchError as error:
            print(f"hackaday: search: {error}", file=sys.stderr)  # noqa: T201
            viewer.results = []
        viewer.searching = False
        viewer.typed = ""
        viewer.chosen[numbers.SEARCH] = 0
        if session in self.viewers:
            self.put(numbers.SEARCH, views.search(words + " (Enter reads)", viewer.results, 0, searching=False), session)

    # Roulette.

    def roulette_screen(self, post: Post | None, *, spinning: bool = False) -> Screen:
        """Draw page 106: a random post from the whole archive."""
        screen = page(
            "Hack roulette",
            Colour.RED,
            [("Front", numbers.FRONT), ("Archive", numbers.ON_THIS_DAY), ("Search", numbers.SEARCH), ("DOOM", numbers.DOOM)],
        )
        screen.coloured(BODY_ROW, 0, Colour.CYAN, f"One of {self.library.total or 58711} hacks since 2004")
        if spinning:
            flashing(screen, 10, 12, Colour.YELLOW, "SPINNING...")
        elif post is None:
            screen.centred(10, Colour.YELLOW, "Press Space to spin the wheel")
        else:
            screen.coloured(5, 0, Colour.YELLOW, f"{post.date.strftime('%d %B %Y')}")
            for offset, line in enumerate(wrap(post.title, TEXT_WIDTH)[:3]):
                screen.coloured(7 + offset, 0, Colour.WHITE, line)
            for offset, line in enumerate(wrap(post.excerpt, TEXT_WIDTH)[:7]):
                screen.coloured(11 + offset, 0, Colour.CYAN, line)
            screen.coloured(19, 0, Colour.GREEN, views.fit(f"by {post.author}  {', '.join(post.categories)}", TEXT_WIDTH))
        screen.coloured(views.HINT_ROW, 0, Colour.GREEN, "Space spins again, Enter reads it")
        return screen

    def roulette_input(self, session: int, event: Event) -> bool:
        """Space spins; Enter reads what came up."""
        viewer = self.viewer(session)
        if event.character == " ":
            self.put(numbers.ROULETTE, self.roulette_screen(viewer.roulette, spinning=True), session)
            self.hear(session, SPIN)
            self.spawn(self.spin(session))
            return True
        if event.pressed_key("Enter") and viewer.roulette is not None:
            self.read(session, viewer.roulette)
            return True
        return False

    async def spin(self, session: int) -> None:
        """Find a random post for the viewer."""
        try:
            post = await self.library.random_post(self.random)
        except FetchError as error:
            print(f"hackaday: roulette: {error}", file=sys.stderr)  # noqa: T201
            post = None
        if session in self.viewers:
            self.viewer(session).roulette = post
            self.put(numbers.ROULETTE, self.roulette_screen(post), session)

    # The quiz.

    def next_question(self, viewer: Viewer) -> None:
        """Set the viewer a new question: a held story, its section and three others."""
        candidates = [post for post in self.posts if post.sections]
        if not candidates:
            return
        post = self.random.choice(candidates)
        others = [name for name in views.section_names() if name not in post.sections]
        answers = [post.sections[0], *self.random.sample(others, 3)]
        self.random.shuffle(answers)
        viewer.question, viewer.answers, viewer.picked = post, answers, None

    def quiz_input(self, session: int, event: Event) -> bool:
        """Answer with the coloured keys; Enter asks the next question."""
        viewer = self.viewer(session)
        if viewer.question is None or (event.pressed_key("Enter") and viewer.picked is not None):
            self.next_question(viewer)
        elif event.colour is not None and viewer.picked is None and event.colour < len(viewer.answers):
            viewer.picked = event.colour
            right = viewer.answers[event.colour] in viewer.question.sections
            done, asked = viewer.score
            viewer.score = (done + int(right), asked + 1)
            self.best = max(self.best, viewer.score[0])
            self.hear(session, RIGHT if right else WRONG)
        elif not event.pressed_key("Enter"):
            return False
        self.put(numbers.QUIZ, views.quiz(viewer.question, viewer.answers, viewer.picked, viewer.score, self.best), session)
        return True

    async def on_open(self, session: int, page: int) -> None:
        """Start a viewer's quiz when they open it."""
        if page == numbers.QUIZ:
            viewer = self.viewer(session)
            if viewer.question is None:
                self.next_question(viewer)
                self.put(numbers.QUIZ, views.quiz(viewer.question, viewer.answers, None, viewer.score, self.best), session)

    # The poll.

    def poll_input(self, session: int, event: Event) -> bool:
        """Choose with the arrows and vote with Enter: once each, the count shared at once."""
        viewer = self.viewer(session)
        chosen = viewer.chosen.get(numbers.POLL, 0)
        if event.pressed_key("ArrowDown"):
            chosen = min(len(POLL_OPTIONS) - 1, chosen + 1)
        elif event.pressed_key("ArrowUp"):
            chosen = max(0, chosen - 1)
        elif event.pressed_key("Enter") and viewer.vote is None:
            viewer.vote = chosen
            self.votes[chosen] += 1
            self.hear(session, RIGHT)
            self.put(numbers.POLL, views.poll(POLL_OPTIONS, self.votes, None, 0))
        else:
            return False
        viewer.chosen[numbers.POLL] = chosen
        self.put(numbers.POLL, views.poll(POLL_OPTIONS, self.votes, viewer.vote, chosen), session)
        return True

    # My Hackaday.

    def mine_input(self, session: int, event: Event) -> bool:
        """Choose a section with the arrows and star it with Enter."""
        viewer = self.viewer(session)
        names = views.section_names()
        chosen = viewer.chosen.get(numbers.MINE, 0)
        if event.pressed_key("ArrowDown"):
            chosen = min(len(names) - 1, chosen + 1)
        elif event.pressed_key("ArrowUp"):
            chosen = max(0, chosen - 1)
        elif event.pressed_key("Enter") or event.character == " ":
            viewer.favourites ^= {names[chosen]}
        else:
            return False
        viewer.chosen[numbers.MINE] = chosen
        self.put(numbers.MINE, views.mine(names, viewer.favourites, chosen, self.posts), session)
        return True

    # Tags.

    def tag_counts(self) -> list[tuple[str, int]]:
        """Answer every tag of the held stories with how many have it, A to Z."""
        counts: dict[str, int] = {}
        for post in self.posts:
            for tag in {tag.lower(): tag for tag in post.tags}.values():
                counts[tag] = counts.get(tag, 0) + 1
        return sorted(counts.items(), key=lambda item: item[0].lower())

    def tags_screen(self, viewer: Viewer) -> Screen:
        """Draw page 103: the tags A to Z, or the stories with the tag chosen."""
        links: list[tuple[str, int | None]] = [
            ("Front", numbers.FRONT),
            ("Sections", numbers.SECTIONS),
            ("Search", numbers.SEARCH),
            ("Mine", numbers.MINE),
        ]
        chosen = viewer.chosen.get(numbers.TAGS, 0)
        if viewer.tag is not None:
            posts = [post for post in self.posts if post.has(viewer.tag)]
            items = [views.Choice(post.title, post) for post in posts]
            return views.choices(f"Tag: {viewer.tag}"[:30], Colour.GREEN, items, chosen, links, ["Enter reads, Backspace back to the tags"])
        items = [views.Choice(f"{tag} ({count})", None) for tag, count in self.tag_counts()]
        return views.choices("Tags A to Z", Colour.GREEN, items, chosen, links, ["Type a letter to jump, Enter to open"])

    def tags_input(self, session: int, event: Event) -> bool:
        """Choose a tag with the arrows or a letter and open it with Enter; then Enter reads and Backspace goes back."""
        viewer = self.viewer(session)
        chosen = viewer.chosen.get(numbers.TAGS, 0)
        if viewer.tag is not None:
            posts = [post for post in self.posts if post.has(viewer.tag)]
            if event.pressed_key("Backspace", "Escape"):
                viewer.tag, chosen = None, 0
            elif event.pressed_key("Enter") and posts:
                self.read(session, posts[min(chosen, len(posts) - 1)])
                return True
            elif event.pressed_key("ArrowDown"):
                chosen = min(max(0, len(posts) - 1), chosen + 1)
            elif event.pressed_key("ArrowUp"):
                chosen = max(0, chosen - 1)
            else:
                return False
        else:
            tags = self.tag_counts()
            character = event.character
            if event.pressed_key("ArrowDown") or (event.kind == "wheel" and event.dy > 0):
                chosen = min(max(0, len(tags) - 1), chosen + 1)
            elif event.pressed_key("ArrowUp") or (event.kind == "wheel" and event.dy < 0):
                chosen = max(0, chosen - 1)
            elif event.pressed_key("Enter") and tags:
                viewer.tag, chosen = tags[min(chosen, len(tags) - 1)][0], 0
            elif character is not None and character.upper() in LETTERS:
                chosen = next((index for index, (tag, _) in enumerate(tags) if tag.upper() >= character.upper()), chosen)
            else:
                return False
        viewer.chosen[numbers.TAGS] = chosen
        self.put(numbers.TAGS, self.tags_screen(viewer), session)
        return True


def main() -> None:
    """Serve the channel, its cache beside it."""
    cache = Path(__file__).resolve().parents[2] / "cache"
    Hackaday(cache).run()
