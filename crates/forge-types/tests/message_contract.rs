#![allow(clippy::unwrap_used)]
use std::collections::BTreeSet;
use std::time::Duration;

use forge_types::{
    COMMON_MESSAGES, FiniteFloat, IntegrationId, MAX_UPSTREAM_DETAIL_CHARS, MessageArg,
    MessageCatalog, MessageCatalogError, MessageCode, MessageRef, PlatformId, UPSTREAM_FAILURE,
    UpstreamDetail, UserText, VariantKind, VerbatimText,
};
use serde_json::{Value, json};
use time::OffsetDateTime;

const FAKE_TOKEN: &str = "oauth:s3cr3tT0kenValue9981";

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

fn round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    serde_json::from_value(to_json(value)).unwrap()
}

#[test]
fn code_parse_accepts_well_formed_inputs() {
    for (raw, namespace, name) in [
        ("a.b", "a", "b"),
        ("obs.scene_missing", "obs", "scene_missing"),
        ("x1.y_2", "x1", "y_2"),
    ] {
        let code = MessageCode::parse(raw).unwrap();
        assert_eq!((code.namespace(), code.name()), (namespace, name), "{raw}");
        assert!(code.is_well_formed(), "{raw}");
    }
}

#[test]
fn code_parse_rejects_malformed_inputs() {
    for raw in [
        "", "nodot", ".x", "x.", "A.b", "a-b.c", "1a.b", "a.b-c", "a b.c", "a.b.c", "_a.b", "a.Ab",
    ] {
        assert!(MessageCode::parse(raw).is_err(), "accepted {raw:?}");
    }
}

#[test]
fn code_display_and_serde_round_trip_through_dotted_string() {
    let code = MessageCode::parse("obs.scene_missing").unwrap();
    assert_eq!(code.to_string(), "obs.scene_missing");
    assert_eq!(to_json(&code), json!("obs.scene_missing"));
    assert_eq!(round_trip(&code), code);
}

#[test]
fn code_deserialize_rejects_malformed_string() {
    for raw in ["nodot", "A.b", "a.b.c", ""] {
        assert!(
            serde_json::from_value::<MessageCode>(json!(raw)).is_err(),
            "{raw:?}"
        );
    }
    assert!(serde_json::from_value::<MessageCode>(json!(5)).is_err());
}

#[test]
fn code_ftl_key_is_prefixed_and_distinct_per_code() {
    let a = MessageCode::parse("obs.scene_missing").unwrap();
    let b = MessageCode::parse("obs.scene_missing_x").unwrap();
    let c = MessageCode::parse("obs_scene.missing").unwrap();
    assert_eq!(a.ftl_key(), "msg-obs-scene_missing");
    let keys: BTreeSet<_> = [&a, &b, &c].iter().map(|code| code.ftl_key()).collect();
    assert_eq!(keys.len(), 3);
}

#[test]
fn code_from_static_with_malformed_segments_is_not_well_formed() {
    for (namespace, name) in [
        ("Bad", "x"),
        ("a", "b-c"),
        ("", "x"),
        ("a", ""),
        ("a", "1b"),
    ] {
        assert!(
            !MessageCode::from_static(namespace, name).is_well_formed(),
            "{namespace}.{name}"
        );
    }
    assert!(MessageCode::from_static("a", "b").is_well_formed());
}

#[test]
fn finite_float_rejects_non_finite_values() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(FiniteFloat::new(value).is_none(), "{value}");
    }
    assert_eq!(FiniteFloat::new(-1.5).unwrap().get(), -1.5);
}

#[test]
fn finite_float_deserialize_rejects_null_and_non_numbers() {
    assert!(serde_json::from_value::<FiniteFloat>(Value::Null).is_err());
    assert!(serde_json::from_value::<FiniteFloat>(json!("1.0")).is_err());
    assert_eq!(
        serde_json::from_value::<FiniteFloat>(json!(2.5))
            .unwrap()
            .get(),
        2.5
    );
}

#[test]
fn arg_from_non_finite_f64_falls_back_to_verbatim_text() {
    assert_eq!(
        MessageArg::from(f64::NAN),
        MessageArg::Verbatim(VerbatimText::new("NaN"))
    );
    assert_eq!(
        MessageArg::from(f64::NEG_INFINITY),
        MessageArg::Verbatim(VerbatimText::new("-inf"))
    );
    assert_eq!(
        MessageArg::from(1.5),
        MessageArg::Float(FiniteFloat::new(1.5).unwrap())
    );
}

