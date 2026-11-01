use crate::date_pattern::{NamesLanguage, render_inline_pattern};
use crate::ids::{ActionId, EventId};
use crate::integration::IntegrationId;
use crate::message::{
    INTEGRATION_DISABLED, MessageRef, UserText, VerbatimText, WHISPERS_UNSUPPORTED,
};
use crate::redaction::RedactedText;
use crate::template::{TemplatePiece, TemplatePieces};
use crate::variant::Variant;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionOutcome {
    Success,
    Failed(UserText),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubActionOutcome {
    Success,
    Failed(UserText),
    Skipped(UserText),
    IntegrationDisabled(IntegrationId),
}

pub fn whispers_unsupported_message(target: &str) -> MessageRef {
    MessageRef::new(WHISPERS_UNSUPPORTED).with_arg("target", VerbatimText::new(target))
}

pub fn integration_disabled_message(integration: &IntegrationId) -> MessageRef {
    MessageRef::new(INTEGRATION_DISABLED).with_arg("integration", integration.clone())
}

impl SubActionOutcome {
    pub fn failure_reason(&self) -> Option<UserText> {
        match self {
            Self::Failed(reason) => Some(reason.clone()),
            Self::IntegrationDisabled(integration) => {
                Some(integration_disabled_message(integration).into())
            }
            Self::Success | Self::Skipped(_) => None,
        }
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::IntegrationDisabled(_))
    }
}

const RELEASED_NESTED_INDEX: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<usize>", into = "Option<usize>")]
pub enum StepPosition {
    TopLevel(usize),
    Nested,
}

impl StepPosition {
    pub fn top_level(self) -> Option<usize> {
        match self {
            Self::TopLevel(index) => Some(index),
            Self::Nested => None,
        }
    }
}

impl From<Option<usize>> for StepPosition {
    fn from(index: Option<usize>) -> Self {
        index
            .filter(|index| *index != RELEASED_NESTED_INDEX)
            .map_or(Self::Nested, Self::TopLevel)
    }
}

impl From<StepPosition> for Option<usize> {
    fn from(position: StepPosition) -> Self {
        position.top_level()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubActionTelemetry {
    #[serde(rename = "index")]
    pub position: StepPosition,
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

struct RedactedValues<'a>(&'a BTreeMap<String, String>);

impl fmt::Debug for RedactedValues<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(
                self.0
                    .iter()
                    .map(|(key, value)| (key, RedactedText::new(value))),
            )
            .finish()
    }
}

impl fmt::Debug for SubActionTelemetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubActionTelemetry")
            .field("position", &self.position)
            .field("kind", &self.kind)
            .field("started_at", &self.started_at)
            .field("duration_ms", &self.duration_ms)
            .field("outcome", &self.outcome)
            .field("args_in", &RedactedValues(&self.args_in))
            .field("produced", &RedactedValues(&self.produced))
            .finish()
    }
}

impl SubActionTelemetry {
    pub fn is_nested(&self) -> bool {
        self.position == StepPosition::Nested
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
    Scheduled {
        event_id: EventId,
        scheduled_run_id: i64,
        #[serde(default)]
        scheduled_by_action: Option<ActionId>,
        #[serde(default)]
        scheduled_by_run: Option<EventId>,
    },
}

pub const COMMAND_NAME_VARIABLE: &str = "command_name";
pub const COMMAND_ARGS_VARIABLE: &str = "command.args";
pub const COMMAND_TAIL_VARIABLE: &str = "command.tail";

#[derive(Clone, Default)]
pub struct ArgStack {
    values: BTreeMap<String, Variant>,
    names_language: NamesLanguage,
}

impl fmt::Debug for ArgStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.values.keys()).finish()
    }
}

impl ArgStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_names_language(mut self, names_language: NamesLanguage) -> Self {
        self.names_language = names_language;
        self
    }

    pub fn names_language(&self) -> NamesLanguage {
        self.names_language
    }

    pub fn get(&self, key: &str) -> Option<&Variant> {
        self.values.get(key)
    }

    pub fn snapshot(&self) -> BTreeMap<String, Variant> {
        self.values.clone()
    }

    pub fn set(mut self, key: String, value: Variant) -> Self {
        self.values.insert(key, value);
        self
    }

    pub fn resolve(&self, key: &str) -> Option<&Variant> {
        if let Some(exact) = self.values.get(key) {
            return Some(exact);
        }
        key.match_indices('.')
            .find_map(|(dot, _)| walk_path(self.values.get(&key[..dot])?, &key[dot + 1..]))
    }

    pub fn interpolate(&self, template: &str) -> String {
        let mut result = String::with_capacity(template.len());
        for piece in TemplatePieces::new(template) {
            match piece {
                TemplatePiece::Literal(text) => result.push_str(text),
                TemplatePiece::Reference { name, format, raw } => match self.resolve(name) {
                    Some(value) => {
                        result.push_str(&render_reference(value, format, self.names_language));
                    }
                    None => result.push_str(raw),
                },
            }
        }
        result
    }
}

