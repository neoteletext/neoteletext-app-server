"""How the Hackaday channel's pages look: Ceefax's shapes in Hackaday's colours.

Every page has the service's header, a title band in double height on
rows 1 and 2, its body, and the Fastext prompts on row 24. Long text
scrolls inside the page, with a scroll bar of whole cells: a cell has one
foreground colour (EN 300 706 clause 12.2), so a block that ended part-way
through a cell would paint the track beside it.
"""

from __future__ import annotations

from dataclasses import dataclass

from neoteletext_app.charset import codes
from neoteletext_app.screen import COLUMNS, FASTEXT_ROW, SIXELS_ACROSS, Attribute, Colour, Screen, Sixels

#: The service's name in every header.
SERVICE = "HACKADAY"
#: The page everything returns to.
FRONT = 0x100
#: The first row under the title band.
BODY_ROW = 3
#: The cells between Fastext labels: the next label's colour and a space.
LABEL_GAP = 2
#: The Fastext colours, red to cyan.
KEY_COLOURS = (Colour.RED, Colour.GREEN, Colour.YELLOW, Colour.CYAN)
#: The last column, where a scroll bar goes, and the one before for its colour.
BAR_COLUMN = COLUMNS - 1
BAR_ATTRIBUTE_COLUMN = BAR_COLUMN - 1
#: Text beside a scroll bar: after its colour in column 0, a cell short of the bar's colour.
SCROLL_TEXT_WIDTH = BAR_ATTRIBUTE_COLUMN - 2
#: Text across a whole page: after its colour in column 0, less a margin.
TEXT_WIDTH = COLUMNS - 2
#: A mosaic cell with every sixel set (EN 300 706 Table 47).
FULL_BLOCK = 0x7F
#: A mosaic cell with its left column of sixels set: a thin line.
LEFT_LINE = 0x20 | 0x01 | 0x04 | 0x10

Link = tuple[str, int | None]
NO_LINK: Link = ("", None)


def label_columns(labels: list[str]) -> list[tuple[int, int] | None]:
    """Answer where each Fastext label goes on row 24: its colour's column and the column after it."""
    column = 0
    spans: list[tuple[int, int] | None] = []
    for label in labels:
        if not label:
            spans.append(None)
            continue
        width = len(codes(label))
        spans.append((column, column + 1 + width))
        column += width + LABEL_GAP
    return spans


def page(title: str, band: Colour, links: list[Link], *, title_colour: Colour = Colour.WHITE) -> Screen:
    """Answer a page with the header, a title band and the Fastext row."""
    screen = Screen()
    screen.header(f"{SERVICE}  fresh hacks every day")
    screen.background(1, 0, band)
    screen.double_height(1, 2, title_colour, title[: COLUMNS - 5])
    labels = [label for label, _ in links]
    for colour, label, span in zip(KEY_COLOURS, labels, label_columns(labels), strict=False):
        if span is not None:
            screen.coloured(FASTEXT_ROW, span[0], colour, label)
    screen.fastext([target for _, target in links])
    screen.index_link(FRONT)
    return screen


def wrap(text: str, width: int) -> list[str]:
    """Answer `text` broken at spaces into lines of at most `width` characters; a longer word is cut."""
    lines: list[str] = []
    line = ""
    for word in text.split():
        while len(codes(word)) > width:
            if line:
                lines.append(line)
                line = ""
            lines.append(word[:width])
            word = word[width:]  # noqa: PLW2901 - the rest of a cut word
        candidate = f"{line} {word}" if line else word
        if len(codes(candidate)) > width:
            lines.append(line)
            line = word
        else:
            line = candidate
    if line:
        lines.append(line)
    return lines


@dataclass(frozen=True)
class Line:
    """A line of a scrolling text: its colour and words."""

    colour: Colour
    text: str


def thumb(top: int, total: int, track: int, shown: int) -> tuple[int, int]:
    """Answer the scroll bar's block, in cells: its first cell and its length."""
    if total <= shown:
        return 0, track
    length = max(1, min(track, round(track * shown / total)))
    last_top = total - shown
    return round((track - length) * min(top, last_top) / last_top), length


def top_at(start: int, total: int, rows: int) -> int:
    """Answer the first line shown when the scroll bar's block is dragged to start at cell `start`: `thumb` turned round."""
    _, length = thumb(0, total, rows, rows)
    room = rows - length
    most = last_top(total, rows)
    if room <= 0 or most <= 0:
        return 0
    return max(0, min(most, round(start * most / room)))


