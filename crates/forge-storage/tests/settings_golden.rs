#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeMap;

use forge_storage::settings::core_settings::{
    ACCENT, AUDIO_OUTPUT_DEVICE_ID, FONT_BODY, FONT_MONO, LANGUAGE, THEME,
};
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    CredentialsKeyLoss, EngineParams, Flag, JsonDocument, Setting, SettingDisclosure,
    SettingPortability, VoiceGateSettings, chat_history_display_limit,
    chat_history_per_viewer_limit, diagnostic_log_level, disabled_tts_engines, engine_params,
    master_volume, read_setting, read_stored_setting, record_credentials_key_loss,
    set_chat_history_display_limit, set_chat_history_per_viewer_limit, set_diagnostic_log_level,
    set_disabled_tts_engines, set_engine_params, set_master_volume, set_voice_gate_enabled,
    set_voice_gate_hold_ms, set_voice_gate_input_device_id, set_voice_gate_threshold,
    take_credentials_key_loss, voice_gate_settings, write_setting,
};
use forge_types::LogLevel;

use forge_storage::Language;

const STORED_ENCODING_GOLDEN: &str = include_str!("fixtures/settings/stored_encoding.json");
const PIPER: &str = "piper";

const OBS_AUTO_RECONNECT: Setting<Flag> = Setting::new(
    "obs.auto_reconnect",
    Flag { default: true },
    SettingPortability::MachineLocal,
    SettingDisclosure::Verbatim,
);
const ALLOWED_DOMAINS: Setting<JsonDocument<Vec<String>>> = Setting::new(
    "script.http_allowed_domains",
    JsonDocument::new(),
    SettingPortability::MachineLocal,
    SettingDisclosure::Kind,
);

fn piper_params() -> EngineParams {
    EngineParams {
        pitch_semitones: 2.5,
        rate_multiplier: 1.25,
        gain: 0.5,
    }
}

async fn written_by_every_typed_writer() -> MemorySettings {
    let repo = MemorySettings::new();
    write_setting(&repo, &LANGUAGE, &Language::Uk.to_string())
        .await
        .unwrap();
    write_setting(&repo, &THEME, &Some("catppuccin-mocha".to_owned()))
        .await
        .unwrap();
    write_setting(&repo, &ACCENT, &Some("blue".to_owned()))
        .await
        .unwrap();
    write_setting(&repo, &FONT_BODY, &Some("Inter".to_owned()))
        .await
        .unwrap();
    write_setting(&repo, &FONT_MONO, &Some("JetBrains Mono".to_owned()))
        .await
        .unwrap();
    write_setting(&repo, &AUDIO_OUTPUT_DEVICE_ID, &Some("hw:1,0".to_owned()))
        .await
        .unwrap();
    write_setting(&repo, &OBS_AUTO_RECONNECT, &false)
        .await
        .unwrap();
    write_setting(
        &repo,
        &ALLOWED_DOMAINS,
        &vec!["example.com".to_owned(), "api.example.org".to_owned()],
    )
    .await
    .unwrap();
    set_chat_history_per_viewer_limit(&repo, 20_000)
        .await
        .unwrap();
    set_chat_history_display_limit(&repo, 250).await.unwrap();
    set_disabled_tts_engines(&repo, &["espeak".to_owned()])
        .await
        .unwrap();
    set_engine_params(&repo, PIPER, piper_params())
        .await
        .unwrap();
    set_master_volume(&repo, 1.7).await.unwrap();
    set_voice_gate_enabled(&repo, true).await.unwrap();
    set_voice_gate_input_device_id(&repo, Some("mic-1".to_owned()))
        .await
        .unwrap();
    set_voice_gate_threshold(&repo, 0.3).await.unwrap();
    set_voice_gate_hold_ms(&repo, 1200).await.unwrap();
    set_diagnostic_log_level(&repo, &LogLevel::Debug)
        .await
        .unwrap();
    record_credentials_key_loss(&repo, CredentialsKeyLoss { stranded: 3 })
        .await
        .unwrap();
    repo
}

fn seeded_from_golden() -> MemorySettings {
    let stored: BTreeMap<String, String> = serde_json::from_str(STORED_ENCODING_GOLDEN).unwrap();
    let repo = MemorySettings::new();
    for (stored_key, value) in &stored {
        repo.seed_foreign(stored_key, value, None);
    }
    repo
}

#[tokio::test]
async fn typed_writers_store_the_golden_string_encoding() {
    let repo = written_by_every_typed_writer().await;

    let stored: BTreeMap<String, String> = repo
        .rows()
        .into_iter()
        .map(|row| (row.key, row.value))
        .collect();
    let rendered = format!("{}\n", serde_json::to_string_pretty(&stored).unwrap());

    assert_eq!(rendered, STORED_ENCODING_GOLDEN);
}

#[tokio::test]
async fn typed_readers_decode_the_golden_encoding_into_the_written_values() {
    let repo = seeded_from_golden();

    assert_eq!(read_setting(&repo, &LANGUAGE).await.unwrap(), "uk");
    assert_eq!(
        (
            read_setting(&repo, &THEME).await.unwrap(),
            read_setting(&repo, &ACCENT).await.unwrap(),
            read_setting(&repo, &FONT_BODY).await.unwrap(),
            read_setting(&repo, &FONT_MONO).await.unwrap(),
            read_setting(&repo, &AUDIO_OUTPUT_DEVICE_ID).await.unwrap(),
        ),
        (
            Some("catppuccin-mocha".to_owned()),
            Some("blue".to_owned()),
            Some("Inter".to_owned()),
            Some("JetBrains Mono".to_owned()),
            Some("hw:1,0".to_owned()),
        )
    );
    assert!(!read_setting(&repo, &OBS_AUTO_RECONNECT).await.unwrap());
    assert_eq!(
        read_stored_setting(&repo, &ALLOWED_DOMAINS).await.unwrap(),
        Some(vec!["example.com".to_owned(), "api.example.org".to_owned()])
    );
    assert_eq!(
        (
            chat_history_per_viewer_limit(&repo).await.unwrap(),
            chat_history_display_limit(&repo).await.unwrap(),
        ),
        (10_000, 250)
    );
    assert_eq!(
        disabled_tts_engines(&repo).await.unwrap(),
        vec!["espeak".to_owned()]
    );
    assert_eq!(engine_params(&repo, PIPER).await.unwrap(), piper_params());
    assert_eq!(master_volume(&repo).await.unwrap(), 1.0);
    assert_eq!(
        voice_gate_settings(&repo).await.unwrap(),
        VoiceGateSettings {
            enabled: true,
            input_device_id: Some("mic-1".to_owned()),
            threshold: 0.3,
            hold_ms: 1200,
        }
    );
    assert_eq!(diagnostic_log_level(&repo).await.unwrap(), LogLevel::Debug);
    assert_eq!(
        take_credentials_key_loss(&repo).await.unwrap(),
        Some(CredentialsKeyLoss { stranded: 3 })
    );
}
