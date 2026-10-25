"""Read the whole of Hackaday's public archive into the store, without the channel: `python -m hackaday.download [DATABASE]`.

The channel does the same in the background; this is for a copy made
elsewhere, such as on a development machine. It goes on where it stopped.
"""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path

from hackaday.fetch import Fetcher
from hackaday.library import DATABASE
from hackaday.store import Store
from hackaday.sync import Sync, run_until_done

#: The store's file when none is named: the channel's own.
DEFAULT = Path(__file__).resolve().parents[2] / "cache" / DATABASE


def main() -> None:
    """Read until the full download is done."""
    database = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT
    database.parent.mkdir(parents=True, exist_ok=True)
    store = Store(database)
    fetcher = Fetcher(store.log_fetch)
    asyncio.run(run_until_done(Sync(store, fetcher)))


if __name__ == "__main__":
    main()
