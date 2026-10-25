"""Every page of the channel, drawn from what the library holds.

These are plain functions from data to screens, so each can be tested
without a server. What scrolls takes the first line shown; what lists
takes the item chosen.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING, Any

from neoteletext_app.charset import codes
from neoteletext_app.screen import COLUMNS, ECHO, FASTEXT_ROW, Attribute, Colour, Screen

from hackaday import numbers
from hackaday.layout import (
    BODY_ROW,
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
    page,
    scroll,
    wrap,
)
from hackaday.posts import OTHER_SECTION, SECTIONS, Post, threaded

if TYPE_CHECKING:
    from hackaday.clips import Clip
    from hackaday.people import Person

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
#: How many headlines the front page lists.
FRONT_HEADLINES = 6
#: How long a newsflash stays, after the post.
NEWSFLASH_FOR = timedelta(minutes=45)
#: Hackaday posts about every three hours (8.9 a day, measured).
POSTING_INTERVAL = timedelta(hours=3)
#: The deepest a comment is indented.
DEEPEST_INDENT = 4
#: Columns a comment is indented per level of reply.
INDENT_STEP = 2
#: Each section's colour band.
SECTION_COLOURS = (
    Colour.MAGENTA,
    Colour.BLUE,
    Colour.RED,
    Colour.GREEN,
    Colour.BLUE,
    Colour.MAGENTA,
    Colour.RED,
    Colour.GREEN,
    Colour.BLUE,
    Colour.MAGENTA,
)
#: The page numbers of the list in front of each line on the front page.
NUMBER_WIDTH = 4


def story_page(slot: int) -> int:
    """Answer story `slot`'s page."""
    return numbers.slot_page(numbers.STORY_MAGAZINE, slot)


def section_index(name: str) -> int:
    """Answer a section's place, the last for the other posts."""
    names = [section for section, _ in SECTIONS]
    return names.index(name) if name in names else len(names)


def section_names() -> list[str]:
    """Answer every section's name, the catch-all last."""
    return [name for name, _ in SECTIONS] + [OTHER_SECTION]


def short_date(when: datetime) -> str:
    """Answer a date as a teletext service writes it."""
    return when.strftime("%a %d %b %H:%M")


def fit(text: str, width: int) -> str:
    """Answer `text` cut to `width` characters, with a mark when cut."""
    return text if len(codes(text)) <= width else text[: width - 2] + ".."


# The front page.


def front(posts: list[Post], now: datetime, ticker: int) -> Screen:
    """Draw page 100: the emblem, the top story, the latest headlines and the news."""
    screen = page(
        "",
        Colour.BLACK,
        [("Headlines", numbers.HEADLINES), ("Sections", numbers.SECTIONS), ("Roulette", numbers.ROULETTE), ("Help", numbers.ABOUT)],
    )
    # The title band is the emblem's: row 1 and 2 are drawn over.
    screen.rows[1] = bytearray([0x20] * COLUMNS)
    emblem(screen, 1, 1, Colour.WHITE)
    screen.double_height(1, 17, Colour.YELLOW, "HACKADAY")
    screen.coloured(3, 17, Colour.WHITE, "fresh hacks every day")
    screen.coloured(4, 17, Colour.CYAN, now.strftime("%a %d %b %Y"))
    if posts:
        due = posts[0].date + POSTING_INTERVAL
        left = due - now
        wait = "any minute now" if left <= timedelta(0) else f"in {left.seconds // 3600}h {left.seconds % 3600 // 60:02d}m"
        screen.coloured(5, 17, Colour.GREEN, "next hack due")
        screen.coloured(6, 17, Colour.GREEN, wait)
        top = posts[0]
        band(screen, 8, Colour.RED, Colour.YELLOW, f"TOP STORY  {numbers.slot_page(numbers.STORY_MAGAZINE, 0):03X}")
        for offset, line in enumerate(wrap(top.title, TEXT_WIDTH)[:2]):
            screen.coloured(9 + offset, 0, Colour.WHITE, line)
        for offset, line in enumerate(wrap(top.excerpt, TEXT_WIDTH)[:3]):
            screen.coloured(11 + offset, 0, Colour.CYAN, line)
        for offset, post in enumerate(posts[1 : 1 + FRONT_HEADLINES]):
            row = 14 + offset
            screen.coloured(row, 0, Colour.YELLOW, f"{story_page(offset + 1):03X}")
            screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
        if now - top.date <= NEWSFLASH_FOR:
            flashing(screen, NEWSFLASH_ROW, 0, Colour.RED, "NEWSFLASH")
            screen.coloured(NEWSFLASH_ROW, 12, Colour.YELLOW, fit(f"{story_page(0):03X} {top.title}", COLUMNS - 13))
        else:
            screen.coloured(NEWSFLASH_ROW, 0, Colour.MAGENTA, "Archive 105 DOOM 107 Quiz 108 Poll 116")
        shown = posts[ticker % min(len(posts), 10)]
        slot = posts.index(shown)
        band(screen, TICKER_ROW, Colour.BLUE, Colour.WHITE, fit(f"{story_page(slot):03X} {shown.title}", COLUMNS - 4))
    else:
        screen.centred(10, Colour.WHITE, "Fetching the latest hacks...")
    screen.coloured(23, 0, Colour.GREEN, "Type a page number or press a colour")
    return screen


