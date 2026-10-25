"""Short sounds made in the application, as 16-bit samples for `App.sound_object`.

Tones and noise with a linear envelope; the engine's audio profile carries
them as IMA ADPCM, and the server encodes them.
"""

from __future__ import annotations

import math
import random
from array import array

#: The rate effects are made at: the profile's lowest (the engine's
#: `audio.md` section 3.2), enough for chimes and blips.
RATE = 8_000
#: The loudest sample: a quarter of full scale, so mixed sounds do not clip.
PEAK = 8_000
#: The fade at each end of a note, against clicks (`audio.md` section 10.2:
#: every change ramps over at least 5 ms).
FADE_MS = 5
#: Milliseconds in a second.
MILLISECONDS = 1_000
#: A semitone's frequency ratio, equal temperament.
SEMITONE = 2 ** (1 / 12)
#: Concert A, the reference pitch (ISO 16).
A4_HZ = 440.0
#: A4's MIDI note number.
A4_NOTE = 69


def note_hz(note: int) -> float:
    """Answer a MIDI note's frequency."""
    return A4_HZ * SEMITONE ** (note - A4_NOTE)


def tone(hz: float, milliseconds: int, *, wave: str = "sine", gain: float = 1.0) -> array[int]:
    """Answer a tone of `hz` for `milliseconds`, faded in and out."""
    count = RATE * milliseconds // MILLISECONDS
    fade = RATE * FADE_MS // MILLISECONDS
    samples = array("h")
    for index in range(count):
        phase = (index * hz / RATE) % 1.0
        if wave == "square":
            value = 1.0 if phase < 0.5 else -1.0  # noqa: PLR2004 - the half cycle
        elif wave == "triangle":
            value = 4 * abs(phase - 0.5) - 1
        else:
            value = math.sin(2 * math.pi * phase)
        envelope = min(1.0, index / fade if fade else 1.0, (count - index) / fade if fade else 1.0)
        samples.append(int(value * envelope * gain * PEAK))
    return samples


def noise(milliseconds: int, *, seed: int = 1, gain: float = 1.0) -> array[int]:
    """Answer white noise for `milliseconds`, fading out to silence."""
    generator = random.Random(seed)  # noqa: S311 - a sound, not a secret
    count = RATE * milliseconds // MILLISECONDS
    return array("h", (int(generator.uniform(-1, 1) * (1 - index / count) * gain * PEAK) for index in range(count)))


def sequence(*parts: array[int]) -> array[int]:
    """Answer sounds one after another."""
    joined = array("h")
    for part in parts:
        joined.extend(part)
    return joined


def chime(notes: list[int], milliseconds: int = 120) -> array[int]:
    """Answer a run of notes, each `milliseconds` long."""
    return sequence(*(tone(note_hz(note), milliseconds, wave="triangle") for note in notes))
