-- Rewrite persisted Kick trigger_instances.kind_id from the 0019-era ids to
-- the event-taxonomy-campaign canonical ids.
--
-- Strategy: a single in-place UPDATE per rename covering BOTH default
-- (user_defined = 0) and user (user_defined = 1) rows, per the 0019 precedent.
-- All 9 renames are 1-to-1, so no duplicate default row can arise under the
-- partial UNIQUE INDEX idx_trigger_instances_default_unique ON
-- trigger_instances(kind_id) WHERE user_defined = 0.
--
-- kick.chat.command is unchanged and not listed here.
-- Rows with kind_ids not listed here are left untouched (idempotent on fresh DB).

UPDATE trigger_instances SET kind_id = 'kick.chat.message.sent'
WHERE kind_id = 'kick.chat.message';

UPDATE trigger_instances SET kind_id = 'kick.chat.message.deleted'
WHERE kind_id = 'kick.chat.message_deleted';

UPDATE trigger_instances SET kind_id = 'kick.moderation.banned'
WHERE kind_id = 'kick.channel.banned';

UPDATE trigger_instances SET kind_id = 'kick.channel.subscribed'
WHERE kind_id = 'kick.channel.subscriber';

UPDATE trigger_instances SET kind_id = 'kick.channel.subscription.gifts'
WHERE kind_id = 'kick.channel.subscription_gift';

UPDATE trigger_instances SET kind_id = 'kick.channel.hosted'
WHERE kind_id = 'kick.channel.host_received';

UPDATE trigger_instances SET kind_id = 'kick.livestream.status.updated'
WHERE kind_id = 'kick.channel.livestream_status';

UPDATE trigger_instances SET kind_id = 'kick.livestream.metadata.updated'
WHERE kind_id = 'kick.channel.livestream_metadata';

UPDATE trigger_instances SET kind_id = 'kick.channel.reward.redemption.updated'
WHERE kind_id = 'kick.channel.reward_redeemed';
