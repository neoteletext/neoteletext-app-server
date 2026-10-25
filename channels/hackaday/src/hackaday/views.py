"""Every page of the channel, drawn from what the library holds.

These are plain functions from data to screens, so each can be tested
without a server. What scrolls takes the first line shown; what lists
takes the item chosen.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from datetime import UTC, date, datetime, timedelta
from typing import TYPE_CHECKING, Any

from neoteletext_app.charset import codes
from neoteletext_app.screen import COLUMNS, ECHO, FASTEXT_ROW, MOUSE_BUTTONS, Attribute, Colour, Screen

from hackaday import numbers, site
from hackaday.layout import (
    BODY_ROW,
    JOLLY_WRENCHER_LARGE,
    NO_LINK,
    SCROLL_TEXT_WIDTH,
    TEXT_WIDTH,
    Line,
    Link,
    band,
    bar,
    big_text,
    concealed,
    emblem,
    flashing,
    headline,
    page,
    scroll,
    scroll_bar,
    thumb,
    wrap,
)
from hackaday.posts import Post, threaded

if TYPE_CHECKING:
    from hackaday.clips import Clip
    from hackaday.people import Person
    from hackaday.site import Magazine, Placed

#: Rows a story's text scrolls in, from `STORY_TEXT_ROW`.
STORY_TEXT_ROW = 8
STORY_ROWS = 14
#: Rows a list or long text scrolls in under the title band.
LIST_ROWS = 18
#: The row saying where the viewer is, and the row of the keys' hint.
POSITION_ROW = 21
HINT_ROW = 22
#: The front page's newsflash and ticker rows.
NEWSFLASH_ROW = 21
TICKER_ROW = 22
#: How many headlines the front page lists under its magazines.
FRONT_HEADLINES = 3
#: How many stories a magazine's front page lists after its lead.
MAGAZINE_HEADLINES = 9
#: How long a newsflash stays, after the post.
NEWSFLASH_FOR = timedelta(minutes=45)
#: Hackaday posts about every three hours (8.9 a day, measured).
POSTING_INTERVAL = timedelta(hours=3)
#: The deepest a comment is indented.
DEEPEST_INDENT = 4
#: Columns a comment is indented per level of reply.
INDENT_STEP = 2
#: The page numbers of the list in front of each line.
NUMBER_WIDTH = 4


def put_headline(  # noqa: PLR0913 - a headline's place, words, number, colours and room
    screen: Screen, row: int, title: str, *, number: str = "", colour: Colour = Colour.WHITE, last_row: int = POSITION_ROW - 1
) -> int:
    """Write a headline from `row`, after its page number when it has one, as `headline` lays it out.

    Answer the rows it took: none when it does not fit by `last_row`.
    """
    indent = NUMBER_WIDTH if number else 0
    lines = headline(title, COLUMNS - indent - 1)
    if row + len(lines) - 1 > last_row:
        return 0
    if number:
        screen.coloured(row, 0, Colour.YELLOW, number)
    for offset, line in enumerate(lines):
        screen.coloured(row + offset, indent, colour, line)
    return len(lines)


def short_date(when: datetime) -> str:
    """Answer a date as a teletext service writes it."""
    return when.strftime("%a %d %b %H:%M")


def fit(text: str, width: int) -> str:
    """Answer `text` cut to `width` characters, with a mark when cut."""
    return text if len(codes(text)) <= width else text[: width - 2] + ".."


# The front page.


def splash() -> Screen:
    """Draw the whole-page Jolly Wrencher a viewer arriving at page 100 sees first."""
    screen = Screen()
    screen.header("HACKADAY  fresh hacks every day")
    emblem(screen, 1, 3, Colour.WHITE, JOLLY_WRENCHER_LARGE)
    screen.double_height(22, 15, Colour.YELLOW, "HACKADAY")
    return screen


def front(posts: list[Post], placed: Placed, now: datetime, ticker: int, closes: datetime) -> Screen:
    """Draw page 100: the emblem, the top story, the magazines, the latest and the news."""
    screen = page(
        "",
        Colour.BLACK,
        [
            ("Headlines", numbers.HEADLINES),
            ("Magazines", numbers.MAGAZINES),
            ("Tags", numbers.POPULAR_TAGS),
            ("Challenge", numbers.CHALLENGE),
        ],
    )
    # The title band is the emblem's. The name is double width, not double
    # height: double height takes the row below across the whole page,
    # which would hide the emblem's second row.
    screen.rows[1] = bytearray([0x20] * COLUMNS)
    emblem(screen, 1, 1, Colour.WHITE)
    screen.double_width(1, 16, Colour.YELLOW, "HACKADAY")
    screen.coloured(3, 16, Colour.WHITE, "fresh hacks every day")
    screen.coloured(4, 16, Colour.CYAN, now.strftime("%a %d %b %Y"))
    if not posts:
        screen.centred(10, Colour.WHITE, "Fetching the latest hacks...")
        return screen
    due = posts[0].date + POSTING_INTERVAL
    left = due - now
    wait = "any minute now" if left <= timedelta(0) else f"in {left.seconds // 3600}h {left.seconds % 3600 // 60:02d}m"
    screen.coloured(5, 16, Colour.GREEN, f"next hack {wait}")
    challenge = closes - now
    if challenge > timedelta(0):
        screen.coloured(6, 16, Colour.MAGENTA, f"Retro Challenge {challenge.days}d {challenge.seconds // 3600}h")
    top = posts[0]
    band(screen, 8, Colour.RED, Colour.YELLOW, fit(f"TOP STORY {number_of(placed, top)}  {site.magazines_of(top)[0].name}", COLUMNS - 3))
    put_headline(screen, 9, top.title)
    for offset, line in enumerate(wrap(top.excerpt, TEXT_WIDTH)[:2]):
        screen.coloured(11 + offset, 0, Colour.CYAN, line)
    band(screen, 13, Colour.BLUE, Colour.WHITE, "THE MAGAZINES")
    for index, mag in enumerate(site.SUBJECTS):
        row, column = 14 + index // 2, (index % 2) * 20
        screen.coloured(row, column, ink_on_black(mag), f"{mag.front:03X} {mag.short}")
        screen.coloured(row, column + 13, Colour.WHITE, f"{len(placed.stories.get(mag.number, [])):3d}")
    screen.coloured(17, 20, Colour.YELLOW, "101 Headlines")
    row = 18
    for post in posts[1 : 1 + FRONT_HEADLINES]:
        taken = put_headline(screen, row, post.title, number=number_of(placed, post), last_row=NEWSFLASH_ROW - 1)
        if not taken:
            break
        row += taken
    if now - top.date <= NEWSFLASH_FOR:
        flashing(screen, NEWSFLASH_ROW, 0, Colour.RED, "NEWSFLASH")
        screen.coloured(NEWSFLASH_ROW, 12, Colour.YELLOW, fit(f"{number_of(placed, top)} {top.title}", COLUMNS - 13))
    else:
        screen.coloured(NEWSFLASH_ROW, 0, Colour.MAGENTA, "Archive 130 Roulette 131 Quiz 140 Poll 141")
    shown = posts[ticker % min(len(posts), 10)]
    band(screen, TICKER_ROW, Colour.BLUE, Colour.WHITE, fit(f"{number_of(placed, shown)} {shown.title}", COLUMNS - 4))
    screen.coloured(23, 0, Colour.GREEN, "Type a page number or press a colour")
    return screen


def ink_on_black(mag: Magazine) -> Colour:
    """Answer the colour a magazine's name is written in on black: its band's, blue being too dark to read."""
    return Colour.CYAN if mag.band == Colour.BLUE else mag.band


def number_of(placed: Placed, post: Post) -> str:
    """Answer a held post's page number as text, or the reader's for one the channel does not hold."""
    found = placed.page_of(post)
    return f"{found if found is not None else numbers.READER:03X}"


# Lists that scroll.


def scrolling(  # noqa: PLR0913 - a scrolling page: title, colours, lines, place, links and hint
    title: str,
    colour: Colour,
    lines: list[Line],
    top: int,
    links: list[Link],
    *,
    hint: str = "PgDn PgUp arrows Home End wheel",
    title_colour: Colour = Colour.WHITE,
) -> Screen:
    """Draw a page of `lines` that scrolls under its title band."""
    screen = page(title, colour, links, title_colour=title_colour)
    scroll(screen, lines, top, BODY_ROW, LIST_ROWS)
    if len(lines) > LIST_ROWS:
        last = min(len(lines), top + LIST_ROWS)
        screen.coloured(POSITION_ROW, 0, Colour.CYAN, f"Lines {top + 1}-{last} of {len(lines)}")
    screen.coloured(HINT_ROW, 0, Colour.GREEN, hint)
    return screen


def headline_lines(posts: list[Post], placed: Placed) -> list[Line]:
    """Answer one line a post: its page and title."""
    lines: list[Line] = []
    for post in posts:
        for offset, line in enumerate(headline(post.title, SCROLL_TEXT_WIDTH - NUMBER_WIDTH)):
            lines.append(Line(Colour.WHITE, f"{number_of(placed, post) if offset == 0 else '':{NUMBER_WIDTH - 1}} {line}"))
    return lines


def day_name(day: date, today: date) -> str:
    """Answer a day as a headline page names it: "Today", "Yesterday" or "Thursday 23 Oct"."""
    if day == today:
        return "Today"
    if day == today - timedelta(days=1):
        return "Yesterday"
    return day.strftime("%A %d %b")


def headlines(posts: list[Post], placed: Placed, now: datetime) -> list[Screen]:
    """Answer page 101: every story the channel holds, newest first, a sub-page a day: "Today", "Yesterday" and back."""
    links: list[Link] = [
        ("Front", numbers.FRONT),
        ("Magazines", numbers.MAGAZINES),
        ("Tags", numbers.POPULAR_TAGS),
        ("Search", numbers.SEARCH),
    ]
    days: dict[date, list[Post]] = {}
    for post in posts:
        days.setdefault(post.date.date(), []).append(post)
    screens: list[Screen] = []
    for day, held in sorted(days.items(), reverse=True):
        screens.extend(document(day_name(day, now.date()), Colour.RED, headline_lines(held, placed), links, title_colour=Colour.YELLOW))
    return screens[:MOST_SUBPAGES] or document("Headlines", Colour.RED, [Line(Colour.WHITE, "Fetching the latest hacks...")], links)


def magazines_index(placed: Placed) -> Screen:
    """Draw page 102: the magazines, how many stories each holds, and their busiest tags."""
    screen = page(
        "The magazines",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Headlines", numbers.HEADLINES), ("Tags", numbers.POPULAR_TAGS), ("Retro", numbers.page(2, 0))],
    )
    screen.coloured(BODY_ROW - 1 + 1, 0, Colour.WHITE, "100 Hackaday: front, lists, columns")
    for index, mag in enumerate(site.SUBJECTS):
        row = BODY_ROW + 1 + index * 2 + 1
        count = len(placed.stories.get(mag.number, []))
        screen.coloured(row, 0, ink_on_black(mag), fit(f"{mag.front:03X} {mag.name}", 30))
        screen.coloured(row, 31, Colour.WHITE, f"{count:3d}")
        tags = "  ".join(f"{number:03X} {tag}" for number, tag, _ in placed.tag_pages(mag.number)[:3])
        screen.coloured(row + 1, NUMBER_WIDTH, Colour.CYAN, fit(tags, COLUMNS - NUMBER_WIDTH - 1))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Each magazine: N00 front  N02 stories")
    screen.coloured(HINT_ROW + 1, 0, Colour.GREEN, "N03 tags  N04-N09 its busiest tags")
    return screen


def magazine_links(mag: Magazine) -> list[Link]:
    """Answer a subject magazine's Fastext: home, its stories, its tags and the next magazine."""
    following = site.SUBJECTS[(site.SUBJECTS.index(mag) + 1) % len(site.SUBJECTS)]
    return [
        ("Home", numbers.FRONT),
        ("Stories", mag.stories_page),
        ("Tags", numbers.page(mag.number, numbers.TAG_INDEX)),
        (following.short, following.front),
    ]