# Lists that scroll.


def scrolling(  # noqa: PLR0913 - a scrolling page: title, colour, lines, place, links and hint
    title: str, colour: Colour, lines: list[Line], top: int, links: list[Link], *, hint: str = "PgDn PgUp arrows Home End wheel"
) -> Screen:
    """Draw a page of `lines` that scrolls under its title band."""
    screen = page(title, colour, links)
    scroll(screen, lines, top, BODY_ROW, LIST_ROWS)
    if len(lines) > LIST_ROWS:
        last = min(len(lines), top + LIST_ROWS)
        screen.coloured(POSITION_ROW, 0, Colour.CYAN, f"Lines {top + 1}-{last} of {len(lines)}")
    screen.coloured(HINT_ROW, 0, Colour.GREEN, hint)
    return screen


def headline_lines(posts: list[Post], slots: dict[int, int]) -> list[Line]:
    """Answer one line a post: its page and title."""
    lines: list[Line] = []
    for post in posts:
        slot = slots.get(post.id)
        number = f"{story_page(slot):03X}" if slot is not None else "199"
        lines.append(Line(Colour.WHITE, f"{number} {fit(post.title, SCROLL_TEXT_WIDTH - NUMBER_WIDTH)}"))
    return lines


def headlines(posts: list[Post], top: int) -> Screen:
    """Draw page 101: every story the channel holds, newest first."""
    slots = {post.id: slot for slot, post in enumerate(posts)}
    return scrolling(
        "Headlines",
        Colour.RED,
        headline_lines(posts, slots),
        top,
        [("Front", numbers.FRONT), ("Sections", numbers.SECTIONS), ("Tags", numbers.TAGS), ("Search", numbers.SEARCH)],
    )


def sections_index(posts: list[Post]) -> Screen:
    """Draw page 102: the sections, how many stories each has, and their pages."""
    screen = page(
        "Sections",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Headlines", numbers.HEADLINES), ("Tags", numbers.TAGS), ("Writers", numbers.WRITERS)],
    )
    counts = Counter(name for post in posts for name in post.sections)
    most = max(counts.values(), default=1)
    for index, name in enumerate(section_names()):
        row = BODY_ROW + 1 + index * 2
        screen.coloured(row, 0, Colour.YELLOW, f"{numbers.section_page(index):03X}")
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, name)
        screen.coloured(row, 26, Colour.CYAN, f"{counts.get(name, 0):3d}")
        bar(screen, row + 1, NUMBER_WIDTH, 30, counts.get(name, 0), most, SECTION_COLOURS[index % len(SECTION_COLOURS)])
    return screen


