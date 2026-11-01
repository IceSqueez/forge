use forge_events::Event;
use forge_registry::{FormField, SubActionConfigExt};
use forge_types::{TriggerConfig, UserText};

use crate::vocabulary::fields::audio as fields;
use crate::vocabulary::{messages, picker_keys};

const SOURCE_NAME_KEY: &str = "source_name";

pub(super) fn config_fields() -> Vec<FormField> {
    vec![FormField::Optional {
        key: SOURCE_NAME_KEY,
        label: "Source name (leave empty to match any)",
        inner: Box::new(FormField::DynamicSelect {
            key: SOURCE_NAME_KEY,
            label: "Source name",
            options_key: picker_keys::AUDIO_INPUT_NAMES,
        }),
    }]
}

pub(super) fn condition_display(config: &TriggerConfig) -> UserText {
    match config.str_nonempty(SOURCE_NAME_KEY) {
        Some(source) => messages::condition_source(source).into(),
        None => messages::CONDITION_ANY_SOURCE.into(),
    }
}

pub(super) fn source_name_matches(config: &TriggerConfig, event: &Event) -> bool {
    match config.str_nonempty(SOURCE_NAME_KEY) {
        Some(source) => {
            event
                .payload
                .get(fields::SOURCE_NAME)
                .and_then(serde_json::Value::as_str)
                == Some(source)
        }
        None => true,
    }
}
