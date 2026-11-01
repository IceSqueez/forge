use super::spec::TwitchTriggerSpec;
use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, CanonicalVariable, FormField, SubActionConfigExt,
    TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, TriggerConfig, UserText, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::channel_points as fields;

fn default_config() -> TriggerConfig {
    let mut cfg = TriggerConfig::new();
    cfg.insert(
        "status_filter".to_owned(),
        Variant::String("any".to_owned()),
    );
    cfg
}

fn config_fields() -> Vec<FormField> {
    vec![FormField::Select {
        key: "status_filter",
        label: "Redemption status",
        options: &["any", "fulfilled", "canceled"],
    }]
}

fn condition_display(config: &TriggerConfig) -> UserText {
    match config.get("status_filter").and_then(Variant::as_str) {
        None | Some("any") => messages::CONDITION_ANY_STATUS.into(),
        Some(status) => messages::condition_status(status).into(),
    }
}

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    let filter = config.str("status_filter").unwrap_or("any");

    if filter == "any" {
        return true;
    }

    let event_status = event
        .payload
        .get(fields::REDEMPTION)
        .and_then(|r| r.get(fields::REDEMPTION_STATUS))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    event_status == filter
}

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), redeemer_identity)
            .message_text(redemption_user_input)
            .event_specific(
                DeclaredVariable {
                    name: "redemption.id".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        fields::REDEMPTION,
                        fields::REDEMPTION_ID,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "redemption.status".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        fields::REDEMPTION,
                        fields::REDEMPTION_STATUS,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "reward.id".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        fields::REWARD,
                        fields::REWARD_ID,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "reward.title".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        fields::REWARD,
                        fields::REWARD_TITLE,
                    ))
                },
            )
            .legacy("user_input", CanonicalVariable::MessageText),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_POINTS_REDEMPTION_UPDATED,
    category: TriggerCategory::ChannelPoints,
    label: "Channel point redemption updated",
    summary: "Fires when a channel point redemption is fulfilled or canceled",
    search_text: "twitch channel points redemption fulfilled canceled updated status",
    icon: "star",
    kind: event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_UPDATE,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    default_config,
    config_fields,
    condition_display,
    ..TwitchTriggerSpec::BASE
};

fn redeemer_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

fn redemption_user_input(event: &Event) -> String {
    payload_read::nested_text(event, fields::REDEMPTION, fields::USER_INPUT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn config_with_filter(filter: &str) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "status_filter".to_owned(),
            Variant::String(filter.to_owned()),
        );
        cfg
    }

    fn fulfilled_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.channel_points_redemption_update",
            serde_json::json!({
                "redemption": {
                    "id": "redemption-42",
                    "status": "fulfilled",
                    "user_input": "play my song",
                },
                "user": {
                    "id": "777",
                    "login": "viewer_one",
                    "display_name": "ViewerOne",
                },
                "reward": {
                    "id": "r1",
                    "title": "Song Request",
                    "cost": 500,
                },
            }),
        )
    }

    #[test]
    fn matches_trigger_gates_fulfilled_event_by_status_filter() {
        let event = fulfilled_event();
        let cases = [
            ("any fires", config_with_filter("any"), true),
            (
                "matching status fires",
                config_with_filter("fulfilled"),
                true,
            ),
            (
                "non-matching status suppressed",
                config_with_filter("canceled"),
                false,
            ),
            ("default config fires (any)", SPEC.default_config(), true),
        ];
        for (name, cfg, expected) in cases {
            assert_eq!(SPEC.matches_trigger(&cfg, &event), expected, "case: {name}");
        }
    }

    #[test]
    fn build_arg_stack_exposes_redemption_user_and_reward_vars() {
        let stack = SPEC.build_arg_stack(&fulfilled_event());
        assert_eq!(
            stack.get("redemption.id"),
            Some(&Variant::String("redemption-42".to_owned()))
        );
        assert_eq!(
            stack.get("redemption.status"),
            Some(&Variant::String("fulfilled".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("777".to_owned()))
        );
        assert_eq!(
            stack.get("user_input"),
            Some(&Variant::String("play my song".to_owned()))
        );
        assert_eq!(
            stack.get("reward.id"),
            Some(&Variant::String("r1".to_owned()))
        );
        assert_eq!(
            stack.get("reward.title"),
            Some(&Variant::String("Song Request".to_owned()))
        );
    }
}