def section(index: int, posts: list[Post], top: int) -> Screen:
    """Draw a section's page: its stories."""
    name = section_names()[index]
    slots = {post.id: slot for slot, post in enumerate(posts)}
    chosen = [post for post in posts if name in post.sections]
    lines = headline_lines(chosen, slots) or [Line(Colour.WHITE, "Nothing in this section yet")]
    links: list[Link] = [("Sections", numbers.SECTIONS), ("Front", numbers.FRONT)]
    if index + 1 < len(section_names()):
        links.append(("Next", numbers.section_page(index + 1)))
    return scrolling(name, SECTION_COLOURS[index % len(SECTION_COLOURS)], lines, top, links)


# A story's four pages.


def story_links(slot: int) -> list[Link]:
    """Answer a story's Fastext: its picture, comments and links, and the next story."""
    return [
        ("Picture", numbers.slot_page(numbers.PICTURE_MAGAZINE, slot)),
        ("Comments", numbers.slot_page(numbers.COMMENTS_MAGAZINE, slot)),
        ("Links", numbers.slot_page(numbers.LINKS_MAGAZINE, slot)),
        ("Next", story_page((slot + 1) % numbers.STORIES)),
    ]


def article_lines(post: Post) -> list[Line]:
    """Answer a post's text as lines beside a scroll bar: headings yellow, quotes cyan, items marked."""
    lines: list[Line] = []
    for block in post.article.blocks:
        if block.kind == "heading":
            lines.extend(Line(Colour.YELLOW, line) for line in wrap(block.text, SCROLL_TEXT_WIDTH))
        elif block.kind == "item":
            for number, line in enumerate(wrap(block.text, SCROLL_TEXT_WIDTH - 2)):
                lines.append(Line(Colour.WHITE, ("* " if number == 0 else "  ") + line))
        elif block.kind in {"quote", "code"}:
            lines.extend(Line(Colour.CYAN, line) for line in wrap(block.text, SCROLL_TEXT_WIDTH))
        else:
            lines.extend(Line(Colour.WHITE, line) for line in wrap(block.text, SCROLL_TEXT_WIDTH))
        lines.append(Line(Colour.WHITE, ""))
    if post.article.videos:
        lines.append(Line(Colour.MAGENTA, f"Video: {len(post.article.videos)} on the links page"))
    return lines


def story(post: Post, slot: int, top: int) -> Screen:
    """Draw story `slot`'s page: its title, who and when, and its text, which scrolls."""
    section_name = post.sections[0]
    screen = page(f"{section_name}", SECTION_COLOURS[section_index(section_name) % len(SECTION_COLOURS)], story_links(slot))
    screen.coloured(1, COLUMNS - 8, Colour.WHITE, f"{slot + 1:3d}/100")
    title = wrap(post.title, TEXT_WIDTH)
    for offset, line in enumerate(title[:2]):
        screen.coloured(BODY_ROW + offset, 0, Colour.YELLOW, line)
    screen.coloured(5, 0, Colour.CYAN, fit(f"by {post.author}, {short_date(post.date)}", TEXT_WIDTH))
    screen.coloured(6, 0, Colour.GREEN, fit(", ".join(post.categories + post.tags), TEXT_WIDTH))
    lines = article_lines(post)
    scroll(screen, lines, top, STORY_TEXT_ROW, STORY_ROWS)
    shown = (
        f"Lines {top + 1}-{min(len(lines), top + STORY_ROWS)} of {len(lines)}" if len(lines) > STORY_ROWS else f"{post.article.words} words"
    )
    screen.coloured(POSITION_ROW + 1, 0, Colour.CYAN, f"{shown}  {post.comment_count} comments")
    screen.coloured(23, 0, Colour.GREEN, "PgDn PgUp to read, colours for more")
    return screen


