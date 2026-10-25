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
    page,
    scroll,
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
#: A story's views, chosen with the coloured keys: its text, its lead
#: picture, its comments and its links; the fourth key is the next story.
STORY, PICTURE, COMMENTS, LINKS = "story", "picture", "comments", "links"
VIEWS = (STORY, PICTURE, COMMENTS, LINKS)
NEXT = "next"


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
    # The title band is the emblem's.
    screen.rows[1] = bytearray([0x20] * COLUMNS)
    emblem(screen, 1, 1, Colour.WHITE)
    screen.double_height(1, 16, Colour.YELLOW, "HACKADAY")
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
    for offset, line in enumerate(wrap(top.title, TEXT_WIDTH)[:2]):
        screen.coloured(9 + offset, 0, Colour.WHITE, line)
    for offset, line in enumerate(wrap(top.excerpt, TEXT_WIDTH)[:2]):
        screen.coloured(11 + offset, 0, Colour.CYAN, line)
    band(screen, 13, Colour.BLUE, Colour.WHITE, "THE MAGAZINES")
    for index, mag in enumerate(site.SUBJECTS):
        row, column = 14 + index // 2, (index % 2) * 20
        screen.coloured(row, column, ink_on_black(mag), f"{mag.front:03X} {mag.short}")
        screen.coloured(row, column + 13, Colour.WHITE, f"{len(placed.stories.get(mag.number, [])):3d}")
    screen.coloured(17, 20, Colour.YELLOW, "101 Headlines")
    for offset, post in enumerate(posts[1 : 1 + FRONT_HEADLINES]):
        row = 18 + offset
        screen.coloured(row, 0, Colour.YELLOW, number_of(placed, post))
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
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
    return [Line(Colour.WHITE, f"{number_of(placed, post)} {fit(post.title, SCROLL_TEXT_WIDTH - NUMBER_WIDTH)}") for post in posts]


def headlines(posts: list[Post], placed: Placed, top: int) -> Screen:
    """Draw page 101: every story the channel holds, newest first, in every magazine."""
    return scrolling(
        "Headlines",
        Colour.RED,
        headline_lines(posts, placed),
        top,
        [("Front", numbers.FRONT), ("Magazines", numbers.MAGAZINES), ("Tags", numbers.POPULAR_TAGS), ("Search", numbers.SEARCH)],
    )


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
    screen.coloured(BODY_ROW, 0, Colour.YELLOW, fit(f"{number_of(placed, lead)} {lead.title}", TEXT_WIDTH))
    for offset, line in enumerate(wrap(lead.excerpt, TEXT_WIDTH)[:3]):
        screen.coloured(BODY_ROW + 1 + offset, 0, Colour.CYAN, line)
    band(screen, 7, mag.band, mag.ink, f"LATEST: {len(posts)} STORIES ON {mag.stories_page:03X}")
    for offset, post in enumerate(posts[1 : 1 + MAGAZINE_HEADLINES]):
        row = 8 + offset
        screen.coloured(row, 0, Colour.YELLOW, number_of(placed, post))
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
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


def columns_index(posts: list[Post], placed: Placed) -> Screen:
    """Draw page 110: Hackaday's columns, the latest edition of each and where it is."""
    screen = page(
        "Columns",
        Colour.RED,
        [("Front", numbers.FRONT), ("Links", numbers.LINKS_COLUMN), ("Podcast", numbers.PODCAST), ("Security", numbers.SECURITY)],
        title_colour=Colour.YELLOW,
    )
    held = [post for post in posts if post.has("hackaday columns") or post.has("podcasts") or post.has("hackaday links")]
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "The regulars, newest first")
    for offset, post in enumerate(held[: LIST_ROWS - 1]):
        row = BODY_ROW + 1 + offset
        screen.coloured(row, 0, Colour.YELLOW, number_of(placed, post))
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "111 Links  112 Podcast  701 Security")
    return screen


# A story, and its views.


def story_links(view: str, post_page: int, next_page: int) -> list[Link]:
    """Answer a story's prompts in `view`: the other three views, then the next story. Each view is the story's own page."""
    named = {STORY: "Story", PICTURE: "Picture", COMMENTS: "Comments", LINKS: "Links"}
    return [*((named[other], post_page) for other in VIEWS if other != view), ("Next", next_page)]


def story_choices(view: str) -> list[str]:
    """Answer what each coloured key does in `view`: a view, or the next story."""
    return [*(other for other in VIEWS if other != view), NEXT]


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


def story(post: Post, mag: Magazine, post_page: int, next_page: int, top: int) -> Screen:
    """Draw a story: its title, who and when, and its text, which scrolls."""
    screen = page(mag.name, mag.band, story_links(STORY, post_page, next_page), title_colour=mag.ink)
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
    screen.coloured(23, 0, Colour.GREEN, fit(f"PgDn reads on; index {mag.front:03X}", TEXT_WIDTH))
    screen.index_link(mag.front)
    return screen


