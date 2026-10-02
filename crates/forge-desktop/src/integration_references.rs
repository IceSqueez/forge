use std::collections::HashMap;

use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_types::{Action, ActionId, IntegrationId, SubActionStep, TriggerInstance};

use crate::actions_screen::nested_chains;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencingAction {
    pub id: ActionId,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationReferences {
    pub actions: Vec<ReferencingAction>,
    pub triggers: usize,
}

impl IntegrationReferences {
    pub fn total(&self) -> usize {
        self.actions.len() + self.triggers
    }
}

pub fn tally_references(
    actions: &[Action],
    triggers: &[TriggerInstance],
    sub_actions: &SubActionRegistry,
    trigger_kinds: &TriggerRegistry,
) -> HashMap<IntegrationId, IntegrationReferences> {
    let mut tally: HashMap<IntegrationId, IntegrationReferences> = HashMap::new();
    for action in actions {
        let mut owners: Vec<IntegrationId> = Vec::new();
        collect_owners(&action.sub_actions, sub_actions, &mut owners);
        for owner in owners {
            tally
                .entry(owner)
                .or_default()
                .actions
                .push(ReferencingAction {
                    id: action.id,
                    name: action.name.clone(),
                });
        }
    }
    for trigger in triggers {
        if let Some(owner) = trigger_kinds.owning_integration(&trigger.kind_id) {
            tally.entry(owner.clone()).or_default().triggers += 1;
        }
    }
    tally
}

fn collect_owners(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    owners: &mut Vec<IntegrationId>,
) {
    for step in steps {
        if !step.enabled {
            continue;
        }
        if let Some(owner) = registry.owning_integration(&step.kind_id)
            && !owners.contains(owner)
        {
            owners.push(owner.clone());
        }
        for chain in nested_chains(step, registry) {
            collect_owners(&chain, registry, owners);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_support::{
        action_running, container, owned_sub_actions, owned_triggers, step, trigger_of,
    };

    const TWITCH_SEND: &str = "twitch.chat.send";
    const TWITCH_SHOUTOUT: &str = "twitch.shoutout";
    const OBS_SCENE: &str = "obs.scene.set";
    const CORE_LOG: &str = "core.log";
    const TWITCH_CHAT: &str = "twitch.chat";
    const OBS_SCENE_CHANGED: &str = "obs.scene.changed";
    const CORE_TIMER: &str = "core.timer";

    fn sub_actions() -> SubActionRegistry {
        owned_sub_actions(&[
            (TWITCH_SEND, "twitch"),
            (TWITCH_SHOUTOUT, "twitch"),
            (OBS_SCENE, "obs"),
        ])
    }

    fn trigger_kinds() -> TriggerRegistry {
        owned_triggers(&[(TWITCH_CHAT, "twitch"), (OBS_SCENE_CHANGED, "obs")])
    }

    fn tally_actions(actions: &[Action]) -> HashMap<IntegrationId, IntegrationReferences> {
        tally_references(actions, &[], &sub_actions(), &trigger_kinds())
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

    #[test]
    fn a_step_buried_in_nested_chains_is_attributed_to_its_integration() {
        let raid = action_running(
            "Raid",
            vec![container(
                vec![container(vec![step(TWITCH_SEND, true)], true)],
                true,
            )],
        );

        let tally = tally_actions(std::slice::from_ref(&raid));

        assert_eq!(tally[&twitch()].actions, vec![referencing(&raid)]);
    }

    #[test]
    fn a_disabled_step_and_anything_nested_under_it_reference_nothing() {
        for (steps, case) in [
            (vec![step(TWITCH_SEND, false)], "a disabled top-level step"),
            (
                vec![container(vec![step(TWITCH_SEND, true)], false)],
                "an enabled step inside a disabled container",
            ),
            (
                vec![container(vec![step(TWITCH_SEND, false)], true)],
                "a disabled step inside an enabled container",
            ),
        ] {
            let tally = tally_actions(&[action_running("Muted", steps)]);

            assert!(!tally.contains_key(&twitch()), "{case}");
        }
    }

    #[test]
    fn an_action_using_one_integration_in_many_steps_is_listed_once() {
        let action = action_running(
            "Welcome",
            vec![
                step(TWITCH_SEND, true),
                step(TWITCH_SHOUTOUT, true),
                container(vec![step(TWITCH_SEND, true)], true),
            ],
        );

        let tally = tally_actions(std::slice::from_ref(&action));

        assert_eq!(tally[&twitch()].actions, vec![referencing(&action)]);
    }

    #[test]
    fn an_action_reaching_two_integrations_is_listed_under_each() {
        let action = action_running(
            "Go live",
            vec![
                step(OBS_SCENE, true),
                container(vec![step(TWITCH_SEND, true)], true),
            ],
        );

        let tally = tally_actions(std::slice::from_ref(&action));

        for id in [twitch(), obs()] {
            assert_eq!(tally[&id].actions, vec![referencing(&action)], "{id}");
        }
    }

    #[test]
    fn same_named_actions_on_one_integration_each_carry_their_own_id_in_order() {
        let first = action_running("Shoutout", vec![step(TWITCH_SEND, true)]);
        let second = action_running("Shoutout", vec![step(TWITCH_SHOUTOUT, true)]);

        let tally = tally_actions(&[first.clone(), second.clone()]);

        assert_eq!(
            tally[&twitch()].actions,
            vec![referencing(&first), referencing(&second)]
        );
    }

    #[test]
    fn steps_no_integration_owns_reference_nothing() {
        let action = action_running(
            "Housekeeping",
            vec![
                step(CORE_LOG, true),
                container(vec![step(CORE_LOG, true)], true),
            ],
        );

        assert!(tally_actions(&[action]).is_empty());
    }

    #[test]
    fn triggers_count_toward_the_integration_owning_their_kind() {
        let triggers = [
            trigger_of(TWITCH_CHAT),
            trigger_of(TWITCH_CHAT),
            trigger_of(OBS_SCENE_CHANGED),
            trigger_of(CORE_TIMER),
        ];

        let tally = tally_references(&[], &triggers, &sub_actions(), &trigger_kinds());

        let counts: HashMap<IntegrationId, usize> = tally
            .into_iter()
            .map(|(id, references)| (id, references.triggers))
            .collect();
        assert_eq!(counts, HashMap::from([(twitch(), 2), (obs(), 1)]));
    }

    #[test]
    fn the_reference_total_adds_actions_and_triggers() {
        let tally = tally_references(
            &[action_running("Clip", vec![step(TWITCH_SEND, true)])],
            &[trigger_of(TWITCH_CHAT)],
            &sub_actions(),
            &trigger_kinds(),
        );

        assert_eq!(tally[&twitch()].total(), 2);
    }
}