def picture(post: Post, slot: int, tti: str | None) -> Screen:
    """Draw story `slot`'s picture page: the lead picture in mosaics, or a word that it is coming."""
    screen = Screen()
    screen.header(f"HACKADAY  picture {story_page(slot):03X}")
    links = story_links(slot)
    links[0] = ("Story", story_page(slot))
    if tti is None:
        screen = page("Picture", Colour.BLUE, links)
        screen.centred(10, Colour.WHITE, "This picture is being drawn")
        screen.centred(12, Colour.CYAN, "in teletext mosaics: back soon")
        screen.centred(14, Colour.YELLOW, fit(post.title, 36))
        return screen
    # The picture takes rows 1 to 24 (the engine's converter); the coloured
    # keys still follow these links, though row 24 shows the picture.
    screen.load_tti(tti, first_row=1, last_row=FASTEXT_ROW)
    screen.fastext([target for _, target in links])
    screen.index_link(numbers.FRONT)
    return screen


def comment_lines(post: Post) -> list[Line]:
    """Answer a post's comments as lines, replies indented under what they answer."""
    lines: list[Line] = []
    for depth, comment in threaded(post.comments):
        indent = " " * (min(depth, DEEPEST_INDENT) * INDENT_STEP)
        width = SCROLL_TEXT_WIDTH - len(indent)
        lines.append(Line(Colour.CYAN, indent + fit(f"{comment.author}, {short_date(comment.date)}", width)))
        lines.extend(Line(Colour.WHITE, indent + line) for line in wrap(comment.text, width))
        lines.append(Line(Colour.WHITE, ""))
    return lines or [Line(Colour.WHITE, "No comments yet: be the first on"), Line(Colour.WHITE, "hackaday.com")]


def comments(post: Post, slot: int, top: int) -> Screen:
    """Draw story `slot`'s comments, which scroll."""
    links = story_links(slot)
    links[1] = ("Story", story_page(slot))
    return scrolling(fit(f"{post.comment_count} comments", 30), Colour.GREEN, comment_lines(post), top, links)


def links_page(post: Post, slot: int, qr: str | None, what: str) -> Screen:
    """Draw one of story `slot`'s link pages: a QR code a phone can follow, and what it leads to."""
    links = story_links(slot)
    links[2] = ("Story", story_page(slot))
    screen = page("Read on", Colour.MAGENTA, links)
    if qr is not None:
        screen.load_tti(qr, first_row=BODY_ROW, last_row=20)
    side = 25
    screen.coloured(BODY_ROW + 1, side, Colour.YELLOW, what)
    for offset, line in enumerate(wrap("Scan the code with a phone to open it", COLUMNS - side - 2)):
        screen.coloured(BODY_ROW + 3 + offset, side, Colour.WHITE, line)
    for offset, line in enumerate(wrap(post.title, COLUMNS - side - 2)[:5]):
        screen.coloured(BODY_ROW + 8 + offset, side, Colour.CYAN, line)
    for offset, (text, _) in enumerate(post.article.links[:3]):
        screen.coloured(21 + offset - 1, 0, Colour.GREEN, fit(f"Link: {text}", TEXT_WIDTH))
    return screen


# Lists to choose from.


@dataclass(frozen=True)
class Choice:
    """A line of a list to choose from, and the post it leads to."""

    text: str
    post: Post | None


