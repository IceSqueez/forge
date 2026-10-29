#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use forge_storage::settings::core_settings::{
    AUDIO_VOICE_GATE_ENABLED, AUDIO_VOICE_GATE_HOLD_MS, AUDIO_VOICE_GATE_THRESHOLD,
    CHAT_HISTORY_DISPLAY_LIMIT, CHAT_HISTORY_PER_VIEWER_LIMIT, DIAGNOSTICS_LOG_LEVEL,
    EVENT_LOG_RETENTION_DAYS, SOUNDBOARD_ALSO_HEADPHONES, SOUNDBOARD_ENABLED,
    SOUNDBOARD_MASTER_VOLUME, TTS_MASTER_VOLUME,
};
use forge_storage::{
    Count, Level, Setting, SettingCodec, SettingsRepo, StorageError, read_setting, write_setting,
};
use forge_types::LogLevel;

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

async fn stored_after_write<C: SettingCodec>(setting: &Setting<C>, value: &C::Value) -> String {
    let repo = MapRepo::default();
    write_setting(&repo, setting, value).await.unwrap();
    repo.get_string(setting.name()).await.unwrap().unwrap()
}

async fn read_back<C: SettingCodec>(setting: &Setting<C>, raw: &str) -> C::Value {
    let repo = MapRepo::default();
    repo.set_string(setting.name(), raw).await.unwrap();
    read_setting(&repo, setting).await.unwrap()
}

#[tokio::test]
async fn every_core_typed_setting_writes_its_golden_stored_string() {
    let written = vec![
        (
            EVENT_LOG_RETENTION_DAYS.name(),
            stored_after_write(&EVENT_LOG_RETENTION_DAYS, &30).await,
        ),
        (
            AUDIO_VOICE_GATE_ENABLED.name(),
            stored_after_write(&AUDIO_VOICE_GATE_ENABLED, &true).await,
        ),
        (
            AUDIO_VOICE_GATE_THRESHOLD.name(),
            stored_after_write(&AUDIO_VOICE_GATE_THRESHOLD, &0.15).await,
        ),
        (
            AUDIO_VOICE_GATE_HOLD_MS.name(),
            stored_after_write(&AUDIO_VOICE_GATE_HOLD_MS, &800).await,
        ),
        (
            CHAT_HISTORY_PER_VIEWER_LIMIT.name(),
            stored_after_write(&CHAT_HISTORY_PER_VIEWER_LIMIT, &0).await,
        ),
        (
            CHAT_HISTORY_DISPLAY_LIMIT.name(),
            stored_after_write(&CHAT_HISTORY_DISPLAY_LIMIT, &500).await,
        ),
        (
            TTS_MASTER_VOLUME.name(),
            stored_after_write(&TTS_MASTER_VOLUME, &0.8).await,
        ),
        (
            SOUNDBOARD_ENABLED.name(),
            stored_after_write(&SOUNDBOARD_ENABLED, &false).await,
        ),
        (
            SOUNDBOARD_MASTER_VOLUME.name(),
            stored_after_write(&SOUNDBOARD_MASTER_VOLUME, &1.0).await,
        ),
        (
            SOUNDBOARD_ALSO_HEADPHONES.name(),
            stored_after_write(&SOUNDBOARD_ALSO_HEADPHONES, &true).await,
        ),
        (
            DIAGNOSTICS_LOG_LEVEL.name(),
            stored_after_write(&DIAGNOSTICS_LOG_LEVEL, &LogLevel::Warn).await,
        ),
    ];

    let golden: Vec<(&str, String)> = [
        ("event_log_retention_days", "30"),
        ("audio.voice_gate_enabled", "true"),
        ("audio.voice_gate_threshold", "0.15"),
        ("audio.voice_gate_hold_ms", "800"),
        ("chat_history.per_viewer_limit", "0"),
        ("chat_history.display_limit", "500"),
        ("tts.master_volume", "0.8"),
        ("soundboard.enabled", "false"),
        ("soundboard.master_volume", "1"),
        ("soundboard.also_headphones", "true"),
        ("diagnostics.log_level", "warn"),
    ]
    .into_iter()
    .map(|(name, stored)| (name, stored.to_owned()))
    .collect();

    assert_eq!(written, golden);
}

