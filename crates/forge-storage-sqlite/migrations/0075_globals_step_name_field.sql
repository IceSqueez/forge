CREATE TEMP TABLE globals_name_steps (kind_id TEXT PRIMARY KEY NOT NULL);
INSERT INTO globals_name_steps (kind_id) VALUES
    ('core.globals.toggle'),
    ('core.globals.decrement'),
    ('core.globals.array_append'),
    ('core.globals.array_remove');
CREATE TEMP TABLE key_fields (
    action_id TEXT NOT NULL,
    position  INTEGER NOT NULL,
    config    TEXT NOT NULL,
    PRIMARY KEY (action_id, position)
);
INSERT INTO key_fields (action_id, position, config)
SELECT action_id, row_number() OVER (PARTITION BY action_id ORDER BY config), config
FROM (
    SELECT documents.id AS action_id, node.path AS config
    FROM (
        SELECT id, CASE WHEN json_valid(sub_actions) THEN sub_actions ELSE '[]' END AS steps
        FROM actions
    ) AS documents, json_tree(documents.steps) AS node
    WHERE node.key = 'key'
      AND node.type = 'object'
      AND json_type(documents.steps, node.path || '.name') IS NULL
      AND (
          (node.path LIKE '%.config'
              AND json_extract(documents.steps, substr(node.path, 1, length(node.path) - 7) || '.kind_id')
                  IN (SELECT kind_id FROM globals_name_steps))
          OR (node.path LIKE '%.config.value'
              AND json_extract(documents.steps, substr(node.path, 1, length(node.path) - 13) || '.kind_id.value')
                  IN (SELECT kind_id FROM globals_name_steps))
      )
);
CREATE TEMP TABLE rewritten (action_id TEXT PRIMARY KEY NOT NULL, steps TEXT NOT NULL);
INSERT INTO rewritten (action_id, steps)
WITH RECURSIVE applied (action_id, steps, position) AS (
    SELECT id, sub_actions, 0 FROM actions WHERE id IN (SELECT action_id FROM key_fields)
    UNION ALL
    SELECT applied.action_id,
        json_set(
            json_remove(applied.steps, field.config || '.key'),
            field.config || '.name',
            json(json_extract(applied.steps, field.config || '.key'))
        ),
        applied.position + 1
    FROM applied
    JOIN key_fields AS field
        ON field.action_id = applied.action_id AND field.position = applied.position + 1
)
SELECT action_id, steps FROM applied
WHERE position = (SELECT count(*) FROM key_fields WHERE key_fields.action_id = applied.action_id);
UPDATE actions SET sub_actions = (SELECT steps FROM rewritten WHERE action_id = actions.id)
WHERE id IN (SELECT action_id FROM rewritten);
DROP TABLE rewritten;
DROP TABLE key_fields;
DROP TABLE globals_name_steps;
