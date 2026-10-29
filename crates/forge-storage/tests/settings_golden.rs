#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use async_trait::async_trait;
use forge_storage::{
    CredentialsKeyLoss, EngineParams, Language, SettingsRepo, StorageError, VoiceGateSettings,
    chat_history_display_limit, chat_history_per_viewer_limit, diagnostic_log_level,
    disabled_tts_engines, engine_params, get_bool_setting, get_json_setting, master_volume,
    record_credentials_key_loss, reserved_keys as key, set_bool_setting,
    set_chat_history_display_limit, set_chat_history_per_viewer_limit, set_diagnostic_log_level,
    set_disabled_tts_engines, set_engine_params, set_json_setting, set_master_volume,
    set_voice_gate_enabled, set_voice_gate_hold_ms, set_voice_gate_input_device_id,
    set_voice_gate_threshold, take_credentials_key_loss, voice_gate_settings,
};
use forge_types::LogLevel;

const STORED_ENCODING_GOLDEN: &str = include_str!("fixtures/settings/stored_encoding.json");
const PIPER: &str = "piper";

#[derive(Default)]
struct MapRepo {
    map: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl SettingsRepo for MapRepo {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self.map.lock().unwrap().get(key).cloned())
    }
    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.map
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.map.lock().unwrap().remove(key).is_some())
    }
    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(self.map.lock().unwrap().clone())
    }
}

fn piper_params() -> EngineParams {
    EngineParams {
        pitch_semitones: 2.5,
        rate_multiplier: 1.25,
        gain: 0.5,
    }
}

async fn written_by_every_typed_writer() -> MapRepo {
    let repo = MapRepo::default();
    repo.set_language(Language::Uk).await.unwrap();
    repo.set_theme("catppuccin-mocha").await.unwrap();
    repo.set_accent("blue").await.unwrap();
    repo.set_font_body(Some("Inter".to_owned())).await.unwrap();
    repo.set_font_mono(Some("JetBrains Mono".to_owned()))
        .await
        .unwrap();
    repo.set_audio_output_device_id(Some("hw:1,0".to_owned()))
        .await
        .unwrap();
    set_bool_setting(&repo, key::OBS_AUTO_RECONNECT, false)
        .await
        .unwrap();
    set_json_setting(
        &repo,
        key::SCRIPT_HTTP_ALLOWED_DOMAINS,
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

async fn seeded_from_golden() -> MapRepo {
    let stored: BTreeMap<String, String> = serde_json::from_str(STORED_ENCODING_GOLDEN).unwrap();
    let repo = MapRepo::default();
    for (stored_key, value) in &stored {
        repo.set_string(stored_key, value).await.unwrap();
    }
    repo
}

#[tokio::test]
async fn typed_writers_store_the_golden_string_encoding() {
    let repo = written_by_every_typed_writer().await;

    let stored: BTreeMap<String, String> = repo.load_all().await.unwrap().into_iter().collect();
    let rendered = format!("{}\n", serde_json::to_string_pretty(&stored).unwrap());

    assert_eq!(rendered, STORED_ENCODING_GOLDEN);
}

#[tokio::test]
async fn typed_readers_decode_the_golden_encoding_into_the_written_values() {
    let repo = seeded_from_golden().await;

    assert_eq!(repo.language().await.unwrap(), Language::Uk);
    assert_eq!(
        (
            repo.get_theme().await.unwrap(),
            repo.get_accent().await.unwrap(),
            repo.font_body().await.unwrap(),
            repo.font_mono().await.unwrap(),
            repo.audio_output_device_id().await.unwrap(),
        ),
        (
            Some("catppuccin-mocha".to_owned()),
            Some("blue".to_owned()),
            Some("Inter".to_owned()),
            Some("JetBrains Mono".to_owned()),
            Some("hw:1,0".to_owned()),
        )
    );
    assert!(!get_bool_setting(&repo, key::OBS_AUTO_RECONNECT, true).await);
    assert_eq!(
        get_json_setting::<Vec<String>>(&repo, key::SCRIPT_HTTP_ALLOWED_DOMAINS).await,
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
