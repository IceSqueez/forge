use forge_storage::StorageError;
use forge_types::{MessageRef, VARIANT_MAX_COLLECTION_LEN, VARIANT_MAX_DEPTH, VariantKind};
use thiserror::Error;

use crate::coded_text::coded_text;

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

    #[error("missing input `{name}`")]
    MissingInput { name: String },

    #[error("input `{name}`: expected {}, got {}", .expected.contract_name(), .got.contract_name())]
    InputTypeMismatch {
        name: String,
        expected: VariantKind,
        got: VariantKind,
    },

    #[error("{}", coded_text(.0))]
    UnknownVoice(MessageRef),

    #[error("the speak queue is unavailable: {}", coded_text(.0))]
    SpeakQueueUnavailable(MessageRef),

    #[error("no action has the id or name '{0}'")]
    UnknownScheduleTarget(String),

    #[error("{count} actions are named '{name}', schedule it by id instead")]
    AmbiguousScheduleTarget { name: String, count: usize },

    #[error("actions cannot be listed: {0}")]
    ActionsUnavailable(String),

    #[error("{0}")]
    ScheduleRejected(String),

    #[error("http: domain not allowed")]
    HttpDomainNotAllowed,

    #[error("http: HTTPS required")]
    HttpsRequired,

    #[error("http: local addresses blocked")]
    HttpPrivateAddress,

    #[error("http: rate limit exceeded")]
    HttpRateLimitExceeded,

    #[error("http: timeout")]
    HttpTimeout,

    #[error("http: {0}")]
    HttpNetwork(String),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("the script catalog never runs script functions")]
    DescribeOnly,

    #[error("value nested deeper than {VARIANT_MAX_DEPTH} levels cannot be stored")]
    NestedTooDeep,

    #[error("{0}")]
    NonFiniteFloat(String),

    #[error("unsupported rhai type for Variant conversion: {0}")]
    UnsupportedType(String),

    #[error("array with more than {VARIANT_MAX_COLLECTION_LEN} items cannot be stored")]
    ArrayTooLong,

    #[error("map with more than {VARIANT_MAX_COLLECTION_LEN} keys cannot be stored")]
    MapTooLong,
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
