pub(crate) mod model {
    pub(crate) const ID: &str = "vtube.model.id";
    pub(crate) const NAME: &str = "vtube.model.name";
    pub(crate) const LOADED: &str = "vtube.model.loaded";
}

pub(crate) mod hotkey {
    pub(crate) const ID: &str = "vtube.hotkey.id";
    pub(crate) const NAME: &str = "vtube.hotkey.name";
}

pub(crate) mod hotkeys {
    pub(crate) const IDS: &str = "vtube.hotkeys.ids";
    pub(crate) const NAMES: &str = "vtube.hotkeys.names";
    pub(crate) const COUNT: &str = "vtube.hotkeys.count";
}

pub(crate) mod expression {
    pub(crate) const FILE: &str = "vtube.expression.file";
    pub(crate) const IS_ACTIVE: &str = "vtube.expression.is_active";
}

pub(crate) mod expressions {
    pub(crate) const NAMES: &str = "vtube.expressions.names";
    pub(crate) const ACTIVE: &str = "vtube.expressions.active";
    pub(crate) const COUNT: &str = "vtube.expressions.count";
}

pub(crate) mod parameters {
    pub(crate) const NAMES: &str = "vtube.parameters.names";
    pub(crate) const COUNT: &str = "vtube.parameters.count";
}

pub(crate) mod tracking {
    pub(crate) const IS_LEFT_HAND_FOUND: &str = "vtube.tracking.is_left_hand_found";
    pub(crate) const IS_RIGHT_HAND_FOUND: &str = "vtube.tracking.is_right_hand_found";
}

pub(crate) mod item {
    pub(crate) const INSTANCE_ID: &str = "vtube.item.instance_id";
    pub(crate) const FILE_NAME: &str = "vtube.item.file_name";
}

pub(crate) mod items {
    pub(crate) const INSTANCE_IDS: &str = "vtube.items.instance_ids";
    pub(crate) const FILE_NAMES: &str = "vtube.items.file_names";
    pub(crate) const COUNT: &str = "vtube.items.count";
}
