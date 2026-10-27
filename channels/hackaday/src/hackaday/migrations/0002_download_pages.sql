-- The full download's pages, each kept when read, so that a download
-- read by many requests at once goes on where it stopped. `month` is
-- the month a dated collection is read by (`2005-02-01T00:00:00Z`), or ''
-- for one read whole.
CREATE TABLE download_pages (
    collection TEXT NOT NULL CHECK (collection IN ('users', 'categories', 'tags', 'posts', 'pages', 'media', 'comments')),
    month TEXT NOT NULL,
    page INTEGER NOT NULL CHECK (page >= 1),
    total_pages INTEGER NOT NULL CHECK (total_pages >= 0),
    items INTEGER NOT NULL CHECK (items >= 0),
    read_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    PRIMARY KEY (collection, month, page)
) STRICT, WITHOUT ROWID;
