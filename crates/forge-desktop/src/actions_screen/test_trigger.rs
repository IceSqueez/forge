use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, QueueSchedulerHandle, SchedulerRequest};
use forge_types::{
    ActionId, ArgStack, DeclaredVariable, QueueId, SynthesisHint, VariableSchema, Variant,
    VariantKind,
};
use rand::RngExt;
use serde_json::json;
use time::OffsetDateTime;

const USERNAME_POOL: &[&str] = &[
    "test_user",
    "stream_fan_42",
    "lurker_99",
    "night_owl",
    "pixel_pal",
];
const DISPLAY_NAME_POOL: &[&str] = &[
    "TestUser",
    "StreamFan42",
    "NightOwl",
    "PixelPal",
    "CoolViewer",
];
const MESSAGE_POOL: &[&str] = &[
    "hey, great stream!",
    "this is a test message",
    "love the content",
    "what game is this?",
    "first time here, hi!",
];

pub(super) fn synthesize_args(schema: &VariableSchema) -> ArgStack {
    let mut stack = ArgStack::new();
    for var in &schema.variables {
        stack = stack.set(var.name.clone(), synthesize_value(var));
    }
    stack
}

fn synthesize_value(var: &DeclaredVariable) -> Variant {
    let mut rng = rand::rng();
    if let Some(hint) = &var.synthesis {
        return match hint {
            SynthesisHint::Username => Variant::String(pick(USERNAME_POOL).to_owned()),
            SynthesisHint::DisplayName => Variant::String(pick(DISPLAY_NAME_POOL).to_owned()),
            SynthesisHint::Message => Variant::String(pick(MESSAGE_POOL).to_owned()),
            SynthesisHint::BoundedInt { min, max } => {
                let (lo, hi) = if min <= max {
                    (*min, *max)
                } else {
                    (*max, *min)
                };
                Variant::Int(rng.random_range(lo..=hi))
            }
        };
    }
    match var.kind {
        VariantKind::String => Variant::String(sample_token(var)),
        VariantKind::Int => Variant::Int(rng.random_range(1..=100)),
        VariantKind::Float => Variant::Float(f64::from(rng.random_range(1..=9_999)) / 100.0),
        VariantKind::Bool => Variant::Bool(rng.random_range(0..=1) == 1),
        VariantKind::Datetime => Variant::Datetime(OffsetDateTime::now_utc()),
        VariantKind::Array => Variant::Array(vec![
            Variant::String(sample_token(var)),
            Variant::String(pick(USERNAME_POOL).to_owned()),
        ]),
        VariantKind::Object => Variant::Object(BTreeMap::new()),
    }
}

fn pick(pool: &[&'static str]) -> &'static str {
    pool[rand::rng().random_range(0..pool.len())]
}

fn sample_token(var: &DeclaredVariable) -> String {
    let base = if var.label.is_empty() {
        &var.name
    } else {
        &var.label
    };
    let slug: String = base
        .split_whitespace()
        .next()
        .unwrap_or("sample")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if slug.is_empty() {
        "sample".to_owned()
    } else {
        format!("{slug}_sample")
    }
}

pub(super) async fn dispatch_test_run(
    scheduler: &QueueSchedulerHandle,
    bus: &EventBus,
    action_id: ActionId,
    queue_id: QueueId,
    bypass_pause: bool,
    trigger_kind: Option<String>,
    initial_args: ArgStack,
) -> Result<(), String> {
    let root = Event::new(
        EventSource::Core,
        "test.run",
        json!({ "action_id": action_id.to_string() }),
    );
    let trigger_event_id = root.id;
    bus.record(root);
    scheduler
        .dispatch(SchedulerRequest {
            queue_id,
            action_id,
            trigger_event_id,
            trigger_kind,
            initial_args,
            bypass_pause,
        })
        .await
        .map_err(|e| e.to_string())
}
