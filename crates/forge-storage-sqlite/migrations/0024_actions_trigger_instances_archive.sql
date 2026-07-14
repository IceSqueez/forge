ALTER TABLE actions ADD COLUMN archived_at INTEGER;
CREATE INDEX actions_archived ON actions(archived_at) WHERE archived_at IS NOT NULL;

ALTER TABLE trigger_instances ADD COLUMN archived_at INTEGER;
CREATE INDEX trigger_instances_archived ON trigger_instances(archived_at) WHERE archived_at IS NOT NULL;
