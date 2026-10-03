CREATE TABLE IF NOT EXISTS media_blobs (
    id          TEXT PRIMARY KEY NOT NULL,
    format      TEXT NOT NULL,
    byte_size   INTEGER NOT NULL,
    label       TEXT NOT NULL,
    imported_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS media_references (
    referrer_kind TEXT NOT NULL,
    referrer_id   TEXT NOT NULL,
    slot          TEXT NOT NULL,
    blob_id       TEXT NOT NULL REFERENCES media_blobs(id) ON DELETE RESTRICT,
    PRIMARY KEY (referrer_kind, referrer_id, slot)
);

CREATE INDEX IF NOT EXISTS idx_media_references_blob ON media_references(blob_id);