fn walk_path<'v>(root: &'v Variant, path: &str) -> Option<&'v Variant> {
    let mut current = root;
    for segment in path.split('.') {
        current = match current {
            Variant::Object(fields) => fields.get(segment)?,
            Variant::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

fn render_reference(value: &Variant, format: Option<&str>, language: NamesLanguage) -> String {
    match (value, format) {
        (Variant::Datetime(at), Some(pattern)) => render_inline_pattern(*at, pattern, language),
        _ => value.to_string(),
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
mod tests {
    #[test]
    fn a_stored_position_decodes_to_top_level_or_nested() {
        for (stored, expected) in [
            ("3", StepPosition::TopLevel(3)),
            ("null", StepPosition::Nested),
            ("18446744073709551615", StepPosition::Nested),
        ] {
            let decoded: StepPosition = serde_json::from_str(stored).unwrap();
            assert_eq!(decoded, expected, "{stored}");
        }
    }

    #[test]
    fn a_nested_position_is_written_as_null() {
        assert_eq!(
            serde_json::to_string(&StepPosition::Nested).unwrap(),
            "null"
        );
    }

    use super::*;
    use crate::ids::{ActionId, EventId};
    use crate::message::MessageArg;
    use crate::template::{normalize_var_name, strip_var_decoration};
    use crate::variant::Variant;
    use crate::variant::variant_preview;
    use time::OffsetDateTime;

    fn stack_with(pairs: &[(&str, Variant)]) -> ArgStack {
        let mut s = ArgStack::new();
        for (k, v) in pairs {
            s = s.set(k.to_string(), v.clone());
        }
        s
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_string())
    }

    fn object(fields: &[(&str, Variant)]) -> Variant {
        Variant::Object(
            fields
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
    }

    #[test]
    fn a_dotted_flat_key_holding_an_array_answers_positional_references() {
        let stack = stack_with(&[
            (
                "command.args",
                Variant::Array(vec![text("alice"), text("bob")]),
            ),
            ("command.tail", text("alice bob")),
        ]);

        for (template, expected) in [
            ("%command.args.0%", "alice"),
            ("%command.args.1%", "bob"),
            ("%command.tail%", "alice bob"),
            ("%command.args.2%", "%command.args.2%"),
            ("%command.args.-1%", "%command.args.-1%"),
            ("%command.args.first%", "%command.args.first%"),
            ("%command.tail.0%", "%command.tail.0%"),
            ("%command.args.%", "%command.args.%"),
        ] {
            assert_eq!(
                stack.interpolate(template),
                expected,
                "template {template:?}"
            );
        }
    }

    #[test]
    fn a_path_walks_nested_values_below_a_dotted_flat_key() {
        let stack = stack_with(&[(
            "command.args",
            Variant::Array(vec![object(&[("name", text("alice"))])]),
        )]);

        assert_eq!(stack.resolve("command.args.0.name"), Some(&text("alice")));
        assert_eq!(stack.resolve("command.args.0.age"), None);
    }

    #[test]
    fn the_shortest_prefix_that_resolves_the_whole_path_answers_a_reference() {
        let shorter = object(&[("b", object(&[("c", text("from a"))]))]);
        let longer = object(&[("c", text("from a.b"))]);
        let dead_end = object(&[("other", text("unused"))]);

        for (label, stack, expected) in [
            (
                "both roots resolve",
                stack_with(&[("a", shorter.clone()), ("a.b", longer.clone())]),
                "from a",
            ),
            (
                "the shorter root dead-ends",
                stack_with(&[("a", dead_end), ("a.b", longer.clone())]),
                "from a.b",
            ),
            (
                "an exact flat key outranks every path",
                stack_with(&[("a", shorter), ("a.b", longer), ("a.b.c", text("exact"))]),
                "exact",
            ),
        ] {
            assert_eq!(stack.resolve("a.b.c"), Some(&text(expected)), "{label}");
        }
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
    fn interpolate_reprints_unresolvable_text_verbatim_and_never_rescans() {
        let stack = stack_with(&[
            ("known", Variant::String("value".to_string())),
            ("recursive", Variant::String("%known%".to_string())),
        ]);

        for (label, template, expected) in [
            ("unknown token reprinted verbatim", "%missing%", "%missing%"),
            (
                "unterminated tail kept verbatim",
                "start %missing",
                "start %missing",
            ),
            ("lone percent", "%", "%"),
            (
                "a closed token before an unterminated tail still resolves",
                "%known% then %dangling",
                "value then %dangling",
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
    fn interpolate_treats_a_percent_opening_no_valid_name_as_literal_and_resumes_after_it() {
        let stack = stack_with(&[
            ("user", Variant::String("alice".to_string())),
            ("count", Variant::Int(3)),
            ("користувач", Variant::String("Оля".to_string())),
        ]);

        for (template, expected) in [
            ("%user% gets 50% off", "alice gets 50% off"),
            ("100%", "100%"),
            ("%%", "%%"),
            ("50%%user%", "50%alice"),
            ("%user%%count%", "alice3"),
            ("%користувач%!", "Оля!"),
            ("%名前%", "%名前%"),
            ("50% the prize %user%", "50% the prize alice"),
            ("%a$b%user%", "%a$balice"),
        ] {
            assert_eq!(
                stack.interpolate(template),
                expected,
                "template {template:?}"
            );
        }
    }

    #[test]
    fn interpolate_never_reads_space_padded_text_between_percents_as_a_token() {
        let stack = stack_with(&[
            ("user", Variant::String("alice".to_string())),
            ("amount", Variant::Int(20)),
        ]);
        for (template, expected) in [
            ("Discount 50% for %user%!", "Discount 50% for alice!"),
            ("50% of %amount%", "50% of 20"),
            ("% user %", "% user %"),
            ("%user %", "%user %"),
        ] {
            assert_eq!(
                stack.interpolate(template),
                expected,
                "template {template:?}"
            );
        }
    }

    fn mid_year_noon() -> OffsetDateTime {
        time::Date::from_calendar_date(2026, time::Month::July, 1)
            .unwrap()
            .with_hms(12, 0, 0)
            .unwrap()
            .assume_utc()
    }

    #[test]
    fn interpolate_ignores_a_format_on_values_that_are_not_datetimes() {
        let stack = stack_with(&[
            ("name", Variant::String("alice".to_string())),
            ("count", Variant::Int(5)),
        ]);
        assert_eq!(stack.interpolate("%name:HH% %count:yyyy%"), "alice 5");
    }

    #[test]
    fn interpolate_renders_a_datetime_with_an_invalid_format_in_canonical_form() {
        let at = mid_year_noon();
        let stack = stack_with(&[("at", Variant::Datetime(at))]);
        for template in ["%at:qqq%", "%at:%", "%at:dddd%"] {
            assert_eq!(
                stack.interpolate(template),
                crate::datetime_to_text(&at),
                "template {template:?}"
            );
        }
    }

    #[test]
    fn interpolate_reprints_a_formatted_token_for_a_missing_variable_verbatim() {
        let stack = stack_with(&[("at", Variant::Datetime(mid_year_noon()))]);
        assert_eq!(
            stack.interpolate("at %gone:HH:mm% then"),
            "at %gone:HH:mm% then"
        );
    }

    #[test]
    fn interpolate_formats_a_datetime_reached_through_a_nested_path() {
        let stack = stack_with(&[(
            "obj",
            Variant::Object(BTreeMap::from([(
                "at".to_string(),
                Variant::Datetime(mid_year_noon()),
            )])),
        )]);
        assert_eq!(stack.interpolate("%obj.at:yyyy/MM%"), "2026/07");
    }

    #[test]
    fn chained_sets_keep_every_earlier_key() {
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
            started_at: OffsetDateTime::UNIX_EPOCH,
            completed_at: None,
            telemetry: vec![SubActionTelemetry {
                position: StepPosition::TopLevel(0),
                kind: "SendChat".to_string(),
                started_at: OffsetDateTime::UNIX_EPOCH,
                duration_ms: 5,
                outcome: SubActionOutcome::Success,
                args_in: BTreeMap::new(),
                produced: BTreeMap::new(),
            }],
            outcome: ExecutionOutcome::Success,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: ExecutionContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ctx);
    }

    #[test]
    fn a_disabled_integration_is_a_failure_naming_the_integration_while_a_skip_is_not() {
        let twitch = IntegrationId::from_static("twitch");
        let cases = [
            (SubActionOutcome::Success, None),
            (
                SubActionOutcome::Failed(VerbatimText::new("timeout").into()),
                Some(VerbatimText::new("timeout").into()),
            ),
            (
                SubActionOutcome::Skipped(VerbatimText::new("disabled").into()),
                None,
            ),
            (
                SubActionOutcome::IntegrationDisabled(twitch.clone()),
                Some(UserText::from(integration_disabled_message(&twitch))),
            ),
        ];
        for (outcome, expected) in cases {
            assert_eq!(outcome.failure_reason(), expected, "{outcome:?}");
            assert_eq!(outcome.is_failure(), expected.is_some(), "{outcome:?}");
        }
        assert_eq!(
            integration_disabled_message(&twitch).arg("integration"),
            Some(&MessageArg::Integration(twitch.clone()))
        );
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
            ("%regex.matched%", "regex.matched"),
            ("a-b", "a-b"),
            (
                "%\u{434}\u{43e}\u{43d}\u{430}\u{442}%",
                "\u{434}\u{43e}\u{43d}\u{430}\u{442}",
            ),
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
            "has space",
            "%%x%%",
            "",
            "   ",
            "%",
            "a:b",
            "%a:b%",
            "a/b",
            "\u{438}\u{306}",
        ] {
            assert_eq!(normalize_var_name(bad), None, "input {bad:?}");
        }
    }

    #[test]
    fn every_name_normalize_accepts_reads_back_through_its_percent_token() {
        for raw in [
            "%regex.matched%",
            "  a-b  ",
            "%\u{434}\u{43e}\u{43d}\u{430}\u{442}%",
            "\u{540d}\u{524d}.\u{ad6d}",
            "trailing.",
        ] {
            let name = normalize_var_name(raw).unwrap();
            let stack = stack_with(&[(name.as_str(), text("stored"))]);

            assert_eq!(
                stack.interpolate(&format!("<%{name}%>")),
                "<stored>",
                "input {raw:?}"
            );
        }
    }

    #[test]
    fn a_nested_path_reaches_a_dotted_output_when_a_shorter_root_does_not_hold_it() {
        let name = normalize_var_name("%user.name%").unwrap();
        let stack = stack_with(&[
            ("user", object(&[("name", text("nested"))])),
            (name.as_str(), object(&[("first", text("flat"))])),
        ]);

        assert_eq!(stack.interpolate("%user.name.first%"), "flat");
    }

    fn telemetry_carrying_interpolated_text() -> SubActionTelemetry {
        SubActionTelemetry {
            position: StepPosition::TopLevel(0),
            kind: "SendChat".to_string(),
            started_at: OffsetDateTime::UNIX_EPOCH,
            duration_ms: 5,
            outcome: SubActionOutcome::Success,
            args_in: BTreeMap::from([("message".to_string(), "SENTINEL_BODY".to_string())]),
            produced: BTreeMap::from([("reply".to_string(), "SENTINEL_REPLY".to_string())]),
        }
    }

    #[test]
    fn sub_action_telemetry_debug_keeps_argument_keys_and_redacts_their_values() {
        let dbg = format!("{:?}", telemetry_carrying_interpolated_text());

        for verbatim in ["message", "reply", "SendChat", "duration_ms: 5"] {
            assert!(dbg.contains(verbatim), "{verbatim:?} missing from {dbg}");
        }
        for planted in ["SENTINEL_BODY", "SENTINEL_REPLY"] {
            assert!(!dbg.contains(planted), "{planted} leaked into {dbg}");
        }
        assert!(dbg.contains("<redacted len=13>"), "args_in value: {dbg}");
        assert!(dbg.contains("<redacted len=14>"), "produced value: {dbg}");
    }

    #[test]
    fn sub_action_telemetry_roundtrips_with_populated_argument_maps() {
        let telemetry = telemetry_carrying_interpolated_text();
        let json = serde_json::to_string(&telemetry).unwrap();
        let back: SubActionTelemetry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, telemetry);
    }

    #[test]
    fn debug_redaction_does_not_reach_the_display_paths() {
        let stack = stack_with(&[("body", Variant::String("SENTINEL_BODY".to_string()))]);
        assert_eq!(stack.interpolate("say %body%"), "say SENTINEL_BODY");
        assert_eq!(
            variant_preview(&Variant::String("SENTINEL_BODY".to_string())),
            "\"SENTINEL_BODY\""
        );
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
