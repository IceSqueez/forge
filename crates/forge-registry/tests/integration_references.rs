#![allow(clippy::expect_used)]

mod fakes;

use std::collections::HashMap;

use forge_events::DONATION_RECEIVED_KIND;
use forge_registry::{
    FormField, IntegrationReferences, ReferencingAction, SubActionRegistry, TriggerRegistry,
    tally_references,
};
use forge_types::{
    Action, IntegrationId, SubActionConfig, SubActionStep, TriggerInstance, Variant,
};

use fakes::{FakeRunner, FakeTrigger, action, encode_chain, runner, step, trigger_of};

const TWITCH_SEND: &str = "twitch.chat.send";
const TWITCH_SHOUTOUT: &str = "twitch.shoutout";
const OBS_SCENE: &str = "obs.scene.set";
const CORE_LOG: &str = "core.log";
const TWITCH_CHAT: &str = "twitch.chat";
const OBS_SCENE_CHANGED: &str = "obs.scene.changed";
const CORE_TIMER: &str = "core.timer";
const NESTING_KIND: &str = "fake.container";
const NESTED_CHAIN_KEY: &str = "body";
const SERVICE_FILTER_KEY: &str = "provider";

fn sub_actions() -> SubActionRegistry {
    let mut registry = SubActionRegistry::new();
    registry
        .register(Box::new(FakeRunner {
            fields: vec![FormField::SubChain {
                key: NESTED_CHAIN_KEY,
                label: "Body",
            }],
            ..runner(NESTING_KIND)
        }))
        .expect("the container fake registers");
    for (kind, owner) in [
        (TWITCH_SEND, "twitch"),
        (TWITCH_SHOUTOUT, "twitch"),
        (OBS_SCENE, "obs"),
    ] {
        registry
            .owned_by(IntegrationId::from_static(owner))
            .register(Box::new(runner(kind)))
            .expect("every owned fake carries its own id");
    }
    registry
}

fn trigger_kinds() -> TriggerRegistry {
    let mut registry = TriggerRegistry::new();
    for (kind, owner) in [(TWITCH_CHAT, "twitch"), (OBS_SCENE_CHANGED, "obs")] {
        registry
            .owned_by(IntegrationId::from_static(owner))
            .register(Box::new(FakeTrigger {
                id: kind,
                variables: None,
            }))
            .expect("every owned fake carries its own id");
    }
    registry
}

fn no_unowned_candidates(_: &str) -> Vec<IntegrationId> {
    Vec::new()
}

fn donation_services(kind_id: &str) -> Vec<IntegrationId> {
    if kind_id == DONATION_RECEIVED_KIND {
        vec![donatello(), monobank()]
    } else {
        Vec::new()
    }
}

fn tally(
    actions: &[Action],
    triggers: &[TriggerInstance],
) -> HashMap<IntegrationId, IntegrationReferences> {
    tally_references(
        actions,
        triggers,
        &sub_actions(),
        &trigger_kinds(),
        &no_unowned_candidates,
    )
}

fn owned(kind_id: &str, enabled: bool) -> SubActionStep {
    step(kind_id, SubActionConfig::new(), enabled)
}

fn container(body: Vec<SubActionStep>, enabled: bool) -> SubActionStep {
    step(
        NESTING_KIND,
        SubActionConfig::from([(NESTED_CHAIN_KEY.to_owned(), encode_chain(&body))]),
        enabled,
    )
}

fn referencing(action: &Action) -> ReferencingAction {
    ReferencingAction {
        id: action.id,
        name: action.name.clone(),
    }
}

fn twitch() -> IntegrationId {
    IntegrationId::new("twitch")
}

fn obs() -> IntegrationId {
    IntegrationId::new("obs")
}

fn donatello() -> IntegrationId {
    IntegrationId::new("donatello")
}

fn monobank() -> IntegrationId {
    IntegrationId::new("monobank")
}

fn trigger_counts(
    tally: HashMap<IntegrationId, IntegrationReferences>,
) -> HashMap<IntegrationId, usize> {
    tally
        .into_iter()
        .map(|(id, references)| (id, references.triggers))
        .collect()
}

#[test]
fn a_step_buried_in_nested_chains_is_attributed_to_its_integration() {
    let raid = action(
        "Raid",
        vec![container(
            vec![container(vec![owned(TWITCH_SEND, true)], true)],
            true,
        )],
    );

    let tally = tally(std::slice::from_ref(&raid), &[]);

    assert_eq!(tally[&twitch()].actions, vec![referencing(&raid)]);
}

