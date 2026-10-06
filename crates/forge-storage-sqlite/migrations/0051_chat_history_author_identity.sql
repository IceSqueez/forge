ALTER TABLE chat_history ADD COLUMN author_id TEXT;

UPDATE chat_history
SET author_id = (
    SELECT CASE WHEN COUNT(*) = 1 THEN MIN(v.viewer_id) END
    FROM viewers v
    WHERE v.platform = chat_history.source
      AND v.username = chat_history.author COLLATE NOCASE
);

CREATE INDEX IF NOT EXISTS idx_chat_history_author_seq
    ON chat_history (source, author_id, seq DESC)
    WHERE author_id IS NOT NULL AND is_event = 0;

CREATE INDEX IF NOT EXISTS idx_chat_history_authorless_seq
    ON chat_history (seq DESC)
    WHERE author_id IS NULL OR is_event = 1;

DELETE FROM voice_aliases
WHERE rowid IN (
    SELECT rowid FROM (
        SELECT rowid,
               ROW_NUMBER() OVER (
                   PARTITION BY viewer_id
                   ORDER BY updated_at DESC, rowid DESC
               ) AS newest_rank
        FROM voice_aliases
    )
    WHERE newest_rank > 1
);

DROP INDEX IF EXISTS ix_voice_aliases_viewer;

CREATE UNIQUE INDEX IF NOT EXISTS ux_voice_aliases_viewer ON voice_aliases (viewer_id);

DELETE FROM settings WHERE key = 'chat_history.store_limit';
