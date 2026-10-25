"""Text to teletext character codes: the English national option of the G0 set.

The server's pages use the English set (EN 300 706 Table 36). Its national
positions show other characters than ASCII's: 0x23 is a pound sign and
0x5F a hash, so text cannot be copied byte for byte. Everything else is
folded to the nearest character the set has, so a page never shows a
character the viewer did not mean.
"""

from __future__ import annotations

import unicodedata

#: The first and last displayable codes of a row (EN 300 706 clause 12.2:
#: 0x00 to 0x1F are spacing attributes).
FIRST_CHARACTER = 0x20
LAST_CHARACTER = 0x7F
#: What a character the set cannot show becomes.
UNSHOWABLE = ord("?")

#: The English set's national positions (EN 300 706 Table 36), by the
#: character each shows.
NATIONAL: dict[str, int] = {
    "£": 0x23,
    "←": 0x5B,
    "½": 0x5C,
    "→": 0x5D,
    "↑": 0x5E,
    "#": 0x5F,
    "—": 0x60,
    "¼": 0x7B,
    "‖": 0x7C,
    "¾": 0x7D,
    "÷": 0x7E,
}

#: ASCII characters whose code shows something else in the English set,
#: and what they are written as instead.
DISPLACED: dict[str, str] = {
    "[": "(",
    "]": ")",
    "\\": "/",
    "^": "'",
    "_": "-",
    "`": "'",
    "{": "(",
    "|": "!",
    "}": ")",
    "~": "-",
}

#: Typographic characters and symbols the web uses, and their nearest.
TYPOGRAPHIC: dict[str, str] = {
    "‘": "'",
    "’": "'",
    "‚": "'",
    "“": '"',
    "”": '"',
    "„": '"',
    "–": "-",
    "‑": "-",
    "−": "-",
    "…": "...",
    "•": "*",
    "·": ".",
    "×": "x",
    "°": " deg",
    "µ": "u",
    "Ω": " ohm",
    "€": "EUR",
    "©": "(c)",
    "®": "(R)",
    "™": "TM",
    "\u00a0": " ",
    "\u2009": " ",
    "\u200b": "",
}


def fold(text: str) -> str:
    """Answer `text` in characters the English set shows as themselves."""
    folded: list[str] = []
    for character in text:
        if character in NATIONAL:
            folded.append(character)
        elif character in DISPLACED:
            folded.append(DISPLACED[character])
        elif character in TYPOGRAPHIC:
            folded.append(TYPOGRAPHIC[character])
        elif FIRST_CHARACTER <= ord(character) < LAST_CHARACTER:
            folded.append(character)
        elif character in "\t\n\r":
            folded.append(" ")
        else:
            # Accented letters lose their accents (NFKD splits them off);
            # what is left that ASCII lacks shows as a question mark.
            plain = "".join(part for part in unicodedata.normalize("NFKD", character) if FIRST_CHARACTER <= ord(part) < LAST_CHARACTER)
            folded.append(plain or chr(UNSHOWABLE))
    return "".join(folded)


def codes(text: str) -> bytes:
    """Answer the character codes of `text` in the English set."""
    out = bytearray()
    for character in fold(text):
        out.append(NATIONAL.get(character, ord(character)))
    return bytes(out)