def scroll(screen: Screen, lines: list[Line], top: int, first_row: int, rows: int) -> None:
    """Draw `rows` lines of `lines` from `top`, with a scroll bar beside them when they do not fit, which drags pull."""
    for offset, line in enumerate(lines[top : top + rows]):
        if line.text:
            screen.coloured(first_row + offset, 0, line.colour, line.text)
    if len(lines) <= rows:
        return
    start, length = thumb(top, len(lines), rows, rows)
    for cell in range(rows):
        inside = start <= cell < start + length
        screen.code(first_row + cell, BAR_ATTRIBUTE_COLUMN, (Colour.WHITE if inside else Colour.BLUE).mosaic)
        screen.code(first_row + cell, BAR_COLUMN, FULL_BLOCK if inside else LEFT_LINE)


def last_top(total: int, rows: int) -> int:
    """Answer the first line shown when a text of `total` lines is scrolled to its end."""
    return max(0, total - rows)


def bar(screen: Screen, row: int, column: int, cells: int, value: float, most: float, colour: Colour) -> None:  # noqa: PLR0913, PLR0917 - a bar's place, size, value and colour
    """Draw a horizontal bar of up to `cells` cells, `value` of `most`, to the half cell."""
    halves = 0 if most <= 0 else round(2 * cells * min(value, most) / most)
    screen.code(row, column, colour.mosaic)
    full, half = divmod(halves, 2)
    screen.fill(row, column + 1, full, FULL_BLOCK)
    if half:
        # The left column of sixels: half a cell.
        screen.code(row, column + 1 + full, LEFT_LINE)


# Large digits, five sixels wide and seven high, for countdowns.
DIGITS = {
    "0": ("#####", "#...#", "#..##", "#.#.#", "##..#", "#...#", "#####"),
    "1": ("..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."),
    "2": ("#####", "....#", "....#", "#####", "#....", "#....", "#####"),
    "3": ("#####", "....#", "....#", ".####", "....#", "....#", "#####"),
    "4": ("#...#", "#...#", "#...#", "#####", "....#", "....#", "....#"),
    "5": ("#####", "#....", "#....", "#####", "....#", "....#", "#####"),
    "6": ("#####", "#....", "#....", "#####", "#...#", "#...#", "#####"),
    "7": ("#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."),
    "8": ("#####", "#...#", "#...#", "#####", "#...#", "#...#", "#####"),
    "9": ("#####", "#...#", "#...#", "#####", "....#", "....#", "#####"),
    ":": (".....", "..#..", "..#..", ".....", "..#..", "..#..", "....."),
    "-": (".....", ".....", ".....", "#####", ".....", ".....", "....."),
}
#: A large digit's size in sixels, and the gap after it.
DIGIT_WIDTH = 5
DIGIT_HEIGHT = 7
DIGIT_GAP = 1


