#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_registry::SubActionRegistry;
use forge_runtime::actions::ActionsService;
use forge_runtime::sub_action_runners::{
    CoreLogicIfThenElseRunner, CoreLogicSwitchCaseRunner, OverlaySendRunner,
};
use forge_runtime::{
    ConditionGate, Config, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlaySendTarget,
    OverlayServiceCell, feeds_overlay, overlay_send_targets,
};
use forge_storage::OverlayId;
use forge_storage::action::MockActionRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::queue::MockQueueRepo;
use forge_storage::soundboard::MockSoundboardClipsRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_types::{
    Action, ActionId, ExecutionMode, PermissionRung, PlatformScope, QueueId, SubActionConfig,
    SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};

const OVERLAY: &str = "goal-box";
const OTHER_OVERLAY: &str = "alert-box";
const BRANCH_KIND: &str = "core.logic.if_then_else";
const SWITCH_KIND: &str = "core.logic.switch_case";
const THEN_CHAIN_KEY: &str = "then_chain";
const ELSE_CHAIN_KEY: &str = "else_chain";
const CASES_KEY: &str = "cases";
const CASE_CHAIN_KEY: &str = "chain";

fn registry() -> SubActionRegistry {
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(OverlaySendRunner::new(OverlayServiceCell::new())))
        .expect("the overlay send runner registers");
    reg.register(Box::new(CoreLogicIfThenElseRunner::new(Arc::new(
        ConditionGate::new(&Config::default()),
    ))))
    .expect("the branching runner registers");
    reg.register(Box::new(CoreLogicSwitchCaseRunner))
        .expect("the switch runner registers");
    reg
}

fn step(kind_id: &str, config: SubActionConfig, enabled: bool) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config,
        enabled,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn send_value(target: Option<Variant>, enabled: bool) -> SubActionStep {
    let mut config = SubActionConfig::new();
    if let Some(target) = target {
        config.insert(OVERLAY_TARGET_KEY.to_owned(), target);
    }
    step(OVERLAY_SEND_KIND_ID, config, enabled)
}

fn send(identity: &str, enabled: bool) -> SubActionStep {
    send_value(Some(Variant::String(identity.to_owned())), enabled)
}

fn encode(steps: &[SubActionStep]) -> Variant {
    Variant::Array(
        steps
            .iter()
            .map(|step| {
                let mut obj = SubActionConfig::new();
                obj.insert("kind_id".to_owned(), Variant::String(step.kind_id.clone()));
                obj.insert("config".to_owned(), Variant::Object(step.config.clone()));
                obj.insert("enabled".to_owned(), Variant::Bool(step.enabled));
                Variant::Object(obj)
            })
            .collect(),
    )
}

fn branch(key: &str, body: Vec<SubActionStep>, enabled: bool) -> SubActionStep {
    let mut config = SubActionConfig::new();
    config.insert(key.to_owned(), encode(&body));
    step(BRANCH_KIND, config, enabled)
}

fn then_branch(body: Vec<SubActionStep>, enabled: bool) -> SubActionStep {
    branch(THEN_CHAIN_KEY, body, enabled)
}

fn switch(cases: Vec<Vec<SubActionStep>>, enabled: bool) -> SubActionStep {
    let encoded = cases
        .iter()
        .map(|body| {
            let mut case = SubActionConfig::new();
            case.insert("match".to_owned(), Variant::String("hit".to_owned()));
            case.insert(CASE_CHAIN_KEY.to_owned(), encode(body));
            Variant::Object(case)
        })
        .collect();
    let mut config = SubActionConfig::new();
    config.insert(CASES_KEY.to_owned(), Variant::Array(encoded));
    step(SWITCH_KIND, config, enabled)
}

fn overlay(identity: &str) -> OverlaySendTarget {
    OverlaySendTarget::Overlay(identity.to_owned())
}

#[test]
fn a_target_is_collected_through_every_nested_chain_shape() {
    for (steps, label) in [
        (vec![send(OVERLAY, true)], "a step at the top of the chain"),
        (
            vec![then_branch(vec![send(OVERLAY, true)], true)],
            "a step inside a then branch",
        ),
        (
            vec![branch(ELSE_CHAIN_KEY, vec![send(OVERLAY, true)], true)],
            "a step inside an else branch",
        ),
        (
            vec![then_branch(
                vec![then_branch(vec![send(OVERLAY, true)], true)],
                true,
            )],
            "a step two branches deep",
        ),
        (
            vec![switch(vec![vec![send(OVERLAY, true)]], true)],
            "a step inside a switch case chain",
        ),
        (
            vec![switch(
                vec![vec![], vec![then_branch(vec![send(OVERLAY, true)], true)]],
                true,
            )],
            "a step inside a branch inside the second switch case",
        ),
        (
            vec![then_branch(
                vec![switch(vec![vec![send(OVERLAY, true)]], true)],
                true,
            )],
            "a step inside a switch case inside a branch",
        ),
    ] {
        assert_eq!(
            overlay_send_targets(&steps, &registry()),
            vec![overlay(OVERLAY)],
            "{label}"
        );
    }
}

