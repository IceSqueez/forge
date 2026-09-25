#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use forge_overlay::config::{DURATION, SOUND, SPEECH, SPEECH_VOICE};
use forge_overlay::{
    DeliveryDisposition, OverlayConfig, OverlayKindRegistry, SpeechProgram, display_window,
    register_builtin_kinds, take_speech,
};
use forge_registry::FormField;
use forge_types::Variant;

const ALERT_KIND: &str = "overlay.alert";
const BLANK_KIND: &str = "overlay.blank";
const CHAT_KIND: &str = "overlay.chat";
const FRAME_KIND: &str = "overlay.frame";
const GOAL_KIND: &str = "overlay.goal";
const TICKER_KIND: &str = "overlay.ticker";

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn field_key(field: &FormField) -> &'static str {
    match field {
        FormField::Text { key, .. }
        | FormField::TextArea { key, .. }
        | FormField::Code { key, .. }
        | FormField::Integer { key, .. }
        | FormField::Slider { key, .. }
        | FormField::Toggle { key, .. }
        | FormField::FilePicker { key, .. }
        | FormField::DateTime { key, .. }
        | FormField::Select { key, .. }
        | FormField::DynamicSelect { key, .. }
        | FormField::DependentSelect { key, .. }
        | FormField::Swatch { key, .. }
        | FormField::Optional { key, .. }
        | FormField::SubChain { key, .. }
        | FormField::CaseList { key, .. } => key,
    }
}

fn stored_duration(value: Variant) -> OverlayConfig {
    OverlayConfig::from([(DURATION.to_owned(), value)])
}

#[test]
fn every_look_offers_one_sound_and_only_a_transient_look_offers_a_duration() {
    for descriptor in registry().all() {
        let transient = descriptor.delivery_disposition() == DeliveryDisposition::Transient;
        let offered: Vec<&str> = descriptor
            .config_fields()
            .iter()
            .map(|sectioned| field_key(&sectioned.field))
            .collect();
        let defaults = descriptor.default_config();

        assert_eq!(
            (
                offered.iter().filter(|key| **key == SOUND).count(),
                offered.iter().filter(|key| **key == DURATION).count(),
                defaults.get(SOUND),
                defaults.contains_key(DURATION),
            ),
            (
                1,
                usize::from(transient),
                Some(&Variant::String(String::new())),
                transient,
            ),
            "{} does not carry the base's sound and duration exactly as its disposition implies",
            descriptor.id()
        );
    }
}

#[test]
fn a_display_window_prefers_a_positive_step_override_then_the_stored_duration_then_the_looks_default()
 {
    let secs = Duration::from_secs;
    let registry = registry();
    for (kind, stored, override_ms, expected, label) in [
        (
            ALERT_KIND,
            OverlayConfig::new(),
            None,
            Some(secs(5)),
            "an alert nobody tuned",
        ),
        (
            TICKER_KIND,
            OverlayConfig::new(),
            None,
            Some(secs(8)),
            "a ticker nobody tuned",
        ),
        (
            BLANK_KIND,
            OverlayConfig::new(),
            None,
            Some(secs(5)),
            "a blank look nobody tuned",
        ),
        (
            ALERT_KIND,
            stored_duration(Variant::Int(3)),
            None,
            Some(secs(3)),
            "a stored duration",
        ),
        (
            ALERT_KIND,
            stored_duration(Variant::Int(3)),
            Some(1_500),
            Some(Duration::from_millis(1_500)),
            "a step override over a stored duration",
        ),
        (
            ALERT_KIND,
            stored_duration(Variant::Int(3)),
            Some(0),
            Some(secs(3)),
            "a zero override, which is no override",
        ),
        (
            ALERT_KIND,
            stored_duration(Variant::Int(0)),
            None,
            Some(secs(5)),
            "a stored zero",
        ),
        (
            ALERT_KIND,
            stored_duration(Variant::Int(-4)),
            None,
            Some(secs(5)),
            "a stored negative",
        ),
        (
            ALERT_KIND,
            stored_duration(Variant::String("7".to_owned())),
            None,
            Some(secs(5)),
            "a stored duration written as text",
        ),
        (
            TICKER_KIND,
            stored_duration(Variant::Int(0)),
            None,
            Some(secs(8)),
            "a stored zero on a look with its own default",
        ),
        (
            GOAL_KIND,
            OverlayConfig::new(),
            Some(2_000),
            None,
            "a replacing goal",
        ),
        (
            FRAME_KIND,
            OverlayConfig::new(),
            Some(2_000),
            None,
            "a replacing frame",
        ),
        (
            CHAT_KIND,
            OverlayConfig::new(),
            Some(2_000),
            None,
            "an appending chat",
        ),
    ] {
        let descriptor = registry.get(kind).expect("the look ships in this build");

        assert_eq!(
            display_window(descriptor, &stored, override_ms),
            expected,
            "{label}"
        );
    }
}

#[test]
fn speech_is_always_stripped_from_the_content_and_spoken_only_when_it_has_words() {
    let registry = registry();
    let descriptor = registry
        .get(ALERT_KIND)
        .expect("the look ships in this build");
    let text = |value: &str| Variant::String(value.to_owned());
    let program = |voice: Option<&str>| {
        Some(SpeechProgram {
            text: "thanks Mira".to_owned(),
            voice_alias: voice.map(str::to_owned),
        })
    };
    for (speech, voice, expected, label) in [
        (
            text("  thanks Mira \n"),
            text(" amy "),
            program(Some("amy")),
            "a voiced speech",
        ),
        (
            text("thanks Mira"),
            text("   "),
            program(None),
            "a blank voice",
        ),
        (
            text(" \t "),
            text("amy"),
            None,
            "a speech of only whitespace",
        ),
        (
            Variant::Int(7),
            text("amy"),
            None,
            "a speech that is not text",
        ),
    ] {
        let stored = OverlayConfig::from([(SPEECH_VOICE.to_owned(), voice)]);
        let mut content = OverlayConfig::from([(SPEECH.to_owned(), speech)]);

        let taken = take_speech(descriptor, &stored, &mut content);

        assert_eq!(
            (taken, content.contains_key(SPEECH)),
            (expected, false),
            "{label}: the wrong speech was taken, or its text stayed in the page content"
        );
    }
}
