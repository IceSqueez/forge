use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct HypeTrainProgressDescriptor;

impl TriggerKindDescriptor for HypeTrainProgressDescriptor {
    fn id(&self) -> &str {
        "twitch.support.hype_train_progress"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Hype
    }

    fn label(&self) -> &str {
        "Hype Train progress"
    }

    fn summary(&self) -> &str {
        "Fires on each Hype Train progress update at or above the configured level"
    }

    fn search_text(&self) -> &str {
        "twitch hype train progress level update"
    }

    fn icon_name(&self) -> &str {
        "train"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_level".to_owned(), Variant::Int(1));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: "min_level",
            label: "Minimum level (1-5)",
            min: 1,
            max: 5,
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let min = config
            .get("min_level")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(1);
        format!("level >= {}", min)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.hype_train.progress".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let min_level = config
            .get("min_level")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(1);

        let level = event
            .payload
            .get("hype")
            .and_then(|h| h.get("level"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        level >= min_level
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let hype = event.payload.get("hype");

        let id = hype
            .and_then(|h| h.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let level = hype
            .and_then(|h| h.get("level"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let goal = hype
            .and_then(|h| h.get("goal"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let progress = hype
            .and_then(|h| h.get("progress"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let total = hype
            .and_then(|h| h.get("total"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set("hype.id".to_owned(), Variant::String(id))
            .set("hype.level".to_owned(), Variant::Int(level))
            .set("hype.goal".to_owned(), Variant::Int(goal))
            .set("hype.progress".to_owned(), Variant::Int(progress))
            .set("hype.total".to_owned(), Variant::Int(total))
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
                        name: "hype.goal".to_owned(),
                        kind: VariantKind::Int,
                        label: "Level goal".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                    DeclaredVariable {
                        name: "hype.progress".to_owned(),
                        kind: VariantKind::Int,
                        label: "Current progress".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
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
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress_event(level: i64) -> Event {
        let payload = serde_json::json!({
            "hype": {
                "id": "ht-3",
                "level": level,
                "goal": 1000,
                "progress": 600,
                "total": 600,
            }
        });
        Event::new(EventSource::Twitch, "channel.hype_train.progress", payload)
    }

    fn config_with_min_level(min: i64) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_level".to_owned(), Variant::Int(min));
        cfg
    }

    #[test]
    fn event_filter_targets_hype_train_progress_on_twitch() {
        let filter = HypeTrainProgressDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.hype_train.progress")
        );
    }

    #[test]
    fn matches_trigger_fires_only_when_level_at_or_above_min_level() {
        // event level is fixed at 3; min_level filter is `level >= min_level`.
        let cases = [
            ("min below event level", config_with_min_level(1), true),
            (
                "min equals event level (boundary, >=)",
                config_with_min_level(3),
                true,
            ),
            ("min one above event level", config_with_min_level(4), false),
        ];
        for (name, cfg, expected) in cases {
            assert_eq!(
                HypeTrainProgressDescriptor.matches_trigger(&cfg, &progress_event(3)),
                expected,
                "case: {name}"
            );
        }
    }

    #[test]
    fn matches_trigger_uses_default_min_level_of_one_when_config_absent() {
        let cfg = HypeTrainProgressDescriptor.default_config();
        assert!(HypeTrainProgressDescriptor.matches_trigger(&cfg, &progress_event(3)));
    }

    #[test]
    fn build_arg_stack_maps_all_vars_with_int_progress_fields() {
        let stack = HypeTrainProgressDescriptor.build_arg_stack(&progress_event(3));
        assert_eq!(
            stack.get("hype.id"),
            Some(&Variant::String("ht-3".to_owned()))
        );
        assert_eq!(stack.get("hype.level"), Some(&Variant::Int(3)));
        assert_eq!(stack.get("hype.goal"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("hype.progress"), Some(&Variant::Int(600)));
        assert_eq!(stack.get("hype.total"), Some(&Variant::Int(600)));
    }
}
