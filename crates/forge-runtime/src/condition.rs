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
