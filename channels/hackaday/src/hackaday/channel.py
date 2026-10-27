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

from neoteletext_app.app import App, Event, page_hears, prompt_at, session_hears
from neoteletext_app.charset import fold
from neoteletext_app.screen import FASTEXT_ROW, TITLE_LIMIT, Colour, Screen, tti
from neoteletext_app.sound import RATE, chime, noise, sequence, tone

from hackaday import clips, numbers, people, site, views
from hackaday.fetch import Fetcher, FetchError
from hackaday.layout import BAR_ATTRIBUTE_COLUMN, BODY_ROW, TEXT_WIDTH, flashing, last_top, page, thumb, top_at, wrap
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
#: The held-buttons bit of the main button on a move (docs/applications.md section 2).
LEFT_HELD = 1
#: What ends a title cut short in the contents.
ELLIPSIS = "..."
#: The columns with a page of their own: their titles and colours.
COLUMN_TITLES = {
    numbers.SECURITY: ("This Week in Security", Colour.RED),
    numbers.LINKS_COLUMN: ("Hackaday Links", Colour.BLUE),
    numbers.PODCAST: ("Podcast", Colour.MAGENTA),
}
#: How long a viewer arriving at page 100 sees the whole-page emblem, in seconds.
SPLASH_S = 2.0
#: The searchable length of what is typed.
LONGEST_SEARCH = 30
#: The letters that start every tag group on the tags page.
LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


def shortened(title: str) -> str:
    """Answer a post's title for the contents: in characters it can hold, and cut with an ellipsis when it is too long."""
    folded = " ".join(fold(title).split())
    if len(folded) <= TITLE_LIMIT:
        return folded
    return folded[: TITLE_LIMIT - len(ELLIPSIS)].rstrip() + ELLIPSIS


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
    grips: dict[int, int] = field(default_factory=dict)
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
    favourites: set[int] = field(default_factory=set)
    tags: dict[int, str] = field(default_factory=dict)
    views: dict[int, str] = field(default_factory=dict)
    greeted: bool = False
    heard: set[int] = field(default_factory=set)
    clips_heard: set[int] = field(default_factory=set)


