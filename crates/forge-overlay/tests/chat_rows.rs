#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
mod common;
use common::{CHAT_KIND, registry, sub_alert_instance, text, with_kind};

use forge_overlay::config::{
    DEFAULT_ROW_LIMIT, HIDE_ROWS, HIDE_ROWS_AFTER_MAX_SECS, HIDE_ROWS_AFTER_MIN_SECS,
    HIDE_ROWS_AFTER_SECS, POSITION, ROW_LIMIT, ROW_LIMIT_MAX, ROW_LIMIT_MIN, TEXT_SIZE,
};
use forge_overlay::{
    OverlayConfig, OverlayError, config_document, effective_overlay_config, validate_overlay_config,
};
use forge_types::Variant;
use serde_json::{Value, json};

fn one(key: &str, value: Variant) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), value)])
}

fn stored_before_the_row_settings() -> OverlayConfig {
    OverlayConfig::from([
        (POSITION.to_owned(), text("top")),
        (TEXT_SIZE.to_owned(), Variant::Int(20)),
    ])
}

fn row_settings(config: &OverlayConfig) -> [Option<&Variant>; 3] {
    [
        config.get(ROW_LIMIT),
        config.get(HIDE_ROWS),
        config.get(HIDE_ROWS_AFTER_SECS),
    ]
}

fn page_config(stored: OverlayConfig) -> Value {
    let reg = registry();
    let descriptor = reg
        .get(CHAT_KIND)
        .expect("the chat kind ships in this build");
    let mut instance = sub_alert_instance(CHAT_KIND);
    instance.config = stored;
    let document = config_document(&instance, descriptor).expect("the config document builds");
    let parsed: Value = serde_json::from_str(&document).expect("the document is valid JSON");
    parsed["config"].clone()
}

fn page_number(behavior: &str, name: &str) -> i64 {
    let declaration = format!("var {name} = ");
    let start = behavior
        .find(&declaration)
        .expect("the chat page declares the constant")
        + declaration.len();
    let rest = &behavior[start..];
    let end = rest.find(';').expect("a declaration ends with a semicolon");
    rest[..end]
        .trim()
        .parse()
        .expect("the constant is a whole number")
}

#[test]
fn a_chat_stored_before_the_row_settings_keeps_fifteen_rows_and_no_hiding() {
    let effective = with_kind(CHAT_KIND, |descriptor| {
        effective_overlay_config(descriptor, &stored_before_the_row_settings())
    });

    assert_eq!(
        row_settings(&effective),
        [
            Some(&Variant::Int(15)),
            Some(&Variant::Bool(false)),
            Some(&Variant::Int(30)),
        ]
    );
}

#[test]
fn a_chat_stored_before_the_row_settings_still_validates() {
    let refusal = with_kind(CHAT_KIND, |descriptor| {
        validate_overlay_config(descriptor, &stored_before_the_row_settings()).err()
    });

    assert!(refusal.is_none(), "{refusal:?}");
}

#[test]
fn chosen_row_settings_override_the_defaults_in_the_effective_config() {
    let stored = OverlayConfig::from([
        (ROW_LIMIT.to_owned(), Variant::Int(40)),
        (HIDE_ROWS.to_owned(), Variant::Bool(true)),
        (HIDE_ROWS_AFTER_SECS.to_owned(), Variant::Int(120)),
    ]);
    let effective = with_kind(CHAT_KIND, |descriptor| {
        effective_overlay_config(descriptor, &stored)
    });

    assert_eq!(
        row_settings(&effective),
        [
            Some(&Variant::Int(40)),
            Some(&Variant::Bool(true)),
            Some(&Variant::Int(120)),
        ]
    );
}

#[test]
fn the_chat_page_document_carries_the_row_settings() {
    for (stored, expected) in [
        (OverlayConfig::new(), [json!(15), json!(false), json!(30)]),
        (
            OverlayConfig::from([
                (ROW_LIMIT.to_owned(), Variant::Int(100)),
                (HIDE_ROWS.to_owned(), Variant::Bool(true)),
                (HIDE_ROWS_AFTER_SECS.to_owned(), Variant::Int(1)),
            ]),
            [json!(100), json!(true), json!(1)],
        ),
    ] {
        let config = page_config(stored.clone());

        assert_eq!(
            [
                config[ROW_LIMIT].clone(),
                config[HIDE_ROWS].clone(),
                config[HIDE_ROWS_AFTER_SECS].clone(),
            ],
            expected,
            "stored {stored:?}"
        );
    }
}

#[test]
fn the_row_settings_accept_their_bounds_and_refuse_one_step_past_each() {
    for (key, min, max) in [(ROW_LIMIT, 1, 100), (HIDE_ROWS_AFTER_SECS, 1, 300)] {
        with_kind(CHAT_KIND, |descriptor| {
            for accepted in [min, min + 1, max - 1, max] {
                validate_overlay_config(descriptor, &one(key, Variant::Int(accepted)))
                    .unwrap_or_else(|e| panic!("refused {key} = {accepted}: {e}"));
            }

            for rejected in [min - 1, max + 1, i64::MIN, i64::MAX] {
                let err = validate_overlay_config(descriptor, &one(key, Variant::Int(rejected)))
                    .expect_err("a value outside the supported range must be refused");

                assert!(
                    matches!(
                        &err,
                        OverlayError::OutOfRange { key: k, min: lo, max: hi }
                            if k == key && *lo == min && *hi == max
                    ),
                    "{key} = {rejected} produced {err:?}"
                );
            }
        });
    }
}

#[test]
fn a_row_setting_held_as_the_wrong_type_is_refused() {
    for (key, stored) in [
        (ROW_LIMIT, Variant::Float(15.0)),
        (ROW_LIMIT, text("15")),
        (HIDE_ROWS_AFTER_SECS, Variant::Bool(true)),
        (HIDE_ROWS, text("true")),
        (HIDE_ROWS, Variant::Int(1)),
    ] {
        let err = with_kind(CHAT_KIND, |descriptor| {
            validate_overlay_config(descriptor, &one(key, stored.clone()))
                .expect_err("a value of the wrong type must be refused")
        });

        assert!(
            matches!(&err, OverlayError::WrongType { key: k, .. } if k == key),
            "{key} held as {stored:?} produced {err:?}"
        );
    }
}

#[test]
fn the_chat_page_falls_back_to_the_bounds_and_default_the_config_declares() {
    let behavior = with_kind(CHAT_KIND, |descriptor| descriptor.page_assets().behavior);

    for (name, declared) in [
        ("DEFAULT_ROW_LIMIT", DEFAULT_ROW_LIMIT),
        ("ROW_LIMIT_MIN", ROW_LIMIT_MIN),
        ("ROW_LIMIT_MAX", ROW_LIMIT_MAX),
        ("HIDE_AFTER_MIN_SECS", HIDE_ROWS_AFTER_MIN_SECS),
        ("HIDE_AFTER_MAX_SECS", HIDE_ROWS_AFTER_MAX_SECS),
    ] {
        assert_eq!(page_number(behavior, name), declared, "{name}");
    }
}

#[test]
fn the_chat_page_reads_the_row_keys_the_config_writes() {
    let behavior = with_kind(CHAT_KIND, |descriptor| descriptor.page_assets().behavior);

    for key in [ROW_LIMIT, HIDE_ROWS, HIDE_ROWS_AFTER_SECS] {
        assert!(behavior.contains(&format!("config.{key}")), "{key}");
    }
}
