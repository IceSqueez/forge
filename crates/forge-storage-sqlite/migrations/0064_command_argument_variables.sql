CREATE TEMP TABLE variable_output_fields (
    kind_id TEXT NOT NULL,
    field   TEXT NOT NULL,
    PRIMARY KEY (kind_id, field)
);

INSERT INTO variable_output_fields (kind_id, field) VALUES
    ('core.action.cancel_scheduled', 'into_var'),
    ('core.action.schedule', 'due_at_into_var'),
    ('core.action.schedule', 'id_into_var'),
    ('core.args.set', 'name'),
    ('core.clipboard.read', 'into_var'),
    ('core.file.list', 'into_var'),
    ('core.file.read', 'target_var'),
    ('core.globals.get', 'into_arg'),
    ('core.latest.get', 'into_var'),
    ('core.math.evaluate', 'into_var'),
    ('core.random.bool', 'into_var'),
    ('core.random.float', 'into_var'),
    ('core.random.int', 'target_var'),
    ('core.random.pick', 'into_var'),
    ('core.string.concat', 'into_var'),
    ('core.string.format', 'into_var'),
    ('core.string.length', 'into_var'),
    ('core.string.lowercase', 'into_var'),
    ('core.string.regex_match', 'captures_into_var'),
    ('core.string.regex_match', 'into_var'),
    ('core.string.replace', 'into_var'),
    ('core.string.split', 'into_var'),
    ('core.string.substring', 'into_var'),
    ('core.string.titlecase', 'into_var'),
    ('core.string.trim', 'into_var'),
    ('core.string.uppercase', 'into_var'),
    ('core.time.add', 'into_var'),
    ('core.time.diff', 'into_var'),
    ('core.time.format', 'into_var'),
    ('core.time.now', 'into_var'),
    ('core.time.parse', 'into_var'),
    ('core.users.get_var', 'into_var'),
    ('script.run.named', 'target_var');

CREATE TEMP TABLE command_argument_writers (action_id TEXT PRIMARY KEY NOT NULL);

INSERT OR IGNORE INTO command_argument_writers (action_id)
WITH documents (id, steps) AS (
    SELECT id, CASE WHEN json_valid(sub_actions) THEN sub_actions ELSE '[]' END FROM actions
),
outputs (id, field, written, kind_id) AS (
    SELECT
        documents.id,
        node.key,
        json_extract(node.value, '$.value'),
        CASE
            WHEN node.path LIKE '%.config'
                THEN json_extract(documents.steps, substr(node.path, 1, length(node.path) - 7) || '.kind_id')
            WHEN node.path LIKE '%.config.value'
                THEN json_extract(documents.steps, substr(node.path, 1, length(node.path) - 13) || '.kind_id.value')
        END
    FROM documents, json_tree(documents.steps) AS node
    WHERE node.type = 'object'
      AND node.key IN (SELECT field FROM variable_output_fields)
      AND json_extract(node.value, '$.type') = 'string'
)
SELECT outputs.id
FROM outputs
JOIN variable_output_fields AS output
    ON output.kind_id = outputs.kind_id AND output.field = outputs.field
WHERE outputs.written IN ('args', '%args%');

CREATE TEMP TABLE command_argument_readers (
    action_id TEXT NOT NULL,
    platform  TEXT NOT NULL CHECK (platform IN ('kick', 'youtube')),
    PRIMARY KEY (action_id, platform)
);

INSERT INTO command_argument_readers (action_id, platform)
WITH RECURSIVE reached (action_id, platform) AS (
    SELECT binding.action_id, substr(instance.kind_id, 1, instr(instance.kind_id, '.') - 1)
    FROM action_trigger_instances AS binding
    JOIN trigger_instances AS instance ON instance.id = binding.trigger_instance_id
    WHERE instance.kind_id IN ('kick.chat.command', 'youtube.chat.command')
    UNION
    SELECT callee.id, reached.platform
    FROM reached
    JOIN actions AS caller ON caller.id = reached.action_id
    JOIN actions AS callee ON instr(caller.sub_actions, callee.id) > 0
    WHERE reached.action_id NOT IN (SELECT action_id FROM command_argument_writers)
)
SELECT action_id, platform FROM reached
WHERE action_id NOT IN (SELECT action_id FROM command_argument_writers);

CREATE TEMP TABLE kick_command_words (
    action_id TEXT NOT NULL,
    typed     TEXT NOT NULL,
    name      TEXT NOT NULL,
    PRIMARY KEY (action_id, typed)
);

