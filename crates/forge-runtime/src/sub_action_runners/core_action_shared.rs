use forge_registry::ACTION_OPTIONS_KEY;
use forge_registry::{ConfigSpec, DynamicSelectField};
use forge_types::ActionId;

pub(super) const ACTION_ID_KEY: &str = "action_id";

pub(super) const ACTION: DynamicSelectField<ActionId> =
    DynamicSelectField::new(ACTION_ID_KEY, "Action", ACTION_OPTIONS_KEY);

pub(super) const ACTION_SPEC: ConfigSpec = ConfigSpec::new(&[&ACTION]);
