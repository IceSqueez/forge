ALTER TABLE queues ADD COLUMN concurrency INTEGER NOT NULL DEFAULT 8;

UPDATE queues SET concurrency = 1 WHERE blocking = 1;
