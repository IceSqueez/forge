ALTER TABLE queues ADD COLUMN description TEXT NOT NULL DEFAULT '';

UPDATE queues
SET description = 'Catch-all queue for actions without explicit queue assignment'
WHERE id = '00000000000000000000000000' AND description = '';
