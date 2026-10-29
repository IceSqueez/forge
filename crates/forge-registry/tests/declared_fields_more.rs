#![allow(clippy::unwrap_used)]
use forge_registry::{
    BoundedTextField, ConfigProblem, ConfigReason, FormField, Interpolation, SeparatedListField,
    TextEditor, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionConfig, Variant};

const NICK: BoundedTextField = BoundedTextField {
    key: "nick",
    label: "Nick",
    editor: TextEditor::Line { placeholder: "" },
    default: "anon",
    required: true,
    length: ValueBounds {
        min: Some(2),
        max: Some(5),
    },
};

const NOTE: BoundedTextField = BoundedTextField {
    key: "note",
    label: "Note",
    editor: TextEditor::Area,
    default: "",
    required: false,
    length: ValueBounds {
        min: None,
        max: Some(3),
    },
};

const TAGS: SeparatedListField = SeparatedListField {
    key: "tags",
    label: "Tags",
    default: "",
    count: ValueBounds {
        min: None,
        max: Some(3),
    },
    item_length: ValueBounds {
        min: None,
        max: Some(4),
    },
};

const PAIR: SeparatedListField = SeparatedListField {
    key: "pair",
    label: "Pair",
    default: "a,b",
    count: ValueBounds {
        min: Some(2),
        max: Some(2),
    },
    item_length: ValueBounds::UNBOUNDED,
};

fn stored(key: &str, value: Variant) -> SubActionConfig {
    SubActionConfig::from([(key.to_owned(), value)])
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn args(name: &str, value: &str) -> ArgStack {
    ArgStack::new().set(name.to_owned(), text(value))
}

fn read_literal<F: TypedField>(field: &F, value: Variant) -> Result<F::Value, ConfigProblem> {
    field.read(&stored(field.key(), value), &ArgStack::new())
}

fn reason<T: std::fmt::Debug>(result: Result<T, ConfigProblem>, key: &str) -> ConfigReason {
    let problem = result.unwrap_err();
    assert_eq!(problem.field.as_deref(), Some(key));
    problem.reason
}

#[test]
fn bounded_text_admits_literals_from_the_minimum_to_the_maximum_length() {
    for literal in ["ab", "abcd", "abcde", "їжак"] {
        assert_eq!(read_literal(&NICK, text(literal)).unwrap(), literal);
    }
}

#[test]
fn bounded_text_refuses_literals_one_outside_the_length_bounds() {
    for literal in ["a", "abcdef", "їжачок"] {
        assert_eq!(
            reason(read_literal(&NICK, text(literal)), "nick"),
            ConfigReason::LengthOutOfRange(NICK.length),
            "{literal:?}"
        );
    }
}

#[test]
fn bounded_text_counts_characters_not_bytes() {
    assert_eq!(read_literal(&NOTE, text("їїї")).unwrap(), "їїї");
}

#[test]
fn required_bounded_text_refuses_blank_and_whitespace_only_values() {
    for blank in ["", "   ", "\t\n"] {
        assert_eq!(
            reason(read_literal(&NICK, text(blank)), "nick"),
            ConfigReason::Required,
            "{blank:?}"
        );
    }
}

#[test]
fn optional_bounded_text_admits_an_empty_value() {
    assert_eq!(read_literal(&NOTE, text("")).unwrap(), "");
}

#[test]
fn bounded_text_refuses_a_non_text_value() {
    assert_eq!(
        reason(read_literal(&NICK, Variant::Int(7)), "nick"),
        ConfigReason::NotText
    );
}

#[test]
fn bounded_text_reads_a_missing_key_as_its_default() {
    assert_eq!(
        NICK.read(&SubActionConfig::new(), &ArgStack::new())
            .unwrap(),
        "anon"
    );
}

#[test]
fn bounded_text_defers_the_length_check_of_a_template_to_run_time() {
    let config = stored("nick", text("x%who%"));

    assert_eq!(NICK.peek(&config, Interpolation::Deferred), Ok(None));
    assert_eq!(NICK.read(&config, &args("who", "yz")).unwrap(), "xyz");
    assert_eq!(
        reason(NICK.read(&config, &args("who", "yzabc")), "nick"),
        ConfigReason::LengthOutOfRange(NICK.length)
    );
}

#[test]
fn bounded_text_checks_the_interpolated_length_when_validating_against_arguments() {
    let config = stored("note", text("%who%"));

    assert_eq!(
        NOTE.peek(&config, Interpolation::Against(&args("who", "four"))),
        Err(ConfigProblem::field(
            "note",
            ConfigReason::LengthOutOfRange(NOTE.length)
        ))
    );
}

#[test]
fn bounded_text_form_field_follows_its_editor() {
    assert!(matches!(
        NICK.form_field(),
        FormField::Text { key: "nick", .. }
    ));
    assert!(matches!(
        NOTE.form_field(),
        FormField::TextArea { key: "note", .. }
    ));
}

#[test]
fn separated_list_splits_on_newlines_and_commas_trimming_and_dropping_blanks() {
    for (raw, expected) in [
        (" ab ,\n cd\n\n, ,ef ", vec!["ab", "cd", "ef"]),
        ("ab\r\ncd", vec!["ab", "cd"]),
        ("їж,ак", vec!["їж", "ак"]),
        ("", vec![]),
        (" ,\n , ", vec![]),
    ] {
        assert_eq!(read_literal(&TAGS, text(raw)).unwrap(), expected, "{raw:?}");
    }
}

#[test]
fn separated_list_item_length_bound_is_inclusive() {
    assert_eq!(read_literal(&TAGS, text("abcd")).unwrap(), vec!["abcd"]);
    assert_eq!(
        reason(read_literal(&TAGS, text("ok,abcde")), "tags"),
        ConfigReason::LengthOutOfRange(TAGS.item_length)
    );
}

#[test]
fn separated_list_item_count_bounds_are_inclusive() {
    assert_eq!(read_literal(&TAGS, text("a,b,c")).unwrap().len(), 3);
    assert_eq!(
        reason(read_literal(&TAGS, text("a,b,c,d")), "tags"),
        ConfigReason::CountOutOfRange(TAGS.count)
    );
    assert_eq!(
        reason(read_literal(&PAIR, text("solo")), "pair"),
        ConfigReason::CountOutOfRange(PAIR.count)
    );
}

#[test]
fn separated_list_refuses_a_non_text_value() {
    assert_eq!(
        reason(read_literal(&TAGS, Variant::Int(1)), "tags"),
        ConfigReason::NotText
    );
}

#[test]
fn separated_list_reads_a_missing_key_as_its_default() {
    assert_eq!(
        PAIR.read(&SubActionConfig::new(), &ArgStack::new())
            .unwrap(),
        vec!["a", "b"]
    );
}

#[test]
fn separated_list_splits_and_bounds_a_template_after_interpolation() {
    let config = stored("tags", text("x,%more%"));

    assert_eq!(TAGS.peek(&config, Interpolation::Deferred), Ok(None));
    assert_eq!(
        TAGS.read(&config, &args("more", "y\nz")).unwrap(),
        vec!["x", "y", "z"]
    );
    assert_eq!(
        reason(TAGS.read(&config, &args("more", "y,z,w")), "tags"),
        ConfigReason::CountOutOfRange(TAGS.count)
    );
}

#[test]
fn separated_list_edits_in_a_text_area() {
    assert!(matches!(
        TAGS.form_field(),
        FormField::TextArea { key: "tags", .. }
    ));
}
