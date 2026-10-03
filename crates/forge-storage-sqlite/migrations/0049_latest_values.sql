CREATE TABLE IF NOT EXISTS latest_values (
    slot        TEXT    NOT NULL,
    platform    TEXT    NOT NULL,
    payload     TEXT    NOT NULL,
    occurred_at INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (slot, platform)
) WITHOUT ROWID;
