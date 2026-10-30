use forge_registry::IdSuccession;

use super::sub_action_ids;

pub const RETIRED_SUB_ACTION_IDS: &[IdSuccession] = &[IdSuccession {
    current: sub_action_ids::RESET_TO_IDLE,
    retired: &["vtube.params.reset"],
}];
