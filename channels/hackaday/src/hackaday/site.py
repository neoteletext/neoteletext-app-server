"""The channel's map: eight magazines, as Ceefax kept a subject to a hundred.

Magazine 1 is Hackaday itself: the front page, the lists of everything,
the columns, the archive and the fun. Magazines 2 to 8 are the subjects
Hackaday writes about, sized by what it posted from 21 September to 25
October 2026 (300 posts: Making 52, Science and space 44, Software and
security 46, Games, music and art 41, Retrocomputing 37, Hardware 31,
Radio 26; 17 were columns or news alone and 6 had no subject, which
Making takes). In each subject magazine:

- N00 is its front page: the lead story, the latest, its tags;
- N01 is its feature, where it has one (the Retrocomputing Challenge,
  This Week in Security, DOOM watch);
- N02 lists every story it holds; N03 lists its tags;
- N04 to N09 are its six most used tags, each a page of its stories;
- N10 to N99 are its stories, newest first.

A page is one thing, and its sub-pages are its parts, each named in its
band (row 1) so that a receiver lists them by name: a story is "Story 1/n"
and on, "Picture", "Comments 1/m" and on, "Links" and a page for each
video; headlines (101) are a sub-page a day; the columns (110) one a
series; talking points (106), facts and figures (108), then and now (132),
help (190) and each person's page are their parts. Only a list to choose
from scrolls: a magazine's stories and its tag pages.

A post goes to the first magazine, in `RULES`' order, that one of its
categories or tags names: the Retrocomputing Challenge's subject first.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from typing import TYPE_CHECKING

from neoteletext_app.screen import Colour

from hackaday import numbers

if TYPE_CHECKING:
    from hackaday.posts import Post


@dataclass(frozen=True)
class Magazine:
    """One magazine: its digit, its name in the contents, a short name for prompts, and its colours."""

    number: int
    name: str
    short: str
    band: Colour
    ink: Colour
    feature: str | None = None

    @property
    def front(self) -> int:
        """Its front page, N00."""
        return numbers.page(self.number, 0)

    @property
    def stories_page(self) -> int:
        """The page that lists every story it holds."""
        return numbers.HEADLINES if self.number == numbers.HOME else numbers.page(self.number, numbers.ALL_STORIES)

    def story_page(self, slot: int) -> int:
        """Answer the page of its story `slot`, 0 the newest."""
        first = numbers.HOME_FIRST_STORY if self.number == numbers.HOME else numbers.FIRST_STORY
        return numbers.page(self.number, first + slot)

    @property
    def slots(self) -> int:
        """How many stories it holds."""
        return numbers.HOME_STORY_SLOTS if self.number == numbers.HOME else numbers.STORY_SLOTS


#: The magazines, 1 to 8. Each band's colour is the colour the browser
#: gives the magazine (red, green, yellow, cyan, magenta, blue, then orange
#: and grey, which Level 1 lacks: red and white).
HOME = Magazine(1, "Hackaday", "Home", Colour.RED, Colour.YELLOW)
MAGAZINES = (
    HOME,
    Magazine(2, "Retrocomputing", "Retro", Colour.GREEN, Colour.BLACK, "Retrocomputing Challenge"),
    Magazine(3, "Hardware", "Hardware", Colour.YELLOW, Colour.BLUE),
    Magazine(4, "Making", "Making", Colour.CYAN, Colour.BLUE),
    Magazine(5, "Radio and wireless", "Radio", Colour.MAGENTA, Colour.WHITE),
    Magazine(6, "Science and space", "Science", Colour.BLUE, Colour.WHITE),
    Magazine(7, "Software and security", "Software", Colour.RED, Colour.WHITE, "This Week in Security"),
    Magazine(8, "Games, music and art", "Games", Colour.WHITE, Colour.BLUE, "DOOM watch"),
)
#: The subject magazines, 2 to 8.
SUBJECTS = MAGAZINES[1:]
#: The magazine that takes a post no rule names.
CATCH_ALL = 4

#: Which magazine a category or tag puts a post in, the first that names
#: one of the post's winning. Security before software, so This Week in
#: Security is in 7 whatever else it names; columns and news alone are
#: magazine 1's.
RULES: tuple[tuple[int, frozenset[str]], ...] = (
    (
        2,
        frozenset(
            {
                "retrocomputing",
                "classic hacks",
                "history",
                "retrotechtacular",
                "nintendo game boy hacks",
                "cyberdecks",
                "2026 retrocomputing challenge",
                "retrocomputer",
            }
        ),
    ),
    (7, frozenset({"security hacks", "network hacks", "internet hacks", "this week in security"})),
    (
        5,
        frozenset(
            {
                "radio hacks",
                "wireless hacks",
                "gps hacks",
                "phone hacks",
                "cellphone hacks",
                "sdr",
                "software-defined radio",
                "ham radio",
                "amateur radio",
                "meshtastic",
            }
        ),
    ),
    (6, frozenset({"science", "space", "chemistry hacks", "medical hacks", "green hacks", "engineering", "solar hacks", "biography"})),
    (
        8,
        frozenset(
            {
                "games",
                "toy hacks",
                "nintendo hacks",
                "nintendo ds hacks",
                "nintendo wii hacks",
                "home entertainment hacks",
                "musical hacks",
                "digital audio hacks",
                "video hacks",
                "art",
                "handhelds hacks",
                "playstation hacks",
                "xbox hacks",
                "virtual reality",
            }
        ),
    ),
    (
        7,
        frozenset(
            {
                "software hacks",
                "software development",
                "linux hacks",
                "artificial intelligence",
                "machine learning",
                "reverse engineering",
                "current events",
            }
        ),
    ),
    (
        3,
        frozenset(
            {
                "microcontrollers",
                "raspberry pi",
                "arduino hacks",
                "hardware",
                "parts",
                "peripherals hacks",
                "computer hacks",
                "teardown",
                "fpga",
                "arm",
                "pcb hacks",
                "attiny hacks",
                "led hacks",
                "laptops hacks",
                "battery hacks",
                "mac hacks",
                "tablet hacks",
                "iphone hacks",
                "ipod hacks",
                "tech hacks",
            }
        ),
    ),
    (
        4,
        frozenset(
            {
                "3d printer hacks",
                "tool hacks",
                "cnc hacks",
                "repair hacks",
                "home hacks",
                "clock hacks",
                "wearable hacks",
                "how-to",
                "robots hacks",
                "laser hacks",
                "transportation hacks",
                "drone hacks",
                "car hacks",
                "misc hacks",
                "digital cameras hacks",
                "cooking hacks",
                "lifehacks",
                "skills",
                "interest",
                "reviews",
                "weapons hacks",
                "engine hacks",
            }
        ),
    ),
    (
        1,
        frozenset(
            {
                "hackaday columns",
                "podcasts",
                "hackaday links",
                "hackaday podcast",
                "news",
                "contests",
                "cons",
                "the hackaday prize",
                "rants",
                "ask hackaday",
                "fail of the week",
                "interviews",
                "roundup",
                "newsletter",
            }
        ),
    ),
)
#: Categories the site uses for its own pages, not subjects: never shown
#: (the REST API's categories, measured 25 October 2026).
HOUSEKEEPING = frozenset({"slider", "featured", "original art", "uncategorized"})
#: A tag needs this many stories to have a page.
FEWEST_FOR_A_PAGE = 2


def magazine(number: int) -> Magazine:
    """Answer magazine `number`, 1 to 8."""
    return MAGAZINES[number - 1]


def labels(post: Post) -> set[str]:
    """Answer a post's categories and tags, in lower case."""
    return {label.lower() for label in (*post.categories, *post.tags)}


