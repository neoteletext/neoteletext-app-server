-- Hackaday's whole public REST API, kept: each object whole, as the API
-- gave it, in `data`; the columns looked up by are generated from that
-- JSON, so the JSON is the one source of truth and SQLite checks each
-- column's type (STRICT). Times are UTC in WordPress's own form,
-- `2026-10-25T17:00:31`, so they compare with the API's as text.

-- Posts and pages (`/wp/v2/posts`, `/wp/v2/pages`: one id space).
-- `title` and `fts` text are made in Python: the API's are HTML.
CREATE TABLE posts (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL CHECK (json_valid(data)),
    title TEXT NOT NULL,
    type TEXT GENERATED ALWAYS AS (json_extract(data, '$.type')) STORED NOT NULL CHECK (type IN ('post', 'page')),
    date_gmt TEXT GENERATED ALWAYS AS (json_extract(data, '$.date_gmt')) STORED NOT NULL,
    modified_gmt TEXT GENERATED ALWAYS AS (json_extract(data, '$.modified_gmt')) STORED NOT NULL,
    author INTEGER GENERATED ALWAYS AS (json_extract(data, '$.author')) STORED NOT NULL,
    featured_media INTEGER GENERATED ALWAYS AS (json_extract(data, '$.featured_media')) VIRTUAL NOT NULL,
    status TEXT GENERATED ALWAYS AS (json_extract(data, '$.status')) VIRTUAL NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    -- When a sweep found it no longer published; it is kept.
    gone_at TEXT,
    CHECK (id = json_extract(data, '$.id'))
) STRICT;
CREATE INDEX posts_date ON posts (type, date_gmt);
CREATE INDEX posts_modified ON posts (modified_gmt);
CREATE INDEX posts_author ON posts (author, date_gmt);

-- Categories and tags (`/wp/v2/categories`, `/wp/v2/tags`: one id space).
CREATE TABLE terms (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL CHECK (json_valid(data)),
    taxonomy TEXT GENERATED ALWAYS AS (json_extract(data, '$.taxonomy')) STORED NOT NULL CHECK (taxonomy IN ('category', 'post_tag')),
    name TEXT GENERATED ALWAYS AS (json_extract(data, '$.name')) STORED NOT NULL,
    slug TEXT GENERATED ALWAYS AS (json_extract(data, '$.slug')) STORED NOT NULL,
    count INTEGER GENERATED ALWAYS AS (json_extract(data, '$.count')) VIRTUAL NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    CHECK (id = json_extract(data, '$.id'))
) STRICT;
CREATE UNIQUE INDEX terms_slug ON terms (taxonomy, slug);

-- Which terms each post has, from its `categories` and `tags` arrays. No
-- foreign key to `terms`: a post can arrive before a term it names.
CREATE TABLE post_terms (
    post INTEGER NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    term INTEGER NOT NULL,
    PRIMARY KEY (post, term)
) STRICT, WITHOUT ROWID;
CREATE INDEX post_terms_term ON post_terms (term, post);

-- The writers (`/wp/v2/users`: those with published posts).
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL CHECK (json_valid(data)),
    name TEXT GENERATED ALWAYS AS (json_extract(data, '$.name')) STORED NOT NULL,
    slug TEXT GENERATED ALWAYS AS (json_extract(data, '$.slug')) STORED NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    CHECK (id = json_extract(data, '$.id'))
) STRICT;

-- The media library's records (`/wp/v2/media`): sizes, captions, credits.
-- The files themselves are fetched only for the pictures the channel shows.
CREATE TABLE media (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL CHECK (json_valid(data)),
    date_gmt TEXT GENERATED ALWAYS AS (json_extract(data, '$.date_gmt')) STORED NOT NULL,
    modified_gmt TEXT GENERATED ALWAYS AS (json_extract(data, '$.modified_gmt')) STORED NOT NULL,
    post INTEGER GENERATED ALWAYS AS (json_extract(data, '$.post')) VIRTUAL,
    source_url TEXT GENERATED ALWAYS AS (json_extract(data, '$.source_url')) VIRTUAL NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    gone_at TEXT,
    CHECK (id = json_extract(data, '$.id'))
) STRICT;
CREATE INDEX media_modified ON media (modified_gmt);

-- Every comment (`/wp/v2/comments`). No foreign key to `posts`: comments
-- arrive before their posts while the archive is read, and some answer pages.
CREATE TABLE comments (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL CHECK (json_valid(data)),
    post INTEGER GENERATED ALWAYS AS (json_extract(data, '$.post')) STORED NOT NULL,
    parent INTEGER GENERATED ALWAYS AS (json_extract(data, '$.parent')) VIRTUAL NOT NULL,
    date_gmt TEXT GENERATED ALWAYS AS (json_extract(data, '$.date_gmt')) STORED NOT NULL,
    status TEXT GENERATED ALWAYS AS (json_extract(data, '$.status')) VIRTUAL NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    -- When a re-read of its post no longer found it; it is kept.
    gone_at TEXT,
    CHECK (id = json_extract(data, '$.id'))
) STRICT;
CREATE INDEX comments_post ON comments (post, date_gmt);
CREATE INDEX comments_date ON comments (date_gmt);

-- Picture files the channel has fetched, with their validators, and the
-- teletext pages made from them; keyed by address, so a post whose
-- picture changes gets its new one.
CREATE TABLE pictures (
    url TEXT PRIMARY KEY,
    headers TEXT NOT NULL CHECK (json_valid(headers)),
    body BLOB NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now'))
) STRICT;
CREATE TABLE picture_pages (
    url TEXT PRIMARY KEY REFERENCES pictures (url) ON DELETE CASCADE,
    level TEXT NOT NULL,
    tti TEXT NOT NULL,
    made_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now'))
) STRICT;

-- Where each reading stands: the full download's place in each
-- collection, the latest times seen, when each poll last ran.
CREATE TABLE sync_state (
    name TEXT PRIMARY KEY,
    value TEXT NOT NULL CHECK (json_valid(value)),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now'))
) STRICT;

-- Every request made to hackaday.com, with the answer's headers.
CREATE TABLE fetches (
    id INTEGER PRIMARY KEY,
    at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    kind TEXT NOT NULL,
    url TEXT NOT NULL,
    status INTEGER NOT NULL,
    wire_bytes INTEGER NOT NULL,
    body_bytes INTEGER NOT NULL,
    elapsed_ms INTEGER NOT NULL,
    headers TEXT NOT NULL CHECK (json_valid(headers))
) STRICT;
CREATE INDEX fetches_at ON fetches (at);

-- Search over every post's title and text. A virtual table cannot be
-- STRICT; it holds only the index (contentless), its rowid the post's id.
CREATE VIRTUAL TABLE posts_search USING fts5 (
    title, body, content = '', contentless_delete = 1, tokenize = 'porter unicode61 remove_diacritics 2'
);
