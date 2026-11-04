use std::collections::HashMap;
use std::sync::Arc;

use forge_overlay::{ContentFeed, OverlayKindRegistry, show_media_fields};
use forge_registry::{FormField, FormSchemaSource};
use forge_runtime::CONTENT_SCHEMA_KEY;

pub(crate) struct OverlayContentSchema {
    kinds: Arc<OverlayKindRegistry>,
    kind_by_identity: HashMap<String, String>,
}

impl OverlayContentSchema {
    pub(crate) fn new(kinds: Arc<OverlayKindRegistry>) -> Self {
        Self {
            kinds,
            kind_by_identity: HashMap::new(),
        }
    }

    pub(crate) fn with_identities(&self, kind_by_identity: HashMap<String, String>) -> Self {
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

    pub(super) fn takes_sends(&self, kind_id: &str) -> bool {
        self.kinds
            .get(kind_id)
            .is_none_or(|descriptor| descriptor.content_feed() == ContentFeed::Deliveries)
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
                let mut fields: Vec<FormField> = descriptor
                    .config_fields()
                    .into_iter()
                    .filter(|sectioned| sectioned.delivered)
                    .map(|sectioned| sectioned.field)
                    .collect();
                fields.extend(show_media_fields(descriptor));
                fields
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use forge_overlay::config::{
        AUTHOR, AUTHOR_COLOR, AVATAR, BADGES, EVENT, HEADLINE, LABEL, MESSAGE, MESSAGE_ID,
        PLACEHOLDER, PLATFORM, SPEECH, SUBLINE, TARGET, USER_ID, VALUE,
    };
    use forge_overlay::register_builtin_kinds;

    use super::*;

    fn schema() -> OverlayContentSchema {
        let mut kinds = OverlayKindRegistry::new();
        register_builtin_kinds(&mut kinds).expect("register the builtin overlay kinds");
        OverlayContentSchema::new(Arc::new(kinds))
    }

    #[test]
    fn a_slot_fed_look_takes_no_sends_while_a_delivery_fed_or_unknown_one_does() {
        let schema = schema();

        for (kind_id, expected) in [
            ("overlay.latest", false),
            ("overlay.alert", true),
            ("overlay.removed_in_a_later_build", true),
        ] {
            assert_eq!(schema.takes_sends(kind_id), expected, "{kind_id}");
        }
    }

    #[test]
    fn the_send_form_of_each_look_offers_exactly_the_fields_a_show_carries_in_their_order() {
        let looks = [
            ("overlay.alert", vec![HEADLINE, SUBLINE, SPEECH]),
            ("overlay.blank", vec![SPEECH]),
            (
                "overlay.chat",
                vec![
                    AUTHOR,
                    AUTHOR_COLOR,
                    BADGES,
                    MESSAGE,
                    PLATFORM,
                    MESSAGE_ID,
                    USER_ID,
                    EVENT,
                    AVATAR,
                    SPEECH,
                ],
            ),
            ("overlay.frame", vec![HEADLINE, SUBLINE, SPEECH]),
            ("overlay.goal", vec![LABEL, VALUE, TARGET, SPEECH]),
            (
                "overlay.latest",
                vec![LABEL, HEADLINE, SUBLINE, PLACEHOLDER],
            ),
            ("overlay.ticker", vec![HEADLINE, SUBLINE, SPEECH]),
        ];
        let schema = schema().with_identities(
            looks
                .iter()
                .map(|(kind_id, _)| ((*kind_id).to_owned(), (*kind_id).to_owned()))
                .collect(),
        );

        for (kind_id, expected) in looks {
            let offered: Vec<String> = schema
                .fields_for(CONTENT_SCHEMA_KEY, kind_id)
                .iter()
                .map(|field| field.key().to_owned())
                .collect();
            assert_eq!(offered, expected, "{kind_id}");
        }
    }
}
