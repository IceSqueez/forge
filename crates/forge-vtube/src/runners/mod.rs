mod expression_set;
mod hold_registry;
mod hotkey_trigger;
mod item_load;
mod item_move;
mod item_pin;
mod item_throw;
mod item_unload_all;
mod lookup_current_model;
mod lookup_expressions;
mod lookup_hotkeys;
mod lookup_items;
mod lookup_parameters;
mod model_load;
mod model_move;
mod model_set_physics;
mod model_tint;
mod numeric;
mod param_set;
mod params_reset;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

use hold_registry::HoldRegistry;

pub use expression_set::ExpressionSetRunner;
pub use hotkey_trigger::HotkeyTriggerRunner;
pub use item_load::ItemLoadRunner;
pub use item_move::ItemMoveRunner;
pub use item_pin::ItemPinRunner;
pub use item_throw::ItemThrowRunner;
pub use item_unload_all::ItemUnloadAllRunner;
pub use lookup_current_model::LookupCurrentModelRunner;
pub use lookup_expressions::LookupExpressionsRunner;
pub use lookup_hotkeys::LookupHotkeysRunner;
pub use lookup_items::LookupItemsRunner;
pub use lookup_parameters::LookupParametersRunner;
pub use model_load::ModelLoadRunner;
pub use model_move::ModelMoveRunner;
pub use model_set_physics::ModelSetPhysicsRunner;
pub use model_tint::ModelTintRunner;
pub use param_set::ParamSetRunner;
pub use params_reset::ParamsResetRunner;

use crate::sink::VTubeSink;