#[test]
fn arg_from_unsigned_overflow_clamps_to_i64_max() {
    assert_eq!(MessageArg::from(u64::MAX), MessageArg::Int(i64::MAX));
    assert_eq!(MessageArg::from(usize::MAX), MessageArg::Int(i64::MAX));
    assert_eq!(MessageArg::from(i64::MAX as u64), MessageArg::Int(i64::MAX));
    assert_eq!(MessageArg::from(7_u64), MessageArg::Int(7));
    assert_eq!(MessageArg::from(7_usize), MessageArg::Int(7));
    assert_eq!(MessageArg::from(-3_i32), MessageArg::Int(-3));
    assert_eq!(
        MessageArg::from(u32::MAX),
        MessageArg::Int(i64::from(u32::MAX))
    );
}

#[test]
fn arg_variants_serialize_to_tagged_shapes_and_round_trip() {
    let moment = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let cases: Vec<(MessageArg, Value)> = vec![
        (MessageArg::Int(-4), json!({"int": -4})),
        (MessageArg::from(2.5), json!({"float": 2.5})),
        (
            MessageArg::Datetime(moment),
            json!({"datetime": "2023-11-14T22:13:20Z"}),
        ),
        (
            MessageArg::Duration(Duration::new(3, 5)),
            json!({"duration": {"secs": 3, "nanos": 5}}),
        ),
        (
            MessageArg::Verbatim(VerbatimText::new("hi")),
            json!({"verbatim": "hi"}),
        ),
        (
            MessageArg::Integration(IntegrationId::new("obs")),
            json!({"integration": "obs"}),
        ),
        (
            MessageArg::Platform(PlatformId::YouTube),
            json!({"platform": "youtube"}),
        ),
        (
            MessageArg::from(VariantKind::Datetime),
            json!({"value_kind": "datetime"}),
        ),
        (
            MessageArg::StepFields {
                kind: "core.wait".to_owned(),
                keys: vec!["from".to_owned(), "to".to_owned()],
            },
            json!({"step_fields": {"kind": "core.wait", "keys": ["from", "to"]}}),
        ),
        (
            MessageArg::from(MessageRef::new(sample_code()).with_arg("n", 2_i64)),
            json!({"message": {"code": "obs.scene_missing", "args": {"n": {"int": 2}}}}),
        ),
    ];
    for (arg, expected) in cases {
        assert_eq!(to_json(&arg), expected, "{arg:?}");
        assert_eq!(round_trip(&arg), arg);
    }
}

#[test]
fn arg_deserialize_rejects_non_finite_float_payload_and_unknown_variant() {
    assert!(serde_json::from_value::<MessageArg>(json!({"float": null})).is_err());
    assert!(serde_json::from_value::<MessageArg>(json!({"unknown": 1})).is_err());
}

#[test]
fn detail_is_trimmed() {
    assert_eq!(UpstreamDetail::from_redacted("  boom \n").as_str(), "boom");
    assert_eq!(UpstreamDetail::from_redacted("").as_str(), "");
    assert_eq!(UpstreamDetail::from_redacted("   ").as_str(), "");
}

#[test]
fn detail_at_limit_is_kept_and_one_over_is_truncated_with_marker() {
    let at_limit = "a".repeat(MAX_UPSTREAM_DETAIL_CHARS);
    assert_eq!(UpstreamDetail::from_redacted(&at_limit).as_str(), at_limit);

    let over = "a".repeat(MAX_UPSTREAM_DETAIL_CHARS + 1);
    let cut = UpstreamDetail::from_redacted(&over);
    assert_eq!(
        cut.as_str(),
        format!("{}...", "a".repeat(MAX_UPSTREAM_DETAIL_CHARS))
    );
}

#[test]
fn detail_truncation_counts_chars_and_respects_multibyte_boundaries() {
    let input = "я".repeat(MAX_UPSTREAM_DETAIL_CHARS + 10);
    let cut = UpstreamDetail::from_redacted(input);
    assert_eq!(
        cut.as_str(),
        format!("{}...", "я".repeat(MAX_UPSTREAM_DETAIL_CHARS))
    );

    let exact = "я".repeat(MAX_UPSTREAM_DETAIL_CHARS);
    assert_eq!(UpstreamDetail::from_redacted(exact.clone()).as_str(), exact);
}

#[test]
fn detail_trims_before_measuring_length() {
    let padded = format!("  {}  ", "b".repeat(MAX_UPSTREAM_DETAIL_CHARS));
    assert_eq!(
        UpstreamDetail::from_redacted(padded).as_str(),
        "b".repeat(MAX_UPSTREAM_DETAIL_CHARS)
    );
}

