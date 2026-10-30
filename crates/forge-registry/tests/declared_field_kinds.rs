#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::time::Duration;

use forge_registry::{
    AmountUnit, BlankableSelectField, ChainLayout, CodeField, CodeKind, CodeLanguage,
    ConfigProblem, ConfigReason, ConfigSpec, DateTimeField, DeclaredField, DependentSelectField,
    DurationBounds, DurationField, DynamicSelectField, FormField, Interpolation, JsonObjectField,
    LinesField, ListedChoiceField, OptionalField, OptionalSwitch, StepChainField, TemplateField,
    TextEditor, TextField, TypedField, UnitAmountBounds, UnitAmountField, ValueBounds, ValueField,
};
use forge_types::{ActionId, ArgStack, MessageCode, MessageRef, SubActionConfig, Variant};
use serde_json::json;
use time::OffsetDateTime;

const SCRIPT: CodeField = CodeField {
    key: "script",
    label: "Script",
    language: CodeLanguage::Rhai,
    kind: CodeKind::Script,
    default: "1 + 1",
    required: true,
};
const NOTES: CodeField = CodeField {
    key: "notes",
    label: "Notes",
    language: CodeLanguage::Json,
    kind: CodeKind::Expression,
    default: "",
    required: false,
};
const ACTION: DynamicSelectField<ActionId> =
    DynamicSelectField::new("action_id", "Action", "action.ids");
const MAYBE_ACTION: BlankableSelectField<ActionId> =
    BlankableSelectField::new("fallback_id", "Fallback", "action.ids");
const SOURCE: DependentSelectField = DependentSelectField {
    key: "source",
    label: "Source",
    options_prefix: "obs.sources.",
    depends_on: "scene",
};
const HOLD: DurationField = DurationField {
    key: "hold",
    label: "Hold",
    default: 5,
    bounds: DurationBounds::in_seconds(1, 60),
};
const GRACE: DurationField = DurationField {
    key: "grace",
    label: "Grace",
    default: 2,
    bounds: DurationBounds::in_minutes(1, 10),
};
const WHEN: DateTimeField = DateTimeField {
    key: "when",
    label: "When",
    default: "",
};
const BODY: StepChainField = StepChainField {
    key: "body",
    label: "Body",
    layout: ChainLayout::Steps,
    seeds_empty_list: true,
};
const CASES: StepChainField = StepChainField {
    key: "cases",
    label: "Cases",
    layout: ChainLayout::Cases,
    seeds_empty_list: false,
};
const PAYLOAD: JsonObjectField = JsonObjectField {
    key: "payload",
    label: "Payload",
    default: "{}",
};
const GREETING: TemplateField = TemplateField {
    key: "greeting",
    label: "Greeting",
    placeholder: "",
    default: "hi %who%",
    required: true,
};
const VALUE: ValueField = ValueField {
    key: "value",
    label: "Value",
    placeholder: "",
    default: "",
};
const ITEMS: LinesField = LinesField {
    key: "items",
    label: "Items",
};
const SLOT: ListedChoiceField = ListedChoiceField {
    key: "slot",
    label: "Slot",
    default: "donation",
    options: slots,
};
const MINUTES: AmountUnit = AmountUnit {
    value: "minutes",
    base_units: 60,
};
const HOURS: AmountUnit = AmountUnit {
    value: "hours",
    base_units: 3_600,
};
const DELAY_BOUNDS: UnitAmountBounds = UnitAmountBounds {
    min: 1,
    max_base_units: 7_200,
    units: &[MINUTES, HOURS],
};
const DELAY: UnitAmountField = UnitAmountField {
    key: "delay_amount",
    unit_key: "delay_unit",
    label: "Run after",
    bounds: DELAY_BOUNDS,
    default_amount: 10,
    default_unit: "minutes",
};
const LATE: OptionalField<DurationField> = OptionalField {
    field: GRACE,
    switch: OptionalSwitch::Toggle {
        key: "skip_late",
        label: "Skip when late",
        default_on: false,
    },
};
const CAPTION: OptionalField<TextField> = OptionalField {
    field: TextField {
        key: "caption",
        label: "Caption",
        editor: TextEditor::Line { placeholder: "" },
        default: "none",
        required: true,
    },
    switch: OptionalSwitch::Toggle {
        key: "show_caption",
        label: "Show a caption",
        default_on: true,
    },
};
const EVERY_KIND: ConfigSpec = ConfigSpec::new(&[
    &SCRIPT,
    &ACTION,
    &MAYBE_ACTION,
    &SOURCE,
    &HOLD,
    &WHEN,
    &BODY,
    &CASES,
    &PAYLOAD,
    &GREETING,
    &VALUE,
    &ITEMS,
    &SLOT,
    &DELAY,
    &LATE,
    &CAPTION,
]);

