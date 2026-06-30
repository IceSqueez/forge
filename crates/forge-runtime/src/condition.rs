use std::cmp::Ordering;

use forge_script::{ConditionEvaluator, EngineConfig, ScriptError};

use crate::config::Config;

#[derive(Debug, thiserror::Error)]
pub enum ConditionError {
    #[error(transparent)]
    Eval(#[from] ScriptError),

    #[error("condition evaluation did not run to completion")]
    Canceled,
}

pub struct ConditionGate {
    evaluator: ConditionEvaluator,
}

impl ConditionGate {
    pub fn new(config: &Config) -> Self {
        let cfg = EngineConfig {
            op_limit: config.condition_op_limit,
            wall_time_ms: config.condition_wall_time_ms,
        };
        Self {
            evaluator: ConditionEvaluator::with_config(cfg),
        }
    }

    pub async fn evaluate(&self, expr: &str) -> Result<bool, ConditionError> {
        if let Some(verdict) = literal_compare(expr) {
            return Ok(verdict);
        }
        let evaluator = self.evaluator.clone();
        let owned = expr.to_owned();
        tokio::task::spawn_blocking(move || evaluator.eval(&owned))
            .await
            .map_err(|_| ConditionError::Canceled)?
            .map_err(ConditionError::from)
    }
}

#[derive(Clone, Copy)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    fn satisfied_by(self, ord: Ordering) -> bool {
        match self {
            CmpOp::Eq => ord == Ordering::Equal,
            CmpOp::Ne => ord != Ordering::Equal,
            CmpOp::Lt => ord == Ordering::Less,
            CmpOp::Le => ord != Ordering::Greater,
            CmpOp::Gt => ord == Ordering::Greater,
            CmpOp::Ge => ord != Ordering::Less,
        }
    }

    fn equality(self, eq: bool) -> Option<bool> {
        match self {
            CmpOp::Eq => Some(eq),
            CmpOp::Ne => Some(!eq),
            _ => None,
        }
    }
}

enum Lit {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

/// Conservative fast path for the catalog's "simple binary expression" form. It
/// fires only for `literal OP literal` where both operands are the same primitive
/// kind and ordering operators are restricted to numerics; every other shape
/// (identifiers, calls, logical/arithmetic operators, mixed kinds, ordered
/// string/bool comparison, escaped or embedded-quote strings) returns `None` and
/// defers to the rhai evaluator, the single semantic source of truth. Because the
/// fast path resolves only cases where Rust and rhai comparison agree, it can
/// never reach a verdict that full evaluation would not.
fn literal_compare(expr: &str) -> Option<bool> {
    let (op_start, op_end, op) = find_operator(expr)?;
    let lhs = parse_literal(&expr[..op_start])?;
    let rhs = parse_literal(&expr[op_end..])?;
    match (lhs, rhs) {
        (Lit::Int(a), Lit::Int(b)) => Some(op.satisfied_by(a.cmp(&b))),
        (Lit::Float(a), Lit::Float(b)) => a.partial_cmp(&b).map(|o| op.satisfied_by(o)),
        (Lit::Bool(a), Lit::Bool(b)) => op.equality(a == b),
        (Lit::Str(a), Lit::Str(b)) => op.equality(a == b),
        _ => None,
    }
}

fn find_operator(expr: &str) -> Option<(usize, usize, CmpOp)> {
    let bytes = expr.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'=' if bytes.get(i + 1) == Some(&b'=') => return Some((i, i + 2, CmpOp::Eq)),
            b'!' if bytes.get(i + 1) == Some(&b'=') => return Some((i, i + 2, CmpOp::Ne)),
            b'<' if bytes.get(i + 1) == Some(&b'=') => return Some((i, i + 2, CmpOp::Le)),
            b'>' if bytes.get(i + 1) == Some(&b'=') => return Some((i, i + 2, CmpOp::Ge)),
            b'<' => return Some((i, i + 1, CmpOp::Lt)),
            b'>' => return Some((i, i + 1, CmpOp::Gt)),
            b'&' | b'|' | b'+' | b'*' | b'/' | b'%' | b'(' | b')' => return None,
            _ => {}
        }
        i += 1;
    }
    None
}