def magazine_front(mag: Magazine, placed: Placed, feature: str | None) -> Screen:
    """Draw a subject magazine's front page: its lead story, the latest, its tags and its feature."""
    screen = page(mag.name, mag.band, magazine_links(mag), title_colour=mag.ink)
    posts = placed.stories.get(mag.number, [])
    if not posts:
        screen.centred(10, Colour.WHITE, "Fetching the latest hacks...")
        return screen
    lead = posts[0]
    taken = put_headline(screen, BODY_ROW, lead.title, number=number_of(placed, lead), colour=Colour.YELLOW)
    for offset, line in enumerate(wrap(lead.excerpt, TEXT_WIDTH)[: 4 - taken]):
        screen.coloured(BODY_ROW + taken + offset, 0, Colour.CYAN, line)
    band(screen, 7, mag.band, mag.ink, f"LATEST: {len(posts)} STORIES ON {mag.stories_page:03X}")
    row = 8
    for post in posts[1 : 1 + MAGAZINE_HEADLINES]:
        taken = put_headline(screen, row, post.title, number=number_of(placed, post), last_row=17)
        if not taken:
            break
        row += taken
    tags = placed.tag_pages(mag.number)
    if tags:
        band(screen, 18, Colour.BLUE, Colour.WHITE, f"BY TAG, ALL ON {numbers.page(mag.number, numbers.TAG_INDEX):03X}")
        for index, (number, tag, count) in enumerate(tags):
            row, column = 19 + index // 2, (index % 2) * 20
            screen.coloured(row, column, Colour.YELLOW, f"{number:03X}")
            screen.coloured(row, column + NUMBER_WIDTH, Colour.WHITE, fit(f"{tag} {count}", 15))
    if feature:
        screen.coloured(HINT_ROW, 0, Colour.MAGENTA, fit(f"{numbers.page(mag.number, numbers.FEATURE):03X} {feature}", TEXT_WIDTH))
    return screen


def magazine_stories(mag: Magazine, placed: Placed, top: int) -> Screen:
    """Draw a magazine's N02: every story it holds, newest first, which scrolls."""
    posts = placed.stories.get(mag.number, [])
    lines = headline_lines(posts, placed) or [Line(Colour.WHITE, "Nothing here yet")]
    return scrolling(f"{mag.short}: every story", mag.band, lines, top, magazine_links(mag), title_colour=mag.ink)


def tag_lines(posts: list[Post], placed: Placed, tag: str) -> list[Line]:
    """Answer the held stories with `tag`, newest first, as lines with their pages."""
    return headline_lines([post for post in posts if post.has(tag)], placed)


def tag_page(mag: Magazine, tag: str, posts: list[Post], placed: Placed, top: int) -> Screen:
    """Draw one of a magazine's tag pages: every held story with the tag, in any magazine."""
    lines = tag_lines(posts, placed, tag) or [Line(Colour.WHITE, "No story has this tag now")]
    return scrolling(fit(f"Tag: {tag}", 30), mag.band, lines, top, magazine_links(mag), title_colour=mag.ink)


def popular_tags(posts: list[Post], placed: Placed) -> Screen:
    """Draw page 103: the tags the held stories have most, each with its page where a magazine gives it one."""
    screen = page(
        "Popular tags",
        Colour.GREEN,
        [("Front", numbers.FRONT), ("A to Z", numbers.TAGS), ("Magazines", numbers.MAGAZINES), ("Search", numbers.SEARCH)],
        title_colour=Colour.BLACK,
    )
    ranked = site.tag_counts(posts)[: LIST_ROWS - 1]
    most = ranked[0][1] if ranked else 1
    screen.coloured(BODY_ROW, 0, Colour.CYAN, f"Tags of the last {len(posts)} stories")
    for offset, (tag, count) in enumerate(ranked):
        row = BODY_ROW + 1 + offset
        found = placed.tag_page(tag)
        screen.coloured(row, 0, Colour.YELLOW, f"{found:03X}" if found is not None else "104")
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(tag, 17))
        screen.coloured(row, 22, Colour.CYAN, f"{count:3d}")
        bar(screen, row, 26, 12, count, most, Colour.GREEN)
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Every tag, A to Z, on 104")
    return screen


