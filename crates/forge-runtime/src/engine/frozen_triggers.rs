use std::sync::{Arc, OnceLock};

use forge_registry::TriggerRegistry;
use forge_types::ArgStack;

#[derive(Clone, Default)]
pub(crate) struct FrozenTriggers(Arc<OnceLock<Arc<TriggerRegistry>>>);

impl FrozenTriggers {
    pub(crate) fn freeze(&self, registry: Arc<TriggerRegistry>) {
        if self.0.set(registry).is_err() {
            tracing::warn!("trigger registry frozen twice; keeping the first");
        }
    }

    pub(crate) fn with_chat_event(&self, kind_id: &str, args: ArgStack) -> ArgStack {
        match self.0.get() {
            Some(registry) => registry.with_chat_event(kind_id, args),
            None => args,
        }
    }
}
