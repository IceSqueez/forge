#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::{
    AudioAnnouncement, AudioCommand, ConfigSection, DeliveryDisposition, OverlayConfig,
    OverlayKindDescriptor, OverlayKindRegistry, announcement_content, command_content,
    register_builtin_kinds,
};
use forge_registry::FormField;
use forge_types::Variant;
use serde_json::{Value, json};

const AUDIO: &str = "overlay.audio";

const DECLARATIONS: &[(&str, bool, bool)] = &[
    ("overlay.alert", true, false),
    ("overlay.audio", false, true),
    ("overlay.chat", true, false),
    ("overlay.frame", true, false),
    ("overlay.goal", true, false),
    ("overlay.ticker", true, false),
];

const CONTENT_KEYS: &[&str] = &[
    "clip_duration_ms",
    "clip_id",
    "clip_media_type",
    "clip_path",
    "report_path",
    "command",
];

const DURATION_KEY: &str = "clip_duration_ms";
const DURATION_MIN_MS: i64 = 0;
const DURATION_MAX_MS: i64 = 3_600_000;

const CLIP: &str = "5f2a";
const CLIP_PATH: &str = "/audio/v1/clip/n3Zq";
const REPORT_PATH: &str = "/audio/v1/report/n3Zq";
const MEDIA_TYPE: &str = "audio/wav";
const DURATION_MS: u64 = 2_500;

const TIMERS: &[&str] = &["setTimeout", "setInterval", "requestAnimationFrame"];
const DOM_SINKS: &[&str] = &[
    "console.",
    "innerHTML",
    "innerText",
    "textContent",
    "document.write",
    "appendChild",
    "setAttribute",
    "localStorage",
    "sessionStorage",
];

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn with_audio<T>(read: impl FnOnce(&dyn OverlayKindDescriptor) -> T) -> T {
    let reg = registry();
    read(reg.get(AUDIO).expect("the audio kind ships in this build"))
}

fn behavior() -> &'static str {
    with_audio(|descriptor| descriptor.page_assets().behavior)
}

fn markup() -> &'static str {
    with_audio(|descriptor| descriptor.page_assets().markup)
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

fn plain(content: &OverlayConfig) -> Value {
    Value::Object(
        content
            .iter()
            .map(|(key, value)| (key.clone(), value.to_plain_json()))
            .collect(),
    )
}

fn announced(duration_ms: u64) -> OverlayConfig {
    announcement_content(&AudioAnnouncement {
        clip_id: CLIP,
        clip_path: CLIP_PATH,
        report_path: REPORT_PATH,
        media_type: MEDIA_TYPE,
        duration_ms,
    })
}

fn at(source: &str, needle: &str) -> usize {
    source
        .find(needle)
        .unwrap_or_else(|| panic!("the audio page no longer contains '{needle}'"))
}

