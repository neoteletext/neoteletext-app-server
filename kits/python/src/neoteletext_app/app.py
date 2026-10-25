"""The protocol loop (docs/applications.md).

A channel's application reads the server's lines on standard input and
answers on standard output.

A subclass of `App` overrides the handlers it needs and calls `publish`,
`goto`, the sound methods and `log`. Page selection is the kit's default
(INPUT.md section 9.4): on a page whose `on_input` does not take a key,
three digits go to that page and the coloured keys follow the page's own
Fastext links.
"""

from __future__ import annotations

import asyncio
import base64
import json
import sys
from array import array
from dataclasses import dataclass
from typing import Any, Literal

from neoteletext_app.screen import FIRST_PAGE, LAST_PAGE, Screen, page_number, page_text, tti

#: The longest line read from the server (docs/applications.md section 1).
LONGEST_LINE = 16 * 1024 * 1024
#: The protocol this kit speaks.
PROTOCOL = 1
#: Digits that select a page.
PAGE_DIGITS = 3
#: The coloured keys, red to cyan, by the names INPUT.md gives them (section
#: 5.6): the function keys and a remote's colour keys.
COLOUR_KEYS: tuple[tuple[str, ...], ...] = (("F1", "ColorF0Red"), ("F2", "ColorF1Green"), ("F3", "ColorF2Yellow"), ("F4", "ColorF3Blue"))

Owner = Literal["channel"] | int


@dataclass(frozen=True)
class Hearer:
    """Who hears a sound: one session, or everyone with a page open."""

    session: int | None = None
    page: int | None = None

    def wire(self) -> dict[str, Any]:
        """Answer the `to` member (docs/applications.md section 3.4)."""
        if self.page is not None:
            return {"page": page_text(self.page)}
        return {"session": self.session}


def session_hears(session: int) -> Hearer:
    """Answer one session as a sound's hearer."""
    return Hearer(session=session)


def page_hears(page: int) -> Hearer:
    """Answer everyone with `page` open as a sound's hearers."""
    return Hearer(page=page)


@dataclass(frozen=True)
class Event:
    """One input event on a page (docs/applications.md section 2)."""

    kind: str
    text: str = ""
    action: str = ""
    key: str = ""
    base: str = ""
    pressed: bool = False
    button: int = 0
    column: int = 0
    row: int = 0
    dx: int = 0
    dy: int = 0

    @classmethod
    def of(cls, value: dict[str, Any]) -> Event:
        """Read an event from the server's line, ignoring what it does not know."""

        def number(name: str) -> int:
            item = value.get(name, 0)
            return item if isinstance(item, int) else 0

        def text(name: str) -> str:
            item = value.get(name, "")
            return item if isinstance(item, str) else ""

        return cls(
            kind=text("kind"),
            text=text("text"),
            action=text("action"),
            key=text("key"),
            base=text("base"),
            pressed=value.get("pressed") is True,
            button=number("button"),
            column=number("column"),
            row=number("row"),
            dx=number("dx"),
            dy=number("dy"),
        )

    def pressed_key(self, *names: str) -> bool:
        """Answer whether this is a press or repeat of a key of these names."""
        return self.kind == "key" and self.action in {"press", "repeat"} and self.key in names

    @property
    def character(self) -> str | None:
        """The one character this event typed, if it typed one."""
        if self.kind == "text" and len(self.text) == 1:
            return self.text
        if self.kind == "key" and self.action in {"press", "repeat"} and len(self.key) == 1:
            return self.text or self.key
        return None

    @property
    def colour(self) -> int | None:
        """The coloured key pressed, 0 (red) to 3 (cyan)."""
        for place, names in enumerate(COLOUR_KEYS):
            if self.pressed_key(*names):
                return place
        return None


