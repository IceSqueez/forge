use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::support as fields;

pub(crate) struct SupportResubscriberDescriptor;

impl TriggerKindDescriptor for SupportResubscriberDescriptor {
    fn id(&self) -> &str {
        "twitch.support.resubscriber"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Re-subscriber"
    }

    fn summary(&self) -> &str {
        "Fires on subscription renewals"
    }

    fn search_text(&self) -> &str {
        "twitch resub resubscribe renewal subscription months streak"
    }

    fn icon_name(&self) -> &str {
        "repeat"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.channel.subscription.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user_login = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let tier = event
            .payload
            .get(fields::TIER)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let cumulative_months = event
            .payload
            .get(fields::CUMULATIVE_MONTHS)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let streak_months = event
            .payload
            .get(fields::STREAK_MONTHS)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let message = event
            .payload
            .get(fields::MESSAGE)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("sub_tier".to_owned(), Variant::String(tier))
            .set(
                "sub_cumulative_months".to_owned(),
                Variant::Int(cumulative_months),
            )
            .set("sub_streak_months".to_owned(), Variant::Int(streak_months))
            .set("sub_message".to_owned(), Variant::String(message))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "User ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "sub_tier".to_owned(),
                        kind: VariantKind::String,
                        label: "Subscription tier".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "sub_cumulative_months".to_owned(),
                        kind: VariantKind::Int,
                        label: "Cumulative months".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 24 }),
                    },
                    DeclaredVariable {
                        name: "sub_streak_months".to_owned(),
                        kind: VariantKind::Int,
                        label: "Streak months".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 24 }),
                    },
                    DeclaredVariable {
                        name: "sub_message".to_owned(),
                        kind: VariantKind::String,
                        label: "Resub message".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn resub_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscription.message",
            serde_json::json!({
                "user": { "id": "222", "login": "loyalfan", "display_name": "LoyalFan" },
                "tier": "1000",
                "cumulative_months": 12,
                "streak_months": 6,
                "message": "Love this channel!",
                "share_streak": true
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(
            SupportResubscriberDescriptor.matches_trigger(&TriggerConfig::new(), &resub_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_resub_fields() {
        let stack = SupportResubscriberDescriptor.build_arg_stack(&resub_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("loyalfan".to_owned()))
        );
        assert_eq!(stack.get("sub_cumulative_months"), Some(&Variant::Int(12)));
        assert_eq!(stack.get("sub_streak_months"), Some(&Variant::Int(6)));
        assert_eq!(
            stack.get("sub_message"),
            Some(&Variant::String("Love this channel!".to_owned()))
        );
    }
}