fn members_after(source: &str, marker: &str) -> BTreeSet<String> {
    source
        .match_indices(marker)
        .map(|(start, _)| {
            source[start + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .filter(|member: &String| !member.is_empty())
        .collect()
}

fn between(source: &str, opening: &str, closing: &str) -> String {
    let start = at(source, opening) + opening.len();
    let rest = &source[start..];
    let end = rest
        .find(closing)
        .unwrap_or_else(|| panic!("'{opening}' is never closed by '{closing}'"));
    rest[..end].to_owned()
}

#[test]
fn every_builtin_kind_states_whether_it_draws_a_page_and_who_fills_its_content() {
    let reg = registry();
    let registered: BTreeSet<&str> = reg.all().map(OverlayKindDescriptor::id).collect();
    let tabled: BTreeSet<&str> = DECLARATIONS.iter().map(|(id, _, _)| *id).collect();

    assert_eq!(
        registered, tabled,
        "a builtin kind ships without a decision on either declaration, so the desktop guesses"
    );
    for (kind_id, draws, machine_filled) in DECLARATIONS {
        let descriptor = reg.get(kind_id).expect("a tabled kind is registered");

        assert_eq!(
            (
                descriptor.has_visual_page(),
                descriptor.content_is_machine_filled()
            ),
            (*draws, *machine_filled),
            "{kind_id} changed what the desktop and the send guard read off it"
        );
    }
}

#[test]
fn an_audio_delivery_is_never_retained_and_never_reordered() {
    with_audio(|descriptor| {
        assert_eq!(
            descriptor.delivery_disposition(),
            DeliveryDisposition::Transient,
            "a retained or conflated announcement is replayed against a capability already spent"
        );
        assert!(
            descriptor.order_sensitive(),
            "a stop that overtakes its announcement reaches a clip the page has never met"
        );
    });
}

#[test]
fn the_audio_form_declares_exactly_the_keys_a_delivery_may_carry() {
    with_audio(|descriptor| {
        let fields = descriptor.config_fields();
        let declared: BTreeSet<&str> = fields.iter().map(|s| field_key(&s.field)).collect();

        assert_eq!(
            declared,
            CONTENT_KEYS.iter().copied().collect::<BTreeSet<&str>>(),
            "a key outside this vocabulary is dropped by delivered_content before it reaches the \
             page, and a key missing from it can never be sent"
        );
        for sectioned in &fields {
            assert_eq!(
                sectioned.section,
                ConfigSection::Content,
                "{} is not content, so a step could never supply it",
                field_key(&sectioned.field)
            );
        }
    });
}

#[test]
fn the_announced_clip_length_is_bounded_to_an_hour_by_the_form_itself() {
    with_audio(|descriptor| {
        let fields = descriptor.config_fields();
        let duration = fields
            .iter()
            .map(|sectioned| &sectioned.field)
            .find(|field| field_key(field) == DURATION_KEY)
            .expect("the audio form declares a clip length");

        assert!(
            matches!(
                duration,
                FormField::Integer {
                    min: DURATION_MIN_MS,
                    max: DURATION_MAX_MS,
                    ..
                }
            ),
            "the clip length no longer admits exactly 0 ..= one hour: {duration:?}"
        );
    });
}

#[test]
fn the_audio_defaults_name_no_clip_and_no_address() {
    with_audio(|descriptor| {
        assert_eq!(
            plain(&descriptor.default_config()),
            json!({
                "clip_id": "",
                "clip_path": "",
                "report_path": "",
                "clip_media_type": "",
                "clip_duration_ms": 0,
                "command": "",
            }),
            "a default that names a real address ships a capability in the generated page"
        );
    });
}

#[test]
fn an_announcement_carries_its_addresses_and_never_a_transport_command() {
    assert_eq!(
        plain(&announced(DURATION_MS)),
        json!({
            "clip_id": "5f2a",
            "clip_path": "/audio/v1/clip/n3Zq",
            "report_path": "/audio/v1/report/n3Zq",
            "clip_media_type": "audio/wav",
            "clip_duration_ms": 2_500,
        }),
        "an announcement carrying a command would be read as a control by the page's own branch"
    );
}

#[test]
fn an_announced_length_outside_the_bound_is_clamped_rather_than_refused() {
    for (duration_ms, expected) in [
        (0_u64, 0_i64),
        (1, 1),
        (3_599_999, 3_599_999),
        (3_600_000, 3_600_000),
        (3_600_001, 3_600_000),
        (u64::MAX, 3_600_000),
    ] {
        assert_eq!(
            announced(duration_ms).get(DURATION_KEY),
            Some(&Variant::Int(expected)),
            "a clip announced as {duration_ms}ms reached the page outside the field's own bound"
        );
    }
}

#[test]
fn a_transport_command_names_one_clip_or_reaches_every_clip_with_an_empty_id() {
    for (command, clip_id, expected) in [
        (
            AudioCommand::Stop,
            Some(CLIP),
            json!({ "command": "stop", "clip_id": "5f2a" }),
        ),
        (
            AudioCommand::Stop,
            None,
            json!({ "command": "stop", "clip_id": "" }),
        ),
        (
            AudioCommand::Pause,
            Some(CLIP),
            json!({ "command": "pause", "clip_id": "5f2a" }),
        ),
        (
            AudioCommand::Resume,
            None,
            json!({ "command": "resume", "clip_id": "" }),
        ),
    ] {
        assert_eq!(
            plain(&command_content(command, clip_id)),
            expected,
            "{command:?} addressed at {clip_id:?} reached the page in the wrong shape"
        );
    }
}

#[test]
fn every_transport_command_rust_sends_is_a_word_the_page_branches_on() {
    for (command, constant) in [
        (AudioCommand::Stop, "COMMAND_STOP"),
        (AudioCommand::Pause, "COMMAND_PAUSE"),
        (AudioCommand::Resume, "COMMAND_RESUME"),
    ] {
        let declaration = format!("var {constant} = \"{}\";", command.as_str());

        assert!(
            behavior().contains(&declaration),
            "the page does not state {declaration}, so {command:?} arrives as an unknown word"
        );
        assert!(
            behavior().contains(&format!("command === {constant}")),
            "the page declares {constant} but never acts on it"
        );
    }
}

#[test]
fn the_audio_page_arms_no_timer_of_its_own() {
    for source in [behavior(), markup()] {
        for timer in TIMERS {
            assert!(
                !source.contains(timer),
                "the page arms {timer}, so a clip that plays past its hint is reported refused"
            );
        }
    }
}

#[test]
fn the_audio_page_reads_delivered_values_by_the_names_the_form_declares() {
    assert_eq!(
        members_after(behavior(), "values."),
        [
            "clip_id",
            "clip_media_type",
            "clip_path",
            "command",
            "report_path"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<String>>(),
        "the page reads a key the form never declares, or stopped reading one it branches on"
    );
}

#[test]
fn the_audio_page_posts_a_body_built_from_nothing_but_the_verdict() {
    let built = between(behavior(), "var body =", ";");

    for expected in ["verdict", "reason"] {
        assert!(
            built.contains(expected),
            "the reported body no longer carries '{expected}'"
        );
    }
    for capability in ["clip", "path", "Path", "src", "objectUrl"] {
        assert!(
            !built.contains(capability),
            "the reported body mentions '{capability}', which hands a capability back over HTTP"
        );
    }
    assert_eq!(
        behavior().matches("JSON.stringify(").count(),
        1,
        "the page serializes something besides the one verdict body"
    );
    assert!(
        behavior().contains("JSON.stringify(body)"),
        "the one thing the page serializes is not the verdict body"
    );
}

#[test]
fn the_audio_page_names_the_two_verdicts_the_report_route_accepts() {
    for declaration in [
        "var VERDICT_PLAYED = \"played\";",
        "var VERDICT_REFUSED = \"refused\";",
    ] {
        assert!(
            behavior().contains(declaration),
            "the page does not state {declaration}, so its report is a word the server drops"
        );
    }
}

#[test]
fn the_audio_page_never_writes_a_capability_into_the_document_or_the_console() {
    for sink in DOM_SINKS {
        assert!(
            !behavior().contains(sink),
            "the page reaches {sink}, where an address or a clip id becomes readable"
        );
    }
    assert!(
        !markup().contains("<script>"),
        "the page carries inline script, which this audit cannot read"
    );
}

#[test]
fn a_previewed_audio_page_reveals_its_note_and_registers_no_delivery_handler() {
    let ready = &behavior()[at(behavior(), "forge.ready(")..];
    let gate = at(ready, "if (previewing)");
    let returned = gate + at(&ready[gate..], "return;");

    assert!(
        at(ready, "forge.show(") < returned,
        "a previewed page returns before it reveals the note explaining why it draws nothing"
    );
    assert!(
        at(ready, "forge.content(") > returned,
        "a previewed page still registers a delivery handler, so a preview tab can fetch and play"
    );
}
