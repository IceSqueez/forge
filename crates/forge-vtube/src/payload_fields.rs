pub(crate) mod model {
    pub(crate) const MODEL_ID: &str = "model_id";
    pub(crate) const MODEL_NAME: &str = "model_name";
}

pub(crate) mod hotkey {
    pub(crate) const HOTKEY_ID: &str = "hotkey_id";
    pub(crate) const HOTKEY_NAME: &str = "hotkey_name";
}

pub(crate) mod expression {
    pub(crate) const EXPRESSION_FILE: &str = "expression_file";
    pub(crate) const IS_ACTIVE: &str = "is_active";
}

pub(crate) mod tracking {
    pub(crate) const IS_LEFT_HAND_FOUND: &str = "is_left_hand_found";
    pub(crate) const IS_RIGHT_HAND_FOUND: &str = "is_right_hand_found";
}

pub(crate) mod item {
    pub(crate) const ITEM_INSTANCE_ID: &str = "item_instance_id";
    pub(crate) const ITEM_FILE_NAME: &str = "item_file_name";
}

pub(crate) mod connection {
    pub(crate) const IS_CONNECTED: &str = "is_connected";
    pub(crate) const ENDPOINT: &str = "endpoint";
    pub(crate) const REASON: &str = "reason";
    pub(crate) const DETAIL: &str = "detail";
}
