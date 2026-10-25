use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionConfigExt, ValueBounds,
};
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

pub(super) fn validate(config: &SubActionConfig) -> Result<(), RegistryError> {
    match config.get(REASON_KEY) {
        None => Ok(()),
        Some(Variant::String(reason)) if reason.chars().count() <= REASON_MAX_CHARS => Ok(()),
        Some(Variant::String(_)) => Err(RegistryError::invalid_field(
            REASON_KEY,
            ConfigReason::LengthOutOfRange(ValueBounds::at_most(REASON_MAX_CHARS)),
        )),
        Some(_) => Err(RegistryError::invalid_field(
            REASON_KEY,
            ConfigReason::NotText,
        )),
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
            assert!(validate(&config).is_ok(), "case: {label}");
        }
    }

    #[test]
    fn validate_keys_an_over_long_or_non_text_reason_to_the_reason_field() {
        let too_long = ConfigReason::LengthOutOfRange(ValueBounds::at_most(REASON_MAX_CHARS));
        for (label, config, reason) in [
            ("one over limit", text(&"a".repeat(101)), too_long.clone()),
            ("multibyte one over limit", text(&"ї".repeat(101)), too_long),
            (
                "integer",
                reason_config(Variant::Int(7)),
                ConfigReason::NotText,
            ),
            (
                "bool",
                reason_config(Variant::Bool(true)),
                ConfigReason::NotText,
            ),
        ] {
            assert_eq!(
                validate(&config).unwrap_err().into_config_problem(),
                forge_registry::ConfigProblem::field(REASON_KEY, reason),
                "case: {label}"
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