#[tokio::test]
async fn bounded_core_counts_store_writes_clamped_to_their_declared_range() {
    let cases: [(&Setting<Count>, u32, &str); 8] = [
        (&CHAT_HISTORY_DISPLAY_LIMIT, 0, "1"),
        (&CHAT_HISTORY_DISPLAY_LIMIT, 1, "1"),
        (&CHAT_HISTORY_DISPLAY_LIMIT, 10_000, "10000"),
        (&CHAT_HISTORY_DISPLAY_LIMIT, 10_001, "10000"),
        (&AUDIO_VOICE_GATE_HOLD_MS, 0, "0"),
        (&AUDIO_VOICE_GATE_HOLD_MS, 10_000, "10000"),
        (&AUDIO_VOICE_GATE_HOLD_MS, 10_001, "10000"),
        (&AUDIO_VOICE_GATE_HOLD_MS, u32::MAX, "10000"),
    ];
    for (setting, value, stored) in cases {
        assert_eq!(
            stored_after_write(setting, &value).await,
            stored,
            "{} <- {value}",
            setting.name(),
        );
    }
}

#[tokio::test]
async fn bounded_core_counts_clamp_stored_values_on_read() {
    let cases: [(&Setting<Count>, &str, u32); 9] = [
        (&CHAT_HISTORY_DISPLAY_LIMIT, "0", 1),
        (&CHAT_HISTORY_DISPLAY_LIMIT, "1", 1),
        (&CHAT_HISTORY_DISPLAY_LIMIT, "10000", 10_000),
        (&CHAT_HISTORY_DISPLAY_LIMIT, "10001", 10_000),
        (&AUDIO_VOICE_GATE_HOLD_MS, "0", 0),
        (&AUDIO_VOICE_GATE_HOLD_MS, "10000", 10_000),
        (&AUDIO_VOICE_GATE_HOLD_MS, "10001", 10_000),
        (&CHAT_HISTORY_PER_VIEWER_LIMIT, " 20 ", 20),
        (&CHAT_HISTORY_PER_VIEWER_LIMIT, "\t0\n", 0),
    ];
    for (setting, raw, expected) in cases {
        assert_eq!(
            read_back(setting, raw).await,
            expected,
            "{} = {raw:?}",
            setting.name(),
        );
    }
}

#[tokio::test]
async fn core_levels_clamp_to_silent_and_full_on_read_and_write() {
    for setting in [
        &TTS_MASTER_VOLUME,
        &SOUNDBOARD_MASTER_VOLUME,
        &AUDIO_VOICE_GATE_THRESHOLD,
    ] {
        assert_eq!(
            (
                read_back(setting, "-0.5").await,
                read_back(setting, "1.5").await,
                stored_after_write(setting, &-0.5).await,
                stored_after_write(setting, &1.5).await,
            ),
            (0.0, 1.0, "0".to_owned(), "1".to_owned()),
            "{}",
            setting.name(),
        );
    }
}

#[tokio::test]
async fn core_levels_read_non_finite_stored_text_as_their_default() {
    let cases: [(&Setting<Level>, f32); 3] = [
        (&TTS_MASTER_VOLUME, 1.0),
        (&SOUNDBOARD_MASTER_VOLUME, 1.0),
        (&AUDIO_VOICE_GATE_THRESHOLD, 0.15),
    ];
    for (setting, default) in cases {
        for raw in ["NaN", "inf", "-inf"] {
            assert_eq!(
                read_back(setting, raw).await,
                default,
                "{} = {raw:?}",
                setting.name(),
            );
        }
    }
}
