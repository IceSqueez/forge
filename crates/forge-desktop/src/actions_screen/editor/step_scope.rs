use super::*;
use crate::actions_screen::sub_action_modal::SubFormTarget;
use forge_registry::{ScopeLevel, ScopedVariable, variables_in_scope};

impl ScreenActionsView {
    pub(super) fn step_scope(&self, target: SubFormTarget) -> Vec<ScopedVariable> {
        let Some(detail) = &self.detail else {
            return Vec::new();
        };
        scope_at(
            detail,
            &self.nav_path,
            target,
            &self.sub_action_registry,
            &self.trigger_registry,
        )
    }
}

fn scope_at(
    detail: &ActionDetail,
    nav_path: &[nav::NavFrame],
    target: SubFormTarget,
    sub_registry: &SubActionRegistry,
    trigger_registry: &TriggerRegistry,
) -> Vec<ScopedVariable> {
    let root = &detail.action.sub_actions;
    let items: Vec<Vec<nav::ChainItem>> = (0..=nav_path.len())
        .map(|depth| nav::resolve_chain(root, &nav_path[..depth]))
        .collect();
    let edited = match target {
        SubFormTarget::Edit(index) => index,
        SubFormTarget::Add => items.last().map_or(0, Vec::len),
    };
    let chains: Vec<Vec<SubActionStep>> =
        items.iter().map(|chain| nav::chain_steps(chain)).collect();
    let levels: Vec<ScopeLevel<'_>> = items
        .iter()
        .zip(&chains)
        .enumerate()
        .map(|(depth, (raw, chain))| ScopeLevel {
            chain,
            step_index: nav::steps_before(
                raw,
                nav_path.get(depth).map_or(edited, |frame| frame.step_index),
            ),
        })
        .collect();
    variables_in_scope(
        &detail.action,
        &levels,
        &detail.trigger_instances,
        sub_registry,
        trigger_registry,
    )
}

#[cfg(test)]
mod tests {
    use forge_registry::{
        CASE_CHAIN_KEY, FormField, ProducedVariable, RegistryError, RunContext, SubActionCategory,
        SubActionIo, SubActionRunner,
    };
    use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

    use super::*;
    use crate::test_support::action_running;

    const TEXT: &str = "test.text";
    const BRANCH: &str = "test.branch";
    const SWITCH: &str = "test.switch";
    const INTO_KEY: &str = "into_var";
    const THEN_KEY: &str = "then";
    const CASES_KEY: &str = "cases";

    struct Fake {
        id: &'static str,
        fields: Vec<FormField>,
        io: SubActionIo,
    }

    #[async_trait::async_trait]
    impl SubActionRunner for Fake {
        fn id(&self) -> &str {
            self.id
        }

        fn category(&self) -> SubActionCategory {
            SubActionCategory::Logic
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
            "bolt"
        }

        fn default_config(&self) -> SubActionConfig {
            SubActionConfig::new()
        }

        fn config_fields(&self) -> Vec<FormField> {
            self.fields.clone()
        }

        fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
            Ok(())
        }

        fn scope_io(&self) -> SubActionIo {
            self.io.clone()
        }

        async fn execute(
            &self,
            _: &SubActionConfig,
            _: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            unreachable!("scope resolution never runs a step")
        }
    }

    fn sub_registry() -> SubActionRegistry {
        let mut reg = SubActionRegistry::new();
        for fake in [
            Fake {
                id: TEXT,
                fields: Vec::new(),
                io: SubActionIo {
                    produces: vec![ProducedVariable {
                        output_name_key: INTO_KEY.to_owned(),
                        kind: VariantKind::String,
                        label: "Output".to_owned(),
                    }],
                    ..SubActionIo::default()
                },
            },
            Fake {
                id: BRANCH,
                fields: vec![FormField::SubChain {
                    key: THEN_KEY,
                    label: THEN_KEY,
                }],
                io: SubActionIo::default(),
            },
            Fake {
                id: SWITCH,
                fields: vec![FormField::CaseList {
                    key: CASES_KEY,
                    label: CASES_KEY,
                }],
                io: SubActionIo::default(),
            },
        ] {
            reg.register(Box::new(fake)).unwrap();
        }
        reg
    }

    fn text(into: &str) -> SubActionStep {
        SubActionStep {
            config: SubActionConfig::from([(
                INTO_KEY.to_owned(),
                Variant::String(into.to_owned()),
            )]),
            ..crate::test_support::step(TEXT, true)
        }
    }

    fn holding(kind_id: &str, key: &str, value: Variant) -> SubActionStep {
        SubActionStep {
            config: SubActionConfig::from([(key.to_owned(), value)]),
            ..crate::test_support::step(kind_id, true)
        }
    }

    fn case(body: &[SubActionStep]) -> Variant {
        Variant::Object(SubActionConfig::from([
            ("match".to_owned(), Variant::String("x".to_owned())),
            (CASE_CHAIN_KEY.to_owned(), nav::encode_chain(body)),
        ]))
    }

    fn frame(step_index: usize, chain_key: &str, case_index: Option<usize>) -> nav::NavFrame {
        nav::NavFrame {
            step_index,
            chain_key: chain_key.to_owned(),
            case_index,
        }
    }

    fn detail(steps: Vec<SubActionStep>) -> ActionDetail {
        ActionDetail {
            action: action_running("scope", steps),
            trigger_instances: Vec::new(),
            sub_action_avg_ms: Vec::new(),
            last_step_outcomes: Vec::new(),
        }
    }

    fn names_at(
        steps: Vec<SubActionStep>,
        nav_path: &[nav::NavFrame],
        target: SubFormTarget,
    ) -> Vec<String> {
        scope_at(
            &detail(steps),
            nav_path,
            target,
            &sub_registry(),
            &TriggerRegistry::new(),
        )
        .into_iter()
        .map(|variable| variable.name)
        .collect()
    }

    fn branch_root() -> Vec<SubActionStep> {
        vec![
            text("before"),
            holding(
                BRANCH,
                THEN_KEY,
                nav::encode_chain(&[text("inner_a"), text("inner_b"), text("inner_c")]),
            ),
        ]
    }

    #[test]
    fn the_edited_step_index_applies_to_the_innermost_chain_of_the_nav_path() {
        let inside_branch = [frame(1, THEN_KEY, None)];
        for (nav_path, target, expected, case) in [
            (&[][..], SubFormTarget::Edit(1), vec!["before"], "root edit"),
            (
                &[][..],
                SubFormTarget::Add,
                vec!["before", "inner_a", "inner_b", "inner_c"],
                "root add",
            ),
            (
                &inside_branch[..],
                SubFormTarget::Edit(0),
                vec!["before"],
                "edit the first step inside a branch",
            ),
            (
                &inside_branch[..],
                SubFormTarget::Edit(1),
                vec!["before", "inner_a"],
                "edit inside a branch",
            ),
            (
                &inside_branch[..],
                SubFormTarget::Add,
                vec!["before", "inner_a", "inner_b", "inner_c"],
                "add inside a branch",
            ),
        ] {
            assert_eq!(
                names_at(branch_root(), nav_path, target),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn a_step_inside_a_case_sees_only_its_own_cases_earlier_steps() {
        let root = vec![
            text("before"),
            holding(
                SWITCH,
                CASES_KEY,
                Variant::Array(vec![
                    case(&[text("other_case")]),
                    case(&[text("this_a"), text("this_b")]),
                ]),
            ),
        ];

        let names = names_at(root, &[frame(1, CASES_KEY, Some(1))], SubFormTarget::Add);

        assert_eq!(names, vec!["before", "this_a", "this_b"]);
    }
}
