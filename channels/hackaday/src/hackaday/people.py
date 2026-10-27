"""The people of Hackaday: a few of its writers and editors, each with a page.

`people/people.json` names them, with each one's author number on
hackaday.com, a title, a line from their own author biography, the
podcast clip in which they say their name, their hackaday.io profile (its
address, tagline, place, joining date and projects), and their portrait: their own
avatar from the site, made once into a Level 2.5 page by the engine's
converter (rows 1 to 16) and kept out of git with the clips. Someone with
no avatar is shown the Jolly Wrencher instead.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


@dataclass
class Person:
    """One writer or editor, and what the site says of them now."""

    slug: str
    name: str
    author: int
    title: str
    podcast: str
    bio: str
    clip: str
    portrait: str | None
    posts: int = 0
    latest: list[str] = field(default_factory=list)
    io_url: str = ""
    io_tagline: str = ""
    io_location: str = ""
    io_joined: str = ""
    io_projects: list[str] = field(default_factory=list)


#: The most hackaday.io projects kept for a person.
MOST_PROJECTS = 8


def load(folder: Path) -> list[Person]:
    """Answer the people `folder`'s manifest names, each with their portrait read; skip any the manifest cannot describe."""
    manifest = folder / "people.json"
    if not manifest.exists():
        return []
    people: list[Person] = []
    for entry in json.loads(manifest.read_text()):
        try:
            portrait = (folder / entry["portrait"]).read_text(encoding="latin-1") if entry.get("portrait") else None
            people.append(
                Person(
                    slug=str(entry["slug"]),
                    name=str(entry["name"]),
                    author=int(entry["author"]),
                    title=str(entry["title"]),
                    podcast=str(entry.get("podcast", "")),
                    bio=str(entry.get("bio", "")),
                    clip=str(entry.get("clip", "")),
                    portrait=portrait,
                    io_url=str((entry.get("io") or {}).get("url", "")),
                    io_tagline=str((entry.get("io") or {}).get("tagline", "")),
                    io_location=str((entry.get("io") or {}).get("location", "")),
                    io_joined=str((entry.get("io") or {}).get("joined", "")),
                    io_projects=[str(project) for project in (entry.get("io") or {}).get("projects", [])][:MOST_PROJECTS],
                )
            )
        except (KeyError, ValueError, OSError) as error:
            print(f"hackaday: people.json: {error}", file=sys.stderr)  # noqa: T201
    return people
