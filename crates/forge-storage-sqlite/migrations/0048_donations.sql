CREATE TABLE IF NOT EXISTS donations (
    seq           INTEGER PRIMARY KEY AUTOINCREMENT,
    provider      TEXT    NOT NULL,
    donation_id   TEXT    NOT NULL,
    amount_micros INTEGER NOT NULL CHECK (amount_micros >= 0),
    currency      TEXT    NOT NULL,
    donor_kind    TEXT    NOT NULL CHECK (donor_kind IN ('named', 'anonymous', 'hidden')),
    donor_name    TEXT,
    message       TEXT,
    occurred_at   INTEGER NOT NULL,
    received_at   INTEGER NOT NULL,
    origin        TEXT    NOT NULL CHECK (origin IN ('live', 'history')),
    announced_at  INTEGER,
    UNIQUE (provider, donation_id),
    CHECK ((donor_kind = 'named') = (donor_name IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS idx_donations_occurred_at ON donations (occurred_at DESC, seq DESC);
CREATE INDEX IF NOT EXISTS idx_donations_unannounced ON donations (occurred_at) WHERE announced_at IS NULL;
