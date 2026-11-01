UPDATE trigger_instances
SET kind_id = 'obs.virtualcam.status_changed'
WHERE kind_id = 'obs.virtualcam.state_changed'
  AND NOT (
      user_defined = 0
      AND EXISTS (
          SELECT 1 FROM trigger_instances AS current
          WHERE current.kind_id = 'obs.virtualcam.status_changed' AND current.user_defined = 0
      )
  );

UPDATE settings
SET value = replace(value, '"obs.virtualcam.state_changed"', '"obs.virtualcam.status_changed"')
WHERE key = 'picker.favorites.triggers'
  AND instr(value, '"obs.virtualcam.state_changed"') > 0;

UPDATE actions
SET sub_actions = replace(sub_actions, '%obs.source.is_enabled%', '%obs.source.is_visible%')
WHERE instr(sub_actions, '%obs.source.is_enabled%') > 0;

UPDATE trigger_instances
SET overrides = replace(overrides, '%obs.source.is_enabled%', '%obs.source.is_visible%')
WHERE instr(overrides, '%obs.source.is_enabled%') > 0;

CREATE TEMP TABLE duration_steps (kind_id TEXT PRIMARY KEY NOT NULL);
INSERT INTO duration_steps (kind_id) VALUES
    ('overlay.send'),
    ('youtube.stream.insert_ad_break');

CREATE TEMP TABLE duration_fields (
    action_id TEXT NOT NULL,
    position  INTEGER NOT NULL,
    config    TEXT NOT NULL,
    PRIMARY KEY (action_id, position)
);
INSERT INTO duration_fields (action_id, position, config)
SELECT action_id, row_number() OVER (PARTITION BY action_id ORDER BY config), config
FROM (
    SELECT documents.id AS action_id, node.path AS config
    FROM (
        SELECT id, CASE WHEN json_valid(sub_actions) THEN sub_actions ELSE '[]' END AS steps
        FROM actions
    ) AS documents, json_tree(documents.steps) AS node
    WHERE node.key = 'duration_secs'
      AND node.type = 'object'
      AND json_type(documents.steps, node.path || '.duration_seconds') IS NULL
      AND (
          (node.path LIKE '%.config'
              AND json_extract(documents.steps, substr(node.path, 1, length(node.path) - 7) || '.kind_id')
                  IN (SELECT kind_id FROM duration_steps))
          OR (node.path LIKE '%.config.value'
              AND json_extract(documents.steps, substr(node.path, 1, length(node.path) - 13) || '.kind_id.value')
                  IN (SELECT kind_id FROM duration_steps))
      )
);
CREATE TEMP TABLE duration_rewritten (action_id TEXT PRIMARY KEY NOT NULL, steps TEXT NOT NULL);
INSERT INTO duration_rewritten (action_id, steps)
WITH RECURSIVE applied (action_id, steps, position) AS (
    SELECT id, sub_actions, 0 FROM actions WHERE id IN (SELECT action_id FROM duration_fields)
    UNION ALL
    SELECT applied.action_id,
        json_set(
            json_remove(applied.steps, field.config || '.duration_secs'),
            field.config || '.duration_seconds',
            json(json_extract(applied.steps, field.config || '.duration_secs'))
        ),
        applied.position + 1
    FROM applied
    JOIN duration_fields AS field
        ON field.action_id = applied.action_id AND field.position = applied.position + 1
)
SELECT action_id, steps FROM applied
WHERE position = (SELECT count(*) FROM duration_fields WHERE duration_fields.action_id = applied.action_id);
UPDATE actions SET sub_actions = (SELECT steps FROM duration_rewritten WHERE action_id = actions.id)
WHERE id IN (SELECT action_id FROM duration_rewritten);

CREATE TEMP TABLE ban_variable_readers (action_id TEXT PRIMARY KEY NOT NULL);
INSERT INTO ban_variable_readers (action_id)
WITH RECURSIVE reached (action_id) AS (
    SELECT binding.action_id
    FROM action_trigger_instances AS binding
    JOIN trigger_instances AS instance ON instance.id = binding.trigger_instance_id
    WHERE instance.kind_id IN ('kick.moderation.banned', 'youtube.channel.user_banned')
    UNION
    SELECT callee.id
    FROM reached
    JOIN actions AS caller ON caller.id = reached.action_id
    JOIN actions AS callee ON instr(caller.sub_actions, callee.id) > 0
)
SELECT action_id FROM reached;
UPDATE actions
SET sub_actions = replace(sub_actions, '%duration_secs%', '%duration_seconds%')
WHERE id IN (SELECT action_id FROM ban_variable_readers)
  AND instr(sub_actions, '%duration_secs%') > 0;

DROP TABLE temp.ban_variable_readers;
DROP TABLE temp.duration_rewritten;
DROP TABLE temp.duration_fields;
DROP TABLE temp.duration_steps;