#[test]
fn a_disabled_step_prunes_itself_and_everything_nested_under_it() {
    for (steps, label) in [
        (vec![send(OVERLAY, false)], "a disabled send"),
        (
            vec![then_branch(vec![send(OVERLAY, true)], false)],
            "a live send inside a disabled branch",
        ),
        (
            vec![then_branch(vec![send(OVERLAY, false)], true)],
            "a disabled send inside a live branch",
        ),
        (
            vec![switch(vec![vec![send(OVERLAY, true)]], false)],
            "a live send inside a disabled switch",
        ),
        (
            vec![then_branch(
                vec![then_branch(vec![send(OVERLAY, true)], true)],
                false,
            )],
            "a live send two branches under a disabled outer branch",
        ),
    ] {
        assert_eq!(
            overlay_send_targets(&steps, &registry()),
            Vec::new(),
            "{label}"
        );
    }
}

#[test]
fn a_target_naming_a_variable_stays_unresolved_and_matches_no_overlay() {
    let steps = vec![send("%overlay%", true)];

    assert_eq!(
        overlay_send_targets(&steps, &registry()),
        vec![OverlaySendTarget::Unresolved("%overlay%".to_owned())],
    );
    assert!(!feeds_overlay(&steps, &registry(), "%overlay%"));
    assert!(!feeds_overlay(&steps, &registry(), "overlay"));
}

#[test]
fn a_percent_that_closes_no_variable_name_is_a_plain_identity() {
    for identity in ["100%", "%%", "%", "% %", "50%%", "goal%box", "%goal-box"] {
        assert_eq!(
            overlay_send_targets(&[send(identity, true)], &registry()),
            vec![overlay(identity)],
            "{identity:?}"
        );
    }
}

#[test]
fn a_blank_or_non_string_target_yields_no_entry() {
    for (target, label) in [
        (None, "a step saved before an overlay was picked"),
        (
            Some(Variant::String(String::new())),
            "a selection cleared back to nothing",
        ),
        (
            Some(Variant::String("   ".to_owned())),
            "a selection holding only spaces",
        ),
        (
            Some(Variant::String("\t\n".to_owned())),
            "a selection holding only a tab and a newline",
        ),
        (Some(Variant::Int(7)), "a numeric target"),
        (Some(Variant::Bool(true)), "a boolean target"),
        (Some(Variant::Array(Vec::new())), "an array target"),
    ] {
        assert_eq!(
            overlay_send_targets(&[send_value(target, true)], &registry()),
            Vec::new(),
            "{label}"
        );
    }
}

#[test]
fn a_target_is_trimmed_to_the_identity_the_runner_would_send_to() {
    let steps = vec![send("  goal-box \n", true)];

    assert_eq!(
        overlay_send_targets(&steps, &registry()),
        vec![overlay(OVERLAY)],
    );
    assert!(feeds_overlay(&steps, &registry(), OVERLAY));
}

#[test]
fn every_send_keeps_its_own_entry_in_depth_first_order() {
    let steps = vec![
        send(OVERLAY, true),
        then_branch(vec![send(OTHER_OVERLAY, true), send(OVERLAY, true)], true),
        send(OVERLAY, true),
    ];

    assert_eq!(
        overlay_send_targets(&steps, &registry()),
        vec![
            overlay(OVERLAY),
            overlay(OTHER_OVERLAY),
            overlay(OVERLAY),
            overlay(OVERLAY),
        ],
    );
}

#[test]
fn the_registry_is_consulted_only_to_descend_into_nested_chains() {
    let bare = SubActionRegistry::new();

    assert_eq!(
        overlay_send_targets(&[send(OVERLAY, true)], &bare),
        vec![overlay(OVERLAY)],
        "an unregistered send kind still yields its own target",
    );
    assert_eq!(
        overlay_send_targets(&[then_branch(vec![send(OVERLAY, true)], true)], &bare),
        Vec::new(),
        "an unregistered container declares no chains to walk",
    );
}

#[test]
fn feeds_overlay_matches_only_the_whole_stored_identity() {
    let steps = vec![send(OVERLAY, true)];

    for (query, expected) in [
        (OVERLAY, true),
        ("goal", false),
        ("goal-boxes", false),
        ("Goal-Box", false),
        ("", false),
    ] {
        assert_eq!(
            feeds_overlay(&steps, &registry(), query),
            expected,
            "{query:?}"
        );
    }
}