INSERT OR IGNORE INTO kick_command_words (action_id, typed, name)
WITH RECURSIVE reached (action_id, phrase) AS (
    SELECT
        binding.action_id,
        coalesce(
            json_extract(
                CASE WHEN json_valid(instance.overrides) THEN instance.overrides ELSE '{}' END,
                '$.phrase.value'
            ),
            '!command'
        )
    FROM action_trigger_instances AS binding
    JOIN trigger_instances AS instance ON instance.id = binding.trigger_instance_id
    WHERE instance.kind_id = 'kick.chat.command'
    UNION
    SELECT callee.id, reached.phrase
    FROM reached
    JOIN actions AS caller ON caller.id = reached.action_id
    JOIN actions AS callee ON instr(caller.sub_actions, callee.id) > 0
),
spaced (action_id, phrase) AS (
    SELECT action_id, ltrim(replace(replace(phrase, char(9), ' '), char(10), ' ')) FROM reached
),
words (action_id, typed) AS (
    SELECT action_id, substr(phrase, 1, instr(phrase || ' ', ' ') - 1) FROM spaced
),
named (action_id, typed, name) AS (
    SELECT action_id, typed, ltrim(typed, '!"#$%&''()*+,-./:;<=>?@[\]^_`{|}~') FROM words
)
SELECT action_id, typed, name FROM named
WHERE name <> '' AND name <> typed;

CREATE TEMP TABLE command_name_edits (
    action_id TEXT NOT NULL,
    path      TEXT NOT NULL,
    old_text  TEXT NOT NULL,
    new_text  TEXT NOT NULL
);

INSERT INTO command_name_edits (action_id, path, old_text, new_text)
WITH documents (id, steps) AS (
    SELECT id, sub_actions FROM actions
    WHERE id IN (SELECT action_id FROM kick_command_words) AND json_valid(sub_actions)
),
conditions (id, path, condition) AS (
    SELECT
        documents.id,
        CASE node.type WHEN 'text' THEN node.fullkey ELSE node.fullkey || '.value' END,
        CASE node.type WHEN 'text' THEN node.atom ELSE json_extract(node.value, '$.value') END
    FROM documents, json_tree(documents.steps) AS node
    WHERE node.key IN ('condition', 'while_condition') AND node.type IN ('text', 'object')
)
SELECT conditions.id, conditions.path, quote.mark || word.typed || quote.mark, quote.mark || word.name || quote.mark
FROM conditions
JOIN kick_command_words AS word ON word.action_id = conditions.id
JOIN (SELECT '"' AS mark UNION ALL SELECT '`') AS quote
WHERE instr(conditions.condition, '%command_name%') > 0
UNION ALL
SELECT documents.id, matched.fullkey || '.value', word.typed, word.name
FROM documents
JOIN json_tree(documents.steps) AS switched
JOIN json_tree(documents.steps) AS matched
JOIN kick_command_words AS word ON word.action_id = documents.id
WHERE switched.key = 'expression'
  AND switched.type = 'object'
  AND instr(json_extract(switched.value, '$.value'), '%command_name%') > 0
  AND matched.key = 'match'
  AND matched.type = 'object'
  AND json_extract(matched.value, '$.value') = word.typed
  AND substr(matched.path, 1, length(switched.path) + 13) = switched.path || '.cases.value['
  AND substr(matched.path, length(switched.path) + 14) NOT LIKE '%.%.%';

WITH RECURSIVE ordered (action_id, path, old_text, new_text, position) AS (
    SELECT action_id, path, old_text, new_text, row_number() OVER (PARTITION BY action_id)
    FROM command_name_edits
),
applied (action_id, position, steps) AS (
    SELECT id, 0, sub_actions FROM actions
    WHERE id IN (SELECT action_id FROM command_name_edits)
    UNION ALL
    SELECT
        applied.action_id,
        edit.position,
        json_set(
            applied.steps,
            edit.path,
            replace(json_extract(applied.steps, edit.path), edit.old_text, edit.new_text)
        )
    FROM applied
    JOIN ordered AS edit
        ON edit.action_id = applied.action_id AND edit.position = applied.position + 1
)
UPDATE actions
SET sub_actions = (
    SELECT applied.steps FROM applied
    WHERE applied.action_id = actions.id
    ORDER BY applied.position DESC
    LIMIT 1
)
WHERE id IN (SELECT action_id FROM command_name_edits);

UPDATE actions
SET sub_actions = replace(
    replace(
        sub_actions,
        '"array_source":{"type":"string","value":"args"}',
        '"array_source":{"type":"string","value":"command.args"}'
    ),
    '"array_source":{"type":"string","value":"%args%"}',
    '"array_source":{"type":"string","value":"%command.args%"}'
)
WHERE id IN (SELECT action_id FROM command_argument_readers);

UPDATE actions
SET sub_actions = replace(replace(sub_actions, '%args%', '%command.tail%'), '%args.', '%command.args.')
WHERE id IN (SELECT action_id FROM command_argument_readers WHERE platform = 'kick');

UPDATE actions
SET sub_actions = replace(replace(sub_actions, '%args%', '%command.args%'), '%args.', '%command.args.')
WHERE id IN (SELECT action_id FROM command_argument_readers WHERE platform = 'youtube');

DROP TABLE temp.command_name_edits;
DROP TABLE temp.kick_command_words;
DROP TABLE temp.command_argument_readers;
DROP TABLE temp.command_argument_writers;
DROP TABLE temp.variable_output_fields;
