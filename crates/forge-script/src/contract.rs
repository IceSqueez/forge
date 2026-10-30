use std::collections::BTreeMap;

use forge_types::{
    ArgStack, COMMAND_ARGS_VARIABLE, COMMAND_NAME_VARIABLE, COMMAND_TAIL_VARIABLE, ScriptContract,
    ScriptInput, Variant, VariantKind,
};

use crate::convert::variant_to_dynamic;

const COMMAND_SCRIPT_VARIABLE: &str = "command";
const COMMAND_MEMBERS: [(&str, &str); 3] = [
    ("name", COMMAND_NAME_VARIABLE),
    ("args", COMMAND_ARGS_VARIABLE),
    ("tail", COMMAND_TAIL_VARIABLE),
];
const ARGUMENTS_INPUT: &str = "args";

#[derive(Debug, thiserror::Error)]
pub enum InputMismatchError {
    #[error("missing input `{name}` in ArgStack")]
    Missing { name: String },

    #[error("input `{name}`: expected {expected:?}, got {got:?}")]
    TypeMismatch {
        name: String,
        expected: VariantKind,
        got: VariantKind,
    },
}

pub fn inert_annotation_lines(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, raw)| {
            let trimmed = raw.trim();
            trimmed.strip_prefix("//").is_some_and(|rest| {
                let directive = rest.trim_start_matches('/').trim_start();
                directive.starts_with("@input") || directive.starts_with("@return")
            })
        })
        .map(|(index, _)| index)
        .collect()
}

pub fn build_scope_for_contract(
    contract: &ScriptContract,
    arg_stack: &ArgStack,
) -> Result<rhai::Scope<'static>, InputMismatchError> {
    let mut scope = rhai::Scope::new();
    if let Some(command) = command_of(arg_stack) {
        scope.push_dynamic(COMMAND_SCRIPT_VARIABLE, variant_to_dynamic(command));
    }
    for input in &contract.inputs {
        let value = arg_stack
            .get(&input.name)
            .or_else(|| command_arguments_for(input, arg_stack))
            .ok_or_else(|| InputMismatchError::Missing {
                name: input.name.clone(),
            })?;
        let actual_kind = VariantKind::from_variant(value);
        if actual_kind != input.kind {
            return Err(InputMismatchError::TypeMismatch {
                name: input.name.clone(),
                expected: input.kind,
                got: actual_kind,
            });
        }
        scope.push_dynamic(input.name.clone(), variant_to_dynamic(value.clone()));
    }
    Ok(scope)
}

fn command_of(arg_stack: &ArgStack) -> Option<Variant> {
    let members: BTreeMap<String, Variant> = COMMAND_MEMBERS
        .into_iter()
        .filter_map(|(member, variable)| {
            Some((member.to_owned(), arg_stack.get(variable)?.clone()))
        })
        .collect();
    (!members.is_empty()).then_some(Variant::Object(members))
}

