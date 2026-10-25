"""A page as an application draws it, written as TTI for the server.

A page is 25 rows of 40 character codes, its links and input modes
(docs/applications.md section 3.2). It mirrors the runtime's `Screen`
(crates/teletext-runtime).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import IntEnum

from neoteletext_app.charset import codes

#: A page's rows and columns (EN 300 706 clause 9.3: rows 0 to 24).
ROWS = 25
COLUMNS = 40
#: The header's first column of text: the page number and status come
#: before it (EN 300 706 clause 9.3.1).
HEADER_TEXT_COLUMN = 8
#: The row of the Fastext prompts.
FASTEXT_ROW = ROWS - 1
#: An empty cell.
SPACE = 0x20
#: A sixel cell is two across and three down (EN 300 706 Table 47).
SIXELS_ACROSS = 2
SIXELS_DOWN = 3
#: Every mosaic code has bit 5 set (EN 300 706 Table 47).
MOSAIC_BASE = 0x20
#: The bit of each sixel in a mosaic code, left to right, top to bottom
#: (EN 300 706 Table 47: bits 0 to 4 and 6).
SIXEL_BITS = ((0x01, 0x02), (0x04, 0x08), (0x10, 0x40))
#: TTI writes a control code as ESC followed by the code plus 0x40
#: (the engine's REQ-INP-014).
ESCAPE = 0x1B
ESCAPE_OFFSET = 0x40
#: The last control code (EN 300 706 Table 26).
LAST_ATTRIBUTE = 0x1F
#: A Fastext link that goes nowhere in TTI (`FL`).
NO_LINK = 0x8FF
#: The cursor record's flags (INPUT.md section 7.1): the receiver echoes
#: what is typed at the cursor; shows it as stars; draws a block.
ECHO = 1
MASKED = 2
BLOCK = 4
#: The lowest and highest page (EN 300 706 clause 3.1).
FIRST_PAGE = 0x100
LAST_PAGE = 0x8FF


class Colour(IntEnum):
    """The eight colours of Level 1 (EN 300 706 Table 26)."""

    BLACK = 0
    RED = 1
    GREEN = 2
    YELLOW = 3
    BLUE = 4
    MAGENTA = 5
    CYAN = 6
    WHITE = 7

    @property
    def text(self) -> int:
        """The alphanumerics colour code."""
        return int(self)

    @property
    def mosaic(self) -> int:
        """The mosaic colour code."""
        return MOSAIC_COLOUR + int(self)


#: Mosaic colour codes follow the alphanumeric ones (EN 300 706 Table 26).
MOSAIC_COLOUR = 0x10


class Attribute(IntEnum):
    """The spacing attributes beyond the colours (EN 300 706 Table 26)."""

    FLASH = 0x08
    STEADY = 0x09
    END_BOX = 0x0A
    START_BOX = 0x0B
    NORMAL_HEIGHT = 0x0C
    DOUBLE_HEIGHT = 0x0D
    CONCEAL = 0x18
    CONTIGUOUS = 0x19
    SEPARATED = 0x1A
    BLACK_BACKGROUND = 0x1C
    NEW_BACKGROUND = 0x1D
    HOLD_MOSAICS = 0x1E
    RELEASE_MOSAICS = 0x1F


def page_number(text: str) -> int:
    """Answer a page number written as the protocol writes it, "100" to "8FF"."""
    number = int(text, 16)
    if not FIRST_PAGE <= number <= LAST_PAGE or len(text) != len("100"):
        message = f"not a page: {text!r}"
        raise ValueError(message)
    return number


def page_text(number: int) -> str:
    """Answer a page number as the protocol writes it."""
    return f"{number:03X}"


def decimal_subcode(subpage: int) -> int:
    """Answer a sub-page's subcode: its number in decimal digits, as receivers show it."""
    return int(str(subpage), 16)