def magazines_of(post: Post) -> list[Magazine]:
    """Answer every magazine a post's labels name, its own first; the catch-all when none does."""
    found: list[Magazine] = []
    names = labels(post)
    for number, members in RULES:
        if names & members and magazine(number) not in found:
            found.append(magazine(number))
    return found or [magazine(CATCH_ALL)]


@dataclass
class Placed:
    """Where the held posts are: each magazine's stories and each post's page."""

    stories: dict[int, list[Post]]
    pages: dict[int, int]
    tags: dict[int, list[tuple[str, int]]]

    def page_of(self, post: Post) -> int | None:
        """Answer the page of a held post."""
        return self.pages.get(post.id)

    def post_at(self, page: int) -> Post | None:
        """Answer the post on story page `page`."""
        found = numbers.decimal(page)
        if found is None:
            return None
        number, place = found
        mag = magazine(number)
        first = numbers.HOME_FIRST_STORY if number == numbers.HOME else numbers.FIRST_STORY
        slot = place - first
        held = self.stories.get(number, [])
        return held[slot] if 0 <= slot < min(len(held), mag.slots) else None

    def tag_pages(self, number: int) -> list[tuple[int, str, int]]:
        """Answer magazine `number`'s tag pages: each page, its tag and how many stories have it."""
        return [
            (numbers.page(number, numbers.FIRST_TAG + index), tag, count) for index, (tag, count) in enumerate(self.tags.get(number, []))
        ]

    def tag_page(self, tag: str) -> int | None:
        """Answer the page of a tag, if a magazine has one for it."""
        wanted = tag.lower()
        for mag in SUBJECTS:
            for page, name, _ in self.tag_pages(mag.number):
                if name.lower() == wanted:
                    return page
        return None


def place(posts: list[Post]) -> Placed:
    """Answer where each of `posts`, newest first, goes: its magazine's next story page, while it has room."""
    stories: dict[int, list[Post]] = {mag.number: [] for mag in MAGAZINES}
    pages: dict[int, int] = {}
    for post in posts:
        mag = magazines_of(post)[0]
        held = stories[mag.number]
        if len(held) < mag.slots:
            pages[post.id] = mag.story_page(len(held))
            held.append(post)
    tags = {mag.number: top_tags(stories[mag.number], numbers.TAG_PAGES) for mag in SUBJECTS}
    return Placed(stories, pages, tags)


def top_tags(posts: list[Post], count: int) -> list[tuple[str, int]]:
    """Answer the `count` tags most of `posts` have, with how many, leaving out those the rules already name."""
    named = {member for _, members in RULES for member in members}
    spelled: dict[str, str] = {}
    counts: Counter[str] = Counter()
    for post in posts:
        for tag in {tag.lower(): tag for tag in post.tags}.values():
            key = tag.lower()
            if key in named or key in HOUSEKEEPING:
                continue
            spelled.setdefault(key, tag)
            counts[key] += 1
    ranked = sorted(counts.items(), key=lambda item: (-item[1], item[0]))
    return [(spelled[key], number) for key, number in ranked if number >= FEWEST_FOR_A_PAGE][:count]


def tag_counts(posts: list[Post]) -> list[tuple[str, int]]:
    """Answer every tag of `posts` with how many have it, most first."""
    spelled: dict[str, str] = {}
    counts: Counter[str] = Counter()
    for post in posts:
        for tag in {tag.lower(): tag for tag in post.tags}.values():
            spelled.setdefault(tag.lower(), tag)
            counts[tag.lower()] += 1
    return [(spelled[key], number) for key, number in sorted(counts.items(), key=lambda item: (-item[1], item[0]))]
