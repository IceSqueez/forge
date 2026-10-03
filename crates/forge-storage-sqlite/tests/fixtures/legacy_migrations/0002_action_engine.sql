CREATE TABLE queues (
    id       TEXT    PRIMARY KEY NOT NULL,
    name     TEXT    NOT NULL,
    blocking INTEGER NOT NULL DEFAULT 0
);

INSERT INTO queues (id, name, blocking) VALUES ('00000000000000000000000000', 'Default', 0);

CREATE TABLE actions (
    id           TEXT    PRIMARY KEY NOT NULL,
    name         TEXT    NOT NULL,
    group_name   TEXT    NOT NULL DEFAULT '',
    queue_id     TEXT    NOT NULL REFERENCES queues(id),
    enabled      INTEGER NOT NULL DEFAULT 1,
    concurrent   INTEGER NOT NULL DEFAULT 0,
    bypass_pause INTEGER NOT NULL DEFAULT 0,
    description  TEXT    NOT NULL DEFAULT '',
    sub_actions  TEXT    NOT NULL DEFAULT '[]'
);

CREATE INDEX idx_actions_group ON actions(group_name);
CREATE INDEX idx_actions_queue ON actions(queue_id);

CREATE TABLE triggers (
    id        TEXT PRIMARY KEY NOT NULL,
    action_id TEXT NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
    kind      TEXT NOT NULL,
    config    TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_triggers_action ON triggers(action_id);

CREATE TABLE commands (
    id            TEXT    PRIMARY KEY NOT NULL,
    action_id     TEXT    NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    cooldown_secs INTEGER NOT NULL DEFAULT 0,
    permission    TEXT    NOT NULL DEFAULT 'everyone'
);

CREATE UNIQUE INDEX idx_commands_name ON commands(name);
