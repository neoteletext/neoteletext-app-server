"""Sound grabs from the Hackaday Podcast, played on the channel.

The clips are cut from published episodes and kept out of git, as the
showcase's pictures are (decision 6): `sounds/clips.json` names them and
`sounds/neoteletext/*.wav` holds each as 16-bit mono samples at a rate
of the engine's audio profile, which the server encodes as IMA ADPCM.
The originals, at the episode's own quality, sit beside them in
`sounds/original/` and are not sent.
"""

from __future__ import annotations

import json
import sys
import wave
from array import array
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

#: The audio profile's sample rates (the engine's `audio.md` section 3.2).
PROFILE_RATES = frozenset({8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000})
#: The longest clip sent: the server refuses a sound over two minutes.
LONGEST_S = 120
#: 16-bit samples.
SAMPLE_BYTES = 2
#: The first sound handle the clips take; the channel's effects have the ones below.
FIRST_HANDLE = 16


@dataclass(frozen=True)
class Clip:
    """One sound grab."""

    name: str
    title: str
    episode: str
    rate: int
    samples: array[int]
    handle: int

    @property
    def seconds(self) -> float:
        """Answer how long the clip is."""
        return len(self.samples) / self.rate


def load(folder: Path) -> list[Clip]:
    """Answer the clips `folder`'s manifest names, skipping any that cannot be played, with the reason on standard error."""
    manifest = folder / "clips.json"
    if not manifest.exists():
        return []
    clips: list[Clip] = []
    for entry in json.loads(manifest.read_text()):
        path = folder / "neoteletext" / f"{entry['name']}.wav"
        try:
            with wave.open(str(path), "rb") as sound:
                rate, channels, width = sound.getframerate(), sound.getnchannels(), sound.getsampwidth()
                if rate not in PROFILE_RATES or channels != 1 or width != SAMPLE_BYTES:
                    print(f"hackaday: {path.name}: not 16-bit mono at a profile rate", file=sys.stderr)  # noqa: T201
                    continue
                if sound.getnframes() > rate * LONGEST_S:
                    print(f"hackaday: {path.name}: over {LONGEST_S} s", file=sys.stderr)  # noqa: T201
                    continue
                samples = array("h", sound.readframes(sound.getnframes()))
        except (OSError, wave.Error) as error:
            print(f"hackaday: {path.name}: {error}", file=sys.stderr)  # noqa: T201
            continue
        if sys.byteorder == "big":
            samples.byteswap()
        clips.append(Clip(entry["name"], entry["title"], entry.get("episode", ""), rate, samples, FIRST_HANDLE + len(clips)))
    return clips
