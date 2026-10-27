"""The kit: text in the English set, screens as TTI, events, and page selection."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

import pytest
from neoteletext_app.app import App, Event, page_hears, session_hears
from neoteletext_app.charset import codes, fold
from neoteletext_app.screen import Attribute, Colour, Screen, Sixels, escaped, page_number, tti
from neoteletext_app.sound import RATE, chime, tone

if TYPE_CHECKING:
    from collections.abc import Callable


def test_text_uses_the_english_sets_national_positions() -> None:
    # EN 300 706 Table 36: # is 0x5F and £ is 0x23 in the English set.
    assert codes("#1 £2") == bytes([0x5F, 0x31, 0x20, 0x23, 0x32])
    assert fold("\u201cSmart\u201d quotes \u2013 and caf\u00e9\u2026") == '"Smart" quotes - and cafe...'
    assert fold("[x]_{y}") == "(x)-(y)"
    assert fold("中") == "?"


def test_a_screen_is_written_as_tti_the_engine_reads() -> None:
    screen = Screen()
    screen.header("HACKADAY")
    screen.double_height(1, 0, Colour.YELLOW, "Hello")
    screen.coloured(3, 0, Colour.CYAN, "#hash")
    screen.fastext([0x200, 0x300, None, 0x101])
    screen.index_link(0x100)
    screen.cursor(22, 4, 20, 1)
    text = screen.tti(0x100)
    lines = text.splitlines()
    assert lines[0] == "PN,10000"
    assert lines[1] == "SC,0000"
    assert "OL,0,        HACKADAY" in lines
    # Double height and yellow, escaped: ESC then the code plus 0x40.
    assert "OL,1,\x1bM\x1bCHello" in lines
    assert "OL,3,\x1bF_hash" in lines
    assert "FL,200,300,8FF,101,8FF,100" in lines
    assert "CU,22,4,20,1" in lines
    assert not any(line.startswith("IM,") for line in lines), "a page asks for no input: every event is sent (INPUT.md section 6.3)"


def test_sub_pages_are_numbered_from_one_in_decimal() -> None:
    screens = [Screen() for _ in range(12)]
    text = tti(0x200, screens)
    assert "PN,20001" in text
    assert "PN,20012" in text
    # Sub-page 12's subcode is 0012, in decimal digits as receivers show it.
    assert "SC,0012" in text


def test_trailing_spaces_are_left_to_the_reader() -> None:
    assert escaped(bytearray(b"ab   ")) == "ab"
    assert escaped(bytearray([Attribute.NEW_BACKGROUND, 0x41])) == "\x1b]A"


def test_sixels_make_mosaic_codes() -> None:
    canvas = Sixels(4, 3)
    for y in range(3):
        canvas.set(0, y)
        canvas.set(1, y)
    screen = Screen()
    canvas.draw(screen, 5, 1, Colour.RED)
    # A full cell is every sixel bit and bit 5 (EN 300 706 Table 47).
    assert screen.rows[5][0] == Colour.RED.mosaic
    assert screen.rows[5][1] == 0x7F
    assert screen.rows[5][2] == 0x20


def test_page_numbers_are_three_hexadecimal_digits() -> None:
    assert page_number("1A0") == 0x1A0
    for bad in ["099", "900", "1000", "zz"]:
        with pytest.raises(ValueError, match=r"."):
            page_number(bad)


class Recorder(App):
    """An app whose lines are kept, not written."""

    def __init__(self) -> None:
        super().__init__({0x100: "Front", 0x200: "Story"})
        self.sent: list[dict[str, Any]] = []

    def send(self, message: dict[str, Any]) -> None:
        self.sent.append(json.loads(json.dumps(message)))


def run(app: App, *messages: dict[str, Any]) -> None:
    import asyncio  # noqa: PLC0415 - only the tests drive the loop by hand

    async def drive() -> None:
        for message in messages:
            await app.receive(message)

    asyncio.run(drive())


def key(name: str) -> dict[str, Any]:
    return {"kind": "key", "action": "press", "key": name}


def test_digits_and_coloured_keys_select_pages() -> None:
    app = Recorder()
    front = Screen()
    front.fastext([0x200, None, None, None])
    app.publish(0x100, front)
    run(
        app,
        {"type": "open", "session": 5, "page": "100"},
        {"type": "input", "session": 5, "page": "100", "event": {"kind": "text", "text": "2"}},
        {"type": "input", "session": 5, "page": "100", "event": {"kind": "text", "text": "0"}},
        {"type": "input", "session": 5, "page": "100", "event": {"kind": "text", "text": "0"}},
        {"type": "input", "session": 5, "page": "100", "event": key("F1")},
        {"type": "input", "session": 5, "page": "100", "event": key("ColorF1Green")},
    )
    gotos = [message for message in app.sent if message["type"] == "goto"]
    assert gotos == [{"type": "goto", "session": 5, "page": "200"}, {"type": "goto", "session": 5, "page": "200"}]


def test_typing_comes_from_a_keys_press_with_its_text_and_from_raw_text() -> None:
    def typed(value: dict[str, Any]) -> str | None:
        return Event.of(value).typed

    assert typed({"kind": "key", "action": "press", "key": "e", "text": "\u00e9"}) == "\u00e9"
    assert typed({"kind": "key", "action": "repeat", "key": "a", "text": "a"}) == "a"
    assert typed({"kind": "key", "action": "release", "key": "a", "text": "a"}) is None, "a release types nothing"
    assert typed({"kind": "key", "action": "press", "key": "a"}) is None, "a key that produced no text types nothing"
    assert typed({"kind": "key", "action": "press", "key": "Enter", "text": "\r"}) is None, "a control character is the key's"
    assert typed({"kind": "text", "text": "\u4f60\u597d"}) == "\u4f60\u597d", "an input method's committed text"
    assert typed({"kind": "paste", "text": "a"}) is None
    assert Event.of({"kind": "key", "action": "press", "key": "Numpad5", "text": "5"}).character == "5", "the keypad's digit"


def test_a_hover_is_read_and_selects_nothing() -> None:
    hover = {"kind": "hover", "action": "enter", "entered": True, "column": 1, "row": 3}
    assert Event.of(hover) == Event(kind="hover", action="enter", entered=True, column=1, row=3)
    assert not Event.of({**hover, "action": "leave", "entered": False}).entered
    app = Recorder()
    screen = Screen()
    screen.coloured(3, 0, Colour.WHITE, "See 200")
    app.publish(0x100, screen)
    run(app, {"type": "input", "session": 1, "page": "100", "event": {**hover, "column": 5}})
    assert not [message for message in app.sent if message["type"] == "goto"], "a hover over a page number is not a click"


def test_digits_typed_on_the_keys_select_pages() -> None:
    app = Recorder()
    app.publish(0x100, Screen())
    app.publish(0x200, Screen())
    run(
        app,
        *(
            {"type": "input", "session": 5, "page": "100", "event": {"kind": "key", "action": "press", "key": digit, "text": digit}}
            for digit in "200"
        ),
    )
    assert [message for message in app.sent if message["type"] == "goto"] == [{"type": "goto", "session": 5, "page": "200"}]


def test_a_page_that_takes_keys_keeps_them() -> None:
    class Typing(Recorder):
        async def on_input(self, session: int, page: int, event: Event) -> bool:  # noqa: ARG002
            return True

    app = Typing()
    run(app, *({"type": "input", "session": 1, "page": "100", "event": {"kind": "text", "text": digit}} for digit in "200"))
    assert not [message for message in app.sent if message["type"] == "goto"]


def test_sections_are_declared_and_a_viewers_own_page_released() -> None:
    app = Recorder()
    app.sections = {0x200: "Stories"}
    assert app.declaration()["pages"] == [{"page": "100", "title": "Front"}, {"page": "200", "title": "Story", "section": "Stories"}]
    app.publish(0x100, Screen(), 3)
    app.release(3, 0x100)
    app.release(3, 0x100)
    assert [message for message in app.sent if message["type"] == "release"] == [{"type": "release", "session": 3, "page": "100"}], "once"


def test_a_click_on_a_prompt_or_a_page_number_follows_it() -> None:
    app = Recorder()
    front = Screen()
    front.coloured(5, 0, Colour.WHITE, "See 200 or 9999")
    front.coloured(24, 0, Colour.RED, "Red")
    front.coloured(24, 10, Colour.GREEN, "Green")
    front.fastext([0x200, 0x100, None, None])
    app.publish(0x100, front)

    def click(column: int, row: int) -> dict[str, Any]:
        return {
            "type": "input",
            "session": 1,
            "page": "100",
            "event": {"kind": "mouse", "action": "press", "button": 0, "column": column, "row": row},
        }

    run(app, click(6, 5), click(13, 5), click(3, 24), click(12, 24))
    assert [message["page"] for message in app.sent if message["type"] == "goto"] == ["200", "200", "100"]


def test_publishing_an_undeclared_page_is_refused() -> None:
    with pytest.raises(ValueError, match="300 was not declared"):
        Recorder().publish(0x300, Screen())


def test_sounds_go_as_base64_samples_to_a_session_or_a_page() -> None:
    app = Recorder()
    app.sound_object(session_hears(7), 2, RATE, chime([72, 76]))
    app.play(page_hears(0x100), 1, 2, gain=60, page=0x100)
    sound, play = app.sent
    assert sound["to"] == {"session": 7}
    assert sound["object"]["rate"] == RATE
    assert len(sound["object"]["pcm"]) > 0
    assert play["to"] == {"page": "100"}
    assert play["play"] == {"channel": 1, "handle": 2, "generation": 0, "gain": 60, "loop": False, "page": "100"}


def test_a_tone_is_faded_and_bounded() -> None:
    samples = tone(440, 100)
    assert len(samples) == RATE // 10
    assert samples[0] == 0
    peak: Callable[[int], int] = abs
    assert max(map(peak, samples)) <= 8_000


def test_another_pages_rows_and_packets_are_taken() -> None:
    picture = (
        "PN,10000\r\nSC,0000\r\nPS,8000\r\nFL,100,100,100,100,100,100\r\n"
        "OL,28,@@@AB\r\nOL,1,\x1bW\x7f\x7f\r\nOL,26,@iAKA\r\nOL,24,bottom\r\n"
    )
    screen = Screen()
    screen.load_tti(picture, last_row=23)
    assert screen.rows[1][:3] == bytes([0x17, 0x7F, 0x7F])
    assert screen.row_text(24).strip() == "", "rows past the last are not taken"
    assert screen.packets == ["OL,28,@@@AB", "OL,26,@iAKA"]
    written = screen.tti(0x300)
    assert written.endswith("OL,28,@@@AB\nOL,26,@iAKA\n")
    assert "FL," not in written, "the other page's links are not taken"
