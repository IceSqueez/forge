CREATE TABLE overlays (
    id                    TEXT PRIMARY KEY NOT NULL,
    display_name          TEXT NOT NULL,
    kind_id               TEXT NOT NULL,
    enabled               INTEGER NOT NULL DEFAULT 1,
    position              INTEGER NOT NULL DEFAULT 0,
    config                TEXT NOT NULL DEFAULT '{}',
    config_schema_version INTEGER NOT NULL DEFAULT 0,
    generator_version     INTEGER NOT NULL DEFAULT 0,
    source_overrides      TEXT NOT NULL DEFAULT '[]',
    credential            TEXT NOT NULL,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL
);

CREATE INDEX idx_overlays_position ON overlays(position);
CREATE UNIQUE INDEX idx_overlays_credential ON overlays(credential);