class Hackaday(App):
    """The channel."""

    def __init__(self, cache: Path) -> None:
        """Serve every page, the library kept in `cache`."""
        pages = {
            numbers.FRONT: "Hackaday",
            numbers.HEADLINES: "Headlines",
            numbers.MAGAZINES: "The magazines",
            numbers.POPULAR_TAGS: "Popular tags",
            numbers.TAGS: "Tags A to Z",
            numbers.SEARCH: "Search the archive",
            numbers.DISCUSSED: "Most discussed",
            numbers.COMMENTS_WALL: "Latest comments",
            numbers.WRITERS: "Writers",
            numbers.STATISTICS: "Statistics",
            numbers.COLUMNS: "Columns",
            numbers.LINKS_COLUMN: "Hackaday Links",
            numbers.PODCAST: "Hackaday Podcast",
            numbers.SOUNDBOARD: "Podcast sounds",
            numbers.PEOPLE: "The people of Hackaday",
            numbers.ON_THIS_DAY: "On this day",
            numbers.ROULETTE: "Hack roulette",
            numbers.THEN_AND_NOW: "Then and now",
            numbers.READER: "From the archive",
            numbers.QUIZ: "Guess the magazine",
            numbers.POLL: "Live poll",
            numbers.MINE: "My Hackaday",
            numbers.SERVER_STATS: "Behind the scenes",
            numbers.ABOUT: "How it works",
        }
        sections: dict[int, str] = {}
        for mag in site.MAGAZINES:
            for slot in range(mag.slots):
                pages[mag.story_page(slot)] = f"{mag.short} story {slot + 1}"
        for mag in site.SUBJECTS:
            pages[mag.front] = mag.name
            sections[mag.front] = mag.name
            if mag.feature:
                pages[numbers.page(mag.number, numbers.FEATURE)] = mag.feature
            pages[mag.stories_page] = f"{mag.short}: every story"
            pages[numbers.page(mag.number, numbers.TAG_INDEX)] = f"{mag.short}: tags"
            for index in range(numbers.TAG_PAGES):
                pages[numbers.page(mag.number, numbers.FIRST_TAG + index)] = f"{mag.short}: tag {index + 1}"
        found = people.load(cache.parent / "people")
        for index, person in enumerate(found):
            pages[views.person_page(index)] = person.name[:32]
        super().__init__(pages, sections)
        self.placed = site.place([])
        self.people = found
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
        self.clips = clips.load(cache.parent / "sounds")
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

    def front_screen(self) -> Screen:
        """Draw the front page as it is now."""
        return views.front(self.posts, self.placed, now_utc(), self.ticker, CHALLENGE_CLOSES)

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
        self.put(numbers.FRONT, self.front_screen())

    def minute(self) -> None:
        """Redraw what shows the time."""
        now = now_utc()
        self.put(numbers.FRONT, self.front_screen())
        self.put(numbers.CHALLENGE, views.challenge(self.entries, now, CHALLENGE_CLOSES))
        self.put(numbers.DOOM, views.doom(self.doom_ports, now))
        self.publish_magazine(site.magazine(numbers.CHALLENGE >> 8))

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
        await self.read_people()
        self.on_this_day = await self.library.on_this_day(now_utc())
        self.publish_on_this_day()

    def publish_people(self) -> None:
        """Publish the people's index and their pages."""
        self.put(numbers.PEOPLE, views.people_index(self.people))
        for index, person in enumerate(self.people):
            self.put(views.person_page(index), views.profile(person, index, len(self.people)))

    async def read_people(self) -> None:
        """Read each person's post count and latest posts, and publish their pages again."""
        for person in self.people:
            person.posts, person.latest = await self.library.author(person.author)
        self.publish_people()

    def publish_columns(self) -> None:
        """Publish the regular columns' latest editions."""
        for number in COLUMN_TITLES:
            self.put(number, self.column_screen(number, 0))

    def column_screen(self, page: int, top: int) -> Screen:
        """Draw a column's page from line `top`."""
        title, colour = COLUMN_TITLES[page]
        links: list[tuple[str, int | None]] = [
            ("Columns", numbers.COLUMNS),
            ("Security", numbers.SECURITY),
            ("Links", numbers.LINKS_COLUMN),
            ("Podcast", numbers.PODCAST),
        ]
        return views.column_page(title, colour, self.columns.get(page), top, links)

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
        self.placed = site.place(posts)
        placed = self.placed
        self.put(numbers.FRONT, self.front_screen())
        self.put(numbers.HEADLINES, views.headlines(posts, placed, 0))
        self.put(numbers.MAGAZINES, views.magazines_index(placed))
        self.put(numbers.POPULAR_TAGS, views.popular_tags(posts, placed))
        self.put(numbers.TAGS, self.tags_screen(numbers.TAGS, Viewer()))
        self.put(numbers.SEARCH, views.search("", [], 0, searching=False))
        self.put(numbers.DISCUSSED, views.discussed(posts, placed))
        self.put(numbers.COMMENTS_WALL, views.comments_wall(posts, placed, 0))
        self.put(numbers.WRITERS, views.writers(posts))
        self.put(numbers.STATISTICS, views.statistics(posts))
        self.put(numbers.COLUMNS, views.columns_index(posts, placed))
        self.publish_columns()
        self.put(numbers.SOUNDBOARD, views.soundboard(self.clips, 0, None))
        self.publish_people()
        self.publish_on_this_day()
        self.put(numbers.ROULETTE, self.roulette_screen(None))
        self.put(numbers.THEN_AND_NOW, views.then_and_now(self.first_posts, posts))
        self.put(numbers.QUIZ, views.quiz(None, [], None, (0, 0), self.best))
        self.put(numbers.POLL, views.poll(POLL_OPTIONS, self.votes, None, 0))
        self.put(numbers.MINE, views.mine(set(), 0, posts, placed))
        self.put(numbers.SERVER_STATS, views.server_stats(self.figures, self.pages))
        self.put(numbers.ABOUT, views.about())
        self.put(numbers.CHALLENGE, views.challenge(self.entries, now, CHALLENGE_CLOSES))
        self.put(numbers.DOOM, views.doom(self.doom_ports, now))
        for mag in site.SUBJECTS:
            self.publish_magazine(mag)
        for mag in site.MAGAZINES:
            held = placed.stories.get(mag.number, [])
            for slot in range(mag.slots):
                number = mag.story_page(slot)
                self.title(number, shortened(held[slot].title) if slot < len(held) else f"{mag.short} story {slot + 1}")
            for slot, post in enumerate(held):
                self.publish_story(post, mag.story_page(slot))
        if posts:
            self.put(numbers.READER, views.reader(posts[0], 0))

    def publish_magazine(self, mag: site.Magazine) -> None:
        """Publish a subject magazine's front, its list of stories, its tags and its tag pages, and title them."""
        placed = self.placed
        feature = mag.feature
        if mag.front == numbers.CHALLENGE & 0xF00:
            left = CHALLENGE_CLOSES - now_utc()
            feature = f"Challenge closes in {left.days}d {left.seconds // 3600}h" if left.total_seconds() > 0 else "Challenge: judging"
        self.put(mag.front, views.magazine_front(mag, placed, feature))
        self.put(mag.stories_page, views.magazine_stories(mag, placed, 0))
        index = numbers.page(mag.number, numbers.TAG_INDEX)
        self.put(index, self.tags_screen(index, Viewer()))
        tagged = placed.tag_pages(mag.number)
        for place in range(numbers.TAG_PAGES):
            number = numbers.page(mag.number, numbers.FIRST_TAG + place)
            if place < len(tagged):
                _, tag, count = tagged[place]
                self.title(number, shortened(f"{tag} ({count})"))
                self.put(number, views.tag_page(mag, tag, self.posts, placed, 0))
            else:
                self.title(number, f"{mag.short}: tag {place + 1}")
                self.put(number, views.tag_page(mag, "none yet", [], placed, 0))

    def magazine_of_page(self, page: int) -> site.Magazine | None:
        """Answer the magazine a page is in."""
        found = numbers.decimal(page)
        return site.magazine(found[0]) if found is not None else None

    def next_story(self, page: int) -> int:
        """Answer the page after story `page` in its magazine, or the magazine's front after its last."""
        mag = self.magazine_of_page(page) or site.HOME
        held = self.placed.stories.get(mag.number, [])
        post = self.placed.post_at(page)
        if post is None or post not in held:
            return mag.front
        following = held.index(post) + 1
        return mag.story_page(following) if following < len(held) else mag.front

    def story_view(self, post: Post, page: int, view: str, top: int) -> list[Screen]:
        """Draw story `page` as `view` shows it, from line `top` where it scrolls."""
        mag = self.magazine_of_page(page) or site.HOME
        following = self.next_story(page)
        if view == views.PICTURE:
            return [views.picture(post, mag, page, following, self.library.picture_pages.get(post.id))]
        if view == views.COMMENTS:
            return [views.comments(post, mag, page, following, top)]
        if view == views.LINKS:
            return self.links_screens(post, mag, page, following)
        return [views.story(post, mag, page, following, top)]

    def publish_story(self, post: Post, page: int) -> None:
        """Publish a story's shared page, its text; its other views are each viewer's own."""
        self.put(page, self.story_view(post, page, views.STORY, 0))
        if post.link not in self.qr:
            self.spawn(self.make_qr(post))

    def links_screens(self, post: Post, mag: site.Magazine, page: int, following: int) -> list[Screen]:
        """Draw a story's link pages: the article, then each video, as sub-pages."""
        screens = [views.links_page(post, mag, page, following, self.qr.get(post.link), "The full story")]
        screens.extend(
            views.links_page(post, mag, page, following, self.qr.get(video), f"Video {number}")
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
        self.redraw_viewers_of(post, views.LINKS)

    async def changed(self, what: str) -> None:
        """Publish again what the library's change shows."""
        if what == "pictures":
            for post in self.posts:
                self.redraw_viewers_of(post, views.PICTURE)
            return
        self.publish_all()
        for post in self.posts:
            self.redraw_viewers_of(post, views.COMMENTS)

    def redraw_viewers_of(self, post: Post, view: str) -> None:
        """Draw again `post`'s `view` for each viewer looking at it."""
        page = self.placed.page_of(post)
        if page is None:
            return
        for session, viewer in self.viewers.items():
            if viewer.views.get(page) == view:
                self.put(page, self.story_view(post, page, view, viewer.tops.get(page, 0)), session)

    async def news(self, post: Post) -> None:
        """Announce a new post: the newsflash on the front page, and a chime for whoever is watching it."""
        self.log(f"news: {post.title}")
        self.put(numbers.FRONT, self.front_screen())
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
        if page == numbers.TAGS or self.is_tag_index(page):
            return self.tags_input(session, page, event)
        if numbers.is_story(page) and self.placed.post_at(page) is not None:
            return self.story_input(session, page, event)
        if page == numbers.ON_THIS_DAY:
            return self.list_input(session, page, event, self.on_this_day, self.on_this_day_screen)
        if page == numbers.SOUNDBOARD:
            return self.soundboard_input(session, event)
        return self.scroll_input(session, page, event)

    # Scrolling.

    def scrolled(self, session: int, page: int, top: int) -> tuple[Screen, int] | None:  # noqa: PLR0911 - one answer per kind of page
        """Answer `page` drawn from line `top`, and its last top line, if it scrolls."""
        posts, placed = self.posts, self.placed
        if numbers.is_story(page):
            post = placed.post_at(page)
            view = self.viewer(session).views.get(page, views.STORY)
            if post is None or view in {views.PICTURE, views.LINKS}:
                return None
            total = len(views.comment_lines(post)) if view == views.COMMENTS else len(views.article_lines(post))
            rows = views.LIST_ROWS if view == views.COMMENTS else views.STORY_ROWS
            return self.story_view(post, page, view, top)[0], last_top(total, rows)
        if page == numbers.HEADLINES:
            return views.headlines(posts, placed, top), last_top(len(posts), views.LIST_ROWS)
        if page == numbers.COMMENTS_WALL:
            return views.comments_wall(posts, placed, top), last_top(len(views.comments_wall_lines(posts, placed)), views.LIST_ROWS)
        if page in COLUMN_TITLES:
            column = self.columns.get(page)
            lines = 3 + len(views.article_lines(column)) if column else 1
            return self.column_screen(page, top), last_top(lines, views.LIST_ROWS)
        if page == numbers.READER:
            reading = self.viewer(session).reading
            if reading is None:
                return None
            return views.reader(reading, top), last_top(len(views.article_lines(reading)), views.STORY_ROWS)
        return self.scrolled_magazine(page, top)

    def scrolled_magazine(self, page: int, top: int) -> tuple[Screen, int] | None:
        """Answer a subject magazine's list of stories or one of its tag pages drawn from line `top`, and its last top line."""
        posts, placed = self.posts, self.placed
        mag = self.magazine_of_page(page)
        if mag is None or mag.number == numbers.HOME:
            return None
        if page == mag.stories_page:
            return views.magazine_stories(mag, placed, top), last_top(len(placed.stories.get(mag.number, [])), views.LIST_ROWS)
        for number, tag, _ in placed.tag_pages(mag.number):
            if page == number:
                return views.tag_page(mag, tag, posts, placed, top), last_top(len(views.tag_lines(posts, placed, tag)), views.LIST_ROWS)
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

    def text_rows(self, session: int, page: int) -> tuple[int, int]:
        """Answer the first row and the rows a scrolling page's text and scroll bar take."""
        story_text = numbers.is_story(page) and self.viewer(session).views.get(page, views.STORY) == views.STORY
        if page == numbers.READER or story_text:
            return views.STORY_TEXT_ROW, views.STORY_ROWS
        return BODY_ROW, views.LIST_ROWS

    @staticmethod
    def dragged(viewer: Viewer, page: int, event: Event, *, top: int, total: int, first_row: int, rows: int) -> int | None:  # noqa: PLR0913 - the view's state and its place
        """Answer where the scroll bar takes a view at `top` to, or None when `event` is not the bar's.

        A press on the block grips it where it was pressed and moves drag it;
        a press above or below it turns a screen, as a desktop scroll bar does.
        """
        cell = event.row - first_row
        if event.action == "press":
            viewer.grips.pop(page, None)
            on_bar = event.button == 0 and event.column >= BAR_ATTRIBUTE_COLUMN and 0 <= cell < rows and total > rows
            return Hackaday.bar_pressed(viewer, page, cell, top=top, total=total, rows=rows) if on_bar else None
        grip = viewer.grips.get(page)
        if grip is None:
            return None
        if event.action == "move" and event.buttons & LEFT_HELD:
            return top_at(cell - grip, total, rows)
        del viewer.grips[page]
        return top

    @staticmethod
    def bar_pressed(viewer: Viewer, page: int, cell: int, *, top: int, total: int, rows: int) -> int:  # noqa: PLR0913 - the view's state and its place
        """Answer where a press on the scroll bar's `cell` takes a view at `top`, gripping the block when it is on it."""
        start, length = thumb(top, total, rows, rows)
        if cell < start:
            return top - (rows - 1)
        if cell >= start + length:
            return top + (rows - 1)
        viewer.grips[page] = cell - start
        return top

    def scroll_input(self, session: int, page: int, event: Event) -> bool:
        """Scroll a page that scrolls: a screen, a line, the ends, the wheel."""
        viewer = self.viewer(session)
        top = viewer.tops.get(page, 0)
        drawn = self.scrolled(session, page, top)
        if drawn is None:
            return False
        _, last = drawn
        first_row, rows = self.text_rows(session, page)
        if event.kind == "mouse":
            moved = self.dragged(viewer, page, event, top=top, total=last + rows, first_row=first_row, rows=rows)
        else:
            moved = self.movement(viewer, page, event, top=top, last=last, screen_lines=rows - 1)
        if moved is None:
            return False
        moved = max(0, min(last, moved))
        if moved != top:
            viewer.tops[page] = moved
            drawn = self.scrolled(session, page, moved)
            if drawn is not None:
                self.put(page, drawn[0], session)
        return True

    # A story's views.

    def story_input(self, session: int, page: int, event: Event) -> bool:
        """Choose a story's view, or the next story, with the coloured keys or a click on their prompts; scroll what scrolls."""
        viewer = self.viewer(session)
        view = viewer.views.get(page, views.STORY)
        key = event.colour
        if event.clicked and event.row == FASTEXT_ROW and view != views.PICTURE:
            shown = self.shown(session, page)
            key = None if shown is None else prompt_at(shown, event.column)
        if key is None:
            return self.scroll_input(session, page, event)
        choice = views.story_choices(view)[key]
        if choice == views.NEXT:
            self.goto(session, self.next_story(page))
            return True
        self.show_view(session, page, choice)
        return True

    def show_view(self, session: int, page: int, view: str) -> None:
        """Show a viewer one of story `page`'s views, from its top: the text is the channel's copy again."""
        viewer = self.viewer(session)
        viewer.views[page] = view
        viewer.tops[page] = 0
        viewer.grips.pop(page, None)
        post = self.placed.post_at(page)
        if view == views.STORY or post is None:
            del viewer.views[page]
            self.release(session, page)
            self.published.pop((session, page), None)
            return
        self.put(page, self.story_view(post, page, view, 0), session)

    async def on_leave(self, session: int, page: int) -> None:
        """Put a story's text back for a viewer who leaves it, to find when they come back."""
        viewer = self.viewers.get(session)
        if viewer is not None and page in viewer.views:
            self.show_view(session, page, views.STORY)

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
        found = self.placed.page_of(post)
        if found is not None:
            self.goto(session, found)
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
        text = event.text if event.kind == "paste" else event.typed
        if text:
            viewer.typed = (viewer.typed + text)[:LONGEST_SEARCH]
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
        candidates = [post for post in self.posts if site.magazines_of(post)[0].number != numbers.HOME]
        if not candidates:
            return
        post = self.random.choice(candidates)
        others = [mag.name for mag in site.SUBJECTS if mag.name not in post.sections]
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
        """Greet a viewer arriving with the emblem; start a viewer's quiz when they open it; greet a podcast listener."""
        viewer = self.viewer(session)
        if page == numbers.FRONT and not viewer.greeted:
            viewer.greeted = True
            self.put(numbers.FRONT, views.splash(), session)
            asyncio.get_running_loop().call_later(SPLASH_S, self.end_splash, session)
        if page == numbers.PODCAST:
            self.play_named(session, "welcome", page)
        for index, person in enumerate(self.people):
            if page == views.person_page(index):
                self.play_named(session, person.clip, page)
        if page == numbers.QUIZ:
            viewer = self.viewer(session)
            if viewer.question is None:
                self.next_question(viewer)
                self.put(numbers.QUIZ, views.quiz(viewer.question, viewer.answers, None, viewer.score, self.best), session)

    def end_splash(self, session: int) -> None:
        """Give a greeted viewer the front page as everyone sees it."""
        if session in self.viewers:
            self.release(session, numbers.FRONT)
            self.published.pop((session, numbers.FRONT), None)

    # The podcast's sound grabs.

    def play_clip(self, session: int, clip: clips.Clip, *, page: int | None = None) -> None:
        """Play a sound grab to one viewer, sending it the first time; bound to `page` when given."""
        viewer = self.viewer(session)
        if clip.handle not in viewer.clips_heard:
            self.sound_object(session_hears(session), clip.handle, clip.rate, clip.samples)
            viewer.clips_heard.add(clip.handle)
        self.play(session_hears(session), SOUND_CHANNEL, clip.handle, gain=90, page=page)

    def play_named(self, session: int, name: str, page: int) -> None:
        """Play the clip called `name` to a viewer, bound to `page`, if the server has it."""
        clip = next((clip for clip in self.clips if clip.name == name), None)
        if clip is not None:
            self.play_clip(session, clip, page=page)

    def soundboard_input(self, session: int, event: Event) -> bool:
        """Choose a grab with the arrows or the wheel; Enter or Space plays it."""
        viewer = self.viewer(session)
        chosen = viewer.chosen.get(numbers.SOUNDBOARD, 0)
        playing = None
        if not self.clips:
            return False
        if event.pressed_key("ArrowDown") or (event.kind == "wheel" and event.dy > 0):
            chosen = min(len(self.clips) - 1, chosen + 1)
        elif event.pressed_key("ArrowUp") or (event.kind == "wheel" and event.dy < 0):
            chosen = max(0, chosen - 1)
        elif event.pressed_key("Enter") or event.character == " ":
            clip = self.clips[chosen]
            self.play_clip(session, clip)
            playing = clip.title
        else:
            return False
        viewer.chosen[numbers.SOUNDBOARD] = chosen
        self.put(numbers.SOUNDBOARD, views.soundboard(self.clips, chosen, playing), session)
        return True

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
        """Choose a magazine with the arrows and star it with Enter."""
        viewer = self.viewer(session)
        chosen = viewer.chosen.get(numbers.MINE, 0)
        if event.pressed_key("ArrowDown"):
            chosen = min(len(site.SUBJECTS) - 1, chosen + 1)
        elif event.pressed_key("ArrowUp"):
            chosen = max(0, chosen - 1)
        elif event.pressed_key("Enter") or event.character == " ":
            viewer.favourites ^= {site.SUBJECTS[chosen].number}
        else:
            return False
        viewer.chosen[numbers.MINE] = chosen
        self.put(numbers.MINE, views.mine(viewer.favourites, chosen, self.posts, self.placed), session)
        return True

    # Tags.

    def is_tag_index(self, page: int) -> bool:
        """Answer whether `page` is a subject magazine's list of its tags, N03."""
        mag = self.magazine_of_page(page)
        return mag is not None and mag.number != numbers.HOME and page == numbers.page(mag.number, numbers.TAG_INDEX)

    def tag_scope(self, page: int) -> tuple[list[Post], str, Colour, list[tuple[str, int | None]]]:
        """Answer a tag list's stories, title, colour and links: every held story's on 104, the magazine's on its N03."""
        mag = self.magazine_of_page(page)
        if page == numbers.TAGS or mag is None:
            links: list[tuple[str, int | None]] = [
                ("Front", numbers.FRONT),
                ("Popular", numbers.POPULAR_TAGS),
                ("Search", numbers.SEARCH),
                ("Mine", numbers.MINE),
            ]
            return self.posts, "Tags A to Z", Colour.GREEN, links
        return self.placed.stories.get(mag.number, []), f"{mag.short}: tags", mag.band, views.magazine_links(mag)

    def tags_screen(self, page: int, viewer: Viewer) -> Screen:
        """Draw a list of tags, A to Z, or the stories with the tag chosen."""
        posts, title, colour, links = self.tag_scope(page)
        chosen = viewer.chosen.get(page, 0)
        tag = viewer.tags.get(page)
        if tag is not None:
            items = [views.Choice(post.title, post) for post in self.posts if post.has(tag)]
            return views.choices(f"Tag: {tag}"[:30], colour, items, chosen, links, ["Enter reads, Backspace back to the tags"])
        items = [
            views.Choice(f"{name} ({count})", None) for name, count in sorted(site.tag_counts(posts), key=lambda item: item[0].lower())
        ]
        return views.choices(title, colour, items, chosen, links, ["Type a letter to jump, Enter to open"])

    def tags_input(self, session: int, page: int, event: Event) -> bool:
        """Choose a tag with the arrows or a letter and open it with Enter; then Enter reads and Backspace goes back."""
        viewer = self.viewer(session)
        chosen = viewer.chosen.get(page, 0)
        tag = viewer.tags.get(page)
        if tag is not None:
            posts = [post for post in self.posts if post.has(tag)]
            if event.pressed_key("Backspace", "Escape"):
                del viewer.tags[page]
                chosen = 0
            elif event.pressed_key("Enter") and posts:
                self.read(session, posts[min(chosen, len(posts) - 1)])
                return True
            elif event.pressed_key("ArrowDown") or (event.kind == "wheel" and event.dy > 0):
                chosen = min(max(0, len(posts) - 1), chosen + 1)
            elif event.pressed_key("ArrowUp") or (event.kind == "wheel" and event.dy < 0):
                chosen = max(0, chosen - 1)
            else:
                return False
        else:
            scope, *_ = self.tag_scope(page)
            tags = sorted(site.tag_counts(scope), key=lambda item: item[0].lower())
            character = event.character
            if event.pressed_key("ArrowDown") or (event.kind == "wheel" and event.dy > 0):
                chosen = min(max(0, len(tags) - 1), chosen + 1)
            elif event.pressed_key("ArrowUp") or (event.kind == "wheel" and event.dy < 0):
                chosen = max(0, chosen - 1)
            elif event.pressed_key("Enter") and tags:
                viewer.tags[page] = tags[min(chosen, len(tags) - 1)][0]
                chosen = 0
            elif character is not None and character.upper() in LETTERS:
                chosen = next((index for index, (name, _) in enumerate(tags) if name.upper() >= character.upper()), chosen)
            else:
                return False
        viewer.chosen[page] = chosen
        self.put(page, self.tags_screen(page, viewer), session)
        return True


def main() -> None:
    """Serve the channel, its cache beside it."""
    cache = Path(__file__).resolve().parents[2] / "cache"
    Hackaday(cache).run()
