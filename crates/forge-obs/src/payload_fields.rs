pub(crate) mod connection {
    pub(crate) const REASON: &str = "reason";
    pub(crate) const DETAIL: &str = "detail";
    pub(crate) const ERROR_MESSAGE: &str = "error_message";

    pub(crate) mod reason {
        pub(crate) const CONNECTION_LOST: &str = "connection_lost";
    }
}

pub(crate) mod scene {
    pub(crate) const FROM_SCENE: &str = "from_scene";
    pub(crate) const TO_SCENE: &str = "to_scene";
    pub(crate) const ALL_NAMES: &str = "all_names";
    pub(crate) const SCENE_NAME: &str = "scene_name";
    pub(crate) const SCENE_NAME_OLD: &str = "scene_name_old";
    pub(crate) const SCENE_NAME_NEW: &str = "scene_name_new";
}

pub(crate) mod profile {
    pub(crate) const PROFILE_NAME: &str = "profile_name";
    pub(crate) const ALL_NAMES: &str = "all_names";
}

pub(crate) mod collection {
    pub(crate) const ALL_NAMES: &str = "all_names";
    pub(crate) const COLLECTION_NAME: &str = "collection_name";
}

pub(crate) mod recording {
    pub(crate) const OUTPUT_PATH: &str = "output_path";
    pub(crate) const IS_ACTIVE: &str = "is_active";
}

pub(crate) mod streaming {
    pub(crate) const IS_ACTIVE: &str = "is_active";
}

pub(crate) mod virtualcam {
    pub(crate) const IS_ACTIVE: &str = "is_active";
}

pub(crate) mod transition {
    pub(crate) const TRANSITION_NAME: &str = "transition_name";
}

pub(crate) mod audio {
    pub(crate) const SOURCE_NAME: &str = "source_name";
    pub(crate) const IS_MUTED: &str = "is_muted";
    pub(crate) const VOLUME_DB: &str = "volume_db";
    pub(crate) const VOLUME_MULTIPLIER: &str = "volume_multiplier";
    pub(crate) const BALANCE: &str = "balance";
    pub(crate) const SYNC_OFFSET_MS: &str = "sync_offset_ms";
}

pub(crate) mod source {
    pub(crate) const SOURCE_NAME: &str = "source_name";
    pub(crate) const SOURCE_KIND: &str = "source_kind";
    pub(crate) const SOURCE_NAME_OLD: &str = "source_name_old";
    pub(crate) const SOURCE_NAME_NEW: &str = "source_name_new";
    pub(crate) const SCENE_NAME: &str = "scene_name";
    pub(crate) const IS_LOCKED: &str = "is_locked";
    pub(crate) const IS_VISIBLE: &str = "is_visible";
}

pub(crate) mod filter {
    pub(crate) const FILTER_KIND: &str = "filter_kind";
    pub(crate) const SOURCE_NAME: &str = "source_name";
    pub(crate) const FILTER_NAME: &str = "filter_name";
    pub(crate) const IS_ENABLED: &str = "is_enabled";
}
