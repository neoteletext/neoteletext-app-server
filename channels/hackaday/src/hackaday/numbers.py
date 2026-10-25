"""The channel's page numbers.

Like Ceefax, the channel keeps its regular pages in magazine 1 and its
stories in magazines 2 to 5: story NN is page 2NN, its picture 3NN, its
comments 4NN and its links 5NN, so a viewer who has a story's number has
all four. NN runs 00 to 99 in decimal digits, which a remote control can
type, newest first.
"""

from __future__ import annotations

FRONT = 0x100
HEADLINES = 0x101
SECTIONS = 0x102
TAGS = 0x103
SEARCH = 0x104
ON_THIS_DAY = 0x105
ROULETTE = 0x106
DOOM = 0x107
QUIZ = 0x108
STATISTICS = 0x109
WRITERS = 0x110
SECURITY = 0x111
LINKS_COLUMN = 0x112
PODCAST = 0x113
CHALLENGE = 0x114
MINE = 0x115
POLL = 0x116
COMMENTS_WALL = 0x117
DISCUSSED = 0x118
THEN_AND_NOW = 0x119
ABOUT = 0x120
#: The server's own figures, live.
SERVER_STATS = 0x121
#: The first section page; each section follows.
FIRST_SECTION = 0x130
#: The archive reader: any post, each viewer's own.
READER = 0x199

#: The magazines of a story's four pages.
STORY_MAGAZINE = 0x200
PICTURE_MAGAZINE = 0x300
COMMENTS_MAGAZINE = 0x400
LINKS_MAGAZINE = 0x500
#: Stories, numbered 00 to 99.
STORIES = 100
#: A decimal place's weight in a page number's hexadecimal digits.
DECIMAL_TENS = 10
HEX_TENS = 0x10


def slot_page(magazine: int, slot: int) -> int:
    """Answer the page of story `slot` (0 to 99) in `magazine`: slot 42 of magazine 2 is page 242."""
    tens, units = divmod(slot, DECIMAL_TENS)
    return magazine + tens * HEX_TENS + units


def slot_of(page: int) -> tuple[int, int] | None:
    """Answer a story page's magazine and slot, or None for another page."""
    magazine = page & 0xF00
    if magazine not in {STORY_MAGAZINE, PICTURE_MAGAZINE, COMMENTS_MAGAZINE, LINKS_MAGAZINE}:
        return None
    tens, units = divmod(page & 0xFF, HEX_TENS)
    if tens >= DECIMAL_TENS or units >= DECIMAL_TENS:
        return None
    return magazine, tens * DECIMAL_TENS + units


def section_page(index: int) -> int:
    """Answer the page of section `index`."""
    return slot_page(FIRST_SECTION & 0xF00, (FIRST_SECTION & 0xFF) // HEX_TENS * DECIMAL_TENS + index)
