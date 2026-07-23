-- Consolidate the removed Twitch Guest Star slot-update trigger descriptor
-- into the surviving guest-update descriptor.
--
-- `trigger_instances.kind_id` stores the TriggerKindDescriptor::id() string,
-- not the Twitch EventSub subscription type / event kind. Twitch descriptor
-- ids have always carried the `twitch.` prefix and were unaffected by the
-- event-kind rename campaign; only this one descriptor was removed outright
-- (`twitch.guest_star.slot_updated` no longer exists - its payload folded
-- into `twitch.guest_star.guest_updated`).
--
-- Same delete-then-update strategy as 0018/0037: the default row for the
-- removed descriptor is deleted (boot re-seeds the surviving default via
-- upsert_default), and user-defined rows are updated in place onto the
-- surviving descriptor id. This may leave more than one user_defined row
-- with the same kind_id, which is fine - no uniqueness constraint applies
-- to non-default rows.

DELETE FROM trigger_instances
WHERE user_defined = 0 AND kind_id = 'twitch.guest_star.slot_updated';

UPDATE trigger_instances SET kind_id = 'twitch.guest_star.guest_updated'
WHERE user_defined = 1 AND kind_id = 'twitch.guest_star.slot_updated';
