CREATE TABLE trigger_instances (
    id           TEXT PRIMARY KEY,
    kind_id      TEXT NOT NULL,
    name         TEXT NOT NULL,
    overrides    TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 1,
    user_defined INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX idx_trigger_instances_default_unique
    ON trigger_instances(kind_id)
    WHERE user_defined = 0;

CREATE INDEX idx_trigger_instances_user_defined
    ON trigger_instances(user_defined);

CREATE TABLE action_trigger_instances (
    action_id           TEXT NOT NULL,
    trigger_instance_id TEXT NOT NULL,
    position            INTEGER NOT NULL,
    PRIMARY KEY (action_id, trigger_instance_id),
    FOREIGN KEY (action_id) REFERENCES actions(id) ON DELETE CASCADE,
    FOREIGN KEY (trigger_instance_id) REFERENCES trigger_instances(id) ON DELETE RESTRICT
);

CREATE INDEX idx_action_trigger_instances_instance
    ON action_trigger_instances(trigger_instance_id);
