ALTER TABLE trigger_instances ADD COLUMN global_cooldown_secs INTEGER NOT NULL DEFAULT 0;
ALTER TABLE trigger_instances ADD COLUMN user_cooldown_secs INTEGER NOT NULL DEFAULT 0;
