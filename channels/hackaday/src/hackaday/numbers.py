"""The channel's page numbers (`site.py` says what is where).

Every page number is three decimal digits, which a remote control can
type: the magazine, then 00 to 99.
"""

from __future__ import annotations

#: Magazine 1: Hackaday itself.
HOME = 1
FRONT = 0x100
HEADLINES = 0x101
MAGAZINES = 0x102
POPULAR_TAGS = 0x103
TAGS = 0x104
SEARCH = 0x105
DISCUSSED = 0x106
COMMENTS_WALL = 0x107
WRITERS = 0x108
STATISTICS = 0x109
COLUMNS = 0x110
LINKS_COLUMN = 0x111
PODCAST = 0x112
SOUNDBOARD = 0x113
#: The people of Hackaday, and each person's page after it.
PEOPLE = 0x120
ON_THIS_DAY = 0x130
ROULETTE = 0x131
THEN_AND_NOW = 0x132
#: The archive reader: any post, each viewer's own.
READER = 0x133
QUIZ = 0x140
POLL = 0x141
MINE = 0x142
#: The server's own figures, live.
SERVER_STATS = 0x150
ABOUT = 0x190
#: Magazine 1's stories: columns and news that name no subject, 160 to 189.
HOME_FIRST_STORY = 60
HOME_STORY_SLOTS = 30

#: In each subject magazine: its feature, its list of stories, its tags,
#: its tag pages and its stories, as places 00 to 99.
FEATURE = 1
ALL_STORIES = 2
TAG_INDEX = 3
FIRST_TAG = 4
TAG_PAGES = 6
FIRST_STORY = 10
STORY_SLOTS = 90
#: The subject magazines' features.
CHALLENGE = 0x201
SECURITY = 0x701
DOOM = 0x801

#: A decimal place's weight in a page number's hexadecimal digits.
DECIMAL_TENS = 10
HEX_TENS = 0x10
#: The places of a magazine, 00 to 99.
PLACES = 100
#: The magazines, 1 to 8 (EN 300 706 clause 3.1: magazine 8 is written 0).
FIRST_MAGAZINE = 1
LAST_MAGAZINE = 8


def page(magazine: int, place: int) -> int:
    """Answer place `place` (0 to 99) of `magazine`: place 42 of magazine 2 is page 242."""
    tens, units = divmod(place, DECIMAL_TENS)
    return magazine * 0x100 + tens * HEX_TENS + units


def decimal(number: int) -> tuple[int, int] | None:
    """Answer a page's magazine and place, or None when its digits are not decimal."""
    magazine = number >> 8
    tens, units = divmod(number & 0xFF, HEX_TENS)
    if not FIRST_MAGAZINE <= magazine <= LAST_MAGAZINE or tens >= DECIMAL_TENS or units >= DECIMAL_TENS:
        return None
    return magazine, tens * DECIMAL_TENS + units


def is_story(number: int) -> bool:
    """Answer whether `number` is a story's page."""
    found = decimal(number)
    if found is None:
        return False
    magazine, place = found
    if magazine == HOME:
        return HOME_FIRST_STORY <= place < HOME_FIRST_STORY + HOME_STORY_SLOTS
    return place >= FIRST_STORY
