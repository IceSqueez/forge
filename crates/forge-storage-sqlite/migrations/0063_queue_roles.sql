ALTER TABLE queues ADD COLUMN role TEXT;

UPDATE queues
SET role = 'overlay_alerts'
WHERE id = (SELECT id FROM queues WHERE name = 'Overlay Alerts' ORDER BY id LIMIT 1);

CREATE UNIQUE INDEX queues_role ON queues(role) WHERE role IS NOT NULL;
