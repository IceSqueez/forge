use forge_registry::{PickedTextField, TextPicker, ToggleField};

use crate::vocabulary::{config_keys, picker_keys};

pub(crate) const SCENE: PickedTextField = PickedTextField {
    key: config_keys::SCENE,
    label: "Scene",
    picker: TextPicker::Options {
        options_key: picker_keys::SCENE_NAMES,
    },
    default: "",
    required: true,
};

pub(crate) const SOURCE: PickedTextField = PickedTextField {
    key: config_keys::SOURCE,
    label: "Source",
    picker: TextPicker::Options {
        options_key: picker_keys::SOURCE_NAMES,
    },
    default: "",
    required: true,
};

pub(crate) const ACTIVE: ToggleField = ToggleField {
    key: config_keys::ON,
    label: "Active",
    default: true,
};
