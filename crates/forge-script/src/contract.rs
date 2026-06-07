use std::collections::HashSet;

pub use forge_types::AnnotationDiagnostic;

use forge_types::{ArgStack, ScriptContract, ScriptInput, VariantKind};

use crate::convert::variant_to_dynamic;

#[derive(Debug, thiserror::Error)]
pub enum ContractParseError {
    #[error(
        "invalid type `{type_name}` on line {line}: \
         must be int/float/bool/string/datetime/array/object"
    )]
    UnknownType { line: usize, type_name: String },

    #[error("malformed @input on line {line}: expected `// @input <name>: <type>`")]
    Malformed { line: usize },

    #[error("@return appears multiple times")]
    DuplicateReturn,

    #[error("duplicate input name `{name}`")]
    DuplicateInput { name: String },
}

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

pub fn collect_annotation_diagnostics(source: &str) -> Vec<AnnotationDiagnostic> {
    let _ = source;
    Vec::new()
}

/// Parses `// @input` and `// @return` doc-comment annotations from script source.
///
/// Scans only the first 50 lines. Lines that are not `@input`/`@return` directives are
/// silently skipped. Whitespace around the name, colon, and type token is stripped.
///
/// Returns `Err` on unknown types, malformed `@input` lines, duplicate input names, or
/// multiple `@return` annotations.
pub fn parse_contract(source: &str) -> Result<ScriptContract, ContractParseError> {
    let mut inputs: Vec<ScriptInput> = Vec::new();
    let mut returns: Option<VariantKind> = None;
    let mut seen_names: HashSet<String> = HashSet::new();

    for (line_idx, raw) in source.lines().take(50).enumerate() {
        let trimmed = raw.trim();
        if !trimmed.starts_with("//") {
            continue;
        }
        let after_slashes = trimmed.trim_start_matches("//").trim();

        if let Some(rest) = after_slashes.strip_prefix("@input ") {
            let parts: Vec<&str> = rest.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(ContractParseError::Malformed { line: line_idx + 1 });
            }
            let name = parts[0].trim().to_string();
            let type_name = parts[1].trim();
            if name.is_empty() || type_name.is_empty() {
                return Err(ContractParseError::Malformed { line: line_idx + 1 });
            }
            let kind = VariantKind::from_contract_name(type_name).ok_or_else(|| {
                ContractParseError::UnknownType {
                    line: line_idx + 1,
                    type_name: type_name.to_string(),
                }
            })?;
            if !seen_names.insert(name.clone()) {
                return Err(ContractParseError::DuplicateInput { name });
            }
            inputs.push(ScriptInput { name, kind });
        } else if let Some(rest) = after_slashes.strip_prefix("@return ") {
            if returns.is_some() {
                return Err(ContractParseError::DuplicateReturn);
            }
            let type_name = rest.trim();
            let kind = VariantKind::from_contract_name(type_name).ok_or_else(|| {
                ContractParseError::UnknownType {
                    line: line_idx + 1,
                    type_name: type_name.to_string(),
                }
            })?;
            returns = Some(kind);
        }
    }

    Ok(ScriptContract { inputs, returns })
}

/// Validates `arg_stack` against `contract` and builds a rhai `Scope` populated with all
/// declared inputs.
///
/// Returns `Err(InputMismatchError::Missing)` if a declared input is absent from the stack,
/// and `Err(InputMismatchError::TypeMismatch)` if the runtime kind differs from the declared
/// kind. An empty contract always succeeds and produces an empty `Scope`.
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
    fn collect_annotation_diagnostics_empty_source_returns_empty_vec() {
        assert!(collect_annotation_diagnostics("").is_empty());
    }

    #[test]
    fn collect_annotation_diagnostics_nonempty_source_returns_empty_vec() {
        assert!(collect_annotation_diagnostics("let x = 1;").is_empty());
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
