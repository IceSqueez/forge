use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::hype_train as fields;

pub(crate) struct HypeTrainEndedDescriptor;

impl TriggerKindDescriptor for HypeTrainEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.support.hype_train_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Hype
    }

    fn label(&self) -> &str {
        "Hype Train ended"
    }

    fn summary(&self) -> &str {
        "Fires when a Hype Train concludes on the channel"
    }

    fn search_text(&self) -> &str {
        "twitch hype train end finish level cooldown"
    }

    fn icon_name(&self) -> &str {
        "train"
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
            kind_prefix: Some("channel.hype_train.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let hype = event.payload.get(fields::HYPE);

        let id = hype
            .and_then(|h| h.get(fields::HYPE_ID))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let level = hype
            .and_then(|h| h.get(fields::LEVEL))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let total = hype
            .and_then(|h| h.get(fields::TOTAL))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let ended_at = hype
            .and_then(|h| h.get(fields::ENDED_AT))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let cooldown_ends_at = hype
            .and_then(|h| h.get(fields::COOLDOWN_ENDS_AT))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        ArgStack::new()
            .set("hype.id".to_owned(), Variant::String(id))
            .set("hype.level".to_owned(), Variant::Int(level))
            .set("hype.total".to_owned(), Variant::Int(total))
            .set("hype.ended_at".to_owned(), Variant::String(ended_at))
            .set(
                "hype.cooldown_ends_at".to_owned(),
                Variant::String(cooldown_ends_at),
            )
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "hype.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Hype Train ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "hype.level".to_owned(),
                        kind: VariantKind::Int,
                        label: "Hype Train level".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 20 }),
                    },
                    DeclaredVariable {
                        name: "hype.total".to_owned(),
                        kind: VariantKind::Int,
                        label: "Total points".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                    DeclaredVariable {
                        name: "hype.ended_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ended at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "hype.cooldown_ends_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Cooldown ends at".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_event() -> Event {
        let payload = serde_json::json!({
            "hype": {
                "id": "ht-9",
                "level": 5,
                "total": 9001,
                "ended_at": "2026-06-13T18:10:00Z",
                "cooldown_ends_at": "2026-06-13T19:10:00Z",
            }
        });
        Event::new(EventSource::Twitch, "channel.hype_train.end", payload)
    }

    #[test]
    fn event_filter_targets_hype_train_end_on_twitch() {
        let filter = HypeTrainEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.hype_train.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_all_vars_with_int_level_and_total() {
        let stack = HypeTrainEndedDescriptor.build_arg_stack(&end_event());
        assert_eq!(
            stack.get("hype.id"),
            Some(&Variant::String("ht-9".to_owned()))
        );
        assert_eq!(stack.get("hype.level"), Some(&Variant::Int(5)));
        assert_eq!(stack.get("hype.total"), Some(&Variant::Int(9001)));
        assert_eq!(
            stack.get("hype.ended_at"),
            Some(&Variant::String("2026-06-13T18:10:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("hype.cooldown_ends_at"),
            Some(&Variant::String("2026-06-13T19:10:00Z".to_owned()))
        );
    }
}
