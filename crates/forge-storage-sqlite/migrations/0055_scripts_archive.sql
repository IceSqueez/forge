ALTER TABLE scripts ADD COLUMN archived_at INTEGER;
CREATE INDEX scripts_archived ON scripts(archived_at) WHERE archived_at IS NOT NULL;