def big_text(screen: Screen, row: int, column: int, text: str, colour: Colour) -> int:
    """Draw `text` in large mosaic digits from `row`, `column`; answer the cells it took across."""
    width = len(text) * (DIGIT_WIDTH + DIGIT_GAP)
    canvas = Sixels(width, DIGIT_HEIGHT)
    for place, character in enumerate(text):
        for y, pattern in enumerate(DIGITS.get(character, DIGITS["-"])):
            for x, sixel in enumerate(pattern):
                if sixel == "#":
                    canvas.set(place * (DIGIT_WIDTH + DIGIT_GAP) + x, y)
    canvas.draw(screen, row, column, colour)
    return -(-width // SIXELS_ACROSS)


#: Hackaday's emblem, the Jolly Wrencher: a skull over crossed wrenches,
#: sampled from Hackaday's own 1024-pixel logo
#: (hackaday.com/wp-content/themes/hackaday-2/img/hackaday-logo_1024x1024.png),
#: cropped to the emblem, each sixel set where more than its share of the
#: logo's area is white: 55% for the small one, 62% for the large, which
#: keeps the eyes and the gaps between the wrenches open. A cell is 12 by
#: 20 pixels as the browser draws it, so a sixel is 6 by 6.67, nearly
#: square, and the emblem keeps its shape: 26 by 21 sixels is 13 cells
#: across and 7 down.
JOLLY_WRENCHER = (
    "...##................##...",
    "....##..............##....",
    "....##.............###....",
    "##.####............####.##",
    ".#######..........#######.",
    "..#######.######.#######..",
    ".....###.########.##......",
    "........##########........",
    ".......############.......",
    ".......##..####..##.......",
    ".......##...##...##.......",
    ".......##.######.##.......",
    ".......############.......",
    ".......#####.#####........",
    ".....##.#############.....",
    "..######.########.######..",
    "########.########.#######.",
    "#..####............####..#",
    "....##.............###....",
    "....##..............##....",
    "...##................##...",
)
#: The large one, for the whole page: 70 by 63 sixels, 35 cells across and
#: 21 down, sampled the same way.
JOLLY_WRENCHER_LARGE = (
    ".......#####.............................................######.......",
    "........######..........................................#####.........",
    ".........######.......................................######..........",
    "..........######......................................#####...........",
    "............#####....................................#####............",
    "............#####....................................#####............",
    "...........######...................................#######...........",
    "#.........#######...................................########.........#",
    "##.......#########..................................#########.......##",
    "###.....##########..................................##########.....###",
    "####...###########.................................############...###.",
    "#####.#############...............................###################.",
    ".###################.............................####################.",
    ".#####################..........................####################..",
    "..#####################........#######.........####################...",
    "...#####################....#############.....####################....",
    ".....###################...################...###################.....",
    "............###########..###################...##########.............",
    ".............#########..######################..########..............",
    "..............#######..########################..######...............",
    "................####..#########################..#####................",
    ".................##..###########################..###.................",
    "..................#..############################..#..................",
    "....................#############################.....................",
    "....................##############################....................",
    "...................###############################....................",
    "...................################################...................",
    "...................################################...................",
    "...................######......########......######...................",
    "..................######.......#######........#####...................",
    "..................#####.........######........#####...................",
    "..................#####.........######.........####...................",
    "..................#####........#######.........####...................",
    "..................#####.......##########.......####...................",
    "..................#####.....##############.....####...................",
    "...................####...#################...#####...................",
    "...................#####..#################..######...................",
    "...................###############.################...................",
    "...................###############..##############....................",
    "....................#############...##############....................",
    ".................#..#############...##############.#..................",
    "................###..############.#..############..###................",
    "...............####..###########################..#####...............",
    "..............######..##########################..######..............",
    ".............########..########################..########.............",
    ".......##############..#######################..##########.####.......",
    "....##################...####################..##################.....",
    "...####################..###################...####################...",
    "..####################...####################..#####################..",
    ".####################....######.######.######....###################..",
    ".###################.....######.#####..#####......###################.",
    "#####..############.......####...####...###........############..####.",
    "###.....##########..................................##########....####",
    "##.......#########..................................#########......###",
    "#........#########..................................########........##",
    "..........#######...................................#######...........",
    "...........######...................................######............",
    "............#####....................................#####............",
    "...........#####.....................................######...........",
    "..........######......................................######..........",
    ".........######........................................######.........",
    ".......######...........................................######........",
    ".......####................................................####.......",
)


def emblem(screen: Screen, row: int, column: int, colour: Colour, bitmap: tuple[str, ...] = JOLLY_WRENCHER) -> None:
    """Draw the Jolly Wrencher, small or large, with its top left cell at `row`, `column`."""
    canvas = Sixels(len(bitmap[0]), len(bitmap))
    for y, line in enumerate(bitmap):
        for x, sixel in enumerate(line):
            if sixel == "#":
                canvas.set(x, y)
    canvas.draw(screen, row, column, colour)


def flashing(screen: Screen, row: int, column: int, colour: Colour, text: str) -> None:
    """Write `text` flashing (EN 300 706 Table 26), steady again after it."""
    screen.codes(row, column, [colour.text, Attribute.FLASH])
    screen.text(row, column + 2, text)
    screen.code(row, column + 2 + len(codes(text)), Attribute.STEADY)


def concealed(screen: Screen, row: int, column: int, colour: Colour, text: str) -> None:
    """Write `text` concealed: shown when the viewer presses Reveal (EN 300 706 Table 26)."""
    screen.codes(row, column, [colour.text, Attribute.CONCEAL])
    screen.text(row, column + 2, text)


def band(screen: Screen, row: int, background: Colour, colour: Colour, text: str) -> None:
    """Write a band across `row`: `text` in `colour` on `background`."""
    screen.background(row, 0, background)
    screen.coloured(row, 2, colour, text)