def column_posts(posts: list[Post]) -> list[Post]:
    """Answer the held posts that are Hackaday's columns, newest first."""
    return [post for post in posts if post.has("hackaday columns") or post.has("podcasts") or post.has("hackaday links")]


#: Hackaday's columns, each the label its editions carry, in the order
#: page 110's sub-pages give them.
SERIES = (
    ("This Week in Security", "this week in security"),
    ("Hackaday Links", "hackaday links"),
    ("Hackaday Podcast", "hackaday podcast"),
    ("Newsletter", "newsletter"),
    ("Ask Hackaday", "ask hackaday"),
    ("Retrotechtacular", "retrotechtacular"),
)


def columns_index(posts: list[Post], placed: Placed) -> list[Screen]:
    """Answer page 110: a sub-page for each column, its editions with their pages, and one for the other columns."""
    links: list[Link] = [
        ("Front", numbers.FRONT),
        ("Links", numbers.LINKS_COLUMN),
        ("Podcast", numbers.PODCAST),
        ("Security", numbers.SECURITY),
    ]
    columns = column_posts(posts)
    screens: list[Screen] = []
    shown: set[int] = set()
    for name, label in SERIES:
        editions = [post for post in columns if post.has(label) or post.title.lower().startswith(name.lower())]
        shown.update(post.id for post in editions)
        if editions:
            screens.extend(document(name, Colour.RED, headline_lines(editions, placed), links, title_colour=Colour.YELLOW, most=2))
    others = [post for post in columns if post.id not in shown]
    if others:
        screens.extend(document("Other columns", Colour.RED, headline_lines(others, placed), links, title_colour=Colour.YELLOW, most=4))
    return screens or document("Columns", Colour.RED, [Line(Colour.WHITE, "Reading the columns...")], links, title_colour=Colour.YELLOW)


# Documents in sub-pages: a page is one thing, its sub-pages its parts
# (`site.py`), each named in its band, so a receiver lists them by name.


#: The most sub-pages one page may have (docs/applications.md section 3.2).
MOST_SUBPAGES = 64
#: A story's text and its comments each take at most this many sub-pages;
#: with the picture and up to four link pages they stay within the most.
TEXT_SUBPAGES = 28
#: Rows of a document's text under its band, and of a story's first
#: sub-page under its headline, byline and labels.
DOCUMENT_ROWS = 19
STORY_FIRST_ROW = 8
STORY_FIRST_ROWS = 14
#: Where a later sub-page of a story starts its text, under its headline.
STORY_MORE_ROW = 5
STORY_MORE_ROWS = 17
#: The links a story's QR pages are made for: the article and its videos.
MOST_VIDEOS = 3


def chunks(lines: list[Line], first: int, rest: int) -> list[list[Line]]:
    """Answer `lines` in screens: `first` lines on the first, `rest` on each after; one empty screen for none."""
    if len(lines) <= first:
        return [lines]
    return [lines[:first], *(lines[start : start + rest] for start in range(first, len(lines), rest))]


def part_name(name: str, place: int, count: int) -> str:
    """Answer a sub-page's name in its band: `name`, with its place among `count` when there are several."""
    return name if count == 1 else f"{name} {place}/{count}"


def document(  # noqa: PLR0913 - a document's name, colours, text, links and room
    name: str,
    colour: Colour,
    lines: list[Line],
    links: list[Link],
    *,
    title_colour: Colour = Colour.WHITE,
    most: int = MOST_SUBPAGES,
) -> list[Screen]:
    """Answer `lines` as a page's sub-pages, a screen each, each band naming it and its place: "Columns 2/3"."""
    screens_of = chunks(lines, DOCUMENT_ROWS, DOCUMENT_ROWS)[:most]
    screens: list[Screen] = []
    for place, part in enumerate(screens_of, start=1):
        screen = page(part_name(name, place, len(screens_of)), colour, links, title_colour=title_colour)
        for offset, line in enumerate(part):
            if line.text:
                screen.coloured(BODY_ROW + offset, 0, line.colour, line.text)
        screens.append(screen)
    return screens


# A story: one page, its sub-pages its text, its picture, its comments and
# its links, in that order.


def story_links(mag: Magazine, next_page: int) -> list[Link]:
    """Answer a story's Fastext: its magazine, the magazine's stories, the next story and home."""
    return [(mag.short, mag.front), ("Stories", mag.stories_page), ("Next", next_page), ("Home", numbers.FRONT)]


def article_lines(post: Post, width: int = TEXT_WIDTH) -> list[Line]:
    """Answer a post's text as lines of `width`: headings yellow, quotes cyan, items marked."""
    lines: list[Line] = []
    for block in post.article.blocks:
        if block.kind == "heading":
            lines.extend(Line(Colour.YELLOW, line) for line in wrap(block.text, width))
        elif block.kind == "item":
            for number, line in enumerate(wrap(block.text, width - 2)):
                lines.append(Line(Colour.WHITE, ("* " if number == 0 else "  ") + line))
        elif block.kind in {"quote", "code"}:
            lines.extend(Line(Colour.CYAN, line) for line in wrap(block.text, width))
        else:
            lines.extend(Line(Colour.WHITE, line) for line in wrap(block.text, width))
        lines.append(Line(Colour.WHITE, ""))
    while lines and not lines[-1].text:
        lines.pop()
    return lines


def story_text(post: Post, colours: tuple[Colour, Colour], links: list[Link], *, name: str = "Story") -> list[Screen]:
    """Answer a story's text as sub-pages in a band of `colours` (band, ink).

    The first has its headline, who and when; each after has its headline small.
    """
    parts = chunks(article_lines(post), STORY_FIRST_ROWS, STORY_MORE_ROWS)[:TEXT_SUBPAGES]
    screens: list[Screen] = []
    for place, part in enumerate(parts, start=1):
        screen = page(part_name(name, place, len(parts)), colours[0], links, title_colour=colours[1])
        if place == 1:
            put_headline(screen, BODY_ROW, post.title, colour=Colour.YELLOW)
            screen.coloured(5, 0, Colour.CYAN, fit(f"by {post.author}, {short_date(post.date)}", TEXT_WIDTH))
            screen.coloured(6, 0, Colour.GREEN, fit(", ".join(post.categories + post.tags), TEXT_WIDTH))
            first_row = STORY_FIRST_ROW
        else:
            screen.coloured(BODY_ROW, 0, Colour.YELLOW, fit(post.title, TEXT_WIDTH))
            first_row = STORY_MORE_ROW
        for offset, line in enumerate(part):
            if line.text:
                screen.coloured(first_row + offset, 0, line.colour, line.text)
        if place == len(parts):
            screen.coloured(HINT_ROW, 0, Colour.CYAN, f"{post.article.words} words  {post.comment_count} comments")
        screens.append(screen)
    return screens


def picture(post: Post, mag: Magazine, links: list[Link], tti: str | None) -> Screen:
    """Answer a story's picture sub-page: its lead picture in mosaics under the band, or a word that it is coming."""
    screen = page("Picture", mag.band, links, title_colour=mag.ink)
    if tti is None:
        screen.centred(10, Colour.WHITE, "This picture is being drawn")
        screen.centred(12, Colour.CYAN, "in teletext mosaics: back soon")
        for offset, line in enumerate(headline(post.title, 36)):
            screen.centred(14 + offset, Colour.YELLOW, line)
        return screen
    # The converter draws rows 1 to 24; the band keeps row 1 and its row
    # below, which names this sub-page, and the picture shows from row 3.
    screen.load_tti(tti, first_row=BODY_ROW, last_row=FASTEXT_ROW)
    screen.fastext([target for _, target in links])
    return screen


