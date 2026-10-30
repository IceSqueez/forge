DROP TABLE IF EXISTS temp.retired_kind_ids;

CREATE TEMP TABLE retired_kind_ids (
    store      TEXT NOT NULL CHECK (store IN ('trigger', 'sub_action')),
    retired_id TEXT NOT NULL,
    current_id TEXT NOT NULL,
    PRIMARY KEY (store, retired_id)
);

INSERT INTO retired_kind_ids (store, retired_id, current_id) VALUES
    ('sub_action', 'vtube.params.reset', 'vtube.expressions.reset');
