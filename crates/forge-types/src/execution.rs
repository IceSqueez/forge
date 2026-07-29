use crate::ids::{ActionId, EventId};
use crate::variant::Variant;
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

/// Trims, peels one enclosing `%...%` pair, then trims again; charset-agnostic so dotted names (`time.formatted`) survive verbatim.
pub fn strip_var_decoration(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('%')
        .and_then(|inner| inner.strip_suffix('%'))
        .unwrap_or(trimmed)
        .trim()
        .to_owned()
}

pub fn normalize_var_name(raw: &str) -> Option<String> {
    let name = strip_var_decoration(raw);
    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        Some(name)
    } else {
        None
    }
}

pub fn variant_preview(value: &Variant) -> String {
    const MAX_CHARS: usize = 800;
    let rendered = match value {
        Variant::Array(items) => return crate::variant::array_summary(items),
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
    /// Sentinel `index` for a nested step lifted from a branch/loop/switch body; such a row's `kind` carries a parent-path locator instead of a bare kind id.
    pub const NESTED: usize = usize::MAX;

    /// Surfaces keyed by top-level position skip nested rows; the full flat list keeps them so a branch failure stays diagnosable.
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
    fn interpolate_substitutes_known_tokens_and_leaves_surrounding_text_alone() {
        let stack = stack_with(&[
            ("first", Variant::String("Hello".to_string())),
            ("second", Variant::String("World".to_string())),
            ("count", Variant::Int(3)),
        ]);

        for (template, expected) in [
            ("", ""),
            ("no substitutions here", "no substitutions here"),
            ("%first%", "Hello"),
            ("say %first%, then stop", "say Hello, then stop"),
            ("%first% %second%", "Hello World"),
            ("%first%%second%", "HelloWorld"),
            ("%count% times", "3 times"),
            ("100%", "100%"),
        ] {
            assert_eq!(
                stack.interpolate(template),
                expected,
                "template {template:?}"
            );
        }
    }

    #[test]
    fn interpolate_trims_whitespace_inside_the_token_before_lookup() {
        let stack = stack_with(&[("index", Variant::Int(3))]);
        for template in ["%index %", "% index%", "% index %"] {
            assert_eq!(stack.interpolate(template), "3", "template {template:?}");
        }
    }

    /// Pinned as the shared contract with the overlay client runtime, which ports this scanner to JS.
    #[test]
    fn interpolate_holds_the_three_behaviours_a_regex_port_would_get_wrong() {
        let stack = stack_with(&[
            ("known", Variant::String("value".to_string())),
            ("recursive", Variant::String("%known%".to_string())),
        ]);

        for (label, template, expected) in [
            ("unknown token reprinted verbatim", "%missing%", "%missing%"),
            (
                "unknown token reprinted untrimmed",
                "% missing %",
                "% missing %",
            ),
            (
                "unterminated tail collapses to a lone percent",
                "start %missing",
                "start %",
            ),
            ("unterminated token alone", "%", "%"),
            (
                "a closed token before an unterminated tail still resolves",
                "%known% then %dangling",
                "value then %",
            ),
            (
                "a substituted value is never rescanned",
                "%recursive%",
                "%known%",
            ),
        ] {
            assert_eq!(stack.interpolate(template), expected, "{label}");
        }
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
    fn from_result_maps_ok_to_success_and_err_to_failed_with_display_text() {
        let ok: Result<(), std::io::Error> = Ok(());
        assert_eq!(
            SubActionOutcome::from_result(&ok),
            SubActionOutcome::Success
        );

        struct DisplayErr;
        impl core::fmt::Display for DisplayErr {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("disk full")
            }
        }
        let err: Result<(), DisplayErr> = Err(DisplayErr);
        assert_eq!(
            SubActionOutcome::from_result(&err),
            SubActionOutcome::Failed("disk full".to_owned())
        );
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

    #[test]
    fn strip_var_decoration_trims_then_peels_one_percent_pair_then_trims() {
        for (input, expected) in [
            ("index", "index"),
            (" index ", "index"),
            ("%index%", "index"),
            ("% index %", "index"),
            ("%%x%%", "%x%"),
            ("", ""),
            ("   ", ""),
            ("%", "%"),
            ("%%", ""),
            ("%index", "%index"),
            ("index%", "index%"),
            ("regex.matched", "regex.matched"),
            ("%regex.matched%", "regex.matched"),
        ] {
            assert_eq!(strip_var_decoration(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn normalize_var_name_accepts_ascii_identifier_after_peeling() {
        for (input, expected) in [
            ("index", "index"),
            ("%index%", "index"),
            ("  raw_name123  ", "raw_name123"),
            ("%  padded_id  %", "padded_id"),
        ] {
            assert_eq!(
                normalize_var_name(input).as_deref(),
                Some(expected),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn normalize_var_name_rejects_non_identifier_charset_after_peeling() {
        for bad in [
            "regex.matched",
            "%regex.matched%",
            "has space",
            "%%x%%",
            "",
            "   ",
            "%",
        ] {
            assert_eq!(normalize_var_name(bad), None, "input {bad:?}");
        }
    }

    #[test]
    fn variant_preview_summarizes_arrays_by_kind_and_length() {
        assert_eq!(
            variant_preview(&Variant::Array(vec![
                Variant::Int(1),
                Variant::Int(2),
                Variant::Int(3),
            ])),
            "int[3]"
        );
        assert_eq!(
            variant_preview(&Variant::Array(vec![Variant::Int(1), Variant::Bool(true)])),
            "[2]"
        );
    }
}
