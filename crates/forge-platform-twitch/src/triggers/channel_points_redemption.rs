use super::spec::TwitchTriggerSpec;
use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, CanonicalVariable, FormField, SubActionConfigExt,
    TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, SynthesisHint, TriggerConfig, UserText, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::collection_keys::REWARD_OPTIONS_KEY;
use crate::vocabulary::fields::channel_points as fields;

fn default_config() -> TriggerConfig {
    let mut cfg = TriggerConfig::new();
    cfg.insert("reward_id".to_owned(), Variant::String(String::new()));
    cfg.insert("reward_title".to_owned(), Variant::String(String::new()));
    cfg
}

fn config_fields() -> Vec<FormField> {
    vec![
        FormField::DynamicSelect {
            key: "reward_id",
            label: "Reward ID (leave blank to match any reward)",
            options_key: REWARD_OPTIONS_KEY,
        },
        FormField::Text {
            key: "reward_title",
            label: "Reward title (used only when Reward ID is blank; leave blank to match any)",
            placeholder: "",
        },
    ]
}

fn condition_display(config: &TriggerConfig) -> UserText {
    let text_of = |key: &str| {
        config
            .get(key)
            .and_then(Variant::as_str)
            .filter(|text| !text.is_empty())
    };
    if let Some(reward_id) = text_of("reward_id") {
        return messages::condition_reward_id(reward_id).into();
    }
    if let Some(reward_title) = text_of("reward_title") {
        return messages::condition_reward(reward_title).into();
    }
    messages::CONDITION_ANY_REWARD.into()
}

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    let reward_id = config.str("reward_id").unwrap_or_default();

    if !reward_id.is_empty() {
        let event_reward_id = event
            .payload
            .get(fields::REWARD)
            .and_then(|r| r.get(fields::REWARD_ID))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        return event_reward_id == reward_id;
    }

    let reward_title = config.str("reward_title").unwrap_or_default();

    if !reward_title.is_empty() {
        let event_reward_title = event
            .payload
            .get(fields::REWARD)
            .and_then(|r| r.get(fields::REWARD_TITLE))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        return event_reward_title == reward_title;
    }

    true
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
            .event_specific_optional(
                DeclaredVariable {
                    name: "redeemed_at".to_owned(),
                    kind: VariantKind::Datetime,
                    synthesis: None,
                },
                |event| {
                    payload_read::nested_timestamp(event, fields::REDEMPTION, fields::REDEEMED_AT)
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
            .event_specific(
                DeclaredVariable {
                    name: "reward.cost".to_owned(),
                    kind: VariantKind::Int,
                    synthesis: Some(SynthesisHint::BoundedInt {
                        min: 0,
                        max: super::MAX_SYNTHESIZED_COUNT,
                    }),
                },
                |event| {
                    Variant::Int(payload_read::nested_number(
                        event,
                        fields::REWARD,
                        fields::REWARD_COST,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "reward.prompt".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        fields::REWARD,
                        fields::REWARD_PROMPT,
                    ))
                },
            )
            .legacy("user_input", CanonicalVariable::MessageText),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_POINTS_REDEMPTION,
    category: TriggerCategory::ChannelPoints,
    label: "Channel Point Redemption",
    summary: "Fires when a viewer redeems a channel point reward",
    search_text: "twitch channel points reward redemption redeem",
    icon: "star",
    kind: event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_ADD,
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
    use crate::timestamp::tests::instant;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn redemption_config(reward_id: &str, reward_title: &str) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "reward_id".to_owned(),
            Variant::String(reward_id.to_owned()),
        );
        cfg.insert(
            "reward_title".to_owned(),
            Variant::String(reward_title.to_owned()),
        );
        cfg
    }

    fn redemption_event() -> Event {
        let payload = serde_json::json!({
            "redemption": {
                "id": "redemption-42",
                "status": "unfulfilled",
                "user_input": "play my song",
                "redeemed_at": "2026-06-13T10:00:00Z",
            },
            "user": {
                "id": "777",
                "login": "viewer_one",
                "display_name": "ViewerOne",
            },
            "reward": {
                "id": "r1",
                "title": "Hydrate",
                "cost": 500,
                "prompt": "Make the streamer drink water",
            },
        });
        Event::new(
            EventSource::Twitch,
            "channel.channel_points_redemption",
            payload,
        )
    }

    #[test]
    fn matches_trigger_applies_reward_id_then_title_precedence() {
        let cases = [
            ("both blank matches any", "", "", true),
            ("reward_id hit", "r1", "", true),
            ("reward_id miss", "rX", "", false),
            ("reward_title hit when id blank", "", "Hydrate", true),
            ("reward_title miss when id blank", "", "Other", false),
            ("reward_id precedence over title", "r1", "Other", true),
        ];
        for (name, reward_id, reward_title, expected) in cases {
            let cfg = redemption_config(reward_id, reward_title);
            assert_eq!(
                SPEC.matches_trigger(&cfg, &redemption_event()),
                expected,
                "case: {name}"
            );
        }
    }

    #[test]
    fn a_redemption_publishes_the_redeemer_the_typed_input_and_the_reward_it_names() {
        let stack = SPEC.build_arg_stack(&redemption_event());
        for (name, value) in [
            ("user_id", "777"),
            ("user_name", "ViewerOne"),
            ("user_login", "viewer_one"),
            ("user_platform", "twitch"),
            ("message_text", "play my song"),
            ("redemption.id", "redemption-42"),
            ("redemption.status", "unfulfilled"),
            ("reward.id", "r1"),
            ("reward.title", "Hydrate"),
            ("reward.prompt", "Make the streamer drink water"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("redeemed_at"),
            Some(&instant("2026-06-13T10:00:00Z"))
        );
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(500)));
    }
}
