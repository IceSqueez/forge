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
    fn collect_empty_source_returns_empty() {
        assert!(collect_annotation_diagnostics("").is_empty());
    }

    #[test]
    fn collect_pure_code_returns_empty() {
        assert!(collect_annotation_diagnostics("let x = 1;").is_empty());
    }

    #[test]
    fn collect_valid_input_return_returns_empty() {
        let src = "// @input foo: string\n// @return int";
        assert!(collect_annotation_diagnostics(src).is_empty());
    }

    #[test]
    fn collect_malformed_input_missing_type_returns_diagnostic() {
        let src = "// @input foo";
        let diags = collect_annotation_diagnostics(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 0);
        assert!(diags[0].message.contains("@input"));
    }

    #[test]
    fn collect_duplicate_input_emits_on_second_line() {
        let src = "// @input x: int\n// @input x: float";
        let diags = collect_annotation_diagnostics(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
        assert!(diags[0].message.contains("duplicate"));
    }

    #[test]
    fn collect_duplicate_return_emits_on_second_line() {
        let src = "// @return int\n// @return string";
        let diags = collect_annotation_diagnostics(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
        assert!(diags[0].message.contains("multiple"));
    }

    #[test]
    fn collect_unknown_type_emits_diagnostic() {
        let src = "// @input x: unicorn";
        let diags = collect_annotation_diagnostics(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unicorn"));
    }

    #[test]
    fn collect_multiple_errors_on_different_lines_all_reported() {
        let src = "// @input x: bad\n// @input x: int\n// @return wrong";
        let diags = collect_annotation_diagnostics(src);
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].line, 0);
        assert!(diags[0].message.contains("bad"));
        assert_eq!(diags[1].line, 1);
        assert!(diags[1].message.contains("duplicate"));
        assert_eq!(diags[2].line, 2);
        assert!(diags[2].message.contains("wrong"));
    }

    #[test]
    fn empty_source_returns_empty_contract() {
        let c = parse_contract("").unwrap();
        assert!(c.inputs.is_empty());
        assert!(c.returns.is_none());
    }

    #[test]
    fn two_inputs_parsed_in_order() {
        let src = "// @input user: string\n// @input count: int";
        let c = parse_contract(src).unwrap();
        assert_eq!(c.inputs.len(), 2);
        assert_eq!(
            c.inputs[0],
            ScriptInput {
                name: "user".into(),
                kind: VariantKind::String
            }
        );
        assert_eq!(
            c.inputs[1],
            ScriptInput {
                name: "count".into(),
                kind: VariantKind::Int
            }
        );
        assert!(c.returns.is_none());
    }

    #[test]
    fn one_input_and_return_parsed() {
        let src = "// @input x: int\n// @return string";
        let c = parse_contract(src).unwrap();
        assert_eq!(c.inputs.len(), 1);
        assert_eq!(c.inputs[0].kind, VariantKind::Int);
        assert_eq!(c.returns, Some(VariantKind::String));
    }

    #[test]
    fn whitespace_tolerant_around_name_colon_type() {
        let src = "//  @input  user  :  string  ";
        let c = parse_contract(src).unwrap();
        assert_eq!(c.inputs.len(), 1);
        assert_eq!(c.inputs[0].name, "user");
        assert_eq!(c.inputs[0].kind, VariantKind::String);
    }

    #[test]
    fn unknown_type_returns_unknown_type_error_with_line_number() {
        let src = "// @input x: foo";
        let err = parse_contract(src).unwrap_err();
        assert!(matches!(
            err,
            ContractParseError::UnknownType { line: 1, .. }
        ));
        assert!(err.to_string().contains("foo"));
    }

    #[test]
    fn duplicate_input_name_returns_error() {
        let src = "// @input x: int\n// @input x: float";
        let err = parse_contract(src).unwrap_err();
        assert!(matches!(
            err,
            ContractParseError::DuplicateInput { ref name } if name == "x"
        ));
    }

    #[test]
    fn duplicate_return_returns_error() {
        let src = "// @return int\n// @return string";
        let err = parse_contract(src).unwrap_err();
        assert!(matches!(err, ContractParseError::DuplicateReturn));
    }

    #[test]
    fn malformed_no_colon_returns_error() {
        let src = "// @input user string";
        let err = parse_contract(src).unwrap_err();
        assert!(matches!(err, ContractParseError::Malformed { line: 1 }));
    }

    #[test]
    fn only_first_50_lines_scanned() {
        let mut src = "\n".repeat(51);
        src.push_str("// @input x: int");
        let c = parse_contract(&src).unwrap();
        assert!(c.inputs.is_empty(), "line 52 must not be read");
    }

    #[test]
    fn non_directive_comments_ignored() {
        let src = "// just a comment\n// another note";
        let c = parse_contract(src).unwrap();
        assert!(c.inputs.is_empty());
        assert!(c.returns.is_none());
    }

    #[test]
    fn at_input_interspersed_with_code_and_comments_all_collected() {
        let src = "// @input a: int\nlet x = 1;\n// a plain comment\n// @input b: bool";
        let c = parse_contract(src).unwrap();
        assert_eq!(c.inputs.len(), 2);
        assert_eq!(c.inputs[0].name, "a");
        assert_eq!(c.inputs[1].name, "b");
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
