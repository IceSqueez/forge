use forge_registry::TRIGGER_INSTANCE_OPTIONS_KEY;
use forge_registry::{ConfigSpec, DynamicSelectField};
use forge_types::TriggerInstanceId;

pub(super) const TRIGGER_INSTANCE_ID_KEY: &str = "trigger_instance_id";

pub(super) const TRIGGER: DynamicSelectField<TriggerInstanceId> = DynamicSelectField::new(
    TRIGGER_INSTANCE_ID_KEY,
    "Trigger",
    TRIGGER_INSTANCE_OPTIONS_KEY,
);

pub(super) const TRIGGER_SPEC: ConfigSpec = ConfigSpec::new(&[&TRIGGER]);
