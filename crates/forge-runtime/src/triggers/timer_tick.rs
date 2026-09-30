use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, SubActionConfigExt,
    TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    DeclaredVariable, SynthesisHint, TriggerConfig, TriggerInstanceId, Variant, VariantKind,
};
use serde_json::json;

pub const TIMER_TICK_KIND: &str = "timer.tick";

const INSTANCE_ID_FIELD: &str = "instance_id";
const INTERVAL_MINUTES_FIELD: &str = "interval_minutes";
const MESSAGE_COUNT_FIELD: &str = "message_count";

const INTERVAL_MINUTES_KEY: &str = "interval_minutes";
const ONLY_WHILE_LIVE_KEY: &str = "only_while_live";
const MIN_CHAT_MESSAGES_KEY: &str = "min_chat_messages";

const MIN_INTERVAL_MINUTES: i64 = 1;
const MAX_INTERVAL_MINUTES: i64 = 1440;
const DEFAULT_INTERVAL_MINUTES: i64 = 30;
const MAX_MIN_CHAT_MESSAGES: i64 = 1000;
const SYNTHESIZED_MESSAGE_COUNT_CEILING: i64 = 50;
const SECONDS_PER_MINUTE: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerSchedule {
    pub interval_minutes: u32,
    pub only_while_live: bool,
    pub min_chat_messages: u32,
}

impl TimerSchedule {
    pub fn from_config(config: &TriggerConfig) -> Self {
        let interval_minutes = config
            .int(INTERVAL_MINUTES_KEY)
            .unwrap_or(DEFAULT_INTERVAL_MINUTES)
            .clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES);
        let min_chat_messages = config
            .int(MIN_CHAT_MESSAGES_KEY)
            .unwrap_or(0)
            .clamp(0, MAX_MIN_CHAT_MESSAGES);
        Self {
            interval_minutes: u32::try_from(interval_minutes).unwrap_or(u32::MAX),
            only_while_live: config.bool(ONLY_WHILE_LIVE_KEY).unwrap_or(false),
            min_chat_messages: u32::try_from(min_chat_messages).unwrap_or(u32::MAX),
        }
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.interval_minutes) * SECONDS_PER_MINUTE)
    }

    pub fn tick_event(&self, instance: TriggerInstanceId, message_count: u32) -> Event {
        Event::new(
            EventSource::Timer,
            TIMER_TICK_KIND,
            json!({
                INSTANCE_ID_FIELD: instance,
                INTERVAL_MINUTES_FIELD: self.interval_minutes,
                MESSAGE_COUNT_FIELD: message_count,
            }),
        )
    }
}

pub struct TimerTickDescriptor;

impl TriggerKindDescriptor for TimerTickDescriptor {
    fn id(&self) -> &str {
        TIMER_TICK_KIND
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Timer
    }

    fn label(&self) -> &str {
        "Interval timer"
    }

    fn summary(&self) -> &str {
        "Fires every N minutes, optionally only while live and after enough chat activity"
    }

    fn search_text(&self) -> &str {
        "timer interval every minutes repeat schedule reminder timed live chat activity"
    }

    fn icon_name(&self) -> &str {
        "clock"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        config.insert(
            INTERVAL_MINUTES_KEY.to_owned(),
            Variant::Int(DEFAULT_INTERVAL_MINUTES),
        );
        config.insert(ONLY_WHILE_LIVE_KEY.to_owned(), Variant::Bool(false));
        config.insert(MIN_CHAT_MESSAGES_KEY.to_owned(), Variant::Int(0));
        config
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Integer {
                key: INTERVAL_MINUTES_KEY,
                label: "Interval (minutes)",
                min: MIN_INTERVAL_MINUTES,
                max: MAX_INTERVAL_MINUTES,
            },
            FormField::Toggle {
                key: ONLY_WHILE_LIVE_KEY,
                label: "Only while live",
            },
            FormField::Integer {
                key: MIN_CHAT_MESSAGES_KEY,
                label: "Minimum chat messages since last fire",
                min: 0,
                max: MAX_MIN_CHAT_MESSAGES,
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let schedule = TimerSchedule::from_config(config);
        let mut display = format!("every {} min", schedule.interval_minutes);
        if schedule.only_while_live {
            display.push_str(", live only");
        }
        if schedule.min_chat_messages > 0 {
            display.push_str(&format!(", {}+ msgs", schedule.min_chat_messages));
        }
        display
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Timer),
            kind_prefix: Some(TIMER_TICK_KIND.to_owned()),
        }
    }

    fn targeted_instance_field(&self) -> Option<&'static str> {
        Some(INSTANCE_ID_FIELD)
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .event_specific(
                    DeclaredVariable {
                        name: "timer.interval_minutes".to_owned(),
                        kind: VariantKind::Int,
                        label: "Interval (minutes)".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: MIN_INTERVAL_MINUTES,
                            max: MAX_INTERVAL_MINUTES,
                        }),
                    },
                    |event| Variant::Int(payload_int(event, INTERVAL_MINUTES_FIELD)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "timer.message_count".to_owned(),
                        kind: VariantKind::Int,
                        label: "Chat messages since last fire".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: SYNTHESIZED_MESSAGE_COUNT_CEILING,
                        }),
                    },
                    |event| Variant::Int(payload_int(event, MESSAGE_COUNT_FIELD)),
                ),
        )
    }
}

fn payload_int(event: &Event, field: &str) -> i64 {
    event
        .payload
        .get(field)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
}

