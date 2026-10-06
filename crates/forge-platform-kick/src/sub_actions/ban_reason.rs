use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionConfigExt};
use forge_types::Variant;

const REASON_KEY: &str = "reason";
const REASON_MAX_CHARS: usize = 100;

pub(super) fn default_entry() -> (String, Variant) {
    (REASON_KEY.to_owned(), Variant::String(String::new()))
}

pub(super) fn form_field() -> FormField {
    FormField::Text {
        key: REASON_KEY,
        label: "Reason (optional, max 100 chars)",
        placeholder: "",
    }
}

pub(super) fn validate(kind_id: &str, config: &SubActionConfig) -> Result<(), RegistryError> {
    match config.get(REASON_KEY) {
        None => Ok(()),
        Some(Variant::String(reason)) if reason.chars().count() <= REASON_MAX_CHARS => Ok(()),
        Some(Variant::String(_)) => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: '{REASON_KEY}' must not exceed {REASON_MAX_CHARS} characters"
        ))),
        Some(_) => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: '{REASON_KEY}' must be a string"
        ))),
    }
}

pub(super) fn resolve(config: &SubActionConfig, ctx: &RunContext<'_>) -> Option<String> {
    config
        .str(REASON_KEY)
        .map(|template| {
            ctx.arg_stack
                .interpolate(template)
                .trim()
                .chars()
                .take(REASON_MAX_CHARS)
                .collect::<String>()
        })
        .filter(|reason| !reason.is_empty())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_events::{Event, EventPublisher};
    use forge_types::{ArgStack, EventId};

    use super::*;

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn reason_config(reason: Variant) -> SubActionConfig {
        BTreeMap::from([(REASON_KEY.to_owned(), reason)])
    }

    fn text(reason: &str) -> SubActionConfig {
        reason_config(Variant::String(reason.to_owned()))
    }

    #[test]
    fn validate_accepts_reasons_up_to_one_hundred_characters() {
        for (label, config) in [
            ("absent", BTreeMap::new()),
            ("blank", text("")),
            ("at limit", text(&"a".repeat(100))),
            ("multibyte at limit", text(&"ї".repeat(100))),
            ("template", text("%reason%")),
        ] {
            assert!(
                validate("kick.moderation.ban", &config).is_ok(),
                "case: {label}"
            );
        }
    }

    #[test]
    fn validate_rejects_an_over_long_or_non_string_reason_naming_the_kind() {
        for (label, config) in [
            ("one over limit", text(&"a".repeat(101))),
            ("multibyte one over limit", text(&"ї".repeat(101))),
            ("integer", reason_config(Variant::Int(7))),
        ] {
            let error = validate("kick.moderation.timeout", &config).unwrap_err();
            assert!(
                matches!(&error, RegistryError::InvalidConfig(message)
                    if message.starts_with("kick.moderation.timeout:")),
                "case: {label}, got {error:?}"
            );
        }
    }

    #[test]
    fn resolve_interpolates_trims_and_drops_blank_reasons() {
        let stack = ArgStack::new()
            .set(
                "why".to_owned(),
                Variant::String("  spam links ".to_owned()),
            )
            .set("nothing".to_owned(), Variant::String("   ".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);

        for (config, expected) in [
            (text("%why%"), Some("spam links")),
            (text("rule 3: %why%"), Some("rule 3:   spam links")),
            (text("  plain  "), Some("plain")),
            (text("%nothing%"), None),
            (text("   "), None),
            (text(""), None),
            (BTreeMap::new(), None),
        ] {
            assert_eq!(
                resolve(&config, &ctx).as_deref(),
                expected,
                "config {config:?}"
            );
        }
    }
}