#[test]
fn detail_deserialize_caps_overlong_input() {
    let raw = "z".repeat(MAX_UPSTREAM_DETAIL_CHARS * 3);
    let detail: UpstreamDetail = serde_json::from_value(json!(raw)).unwrap();
    assert_eq!(
        detail.as_str().chars().count(),
        MAX_UPSTREAM_DETAIL_CHARS + 3
    );
    assert!(detail.as_str().ends_with("..."));
}

#[test]
fn detail_serializes_as_plain_string() {
    assert_eq!(to_json(&UpstreamDetail::from_redacted("x")), json!("x"));
}

#[test]
fn detail_debug_shows_only_length() {
    let detail = UpstreamDetail::from_redacted(FAKE_TOKEN);
    let shown = format!("{detail:?}");
    assert_eq!(
        shown,
        format!(
            "UpstreamDetail(<redacted len={}>)",
            FAKE_TOKEN.chars().count()
        )
    );
    assert!(!shown.contains("s3cr3t"));
}

#[test]
fn verbatim_debug_is_redacted_and_serde_is_transparent() {
    let text = VerbatimText::new(FAKE_TOKEN);
    let shown = format!("{text:?}");
    assert_eq!(
        shown,
        format!(
            "VerbatimText(<redacted len={}>)",
            FAKE_TOKEN.chars().count()
        )
    );
    assert!(!shown.contains("s3cr3t"));
    assert_eq!(to_json(&text), json!(FAKE_TOKEN));
    assert_eq!(round_trip(&text), text);
    assert_eq!(text.into_string(), FAKE_TOKEN);
}

fn sample_code() -> MessageCode {
    MessageCode::parse("obs.scene_missing").unwrap()
}

#[test]
fn ref_with_args_and_detail_round_trips() {
    let message = MessageRef::new(sample_code())
        .with_arg("count", 3_i64)
        .with_arg("who", VerbatimText::new("neo"))
        .with_detail(UpstreamDetail::from_redacted("boom"));
    assert_eq!(
        to_json(&message),
        json!({
            "code": "obs.scene_missing",
            "args": {"count": {"int": 3}, "who": {"verbatim": "neo"}},
            "detail": "boom",
        })
    );
    assert_eq!(round_trip(&message), message);
}

#[test]
fn ref_omits_empty_args_and_detail_and_defaults_them_back() {
    let message = MessageRef::new(sample_code());
    assert_eq!(to_json(&message), json!({"code": "obs.scene_missing"}));
    let back: MessageRef = serde_json::from_value(json!({"code": "obs.scene_missing"})).unwrap();
    assert_eq!(back, message);
    assert!(back.detail().is_none());
    assert_eq!(back.args().count(), 0);
}

#[test]
fn ref_with_arg_replaces_existing_value_for_same_name() {
    let message = MessageRef::new(sample_code())
        .with_arg("n", 1_i64)
        .with_arg("n", 2_i64);
    assert_eq!(message.arg("n"), Some(&MessageArg::Int(2)));
    assert_eq!(message.args().count(), 1);
}

#[test]
fn ref_args_iterate_sorted_by_name() {
    let message = MessageRef::new(sample_code())
        .with_arg("zeta", 1_i64)
        .with_arg("alpha", 2_i64)
        .with_arg("mid", 3_i64);
    let names: Vec<_> = message.args().map(|(name, _)| name).collect();
    assert_eq!(names, ["alpha", "mid", "zeta"]);
    assert!(message.arg("missing").is_none());
}

#[test]
fn ref_upstream_failure_uses_common_code_with_detail() {
    let message = MessageRef::upstream_failure(UpstreamDetail::from_redacted("502"));
    assert_eq!(message.code(), &UPSTREAM_FAILURE);
    assert_eq!(message.detail().unwrap().as_str(), "502");
    assert_eq!(message.args().count(), 0);
}

#[test]
fn ref_deserialize_rejects_malformed_code_and_missing_code() {
    assert!(serde_json::from_value::<MessageRef>(json!({"code": "NoDot"})).is_err());
    assert!(serde_json::from_value::<MessageRef>(json!({})).is_err());
}

#[test]
fn ref_debug_leaks_neither_detail_nor_verbatim_arg() {
    let message = MessageRef::new(sample_code())
        .with_arg("who", VerbatimText::new(FAKE_TOKEN))
        .with_detail(UpstreamDetail::from_redacted(format!(
            "denied {FAKE_TOKEN}"
        )));
    let shown = format!("{message:?}");
    assert!(!shown.contains("s3cr3t"), "{shown}");
    assert!(!shown.contains("denied"), "{shown}");
    assert!(shown.contains("<redacted len="), "{shown}");
}

