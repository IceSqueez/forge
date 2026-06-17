-- Rewrite persisted Kick trigger_instances.kind_id from pre-catalog ids to
-- catalog-canonical ids (RFC-086).
--
-- Strategy per RFC-086 OQ-1:
--   user_defined = 0 (default rows)  → DELETE; boot recreates via upsert_default
--   user_defined = 1 (user rows)     → UPDATE kind_id in-place
--
-- All 6 renames are 1-to-1; no consolidation occurs, so there is no risk of
-- violating the partial UNIQUE INDEX idx_trigger_instances_default_unique ON
-- trigger_instances(kind_id) WHERE user_defined = 0 via UPDATE. Deleting
-- defaults before boot re-seeds them is still the correct pattern for parity
-- with 0018 and for correctness against any stale default row.
--
-- Rows with kind_ids not listed here are left untouched (idempotent on fresh DB).

-- Step 1: remove default rows for all 6 old ids.
-- Boot will re-seed canonical defaults from the registry.
DELETE FROM trigger_instances
WHERE user_defined = 0
  AND kind_id IN (
      'kick.chat',
      'kick.message_deleted',
      'kick.ban',
      'kick.sub',
      'kick.sub_gift',
      'kick.host'
  );

-- Step 2: rewrite user-defined rows — 1-to-1 renames.
UPDATE trigger_instances SET kind_id = 'kick.chat.message'
WHERE user_defined = 1 AND kind_id = 'kick.chat';

UPDATE trigger_instances SET kind_id = 'kick.chat.message_deleted'
WHERE user_defined = 1 AND kind_id = 'kick.message_deleted';

UPDATE trigger_instances SET kind_id = 'kick.channel.banned'
WHERE user_defined = 1 AND kind_id = 'kick.ban';

UPDATE trigger_instances SET kind_id = 'kick.channel.subscriber'
WHERE user_defined = 1 AND kind_id = 'kick.sub';

UPDATE trigger_instances SET kind_id = 'kick.channel.subscription_gift'
WHERE user_defined = 1 AND kind_id = 'kick.sub_gift';

UPDATE trigger_instances SET kind_id = 'kick.channel.host_received'
WHERE user_defined = 1 AND kind_id = 'kick.host';
