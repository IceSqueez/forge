-- Rewrite persisted MIDI and Hotkey trigger_instances.kind_id from pre-catalog
-- ids to catalog-canonical ids (RFC-087 Decision 1).
--
-- Strategy (parity with 0018 / 0019):
--   user_defined = 0 (default rows)  → DELETE; boot recreates via upsert_default
--   user_defined = 1 (user rows)     → UPDATE kind_id in-place
--
-- All renames are 1-to-1; no consolidation occurs, so there is no risk of
-- violating the partial UNIQUE INDEX idx_trigger_instances_default_unique ON
-- trigger_instances(kind_id) WHERE user_defined = 0 via UPDATE. Deleting
-- defaults before boot re-seeds them is still the correct pattern for parity
-- with prior renames and for correctness against any stale default row.
--
-- Discord sub-action ids live inside the actions.sub_actions JSON blob, not in
-- trigger_instances, and are rewritten in registry_migration.rs (format 1 -> 2).
--
-- Rows with kind_ids not listed here are left untouched (idempotent on fresh DB).

-- Step 1: remove default rows for all old ids.
-- Boot will re-seed canonical defaults from the registry.
DELETE FROM trigger_instances
WHERE user_defined = 0
  AND kind_id IN (
      'midi.event.note_on',
      'midi.event.note_off',
      'midi.event.control_change',
      'hotkey.triggered'
  );

-- Step 2: rewrite user-defined rows — 1-to-1 renames.
UPDATE trigger_instances SET kind_id = 'midi.input.note_on'
WHERE user_defined = 1 AND kind_id = 'midi.event.note_on';

UPDATE trigger_instances SET kind_id = 'midi.input.note_off'
WHERE user_defined = 1 AND kind_id = 'midi.event.note_off';

UPDATE trigger_instances SET kind_id = 'midi.input.control_change'
WHERE user_defined = 1 AND kind_id = 'midi.event.control_change';

UPDATE trigger_instances SET kind_id = 'hotkey.global.pressed'
WHERE user_defined = 1 AND kind_id = 'hotkey.triggered';