#[test]
fn a_disabled_step_and_anything_nested_under_it_reference_nothing() {
    for (steps, case) in [
        (vec![owned(TWITCH_SEND, false)], "a disabled top-level step"),
        (
            vec![container(vec![owned(TWITCH_SEND, true)], false)],
            "an enabled step inside a disabled container",
        ),
        (
            vec![container(vec![owned(TWITCH_SEND, false)], true)],
            "a disabled step inside an enabled container",
        ),
    ] {
        let tally = tally(&[action("Muted", steps)], &[]);

        assert!(!tally.contains_key(&twitch()), "{case}");
    }
}

#[test]
fn an_action_using_one_integration_in_many_steps_is_listed_once() {
    let welcome = action(
        "Welcome",
        vec![
            owned(TWITCH_SEND, true),
            owned(TWITCH_SHOUTOUT, true),
            container(vec![owned(TWITCH_SEND, true)], true),
        ],
    );

    let tally = tally(std::slice::from_ref(&welcome), &[]);

    assert_eq!(tally[&twitch()].actions, vec![referencing(&welcome)]);
}

#[test]
fn an_action_reaching_two_integrations_is_listed_under_each() {
    let go_live = action(
        "Go live",
        vec![
            owned(OBS_SCENE, true),
            container(vec![owned(TWITCH_SEND, true)], true),
        ],
    );

    let tally = tally(std::slice::from_ref(&go_live), &[]);

    for id in [twitch(), obs()] {
        assert_eq!(tally[&id].actions, vec![referencing(&go_live)], "{id}");
    }
}

#[test]
fn same_named_actions_on_one_integration_each_carry_their_own_id_in_order() {
    let first = action("Shoutout", vec![owned(TWITCH_SEND, true)]);
    let second = action("Shoutout", vec![owned(TWITCH_SHOUTOUT, true)]);

    let tally = tally(&[first.clone(), second.clone()], &[]);

    assert_eq!(
        tally[&twitch()].actions,
        vec![referencing(&first), referencing(&second)]
    );
}

#[test]
fn steps_no_integration_owns_reference_nothing() {
    let housekeeping = action(
        "Housekeeping",
        vec![
            owned(CORE_LOG, true),
            container(vec![owned(CORE_LOG, true)], true),
        ],
    );

    assert!(tally(&[housekeeping], &[]).is_empty());
}

#[test]
fn triggers_count_toward_the_integration_owning_their_kind() {
    let triggers = [
        trigger_of(TWITCH_CHAT),
        trigger_of(TWITCH_CHAT),
        trigger_of(OBS_SCENE_CHANGED),
        trigger_of(CORE_TIMER),
    ];

    assert_eq!(
        trigger_counts(tally(&[], &triggers)),
        HashMap::from([(twitch(), 2), (obs(), 1)])
    );
}

#[test]
fn the_reference_total_adds_actions_and_triggers() {
    let tally = tally(
        &[action("Clip", vec![owned(TWITCH_SEND, true)])],
        &[trigger_of(TWITCH_CHAT)],
    );

    assert_eq!(tally[&twitch()].total(), 2);
}

fn donation_trigger(filter: Option<&str>) -> TriggerInstance {
    let mut trigger = trigger_of(DONATION_RECEIVED_KIND);
    if let Some(service) = filter {
        trigger.overrides.insert(
            SERVICE_FILTER_KEY.to_owned(),
            Variant::String(service.to_owned()),
        );
    }
    trigger
}

#[test]
fn an_unowned_trigger_counts_toward_the_candidates_its_service_filter_admits() {
    let both = HashMap::from([(donatello(), 1), (monobank(), 1)]);

    let cases = [
        (None, both.clone(), "no filter means any candidate"),
        (
            Some(""),
            both.clone(),
            "an empty filter means any candidate",
        ),
        (Some("   "), both, "a blank filter means any candidate"),
        (
            Some("monobank"),
            HashMap::from([(monobank(), 1)]),
            "a filter names exactly one candidate",
        ),
        (
            Some(" donatello "),
            HashMap::from([(donatello(), 1)]),
            "the filter is read trimmed",
        ),
        (
            Some("streamlabs"),
            HashMap::new(),
            "a filter naming no candidate counts nowhere",
        ),
    ];

    for (filter, expected, case) in cases {
        let tally = tally_references(
            &[],
            &[donation_trigger(filter)],
            &sub_actions(),
            &trigger_kinds(),
            &donation_services,
        );
        assert_eq!(trigger_counts(tally), expected, "{case}");
    }
}

#[test]
fn an_owned_trigger_ignores_the_unowned_candidates() {
    let tally = tally_references(
        &[],
        &[trigger_of(TWITCH_CHAT)],
        &sub_actions(),
        &trigger_kinds(),
        &|_| vec![donatello()],
    );

    assert_eq!(trigger_counts(tally), HashMap::from([(twitch(), 1)]));
}
