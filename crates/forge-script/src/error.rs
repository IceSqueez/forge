use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("compile error: {reason}")]
    Compile { reason: String },

    #[error("runtime error: {reason}")]
    Runtime { reason: String },

    #[error("timed out after {elapsed_ms}ms (limit {limit_ms}ms)")]
    Timeout { elapsed_ms: u64, limit_ms: u64 },

    #[error("exceeded op-count limit ({ops} operations)")]
    OperationLimit { ops: u64 },

    #[error("condition '{expr}' did not evaluate to a boolean (got {got})")]
    ConditionNotBoolean { expr: String, got: String },

    #[error("call to '{fn_name}' denied: {reason}")]
    HostCallDenied { fn_name: String, reason: String },

    #[error("script not found: '{name}'")]
    NotFound { name: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl ScriptError {
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::Compile { .. } => "syntax",
            Self::Timeout { .. } => "timeout",
            Self::OperationLimit { .. } => "ops_exceeded",
            _ => "runtime",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_display_carries_numeric_fields() {
        let s = ScriptError::Timeout {
            elapsed_ms: 750,
            limit_ms: 500,
        }
        .to_string();
        assert!(s.contains("750") && s.contains("500"), "{s}");
    }

    #[test]
    fn operation_limit_display_carries_ops_count() {
        assert!(
            ScriptError::OperationLimit { ops: 100_000 }
                .to_string()
                .contains("100000")
        );
    }

    #[test]
    fn error_type_maps_each_failure_kind() {
        for (err, kind) in [
            (ScriptError::Compile { reason: "x".into() }, "syntax"),
            (ScriptError::Runtime { reason: "x".into() }, "runtime"),
            (
                ScriptError::Timeout {
                    elapsed_ms: 1,
                    limit_ms: 1,
                },
                "timeout",
            ),
            (ScriptError::OperationLimit { ops: 1 }, "ops_exceeded"),
            (
                ScriptError::ConditionNotBoolean {
                    expr: "1".into(),
                    got: "i64".into(),
                },
                "runtime",
            ),
        ] {
            assert_eq!(err.error_type(), kind, "{err}");
        }
    }
}