pub fn register_vtube_sub_actions(
    reg: &mut SubActionRegistry,
    sink: Arc<dyn VTubeSink>,
) -> Result<(), RegistryError> {
    let holds = Arc::new(HoldRegistry::new());
    reg.register(Box::new(HotkeyTriggerRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ExpressionSetRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ParamSetRunner::new(
        Arc::clone(&sink),
        Arc::clone(&holds),
    )))?;
    reg.register(Box::new(ModelLoadRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ParamsResetRunner::new(Arc::clone(&sink), holds)))?;
    reg.register(Box::new(ModelMoveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemMoveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupCurrentModelRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupHotkeysRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupExpressionsRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupParametersRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupItemsRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemPinRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemLoadRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemUnloadAllRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemThrowRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ModelTintRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ModelSetPhysicsRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;

    use forge_platform_core::QuickActions;
    use forge_registry::FormField;

    use super::*;
    use crate::client::VTubeClient;
    use crate::runners::test_support::{MockSink, RecordingSink, make_ctx};
    use forge_registry::runner::SubActionConfig;
    use forge_types::{ArgStack, SubActionOutcome, Variant};

    fn registry() -> SubActionRegistry {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new())).unwrap();
        reg
    }

    fn field_key(field: &FormField) -> &'static str {
        match field {
            FormField::Text { key, .. }
            | FormField::TextArea { key, .. }
            | FormField::Code { key, .. }
            | FormField::Integer { key, .. }
            | FormField::Toggle { key, .. }
            | FormField::FilePicker { key, .. }
            | FormField::DateTime { key, .. }
            | FormField::Select { key, .. }
            | FormField::DynamicSelect { key, .. }
            | FormField::DependentSelect { key, .. }
            | FormField::Optional { key, .. }
            | FormField::SubChain { key, .. }
            | FormField::CaseList { key, .. }
            | FormField::Slider { key, .. }
            | FormField::Swatch { key, .. } => key,
        }
    }

    #[test]
    fn all_expected_runner_ids_are_present() {
        let reg = registry();
        for id in &[
            "vtube.hotkey.trigger",
            "vtube.expression.set",
            "vtube.param.set",
            "vtube.model.load",
            "vtube.params.reset",
            "vtube.model.move",
            "vtube.model.tint",
            "vtube.model.set_physics",
            "vtube.item.move",
            "vtube.item.pin",
            "vtube.item.load",
            "vtube.item.throw",
            "vtube.item.unload_all",
            "vtube.lookup.current_model",
            "vtube.lookup.hotkeys",
            "vtube.lookup.expressions",
            "vtube.lookup.parameters",
            "vtube.lookup.items",
        ] {
            assert!(reg.get(id).is_some(), "missing runner: {id}");
        }
    }

    // Why: a quick action naming an unregistered runner, or presetting a key the runner never
    // reads, silently does nothing when the user clicks it - no error, no log, no effect.
    #[test]
    fn every_quick_action_targets_a_registered_runner_that_reads_its_keys() {
        let reg = registry();

        for action in VTubeClient::new_for_test("ws://127.0.0.1:8001/").actions() {
            let kind_id = &action.subaction_template.kind_id;
            let runner = reg.get(kind_id).unwrap_or_else(|| {
                panic!(
                    "quick action '{}' targets unknown runner '{kind_id}'",
                    action.label
                )
            });

            let mut read: BTreeSet<String> = runner.default_config().into_keys().collect();
            read.extend(
                runner
                    .config_fields()
                    .iter()
                    .map(|f| field_key(f).to_owned()),
            );

            let written = action
                .subaction_template
                .config
                .keys()
                .cloned()
                .chain(action.fields.iter().map(|f| f.key.clone()));

            for key in written {
                assert!(
                    read.contains(&key),
                    "quick action '{}' sets '{key}', which runner '{kind_id}' never reads",
                    action.label,
                );
            }
        }
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new())).unwrap();
        let result = register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new()));
        assert!(result.is_err());
    }

    enum WhenEmpty {
        Fails,
        SkipsTheCall,
        Sends(Option<f64>),
    }

    struct NumericField {
        kind_id: &'static str,
        base: &'static [(&'static str, &'static str)],
        key: &'static str,
        literal: (&'static str, f64),
        method: &'static str,
        arg: usize,
        when_empty: WhenEmpty,
    }

    const NUMERIC_FIELDS: [NumericField; 10] = [
        NumericField {
            kind_id: "vtube.param.set",
            base: &[("param_id", "MouthSmile")],
            key: "value",
            literal: ("0.5", 0.5),
            method: "set_param",
            arg: 0,
            when_empty: WhenEmpty::Fails,
        },
        NumericField {
            kind_id: "vtube.model.move",
            base: &[],
            key: "rotation",
            literal: ("0.5", 0.5),
            method: "move_model",
            arg: 2,
            when_empty: WhenEmpty::SkipsTheCall,
        },
        NumericField {
            kind_id: "vtube.item.move",
            base: &[("item_instance_id", "inst-1")],
            key: "size",
            literal: ("0.5", 0.5),
            method: "move_item",
            arg: 2,
            when_empty: WhenEmpty::SkipsTheCall,
        },
        NumericField {
            kind_id: "vtube.item.load",
            base: &[("file_name", "crown.png")],
            key: "rotation",
            literal: ("0.5", 0.5),
            method: "load_item",
            arg: 3,
            when_empty: WhenEmpty::Sends(None),
        },
        NumericField {
            kind_id: "vtube.item.load",
            base: &[("file_name", "crown.png")],
            key: "order",
            literal: ("3", 3.0),
            method: "load_item",
            arg: 5,
            when_empty: WhenEmpty::Sends(None),
        },
        NumericField {
            kind_id: "vtube.item.throw",
            base: &[("file_name", "crown.png")],
            key: "duration",
            literal: ("0.5", 0.5),
            method: "move_item",
            arg: 5,
            when_empty: WhenEmpty::Sends(Some(0.4)),
        },
        NumericField {
            kind_id: "vtube.item.pin",
            base: &[("item_instance_id", "inst-1")],
            key: "size",
            literal: ("0.5", 0.5),
            method: "pin_item",
            arg: 1,
            when_empty: WhenEmpty::Sends(Some(0.33)),
        },
        NumericField {
            kind_id: "vtube.model.set_physics",
            base: &[],
            key: "duration",
            literal: ("0.5", 0.5),
            method: "set_physics_override",
            arg: 1,
            when_empty: WhenEmpty::Sends(Some(2.0)),
        },
        NumericField {
            kind_id: "vtube.model.tint",
            base: &[],
            key: "mix_with_scene_lighting",
            literal: ("0.5", 0.5),
            method: "tint_all_art_meshes",
            arg: 4,
            when_empty: WhenEmpty::Sends(None),
        },
        NumericField {
            kind_id: "vtube.model.tint",
            base: &[],
            key: "color_r",
            literal: ("128", 128.0),
            method: "tint_all_art_meshes",
            arg: 0,
            when_empty: WhenEmpty::Sends(Some(255.0)),
        },
    ];

    enum Expect {
        Sent(Option<f64>),
        NotSent,
        Failed,
    }

    async fn run_with_text(
        field: &NumericField,
        text: &str,
    ) -> (SubActionOutcome, Option<Option<f64>>, bool) {
        let sink = Arc::new(RecordingSink::default());
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::clone(&sink) as Arc<dyn VTubeSink>).unwrap();
        let mut config: SubActionConfig = field
            .base
            .iter()
            .map(|(k, v)| ((*k).to_owned(), Variant::String((*v).to_owned())))
            .collect();
        config.insert(field.key.to_owned(), Variant::String(text.to_owned()));
        let stack = ArgStack::new().set("v".to_owned(), Variant::Int(2));

        let (telemetry, _) = reg
            .get(field.kind_id)
            .unwrap()
            .execute(&config, &make_ctx(&stack))
            .await;

        let sent = sink
            .numbers_sent_to(field.method)
            .map(|args| args[field.arg]);
        (telemetry.outcome, sent, sink.was_called())
    }

    // Why: the Actions editor stores every one of these fields as text, so a runner that only
    // reads `Variant::Float` silently sends its default instead of what the streamer typed.
    #[tokio::test]
    async fn numeric_fields_typed_as_text_reach_vts_as_numbers() {
        for field in &NUMERIC_FIELDS {
            let when_empty = match field.when_empty {
                WhenEmpty::Fails => Expect::Failed,
                WhenEmpty::SkipsTheCall => Expect::NotSent,
                WhenEmpty::Sends(v) => Expect::Sent(v),
            };
            for (text, expected) in [
                (field.literal.0, Expect::Sent(Some(field.literal.1))),
                ("%v%", Expect::Sent(Some(2.0))),
                ("abc", Expect::Failed),
                ("", when_empty),
            ] {
                let case = format!("{} {}={text:?}", field.kind_id, field.key);
                let (outcome, sent, called) = run_with_text(field, text).await;

                match expected {
                    Expect::Sent(value) => {
                        assert_eq!(outcome, SubActionOutcome::Success, "{case}");
                        assert_eq!(sent, Some(value), "{case}");
                    }
                    Expect::NotSent => {
                        assert_eq!(outcome, SubActionOutcome::Success, "{case}");
                        assert!(!called, "{case}: nothing to change, yet VTS was called");
                    }
                    Expect::Failed => {
                        assert!(
                            matches!(&outcome, SubActionOutcome::Failed(reason) if reason.contains(field.key)),
                            "{case}: expected a failure naming the key, got {outcome:?}"
                        );
                        assert!(!called, "{case}: a rejected value still reached VTS");
                    }
                }
            }
        }
    }
}
