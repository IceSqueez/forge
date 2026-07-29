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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_registry::FormField;
    use forge_types::Variant;

    use super::*;
    use crate::preview::{PreviewComposition, PreviewShape, compose};

    const STUB_KEY: &str = "stub.key";

    struct StubKind {
        id: &'static str,
        label: &'static str,
    }

    impl OverlayKindDescriptor for StubKind {
        fn id(&self) -> &str {
            self.id
        }

        fn label(&self) -> &str {
            self.label
        }

        fn summary(&self) -> &str {
            ""
        }

        fn icon_name(&self) -> &str {
            ""
        }

        fn config_schema_version(&self) -> u32 {
            1
        }

        fn default_config(&self) -> OverlayConfig {
            OverlayConfig::from([(STUB_KEY.to_owned(), Variant::Int(7))])
        }

        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }

        fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
            compose(PreviewShape::Strip, config)
        }
    }

    fn stub(id: &'static str, label: &'static str) -> Box<dyn OverlayKindDescriptor> {
        Box::new(StubKind { id, label })
    }

    #[test]
    fn registering_a_duplicate_id_is_rejected_and_keeps_the_incumbent() {
        let mut registry = OverlayKindRegistry::new();
        registry.register(stub("dup", "First")).expect("first");

        let err = registry
            .register(stub("dup", "Second"))
            .expect_err("a second descriptor claiming a taken id must be rejected");

        assert!(
            matches!(&err, OverlayError::DuplicateKind(id) if id == "dup"),
            "unexpected error: {err:?}"
        );
        assert_eq!(
            registry.get("dup").expect("still registered").label(),
            "First",
            "a rejected duplicate must not replace the descriptor already registered"
        );
    }

    #[test]
    fn effective_config_resolves_by_kind_id_and_errors_for_a_kind_this_build_lacks() {
        let mut registry = OverlayKindRegistry::new();
        registry.register(stub("known", "Known")).expect("register");
        let stored = OverlayConfig::from([(STUB_KEY.to_owned(), Variant::Int(9))]);

        let effective = registry
            .effective_config("known", &stored)
            .expect("a registered kind resolves");
        assert_eq!(effective.get(STUB_KEY), Some(&Variant::Int(9)));

        let err = registry
            .effective_config("vendor.unshipped", &stored)
            .expect_err("an unregistered kind must not silently return the stored config");
        assert!(
            matches!(&err, OverlayError::UnknownKind(id) if id == "vendor.unshipped"),
            "unexpected error: {err:?}"
        );
    }
}
