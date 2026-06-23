-- Rewrite persisted MIDI and Hotkey trigger_instances.kind_id from pre-catalog
-- ids to catalog-canonical ids (RFC-087 Decision 1).
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
-- Unique-index safety: all 4 renames are 1-to-1, and this migration runs before
-- boot seeding, so no new-kind default row exists yet. The UPDATE therefore
-- cannot create a duplicate default under the partial UNIQUE INDEX
-- idx_trigger_instances_default_unique ON trigger_instances(kind_id)
-- WHERE user_defined = 0.
--
-- Discord sub-action ids live inside the actions.sub_actions JSON blob, not in
-- trigger_instances, and are rewritten in registry_migration.rs (format 1 -> 2).
--
-- Rows with kind_ids not listed here are left untouched (idempotent on fresh DB).

UPDATE trigger_instances SET kind_id = 'midi.input.note_on'
WHERE kind_id = 'midi.event.note_on';

UPDATE trigger_instances SET kind_id = 'midi.input.note_off'
WHERE kind_id = 'midi.event.note_off';

UPDATE trigger_instances SET kind_id = 'midi.input.control_change'
WHERE kind_id = 'midi.event.control_change';

UPDATE trigger_instances SET kind_id = 'hotkey.global.pressed'
WHERE kind_id = 'hotkey.triggered';