class App:
    """A channel's application: one instance serves every viewer."""

    def __init__(self, pages: dict[int, str]) -> None:
        """Serve `pages`, page numbers and their titles."""
        self.pages = pages
        self.epoch_ms = 0
        self._digits: dict[int, str] = {}
        self._links: dict[tuple[Owner, int], list[int | None]] = {}
        self._owned: set[tuple[int, int]] = set()
        self._open: dict[int, int] = {}

    # Handlers, for a subclass to override.

    async def on_start(self) -> None:
        """Publish the channel's pages."""

    async def on_join(self, session: int) -> None:
        """Greet a viewer who connected."""

    async def on_open(self, session: int, page: int) -> None:
        """Answer the viewer opening `page`."""

    async def on_leave(self, session: int, page: int) -> None:
        """Answer the viewer leaving `page`."""

    async def on_input(self, session: int, page: int, event: Event) -> bool:  # noqa: ARG002
        """Answer whether the page took `event`; if not, the kit selects pages with it."""
        return False

    async def on_close(self, session: int) -> None:
        """Forget a viewer who went."""

    async def on_stats(self, stats: dict[str, Any]) -> None:
        """Take the server's figures, sent every 10 seconds (docs/applications.md section 7)."""

    # What an application does.

    def publish(self, page: int, screens: Screen | list[Screen], owner: Owner = "channel") -> None:
        """Publish `page`: the channel's copy, or one session's own."""
        if page not in self.pages:
            message = f"{page_text(page)} was not declared"
            raise ValueError(message)
        screens = screens if isinstance(screens, list) else [screens]
        if screens:
            self._links[(owner, page)] = [*screens[0].fastext_links, screens[0].index]
        if owner != "channel":
            self._owned.add((owner, page))
        self.send({"type": "publish", "page": page_text(page), "owner": owner, "tti": tti(page, screens)})

    def goto(self, session: int, page: int) -> None:
        """Send the viewer to `page`."""
        self.send({"type": "goto", "session": session, "page": page_text(page)})

    def log(self, text: str) -> None:
        """Write a line to the server's log."""
        self.send({"type": "log", "text": text})

    def sound_object(self, to: Hearer, handle: int, rate: int, samples: array[int], generation: int = 0) -> None:
        """Send a stored sound, 16-bit mono samples at `rate`, to a session or a page's viewers."""
        sound = {"handle": handle, "generation": generation, "rate": rate, "pcm": pcm(samples)}
        self.send({"type": "sound", "to": to.wire(), "object": sound})

    def play(  # noqa: PLR0913 - the fields of one play command (audio.md section 4.4)
        self, to: Hearer, channel: int, handle: int, *, gain: int = 100, page: int | None = None, generation: int = 0, loop: bool = False
    ) -> None:
        """Play a stored sound on one of the profile's four channels."""
        play: dict[str, Any] = {"channel": channel, "handle": handle, "generation": generation, "gain": gain, "loop": loop}
        if page is not None:
            play["page"] = page_text(page)
        self.send({"type": "sound", "to": to.wire(), "play": play})

    def stop(self, to: Hearer, channel: int, fade_ms: int = 20) -> None:
        """Stop a channel's sound."""
        self.send({"type": "sound", "to": to.wire(), "stop": {"channel": channel, "fade_ms": fade_ms}})

    def page_of(self, session: int) -> int | None:
        """Answer the page `session` has open, if it is one of ours."""
        return self._open.get(session)

    def send(self, message: dict[str, Any]) -> None:
        """Write one line to the server."""
        sys.stdout.write(json.dumps(message, ensure_ascii=False, separators=(",", ":")) + "\n")
        sys.stdout.flush()

    # The loop.

    def run(self) -> None:
        """Declare the pages and serve until the server closes standard input."""
        asyncio.run(self.serve())

    async def serve(self) -> None:
        """Declare the pages, then answer each line from the server."""
        self.send({"type": "pages", "pages": [{"page": page_text(number), "title": title} for number, title in sorted(self.pages.items())]})
        loop = asyncio.get_running_loop()
        reader = asyncio.StreamReader(limit=LONGEST_LINE)
        await loop.connect_read_pipe(lambda: asyncio.StreamReaderProtocol(reader), sys.stdin)
        while line := await reader.readline():
            try:
                await self.receive(json.loads(line))
            except (ValueError, KeyError, TypeError) as error:
                print(f"neoteletext_app: refused a line from the server: {error}", file=sys.stderr)  # noqa: T201

    async def receive(self, message: dict[str, Any]) -> None:
        """Act on one line from the server."""
        kind = message.get("type")
        session = message.get("session")
        if kind == "start":
            if message.get("protocol") != PROTOCOL:
                print(f"neoteletext_app: protocol {message.get('protocol')}, this kit speaks {PROTOCOL}", file=sys.stderr)  # noqa: T201
            self.epoch_ms = int(message.get("epoch_ms", 0))
            await self.on_start()
        elif kind == "stats":
            await self.on_stats(message)
        elif isinstance(session, int):
            await self.receive_for(session, kind, message)

    async def receive_for(self, session: int, kind: object, message: dict[str, Any]) -> None:
        """Act on a line about one session."""
        if kind == "join":
            await self.on_join(session)
        elif kind == "open":
            page = page_number(message["page"])
            self._open[session] = page
            self._digits.pop(session, None)
            await self.on_open(session, page)
        elif kind == "leave":
            page = page_number(message["page"])
            if self._open.get(session) == page:
                del self._open[session]
            await self.on_leave(session, page)
        elif kind == "input":
            page = page_number(message["page"])
            event = Event.of(message.get("event") or {})
            if not await self.on_input(session, page, event):
                self.navigate(session, page, event)
        elif kind == "close":
            self._digits.pop(session, None)
            self._open.pop(session, None)
            self._owned = {owned for owned in self._owned if owned[0] != session}
            await self.on_close(session)

    def navigate(self, session: int, page: int, event: Event) -> None:
        """Select pages with digits and the coloured keys (INPUT.md section 9.4)."""
        colour = event.colour
        if colour is not None:
            owner: Owner = session if (session, page) in self._owned else "channel"
            links = self._links.get((owner, page)) or self._links.get(("channel", page)) or []
            target = links[colour] if colour < len(links) else None
            if target is not None:
                self.goto(session, target)
            return
        character = event.character
        if character is None or not character.isdigit():
            return
        digits = self._digits.get(session, "") + character
        if len(digits) < PAGE_DIGITS:
            self._digits[session] = digits
            return
        self._digits.pop(session, None)
        number = int(digits, 16)
        if FIRST_PAGE <= number <= LAST_PAGE:
            self.goto(session, number)


def pcm(samples: array[int]) -> str:
    """Answer 16-bit samples as base64 of little-endian bytes, whatever this machine's order."""
    data = array("h", samples)
    if sys.byteorder == "big":
        data.byteswap()
    return base64.b64encode(data.tobytes()).decode("ascii")
