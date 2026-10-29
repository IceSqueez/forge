use forge_registry::references::QUEUE_OPTIONS_KEY;
use forge_registry::{ConfigSpec, DynamicSelectField};
use forge_types::QueueId;

pub(super) const QUEUE: DynamicSelectField<QueueId> =
    DynamicSelectField::new("queue_id", "Queue", QUEUE_OPTIONS_KEY);

pub(super) const QUEUE_SPEC: ConfigSpec = ConfigSpec::new(&[&QUEUE]);
