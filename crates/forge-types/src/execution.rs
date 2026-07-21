use crate::ids::{ActionId, EventId};
use crate::variant::{Variant, VariantKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionOutcome {
    Success,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubActionOutcome {
    Success,
    Failed(String),
    Skipped(String),
}

impl SubActionOutcome {
    pub fn from_result<T, E: core::fmt::Display>(result: &Result<T, E>) -> Self {
        match result {
            Ok(_) => Self::Success,
            Err(e) => Self::Failed(e.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubActionTelemetry {
    pub index: usize,
    pub kind: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
    pub outcome: SubActionOutcome,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args_in: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub produced: BTreeMap<String, String>,
}

pub fn normalize_var_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let unwrapped = trimmed
        .strip_prefix('%')
        .and_then(|inner| inner.strip_suffix('%'))
        .unwrap_or(trimmed)
        .trim();
    if !unwrapped.is_empty()
        && unwrapped
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        Some(unwrapped.to_owned())
    } else {
        None
    }
}

pub fn variant_preview(value: &Variant) -> String {
    const MAX_CHARS: usize = 800;
    let rendered = match value {
        Variant::Array(items) => {
            let uniform = items.first().map(VariantKind::from_variant).filter(|kind| {
                items
                    .iter()
                    .all(|item| VariantKind::from_variant(item) == *kind)
            });
            return match uniform {
                Some(kind) => format!("{}[{}]", kind.contract_name(), items.len()),
                None => format!("[{}]", items.len()),
            };
        }
        Variant::Object(_) => serde_json::to_string_pretty(&value.to_plain_json())
            .unwrap_or_else(|_| value.to_string()),
        Variant::String(s) if !s.contains('\n') => {
            serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
        }
        scalar => scalar.to_string(),
    };
    if rendered.chars().count() <= MAX_CHARS {
        return rendered;
    }
    let clipped: String = rendered.chars().take(MAX_CHARS).collect();
    let clipped = match clipped.rfind('\n') {
        Some(pos) => &clipped[..pos],
        None => clipped.as_str(),
    };
    format!("{clipped}\n...")
}

impl SubActionTelemetry {
    /// `index` value marking a row that is not a positional top-level chain step
    /// but a nested step lifted out of a branch/loop/switch body into the same
    /// flat list. Such a row carries its parent-path locator in `kind` (segments
    /// `parentIndex.arm` joined by `/`, ending in `localIndex.kindId`) rather than
    /// a bare kind id, and holds no top-level position.
    pub const NESTED: usize = usize::MAX;

    /// Whether this row is a nested step surfaced from a composite body. Surfaces
    /// keyed by top-level position (per-step averages, total-time sums) skip these;
    /// the full flat list keeps them so a failure inside a branch stays diagnosable.
    pub fn is_nested(&self) -> bool {
        self.index == Self::NESTED
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionMetadata {
    Trigger {
        event_id: EventId,
        #[serde(default)]
        trigger_kind: Option<String>,
    },
    QuickAction {
        builtin_id: String,
        label: String,
    },
}

/// Immutable after construction; built once from trigger event and globals snapshot.
#[derive(Clone)]
pub struct ArgStack(BTreeMap<String, Variant>);

impl ArgStack {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn get(&self, key: &str) -> Option<&Variant> {
        self.0.get(key)
    }

    pub fn snapshot(&self) -> BTreeMap<String, Variant> {
        self.0.clone()
    }

    /// Returns a new `ArgStack` with `key` bound to `value`.
    pub fn set(mut self, key: String, value: Variant) -> Self {
        self.0.insert(key, value);
        self
    }

    /// Single-pass `%name%` substitution over `template`. Unknown tokens remain verbatim.
    pub fn interpolate(&self, template: &str) -> String {
        let mut result = String::with_capacity(template.len());
        let mut chars = template.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                result.push(ch);
                continue;
            }
            let token_start = result.len();
            result.push('%');
            let mut key = String::new();
            let mut closed = false;
            for inner in chars.by_ref() {
                if inner == '%' {
                    closed = true;
                    break;
                }
                key.push(inner);
            }
            if !closed {
                continue;
            }
            if let Some(val) = self.0.get(key.trim()) {
                result.truncate(token_start);
                result.push_str(&val.to_string());
            } else {
                result.push_str(&key);
                result.push('%');
            }
        }
        result
    }
}

impl Default for ArgStack {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub action_id: ActionId,
    pub metadata: ExecutionMetadata,
    pub arg_stack_snapshot: BTreeMap<String, Variant>,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    pub telemetry: Vec<SubActionTelemetry>,
    pub outcome: ExecutionOutcome,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::ids::{ActionId, EventId};
    use crate::variant::Variant;
    use time::OffsetDateTime;

    fn stack_with(pairs: &[(&str, Variant)]) -> ArgStack {
        let mut s = ArgStack::new();
        for (k, v) in pairs {
            s = s.set(k.to_string(), v.clone());
        }
        s
    }

    #[test]
    fn interpolate_empty_template() {
        let stack = ArgStack::new();
        assert_eq!(stack.interpolate(""), "");
    }

    #[test]
    fn interpolate_single_known_var() {
        let stack = stack_with(&[("name", Variant::String("Twitch".to_string()))]);
        assert_eq!(stack.interpolate("%name%"), "Twitch");
    }

    #[test]
    fn interpolate_var_in_sentence() {
        let stack = stack_with(&[("user", Variant::String("alice".to_string()))]);
        assert_eq!(
            stack.interpolate("hello %user%, welcome"),
            "hello alice, welcome"
        );
    }

    #[test]
    fn interpolate_unknown_var_stays_verbatim() {
        let stack = ArgStack::new();
        assert_eq!(stack.interpolate("%missing%"), "%missing%");
    }

    #[test]
    fn interpolate_trims_whitespace_inside_the_token_before_lookup() {
        // The lookup key is trimmed, so padding inside the `%...%` still resolves
        // the same binding. An unknown padded token stays verbatim (padding kept).
        let stack = stack_with(&[("index", Variant::Int(3))]);
        for template in ["%index %", "% index%", "% index %"] {
            assert_eq!(stack.interpolate(template), "3", "template {template:?}");
        }
        assert_eq!(stack.interpolate("%unknown %"), "%unknown %");
    }

    #[test]
    fn interpolate_is_single_pass_no_recursion() {
        let stack = stack_with(&[("a", Variant::String("%b%".to_string()))]);
        assert_eq!(stack.interpolate("%a%"), "%b%");
    }

    #[test]
    fn interpolate_multiple_vars() {
        let stack = stack_with(&[
            ("first", Variant::String("Hello".to_string())),
            ("second", Variant::String("World".to_string())),
        ]);
        assert_eq!(stack.interpolate("%first% %second%"), "Hello World");
    }

    #[test]
    fn interpolate_no_tokens_unchanged() {
        let stack = ArgStack::new();
        assert_eq!(
            stack.interpolate("no substitutions here"),
            "no substitutions here"
        );
    }

    #[test]
    fn arg_stack_set_returns_new_layer() {
        let s1 = ArgStack::new();
        let s2 = s1.set("x".to_string(), Variant::Int(1));
        let s3 = s2.set("y".to_string(), Variant::Int(2));
        assert_eq!(s3.get("x"), Some(&Variant::Int(1)));
        assert_eq!(s3.get("y"), Some(&Variant::Int(2)));
    }

    #[test]
    fn execution_context_serde_roundtrip() {
        let event_id = EventId::new();
        let ctx = ExecutionContext {
            action_id: ActionId::new(),
            metadata: ExecutionMetadata::Trigger {
                event_id,
                trigger_kind: None,
            },
            arg_stack_snapshot: BTreeMap::new(),
            started_at: OffsetDateTime::now_utc(),
            completed_at: None,
            telemetry: vec![SubActionTelemetry {
                index: 0,
                kind: "SendChat".to_string(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 5,
                outcome: SubActionOutcome::Success,
                args_in: BTreeMap::new(),
                produced: BTreeMap::new(),
            }],
            outcome: ExecutionOutcome::Success,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: ExecutionContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx.action_id, back.action_id);
        assert_eq!(ctx.metadata, back.metadata);
        assert_eq!(ctx.outcome, back.outcome);
        assert_eq!(ctx.telemetry.len(), back.telemetry.len());
    }

    #[test]
    fn execution_metadata_quick_action_serde_roundtrip() {
        let meta = ExecutionMetadata::QuickAction {
            builtin_id: "obs".to_string(),
            label: "Toggle Stream".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: ExecutionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, back);
    }

    #[test]
    fn execution_outcome_variants_serde_roundtrip() {
        let outcomes = [
            ExecutionOutcome::Success,
            ExecutionOutcome::Failed("rhai panic".to_string()),
            ExecutionOutcome::Cancelled,
        ];
        for o in outcomes {
            let json = serde_json::to_string(&o).unwrap();
            let back: ExecutionOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(o, back);
        }
    }

    #[test]
    fn sub_action_outcome_variants_serde_roundtrip() {
        let outcomes = [
            SubActionOutcome::Success,
            SubActionOutcome::Failed("timeout".to_string()),
            SubActionOutcome::Skipped("disabled".to_string()),
        ];
        for o in outcomes {
            let json = serde_json::to_string(&o).unwrap();
            let back: SubActionOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(o, back);
        }
    }
}