fn command_arguments_for<'s>(input: &ScriptInput, arg_stack: &'s ArgStack) -> Option<&'s Variant> {
    if input.name != ARGUMENTS_INPUT {
        return None;
    }
    match input.kind {
        VariantKind::Array => arg_stack.get(COMMAND_ARGS_VARIABLE),
        VariantKind::String => arg_stack.get(COMMAND_TAIL_VARIABLE),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{ArgStack, ScriptInput, Variant, VariantKind};

    use super::*;

    #[test]
    fn inert_annotation_lines_selects_only_comments_whose_body_opens_with_a_directive() {
        for (source, expected) in [
            ("", vec![]),
            ("let x = 1;", vec![]),
            ("// @input n: Int", vec![0]),
            ("// @return String", vec![0]),
            ("///  @input n: Int", vec![0]),
            ("//@return String", vec![0]),
            ("\t    // @input n: Int", vec![0]),
            ("// see @input docs", vec![]),
            ("let y = \"// @input n: Int\";", vec![]),
            ("/* @input n: Int */", vec![]),
            (
                "let a = 1;\n// @input n: Int\n// plain note\nlet b = 2;\n/// @return String",
                vec![1, 4],
            ),
        ] {
            assert_eq!(
                inert_annotation_lines(source),
                expected,
                "source: {source:?}"
            );
        }
    }

    #[test]
    fn inert_annotation_lines_scans_past_the_fiftieth_line() {
        let source = format!("{}// @input late: Int", "let x = 1;\n".repeat(80));
        assert_eq!(inert_annotation_lines(&source), vec![80]);
    }

    #[test]
    fn empty_contract_and_empty_stack_yields_empty_scope() {
        let contract = ScriptContract::default();
        let stack = ArgStack::new();
        let scope = build_scope_for_contract(&contract, &stack).unwrap();
        assert!(scope.is_empty());
    }

    #[test]
    fn matching_string_input_added_to_scope() {
        let contract = ScriptContract {
            inputs: vec![ScriptInput {
                name: "user".into(),
                kind: VariantKind::String,
            }],
            returns: None,
        };
        let stack = ArgStack::new().set("user".into(), Variant::String("alice".into()));
        let scope = build_scope_for_contract(&contract, &stack).unwrap();
        let val = scope.get_value::<rhai::ImmutableString>("user").unwrap();
        assert_eq!(val.as_str(), "alice");
    }

    #[test]
    fn type_mismatch_returns_type_mismatch_error() {
        let contract = ScriptContract {
            inputs: vec![ScriptInput {
                name: "count".into(),
                kind: VariantKind::Int,
            }],
            returns: None,
        };
        let stack = ArgStack::new().set("count".into(), Variant::String("not-an-int".into()));
        let err = build_scope_for_contract(&contract, &stack).unwrap_err();
        assert!(matches!(
            err,
            InputMismatchError::TypeMismatch {
                ref name,
                expected: VariantKind::Int,
                got: VariantKind::String,
            } if name == "count"
        ));
    }

    #[test]
    fn missing_input_returns_missing_error() {
        let contract = ScriptContract {
            inputs: vec![ScriptInput {
                name: "missing".into(),
                kind: VariantKind::Int,
            }],
            returns: None,
        };
        let stack = ArgStack::new();
        let err = build_scope_for_contract(&contract, &stack).unwrap_err();
        assert!(matches!(
            err,
            InputMismatchError::Missing { ref name } if name == "missing"
        ));
    }

    fn command_stack() -> ArgStack {
        ArgStack::new()
            .set(
                COMMAND_ARGS_VARIABLE.to_owned(),
                Variant::Array(vec![
                    Variant::String("alice".into()),
                    Variant::String("bob".into()),
                ]),
            )
            .set(
                COMMAND_TAIL_VARIABLE.to_owned(),
                Variant::String("alice bob".into()),
            )
            .set(
                COMMAND_NAME_VARIABLE.to_owned(),
                Variant::String("so".into()),
            )
    }

    fn evaluated(contract: &ScriptContract, stack: &ArgStack, body: &str) -> Option<Variant> {
        let engine = crate::Engine::with_config(crate::EngineConfig {
            wall_time_ms: 30_000,
            ..crate::EngineConfig::default()
        });
        let mut scope = build_scope_for_contract(contract, stack).unwrap();
        engine
            .eval_script_with_scope_as_variant(body, &mut scope)
            .unwrap()
    }

    fn arguments_input(kind: VariantKind) -> ScriptContract {
        ScriptContract {
            inputs: vec![ScriptInput {
                name: ARGUMENTS_INPUT.to_owned(),
                kind,
            }],
            returns: None,
        }
    }

    #[test]
    fn a_script_with_no_declared_input_reads_the_command_through_its_members() {
        for (body, expected) in [
            ("command.tail", Variant::String("alice bob".into())),
            ("command.name", Variant::String("so".into())),
            ("command.args[1]", Variant::String("bob".into())),
            ("command.args.len()", Variant::Int(2)),
        ] {
            assert_eq!(
                evaluated(&ScriptContract::default(), &command_stack(), body),
                Some(expected),
                "{body}"
            );
        }
    }

    #[test]
    fn a_stack_without_a_command_adds_nothing_to_the_scope() {
        let stack = ArgStack::new().set("user".into(), Variant::String("alice".into()));

        let scope = build_scope_for_contract(&ScriptContract::default(), &stack).unwrap();

        assert!(scope.is_empty());
    }

    #[test]
    fn an_arguments_input_reads_the_command_value_of_its_declared_kind() {
        for (kind, body, expected) in [
            (
                VariantKind::String,
                "args",
                Variant::String("alice bob".into()),
            ),
            (
                VariantKind::Array,
                "args[0]",
                Variant::String("alice".into()),
            ),
        ] {
            assert_eq!(
                evaluated(&arguments_input(kind), &command_stack(), body),
                Some(expected),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn an_arguments_input_prefers_a_variable_of_that_exact_name() {
        let stack = command_stack().set(
            ARGUMENTS_INPUT.to_owned(),
            Variant::String("own value".into()),
        );

        assert_eq!(
            evaluated(&arguments_input(VariantKind::String), &stack, "args"),
            Some(Variant::String("own value".into()))
        );
    }

    #[test]
    fn an_arguments_input_of_another_kind_or_another_name_stays_missing() {
        let other_name = ScriptContract {
            inputs: vec![ScriptInput {
                name: "tail".into(),
                kind: VariantKind::String,
            }],
            returns: None,
        };
        for contract in [arguments_input(VariantKind::Int), other_name] {
            assert!(matches!(
                build_scope_for_contract(&contract, &command_stack()),
                Err(InputMismatchError::Missing { .. })
            ));
        }
    }
}