def picture(post: Post, mag: Magazine, post_page: int, next_page: int, tti: str | None) -> Screen:
    """Draw a story's lead picture in mosaics, or a word that it is coming."""
    links = story_links(PICTURE, post_page, next_page)
    if tti is None:
        screen = page("Picture", mag.band, links, title_colour=mag.ink)
        screen.centred(10, Colour.WHITE, "This picture is being drawn")
        screen.centred(12, Colour.CYAN, "in teletext mosaics: back soon")
        screen.centred(14, Colour.YELLOW, fit(post.title, 36))
        screen.index_link(mag.front)
        return screen
    screen = Screen()
    screen.header(f"HACKADAY  picture {post_page:03X}")
    # The picture takes rows 1 to 24 (the engine's converter); the coloured
    # keys still choose the views, though row 24 shows the picture.
    screen.load_tti(tti, first_row=1, last_row=FASTEXT_ROW)
    screen.fastext([target for _, target in links])
    screen.index_link(mag.front)
    return screen


def comment_lines(post: Post) -> list[Line]:
    """Answer every one of a post's comments as lines, replies indented under what they answer."""
    lines: list[Line] = []
    for depth, comment in threaded(post.comments):
        indent = " " * (min(depth, DEEPEST_INDENT) * INDENT_STEP)
        width = SCROLL_TEXT_WIDTH - len(indent)
        lines.append(Line(Colour.CYAN, indent + fit(f"{comment.author}, {short_date(comment.date)}", width)))
        lines.extend(Line(Colour.WHITE, indent + line) for line in wrap(comment.text, width))
        lines.append(Line(Colour.WHITE, ""))
    return lines or [Line(Colour.WHITE, "No comments yet: be the first on"), Line(Colour.WHITE, "hackaday.com")]


def comments(post: Post, mag: Magazine, post_page: int, next_page: int, top: int) -> Screen:
    """Draw a story's comments, all of them on one page, which scrolls."""
    screen = scrolling(
        fit(f"{post.comment_count} comments", 30),
        Colour.GREEN,
        comment_lines(post),
        top,
        story_links(COMMENTS, post_page, next_page),
        title_colour=Colour.BLACK,
    )
    screen.index_link(mag.front)
    return screen


def links_page(post: Post, mag: Magazine, post_page: int, next_page: int, qr: str | None, what: str) -> Screen:  # noqa: PLR0913, PLR0917 - a link page: the story, its place, its code and what it leads to
    """Draw one of a story's link pages: a QR code a phone can follow, and what it leads to."""
    screen = page("Read on", Colour.MAGENTA, story_links(LINKS, post_page, next_page))
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
    screen.index_link(mag.front)
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
    """Draw page 133: a post from the archive, which scrolls."""
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
    """Draw page 801, Games' feature: days since the last port of DOOM, and the ports."""
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
    screen.coloured(BODY_ROW + 4, 0, Colour.CYAN, fit(f"Last: {ports[0].title}", TEXT_WIDTH))
    band(screen, 9, Colour.RED, Colour.YELLOW, f"Recent ports: {len(ports)} found")
    for offset, post in enumerate(ports[:11]):
        screen.coloured(10 + offset, 0, Colour.WHITE, fit(f"{post.date.year} {post.title}", TEXT_WIDTH))
    screen.coloured(HINT_ROW, 0, Colour.GREEN, "Can it run DOOM? On Hackaday, it can")
    return screen


