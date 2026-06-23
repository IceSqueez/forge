-- Rewrite persisted Kick trigger_instances.kind_id from pre-catalog ids to
-- catalog-canonical ids (RFC-086).
--
-- Strategy: a SINGLE in-place UPDATE per rename covering BOTH default
-- (user_defined = 0) and user (user_defined = 1) rows. The earlier
-- DELETE-defaults-then-UPDATE-user pair is unsafe: action_trigger_instances
-- has FK trigger_instance_id -> trigger_instances(id) ON DELETE RESTRICT. If a
-- user attached an action to a default trigger of a renamed kind, the DELETE
-- fails with SQLITE_CONSTRAINT_FOREIGNKEY, aborts the whole migration, and the
-- database never opens. The in-place UPDATE never deletes an FK-referenced row,
-- so the action_trigger_instances links survive untouched. Boot's upsert_default
-- then idempotently normalizes the surviving default row's name/config.
--
-- Unique-index safety: all 6 renames are 1-to-1, and this migration runs before
-- boot seeding, so no new-kind default row exists yet. The UPDATE therefore
-- cannot create a duplicate default under the partial UNIQUE INDEX
-- idx_trigger_instances_default_unique ON trigger_instances(kind_id)
-- WHERE user_defined = 0.
--
-- Rows with kind_ids not listed here are left untouched (idempotent on fresh DB).

UPDATE trigger_instances SET kind_id = 'kick.chat.message'
WHERE kind_id = 'kick.chat';

UPDATE trigger_instances SET kind_id = 'kick.chat.message_deleted'
WHERE kind_id = 'kick.message_deleted';

UPDATE trigger_instances SET kind_id = 'kick.channel.banned'
WHERE kind_id = 'kick.ban';

UPDATE trigger_instances SET kind_id = 'kick.channel.subscriber'
WHERE kind_id = 'kick.sub';

UPDATE trigger_instances SET kind_id = 'kick.channel.subscription_gift'
WHERE kind_id = 'kick.sub_gift';

UPDATE trigger_instances SET kind_id = 'kick.channel.host_received'
WHERE kind_id = 'kick.host';
