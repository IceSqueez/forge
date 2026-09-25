use std::collections::HashMap;
use std::sync::Arc;

use forge_overlay::{ConfigSection, OverlayKindRegistry};
use forge_registry::{FormField, FormSchemaSource};
use forge_runtime::CONTENT_SCHEMA_KEY;

pub(super) struct OverlayContentSchema {
    kinds: Arc<OverlayKindRegistry>,
    kind_by_identity: HashMap<String, String>,
}

impl OverlayContentSchema {
    pub(super) fn new(kinds: Arc<OverlayKindRegistry>) -> Self {
        Self {
            kinds,
            kind_by_identity: HashMap::new(),
        }
    }

    pub(super) fn with_identities(&self, kind_by_identity: HashMap<String, String>) -> Self {
        Self {
            kinds: Arc::clone(&self.kinds),
            kind_by_identity,
        }
    }

    pub(super) fn is_order_sensitive(&self, identity: &str) -> bool {
        self.kind_by_identity
            .get(identity.trim())
            .and_then(|kind_id| self.kinds.get(kind_id))
            .is_some_and(|descriptor| descriptor.order_sensitive())
    }
}

impl FormSchemaSource for OverlayContentSchema {
    fn fields_for(&self, schema_key: &str, selector_value: &str) -> Vec<FormField> {
        if schema_key != CONTENT_SCHEMA_KEY {
            return Vec::new();
        }
        self.kind_by_identity
            .get(selector_value.trim())
            .and_then(|kind_id| self.kinds.get(kind_id))
            .map(|descriptor| {
                descriptor
                    .config_fields()
                    .into_iter()
                    .filter(|sectioned| sectioned.section == ConfigSection::Content)
                    .map(|sectioned| sectioned.field)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_overlay::register_builtin_kinds;

    use super::*;

    fn schema() -> OverlayContentSchema {
        let mut kinds = OverlayKindRegistry::new();
        register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");
        OverlayContentSchema::new(Arc::new(kinds))
    }

    #[test]
    fn only_a_kind_that_fills_its_own_content_is_withheld_from_the_send_target_picker() {
        let schema = schema();

        for (kind_id, sendable) in [
            ("overlay.alert", true),
            ("overlay.audio", false),
            ("overlay.chat", true),
            ("overlay.frame", true),
            ("overlay.goal", true),
            ("overlay.ticker", true),
            ("overlay.written.by.a.newer.build", true),
            ("", true),
        ] {
            assert_eq!(
                schema.is_sendable_kind(kind_id),
                sendable,
                "kind {kind_id:?} is offered to a step that cannot address it, or hidden from one \
                 that can"
            );
        }
    }
}
