CREATE TABLE IF NOT EXISTS ban_ledger (
    platform        TEXT    NOT NULL,
    channel_id      TEXT    NOT NULL,
    viewer_id       TEXT    NOT NULL,
    viewer_name     TEXT    NOT NULL,
    reason          TEXT,
    moderator       TEXT,
    banned_at       INTEGER NOT NULL,
    expires_at      INTEGER,
    platform_ban_id TEXT,
    origin          TEXT    NOT NULL CHECK (origin IN ('forge', 'observed')),
    PRIMARY KEY (platform, channel_id, viewer_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_ban_ledger_channel_banned_at
    ON ban_ledger(platform, channel_id, banned_at DESC);

CREATE INDEX IF NOT EXISTS idx_ban_ledger_expires_at
    ON ban_ledger(expires_at) WHERE expires_at IS NOT NULL;
