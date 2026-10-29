use forge_registry::{TextEditor, TextField, ValueBounds};

use super::field_kinds::{CommaListField, WholeNumberField};
use crate::vocabulary::config_keys;
use crate::vocabulary::variables::references;

const MAX_REDEMPTIONS_PER_CALL: usize = 25;

pub(super) const USER_ID: WholeNumberField = WholeNumberField {
    key: config_keys::USER_ID,
    label: "Target User ID",
    placeholder: references::USER_ID,
    bounds: ValueBounds::UNBOUNDED,
};

pub(super) const REWARD_ID: TextField = TextField {
    key: config_keys::REWARD_ID,
    label: "Reward ID",
    editor: TextEditor::Line {
        placeholder: references::REWARD_ID,
    },
    default: "",
    required: true,
};

pub(super) const REDEMPTION_IDS: CommaListField = CommaListField {
    key: config_keys::REDEMPTION_IDS,
    label: "Redemption ID(s)",
    placeholder: references::REDEMPTION_IDS_HINT,
    default: references::REDEMPTION_ID,
    max_items: MAX_REDEMPTIONS_PER_CALL,
    required: true,
};
