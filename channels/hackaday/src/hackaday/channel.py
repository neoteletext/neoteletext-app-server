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
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import TYPE_CHECKING, Any

from neoteletext_app.app import App, Event, page_hears, session_hears
from neoteletext_app.charset import fold
from neoteletext_app.screen import TITLE_LIMIT, Colour, Screen, tti
from neoteletext_app.sound import RATE, chime, noise, sequence, tone

from hackaday import clips, numbers, people, site, views
from hackaday.fetch import FetchError
from hackaday.layout import BAR_ATTRIBUTE_COLUMN, BODY_ROW, TEXT_WIDTH, last_top, page, thumb, top_at, wheel, wrap
from hackaday.library import DATABASE, Library, now_utc
from hackaday.tools import ToolError, qr_tti

if TYPE_CHECKING:
    from array import array
    from collections.abc import Callable

    from hackaday.posts import Post

#: The ticker moves on this often, in seconds.
TICKER_EVERY_S = 7
#: Clocks and countdowns are redrawn this often.
MINUTE_S = 60
#: The archive's lists are read again from the store this often: cheap,
#: and the full download fills them as it goes.
ARCHIVE_EVERY_S = 600
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
#: The spin's steps, each the wait before the next: quick, then slowing
#: to a stop, about two and a half seconds in all, while the archive is
#: asked for its post.
SPIN_STEPS_S = (*(0.06,) * 10, 0.08, 0.1, 0.13, 0.17, 0.22, 0.28, 0.35, 0.45)
#: How long a person's page shows before they say their name, in seconds:
#: time to see who it is first.
NAME_DELAY_S = 2.0
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
            numbers.TALKING: "Talking points",
            numbers.FIGURES: "Facts and figures",
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
            numbers.ABOUT: "Help",
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
        cache.mkdir(parents=True, exist_ok=True)
        self.library = Library.open(cache / DATABASE)
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

    def figures_pages(self) -> list[Screen]:
        """Draw page 108's sub-pages from the posts, the server's figures and the store."""
        store = self.library.store
        archive = {
            "counts": store.counts(),
            "total": self.library.total,
            "backfill": store.state("backfill", {}),
            "download": store.state("download", {}),
            "fetched": store.fetched_since((now_utc() - timedelta(days=1)).strftime("%Y-%m-%dT%H:%M:%S")),
        }
        return views.figures(self.posts, self.figures, self.pages, archive)

    def minute(self) -> None:
        """Redraw what shows the time, and the archive's figures."""
        now = now_utc()
        self.put(numbers.FIGURES, self.figures_pages())
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
            self.columns[number] = await self.library.latest_titled(words)
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
        """Publish the regular columns' latest editions, a screen each a sub-page."""
        links: list[tuple[str, int | None]] = [
            ("Columns", numbers.COLUMNS),
            ("Security", numbers.SECURITY),
            ("Links", numbers.LINKS_COLUMN),
            ("Podcast", numbers.PODCAST),
        ]
        for number, (title, colour) in COLUMN_TITLES.items():
            self.put(number, views.column_pages(title, colour, self.columns.get(number), links))

    def publish_on_this_day(self) -> None:
        """Publish page 130's list for viewers who have not chosen."""
        self.put(numbers.ON_THIS_DAY, self.on_this_day_screen(0, 0))

    def on_this_day_items(self) -> list[views.Choice]:
        """Answer page 130's items: a post from each year."""
        return [views.Choice(f"{post.date.year} {post.title}", post) for post in self.on_this_day]

    def on_this_day_intro(self) -> list[str]:
        """Answer the lines over page 130's list."""
        intro = [f"Hackaday on {now_utc().strftime('%d %B')}, every year", "since 2004"]
        return intro if self.on_this_day else [*intro, "Reading the archive..."]

    def on_this_day_screen(self, chosen: int, top: int) -> Screen:
        """Draw page 130 with item `chosen` picked, from item `top`."""
        return views.choices(
            "On this day",
            Colour.BLUE,
            self.on_this_day_items(),
            chosen,
            [("Front", numbers.FRONT), ("Roulette", numbers.ROULETTE), ("Then+now", numbers.THEN_AND_NOW), ("Search", numbers.SEARCH)],
            self.on_this_day_intro(),
            top=top,
        )

    def publish_all(self) -> None:
        """Publish every shared page from what the library holds."""
        now = now_utc()
        posts = self.posts
        self.placed = site.place(posts)
        placed = self.placed
        self.put(numbers.FRONT, self.front_screen())
        self.put(numbers.HEADLINES, views.headlines(posts, placed, now))
        self.put(numbers.MAGAZINES, views.magazines_index(placed))
        self.put(numbers.POPULAR_TAGS, views.popular_tags(posts, placed))
        self.put(numbers.TAGS, self.tags_screen(numbers.TAGS, Viewer()))
        self.put(numbers.SEARCH, views.search("", [], 0, searching=False))
        self.put(numbers.TALKING, views.talking_points(posts, placed))
        self.put(numbers.FIGURES, self.figures_pages())
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
        self.put(numbers.ABOUT, views.help_pages())
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
            self.put(numbers.READER, views.reader_pages(posts[0]))

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
            # A place with no tag is left unpublished, so it is not listed.
            if place < len(tagged):
                _, tag, count = tagged[place]
                self.title(number, shortened(f"{tag} ({count})"))
                self.put(number, views.tag_page(mag, tag, self.posts, placed, 0))

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

    def publish_story(self, post: Post, page: int) -> None:
        """Publish a story's page: its text, picture, comments and links, each a sub-page (`views.story_pages`)."""
        mag = self.magazine_of_page(page) or site.HOME
        self.put(page, views.story_pages(post, mag, self.next_story(page), self.library.picture_pages.get(post.id), self.qr))
        if post.link not in self.qr:
            self.spawn(self.make_qr(post))

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
        page = self.placed.page_of(post)
        if page is not None:
            self.publish_story(post, page)

    async def changed(self, what: str) -> None:
        """Publish again what the library's change shows."""
        if what == "pictures":
            for post in self.posts:
                page = self.placed.page_of(post)
                if page is not None:
                    self.publish_story(post, page)
            return
        self.publish_all()

    async def news(self, post: Post) -> None:
        """Announce a new post: the newsflash on the front page, and a chime for whoever is watching it."""
        self.log(f"news: {post.title}")
        self.put(numbers.FRONT, self.front_screen())
        self.sound_object(page_hears(numbers.FRONT), CHIME, RATE, self.sounds[CHIME])
        self.play(page_hears(numbers.FRONT), SOUND_CHANNEL, CHIME, gain=70, page=numbers.FRONT)

    async def on_stats(self, stats: dict[str, Any]) -> None:
        """Show the server's figures on page 108's last sub-page as they come."""
        self.figures = stats
        self.put(numbers.FIGURES, self.figures_pages())

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

    async def on_input(self, session: int, page: int, event: Event) -> bool:
        """Answer whether the page took the input: each page that takes it has its handler; a page that scrolls scrolls."""
        handlers: dict[int, Callable[[int, Event], bool]] = {
            numbers.SEARCH: self.search_input,
            numbers.QUIZ: self.quiz_input,
            numbers.ROULETTE: self.roulette_input,
            numbers.POLL: self.poll_input,
            numbers.MINE: self.mine_input,
            numbers.SOUNDBOARD: self.soundboard_input,
            numbers.ON_THIS_DAY: self.on_this_day_input,
            numbers.DOOM: self.doom_input,
            numbers.CHALLENGE: self.challenge_input,
        }
        handler = handlers.get(page)
        if handler is not None:
            return handler(session, event)
        if page == numbers.TAGS or self.is_tag_index(page):
            return self.tags_input(session, page, event)
        return self.scroll_input(session, page, event)

    def on_this_day_input(self, session: int, event: Event) -> bool:
        """Work page 130's list: a click or Enter reads the post."""
        return self.listing_input(
            session,
            numbers.ON_THIS_DAY,
            event,
            laid=views.choices_listing(self.on_this_day_items(), self.on_this_day_intro()),
            choose=lambda index: self.read(session, self.on_this_day[index]),
            draw=self.on_this_day_screen,
        )

    def doom_input(self, session: int, event: Event) -> bool:
        """Work DOOM watch's list of ports: a click or Enter reads one."""
        return self.listing_input(
            session,
            numbers.DOOM,
            event,
            laid=views.doom_listing(self.doom_ports),
            choose=lambda index: self.read(session, self.doom_ports[index]),
            draw=lambda chosen, top: views.doom(self.doom_ports, now_utc(), chosen=chosen, top=top),
        )

    def challenge_input(self, session: int, event: Event) -> bool:
        """Work the Challenge's list of entries: a click or Enter reads one."""
        return self.listing_input(
            session,
            numbers.CHALLENGE,
            event,
            laid=views.challenge_listing(self.entries),
            choose=lambda index: self.read(session, self.entries[index]),
            draw=lambda chosen, top: views.challenge(self.entries, now_utc(), CHALLENGE_CLOSES, chosen=chosen, top=top),
        )

    # Scrolling.

    def scrolled(self, page: int, top: int) -> tuple[Screen, int] | None:
        """Answer `page` drawn from line `top`, and its last top line, if it scrolls: only the magazines' lists do."""
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

    @staticmethod
    def dragged(  # noqa: PLR0913 - the view's state and its place
        viewer: Viewer, page: int, event: Event, *, top: int, total: int, first_row: int, rows: int, shown: int | None = None
    ) -> int | None:
        """Answer where the scroll bar takes a view at `top` to, or None when `event` is not the bar's.

        A press on the block grips it where it was pressed and moves drag it;
        a press above or below it turns a screen, as a desktop scroll bar does.
        """
        cell = event.row - first_row
        if event.action == "press":
            viewer.grips.pop(page, None)
            on_bar = event.button == 0 and event.column >= BAR_ATTRIBUTE_COLUMN and 0 <= cell < rows and total > rows
            return Hackaday.bar_pressed(viewer, page, cell, top=top, total=total, rows=rows, shown=shown) if on_bar else None
        grip = viewer.grips.get(page)
        if grip is None:
            return None
        if event.action == "move" and event.buttons & LEFT_HELD:
            return top_at(cell - grip, total, rows, shown)
        del viewer.grips[page]
        return top

    @staticmethod
    def bar_pressed(  # noqa: PLR0913 - the view's state and its place
        viewer: Viewer, page: int, cell: int, *, top: int, total: int, rows: int, shown: int | None = None
    ) -> int:
        """Answer where a press on the scroll bar's `cell` takes a view at `top`, gripping the block when it is on it."""
        shown = rows if shown is None else shown
        start, length = thumb(top, total, rows, shown)
        page_by = max(1, shown - 1)
        if cell < start:
            return top - page_by
        if cell >= start + length:
            return top + page_by
        viewer.grips[page] = cell - start
        return top

    def scroll_input(self, session: int, page: int, event: Event) -> bool:
        """Scroll a page that scrolls: a screen, a line, the ends, the wheel."""
        viewer = self.viewer(session)
        top = viewer.tops.get(page, 0)
        drawn = self.scrolled(page, top)
        if drawn is None:
            return False
        _, last = drawn
        first_row, rows = BODY_ROW, views.LIST_ROWS
        if event.kind == "mouse":
            moved = self.dragged(viewer, page, event, top=top, total=last + rows, first_row=first_row, rows=rows)
        else:
            moved = self.movement(viewer, page, event, top=top, last=last, screen_lines=rows - 1)
        if moved is None:
            return False
        moved = max(0, min(last, moved))
        if moved != top:
            viewer.tops[page] = moved
            drawn = self.scrolled(page, moved)
            if drawn is not None:
                self.put(page, drawn[0], session)
        return True

    # Lists to choose from.

    def listing_input(  # noqa: PLR0913 - every way to work a list: keys, wheel, click and bar
        self,
        session: int,
        page: int,
        event: Event,
        *,
        laid: views.Listing,
        choose: Callable[[int], None],
        draw: Callable[[int, int], Screen],
    ) -> bool:
        """Work a list to choose from, the one way every list is worked.

        Up and Down choose the item before or after, Page Up and Page Down a
        screen's worth, Home and End the ends; Enter or a click on an item
        does `choose` with it; the wheel and the scroll bar move the list
        without choosing. `draw` draws the list with an item chosen, from an
        item at the top.
        """
        count = len(laid.items)
        if not count:
            return False
        viewer = self.viewer(session)
        chosen = min(viewer.chosen.get(page, 0), count - 1)
        top = min(viewer.tops.get(page, 0), laid.last_top())
        screenful = max(1, len(laid.shown(top)) - 1)
        if event.kind == "mouse":
            shown = len(laid.shown(top))
            moved = self.dragged(viewer, page, event, top=top, total=count, first_row=laid.first_row, rows=laid.rows, shown=shown)
            if moved is not None:
                top = max(0, min(laid.last_top(), moved))
            elif event.clicked and (index := laid.item_at(top, event.row)) is not None:
                viewer.chosen[page] = index
                choose(index)
                return True
            else:
                return False
        elif event.kind == "wheel":
            turned = viewer.wheel.get(page, 0) + event.dy
            notches = int(turned / NOTCH)
            viewer.wheel[page] = turned - notches * NOTCH
            top = max(0, min(laid.last_top(), top + notches))
        elif event.pressed_key("Enter"):
            choose(chosen)
            return True
        else:
            moves = {"ArrowDown": 1, "ArrowUp": -1, "PageDown": screenful, "PageUp": -screenful, "Home": -count, "End": count}
            step = next((moves[name] for name in moves if event.pressed_key(name)), None)
            if step is None:
                return False
            chosen = max(0, min(count - 1, chosen + step))
            top = laid.keeping(top, chosen)
        viewer.chosen[page], viewer.tops[page] = chosen, top
        self.put(page, draw(chosen, top), session)
        return True

    def read(self, session: int, post: Post) -> None:
        """Show a post: its story page when the channel holds it, else the archive reader."""
        found = self.placed.page_of(post)
        if found is not None:
            self.goto(session, found)
            return
        self.viewer(session).reading = post
        self.put(numbers.READER, views.reader_pages(post), session)
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
        elif event.pressed_key("Enter") and viewer.typed.strip():
            viewer.searching = True
            self.spawn(self.run_search(session, viewer.typed.strip()))
        elif event.pressed_key("Escape"):
            viewer.typed = ""
        else:
            # With nothing typed, the results are a list like any other.
            return self.listing_input(
                session,
                numbers.SEARCH,
                event,
                laid=views.choices_listing(views.search_items(viewer.results), views.SEARCH_INTRO),
                choose=lambda index: self.read(session, viewer.results[index]),
                draw=lambda chosen, top: views.search(viewer.typed, viewer.results, chosen, searching=viewer.searching, top=top),
            )
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
        viewer.tops[numbers.SEARCH] = 0
        if session in self.viewers:
            self.put(numbers.SEARCH, views.search(words + " (Enter reads)", viewer.results, 0, searching=False), session)

    # Roulette.

    def roulette_screen(self, post: Post | None, *, spinning: int | None = None) -> Screen:
        """Draw page 131: a random post from the whole archive.

        While it spins, the wheel at step `spinning`, `post` a title flicking past.
        """
        screen = page(
            "Hack roulette",
            Colour.RED,
            [("Front", numbers.FRONT), ("Archive", numbers.ON_THIS_DAY), ("Search", numbers.SEARCH), ("DOOM", numbers.DOOM)],
        )
        screen.coloured(BODY_ROW, 0, Colour.CYAN, f"One of {self.library.total or 58711} hacks since 2004")
        if spinning is not None:
            wheel(screen, 10, 19, spinning)
            screen.coloured(10, 16, Colour.YELLOW, "SPIN")
            if post is not None:
                views.put_headline(screen, 17, post.title, colour=Colour.CYAN)
        elif post is None:
            screen.centred(10, Colour.YELLOW, "Press Space to spin the wheel")
        else:
            screen.coloured(5, 0, Colour.YELLOW, f"{post.date.strftime('%d %B %Y')}")
            views.put_headline(screen, 7, post.title)
            for offset, line in enumerate(wrap(post.excerpt, TEXT_WIDTH)[:7]):
                screen.coloured(11 + offset, 0, Colour.CYAN, line)
            screen.coloured(19, 0, Colour.GREEN, views.fit(f"by {post.author}  {', '.join(post.categories)}", TEXT_WIDTH))
        screen.coloured(views.HINT_ROW, 0, Colour.GREEN, "Space spins again, Enter reads it")
        return screen

    def roulette_input(self, session: int, event: Event) -> bool:
        """Space spins; Enter reads what came up."""
        viewer = self.viewer(session)
        if event.character == " ":
            self.put(numbers.ROULETTE, self.roulette_screen(None, spinning=0), session)
            self.hear(session, SPIN)
            self.spawn(self.spin(session))
            return True
        if event.pressed_key("Enter") and viewer.roulette is not None:
            self.read(session, viewer.roulette)
            return True
        return False

    async def spin(self, session: int) -> None:
        """Turn the wheel for the viewer, titles flicking past, while the archive finds a post; it lands on that."""
        found = asyncio.ensure_future(self.library.random_post(self.random))
        for step, wait in enumerate(SPIN_STEPS_S):
            if session not in self.viewers:
                found.cancel()
                return
            passing = self.random.choice(self.posts) if self.posts else None
            self.put(numbers.ROULETTE, self.roulette_screen(passing, spinning=step), session)
            await asyncio.sleep(wait)
        try:
            post = await found
        except FetchError as error:
            print(f"hackaday: roulette: {error}", file=sys.stderr)  # noqa: T201
            post = None
        if session in self.viewers:
            self.viewer(session).roulette = post
            self.put(numbers.ROULETTE, self.roulette_screen(post), session)
            if post is not None:
                self.hear(session, RIGHT)

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
                self.play_named(session, person.clip, page, delay=NAME_DELAY_S)
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

    def play_clip(self, session: int, clip: clips.Clip, *, page: int | None = None, delay: float = 0.0) -> None:
        """Play a sound grab to one viewer, sending it the first time; bound to `page` when given.

        With a `delay`, the sound goes now, so it is there in time, and the
        play after it, if the viewer still has `page` open then.
        """
        viewer = self.viewer(session)
        if clip.handle not in viewer.clips_heard:
            self.sound_object(session_hears(session), clip.handle, clip.rate, clip.samples)
            viewer.clips_heard.add(clip.handle)
        if delay <= 0:
            self.play(session_hears(session), SOUND_CHANNEL, clip.handle, gain=90, page=page)
            return

        def later() -> None:
            if session in self.viewers and (page is None or self.page_of(session) == page):
                self.play(session_hears(session), SOUND_CHANNEL, clip.handle, gain=90, page=page)

        asyncio.get_running_loop().call_later(delay, later)

    def play_named(self, session: int, name: str, page: int, *, delay: float = 0.0) -> None:
        """Play the clip called `name` to a viewer, bound to `page`, after `delay` seconds, if the server has it."""
        clip = next((clip for clip in self.clips if clip.name == name), None)
        if clip is not None:
            self.play_clip(session, clip, page=page, delay=delay)

    def soundboard_input(self, session: int, event: Event) -> bool:
        """Work the sound board's list; Enter, Space or a click on a grab plays it."""
        viewer = self.viewer(session)

        def play(index: int) -> None:
            clip = self.clips[index]
            self.play_clip(session, clip)
            top = viewer.tops.get(numbers.SOUNDBOARD, 0)
            self.put(numbers.SOUNDBOARD, views.soundboard(self.clips, index, clip.title, top), session)

        if event.character == " " and self.clips:
            play(min(viewer.chosen.get(numbers.SOUNDBOARD, 0), len(self.clips) - 1))
            return True
        return self.listing_input(
            session,
            numbers.SOUNDBOARD,
            event,
            laid=views.soundboard_listing(self.clips),
            choose=play,
            draw=lambda chosen, top: views.soundboard(self.clips, chosen, None, top),
        )

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

    def tag_items(self, page: int, viewer: Viewer) -> tuple[list[views.Choice], list[str]]:
        """Answer a tag list's items and the line over them: the tags A to Z, or the stories with the tag chosen."""
        posts, *_ = self.tag_scope(page)
        tag = viewer.tags.get(page)
        if tag is not None:
            return [views.Choice(post.title, post) for post in self.posts if post.has(tag)], ["Enter reads, Backspace back to the tags"]
        tags = sorted(site.tag_counts(posts), key=lambda item: item[0].lower())
        return [views.Choice(f"{name} ({count})", None) for name, count in tags], ["Type a letter to jump, Enter to open"]

    def tags_screen(self, page: int, viewer: Viewer) -> Screen:
        """Draw a list of tags, A to Z, or the stories with the tag chosen, as the viewer has it."""
        _, title, colour, links = self.tag_scope(page)
        items, intro = self.tag_items(page, viewer)
        tag = viewer.tags.get(page)
        heading = f"Tag: {tag}"[:30] if tag is not None else title
        return views.choices(heading, colour, items, viewer.chosen.get(page, 0), links, intro, top=viewer.tops.get(page, 0))

    def tags_input(self, session: int, page: int, event: Event) -> bool:
        """Work a tag list: choosing a tag opens its stories, choosing a story reads it; Backspace goes back and a letter jumps."""
        viewer = self.viewer(session)
        items, intro = self.tag_items(page, viewer)
        character = event.character
        if viewer.tags.get(page) is not None and event.pressed_key("Backspace", "Escape"):
            del viewer.tags[page]
            viewer.chosen[page], viewer.tops[page] = 0, 0
        elif viewer.tags.get(page) is None and character is not None and character.upper() in LETTERS:
            jump = next((index for index, item in enumerate(items) if item.text.upper() >= character.upper()), None)
            if jump is None:
                return False
            laid = views.choices_listing(items, intro)
            viewer.chosen[page] = jump
            viewer.tops[page] = laid.keeping(viewer.tops.get(page, 0), jump)
        else:

            def choose(index: int) -> None:
                if viewer.tags.get(page) is not None:
                    post = items[index].post
                    if post is not None:
                        self.read(session, post)
                    return
                viewer.tags[page] = items[index].text.rsplit(" (", 1)[0]
                viewer.chosen[page], viewer.tops[page] = 0, 0
                self.put(page, self.tags_screen(page, viewer), session)

            def draw(chosen: int, top: int) -> Screen:
                viewer.chosen[page], viewer.tops[page] = chosen, top
                return self.tags_screen(page, viewer)

            return self.listing_input(session, page, event, laid=views.choices_listing(items, intro), choose=choose, draw=draw)
        self.put(page, self.tags_screen(page, viewer), session)
        return True


def main() -> None:
    """Serve the channel, its cache beside it."""
    cache = Path(__file__).resolve().parents[2] / "cache"
    Hackaday(cache).run()