#[test]
fn user_text_serializes_as_externally_tagged_variants() {
    let message = UserText::from(MessageRef::new(sample_code()));
    assert_eq!(
        to_json(&message),
        json!({"message": {"code": "obs.scene_missing"}})
    );
    let verbatim = UserText::from(VerbatimText::new("hello"));
    assert_eq!(to_json(&verbatim), json!({"verbatim": "hello"}));
    assert_eq!(round_trip(&message), message);
    assert_eq!(round_trip(&verbatim), verbatim);
}

#[test]
fn user_text_debug_does_not_leak_verbatim_content() {
    let shown = format!("{:?}", UserText::from(VerbatimText::new(FAKE_TOKEN)));
    assert!(!shown.contains("s3cr3t"), "{shown}");
}

const DUPLICATED: &[MessageCode] = &[
    MessageCode::from_static("ns", "one"),
    MessageCode::from_static("ns", "one"),
];
const FOREIGN: &[MessageCode] = &[MessageCode::from_static("other", "one")];
const BAD_CODE: &[MessageCode] = &[MessageCode::from_static("ns", "Bad-Name")];
const GOOD: &[MessageCode] = &[
    MessageCode::from_static("ns", "one"),
    MessageCode::from_static("ns", "two"),
];

fn catalog(namespace: &'static str, codes: &'static [MessageCode]) -> MessageCatalog {
    MessageCatalog {
        namespace,
        codes,
        en_ftl: "",
        uk_ftl: "",
    }
}

#[test]
fn catalog_check_accepts_valid_catalog() {
    assert_eq!(catalog("ns", GOOD).check(), Ok(()));
    assert_eq!(catalog("ns", &[]).check(), Ok(()));
}

#[test]
fn catalog_check_reports_each_error_variant() {
    assert_eq!(
        catalog("Bad-Ns", GOOD).check(),
        Err(MessageCatalogError::MalformedNamespace("Bad-Ns"))
    );
    assert_eq!(
        catalog("", GOOD).check(),
        Err(MessageCatalogError::MalformedNamespace(""))
    );
    assert_eq!(
        catalog("ns", BAD_CODE).check(),
        Err(MessageCatalogError::MalformedCode(BAD_CODE[0].clone()))
    );
    assert_eq!(
        catalog("ns", FOREIGN).check(),
        Err(MessageCatalogError::ForeignCode {
            namespace: "ns",
            code: FOREIGN[0].clone()
        })
    );
    assert_eq!(
        catalog("ns", DUPLICATED).check(),
        Err(MessageCatalogError::DuplicateCode(DUPLICATED[0].clone()))
    );
}

#[test]
fn catalog_ftl_keys_follow_code_order() {
    let keys: Vec<_> = catalog("ns", GOOD).ftl_keys().collect();
    assert_eq!(keys, ["msg-ns-one", "msg-ns-two"]);
}

fn ftl_message_ids(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace) && !line.starts_with('#'))
        .filter_map(|line| line.split_once('=').map(|(id, _)| id.trim().to_owned()))
        .filter(|id| !id.is_empty())
        .collect()
}

#[test]
fn common_catalog_is_valid() {
    assert_eq!(COMMON_MESSAGES.check(), Ok(()));
}

#[test]
fn common_catalog_locales_define_every_key_with_identical_keysets() {
    let expected: BTreeSet<String> = COMMON_MESSAGES.ftl_keys().collect();
    let en = ftl_message_ids(COMMON_MESSAGES.en_ftl);
    let uk = ftl_message_ids(COMMON_MESSAGES.uk_ftl);
    assert_eq!(en, expected);
    assert_eq!(uk, expected);
    for code in [
        UPSTREAM_FAILURE,
        forge_types::INTEGRATION_DISABLED,
        forge_types::NO_CHAT_PLATFORM_ENABLED,
        forge_types::NO_WHISPER_PLATFORM_ENABLED,
        forge_types::WHISPERS_UNSUPPORTED,
        forge_types::UNKNOWN_CHAT_TARGET,
    ] {
        assert!(expected.contains(&code.ftl_key()), "{code}");
    }
}

#[test]
fn common_catalog_translations_are_non_empty_and_differ_between_locales() {
    let value_of = |source: &str| {
        source
            .lines()
            .find_map(|line| line.split_once('='))
            .map(|(_, value)| value.trim().to_owned())
            .unwrap()
    };
    let en = value_of(COMMON_MESSAGES.en_ftl);
    let uk = value_of(COMMON_MESSAGES.uk_ftl);
    assert!(!en.is_empty() && !uk.is_empty());
    assert_ne!(en, uk);
}
