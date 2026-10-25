"""The engine's own tools, run for the channel: shared, not rewritten.

- Pictures: the engine's `scripts/image-to-teletext.py` turns a post's lead
  picture into a Level 2.5 page of mosaics, its colours in the page's own
  X/26 and X/28 (`--no-objects`), about 14 s each (measured 25 October
  2026). It is run once a picture, its file and its page kept in the store.
- QR codes: `neottx qr` writes a link as a QR code in a page's mosaics.

`HACKADAY_CONVERTER` names the converter script, `NEOTTX` the program,
`UV` the uv that runs the script; the defaults are the development
machine's.
"""

from __future__ import annotations

import asyncio
import os
import tempfile
from pathlib import Path

#: The converter, the engine's.
CONVERTER = Path(os.environ.get("HACKADAY_CONVERTER", str(Path.home() / "devel/neoteletext-engine/scripts/image-to-teletext.py")))
#: The engine's command line program.
NEOTTX = os.environ.get("NEOTTX", str(Path.home() / "devel/neoteletext-engine/target/release/neottx"))
#: uv, which runs the converter with its own pinned libraries.
UV = os.environ.get("UV", "uv")
#: The level pictures are made for: 2.5's 31 colours in the page itself,
#: which the browser shows (its decision 60).
PICTURE_LEVEL = "2.5"
#: How long a conversion may take, in seconds.
CONVERT_WITHIN_S = 180
#: How long a QR code may take, in seconds.
QR_WITHIN_S = 20
#: The page number the tools are told: any will do, the channel places the result.
SCRATCH_PAGE = "100"


class ToolError(Exception):
    """A tool that failed."""


async def _run(*command: str, within: float) -> None:
    process = await asyncio.create_subprocess_exec(*command, stdout=asyncio.subprocess.DEVNULL, stderr=asyncio.subprocess.PIPE)
    try:
        _, error = await asyncio.wait_for(process.communicate(), within)
    except TimeoutError as timeout:
        process.kill()
        await process.wait()
        message = f"{command[0]} took over {within} s"
        raise ToolError(message) from timeout
    if process.returncode != 0:
        message = f"{Path(command[0]).name} failed: {error.decode(errors='replace')[-300:]}"
        raise ToolError(message)


async def picture_tti(picture: bytes, suffix: str) -> str:
    """Answer the TTI page the engine's converter makes of a picture."""
    with tempfile.TemporaryDirectory(prefix="hackaday-picture-") as folder:
        source = Path(folder) / f"picture{suffix}"
        target = Path(folder) / "picture.tti"
        source.write_bytes(picture)
        await _run(
            UV,
            "run",
            "--quiet",
            str(CONVERTER),
            str(source),
            str(target),
            "--level",
            PICTURE_LEVEL,
            "--no-objects",
            within=CONVERT_WITHIN_S,
        )
        return target.read_text(encoding="latin-1")


async def qr_tti(text: str, row: int, column: int) -> str:
    """Answer a TTI page holding `text` as a QR code from `row`, `column`."""
    with tempfile.TemporaryDirectory(prefix="hackaday-qr-") as folder:
        target = Path(folder) / "qr.tti"
        await _run(NEOTTX, "qr", "--page", SCRATCH_PAGE, "--row", str(row), "--column", str(column), text, str(target), within=QR_WITHIN_S)
        return target.read_text(encoding="latin-1")
