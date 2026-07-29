use std::collections::HashMap;

use crate::config::effective_overlay_config;
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::error::OverlayError;

#[derive(Default)]
pub struct OverlayKindRegistry {
    descriptors: HashMap<String, Box<dyn OverlayKindDescriptor>>,
}

impl OverlayKindRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, d: Box<dyn OverlayKindDescriptor>) -> Result<(), OverlayError> {
        let id = d.id().to_owned();
        if self.descriptors.contains_key(&id) {
            return Err(OverlayError::DuplicateKind(id));
        }
        self.descriptors.insert(id, d);
        Ok(())
    }

    pub fn get(&self, kind_id: &str) -> Option<&dyn OverlayKindDescriptor> {
        self.descriptors.get(kind_id).map(|b| b.as_ref())
    }

    pub fn all(&self) -> impl Iterator<Item = &dyn OverlayKindDescriptor> {
        self.descriptors.values().map(|b| b.as_ref())
    }

    /// Fails for a kind this build does not carry, so the caller can keep the record and mark it unavailable.
    pub fn effective_config(
        &self,
        kind_id: &str,
        stored: &OverlayConfig,
    ) -> Result<OverlayConfig, OverlayError> {
        self.get(kind_id)
            .map(|d| effective_overlay_config(d, stored))
            .ok_or_else(|| OverlayError::UnknownKind(kind_id.to_owned()))
    }
}