def comment_lines(post: Post) -> list[Line]:
    """Answer every one of a post's comments as lines, replies indented under what they answer."""
    lines: list[Line] = []
    for depth, comment in threaded(post.comments):
        indent = " " * (min(depth, DEEPEST_INDENT) * INDENT_STEP)
        width = TEXT_WIDTH - len(indent)
        lines.append(Line(Colour.CYAN, indent + fit(f"{comment.author}, {short_date(comment.date)}", width)))
        lines.extend(Line(Colour.WHITE, indent + line) for line in wrap(comment.text, width))
        lines.append(Line(Colour.WHITE, ""))
    while lines and not lines[-1].text:
        lines.pop()
    return lines or [Line(Colour.WHITE, "No comments yet: be the first on"), Line(Colour.WHITE, "hackaday.com")]


def links_page(post: Post, links: list[Link], qr: str | None, what: str, name: str) -> Screen:
    """Answer one of a story's link sub-pages: a QR code a phone can follow, and what it leads to."""
    screen = page(name, Colour.MAGENTA, links)
    if qr is not None:
        screen.load_tti(qr, first_row=BODY_ROW, last_row=20)
    side = 25
    screen.coloured(BODY_ROW + 1, side, Colour.YELLOW, what)
    for offset, line in enumerate(wrap("Scan the code with a phone to open it", COLUMNS - side - 2)):
        screen.coloured(BODY_ROW + 3 + offset, side, Colour.WHITE, line)
    for offset, line in enumerate(headline(post.title, COLUMNS - side - 2)):
        screen.coloured(BODY_ROW + 8 + offset, side, Colour.CYAN, line)
    for offset, (text, _) in enumerate(post.article.links[:3]):
        screen.coloured(21 + offset - 1, 0, Colour.GREEN, fit(f"Link: {text}", TEXT_WIDTH))
    return screen


def story_pages(post: Post, mag: Magazine, next_page: int, picture_tti: str | None, codes: dict[str, str]) -> list[Screen]:
    """Answer a story's page as its sub-pages: "Story 1/n" and on, "Picture", "Comments 1/m" and on, then "Links" and each video's."""
    links = story_links(mag, next_page)
    screens = story_text(post, (mag.band, mag.ink), links)
    screens.append(picture(post, mag, links, picture_tti))
    screens.extend(document("Comments", Colour.GREEN, comment_lines(post), links, title_colour=Colour.BLACK, most=TEXT_SUBPAGES))
    targets = [
        ("The full story", post.link),
        *((f"Video {number}", video) for number, video in enumerate(post.article.videos[:MOST_VIDEOS], start=1)),
    ]
    for what, target in targets:
        screens.append(links_page(post, links, codes.get(target), what, "Links" if what == "The full story" else what))
    for screen in screens:
        screen.index_link(mag.front)
    return screens[:MOST_SUBPAGES]


# Lists to choose from.


@dataclass(frozen=True)
class Choice:
    """An item of a list to choose from, and the post it leads to."""

    text: str
    post: Post | None


#: The width of a list's items: a chosen item's band takes three cells
#: (its background, then its colour) before the text, which stops a cell
#: short of the scroll bar's colour.
LISTING_WIDTH = SCROLL_TEXT_WIDTH - 1


@dataclass(frozen=True)
class Listing:
    """A list to choose from, as it is laid out: the rows it has and each item's lines.

    Every list the viewer chooses from is one of these, drawn by `draw_listing`
    and worked by the channel's `listing_input`: the arrows, the wheel, a
    click on an item, and the scroll bar.
    """

    first_row: int
    rows: int
    items: list[list[str]]

    def shown(self, top: int) -> list[tuple[int, int, list[str]]]:
        """Answer the items that fit from item `top`: each one's index, first row and lines."""
        placed: list[tuple[int, int, list[str]]] = []
        row = self.first_row
        for index in range(max(0, top), len(self.items)):
            lines = self.items[index]
            if row + len(lines) > self.first_row + self.rows:
                break
            placed.append((index, row, lines))
            row += len(lines)
        return placed

    def item_at(self, top: int, row: int) -> int | None:
        """Answer the item drawn on `row` when the list starts at `top`."""
        for index, first, lines in self.shown(top):
            if first <= row < first + len(lines):
                return index
        return None

    def last_top(self) -> int:
        """Answer the first item shown when the list is scrolled to its end."""
        top = len(self.items)
        while top > 0 and self.shown(top - 1)[-1:] and self.shown(top - 1)[-1][0] == len(self.items) - 1:
            top -= 1
        return min(top, max(0, len(self.items) - 1))

    def keeping(self, top: int, chosen: int) -> int:
        """Answer the first item to show so that `chosen` is in view, moving from `top` no more than it must."""
        if chosen < top:
            return chosen
        while chosen not in {index for index, _, _ in self.shown(top)} and top < chosen:
            top += 1
        return top


def listing(texts: list[str], first_row: int, last_row: int = POSITION_ROW - 1) -> Listing:
    """Lay out a list of `texts` from `first_row` to `last_row`, each a headline beside the scroll bar."""
    return Listing(first_row, last_row - first_row + 1, [headline(text, LISTING_WIDTH) for text in texts])


def draw_listing(screen: Screen, laid: Listing, top: int, chosen: int) -> None:
    """Draw a list from item `top`, the chosen item on a band, with a scroll bar when not every item fits."""
    shown = laid.shown(top)
    for index, row, lines in shown:
        for offset, line in enumerate(lines):
            if index == chosen:
                band(screen, row + offset, Colour.YELLOW, Colour.BLUE, line)
            else:
                screen.coloured(row + offset, 0, Colour.WHITE, line)
    if len(shown) < len(laid.items):
        scroll_bar(screen, laid.first_row, laid.rows, thumb(top, len(laid.items), laid.rows, len(shown)))
    else:
        # Every item is clicked for, even with no bar to pull.
        screen.modes(screen.input_modes | MOUSE_BUTTONS)


def choices_listing(items: list[Choice], intro: list[str]) -> Listing:
    """Lay out `choices`' items under its introduction."""
    return listing([item.text for item in items], BODY_ROW + len(intro) + (1 if intro else 0))


def choices(  # noqa: PLR0913, PLR0917 - a list page: title, colour, items, choice, links, introduction and place
    title: str, colour: Colour, items: list[Choice], chosen: int, links: list[Link], intro: list[str], *, top: int = 0
) -> Screen:
    """Draw a list to choose from, from item `top`: the arrows, the wheel, a click or the scroll bar, and Enter."""
    screen = page(title, colour, links)
    for offset, line in enumerate(intro):
        screen.coloured(BODY_ROW + offset, 0, Colour.CYAN, line)
    draw_listing(screen, choices_listing(items, intro), top, chosen)
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Up Down or click to choose, Enter reads")
    return screen


def reader_pages(post: Post) -> list[Screen]:
    """Answer page 133 for a post from the archive: its text as sub-pages, "Archive 1/n" and on."""
    links: list[Link] = [("Front", numbers.FRONT), ("Day", numbers.ON_THIS_DAY), ("Roulette", numbers.ROULETTE), ("Search", numbers.SEARCH)]
    return story_text(post, (Colour.BLUE, Colour.WHITE), links, name="Archive")


# Fun and figures.


#: Where DOOM watch's and the Challenge's lists start, under their figures.
DOOM_LIST_ROW = 10
CHALLENGE_LIST_ROW = 11


def doom_listing(ports: list[Post]) -> Listing:
    """Lay out DOOM watch's ports, each with its year."""
    return listing([f"{post.date.year} {post.title}" for post in ports], DOOM_LIST_ROW)


