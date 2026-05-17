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

    #[error("call to '{fn_name}' denied: {reason}")]
    HostCallDenied { fn_name: String, reason: String },

    #[error("script not found: '{name}'")]
    NotFound { name: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn compile_display_non_empty() {
        let e = ScriptError::Compile {
            script: "my_script".into(),
            reason: "unexpected token".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn runtime_display_non_empty() {
        let e = ScriptError::Runtime {
            script: "my_script".into(),
            reason: "index out of bounds".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn timeout_carries_elapsed_and_limit() {
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
    fn operation_limit_carries_ops_count() {
        let e = ScriptError::OperationLimit {
            script: "heavy_script".into(),
            ops: 100_000,
        };
        let s = e.to_string();
        assert!(s.contains("100000"), "missing ops count in: {s}");
    }

    #[test]
    fn host_call_denied_display_non_empty() {
        let e = ScriptError::HostCallDenied {
            fn_name: "fs::read".into(),
            reason: "filesystem access is sandboxed".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn not_found_display_non_empty() {
        let e = ScriptError::NotFound {
            name: "missing_script".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn io_error_satisfies_std_error_trait() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let e: ScriptError = io_err.into();
        let _: &dyn Error = &e;
        assert!(!e.to_string().is_empty());
    }
}
