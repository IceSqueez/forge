use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
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

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), resubscriber_identity)
                .message_text(resub_message)
                .count(CanonicalCount::SubCumulativeMonths, |event| {
                    payload_read::number(event, fields::CUMULATIVE_MONTHS)
                })
                .count(CanonicalCount::SubStreakMonths, |event| {
                    payload_read::number(event, fields::STREAK_MONTHS)
                })
                .sub_tier(|event| payload_read::text(event, fields::TIER))
                .legacy(
                    DeclaredVariable {
                        name: "sub_message".to_owned(),
                        kind: VariantKind::String,
                        label: "Resub message".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    CanonicalVariable::MessageText,
                    |event| Variant::String(resub_message(event)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn resubscriber_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

fn resub_message(event: &Event) -> String {
    payload_read::text(event, fields::MESSAGE)
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
