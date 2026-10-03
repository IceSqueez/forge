ALTER TABLE globals ADD COLUMN archived_at INTEGER;
CREATE INDEX globals_archived ON globals(archived_at) WHERE archived_at IS NOT NULL;
