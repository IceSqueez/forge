ALTER TABLE trigger_instances ADD COLUMN cooldown_secs INTEGER NOT NULL DEFAULT 0;
ALTER TABLE trigger_instances ADD COLUMN cooldown_global INTEGER NOT NULL DEFAULT 1;

UPDATE trigger_instances
SET cooldown_secs = CASE WHEN user_cooldown_secs > 0 THEN user_cooldown_secs ELSE global_cooldown_secs END,
    cooldown_global = CASE WHEN user_cooldown_secs > 0 THEN 0 ELSE 1 END;

ALTER TABLE trigger_instances DROP COLUMN global_cooldown_secs;
ALTER TABLE trigger_instances DROP COLUMN user_cooldown_secs;
