INSERT OR IGNORE INTO media_references (referrer_kind, referrer_id, slot, blob_id)
SELECT 'library', id, 'image', id
FROM media_blobs
WHERE format IN ('png', 'gif', 'webp', 'svg');
