use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("compile error in '{script}': {reason}")]
    Compile { script: String, reason: String },

    #[error("runtime error in '{script}': {reason}")]
    Runtime { script: String, reason: String },

    #[error("script '{script}' timed out after {elapsed_ms}ms (limit {limit_ms}ms)")]
    Timeout {
        script: String,
        elapsed_ms: u64,
        limit_ms: u64,
    },

    #[error("script '{script}' exceeded op-count limit ({ops} operations)")]
    OperationLimit { script: String, ops: u64 },

    #[error("condition '{expr}' did not evaluate to a boolean (got {got})")]
    ConditionNotBoolean { expr: String, got: String },

    #[error("call to '{fn_name}' denied: {reason}")]
    HostCallDenied { fn_name: String, reason: String },

    #[error("script not found: '{name}'")]
    NotFound { name: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_display_carries_numeric_fields() {
        let e = ScriptError::Timeout {
            script: "slow_script".into(),
            elapsed_ms: 750,
            limit_ms: 500,
        };
        let s = e.to_string();
        assert!(s.contains("750"), "missing elapsed_ms in: {s}");
        assert!(s.contains("500"), "missing limit_ms in: {s}");
    }

    #[test]
    fn operation_limit_display_carries_ops_count() {
        let e = ScriptError::OperationLimit {
            script: "heavy_script".into(),
            ops: 100_000,
        };
        assert!(e.to_string().contains("100000"));
    }
}