#[test]
fn the_overlay_send_literals_live_in_exactly_one_file_under_crates() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits under the workspace crates directory")
        .to_path_buf();
    let owner = crates_dir.join("forge-runtime/src/sub_action_runners/overlay_targets.rs");

    let mut sources = Vec::new();
    collect_rust_sources(&crates_dir, &mut sources);
    assert!(
        sources.contains(&owner),
        "the walk never reached the owning file",
    );

    for constant in [OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY] {
        let needle = format!("{0}{constant}{0}", '"');
        let holders: BTreeSet<&PathBuf> = sources
            .iter()
            .filter(|path| fs::read_to_string(path).is_ok_and(|source| source.contains(&needle)))
            .collect();
        assert_eq!(
            holders,
            BTreeSet::from([&owner]),
            "{constant} must be spelled out in one production file only",
        );
    }
}

fn collect_rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn action(name: &str, enabled: bool, sub_actions: Vec<SubActionStep>) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions,
    }
}

fn trigger(name: &str) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "stub.trigger".to_owned(),
        name: name.to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: false,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::default(),
    }
}

const DIRECT: &str = "feeds the overlay directly";
const NESTED: &str = "feeds the overlay from a branch";
const SHUT_OFF: &str = "feeds the overlay while switched off";

fn service_over(seed: Vec<(Action, Vec<TriggerInstance>)>) -> ActionsService {
    let actions: Vec<Action> = seed.iter().map(|(action, _)| action.clone()).collect();
    let triggers: HashMap<ActionId, Vec<TriggerInstance>> = seed
        .into_iter()
        .map(|(action, instances)| (action.id, instances))
        .collect();

    let mut action_repo = MockActionRepo::new();
    action_repo
        .expect_list()
        .returning(move || Ok(actions.clone()));

    let mut trigger_repo = MockTriggerInstanceRepo::new();
    trigger_repo
        .expect_list_for_action()
        .returning(move |id| Ok(triggers.get(&id).cloned().unwrap_or_default()));

    ActionsService::new(
        Arc::new(action_repo),
        Arc::new(MockQueueRepo::new()),
        Arc::new(MockHistoryRepo::new()),
        Arc::new(trigger_repo),
        Arc::new(MockSoundboardClipsRepo::new()),
    )
}

fn seeded_service() -> ActionsService {
    service_over(vec![
        (
            action(DIRECT, true, vec![send(OVERLAY, true)]),
            vec![trigger("a raid"), trigger("a follow")],
        ),
        (
            action(
                NESTED,
                true,
                vec![then_branch(vec![send(OVERLAY, true)], true)],
            ),
            vec![trigger("a cheer")],
        ),
        (
            action(SHUT_OFF, false, vec![send(OVERLAY, true)]),
            Vec::new(),
        ),
        (
            action(
                "sends to another overlay",
                true,
                vec![send(OTHER_OVERLAY, true)],
            ),
            vec![trigger("a sub")],
        ),
        (
            action(
                "names its overlay with a variable",
                true,
                vec![send("%overlay%", true)],
            ),
            vec![trigger("a chat message")],
        ),
        (
            action("has a switched-off send", true, vec![send(OVERLAY, false)]),
            Vec::new(),
        ),
    ])
}

#[tokio::test]
async fn overlay_feeds_lists_only_the_actions_whose_live_steps_name_the_overlay() {
    let feeds = seeded_service()
        .overlay_feeds(&OverlayId::new(OVERLAY), &registry())
        .await
        .expect("the seeded repositories answer");

    assert_eq!(
        feeds
            .iter()
            .map(|feed| feed.action_name.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([DIRECT, NESTED, SHUT_OFF]),
    );
}

#[tokio::test]
async fn overlay_feeds_carries_each_actions_triggers_and_switched_off_state() {
    let feeds = seeded_service()
        .overlay_feeds(&OverlayId::new(OVERLAY), &registry())
        .await
        .expect("the seeded repositories answer");

    let carried: BTreeMap<&str, (bool, Vec<&str>)> = feeds
        .iter()
        .map(|feed| {
            (
                feed.action_name.as_str(),
                (
                    feed.action_enabled,
                    feed.triggers
                        .iter()
                        .map(|instance| instance.name.as_str())
                        .collect(),
                ),
            )
        })
        .collect();

    assert_eq!(
        carried,
        BTreeMap::from([
            (DIRECT, (true, vec!["a raid", "a follow"])),
            (NESTED, (true, vec!["a cheer"])),
            (SHUT_OFF, (false, Vec::new())),
        ]),
    );
}
