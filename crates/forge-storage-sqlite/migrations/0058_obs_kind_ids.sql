CREATE TEMP TABLE retired_kind_ids (
    store      TEXT NOT NULL CHECK (store IN ('trigger', 'sub_action')),
    retired_id TEXT NOT NULL,
    current_id TEXT NOT NULL,
    PRIMARY KEY (store, retired_id)
);

INSERT INTO retired_kind_ids (store, retired_id, current_id) VALUES
    ('trigger', 'obs.filters.created', 'obs.filter.created'),
    ('trigger', 'obs.filters.removed', 'obs.filter.removed'),
    ('trigger', 'obs.filters.enabled_changed', 'obs.filter.enabled_changed'),
    ('trigger', 'obs.record.started', 'obs.recording.started'),
    ('trigger', 'obs.record.starting', 'obs.recording.starting'),
    ('trigger', 'obs.record.stopped', 'obs.recording.stopped'),
    ('trigger', 'obs.record.stopping', 'obs.recording.stopping'),
    ('trigger', 'obs.record.paused', 'obs.recording.paused'),
    ('trigger', 'obs.record.resumed', 'obs.recording.resumed'),
    ('trigger', 'obs.record.file_changed', 'obs.recording.file_changed'),
    ('trigger', 'obs.record.status_changed', 'obs.recording.status_changed'),
    ('trigger', 'obs.stream.started', 'obs.streaming.started'),
    ('trigger', 'obs.stream.starting', 'obs.streaming.starting'),
    ('trigger', 'obs.stream.stopped', 'obs.streaming.stopped'),
    ('trigger', 'obs.stream.stopping', 'obs.streaming.stopping'),
    ('trigger', 'obs.stream.status_changed', 'obs.streaming.status_changed'),
    ('trigger', 'obs.scenes.created', 'obs.scene.created'),
    ('trigger', 'obs.scenes.removed', 'obs.scene.removed'),
    ('trigger', 'obs.scenes.renamed', 'obs.scene.renamed'),
    ('trigger', 'obs.scenes.list_changed', 'obs.scene.list_changed'),
    ('trigger', 'obs.scenes.preview_changed', 'obs.scene.preview_changed'),
    ('trigger', 'obs.scenes.current_changed', 'obs.scene.changed'),
    ('trigger', 'obs.sources.input_created', 'obs.source.input_created'),
    ('trigger', 'obs.sources.input_removed', 'obs.source.input_removed'),
    ('trigger', 'obs.sources.input_renamed', 'obs.source.input_renamed'),
    ('trigger', 'obs.sources.scene_item_created', 'obs.source.scene_item_created'),
    ('trigger', 'obs.sources.scene_item_removed', 'obs.source.scene_item_removed'),
    ('trigger', 'obs.sources.scene_item_lock_changed', 'obs.source.lock_changed'),
    ('trigger', 'obs.sources.scene_item_visibility_changed', 'obs.source.visibility_changed'),
    ('trigger', 'obs.collection.current_changed', 'obs.collection.changed'),
    ('sub_action', 'obs.scenes.get_list', 'obs.scene.get_list'),
    ('sub_action', 'obs.scenes.switch_current', 'obs.scene.switch_current'),
    ('sub_action', 'obs.scenes.set_preview', 'obs.scene.set_preview'),
    ('sub_action', 'obs.scenes.set_transition', 'obs.scene.set_transition'),
    ('sub_action', 'obs.sources.get_list', 'obs.source.get_list'),
    ('sub_action', 'obs.sources.get_input_settings', 'obs.source.get_input_settings'),
    ('sub_action', 'obs.sources.set_input_settings', 'obs.source.set_input_settings'),
    ('sub_action', 'obs.sources.set_visible', 'obs.source.set_visible'),
    ('sub_action', 'obs.sources.set_locked', 'obs.source.set_locked'),
    ('sub_action', 'obs.record.get_status', 'obs.recording.get_status'),
    ('sub_action', 'obs.record.start', 'obs.recording.start'),
    ('sub_action', 'obs.record.stop', 'obs.recording.stop'),
    ('sub_action', 'obs.record.pause', 'obs.recording.pause'),
    ('sub_action', 'obs.record.resume', 'obs.recording.resume'),
    ('sub_action', 'obs.record.toggle_pause', 'obs.recording.toggle_pause'),
    ('sub_action', 'obs.record.set_active', 'obs.recording.set_active'),
    ('sub_action', 'obs.record.set_directory', 'obs.recording.set_directory'),
    ('sub_action', 'obs.stream.get_status', 'obs.streaming.get_status'),
    ('sub_action', 'obs.stream.start', 'obs.streaming.start'),
    ('sub_action', 'obs.stream.stop', 'obs.streaming.stop'),
    ('sub_action', 'obs.stream.send_caption', 'obs.streaming.send_caption'),
    ('sub_action', 'obs.stream.set_active', 'obs.streaming.set_active'),
    ('sub_action', 'obs.scene_collection.switch', 'obs.collection.switch');

UPDATE trigger_instances
SET kind_id = (
    SELECT current_id FROM retired_kind_ids
    WHERE store = 'trigger' AND retired_id = trigger_instances.kind_id
)
WHERE kind_id IN (SELECT retired_id FROM retired_kind_ids WHERE store = 'trigger');
