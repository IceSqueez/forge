use forge_types::{ArgStack, ScriptContract, VariantKind};

use crate::convert::variant_to_dynamic;

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

/// Line indices are 0-based.
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

/// Err on missing input or type mismatch; empty contract always succeeds and returns an empty `Scope`.
pub fn build_scope_for_contract(
    contract: &ScriptContract,
    arg_stack: &ArgStack,
) -> Result<rhai::Scope<'static>, InputMismatchError> {
    let mut scope = rhai::Scope::new();
    for input in &contract.inputs {
        let value = arg_stack
            .get(&input.name)
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
}
