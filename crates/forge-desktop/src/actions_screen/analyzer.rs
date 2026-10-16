use forge_registry::SubActionRegistry;
use forge_types::SubActionStep;

pub(super) fn sends_order_sensitive_overlay(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    order_sensitive: &dyn Fn(&str) -> bool,
) -> bool {
    forge_runtime::overlay_send_targets(steps, registry)
        .iter()
        .filter_map(forge_runtime::OverlaySendTarget::overlay)
        .any(order_sensitive)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_events::Event;
    use forge_registry::{
        ActorBlock, ActorIdentity, EventFilter, FormField, KindPlatformContract, LoginSlot,
        SubActionRegistry, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
    };
    use forge_runtime::sub_action_runners::{
        CoreLogicIfThenElseRunner, CoreLogicLoopRunner, CoreLogicSwitchCaseRunner,
    };
    use forge_runtime::{ConditionGate, Config};
    use forge_types::{
        ActionId, ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PermissionRung,
        PlatformId, PlatformScope, QueueId, SubActionConfig, TriggerConfig, TriggerInstanceId,
        VariableSchema, VariantKind,
    };

    use super::*;

    struct StubTrigger {
        id: &'static str,
        variables: fn() -> TriggerVariables,
    }

    impl StubTrigger {
        fn declaring(id: &'static str, variables: fn() -> TriggerVariables) -> Self {
            StubTrigger { id, variables }
        }
    }

    impl TriggerKindDescriptor for StubTrigger {
        fn id(&self) -> &str {
            self.id
        }

        fn category(&self) -> TriggerCategory {
            TriggerCategory::Chat
        }

        fn label(&self) -> &str {
            self.id
        }

        fn summary(&self) -> &str {
            self.id
        }

        fn search_text(&self) -> &str {
            self.id
        }

        fn icon_name(&self) -> &str {
            "chat"
        }

        fn platform_contract(&self) -> KindPlatformContract {
            KindPlatformContract::PlatformSpecific(PlatformId::Kick)
        }

        fn default_config(&self) -> TriggerConfig {
            TriggerConfig::new()
        }

        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }

        fn condition_display(&self, _config: &TriggerConfig) -> String {
            String::new()
        }

        fn event_filter(&self) -> EventFilter {
            EventFilter {
                source: None,
                kind_prefix: None,
            }
        }

        fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
            true
        }

        fn variables(&self) -> Option<TriggerVariables> {
            Some((self.variables)())
        }

        fn output_schema(&self) -> Option<VariableSchema> {
            self.variables().map(|variables| variables.schema())
        }
    }

    const BRANCH_KIND: &str = "core.logic.if_then_else";
    const THEN_CHAIN_KEY: &str = "then_chain";
    const ORDERED: &str = "chat-wall";
    const UNORDERED: &str = "alert-box";
    const CONSUMER_KIND: &str = "chat.send";
    const MESSAGE_KEY: &str = "message";
    const LEGACY_TRIGGER: &str = "stub.chat_with_legacy";
    const CANONICAL_TRIGGER: &str = "stub.chat_canonical";
    const LEGACY_NAME: &str = "username";

    fn registry() -> SubActionRegistry {
        let gate = Arc::new(ConditionGate::new(&Config::default()));
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(CoreLogicIfThenElseRunner::new(Arc::clone(&gate))))
            .expect("the branching runner registers");
        reg.register(Box::new(CoreLogicLoopRunner::new(gate)))
            .expect("the loop runner registers");
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

    fn send(identity: &str) -> SubActionStep {
        step(
            forge_runtime::OVERLAY_SEND_KIND_ID,
            SubActionConfig::from([(
                forge_runtime::OVERLAY_TARGET_KEY.to_owned(),
                Variant::String(identity.to_owned()),
            )]),
            true,
        )
    }

    fn branch(body: Vec<SubActionStep>) -> SubActionStep {
        step(
            BRANCH_KIND,
            SubActionConfig::from([(THEN_CHAIN_KEY.to_owned(), nav::encode_chain(&body))]),
            true,
        )
    }

    #[test]
    fn only_a_resolved_order_sensitive_target_raises_the_warning() {
        for (steps, expected, label) in [
            (
                vec![send(ORDERED)],
                true,
                "a step sending to an overlay whose delivery order matters",
            ),
            (
                vec![send(UNORDERED)],
                false,
                "a step sending to an overlay whose delivery order does not matter",
            ),
            (
                vec![branch(vec![send(ORDERED)])],
                true,
                "an ordered overlay inside a branch this screen encoded itself",
            ),
            (
                vec![send("%overlay_target%")],
                false,
                "an overlay named by a variable no stored identity matches",
            ),
        ] {
            assert_eq!(
                sends_order_sensitive_overlay(&steps, &registry(), &|id| id == ORDERED),
                expected,
                "{label}"
            );
        }
    }

    fn chatter(_event: &forge_events::Event) -> ActorIdentity {
        ActorIdentity {
            id: "51938264".to_owned(),
            display_name: "StreamFan42".to_owned(),
            login: Some("streamfan42".to_owned()),
        }
    }

    fn principal_block() -> ActorBlock {
        ActorBlock {
            role: ActorRole::Principal,
            platform: PlatformId::Kick,
            login: LoginSlot::Declared,
        }
    }

    fn canonical_actor_only() -> TriggerVariables {
        TriggerVariables::new().actor(principal_block(), chatter)
    }

    fn canonical_actor_plus_legacy_username() -> TriggerVariables {
        canonical_actor_only().legacy(
            DeclaredVariable {
                name: LEGACY_NAME.to_owned(),
                kind: VariantKind::String,
                label: "Username".to_owned(),
                synthesis: None,
            },
            CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            |_| Variant::String("streamfan42".to_owned()),
        )
    }

    fn triggers(descriptors: Vec<StubTrigger>) -> TriggerRegistry {
        let mut reg = TriggerRegistry::new();
        for descriptor in descriptors {
            reg.register(Box::new(descriptor))
                .expect("each stub trigger registers under its own id");
        }
        reg
    }

    fn instance(kind_id: &str) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: kind_id.to_owned(),
            overrides: TriggerConfig::new(),
            enabled: true,
            user_defined: false,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::default(),
        }
    }

    fn action_consuming(templates: &[&str]) -> Action {
        Action {
            id: ActionId::new(),
            name: "stub action".to_owned(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: templates
                .iter()
                .map(|template| {
                    step(
                        CONSUMER_KIND,
                        SubActionConfig::from([(
                            MESSAGE_KEY.to_owned(),
                            Variant::String((*template).to_owned()),
                        )]),
                        true,
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn a_legacy_name_a_step_consumes_is_never_reported_as_unknown() {
        let health = analyze(
            &action_consuming(&["%username%", "%not_declared_anywhere%"]),
            &[instance(LEGACY_TRIGGER)],
            &[None, None],
            &registry(),
            &triggers(vec![StubTrigger::declaring(
                LEGACY_TRIGGER,
                canonical_actor_plus_legacy_username,
            )]),
        );

        assert_eq!(
            health,
            vec![
                StepHealth::default(),
                StepHealth {
                    findings: vec![Finding::UnknownVariable("not_declared_anywhere".to_owned())],
                },
            ],
        );
    }

    #[test]
    fn a_legacy_name_only_one_of_two_triggers_declares_is_reported_as_some_triggers_only() {
        let health = analyze(
            &action_consuming(&["%username%"]),
            &[instance(LEGACY_TRIGGER), instance(CANONICAL_TRIGGER)],
            &[None],
            &registry(),
            &triggers(vec![
                StubTrigger::declaring(LEGACY_TRIGGER, canonical_actor_plus_legacy_username),
                StubTrigger::declaring(CANONICAL_TRIGGER, canonical_actor_only),
            ]),
        );

        assert_eq!(
            health,
            vec![StepHealth {
                findings: vec![Finding::SomeTriggersOnly(LEGACY_NAME.to_owned())],
            }],
        );
    }
    const LOOP_KIND: &str = "core.logic.loop";
    const SWITCH_KIND: &str = "core.logic.switch_case";
    const FLOW_CONTROL_KINDS: [&str; 3] = [BREAK_LOOP_KIND_ID, CONTINUE_LOOP_KIND_ID, STOP_KIND_ID];

    fn plain(kind_id: &str) -> SubActionStep {
        step(kind_id, SubActionConfig::new(), true)
    }

    fn chat_send() -> SubActionStep {
        step(
            CONSUMER_KIND,
            SubActionConfig::from([(MESSAGE_KEY.to_owned(), Variant::String("hi".to_owned()))]),
            true,
        )
    }

    fn inside_loop(body: Vec<SubActionStep>) -> SubActionStep {
        step(
            LOOP_KIND,
            SubActionConfig::from([("body".to_owned(), nav::encode_chain(&body))]),
            true,
        )
    }

    fn inside_case(body: Vec<SubActionStep>) -> SubActionStep {
        let case = SubActionConfig::from([
            ("match".to_owned(), Variant::String("a".to_owned())),
            ("chain".to_owned(), nav::encode_chain(&body)),
        ]);
        step(
            SWITCH_KIND,
            SubActionConfig::from([(
                "cases".to_owned(),
                Variant::Array(vec![Variant::Object(case)]),
            )]),
            true,
        )
    }

    fn inside_default(body: Vec<SubActionStep>) -> SubActionStep {
        step(
            SWITCH_KIND,
            SubActionConfig::from([("default_chain".to_owned(), nav::encode_chain(&body))]),
            true,
        )
    }

    fn outside_any_loop(kind_id: &str) -> Vec<(SubActionStep, &'static str)> {
        vec![
            (plain(kind_id), "at the top level"),
            (branch(vec![plain(kind_id)]), "inside an if branch"),
            (inside_case(vec![plain(kind_id)]), "inside a switch case"),
            (
                inside_default(vec![plain(kind_id)]),
                "inside a switch default",
            ),
            (
                inside_case(vec![branch(vec![plain(kind_id)])]),
                "inside an if nested in a switch case",
            ),
        ]
    }

    fn inside_a_loop(kind_id: &str) -> Vec<(SubActionStep, &'static str)> {
        vec![
            (inside_loop(vec![plain(kind_id)]), "inside a loop body"),
            (
                inside_loop(vec![branch(vec![plain(kind_id)])]),
                "inside an if nested in a loop",
            ),
        ]
    }

    fn action_of(steps: Vec<SubActionStep>, concurrent: bool) -> Action {
        Action {
            concurrent,
            sub_actions: steps,
            ..action_consuming(&[])
        }
    }

    fn health_without_triggers(action: &Action) -> Vec<StepHealth> {
        analyze(
            action,
            &[],
            &vec![None; action.sub_actions.len()],
            &registry(),
            &triggers(Vec::new()),
        )
    }

    fn concurrent_warning() -> StepHealth {
        StepHealth {
            findings: vec![Finding::ControlFlowInConcurrentAction],
        }
    }

    #[test]
    fn flow_control_no_loop_absorbs_in_a_concurrent_action_flags_its_top_level_step() {
        let flagged = FLOW_CONTROL_KINDS
            .into_iter()
            .flat_map(|kind_id| {
                outside_any_loop(kind_id)
                    .into_iter()
                    .map(move |p| (kind_id, p))
            })
            .chain(
                inside_a_loop(STOP_KIND_ID)
                    .into_iter()
                    .map(|p| (STOP_KIND_ID, p)),
            );
        for (kind_id, (placed, place)) in flagged {
            let health = health_without_triggers(&action_of(vec![placed], true));
            assert_eq!(health, vec![concurrent_warning()], "{kind_id} {place}");
        }
    }

    #[test]
    fn flow_control_in_a_sequential_action_raises_no_finding() {
        for kind_id in FLOW_CONTROL_KINDS {
            for (placed, place) in outside_any_loop(kind_id)
                .into_iter()
                .chain(inside_a_loop(kind_id))
            {
                let health = health_without_triggers(&action_of(vec![placed], false));
                assert_eq!(health, vec![StepHealth::default()], "{kind_id} {place}");
            }
        }
    }

    #[test]
    fn a_loop_break_or_continue_in_a_concurrent_action_is_not_flagged() {
        for kind_id in [BREAK_LOOP_KIND_ID, CONTINUE_LOOP_KIND_ID] {
            for (placed, place) in inside_a_loop(kind_id) {
                let health = health_without_triggers(&action_of(vec![placed], true));
                assert_eq!(health, vec![StepHealth::default()], "{kind_id} {place}");
            }
        }
    }

    #[test]
    fn flow_control_in_a_concurrent_action_is_flagged_whether_or_not_triggers_seed_it() {
        let action = action_of(vec![plain(STOP_KIND_ID)], true);
        for (instances, seeded) in [
            (Vec::new(), "no triggers"),
            (vec![instance(CANONICAL_TRIGGER)], "a declaring trigger"),
        ] {
            let health = analyze(
                &action,
                &instances,
                &[None],
                &registry(),
                &triggers(vec![StubTrigger::declaring(
                    CANONICAL_TRIGGER,
                    canonical_actor_only,
                )]),
            );
            assert_eq!(health, vec![concurrent_warning()], "{seeded}");
        }
    }

    #[test]
    fn a_concurrent_action_flags_only_steps_that_hold_flow_control() {
        let health = health_without_triggers(&action_of(
            vec![
                chat_send(),
                branch(vec![chat_send()]),
                branch(vec![plain(STOP_KIND_ID)]),
            ],
            true,
        ));

        assert_eq!(
            health,
            vec![
                StepHealth::default(),
                StepHealth::default(),
                concurrent_warning()
            ]
        );
    }

    #[test]
    fn disabled_flow_control_in_a_concurrent_action_raises_no_finding() {
        let disabled_stop = step(STOP_KIND_ID, SubActionConfig::new(), false);
        let mut disabled_branch = branch(vec![plain(STOP_KIND_ID)]);
        disabled_branch.enabled = false;

        let health = health_without_triggers(&action_of(
            vec![
                disabled_stop.clone(),
                disabled_branch,
                branch(vec![disabled_stop]),
            ],
            true,
        ));

        assert_eq!(health, vec![StepHealth::default(); 3]);
    }

    #[test]
    fn flow_control_in_a_concurrent_action_warns_without_marking_the_step_broken() {
        let health = health_without_triggers(&action_of(vec![plain(STOP_KIND_ID)], true));

        assert_eq!(health[0].severity(), HealthSeverity::Yellow);
    }

    const OBS_SCENE: &str = "obs.scene.set";
    const TWITCH_SEND: &str = "twitch.chat.send";
    const CORE_LOG: &str = "core.log";

    fn obs() -> IntegrationId {
        IntegrationId::new("obs")
    }

    type Flag =
        fn(&[SubActionStep], &SubActionRegistry, &HashSet<IntegrationId>, &mut [StepHealth]);
    type Owner = fn(&StepHealth) -> Option<&IntegrationId>;

    const GATES: [(Flag, Owner, &str); 2] = [
        (
            flag_switched_off,
            StepHealth::disabled_integration,
            "switched off",
        ),
        (flag_failed, StepHealth::failed_integration, "failed"),
    ];

    fn flagged_owners(
        flag: Flag,
        owner: Owner,
        steps: &[SubActionStep],
        gated: &[IntegrationId],
    ) -> Vec<Option<IntegrationId>> {
        use crate::test_support::owned_sub_actions;

        let registry = owned_sub_actions(&[(OBS_SCENE, "obs"), (TWITCH_SEND, "twitch")]);
        let mut health = vec![StepHealth::default(); steps.len()];
        flag(
            steps,
            &registry,
            &gated.iter().cloned().collect(),
            &mut health,
        );
        health.iter().map(|step| owner(step).cloned()).collect()
    }

    #[test]
    fn only_an_enabled_step_reaching_a_gated_integration_is_flagged() {
        use crate::test_support::{container, step as owned};

        for (steps, expected, case) in [
            (
                vec![owned(CORE_LOG, true), owned(OBS_SCENE, true)],
                vec![None, Some(obs())],
                "an enabled step the gated integration owns",
            ),
            (vec![owned(OBS_SCENE, false)], vec![None], "a disabled step"),
            (
                vec![container(vec![owned(OBS_SCENE, true)], true)],
                vec![Some(obs())],
                "a step nested under an enabled container flags the top step",
            ),
            (
                vec![container(vec![owned(OBS_SCENE, true)], false)],
                vec![None],
                "a step nested under a disabled container",
            ),
            (
                vec![container(vec![owned(OBS_SCENE, false)], true)],
                vec![None],
                "a disabled step nested under an enabled container",
            ),
            (
                vec![owned(TWITCH_SEND, true)],
                vec![None],
                "a step owned by an integration that is not gated",
            ),
            (
                vec![
                    owned(CORE_LOG, true),
                    container(vec![owned(CORE_LOG, true)], true),
                ],
                vec![None, None],
                "steps no integration owns",
            ),
        ] {
            for (flag, owner, gate) in GATES {
                assert_eq!(
                    flagged_owners(flag, owner, &steps, &[obs()]),
                    expected,
                    "{gate}: {case}"
                );
            }
        }
    }

    #[test]
    fn each_gate_raises_its_own_finding_and_not_the_other() {
        use crate::test_support::step as owned;

        let steps = [owned(OBS_SCENE, true)];

        assert_eq!(
            [
                flagged_owners(
                    flag_switched_off,
                    StepHealth::failed_integration,
                    &steps,
                    &[obs()]
                ),
                flagged_owners(
                    flag_failed,
                    StepHealth::disabled_integration,
                    &steps,
                    &[obs()]
                ),
            ],
            [vec![None], vec![None]]
        );
    }

    #[test]
    fn a_step_hit_by_a_gated_integration_warns() {
        use crate::test_support::{owned_sub_actions, step as owned};

        let registry = owned_sub_actions(&[(OBS_SCENE, "obs")]);

        for (flag, _, gate) in GATES {
            let mut health = vec![StepHealth::default()];
            flag(
                &[owned(OBS_SCENE, true)],
                &registry,
                &HashSet::from([obs()]),
                &mut health,
            );
            assert_eq!(health[0].severity(), HealthSeverity::Yellow, "{gate}");
        }
    }
}
