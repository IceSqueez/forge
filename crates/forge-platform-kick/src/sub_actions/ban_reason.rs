use forge_registry::{ConfigReason, FieldLiteral, FormField, TypedField, ValueBounds};
use forge_types::Variant;

use crate::vocabulary::config_keys;

const REASON_MAX_CHARS: usize = 100;

pub(super) struct BanReasonField;

pub(super) const REASON: BanReasonField = BanReasonField;

fn moderator_reason(text: &str) -> Option<String> {
    let reason: String = text.trim().chars().take(REASON_MAX_CHARS).collect();
    (!reason.is_empty()).then_some(reason)
}

impl TypedField for BanReasonField {
    type Value = Option<String>;

    fn key(&self) -> &'static str {
        config_keys::REASON
    }

    fn form_field(&self) -> FormField {
        FormField::Text {
            key: config_keys::REASON,
            label: "Reason (optional, max 100 chars)",
            placeholder: "",
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Option<String>>, ConfigReason> {
        let Variant::String(text) = value else {
            return Err(ConfigReason::NotText);
        };
        if let Some(template) = FieldLiteral::of_text(text) {
            return Ok(template);
        }
        if text.chars().count() > REASON_MAX_CHARS {
            return Err(ConfigReason::LengthOutOfRange(ValueBounds::at_most(
                REASON_MAX_CHARS,
            )));
        }
        Ok(FieldLiteral::Value(moderator_reason(text)))
    }

    fn parse_text(&self, text: &str) -> Result<Option<String>, ConfigReason> {
        Ok(moderator_reason(text))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_registry::{ConfigProblem, Interpolation};
    use forge_types::{ArgStack, SubActionConfig};

    use super::*;

    fn reason_config(reason: Variant) -> SubActionConfig {
        BTreeMap::from([(config_keys::REASON.to_owned(), reason)])
    }

    fn text(reason: &str) -> SubActionConfig {
        reason_config(Variant::String(reason.to_owned()))
    }

    fn edit_time_check(config: &SubActionConfig) -> Result<(), ConfigProblem> {
        REASON.peek(config, Interpolation::Deferred).map(|_| ())
    }

    #[test]
    fn stored_reasons_up_to_one_hundred_characters_pass_the_edit_time_check() {
        for (label, config) in [
            ("absent", BTreeMap::new()),
            ("blank", text("")),
            ("at limit", text(&"a".repeat(100))),
            ("multibyte at limit", text(&"ї".repeat(100))),
            ("template", text("%reason%")),
        ] {
            assert_eq!(edit_time_check(&config), Ok(()), "case: {label}");
        }
    }

    #[test]
    fn an_over_long_or_non_text_stored_reason_is_rejected_on_the_reason_key() {
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
                edit_time_check(&config),
                Err(ConfigProblem::field(config_keys::REASON, reason)),
                "case: {label}"
            );
        }
    }

    #[test]
    fn read_interpolates_trims_and_drops_blank_reasons() {
        let stack = ArgStack::new()
            .set(
                "why".to_owned(),
                Variant::String("  spam links ".to_owned()),
            )
            .set("nothing".to_owned(), Variant::String("   ".to_owned()));

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
                REASON.read(&config, &stack).unwrap().as_deref(),
                expected,
                "config {config:?}"
            );
        }
    }

    #[test]
    fn a_template_that_resolves_past_the_limit_is_cut_to_one_hundred_characters() {
        let stack = ArgStack::new().set("why".to_owned(), Variant::String("ї".repeat(150)));

        let reason = REASON.read(&text("%why%"), &stack).unwrap();

        assert_eq!(reason, Some("ї".repeat(100)));
    }
}
