"""A post's HTML as teletext can show it: blocks of plain text, its links and its videos.

Teletext has no markup: a paragraph is words, a heading a coloured line,
a list item a line with a marker. Links and videos are gathered for the
story's links page, where each becomes a QR code a phone can follow.
Scripts, styles and embedded media leave no text.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from html.parser import HTMLParser

#: The most blocks kept from one post: Hackaday's longest column this
#: fortnight was 4,576 words, about 200 paragraphs.
MOST_BLOCKS = 400
#: The most links kept from one post (the most this fortnight was 51).
MOST_LINKS = 60

#: Elements whose text is not shown.
SILENT = frozenset({"script", "style", "noscript", "svg", "iframe", "video", "audio", "figure", "figcaption", "form", "button"})
#: Elements that start a block, and the kind of block.
BLOCKS = {
    "p": "text",
    "h1": "heading",
    "h2": "heading",
    "h3": "heading",
    "h4": "heading",
    "li": "item",
    "blockquote": "quote",
    "pre": "code",
}
#: Hosts of videos a post embeds.
VIDEO_HOSTS = ("youtube.com", "youtu.be", "vimeo.com")


@dataclass(frozen=True)
class Block:
    """A paragraph, heading, list item, quote or code: one run of text."""

    kind: str
    text: str


@dataclass
class Article:
    """What a post says, and where it points."""

    blocks: list[Block] = field(default_factory=list)
    links: list[tuple[str, str]] = field(default_factory=list)
    videos: list[str] = field(default_factory=list)

    @property
    def words(self) -> int:
        """Answer how many words the post has."""
        return sum(len(block.text.split()) for block in self.blocks)


class _Reader(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.article = Article()
        self.kind = "text"
        self.words: list[str] = []
        self.silent = 0
        self.link: str | None = None
        self.link_text: list[str] = []

    def end_block(self) -> None:
        text = " ".join(" ".join(self.words).split())
        if text and len(self.article.blocks) < MOST_BLOCKS:
            self.article.blocks.append(Block(self.kind, text))
        self.words = []
        self.kind = "text"

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attributes = dict(attrs)
        if tag in {"iframe", "video"}:
            source = attributes.get("src") or ""
            if any(host in source for host in VIDEO_HOSTS) and source not in self.article.videos:
                self.article.videos.append(source)
        if tag in SILENT:
            self.silent += 1
            return
        if tag in BLOCKS:
            self.end_block()
            self.kind = BLOCKS[tag]
        elif tag == "br":
            self.words.append(" ")
        elif tag == "a":
            href = attributes.get("href") or ""
            if href.startswith(("https://", "http://")):
                self.link = href
                self.link_text = []

    def handle_endtag(self, tag: str) -> None:
        if tag in SILENT:
            self.silent = max(0, self.silent - 1)
            return
        if tag in BLOCKS:
            self.end_block()
        elif tag == "a" and self.link is not None:
            text = " ".join(" ".join(self.link_text).split())
            if any(host in self.link for host in VIDEO_HOSTS):
                if self.link not in self.article.videos:
                    self.article.videos.append(self.link)
            elif text and len(self.article.links) < MOST_LINKS and all(url != self.link for _, url in self.article.links):
                self.article.links.append((text, self.link))
            self.link = None

    def handle_data(self, data: str) -> None:
        if self.silent:
            return
        self.words.append(data)
        if self.link is not None:
            self.link_text.append(data)


def read(html: str) -> Article:
    """Answer the article an HTML post holds."""
    reader = _Reader()
    reader.feed(html)
    reader.close()
    reader.end_block()
    return reader.article


def plain(html: str) -> str:
    """Answer HTML's text in one line: for titles and excerpts."""
    return " ".join(block.text for block in read(f"<p>{html}</p>").blocks)
