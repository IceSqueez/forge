use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct HypeTrainProgressDescriptor;

impl TriggerKindDescriptor for HypeTrainProgressDescriptor {
    fn id(&self) -> &str {
        "twitch.support.hype_train_progress"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
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
            label: "Minimum level (1–5)",
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
}