def statistics(posts: list[Post]) -> Screen:
    """Draw page 109: the stories by magazine, by hour of posting, and the comments."""
    screen = page(
        "Statistics",
        Colour.GREEN,
        [("Front", numbers.FRONT), ("Writers", numbers.WRITERS), ("Discussed", numbers.DISCUSSED), ("Magazines", numbers.MAGAZINES)],
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
    """Draw page 108: who writes Hackaday, and how much."""
    screen = page(
        "Writers",
        Colour.MAGENTA,
        [("Front", numbers.FRONT), ("Stats", numbers.STATISTICS), ("People", numbers.PEOPLE), ("Headlines", numbers.HEADLINES)],
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
    """Draw page 106: the most argued-about stories."""
    screen = page(
        "Most discussed",
        Colour.RED,
        [("Front", numbers.FRONT), ("Comments", numbers.COMMENTS_WALL), ("Stats", numbers.STATISTICS), ("Headlines", numbers.HEADLINES)],
    )
    ranked = sorted(posts, key=lambda post: post.comment_count, reverse=True)[:9]
    most = ranked[0].comment_count if ranked and ranked[0].comment_count else 1
    for offset, post in enumerate(ranked):
        row = BODY_ROW + offset * 2
        screen.coloured(row, 0, Colour.YELLOW, number_of(placed, post))
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
        bar(screen, row + 1, NUMBER_WIDTH, 26, post.comment_count, most, Colour.RED)
        screen.coloured(row + 1, 33, Colour.CYAN, f"{post.comment_count:4d}")
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


def comments_wall(posts: list[Post], placed: Placed, top: int) -> Screen:
    """Draw page 107: the newest comments on every story."""
    links: list[Link] = [
        ("Front", numbers.FRONT),
        ("Discussed", numbers.DISCUSSED),
        ("Headlines", numbers.HEADLINES),
        ("Stats", numbers.STATISTICS),
    ]
    return scrolling("Comments", Colour.GREEN, comments_wall_lines(posts, placed), top, links, title_colour=Colour.BLACK)


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
    band(screen, 10, Colour.MAGENTA, Colour.WHITE, f"Entries written up: {len(entries)}")
    for offset, post in enumerate(entries[:10]):
        title = post.title.removeprefix("2026 Retrocomputing Challenge: ")
        screen.coloured(11 + offset, 0, Colour.WHITE, fit(title, TEXT_WIDTH))
    return screen


def then_and_now(first: list[Post], latest: list[Post]) -> Screen:
    """Draw page 132: Hackaday's first posts beside today's."""
    screen = page(
        "Then and now",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Archive", numbers.ON_THIS_DAY), ("Roulette", numbers.ROULETTE), ("Challenge", numbers.CHALLENGE)],
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
    """Draw page 190: how to use the channel, and where everything is."""
    screen = page(
        "How it works",
        Colour.BLUE,
        [("Front", numbers.FRONT), ("Magazines", numbers.MAGAZINES), ("Tags", numbers.POPULAR_TAGS), ("Archive", numbers.ON_THIS_DAY)],
    )
    rows = [
        (Colour.YELLOW, "Hackaday, live, as 1980s teletext"),
        (Colour.WHITE, "Type a page number, press a coloured key"),
        (Colour.WHITE, "for its prompt, or click a number."),
        (Colour.CYAN, "1xx Hackaday  2xx Retro  3xx Hardware"),
        (Colour.CYAN, "4xx Making    5xx Radio  6xx Science"),
        (Colour.CYAN, "7xx Software and security  8xx Games"),
        (Colour.GREEN, "N00 front  N02 stories  N03 tags"),
        (Colour.GREEN, "N04-N09 its busiest tags  N10+ stories"),
        (Colour.WHITE, "In a story the colours show its picture,"),
        (Colour.WHITE, "comments and links, or the next story."),
        (Colour.GREEN, "101 headlines 102 magazines 103 tags"),
        (Colour.GREEN, "104 tags A-Z  105 search  106 discussed"),
        (Colour.GREEN, "107 comments  108 writers  109 stats"),
        (Colour.GREEN, "110 columns   112 podcast  113 sounds"),
        (Colour.GREEN, "120 people  130 on this day  131 roulette"),
        (Colour.GREEN, "140 quiz  141 poll  142 my Hackaday"),
        (Colour.GREEN, "201 Retro Challenge  801 DOOM watch"),
        (Colour.WHITE, "Long pages scroll: the keys, the wheel"),
        (Colour.WHITE, "or the bar at the right, pulled."),
        (Colour.MAGENTA, "A Rust server, a Python channel: live"),
    ]
    for offset, (colour, text) in enumerate(rows):
        screen.coloured(BODY_ROW + offset - 1 + 1, 0, colour, text)
    return screen


def quiz(post: Post | None, answers: list[str], picked: int | None, score: tuple[int, int], best: int) -> Screen:
    """Draw a quiz question: which magazine is this story in? The answer is concealed until it is given."""
    screen = page("Guess the magazine", Colour.YELLOW, [(answer[:9], None) for answer in answers] or [NO_LINK], title_colour=Colour.BLUE)
    if post is None:
        screen.centred(10, Colour.WHITE, "Fetching a question...")
        return screen
    screen.coloured(BODY_ROW, 0, Colour.CYAN, "Which magazine is this story in?")
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
    for offset, post in enumerate(picked):
        row = first + offset
        if row >= HINT_ROW:
            break
        screen.coloured(row, 0, Colour.YELLOW, number_of(placed, post))
        screen.coloured(row, NUMBER_WIDTH, Colour.WHITE, fit(post.title, COLUMNS - NUMBER_WIDTH - 1))
    return screen


def search(typed: str, results: list[Post], chosen: int, *, searching: bool) -> Screen:
    """Draw page 105: words typed, and the archive's posts for them."""
    items = [Choice(f"{post.date.year} {post.title}", post) for post in results]
    intro = ["Type words, then Enter: the whole", "archive, back to 2004", ""]
    screen = choices(
        "Search",
        Colour.BLUE,
        items,
        chosen,
        [("Front", numbers.FRONT), ("Tags", numbers.POPULAR_TAGS), ("Archive", numbers.ON_THIS_DAY), ("Roulette", numbers.ROULETTE)],
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
    """Draw page 150: the Rust server's own figures, live (docs/applications.md section 7)."""
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
    """Draw page 113: the Hackaday Podcast's sound grabs, played with Enter."""
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
    for offset, line in enumerate(two_lines(detail)):
        screen.coloured(PORTRAIT_LAST_ROW + 4 + offset, 0, Colour.WHITE, line)
    if person.posts:
        screen.coloured(PORTRAIT_LAST_ROW + 6, 0, Colour.GREEN, f"{person.posts} posts on Hackaday")
    return screen