def doom(ports: list[Post], now: datetime, *, chosen: int = 0, top: int = 0) -> Screen:
    """Draw page 801, Games' feature: days since the last port of DOOM, and every port, a list to read from."""
    games = site.magazine(8)
    screen = page(
        "DOOM watch", Colour.RED, [("Home", numbers.FRONT), ("Games", games.front), ("Roulette", numbers.ROULETTE), ("Quiz", numbers.QUIZ)]
    )
    if not ports:
        screen.centred(10, Colour.WHITE, "Counting the ports of DOOM...")
        return screen
    days = max(0, (now - ports[0].date).days)
    screen.coloured(BODY_ROW + 1, 0, Colour.YELLOW, "DAYS SINCE THE LAST")
    screen.coloured(BODY_ROW + 2, 0, Colour.YELLOW, "PORT OF DOOM")
    big_text(screen, BODY_ROW + 1, 22, f"{min(days, 999)}", Colour.RED)
    put_headline(screen, BODY_ROW + 4, ports[0].title, colour=Colour.CYAN, last_row=BODY_ROW + 5)
    band(screen, DOOM_LIST_ROW - 1, Colour.RED, Colour.YELLOW, f"Ports: {len(ports)} found")
    draw_listing(screen, doom_listing(ports), top, chosen)
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Can it run DOOM? Click one to read it")
    return screen


