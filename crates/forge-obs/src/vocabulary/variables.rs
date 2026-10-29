pub(crate) mod connection {
    pub(crate) const REASON: &str = "obs.reason";
    pub(crate) const DETAIL: &str = "obs.detail";
    pub(crate) const ERROR_MESSAGE: &str = "obs.error_message";
}

pub(crate) mod scene {
    pub(crate) const PROGRAM: &str = "obs.scene";
    pub(crate) const PREVIOUS: &str = "obs.previous_scene";
    pub(crate) const NAME: &str = "obs.scene.name";
    pub(crate) const NAME_OLD: &str = "obs.scene.name_old";
    pub(crate) const NAME_NEW: &str = "obs.scene.name_new";
    pub(crate) const ALL_NAMES: &str = "obs.scene_names";
}

pub(crate) mod source {
    pub(crate) const NAME: &str = "obs.source.name";
    pub(crate) const NAME_OLD: &str = "obs.source.name_old";
    pub(crate) const NAME_NEW: &str = "obs.source.name_new";
    pub(crate) const KIND: &str = "obs.source.kind";
    pub(crate) const ITEM_ID: &str = "obs.source.item_id";
    pub(crate) const IS_LOCKED: &str = "obs.source.is_locked";
    pub(crate) const IS_ENABLED: &str = "obs.source.is_enabled";
    pub(crate) const IS_MUTED: &str = "obs.source.is_muted";
    pub(crate) const VOLUME_DB: &str = "obs.source.volume_db";
    pub(crate) const VOLUME_MULTIPLIER: &str = "obs.source.volume_multiplier";
    pub(crate) const BALANCE: &str = "obs.source.balance";
    pub(crate) const SYNC_OFFSET_MS: &str = "obs.source.sync_offset_ms";
}

pub(crate) mod filter {
    pub(crate) const NAME: &str = "obs.filter.name";
    pub(crate) const KIND: &str = "obs.filter.kind";
    pub(crate) const IS_ENABLED: &str = "obs.filter.is_enabled";
}

pub(crate) mod collection {
    pub(crate) const NAME: &str = "obs.collection";
    pub(crate) const ALL_NAMES: &str = "obs.collection.all_names";
}

pub(crate) mod profile {
    pub(crate) const NAME: &str = "obs.profile.name";
    pub(crate) const ALL_NAMES: &str = "obs.profile.all_names";
}

pub(crate) mod recording {
    pub(crate) const OUTPUT_STATE: &str = "obs.record.output_state";
    pub(crate) const IS_ACTIVE: &str = "obs.record.is_active";
    pub(crate) const IS_PAUSED: &str = "obs.record.is_paused";
    pub(crate) const DURATION_MS: &str = "obs.record.duration_ms";
    pub(crate) const OUTPUT_PATH: &str = "obs.record.output_path";
}

pub(crate) mod streaming {
    pub(crate) const OUTPUT_STATE: &str = "obs.stream.output_state";
    pub(crate) const IS_ACTIVE: &str = "obs.stream.is_active";
    pub(crate) const DURATION_MS: &str = "obs.stream.duration_ms";
}

pub(crate) mod virtualcam {
    pub(crate) const OUTPUT_STATE: &str = "obs.virtualcam.output_state";
    pub(crate) const IS_ACTIVE: &str = "obs.virtualcam.is_active";
}

pub(crate) mod studio {
    pub(crate) const ENABLED: &str = "obs.studio.enabled";
}

pub(crate) mod transition {
    pub(crate) const NAME: &str = "obs.transition.name";
}

pub(crate) mod scene_query {
    pub(crate) const ALL_NAMES: &str = "obs.scenes.all_names";
    pub(crate) const CURRENT: &str = "obs.scenes.current";
}

pub(crate) mod source_query {
    pub(crate) const ALL_NAMES: &str = "obs.sources.all_names";
}

pub(crate) mod input_query {
    pub(crate) const SETTINGS: &str = "obs.input.settings";
    pub(crate) const KIND: &str = "obs.input.kind";
}
