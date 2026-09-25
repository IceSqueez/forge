#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::{
    AudioAnnouncement, AudioCommand, OverlayConfig, OverlayKindRegistry, RUNTIME_SOURCE,
    announcement_content, command_content, register_builtin_kinds,
};
use forge_types::Variant;
use serde_json::{Value, json};

const DURATION_KEY: &str = "clip_duration_ms";

const CLIP: &str = "5f2a";
const CLIP_PATH: &str = "/audio/v1/clip/n3Zq";
const REPORT_PATH: &str = "/audio/v1/report/n3Zq";
const MEDIA_TYPE: &str = "audio/wav";
const DURATION_MS: u64 = 2_500;

const AUDIO_SECTION_START: &str = "function textOf(";
const AUDIO_SECTION_END: &str = "function deliver(";

const TIMERS: &[&str] = &["setTimeout", "setInterval", "requestAnimationFrame"];
const DOM_SINKS: &[&str] = &[
    "console.",
    "warn(",
    "innerHTML",
    "innerText",
    "textContent",
    "document.write",
    "appendChild",
    "setAttribute",
    "localStorage",
    "sessionStorage",
];

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
        .unwrap_or_else(|| panic!("the runtime no longer contains '{needle}'"))
}

fn audio_section() -> &'static str {
    let start = at(RUNTIME_SOURCE, AUDIO_SECTION_START);
    let end = start + at(&RUNTIME_SOURCE[start..], AUDIO_SECTION_END);
    &RUNTIME_SOURCE[start..end]
}

fn receive_body() -> &'static str {
    let start = at(RUNTIME_SOURCE, "function receive(");
    let end = start + at(&RUNTIME_SOURCE[start..], "\n  }");
    &RUNTIME_SOURCE[start..end]
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
        "an announcement carrying a command would be read as a control by the runtime's own branch"
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
            "a clip announced as {duration_ms}ms reached the page outside the one-hour bound"
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
fn every_transport_command_rust_sends_is_a_word_the_runtime_branches_on() {
    for (command, constant) in [
        (AudioCommand::Stop, "COMMAND_STOP"),
        (AudioCommand::Pause, "COMMAND_PAUSE"),
        (AudioCommand::Resume, "COMMAND_RESUME"),
    ] {
        let declaration = format!("var {constant} = \"{}\";", command.as_str());

        assert!(
            RUNTIME_SOURCE.contains(&declaration),
            "the runtime does not state {declaration}, so {command:?} arrives as an unknown word"
        );
        assert!(
            audio_section().contains(&format!("command === {constant}")),
            "the runtime declares {constant} but never acts on it"
        );
    }
}

#[test]
fn an_audio_frame_returns_before_it_reaches_the_looks_content_callbacks() {
    let body = receive_body();
    let gate = at(body, "if (isAudioTransport(values))");
    let returned = gate + at(&body[gate..], "return;");

    assert!(
        returned < at(body, "deliver(values"),
        "an announcement falls through to deliver(), so the look shows or clears on a sound"
    );
}

#[test]
fn a_frame_is_audio_when_either_the_clip_address_or_a_command_is_present() {
    let check = between(audio_section(), "function isAudioTransport(values) {", "}");

    for key in ["values.clip_path", "values.command"] {
        assert!(
            check.contains(key),
            "{key} no longer marks a frame as audio, so it would reach the look"
        );
    }
}

#[test]
fn a_preview_tab_swallows_audio_frames_without_playing_them() {
    let body = receive_body();
    let gate = at(body, "if (isAudioTransport(values))");
    let branch = &body[gate..gate + at(&body[gate..], "return;")];

    assert!(
        at(branch, "if (!previewing)") < at(branch, "audioTransport(values)"),
        "a preview tab fetches and plays the clip, spending the capability and doubling the sound"
    );
}

#[test]
fn the_audio_transport_arms_no_timer_of_its_own() {
    for timer in TIMERS {
        assert!(
            !audio_section().contains(timer),
            "the audio transport arms {timer}, so a clip that plays past its hint is refused"
        );
    }
}

#[test]
fn the_audio_transport_reads_exactly_the_keys_an_announcement_or_command_carries() {
    assert_eq!(
        members_after(audio_section(), "values."),
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
        "the runtime reads a key Rust never sends, or stopped reading one it branches on"
    );
}

#[test]
fn the_audio_transport_posts_a_body_built_from_nothing_but_the_verdict() {
    let section = audio_section();
    let built = between(section, "var body =", ";");

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
        section.matches("JSON.stringify(").count(),
        1,
        "the audio transport serializes something besides the one verdict body"
    );
    assert!(
        section.contains("JSON.stringify(body)"),
        "the one thing the audio transport serializes is not the verdict body"
    );
}

#[test]
fn the_runtime_names_the_two_verdicts_the_report_route_accepts() {
    for declaration in [
        "var VERDICT_PLAYED = \"played\";",
        "var VERDICT_REFUSED = \"refused\";",
    ] {
        assert!(
            RUNTIME_SOURCE.contains(declaration),
            "the runtime does not state {declaration}, so its report is a word the server drops"
        );
    }
}

#[test]
fn the_audio_transport_never_writes_a_capability_into_the_document_or_the_console() {
    for sink in DOM_SINKS {
        assert!(
            !audio_section().contains(sink),
            "the audio transport reaches {sink}, where an address or a clip id becomes readable"
        );
    }
}

#[test]
fn the_blank_look_registers_no_content_handler_of_its_own() {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    let assets = reg
        .get("overlay.blank")
        .expect("the blank look ships in this build")
        .page_assets();

    assert!(
        !assets.behavior.contains("forge.content("),
        "the blank look handles content, so a sent overlay step draws on a page meant to be empty"
    );
    assert!(
        !assets.markup.contains("<script>"),
        "the blank look carries inline script, which this audit cannot read"
    );
}
