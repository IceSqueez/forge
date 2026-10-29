use std::borrow::Cow;
use std::collections::HashMap;

use crate::config::{effective_overlay_config, validate_overlay_config};
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::error::OverlayError;
use crate::upgrade::upgrade_config;

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

    pub fn effective_config(
        &self,
        kind_id: &str,
        stored: &OverlayConfig,
    ) -> Result<OverlayConfig, OverlayError> {
        self.get(kind_id)
            .map(|d| effective_overlay_config(d, stored))
            .ok_or_else(|| OverlayError::UnknownKind(kind_id.to_owned()))
    }

    pub fn validate_stored(
        &self,
        kind_id: &str,
        config_schema_version: u32,
        stored: &OverlayConfig,
    ) -> Result<(), OverlayError> {
        let descriptor = self
            .get(kind_id)
            .ok_or_else(|| OverlayError::UnknownKind(kind_id.to_owned()))?;
        let current = upgrade_config(descriptor, config_schema_version, stored)
            .map_or(Cow::Borrowed(stored), Cow::Owned);
        validate_overlay_config(descriptor, &effective_overlay_config(descriptor, &current))
    }
}

#[cfg(test)]
mod tests {
    use crate::descriptor::{DeliveryDisposition, SectionedField};
    use forge_types::Variant;

    use super::*;
    use crate::assets::PageAssets;
    use crate::config::{MARGIN_MAX_PERCENT, MARGIN_MIN_PERCENT, MARGIN_TOP, POSITION};
    use crate::kinds::chat;
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

        fn delivery_disposition(&self) -> DeliveryDisposition {
            DeliveryDisposition::Transient
        }

        fn order_sensitive(&self) -> bool {
            false
        }

        fn config_schema_version(&self) -> u32 {
            1
        }

        fn default_config(&self) -> OverlayConfig {
            OverlayConfig::from([(STUB_KEY.to_owned(), Variant::Int(7))])
        }

        fn config_fields(&self) -> Vec<SectionedField> {
            Vec::new()
        }

        fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
            compose(PreviewShape::Strip, None, config)
        }
        fn page_assets(&self) -> PageAssets {
            PageAssets {
                markup: "",
                style: "",
                behavior: "",
            }
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

    fn builtin() -> OverlayKindRegistry {
        let mut registry = OverlayKindRegistry::new();
        crate::kinds::register_builtin_kinds(&mut registry).expect("builtin kinds register");
        registry
    }

    fn chat_config(entries: &[(&str, Variant)]) -> OverlayConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    #[test]
    fn validate_stored_refuses_a_look_this_build_lacks() {
        let err = builtin()
            .validate_stored("overlay.vendor_unshipped", 1, &OverlayConfig::new())
            .expect_err("an unregistered look cannot be validated");

        assert!(
            matches!(&err, OverlayError::UnknownKind(id) if id == "overlay.vendor_unshipped"),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn validate_stored_judges_the_config_after_upgrading_it_from_the_recorded_schema() {
        let registry = builtin();
        let current = chat::MOTION_SCHEMA_VERSION;
        let older = chat::BOX_CONTRACT_SCHEMA_VERSION - 1;
        let max = MARGIN_MAX_PERCENT;
        let cases: [(&str, u32, OverlayConfig, Result<(), &str>); 9] = [
            (
                "sparse config takes defaults",
                current,
                OverlayConfig::new(),
                Ok(()),
            ),
            (
                "margin at the upper bound",
                current,
                chat_config(&[(MARGIN_TOP, Variant::Int(max))]),
                Ok(()),
            ),
            (
                "margin at the lower bound",
                current,
                chat_config(&[(MARGIN_TOP, Variant::Int(MARGIN_MIN_PERCENT))]),
                Ok(()),
            ),
            (
                "older unknown position folded by the upgrade",
                older,
                chat_config(&[(POSITION, text("middle"))]),
                Ok(()),
            ),
            (
                "unknown position at the current schema",
                current,
                chat_config(&[(POSITION, text("middle"))]),
                Err("unknown choice"),
            ),
            (
                "margin one over the upper bound",
                current,
                chat_config(&[(MARGIN_TOP, Variant::Int(max + 1))]),
                Err("out of range"),
            ),
            (
                "margin one under the lower bound",
                current,
                chat_config(&[(MARGIN_TOP, Variant::Int(MARGIN_MIN_PERCENT - 1))]),
                Err("out of range"),
            ),
            (
                "older out-of-range margin the upgrade keeps",
                older,
                chat_config(&[(MARGIN_TOP, Variant::Int(max + 1))]),
                Err("out of range"),
            ),
            (
                "margin of the wrong type",
                current,
                chat_config(&[(MARGIN_TOP, text("wide"))]),
                Err("wrong type"),
            ),
        ];

        for (label, version, stored, expected) in cases {
            let verdict = registry
                .validate_stored(chat::KIND_ID, version, &stored)
                .map_err(|err| match err {
                    OverlayError::UnknownChoice { key, .. } if key == POSITION => "unknown choice",
                    OverlayError::OutOfRange { key, .. } if key == MARGIN_TOP => "out of range",
                    OverlayError::WrongType { key, .. } if key == MARGIN_TOP => "wrong type",
                    _ => "unexpected error",
                });
            assert_eq!(verdict, expected, "{label}");
        }
    }
}
