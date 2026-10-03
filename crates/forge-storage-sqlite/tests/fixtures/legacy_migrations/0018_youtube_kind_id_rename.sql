-- Rewrite persisted YouTube trigger_instances.kind_id from pre-catalog ids to
-- catalog-canonical ids (RFC-085).
--
-- Strategy per RFC-085 OQ-1:
--   user_defined = 0 (default rows)  → DELETE; boot recreates via upsert_default
--   user_defined = 1 (user rows)     → UPDATE kind_id in-place
--
-- The ban+timeout consolidation (two old ids → one new id) would violate the
-- partial UNIQUE INDEX idx_trigger_instances_default_unique ON trigger_instances(kind_id)
-- WHERE user_defined = 0 if both default rows existed and we tried an UPDATE.
-- Deleting defaults sidesteps the collision entirely. No such constraint exists
-- for user_defined = 1 rows, so the UPDATE path is safe there.
--
-- Rows with kind_ids not listed here are left untouched (idempotent on fresh DB).

-- Step 1: remove default rows for all 8 old ids.
-- Boot will re-seed canonical defaults from the registry.
DELETE FROM trigger_instances
WHERE user_defined = 0
  AND kind_id IN (
      'youtube.support.super_chat',
      'youtube.support.super_sticker',
      'youtube.support.new_member',
      'youtube.support.member_milestone',
      'youtube.channel.live_broadcast_started',
      'youtube.channel.live_broadcast_ended',
      'youtube.moderation.ban',
      'youtube.moderation.timeout'
  );

-- Step 2: rewrite user-defined rows — simple 1-to-1 renames.
UPDATE trigger_instances SET kind_id = 'youtube.chat.super_chat'
WHERE user_defined = 1 AND kind_id = 'youtube.support.super_chat';

UPDATE trigger_instances SET kind_id = 'youtube.chat.super_sticker'
WHERE user_defined = 1 AND kind_id = 'youtube.support.super_sticker';

UPDATE trigger_instances SET kind_id = 'youtube.channel.member'
WHERE user_defined = 1 AND kind_id = 'youtube.support.new_member';

UPDATE trigger_instances SET kind_id = 'youtube.channel.member_milestone'
WHERE user_defined = 1 AND kind_id = 'youtube.support.member_milestone';

UPDATE trigger_instances SET kind_id = 'youtube.stream.online'
WHERE user_defined = 1 AND kind_id = 'youtube.channel.live_broadcast_started';

UPDATE trigger_instances SET kind_id = 'youtube.stream.offline'
WHERE user_defined = 1 AND kind_id = 'youtube.channel.live_broadcast_ended';

-- Step 3: ban + timeout consolidation — both old ids map to youtube.channel.user_banned.
-- No UNIQUE constraint applies to user_defined = 1 rows, so two UPDATE statements
-- may produce multiple rows with kind_id = 'youtube.channel.user_banned'; that is
-- intentional (each user-configured instance is a separate trigger with its own
-- overrides/enabled state, and the user can reconcile them post-migration).
UPDATE trigger_instances SET kind_id = 'youtube.channel.user_banned'
WHERE user_defined = 1 AND kind_id = 'youtube.moderation.ban';

UPDATE trigger_instances SET kind_id = 'youtube.channel.user_banned'
WHERE user_defined = 1 AND kind_id = 'youtube.moderation.timeout';
