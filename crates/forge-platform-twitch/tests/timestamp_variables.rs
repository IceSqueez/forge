#![allow(clippy::expect_used)]
use forge_events::{Event, EventSource};
use forge_platform_twitch::register_twitch_triggers;
use forge_registry::{
    KindPlatformContract, SynthesisSample, TriggerKindDescriptor, TriggerRegistry,
    declared_variables, synthesize_args,
};
use forge_types::{Variant, VariantKind};
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const MOMENT: &str = "2026-06-13T18:00:00Z";

const MOMENT_KEYS: &[&str] = &[
    "banned_at",
    "ends_at",
    "followed_at",
    "redeemed_at",
    "held_at",
    "started_at",
    "ended_at",
    "expires_at",
    "cooldown_ends_at",
    "locks_at",
    "locked_at",
];

const PAYLOAD_OBJECTS: &[&str] = &[
    "redemption",
    "automod",
    "ad_break",
    "poll",
    "prediction",
    "hype",
    "goal",
    "session",
    "stream",
];

fn twitch_triggers() -> TriggerRegistry {
    let mut reg = TriggerRegistry::new();
    register_twitch_triggers(&mut reg).expect("the twitch triggers register");
    reg
}

fn datetime_names(descriptor: &dyn TriggerKindDescriptor) -> Vec<String> {
    declared_variables(descriptor)
        .unwrap_or_default()
        .into_iter()
        .filter(|variable| variable.declared.kind == VariantKind::Datetime)
        .map(|variable| variable.declared.name)
        .collect()
}

fn every_moment_set_to(value: Value) -> Value {
    let moments: Map<String, Value> = MOMENT_KEYS
        .iter()
        .map(|key| ((*key).to_owned(), value.clone()))
        .collect();
    let mut payload = moments.clone();
    for object in PAYLOAD_OBJECTS {
        payload.insert((*object).to_owned(), Value::Object(moments.clone()));
    }
    Value::Object(payload)
}

fn event_for(descriptor: &dyn TriggerKindDescriptor, payload: Value) -> Event {
    let kind = descriptor.event_filter().kind_prefix.unwrap_or_default();
    Event::new(EventSource::Twitch, kind, payload)
}

#[test]
fn every_twitch_trigger_variable_named_for_a_moment_is_declared_as_a_datetime() {
    let reg = twitch_triggers();
    for descriptor in reg.all() {
        for variable in declared_variables(descriptor).unwrap_or_default() {
            if variable.declared.name.ends_with("_at") {
                assert_eq!(
                    variable.declared.kind,
                    VariantKind::Datetime,
                    "{} declares {} as {:?}",
                    descriptor.id(),
                    variable.declared.name,
                    variable.declared.kind,
                );
            }
        }
    }
}

#[test]
fn every_declared_datetime_variable_is_emitted_as_the_instant_in_the_payload() {
    let reg = twitch_triggers();
    let expected = Variant::Datetime(OffsetDateTime::parse(MOMENT, &Rfc3339).unwrap());
    let mut checked = 0;
    for descriptor in reg.all() {
        let stack =
            descriptor.build_arg_stack(&event_for(descriptor, every_moment_set_to(json!(MOMENT))));
        for name in datetime_names(descriptor) {
            assert_eq!(
                stack.get(&name),
                Some(&expected),
                "{} emits {name} differently from its Datetime declaration",
                descriptor.id(),
            );
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "no twitch trigger declares a Datetime variable"
    );
}

#[test]
fn every_emitted_datetime_value_is_declared_as_a_datetime() {
    let reg = twitch_triggers();
    for descriptor in reg.all() {
        let declared = datetime_names(descriptor);
        let stack =
            descriptor.build_arg_stack(&event_for(descriptor, every_moment_set_to(json!(MOMENT))));
        for (name, value) in stack.snapshot() {
            if matches!(value, Variant::Datetime(_)) {
                assert!(
                    declared.contains(&name),
                    "{} emits {name} as a Datetime without declaring it so",
                    descriptor.id(),
                );
            }
        }
    }
}

#[test]
fn a_missing_or_unreadable_moment_leaves_its_variable_off_the_stack() {
    let reg = twitch_triggers();
    for (payload, what) in [
        (json!({}), "no nested objects at all"),
        (every_moment_set_to(Value::Null), "null moments"),
        (every_moment_set_to(json!("")), "empty moments"),
        (every_moment_set_to(json!("not a time")), "garbage moments"),
    ] {
        for descriptor in reg.all() {
            let stack = descriptor.build_arg_stack(&event_for(descriptor, payload.clone()));
            for name in datetime_names(descriptor) {
                assert_eq!(
                    stack.get(&name),
                    None,
                    "{} kept {name} for {what}",
                    descriptor.id(),
                );
            }
        }
    }
}

#[test]
fn a_test_run_previews_every_datetime_variable_as_the_sample_clock() {
    let reg = twitch_triggers();
    let sample = SynthesisSample::stable(KindPlatformContract::PlatformSpecific(
        forge_types::PlatformId::Twitch,
    ));
    for descriptor in reg.all() {
        let Some(variables) = declared_variables(descriptor) else {
            continue;
        };
        let stack = synthesize_args(&variables, &sample);
        for name in datetime_names(descriptor) {
            assert_eq!(
                stack.get(&name),
                Some(&Variant::Datetime(OffsetDateTime::UNIX_EPOCH)),
                "{} previews {name} as something other than the sample clock",
                descriptor.id(),
            );
        }
    }
}