def statistics(posts: list[Post]) -> Screen:
    """Draw page 108's first sub-page: the stories by magazine, by hour of posting, and the comments."""
    screen = page(
        "Statistics",
        Colour.GREEN,
        [("Front", numbers.FRONT), ("Talking", numbers.TALKING), ("Headlines", numbers.HEADLINES), ("Magazines", numbers.MAGAZINES)],
        title_colour=Colour.BLACK,
    )
    counts = Counter(site.magazines_of(post)[0].number for post in posts)
    most = max(counts.values(), default=1)
    screen.coloured(BODY_ROW, 0, Colour.YELLOW, f"The last {len(posts)} stories by magazine")
    for index, mag in enumerate(site.MAGAZINES):
        row = BODY_ROW + 1 + index
        screen.coloured(row, 0, ink_on_black(mag), fit(f"{mag.front:03X} {mag.short}", 20))
        screen.coloured(row, 21, Colour.CYAN, f"{counts.get(mag.number, 0):3d}")
        bar(screen, row, 25, 13, counts.get(mag.number, 0), most, Colour.GREEN)
    hours = Counter(post.date.hour // 3 for post in posts)
    peak = max(hours.values(), default=1)
    screen.coloured(14, 0, Colour.YELLOW, "When they post, UTC")
    for block in range(8):
        row = 15 + block // 2
        column = 0 if block % 2 == 0 else 20
        screen.coloured(row, column, Colour.WHITE, f"{block * 3:02d}-{block * 3 + 3:02d}")
        bar(screen, row, column + 7, 11, hours.get(block, 0), peak, Colour.MAGENTA)
    total = sum(post.comment_count for post in posts)
    screen.coloured(20, 0, Colour.CYAN, f"{total} comments on the last {len(posts)} stories")
    return screen


def writers(posts: list[Post]) -> Screen:
    """Draw page 108's second sub-page: who writes Hackaday, and how much."""
    screen = page(
        "Writers",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Talking", numbers.TALKING), ("People", numbers.PEOPLE), ("Headlines", numbers.HEADLINES)],
    )
    counts = Counter(post.author for post in posts).most_common(17)
    most = counts[0][1] if counts else 1
    screen.coloured(BODY_ROW, 0, Colour.YELLOW, f"Of the last {len(posts)} stories")
    for offset, (author, count) in enumerate(counts):
        row = BODY_ROW + 1 + offset
        screen.coloured(row, 0, Colour.WHITE, fit(author, 18))
        screen.coloured(row, 19, Colour.CYAN, f"{count:3d}")
        bar(screen, row, 23, 15, count, most, Colour.YELLOW)
    return screen


def discussed(posts: list[Post], placed: Placed) -> Screen:
    """Draw page 106's first sub-page: the most argued-about stories."""
    screen = page(
        "Most discussed",
        Colour.RED,
        [("Front", numbers.FRONT), ("Figures", numbers.FIGURES), ("Headlines", numbers.HEADLINES), ("Magazines", numbers.MAGAZINES)],
    )
    ranked = sorted(posts, key=lambda post: post.comment_count, reverse=True)[:9]
    most = ranked[0].comment_count if ranked and ranked[0].comment_count else 1
    row = BODY_ROW
    for post in ranked:
        taken = put_headline(screen, row, post.title, number=number_of(placed, post), last_row=HINT_ROW - 2)
        if not taken:
            break
        bar(screen, row + taken, NUMBER_WIDTH, 26, post.comment_count, most, Colour.RED)
        screen.coloured(row + taken, 33, Colour.CYAN, f"{post.comment_count:4d}")
        row += taken + 1
    return screen


def comments_wall_lines(posts: list[Post], placed: Placed) -> list[Line]:
    """Answer the newest comments on every story as lines: who, on which story, and the start of what they said."""
    every = sorted(((comment, post) for post in posts for comment in post.comments), key=lambda pair: pair[0].date, reverse=True)[:60]
    lines: list[Line] = []
    for comment, post in every:
        lines.append(Line(Colour.CYAN, fit(f"{comment.author} on {number_of(placed, post)}", SCROLL_TEXT_WIDTH)))
        lines.extend(Line(Colour.WHITE, line) for line in wrap(comment.text, SCROLL_TEXT_WIDTH)[:4])
        lines.append(Line(Colour.WHITE, ""))
    return lines or [Line(Colour.WHITE, "Reading the comments...")]


def figures(posts: list[Post], stats: dict[str, Any], titles: dict[int, str], archive: dict[str, Any] | None = None) -> list[Screen]:
    """Answer page 108: "Statistics", "Writers", "Behind the scenes" (the server's figures) and "The archive" (the copy kept)."""
    return [statistics(posts), writers(posts), server_stats(stats, titles), archive_figures(archive or {})]


#: The full download's collections as the archive page names them, in its order (`sync.BACKFILL`).
BACKFILL_NAMES = ("writers", "categories", "tags", "posts", "pages", "media records", "comments")


def archive_figures(archive: dict[str, Any]) -> Screen:
    """Draw page 108's last sub-page: what of Hackaday's archive is kept here, and what keeping it up to date costs."""
    screen = page(
        "The archive",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Search", numbers.SEARCH), ("Roulette", numbers.ROULETTE), ("Help", numbers.ABOUT)],
    )
    counts: dict[str, int] = archive.get("counts") or {}
    rows = [
        (Colour.YELLOW, f"Posts      {figure(counts.get('posts')):>9,} of {figure(archive.get('total')):,}"),
        (Colour.WHITE, f"Comments   {figure(counts.get('comments')):>9,}"),
        (Colour.WHITE, f"Tags+cats  {figure(counts.get('terms')):>9,}"),
        (Colour.WHITE, f"Writers    {figure(counts.get('users')):>9,}"),
        (Colour.WHITE, f"Media      {figure(counts.get('media')):>9,}"),
        (Colour.WHITE, f"Pictures   {figure(counts.get('pictures')):>9,} made into pages"),
    ]
    for offset, (colour, text) in enumerate(rows):
        screen.coloured(BODY_ROW + offset, 0, colour, fit(text, TEXT_WIDTH))
    place: dict[str, Any] = archive.get("backfill") or {}
    if place.get("done"):
        progress = f"Whole archive read, {str(place.get('finished_at', ''))[:10]}"
    else:
        stage = figure(place.get("stage"))
        name = BACKFILL_NAMES[stage] if stage < len(BACKFILL_NAMES) else ""
        month = str(place.get("window") or "")[:7]
        progress = f"Reading {name} {month} page {figure(place.get('page')) or 1}"
    screen.coloured(BODY_ROW + 7, 0, Colour.CYAN, fit(progress, TEXT_WIDTH))
    band(screen, 12, Colour.BLUE, Colour.YELLOW, "Asked of hackaday.com, 24 hours")
    fetched: list[tuple[str, int, int]] = archive.get("fetched") or []
    for offset, (kind, requests, wire) in enumerate(fetched[:6]):
        screen.coloured(13 + offset, 0, Colour.WHITE, fit(kind, 12))
        screen.coloured(13 + offset, 13, Colour.CYAN, f"{requests:>6,} asks")
        screen.coloured(13 + offset, 27, Colour.GREEN, f"{amount(wire):>10}")
    if fetched:
        total = sum(wire for _, _, wire in fetched)
        screen.coloured(20, 0, Colour.YELLOW, fit(f"In all {sum(n for _, n, _ in fetched):,} asks, {amount(total)}", TEXT_WIDTH))
    screen.coloured(21, 0, Colour.GREEN, "All of it in SQLite, read from there")
    return screen


def talking_points(posts: list[Post], placed: Placed) -> list[Screen]:
    """Answer page 106: "Most discussed", then the latest comments on every story, "Comments 1/n" and on."""
    links: list[Link] = [
        ("Front", numbers.FRONT),
        ("Headlines", numbers.HEADLINES),
        ("Figures", numbers.FIGURES),
        ("Magazines", numbers.MAGAZINES),
    ]
    return [
        discussed(posts, placed),
        *document("Comments", Colour.GREEN, comments_wall_lines(posts, placed), links, title_colour=Colour.BLACK, most=8),
    ]


def column_pages(title: str, colour: Colour, post: Post | None, links: list[Link]) -> list[Screen]:
    """Answer a regular column's latest edition as sub-pages, a screen each: "Hackaday Links 1/4" and on."""
    if post is None:
        return document(title, colour, [Line(Colour.WHITE, "Looking for the latest edition...")], links)
    lines = [Line(Colour.YELLOW, line) for line in headline(post.title, TEXT_WIDTH)]
    lines.append(Line(Colour.CYAN, fit(f"by {post.author}, {short_date(post.date)}", TEXT_WIDTH)))
    lines.append(Line(Colour.WHITE, ""))
    lines.extend(article_lines(post))
    return document(title, colour, lines, links, most=TEXT_SUBPAGES)


def challenge_listing(entries: list[Post]) -> Listing:
    """Lay out the Challenge's entries written up."""
    return listing([post.title.removeprefix("2026 Retrocomputing Challenge: ") for post in entries], CHALLENGE_LIST_ROW)


def challenge(entries: list[Post], now: datetime, closes: datetime, *, chosen: int = 0, top: int = 0) -> Screen:
    """Draw page 201, Retrocomputing's feature: the Challenge, its countdown and the entries written up."""
    retro = site.magazine(2)
    screen = page(
        "Retro Challenge",
        retro.band,
        [("Home", numbers.FRONT), ("Retro", retro.front), ("Poll", numbers.POLL), ("Then+now", numbers.THEN_AND_NOW)],
        title_colour=retro.ink,
    )
    screen.coloured(BODY_ROW, 0, Colour.YELLOW, "2026 Hackaday Retrocomputing Challenge")
    left = closes - now
    if left > timedelta(0):
        screen.coloured(BODY_ROW + 1, 0, Colour.WHITE, "Entries close Tuesday 27 October in")
        hours, minutes = divmod(left.seconds // 60, 60)
        big_text(screen, BODY_ROW + 3, 2, f"{left.days}:{hours:02d}:{minutes:02d}", Colour.GREEN)
        screen.coloured(BODY_ROW + 6, 2, Colour.CYAN, "days    hours    minutes")
    else:
        screen.coloured(BODY_ROW + 1, 0, Colour.WHITE, "Entries have closed: judging is on")
    band(screen, CHALLENGE_LIST_ROW - 1, Colour.MAGENTA, Colour.WHITE, f"Entries written up: {len(entries)}")
    draw_listing(screen, challenge_listing(entries), top, chosen)
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Up Down or click to choose, Enter reads")
    return screen


def dated_lines(posts: list[Post]) -> list[Line]:
    """Answer posts as headline lines, each after its date."""
    lines: list[Line] = []
    for post in posts:
        for offset, line in enumerate(headline(post.title, TEXT_WIDTH - 7)):
            date = post.date.strftime("%d %b") if offset == 0 else ""
            lines.append(Line(Colour.WHITE, f"{date:6} {line}"))
    return lines


def then_and_now(first: list[Post], latest: list[Post]) -> list[Screen]:
    """Answer page 132's sub-pages: "Then", Hackaday's first posts, and "Now", today's."""
    links: list[Link] = [
        ("Front", numbers.FRONT),
        ("Archive", numbers.ON_THIS_DAY),
        ("Roulette", numbers.ROULETTE),
        ("Challenge", numbers.CHALLENGE),
    ]
    first_year = first[0].date.year if first else 2004
    then = document(
        f"Then: {first_year}",
        Colour.YELLOW,
        dated_lines(first) or [Line(Colour.WHITE, "Reading the archive...")],
        links,
        title_colour=Colour.BLUE,
        most=1,
    )
    now = document(f"Now: {datetime.now(UTC).year}", Colour.GREEN, dated_lines(latest[:9]), links, title_colour=Colour.BLACK, most=1)
    return then + now


#: The help's sub-pages: each one's name and lines.
HELP: tuple[tuple[str, tuple[tuple[Colour, str], ...]], ...] = (
    (
        "Getting about",
        (
            (Colour.YELLOW, "Hackaday, live, as 1980s teletext"),
            (Colour.WHITE, "Type a page number, press a coloured"),
            (Colour.WHITE, "key for its prompt below, or click a"),
            (Colour.WHITE, "number or a prompt."),
            (Colour.WHITE, ""),
            (Colour.CYAN, "A page can have sub-pages, its parts:"),
            (Colour.CYAN, "a story's text, picture, comments and"),
            (Colour.CYAN, "links. The page list names them, and"),
            (Colour.CYAN, "the sub-page arrows step through them."),
            (Colour.WHITE, ""),
            (Colour.GREEN, "Lists scroll with the wheel or their"),
            (Colour.GREEN, "bar; click an item to open it."),
        ),
    ),
    (
        "The magazines",
        (
            (Colour.YELLOW, "1xx Hackaday: lists, columns, people,"),
            (Colour.YELLOW, "    the archive, the fun"),
            (Colour.GREEN, "2xx Retrocomputing  3xx Hardware"),
            (Colour.CYAN, "4xx Making          5xx Radio"),
            (Colour.MAGENTA, "6xx Science and space"),
            (Colour.RED, "7xx Software and security"),
            (Colour.WHITE, "8xx Games, music and art"),
            (Colour.WHITE, ""),
            (Colour.WHITE, "In each: N00 its front, N01 its feature"),
            (Colour.WHITE, "N02 every story, N03 its tags, N04-N09"),
            (Colour.WHITE, "its busiest tags, N10-N99 its stories"),
        ),
    ),
    (
        "Hackaday's pages",
        (
            (Colour.GREEN, "101 headlines, a sub-page a day"),
            (Colour.GREEN, "102 the magazines   103 popular tags"),
            (Colour.GREEN, "104 tags A to Z     105 search"),
            (Colour.GREEN, "106 talking points  108 facts, figures"),
            (Colour.GREEN, "110 columns  111 Links  112 Podcast"),
            (Colour.GREEN, "113 podcast sounds  120 the people"),
            (Colour.GREEN, "130 on this day  131 roulette"),
            (Colour.GREEN, "132 then and now  133 the archive"),
            (Colour.GREEN, "140 quiz  141 poll  142 my Hackaday"),
            (Colour.GREEN, "201 Retro Challenge  801 DOOM watch"),
        ),
    ),
    (
        "About this channel",
        (
            (Colour.WHITE, "Today's hackaday.com as a teletext"),
            (Colour.WHITE, "service: its stories, pictures and"),
            (Colour.WHITE, "comments, and its archive back to 2004."),
            (Colour.WHITE, ""),
            (Colour.MAGENTA, "Served by the NeoTeletext server, in"),
            (Colour.MAGENTA, "Rust, from a channel written in Python."),
        ),
    ),
)


def help_pages() -> list[Screen]:
    """Answer page 190's sub-pages: getting about, the magazines, Hackaday's pages and this channel."""
    links: list[Link] = [
        ("Front", numbers.FRONT),
        ("Magazines", numbers.MAGAZINES),
        ("Tags", numbers.POPULAR_TAGS),
        ("Archive", numbers.ON_THIS_DAY),
    ]
    return [document(name, Colour.BLUE, [Line(colour, text) for colour, text in rows], links, most=1)[0] for name, rows in HELP]


def quiz(post: Post | None, answers: list[str], picked: int | None, score: tuple[int, int], best: int) -> Screen:
    """Draw a quiz question: which magazine is this story in? The answer is concealed until it is given."""
    screen = page("Guess the magazine", Colour.YELLOW, [(answer[:9], None) for answer in answers] or [NO_LINK], title_colour=Colour.BLUE)
    if post is None:
        screen.centred(10, Colour.WHITE, "Fetching a question...")
        return screen
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Which magazine is this story in?")
    put_headline(screen, BODY_ROW + 2, post.title)
    for offset, line in enumerate(wrap(post.excerpt, TEXT_WIDTH)[:4]):
        screen.coloured(BODY_ROW + 6 + offset, 0, Colour.CYAN, line)
    keys = ("red", "green", "yellow", "cyan")
    colours = (Colour.RED, Colour.GREEN, Colour.YELLOW, Colour.CYAN)
    for offset, answer in enumerate(answers):
        row = 14 + offset
        screen.coloured(row, 0, colours[offset], f"{keys[offset]:7s} {answer}")
        if picked is not None and answer == post.sections[0]:
            screen.code(row, 34, Attribute.FLASH)
            screen.text(row, 35, "<--")
    if picked is None:
        concealed(screen, 19, 0, Colour.MAGENTA, f"Peek with Reveal: {post.sections[0]}")
    else:
        right = answers[picked] in post.sections
        screen.coloured(19, 0, Colour.GREEN if right else Colour.RED, "RIGHT!" if right else f"No: it is {post.sections[0]}")
        screen.coloured(20, 0, Colour.WHITE, "Enter for the next question")
    screen.coloured(21, 0, Colour.YELLOW, f"Score {score[0]} of {score[1]}   best tonight {best}")
    return screen


def poll(options: list[str], votes: list[int], mine: int | None, chosen: int) -> Screen:
    """Draw page 141: the live poll, every vote counted for everyone at once."""
    screen = page(
        "Live poll",
        Colour.GREEN,
        [
            ("Front", numbers.FRONT),
            ("Challenge", numbers.CHALLENGE),
            ("Retro", site.magazine(2).front),
            ("Then+now", numbers.THEN_AND_NOW),
        ],
    )
    screen.coloured(BODY_ROW, 0, Colour.YELLOW, "The best retro computer is...")
    total = sum(votes)
    most = max(votes, default=1) or 1
    for index, (option, count) in enumerate(zip(options, votes, strict=True)):
        row = BODY_ROW + 2 + index * 2
        marker = ">" if index == chosen else " "
        mark = "*" if mine == index else " "
        screen.coloured(row, 0, Colour.WHITE if index != chosen else Colour.YELLOW, f"{marker}{mark}{option}")
        screen.coloured(row, 30, Colour.CYAN, f"{count:4d}")
        bar(screen, row + 1, 2, 30, count, most, Colour.MAGENTA)
    screen.coloured(BODY_ROW + 2 + len(options) * 2, 0, Colour.WHITE, f"{total} votes, live")
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Up Down to choose, Enter to vote" if mine is None else "Thanks for voting: * is yours")
    return screen


def mine(favourites: set[int], chosen: int, posts: list[Post], placed: Placed) -> Screen:
    """Draw page 142: the viewer's favourite magazines, and the latest stories in them."""
    screen = page(
        "My Hackaday",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Magazines", numbers.MAGAZINES), ("Headlines", numbers.HEADLINES), ("Tags", numbers.POPULAR_TAGS)],
    )
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Up Down to choose, Enter to star")
    for index, mag in enumerate(site.SUBJECTS):
        row = BODY_ROW + 1 + index
        star = "*" if mag.number in favourites else " "
        if index == chosen:
            band(screen, row, Colour.YELLOW, Colour.BLUE, f"{star} {mag.name}")
        else:
            screen.coloured(row, 0, Colour.WHITE, f"{star} {mag.name}")
    picked = [post for post in posts if site.magazines_of(post)[0].number in favourites][:7]
    first = BODY_ROW + 2 + len(site.SUBJECTS)
    if favourites:
        screen.coloured(first - 1, 0, Colour.YELLOW, "Your latest")
    row = first
    for post in picked:
        taken = put_headline(screen, row, post.title, number=number_of(placed, post), last_row=HINT_ROW - 1)
        if not taken:
            break
        row += taken
    return screen


#: The lines over the search's results: the typing goes on the third.
SEARCH_INTRO = ["Type words, then Enter: the whole", "archive, back to 2004", ""]


def search_items(results: list[Post]) -> list[Choice]:
    """Answer the search's results as items: each post's year and title."""
    return [Choice(f"{post.date.year} {post.title}", post) for post in results]


def search(typed: str, results: list[Post], chosen: int, *, searching: bool, top: int = 0) -> Screen:
    """Draw page 105: words typed, and the archive's posts for them, from result `top`."""
    items = search_items(results)
    intro = SEARCH_INTRO
    screen = choices(
        "Search",
        Colour.BLUE,
        items,
        chosen,
        [("Front", numbers.FRONT), ("Tags", numbers.POPULAR_TAGS), ("Archive", numbers.ON_THIS_DAY), ("Roulette", numbers.ROULETTE)],
        intro,
        top=top,
    )
    screen.background(BODY_ROW + 2, 0, Colour.BLUE)
    screen.coloured(BODY_ROW + 2, 3, Colour.YELLOW, f"> {typed}")
    screen.cursor(BODY_ROW + 2, 6 + len(codes(typed)), COLUMNS - 7 - len(codes(typed)), ECHO)
    if searching:
        screen.coloured(BODY_ROW + 4, 0, Colour.WHITE, "Searching...")
    elif typed and not results:
        screen.coloured(BODY_ROW + 4, 0, Colour.WHITE, "Type words and press Enter")
    return screen


def figure(value: object) -> int:
    """Answer a figure from the server's `stats`, or 0 when it is missing."""
    return value if isinstance(value, int) else 0


def amount(value: int) -> str:
    """Answer a count of bytes as a reader says it."""
    for unit, size in (("GB", 1 << 30), ("MB", 1 << 20), ("KB", 1 << 10)):
        if value >= size:
            return f"{value / size:.1f} {unit}"
    return f"{value} B"


def server_stats(stats: dict[str, Any], titles: dict[int, str]) -> Screen:
    """Draw page 108's third sub-page: the Rust server's own figures, live (docs/applications.md section 7)."""
    screen = page(
        "Behind the scenes",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Talking", numbers.TALKING), ("Poll", numbers.POLL), ("Help", numbers.ABOUT)],
    )
    server = stats.get("server") or {}
    channel = stats.get("channel") or {}
    if not server:
        screen.centred(10, Colour.WHITE, "Waiting for the server's figures...")
        return screen
    viewers = figure(channel.get("viewers"))
    screen.coloured(BODY_ROW, 0, Colour.YELLOW, "WATCHING")
    screen.coloured(BODY_ROW + 1, 0, Colour.YELLOW, "HACKADAY NOW")
    big_text(screen, BODY_ROW, 16, f"{min(viewers, 9999)}", Colour.GREEN)
    uptime = figure(server.get("uptime_s"))
    memory_kb = figure(server.get("memory_kb"))
    memory = amount(memory_kb * 1024) if memory_kb > 0 else "n/a"
    rows = [
        (Colour.CYAN, f"Peak {figure(channel.get('viewers_peak'))}, {figure(channel.get('viewers_total'))} visits since start"),
        (Colour.WHITE, f"Pages opened {figure(channel.get('opens'))}, keys {figure(channel.get('keys'))}"),
        (Colour.WHITE, f"Pages drawn {figure(channel.get('shared_published'))} shared, {figure(channel.get('own_published'))} own"),
        (Colour.WHITE, f"Sent {amount(figure(server.get('bytes_sent')))} in {figure(server.get('messages_sent'))} messages"),
        (Colour.WHITE, f"Server up {uptime // 3600}h {uptime % 3600 // 60:02d}m, memory {memory}"),
    ]
    for offset, (colour, text) in enumerate(rows):
        screen.coloured(BODY_ROW + 3 + offset, 0, colour, fit(text, TEXT_WIDTH))
    band(screen, 12, Colour.BLUE, Colour.YELLOW, "Most opened pages")
    most = channel.get("most_opened") or {}
    for offset, (number, opens) in enumerate(list(most.items())[:6]):
        try:
            title = titles.get(int(number, 16), "")
        except ValueError:
            continue
        screen.coloured(13 + offset, 0, Colour.YELLOW, number)
        screen.coloured(13 + offset, NUMBER_WIDTH, Colour.WHITE, fit(title, 26))
        screen.coloured(13 + offset, 32, Colour.CYAN, f"{figure(opens):6d}")
    others = {folder: figure(count) for folder, count in (server.get("viewers") or {}).items() if folder != "hackaday"}
    if others:
        screen.coloured(
            20, 0, Colour.MAGENTA, fit("Also watching: " + ", ".join(f"{folder} {count}" for folder, count in others.items()), TEXT_WIDTH)
        )
    screen.coloured(21, 0, Colour.GREEN, "A Rust server, a Python channel: live")
    return screen


def soundboard_listing(clips: list[Clip]) -> Listing:
    """Lay out the sound board's grabs, each with its length."""
    return listing([f"{clip.title} ({clip.seconds:.1f}s)" for clip in clips], BODY_ROW + 2, HINT_ROW - 2)


def soundboard(clips: list[Clip], chosen: int, playing: str | None, top: int = 0) -> Screen:
    """Draw page 113: the Hackaday Podcast's sound grabs, played with Enter or a click."""
    screen = page(
        "Podcast sounds",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Podcast", numbers.PODCAST), ("Figures", numbers.FIGURES), ("Help", numbers.ABOUT)],
    )
    if not clips:
        screen.centred(10, Colour.WHITE, "No sound grabs on this server")
        return screen
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Sound grabs from the Hackaday Podcast")
    draw_listing(screen, soundboard_listing(clips), top, chosen)
    if playing:
        flashing(screen, HINT_ROW - 1, 0, Colour.GREEN, fit(f"Playing: {playing}", 34))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Up Down or click to choose, Enter plays")
    return screen


#: The rows a portrait takes: the engine's converter draws it into rows 2
#: to 16, the rows under it are the profile's own.
PORTRAIT_LAST_ROW = 16


def person_page(index: int) -> int:
    """Answer the page of person `index`."""
    return numbers.PEOPLE + 1 + index


#: The lines a profile's biography may take.
BIO_LINES = 2


def two_lines(text: str) -> list[str]:
    """Answer `text` in at most two lines: whole sentences when they fit, else cut with a mark."""
    lines = wrap(text, TEXT_WIDTH)
    if len(lines) <= BIO_LINES:
        return lines
    sentences = text.split(". ")
    for count in range(len(sentences) - 1, 0, -1):
        shorter = ". ".join(sentences[:count]).rstrip(".") + "."
        if len(wrap(shorter, TEXT_WIDTH)) <= BIO_LINES:
            return wrap(shorter, TEXT_WIDTH)
    kept = lines[:BIO_LINES]
    kept[-1] = fit(kept[-1] + " ..", TEXT_WIDTH)
    return kept


def people_index(people: list[Person]) -> Screen:
    """Draw page 120: the people of Hackaday, each with their page."""
    screen = page(
        "The people",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Figures", numbers.FIGURES), ("Podcast", numbers.PODCAST), ("Sounds", numbers.SOUNDBOARD)],
    )
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Some of the people who write Hackaday")
    screen.coloured(BODY_ROW + 1, 0, Colour.CYAN, "and talk about it on the podcast")
    for index, person in enumerate(people):
        row = BODY_ROW + 3 + index * 3
        screen.coloured(row, 0, Colour.YELLOW, f"{person_page(index):03X}")
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, person.name)
        posts = f"{person.posts} posts" if person.posts else ""
        screen.coloured(row + 1, NUMBER_WIDTH, Colour.CYAN, fit(f"{person.title}  {posts}", COLUMNS - NUMBER_WIDTH - 1))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Each says hello when you open the page")
    return screen


def profile(person: Person, index: int, count: int) -> list[Screen]:
    """Answer one person's page: their portrait and who they are, then "Latest from" them, then their hackaday.io profile."""
    links: list[Link] = [
        ("People", numbers.PEOPLE),
        ("Next", person_page((index + 1) % count)),
        ("Sounds", numbers.SOUNDBOARD),
        ("Front", numbers.FRONT),
    ]
    screen = page("", Colour.BLACK, links)
    screen.rows[1] = bytearray([0x20] * COLUMNS)
    if person.portrait is not None:
        screen.load_tti(person.portrait, first_row=2, last_row=PORTRAIT_LAST_ROW)
    else:
        emblem(screen, 4, 13, Colour.WHITE)
        screen.centred(12, Colour.CYAN, "(no photo: the Jolly Wrencher)")
    # Row 1 names the sub-page, as the page list shows it.
    screen.centred(1, Colour.CYAN, person.name)
    screen.double_height(PORTRAIT_LAST_ROW + 1, 1, Colour.YELLOW, person.name)
    screen.coloured(PORTRAIT_LAST_ROW + 3, 0, Colour.CYAN, fit(f"{person.title}. {person.podcast}", TEXT_WIDTH))
    for offset, line in enumerate(two_lines(person.bio)):
        screen.coloured(PORTRAIT_LAST_ROW + 4 + offset, 0, Colour.WHITE, line)
    if person.posts:
        screen.coloured(PORTRAIT_LAST_ROW + 6, 0, Colour.GREEN, f"{person.posts} posts on Hackaday")
    screens = [screen]
    first_name = person.name.split()[0]
    latest = [Line(Colour.CYAN, f"{person.posts} posts on hackaday.com"), Line(Colour.WHITE, "")] if person.posts else []
    for title in person.latest:
        latest.extend(Line(Colour.WHITE, line) for line in headline(title, TEXT_WIDTH))
        latest.append(Line(Colour.WHITE, ""))
    if person.latest:
        screens.extend(document(f"Latest from {first_name}", Colour.MAGENTA, latest, links, most=1))
    if person.io_url:
        about = [
            Line(Colour.YELLOW, fit(person.io_url.removeprefix("https://"), TEXT_WIDTH)),
            Line(Colour.WHITE, ""),
            *(Line(Colour.WHITE, line) for line in wrap(person.io_tagline, TEXT_WIDTH)[:4]),
            Line(Colour.WHITE, ""),
            Line(Colour.CYAN, fit(f"{person.io_location}  joined {person.io_joined}", TEXT_WIDTH)),
            Line(Colour.WHITE, ""),
            Line(Colour.GREEN, "Their projects:"),
            *(Line(Colour.WHITE, fit(f"* {project}", TEXT_WIDTH)) for project in person.io_projects[:8]),
        ]
        screens.extend(document("On hackaday.io", Colour.BLUE, about, links, most=1))
    return screens