fn slots() -> &'static [&'static str] {
    &["donation", "now_playing"]
}

const ACTION_ULID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0AA";
const UNIX_SECS: i64 = 1_790_000_000;

fn instant() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(UNIX_SECS).unwrap()
}

fn action() -> ActionId {
    ACTION_ULID.parse().unwrap()
}

fn args() -> ArgStack {
    ArgStack::new()
        .set("who".to_owned(), text("ann"))
        .set("n".to_owned(), Variant::Int(7))
        .set("picked".to_owned(), text(ACTION_ULID))
        .set("nothing".to_owned(), text(""))
        .set("at".to_owned(), Variant::Datetime(instant()))
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn stored(key: &str, value: Variant) -> SubActionConfig {
    SubActionConfig::from([(key.to_owned(), value)])
}

fn read<F: TypedField>(field: &F, value: Variant) -> Result<F::Value, ConfigProblem> {
    field.read(&stored(field.key(), value), &args())
}

fn deferred<F: TypedField>(field: &F, value: Variant) -> Result<Option<F::Value>, ConfigProblem> {
    field.peek(&stored(field.key(), value), Interpolation::Deferred)
}

fn rejected<T>(key: &str, reason: ConfigReason) -> Result<T, ConfigProblem> {
    Err(ConfigProblem::field(key, reason))
}

fn config_code(name: &'static str) -> ConfigReason {
    ConfigReason::Coded(MessageRef::new(MessageCode::from_static("config", name)))
}

fn not_a_known_entry() -> ConfigReason {
    config_code("unknown_entry")
}

#[test]
fn every_field_kind_seeds_its_stored_default_shape() {
    let expected = SubActionConfig::from([
        ("script".to_owned(), text("1 + 1")),
        ("action_id".to_owned(), text("")),
        ("fallback_id".to_owned(), text("")),
        ("source".to_owned(), text("")),
        ("hold".to_owned(), Variant::Int(5)),
        ("when".to_owned(), text("")),
        ("body".to_owned(), Variant::Array(Vec::new())),
        ("payload".to_owned(), text("{}")),
        ("greeting".to_owned(), text("hi %who%")),
        ("value".to_owned(), text("")),
        ("items".to_owned(), text("")),
        ("slot".to_owned(), text("donation")),
        ("delay_amount".to_owned(), Variant::Int(10)),
        ("delay_unit".to_owned(), text("minutes")),
        ("skip_late".to_owned(), Variant::Bool(false)),
        ("grace".to_owned(), Variant::Int(2)),
        ("show_caption".to_owned(), Variant::Bool(true)),
        ("caption".to_owned(), text("none")),
    ]);

    assert_eq!(EVERY_KIND.default_config(), expected);
}

#[test]
fn every_field_kind_offers_its_own_editor() {
    let fields = EVERY_KIND.form_fields();
    let offered = |key: &str| {
        fields
            .iter()
            .find(|field| field.key() == key)
            .unwrap_or_else(|| panic!("no form field for {key}"))
            .clone()
    };

    assert!(matches!(
        offered("script"),
        FormField::Code {
            language: CodeLanguage::Rhai,
            kind: CodeKind::Script,
            ..
        }
    ));
    assert!(matches!(
        offered("action_id"),
        FormField::DynamicSelect {
            options_key: "action.ids",
            ..
        }
    ));
    assert!(matches!(
        offered("fallback_id"),
        FormField::DynamicSelect {
            options_key: "action.ids",
            ..
        }
    ));
    assert!(matches!(
        offered("source"),
        FormField::DependentSelect {
            options_prefix: "obs.sources.",
            depends_on: "scene",
            ..
        }
    ));
    assert!(matches!(
        offered("hold"),
        FormField::Duration { bounds, .. } if bounds == HOLD.bounds
    ));
    assert!(matches!(offered("when"), FormField::DateTime { .. }));
    assert!(matches!(offered("body"), FormField::SubChain { .. }));
    assert!(matches!(offered("cases"), FormField::CaseList { .. }));
    assert!(matches!(offered("payload"), FormField::TextArea { .. }));
    assert!(matches!(offered("greeting"), FormField::Text { .. }));
    assert!(matches!(offered("items"), FormField::TextArea { .. }));
    assert!(matches!(
        offered("slot"),
        FormField::Select {
            options: ["donation", "now_playing"],
            ..
        }
    ));
    assert!(matches!(
        offered("delay_amount"),
        FormField::UnitAmount {
            unit_key: "delay_unit",
            bounds,
            ..
        } if bounds == DELAY_BOUNDS
    ));
}

#[test]
fn every_field_kind_default_config_validates_once_required_selections_are_filled() {
    let mut config = EVERY_KIND.default_config();
    config.insert("action_id".to_owned(), text(ACTION_ULID));
    config.insert("source".to_owned(), text("Camera"));
    config.insert("when".to_owned(), text("2026-10-29T12:00:00Z"));

    assert_eq!(
        EVERY_KIND.validate(&config, Interpolation::Deferred),
        Ok(())
    );
}

#[test]
fn code_field_returns_the_text_raw_without_interpolating() {
    for value in ["%who% + 1", "  let x = 1;\n", "100%"] {
        assert_eq!(
            read(&SCRIPT, text(value)),
            Ok(value.to_owned()),
            "{value:?}"
        );
    }
}

#[test]
fn code_field_refuses_blank_only_when_required_and_never_accepts_non_text() {
    for (field, value, expected) in [
        (
            &SCRIPT,
            text(" \n "),
            rejected("script", ConfigReason::Required),
        ),
        (&NOTES, text(" \n "), Ok(" \n ".to_owned())),
        (
            &SCRIPT,
            Variant::Int(1),
            rejected("script", ConfigReason::NotText),
        ),
        (
            &NOTES,
            Variant::Bool(true),
            rejected("notes", ConfigReason::NotText),
        ),
    ] {
        assert_eq!(
            read(field, value.clone()),
            expected,
            "{}: {value:?}",
            field.key
        );
    }
}

#[test]
fn dynamic_select_reads_a_trimmed_literal_or_an_interpolated_reference() {
    for value in [
        text(ACTION_ULID),
        text(&format!(" {ACTION_ULID}\t")),
        text("%picked%"),
    ] {
        assert_eq!(read(&ACTION, value.clone()), Ok(action()), "{value:?}");
    }
}

#[test]
fn dynamic_select_refuses_blank_unknown_and_non_text_without_echoing_the_value() {
    for (value, reason) in [
        (text(""), ConfigReason::Required),
        (text("   "), ConfigReason::Required),
        (text("%nothing%"), ConfigReason::Required),
        (text("not-an-action-typed-by-a-viewer"), not_a_known_entry()),
        (text("%missing%"), not_a_known_entry()),
        (Variant::Int(3), ConfigReason::NotText),
    ] {
        let result = read(&ACTION, value.clone());
        assert_eq!(result, rejected("action_id", reason), "{value:?}");
        assert!(
            !result.unwrap_err().to_string().contains("viewer"),
            "{value:?}"
        );
    }
}

#[test]
fn dynamic_select_defers_a_reference_but_refuses_an_unknown_literal_while_editing() {
    assert_eq!(
        (
            deferred(&ACTION, text("%picked%")),
            deferred(&ACTION, text("nope")),
        ),
        (Ok(None), rejected("action_id", not_a_known_entry())),
    );
}

#[test]
fn dynamic_select_with_a_default_reads_it_when_the_key_is_absent() {
    let with_default = ACTION.with_default(ACTION_ULID);

    assert_eq!(
        with_default.read(&SubActionConfig::new(), &args()),
        Ok(action())
    );
}

#[test]
fn blankable_select_reads_blank_as_none_and_a_known_entry_as_some() {
    for (value, expected) in [
        (text(""), Ok(None)),
        (text("  "), Ok(None)),
        (text("%nothing%"), Ok(None)),
        (text(ACTION_ULID), Ok(Some(action()))),
        (text("%picked%"), Ok(Some(action()))),
        (text("nope"), rejected("fallback_id", not_a_known_entry())),
        (
            Variant::Bool(false),
            rejected("fallback_id", ConfigReason::NotText),
        ),
    ] {
        assert_eq!(read(&MAYBE_ACTION, value.clone()), expected, "{value:?}");
    }
}

#[test]
fn dependent_select_reads_trimmed_text_and_refuses_blank_or_non_text() {
    for (value, expected) in [
        (text(" Camera "), Ok("Camera".to_owned())),
        (text("%who% cam"), Ok("ann cam".to_owned())),
        (text(""), rejected("source", ConfigReason::Required)),
        (
            text("%nothing%"),
            rejected("source", ConfigReason::Required),
        ),
        (Variant::Int(2), rejected("source", ConfigReason::NotText)),
    ] {
        assert_eq!(read(&SOURCE, value.clone()), expected, "{value:?}");
    }
}

#[test]
fn duration_field_converts_its_base_unit_and_admits_both_bounds() {
    for (field, value, expected) in [
        (&HOLD, Variant::Int(1), Duration::from_secs(1)),
        (&HOLD, Variant::Int(60), Duration::from_secs(60)),
        (&HOLD, text(" 30 "), Duration::from_secs(30)),
        (&HOLD, text("%n%"), Duration::from_secs(7)),
        (&GRACE, Variant::Int(2), Duration::from_secs(120)),
        (&GRACE, Variant::Int(10), Duration::from_secs(600)),
    ] {
        assert_eq!(
            read(field, value.clone()),
            Ok(expected),
            "{}: {value:?}",
            field.key
        );
    }
}

#[test]
fn duration_field_fails_one_past_each_bound_instead_of_clamping() {
    let out_of_range = || {
        rejected(
            "hold",
            ConfigReason::OutOfRange(ValueBounds::between(1, 60)),
        )
    };
    for value in [
        Variant::Int(0),
        Variant::Int(61),
        Variant::Int(-1),
        Variant::Int(i64::MIN),
        text("61"),
    ] {
        assert_eq!(read(&HOLD, value.clone()), out_of_range(), "{value:?}");
    }
}

#[test]
fn duration_field_refuses_blank_unparseable_and_non_whole_values() {
    for (value, reason) in [
        (text(""), ConfigReason::Required),
        (text("%nothing%"), ConfigReason::Required),
        (text("five"), ConfigReason::NotANumber),
        (text("1.5"), ConfigReason::NotANumber),
        (Variant::Float(1.0), ConfigReason::NotANumber),
        (Variant::Bool(true), ConfigReason::NotANumber),
    ] {
        assert_eq!(
            read(&HOLD, value.clone()),
            rejected("hold", reason),
            "{value:?}"
        );
    }
}

#[test]
fn duration_field_defers_a_reference_while_editing_and_checks_it_at_run_time() {
    let big = stored("hold", text("%n%"));
    let too_big = ArgStack::new().set("n".to_owned(), Variant::Int(61));

    assert_eq!(
        (
            HOLD.peek(&big, Interpolation::Deferred),
            HOLD.peek(&big, Interpolation::Against(&too_big)),
        ),
        (
            Ok(None),
            rejected(
                "hold",
                ConfigReason::OutOfRange(ValueBounds::between(1, 60))
            ),
        ),
    );
}

#[test]
fn date_time_field_reads_every_accepted_spelling_of_the_same_instant() {
    for value in [
        Variant::Datetime(instant()),
        Variant::Int(UNIX_SECS),
        text("2026-09-21T14:13:20Z"),
        text(&format!(" {UNIX_SECS} ")),
        text("%at%"),
    ] {
        assert_eq!(read(&WHEN, value.clone()), Ok(instant()), "{value:?}");
    }
}

#[test]
fn date_time_field_refuses_blank_unreadable_out_of_range_and_non_text() {
    for (value, reason) in [
        (text(""), ConfigReason::Required),
        (text("%nothing%"), ConfigReason::Required),
        (text("next tuesday"), config_code("not_a_datetime")),
        (Variant::Bool(true), ConfigReason::NotText),
        (
            Variant::Int(i64::MAX),
            config_code("timestamp_out_of_range"),
        ),
    ] {
        assert_eq!(
            read(&WHEN, value.clone()),
            rejected("when", reason),
            "{value:?}"
        );
    }
}

#[test]
fn step_chain_field_accepts_an_absent_or_listed_chain_and_refuses_anything_else() {
    let not_a_list = || Err(ConfigProblem::field("body", config_code("not_a_step_list")));
    for (config, expected) in [
        (SubActionConfig::new(), Ok(())),
        (stored("body", Variant::Array(Vec::new())), Ok(())),
        (stored("body", Variant::Int(7)), not_a_list()),
        (stored("body", text("[]")), not_a_list()),
    ] {
        assert_eq!(
            BODY.check(&config, Interpolation::Deferred),
            expected,
            "{config:?}"
        );
    }
}

#[test]
fn json_object_field_parses_nested_objects_and_keeps_nulls() {
    assert_eq!(
        read(&PAYLOAD, text(r#"{"a": null, "b": {"c": [1, "x"]}}"#)),
        Ok(json!({"a": null, "b": {"c": [1, "x"]}})
            .as_object()
            .unwrap()
            .clone())
    );
}

#[test]
fn json_object_field_keeps_variable_references_verbatim() {
    assert_eq!(
        read(&PAYLOAD, text(r#"{"who": "%who%"}"#)),
        Ok(json!({"who": "%who%"}).as_object().unwrap().clone())
    );
}

#[test]
fn json_object_field_reads_blank_text_as_an_empty_object() {
    for value in ["", "  \n"] {
        assert_eq!(
            read(&PAYLOAD, text(value)),
            Ok(serde_json::Map::new()),
            "{value:?}"
        );
    }
}

#[test]
fn json_object_field_accepts_a_stored_object() {
    let stored_object = Variant::Object(BTreeMap::from([
        ("n".to_owned(), Variant::Int(2)),
        ("tags".to_owned(), Variant::Array(vec![text("a")])),
    ]));

    assert_eq!(
        read(&PAYLOAD, stored_object),
        Ok(json!({"n": 2, "tags": ["a"]}).as_object().unwrap().clone())
    );
}

#[test]
fn json_object_field_refuses_non_objects_invalid_json_and_non_text() {
    let not_an_object = || rejected("payload", config_code("not_a_json_object"));
    for (value, expected) in [
        (text("[1]"), not_an_object()),
        (text("null"), not_an_object()),
        (text("\"x\""), not_an_object()),
        (Variant::Int(1), rejected("payload", ConfigReason::NotText)),
        (
            Variant::Array(Vec::new()),
            rejected("payload", ConfigReason::NotText),
        ),
    ] {
        assert_eq!(read(&PAYLOAD, value.clone()), expected, "{value:?}");
    }
    assert!(matches!(
        read(&PAYLOAD, text("object{0}")),
        Err(ConfigProblem {
            reason: ConfigReason::Coded(message),
            ..
        }) if message.code().name() == "not_json" && message.detail().is_some()
    ));
}

#[test]
fn template_field_returns_the_text_uninterpolated() {
    assert_eq!(read(&GREETING, text("hi %who%")), Ok("hi %who%".to_owned()));
}

#[test]
fn template_field_refuses_blank_when_required_and_non_text() {
    for (value, reason) in [
        (text("  "), ConfigReason::Required),
        (Variant::Int(1), ConfigReason::NotText),
    ] {
        assert_eq!(
            read(&GREETING, value.clone()),
            rejected("greeting", reason),
            "{value:?}"
        );
    }
}

#[test]
fn value_field_keeps_typed_values_and_interpolates_references_into_text() {
    for (value, expected) in [
        (Variant::Int(3), Variant::Int(3)),
        (Variant::Bool(false), Variant::Bool(false)),
        (text("plain"), text("plain")),
        (text("%who%!"), text("ann!")),
        (text("%n%"), text("7")),
    ] {
        assert_eq!(read(&VALUE, value.clone()), Ok(expected), "{value:?}");
    }
}

#[test]
fn value_field_defers_a_reference_while_editing() {
    assert_eq!(deferred(&VALUE, text("%who%")), Ok(None));
}

#[test]
fn lines_field_splits_text_into_one_entry_per_line() {
    for (value, expected) in [
        ("a\nb", vec![text("a"), text("b")]),
        ("a\r\n\nb\n", vec![text("a"), text(""), text("b")]),
        ("", Vec::new()),
        ("%who%", vec![text("%who%")]),
    ] {
        assert_eq!(read(&ITEMS, text(value)), Ok(expected), "{value:?}");
    }
}

#[test]
fn lines_field_keeps_a_stored_array_and_refuses_other_shapes() {
    let entries = vec![Variant::Int(1), text("x")];

    assert_eq!(
        (
            read(&ITEMS, Variant::Array(entries.clone())),
            read(&ITEMS, Variant::Int(1)),
        ),
        (Ok(entries), rejected("items", ConfigReason::NotText)),
    );
}

#[test]
fn listed_choice_field_reads_exact_options_and_lists_them_for_anything_else() {
    let listing = || rejected("slot", ConfigReason::choices(["donation", "now_playing"]));
    for (value, expected) in [
        (text("now_playing"), Ok("now_playing")),
        (text(""), listing()),
        (text(" donation"), listing()),
        (text("Donation"), listing()),
        (text("%slot%"), listing()),
        (Variant::Int(0), listing()),
    ] {
        assert_eq!(read(&SLOT, value.clone()), expected, "{value:?}");
    }
}

#[test]
fn toggled_optional_field_reads_its_inner_value_only_while_switched_on() {
    let config = |on: Variant, grace: Variant| {
        SubActionConfig::from([("skip_late".to_owned(), on), ("grace".to_owned(), grace)])
    };
    for (stored_config, expected) in [
        (SubActionConfig::new(), Ok(None)),
        (config(Variant::Bool(false), Variant::Int(3)), Ok(None)),
        (config(Variant::Bool(false), text("not a number")), Ok(None)),
        (
            config(Variant::Bool(true), Variant::Int(3)),
            Ok(Some(Duration::from_secs(180))),
        ),
        (
            config(text("%switch%"), Variant::Int(3)),
            Ok(Some(Duration::from_secs(180))),
        ),
        (
            config(Variant::Bool(true), Variant::Int(11)),
            rejected(
                "grace",
                ConfigReason::OutOfRange(ValueBounds::between(1, 10)),
            ),
        ),
        (
            config(text("yes"), Variant::Int(3)),
            rejected("skip_late", ConfigReason::choices(["true", "false"])),
        ),
    ] {
        let args = ArgStack::new().set("switch".to_owned(), text("true"));
        assert_eq!(
            LATE.read(&stored_config, &args),
            expected,
            "{stored_config:?}"
        );
    }
}

#[test]
fn toggled_optional_field_switched_on_by_default_reads_its_inner_default() {
    assert_eq!(
        CAPTION.read(&SubActionConfig::new(), &ArgStack::new()),
        Ok(Some("none".to_owned()))
    );
}

#[test]
fn toggled_optional_field_defers_a_referenced_switch_while_editing() {
    let config = |on: Variant| {
        SubActionConfig::from([
            ("skip_late".to_owned(), on),
            ("grace".to_owned(), Variant::Int(3)),
        ])
    };

    assert_eq!(
        (
            LATE.peek(&config(text("%switch%")), Interpolation::Deferred),
            LATE.peek(&config(Variant::Bool(false)), Interpolation::Deferred),
            LATE.peek(&config(Variant::Bool(true)), Interpolation::Deferred),
        ),
        (
            Ok(None),
            Ok(Some(None)),
            Ok(Some(Some(Duration::from_secs(180))))
        ),
    );
}

#[test]
fn toggled_optional_field_offers_its_switch_around_the_inner_editor() {
    let FormField::Optional { key, label, inner } = TypedField::form_field(&LATE) else {
        panic!("expected an optional form field");
    };

    assert_eq!(
        (key, label, inner.key()),
        ("skip_late", "Skip when late", "grace")
    );
}

#[test]
fn shared_optional_field_takes_an_explicit_label_over_the_inner_one() {
    let relabelled = OptionalField {
        field: GREETING,
        switch: OptionalSwitch::SharedKey {
            label: Some("Custom greeting"),
        },
    };
    let FormField::Optional { key, label, .. } = TypedField::form_field(&relabelled) else {
        panic!("expected an optional form field");
    };

    assert_eq!((key, label), ("greeting", "Custom greeting"));
}

#[test]
fn unit_amount_field_scales_the_amount_by_its_unit() {
    let config = |amount: Variant, unit: Option<&str>| {
        let mut config = stored("delay_amount", amount);
        if let Some(unit) = unit {
            config.insert("delay_unit".to_owned(), text(unit));
        }
        config
    };
    for (stored_config, expected) in [
        (config(Variant::Int(3), None), 180),
        (config(Variant::Int(1), Some("minutes")), 60),
        (config(Variant::Int(120), Some("minutes")), 7_200),
        (config(Variant::Int(2), Some("hours")), 7_200),
        (config(text("%n%"), Some("minutes")), 420),
    ] {
        assert_eq!(
            DELAY.read(&stored_config, &args()),
            Ok(expected),
            "{stored_config:?}"
        );
    }
}

#[test]
fn unit_amount_field_refuses_an_amount_outside_the_range_of_its_unit() {
    for (amount, unit, max) in [
        (0, "minutes", 120),
        (121, "minutes", 120),
        (-1, "hours", 2),
        (3, "hours", 2),
    ] {
        let config = SubActionConfig::from([
            ("delay_amount".to_owned(), Variant::Int(amount)),
            ("delay_unit".to_owned(), text(unit)),
        ]);
        assert_eq!(
            DELAY.read(&config, &args()),
            rejected(
                "delay_amount",
                ConfigReason::OutOfRange(ValueBounds::between(1, max))
            ),
            "{amount} {unit}"
        );
    }
}

#[test]
fn unit_amount_field_refuses_a_unit_it_does_not_offer() {
    for unit in [
        text("weeks"),
        text("Minutes"),
        text("%unit%"),
        Variant::Int(60),
    ] {
        let config = SubActionConfig::from([
            ("delay_amount".to_owned(), Variant::Int(1)),
            ("delay_unit".to_owned(), unit.clone()),
        ]);
        assert_eq!(
            DELAY.read(&config, &args()),
            rejected("delay_unit", ConfigReason::choices(["minutes", "hours"])),
            "{unit:?}"
        );
    }
}

#[test]
fn unit_amount_field_defers_a_referenced_amount_while_editing() {
    let config = SubActionConfig::from([
        ("delay_amount".to_owned(), text("%n%")),
        ("delay_unit".to_owned(), text("hours")),
    ]);

    assert_eq!(DELAY.check(&config, Interpolation::Deferred), Ok(()));
}