@dataclass
class Screen:
    """One sub-page being drawn."""

    rows: list[bytearray] = field(default_factory=lambda: [bytearray([SPACE] * COLUMNS) for _ in range(ROWS)])
    fastext_links: list[int | None] = field(default_factory=lambda: [None, None, None, None])
    index: int | None = None
    cursor_at: tuple[int, int, int, int] | None = None
    input_modes: int = 0
    #: Packets beyond the rows, as TTI `OL` lines: a picture's X/26
    #: enhancements and X/28 colours (EN 300 706 clauses 9.4 and 9.4.2),
    #: written after the rows.
    packets: list[str] = field(default_factory=list)

    def code(self, row: int, column: int, code: int) -> None:
        """Put one code; outside the page, nothing."""
        if 0 <= row < ROWS and 0 <= column < COLUMNS:
            self.rows[row][column] = code & LAST_CHARACTER_CODE

    def codes(self, row: int, column: int, values: bytes | list[int]) -> None:
        """Put codes from `column` on; what runs past the row is lost."""
        for offset, value in enumerate(values):
            self.code(row, column + offset, value)

    def text(self, row: int, column: int, text: str) -> None:
        """Write `text` from `column`, in the English set."""
        self.codes(row, column, codes(text))

    def coloured(self, row: int, column: int, colour: Colour, text: str) -> None:
        """Write `text` after its colour attribute, which takes `column`."""
        self.code(row, column, colour.text)
        self.text(row, column + 1, text)

    def centred(self, row: int, colour: Colour, text: str) -> None:
        """Write `text` centred on `row`, after its colour."""
        width = len(codes(text)) + 1
        self.coloured(row, max(0, (COLUMNS - width) // 2), colour, text)

    def double_height(self, row: int, column: int, colour: Colour, text: str) -> None:
        """Write `text` in double height: it covers `row` and the row below, which stays empty."""
        self.codes(row, column, [Attribute.DOUBLE_HEIGHT, colour.text])
        self.text(row, column + 2, text)

    def background(self, row: int, column: int, colour: Colour) -> None:
        """Start a background of `colour` from `column`: the colour, then New Background."""
        self.codes(row, column, [colour.text, Attribute.NEW_BACKGROUND])

    def fill(self, row: int, column: int, count: int, code: int) -> None:
        """Put `count` copies of `code`."""
        self.codes(row, column, [code] * count)

    def header(self, text: str) -> None:
        """Write the header's text, after the page number the receiver shows."""
        self.text(0, HEADER_TEXT_COLUMN, text[: COLUMNS - HEADER_TEXT_COLUMN])

    def fastext(self, links: list[int | None]) -> None:
        """Set the four coloured links, red to cyan."""
        self.fastext_links = [*links[:4], None, None, None, None][:4]

    def index_link(self, page: int) -> None:
        """Set the Index link (the fifth Fastext link)."""
        self.index = page

    def cursor(self, row: int, column: int, width: int, flags: int) -> None:
        """Set the input cursor and how far echo may run (INPUT.md section 7.1)."""
        self.cursor_at = (row, column, width, flags)

    def modes(self, modes: int) -> None:
        """Set the input modes the page asks for (INPUT.md section 6)."""
        self.input_modes = modes

    def row_text(self, row: int) -> str:
        """Answer a row as text, control codes as spaces."""
        return "".join(chr(code) if code >= SPACE else " " for code in self.rows[row])

    def tti(self, page: int, subpage: int = 0) -> str:
        """Answer the sub-page as TTI lines (the engine's REQ-INP-013)."""
        lines = [f"PN,{page:03X}{subpage:02d}", f"SC,{decimal_subcode(subpage):04X}"]
        for number, row in enumerate(self.rows):
            if any(code != SPACE for code in row):
                lines.append(f"OL,{number},{escaped(row)}")
        links = [*self.fastext_links, None, self.index]
        if any(link is not None for link in links):
            lines.append("FL," + ",".join(f"{link if link is not None else NO_LINK:03X}" for link in links))
        if self.cursor_at is not None:
            cursor_row, column, width, flags = self.cursor_at
            lines.append(f"CU,{cursor_row},{column},{width},{flags}")
        if self.input_modes:
            lines.append(f"IM,{self.input_modes}")
        lines.extend(self.packets)
        return "\n".join(lines) + "\n"

    def load_tti(self, text: str, *, first_row: int = 0, last_row: int = ROWS - 1) -> None:
        """Take another TTI page's rows between `first_row` and `last_row`, and its enhancement packets.

        What the engine's tools write (a converted picture, a QR code) is
        drawn into this page; its own page number, links and status are not.
        """
        for line in text.splitlines():
            line = line.rstrip("\r")  # noqa: PLW2901 - the tools write CRLF
            if not line.startswith("OL,"):
                continue
            number, _, row_text = line[len("OL,") :].partition(",")
            if not number.isdigit():
                continue
            row = int(number)
            if row >= ROWS:
                self.packets.append(line)
            elif first_row <= row <= last_row:
                self.rows[row] = bytearray(unescaped(row_text).ljust(COLUMNS, bytes([SPACE]))[:COLUMNS])


#: Codes are seven bits (EN 300 706 clause 8.2).
LAST_CHARACTER_CODE = 0x7F


def escaped(row: bytes | bytearray) -> str:
    """Answer a row in TTI's escape form, trailing spaces left out (the engine restores them)."""
    end = len(row)
    while end and row[end - 1] == SPACE:
        end -= 1
    out: list[str] = []
    for code in row[:end]:
        if code <= LAST_ATTRIBUTE:
            out.append(chr(ESCAPE) + chr(code + ESCAPE_OFFSET))
        else:
            out.append(chr(code))
    return "".join(out)


def unescaped(text: str) -> bytes:
    """Answer a TTI row's codes from its escape form."""
    out = bytearray()
    escape = False
    for character in text:
        code = ord(character) & LAST_CHARACTER_CODE
        if escape:
            out.append(code - ESCAPE_OFFSET)
            escape = False
        elif code == ESCAPE:
            escape = True
        else:
            out.append(code)
    return bytes(out)


def tti(page: int, screens: list[Screen]) -> str:
    """Answer the page's sub-pages as one TTI text: one screen is sub-page 0, more are 1 on."""
    if len(screens) == 1:
        return screens[0].tti(page, 0)
    return "".join(screen.tti(page, number) for number, screen in enumerate(screens, start=1))


class Sixels:
    """A canvas of sixels, drawn as mosaic graphics into a block of cells."""

    def __init__(self, width: int, height: int) -> None:
        """Make a clear canvas `width` by `height` sixels."""
        self.width = width
        self.height = height
        self.bits = bytearray(width * height)

    def set(self, x: int, y: int, *, on: bool = True) -> None:
        """Set or clear the sixel at `x`, `y`; outside the canvas, nothing."""
        if 0 <= x < self.width and 0 <= y < self.height:
            self.bits[y * self.width + x] = 1 if on else 0

    def get(self, x: int, y: int) -> bool:
        """Answer whether the sixel at `x`, `y` is set."""
        return 0 <= x < self.width and 0 <= y < self.height and self.bits[y * self.width + x] == 1

    def cell(self, x: int, y: int) -> int:
        """Answer the mosaic code for the cell whose top-left sixel is `x`, `y`."""
        code = MOSAIC_BASE
        for down, pair in enumerate(SIXEL_BITS):
            for across, bit in enumerate(pair):
                if self.get(x + across, y + down):
                    code |= bit
        return code

    def draw(self, screen: Screen, row: int, column: int, colour: Colour) -> None:
        """Draw the canvas from `row`, `column`; each row of cells starts with the mosaic colour in the cell before `column`."""
        for cell_row in range((self.height + SIXELS_DOWN - 1) // SIXELS_DOWN):
            screen.code(row + cell_row, column - 1, colour.mosaic)
            for cell_column in range((self.width + SIXELS_ACROSS - 1) // SIXELS_ACROSS):
                screen.code(row + cell_row, column + cell_column, self.cell(cell_column * SIXELS_ACROSS, cell_row * SIXELS_DOWN))