def choices(title: str, colour: Colour, items: list[Choice], chosen: int, links: list[Link], intro: list[str]) -> Screen:  # noqa: PLR0913, PLR0917 - a list page: title, colour, items, choice, links, introduction
    """Draw a list to choose from with the arrows and Enter: the chosen line on a band."""
    screen = page(title, colour, links)
    for offset, line in enumerate(intro):
        screen.coloured(BODY_ROW + offset, 0, Colour.CYAN, line)
    first = BODY_ROW + len(intro) + (1 if intro else 0)
    rows = POSITION_ROW - first
    top = max(0, min(chosen - rows // 2, len(items) - rows))
    for offset, item in enumerate(items[top : top + rows]):
        row = first + offset
        if top + offset == chosen:
            band(screen, row, Colour.YELLOW, Colour.BLUE, fit(item.text, COLUMNS - 4))
        else:
            screen.coloured(row, 0, Colour.WHITE, fit(item.text, COLUMNS - 2))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Up Down to choose, Enter to read")
    return screen


def reader(post: Post, top: int) -> Screen:
    """Draw page 199: a post from the archive, which scrolls."""
    screen = page(
        "From the archive",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Day", numbers.ON_THIS_DAY), ("Roulette", numbers.ROULETTE), ("Search", numbers.SEARCH)],
    )
    for offset, line in enumerate(wrap(post.title, TEXT_WIDTH)[:2]):
        screen.coloured(BODY_ROW + offset, 0, Colour.YELLOW, line)
    screen.coloured(5, 0, Colour.CYAN, fit(f"by {post.author}, {post.date.strftime('%d %b %Y')}", TEXT_WIDTH))
    screen.coloured(6, 0, Colour.GREEN, fit(", ".join(post.categories + post.tags), TEXT_WIDTH))
    lines = article_lines(post)
    scroll(screen, lines, top, STORY_TEXT_ROW, STORY_ROWS)
    screen.coloured(POSITION_ROW + 1, 0, Colour.CYAN, f"{post.article.words} words, {post.date.year}")
    screen.coloured(23, 0, Colour.GREEN, "PgDn PgUp to read")
    return screen


# Fun and figures.


def doom(ports: list[Post], now: datetime) -> Screen:
    """Draw page 107: days since the last port of DOOM, and the ports."""
    screen = page(
        "DOOM watch",
        Colour.RED,
        [("Front", numbers.FRONT), ("Roulette", numbers.ROULETTE), ("Quiz", numbers.QUIZ), ("Stats", numbers.STATISTICS)],
    )
    if not ports:
        screen.centred(10, Colour.WHITE, "Counting the ports of DOOM...")
        return screen
    days = max(0, (now - ports[0].date).days)
    screen.coloured(BODY_ROW + 1, 0, Colour.YELLOW, "DAYS SINCE THE LAST")
    screen.coloured(BODY_ROW + 2, 0, Colour.YELLOW, "PORT OF DOOM")
    big_text(screen, BODY_ROW + 1, 22, f"{min(days, 999)}", Colour.RED)
    screen.coloured(BODY_ROW + 4, 0, Colour.CYAN, fit(f"Last: {ports[0].title}", TEXT_WIDTH))
    band(screen, 9, Colour.RED, Colour.YELLOW, f"Recent ports: {len(ports)} found")
    for offset, post in enumerate(ports[:11]):
        screen.coloured(10 + offset, 0, Colour.WHITE, fit(f"{post.date.year} {post.title}", TEXT_WIDTH))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Can it run DOOM? On Hackaday, it can")
    return screen


def statistics(posts: list[Post]) -> Screen:
    """Draw page 109: the stories by section, by hour of posting, and the busiest threads."""
    screen = page(
        "Statistics",
        Colour.GREEN,
        [("Front", numbers.FRONT), ("Writers", numbers.WRITERS), ("Discussed", numbers.DISCUSSED), ("DOOM", numbers.DOOM)],
    )
    counts = Counter(name for post in posts for name in post.sections)
    most = max(counts.values(), default=1)
    screen.coloured(BODY_ROW, 0, Colour.YELLOW, f"The last {len(posts)} stories by section")
    for index, name in enumerate(section_names()):
        row = BODY_ROW + 1 + index
        screen.coloured(row, 0, Colour.WHITE, fit(name, 20))
        screen.coloured(row, 21, Colour.CYAN, f"{counts.get(name, 0):3d}")
        bar(screen, row, 25, 13, counts.get(name, 0), most, Colour.GREEN)
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
    """Draw page 110: who writes Hackaday, and how much."""
    screen = page(
        "Writers",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Stats", numbers.STATISTICS), ("Sections", numbers.SECTIONS), ("Headlines", numbers.HEADLINES)],
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


def discussed(posts: list[Post]) -> Screen:
    """Draw page 118: the most argued-about stories."""
    screen = page(
        "Most discussed",
        Colour.RED,
        [("Front", numbers.FRONT), ("Comments", numbers.COMMENTS_WALL), ("Stats", numbers.STATISTICS), ("Headlines", numbers.HEADLINES)],
    )
    slots = {post.id: slot for slot, post in enumerate(posts)}
    ranked = sorted(posts, key=lambda post: post.comment_count, reverse=True)[:9]
    most = ranked[0].comment_count if ranked and ranked[0].comment_count else 1
    for offset, post in enumerate(ranked):
        row = BODY_ROW + offset * 2
        screen.coloured(row, 0, Colour.YELLOW, f"{story_page(slots[post.id]):03X}")
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
        bar(screen, row + 1, NUMBER_WIDTH, 26, post.comment_count, most, Colour.RED)
        screen.coloured(row + 1, 33, Colour.CYAN, f"{post.comment_count:4d}")
    return screen


def comments_wall_lines(posts: list[Post]) -> list[Line]:
    """Answer the newest comments on every story as lines: who, on which story, and the start of what they said."""
    slots = {post.id: slot for slot, post in enumerate(posts)}
    every = sorted(((comment, post) for post in posts for comment in post.comments), key=lambda pair: pair[0].date, reverse=True)[:60]
    lines: list[Line] = []
    for comment, post in every:
        lines.append(Line(Colour.CYAN, fit(f"{comment.author} on {story_page(slots[post.id]):03X}", SCROLL_TEXT_WIDTH)))
        lines.extend(Line(Colour.WHITE, line) for line in wrap(comment.text, SCROLL_TEXT_WIDTH)[:4])
        lines.append(Line(Colour.WHITE, ""))
    return lines or [Line(Colour.WHITE, "Reading the comments...")]


def comments_wall(posts: list[Post], top: int) -> Screen:
    """Draw page 117: the newest comments on every story."""
    links: list[Link] = [
        ("Front", numbers.FRONT),
        ("Discussed", numbers.DISCUSSED),
        ("Headlines", numbers.HEADLINES),
        ("Stats", numbers.STATISTICS),
    ]
    return scrolling("Comments", Colour.GREEN, comments_wall_lines(posts), top, links)


def column_page(title: str, colour: Colour, post: Post | None, top: int, links: list[Link]) -> Screen:
    """Draw a regular column, its latest edition, which scrolls."""
    if post is None:
        return scrolling(title, colour, [Line(Colour.WHITE, "Looking for the latest edition...")], 0, links)
    lines = [Line(Colour.YELLOW, line) for line in wrap(post.title, SCROLL_TEXT_WIDTH)]
    lines.append(Line(Colour.CYAN, fit(f"by {post.author}, {short_date(post.date)}", SCROLL_TEXT_WIDTH)))
    lines.append(Line(Colour.WHITE, ""))
    lines.extend(article_lines(post))
    return scrolling(title, colour, lines, top, links)


def challenge(entries: list[Post], now: datetime, closes: datetime) -> Screen:
    """Draw page 114: the Retrocomputing Challenge, its countdown and the entries written up."""
    screen = page(
        "Retro Challenge",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Retro", numbers.section_page(0)), ("Then+now", numbers.THEN_AND_NOW), ("DOOM", numbers.DOOM)],
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
    band(screen, 10, Colour.MAGENTA, Colour.WHITE, f"Entries written up: {len(entries)}")
    for offset, post in enumerate(entries[:10]):
        title = post.title.removeprefix("2026 Retrocomputing Challenge: ")
        screen.coloured(11 + offset, 0, Colour.WHITE, fit(title, TEXT_WIDTH))
    return screen


def then_and_now(first: list[Post], latest: list[Post]) -> Screen:
    """Draw page 119: Hackaday's first posts beside today's."""
    screen = page(
        "Then and now",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Archive", numbers.ON_THIS_DAY), ("Roulette", numbers.ROULETTE), ("Retro", numbers.CHALLENGE)],
    )
    first_year = first[0].date.year if first else 2004
    band(screen, BODY_ROW, Colour.YELLOW, Colour.BLUE, f"THEN: the first posts, {first_year}")
    for offset, post in enumerate(first[:7]):
        screen.coloured(BODY_ROW + 1 + offset, 0, Colour.WHITE, fit(f"{post.date.strftime('%d %b')} {post.title}", TEXT_WIDTH))
    band(screen, 12, Colour.GREEN, Colour.BLACK, f"NOW: {datetime.now(UTC).year}")
    for offset, post in enumerate(latest[:8]):
        screen.coloured(13 + offset, 0, Colour.WHITE, fit(f"{post.date.strftime('%d %b')} {post.title}", TEXT_WIDTH))
    return screen


def about() -> Screen:
    """Draw page 120: how to use the channel, and its pages."""
    screen = page(
        "How it works",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Headlines", numbers.HEADLINES), ("Sections", numbers.SECTIONS), ("Archive", numbers.ON_THIS_DAY)],
    )
    rows = [
        (Colour.YELLOW, "Hackaday, live, as 1980s teletext"),
        (Colour.WHITE, "Type a page number, or press a coloured"),
        (Colour.WHITE, "key (F1 to F4) for its prompt below."),
        (Colour.CYAN, "2NN story NN    3NN its picture"),
        (Colour.CYAN, "4NN comments    5NN links and QR code"),
        (Colour.GREEN, "101 headlines   102 sections  103 tags"),
        (Colour.GREEN, "104 search      105 on this day"),
        (Colour.GREEN, "106 roulette    107 DOOM watch"),
        (Colour.GREEN, "108 quiz        109 statistics"),
        (Colour.GREEN, "110 writers     111 security"),
        (Colour.GREEN, "112 links       113 podcast"),
        (Colour.GREEN, "114 challenge   115 my Hackaday"),
        (Colour.GREEN, "116 poll        117 comments"),
        (Colour.GREEN, "118 discussed   119 then and now"),
        (Colour.WHITE, "Long pages scroll: Page Up, Page Down,"),
        (Colour.WHITE, "the arrows, Home, End and the wheel."),
        (Colour.GREEN, "121 behind the scenes 122 podcast sounds"),
        (Colour.GREEN, "140 the people of Hackaday"),
        (Colour.MAGENTA, "Served by the NeoTeletext server (Rust)"),
        (Colour.MAGENTA, "from hackaday.com, by an app in Python."),
    ]
    for offset, (colour, text) in enumerate(rows):
        screen.coloured(BODY_ROW + offset, 0, colour, text)
    return screen


def quiz(post: Post | None, answers: list[str], picked: int | None, score: tuple[int, int], best: int) -> Screen:
    """Draw a quiz question: which section is this story in? The answer is concealed until it is given."""
    screen = page("Guess the section", Colour.YELLOW, [(answer[:9], None) for answer in answers] or [NO_LINK], title_colour=Colour.BLUE)
    if post is None:
        screen.centred(10, Colour.WHITE, "Fetching a question...")
        return screen
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Which section is this story in?")
    for offset, line in enumerate(wrap(post.title, TEXT_WIDTH)[:3]):
        screen.coloured(BODY_ROW + 2 + offset, 0, Colour.WHITE, line)
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
    """Draw page 116: the live poll, every vote counted for everyone at once."""
    screen = page(
        "Live poll",
        Colour.GREEN,
        [
            ("Front", numbers.FRONT),
            ("Challenge", numbers.CHALLENGE),
            ("Retro", numbers.section_page(0)),
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


def mine(names: list[str], favourites: set[str], chosen: int, posts: list[Post]) -> Screen:
    """Draw page 115: the viewer's favourite sections, and the stories in them."""
    screen = page(
        "My Hackaday",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Sections", numbers.SECTIONS), ("Headlines", numbers.HEADLINES), ("Tags", numbers.TAGS)],
    )
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Up Down to choose, Enter to star")
    for index, name in enumerate(names):
        row = BODY_ROW + 1 + index
        star = "*" if name in favourites else " "
        if index == chosen:
            band(screen, row, Colour.YELLOW, Colour.BLUE, f"{star} {name}")
        else:
            screen.coloured(row, 0, Colour.WHITE, f"{star} {name}")
    slots = {post.id: slot for slot, post in enumerate(posts)}
    picked = [post for post in posts if favourites & set(post.sections)][:7]
    first = BODY_ROW + 2 + len(names)
    if favourites:
        screen.coloured(first - 1, 0, Colour.YELLOW, "Your latest")
    for offset, post in enumerate(picked):
        row = first + offset
        if row >= HINT_ROW:
            break
        screen.coloured(row, 0, Colour.YELLOW, f"{story_page(slots[post.id]):03X}")
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
    return screen


def search(typed: str, results: list[Post], chosen: int, *, searching: bool) -> Screen:
    """Draw page 104: words typed, and the archive's posts for them."""
    items = [Choice(f"{post.date.year} {post.title}", post) for post in results]
    intro = ["Type words, then Enter: the whole", "archive, back to 2004", ""]
    screen = choices(
        "Search",
        Colour.BLUE,
        items,
        chosen,
        [("Front", numbers.FRONT), ("Tags", numbers.TAGS), ("Archive", numbers.ON_THIS_DAY), ("Roulette", numbers.ROULETTE)],
        intro,
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
    """Draw page 121: the Rust server's own figures, live (docs/applications.md section 7)."""
    screen = page(
        "Behind the scenes",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Stats", numbers.STATISTICS), ("Poll", numbers.POLL), ("Help", numbers.ABOUT)],
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


def soundboard(clips: list[Clip], chosen: int, playing: str | None) -> Screen:
    """Draw page 122: the Hackaday Podcast's sound grabs, played with Enter."""
    screen = page(
        "Podcast sounds",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Podcast", numbers.PODCAST), ("Behind", numbers.SERVER_STATS), ("Help", numbers.ABOUT)],
    )
    if not clips:
        screen.centred(10, Colour.WHITE, "No sound grabs on this server")
        return screen
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Sound grabs from the Hackaday Podcast")
    first = BODY_ROW + 2
    rows = HINT_ROW - first - 1
    top = max(0, min(chosen - rows // 2, len(clips) - rows))
    for offset, clip in enumerate(clips[top : top + rows]):
        row = first + offset
        text = fit(f"{clip.title}", 30)
        length = f"{clip.seconds:4.1f}s"
        if top + offset == chosen:
            band(screen, row, Colour.YELLOW, Colour.BLUE, f"{text:30s} {length}")
        else:
            screen.coloured(row, 0, Colour.WHITE, text)
            screen.coloured(row, 33, Colour.CYAN, length)
    if playing:
        flashing(screen, HINT_ROW - 1, 0, Colour.GREEN, fit(f"Playing: {playing}", 34))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Up Down to choose, Enter to play")
    return screen


#: The rows a portrait takes: the engine's converter draws it into rows 2
#: to 16, the rows under it are the profile's own.
PORTRAIT_LAST_ROW = 16


def person_page(index: int) -> int:
    """Answer the page of person `index`."""
    return numbers.PEOPLE + 1 + index


def people_index(people: list[Person]) -> Screen:
    """Draw page 140: the people of Hackaday, each with their page."""
    screen = page(
        "The people",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Writers", numbers.WRITERS), ("Podcast", numbers.PODCAST), ("Sounds", numbers.SOUNDBOARD)],
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


def profile(person: Person, index: int, count: int) -> Screen:
    """Draw one person's page: their portrait, who they are, what they write and how they say hello."""
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
    screen.centred(1, Colour.CYAN, "THE PEOPLE OF HACKADAY")
    screen.double_height(PORTRAIT_LAST_ROW + 1, 1, Colour.YELLOW, person.name)
    screen.coloured(PORTRAIT_LAST_ROW + 3, 0, Colour.CYAN, fit(f"{person.title}. {person.podcast}", TEXT_WIDTH))
    detail = person.bio or (f"Latest: {person.latest[0]}" if person.latest else "")
    for offset, line in enumerate(wrap(detail, TEXT_WIDTH)[:2]):
        screen.coloured(PORTRAIT_LAST_ROW + 4 + offset, 0, Colour.WHITE, line)
    if person.posts:
        screen.coloured(PORTRAIT_LAST_ROW + 6, 0, Colour.GREEN, f"{person.posts} posts on Hackaday")
    return screen