fn parse_literal(s: &str) -> Option<Lit> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s == "true" {
        return Some(Lit::Bool(true));
    }
    if s == "false" {
        return Some(Lit::Bool(false));
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        if !inner.contains('"') && !inner.contains('\\') {
            return Some(Lit::Str(inner.to_owned()));
        }
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        return Some(Lit::Int(n));
    }
    if let Ok(f) = s.parse::<f64>()
        && f.is_finite()
    {
        return Some(Lit::Float(f));
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn gate_with(op_limit: u64, wall_time_ms: u64) -> ConditionGate {
        ConditionGate::new(&Config {
            max_nesting_depth: 32,
            condition_op_limit: op_limit,
            condition_wall_time_ms: wall_time_ms,
        })
    }

    /// THE PARITY INVARIANT. For each expression the gate's verdict (which may be
    /// reached by the literal fast path) must equal full rhai evaluation, which in
    /// turn must equal the independently-reasoned expected boolean. Rows split
    /// between cases the fast path resolves and cases it defers to rhai; if the
    /// fast path ever diverged from rhai, a fast-path row would mismatch here.
    #[tokio::test]
    async fn fast_path_verdict_never_diverges_from_full_evaluation() {
        let cfg = Config::default();
        let gate = ConditionGate::new(&cfg);
        let evaluator = ConditionEvaluator::with_config(EngineConfig {
            op_limit: cfg.condition_op_limit,
            wall_time_ms: cfg.condition_wall_time_ms,
        });

        // (expr, expected, fast_path_resolves) — the third column documents intent;
        // the assertion holds regardless of which path the gate actually took.
        let rows: &[(&str, bool, bool)] = &[
            // --- fast-path-resolved: literal OP literal, same primitive kind ---
            ("5 == 5", true, true),
            ("5 == 6", false, true),
            ("3 < 5", true, true),
            ("5 < 3", false, true),
            ("5 <= 5", true, true),
            ("5 >= 6", false, true),
            ("7 != 7", false, true),
            ("7 != 8", true, true),
            ("1.5 < 2.5", true, true),
            ("2.5 <= 2.5", true, true),
            ("true == true", true, true),
            ("true == false", false, true),
            ("false != true", true, true),
            (r#""abc" == "abc""#, true, true),
            (r#""abc" == "abd""#, false, true),
            (r#""abc" != "abd""#, true, true),
            // --- deferred to rhai (literal_compare returns None) ---
            ("1 == 1.0", true, false),            // mixed kinds
            ("1 != 1.0", false, false),           // mixed kinds
            (r#""a" < "b""#, true, false),        // string ordering
            (r#""b" < "a""#, false, false),       // string ordering
            ("1 < 2 && 3 < 4", true, false),      // logical operator
            ("1 + 1 == 2", true, false),          // arithmetic operator
            ("let n = 10; n > 3", true, false),   // identifier / statement
            (r#""a\"b" == "a\"b""#, true, false), // embedded-quote string
        ];

        let mut saw_fast = false;
        let mut saw_deferred = false;
        for &(expr, expected, fast) in rows {
            saw_fast |= fast;
            saw_deferred |= !fast;

            let full = evaluator
                .eval(expr)
                .unwrap_or_else(|e| panic!("full evaluation of {expr} errored: {e:?}"));
            assert_eq!(full, expected, "rhai verdict for {expr}");

            let gated = gate
                .evaluate(expr)
                .await
                .unwrap_or_else(|e| panic!("gate evaluation of {expr} errored: {e:?}"));
            assert_eq!(gated, expected, "gate verdict for {expr}");
        }
        assert!(saw_fast && saw_deferred, "table must cover both paths");
    }

    #[tokio::test]
    async fn deferred_string_ordering_returns_correct_boolean_without_error() {
        let gate = ConditionGate::new(&Config::default());
        assert!(gate.evaluate(r#""apple" < "banana""#).await.unwrap());
        assert!(!gate.evaluate(r#""banana" < "apple""#).await.unwrap());
    }

    #[tokio::test]
    async fn deferred_mixed_kind_comparison_returns_correct_boolean_without_error() {
        let gate = ConditionGate::new(&Config::default());
        assert!(gate.evaluate("2 == 2.0").await.unwrap());
        assert!(!gate.evaluate("2 < 1.0").await.unwrap());
    }

    #[tokio::test]
    async fn non_boolean_result_surfaces_as_eval_error_not_truthiness() {
        let gate = ConditionGate::new(&Config::default());
        for expr in ["1 + 1", "42", r#""x""#] {
            let err = gate.evaluate(expr).await.unwrap_err();
            assert!(
                matches!(
                    err,
                    ConditionError::Eval(ScriptError::ConditionNotBoolean { .. })
                ),
                "expr {expr} expected ConditionNotBoolean, got {err:?}",
            );
        }
    }

    #[tokio::test]
    async fn op_limit_exhausting_condition_returns_typed_eval_error() {
        let gate = gate_with(100, 5_000);
        let err = gate
            .evaluate("let x = 0; while x < 1000000 { x += 1; } x > 0")
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ConditionError::Eval(ScriptError::OperationLimit { .. })
            ),
            "expected OperationLimit, got {err:?}",
        );
    }

    #[tokio::test]
    async fn wall_budget_exhausting_condition_returns_typed_error_without_hanging() {
        let gate = gate_with(1_000_000_000, 1);
        let start = Instant::now();
        let err = gate.evaluate("loop {}").await.unwrap_err();
        let elapsed = start.elapsed();
        assert!(
            matches!(err, ConditionError::Eval(ScriptError::Timeout { .. })),
            "expected Timeout, got {err:?}",
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "wall-bounded condition must return promptly, took {elapsed:?}",
        );
    }
}
