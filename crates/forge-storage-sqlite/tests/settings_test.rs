#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::reserved_keys::{
    AUDIO_VOICE_GATE_HOLD_MS, AUDIO_VOICE_GATE_INPUT_DEVICE_ID, AUDIO_VOICE_GATE_THRESHOLD,
    EVENT_LOG_RETENTION_DAYS,
};
use forge_storage::{
    Language, SettingsRepo, VOICE_GATE_DEFAULT_HOLD_MS, VOICE_GATE_DEFAULT_THRESHOLD,
    VoiceGateSettings, event_log_retention_days, set_event_log_retention_days,
    set_voice_gate_enabled, set_voice_gate_hold_ms, set_voice_gate_input_device_id,
    set_voice_gate_threshold, voice_gate_settings,
};
use forge_storage_sqlite::{SqliteBackend, SqliteSettingsRepo, apply_migrations};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup_backend() -> SqliteBackend {
    SqliteBackend::open_with_key(":memory:", TEST_KEY)
        .await
        .expect("in-memory backend")
}

async fn setup() -> SqliteSettingsRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteSettingsRepo::new(pool)
}

#[tokio::test]
async fn set_then_get_roundtrips_value() {
    let repo = setup().await;
    repo.set_string("theme", "catppuccin_mocha")
        .await
        .expect("set");
    let got = repo.get_string("theme").await.expect("get");
    assert_eq!(got, Some("catppuccin_mocha".to_owned()));
}

#[tokio::test]
async fn get_missing_key_returns_none() {
    let repo = setup().await;
    let got = repo.get_string("nonexistent").await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn load_all_returns_all_entries() {
    let repo = setup().await;
    repo.set_string("theme", "catppuccin_mocha")
        .await
        .expect("set theme");
    repo.set_string("density", "cozy")
        .await
        .expect("set density");

    let map = repo.load_all().await.expect("load_all");
    assert_eq!(
        map.get("theme").map(String::as_str),
        Some("catppuccin_mocha")
    );
    assert_eq!(map.get("density").map(String::as_str), Some("cozy"));
    assert_eq!(map.len(), 3);
}

#[tokio::test]
async fn delete_existing_returns_true_and_key_gone() {
    let repo = setup().await;
    repo.set_string("font_body", "inter").await.expect("set");
    let deleted = repo.delete("font_body").await.expect("delete");
    assert!(deleted);
    let got = repo
        .get_string("font_body")
        .await
        .expect("get after delete");
    assert!(got.is_none());
}

#[tokio::test]
async fn delete_missing_returns_false() {
    let repo = setup().await;
    let deleted = repo.delete("ghost_key").await.expect("delete missing");
    assert!(!deleted);
}

#[tokio::test]
async fn set_string_overwrites_existing_value() {
    let repo = setup().await;
    repo.set_string("accent_color", "lavender")
        .await
        .expect("set 1");
    repo.set_string("accent_color", "sky").await.expect("set 2");
    let got = repo.get_string("accent_color").await.expect("get");
    assert_eq!(got, Some("sky".to_owned()));
}

#[tokio::test]
async fn language_seeds_en_and_round_trips_through_typed_repo() {
    let repo = setup().await;
    assert_eq!(repo.language().await.expect("read seeded"), Language::En);
    repo.set_language(Language::Uk).await.expect("write Uk");
    assert_eq!(repo.language().await.expect("read after Uk"), Language::Uk);
    repo.set_language(Language::En).await.expect("write En");
    assert_eq!(repo.language().await.expect("read after En"), Language::En);
}

#[tokio::test]
async fn event_log_retention_days_default_is_seven() {
    let backend = setup_backend().await;
    let days = event_log_retention_days(&backend)
        .await
        .expect("default retention");
    assert_eq!(days, 7);
}

#[tokio::test]
async fn event_log_retention_days_roundtrip() {
    let backend = setup_backend().await;
    for value in [1u32, 7, 30] {
        set_event_log_retention_days(&backend, value)
            .await
            .expect("set");
        let got = event_log_retention_days(&backend).await.expect("get");
        assert_eq!(got, value);
    }
}

#[tokio::test]
async fn event_log_retention_days_invalid_string_falls_back_to_seven() {
    let backend = setup_backend().await;
    backend
        .set_string(EVENT_LOG_RETENTION_DAYS, "not_a_number")
        .await
        .expect("set invalid");
    let days = event_log_retention_days(&backend).await.expect("fallback");
    assert_eq!(days, 7);
}

#[tokio::test]
async fn density_round_trips_through_typed_accessors() {
    use forge_storage::settings::Density;
    let backend = setup_backend().await;
    backend
        .set_density(Density::Spacious)
        .await
        .expect("set density");
    assert_eq!(
        backend.density().await.expect("get density"),
        Density::Spacious
    );
}

#[tokio::test]
async fn density_absent_key_defaults_to_cozy() {
    use forge_storage::settings::Density;
    let backend = setup_backend().await;
    assert_eq!(backend.density().await.expect("get density"), Density::Cozy);
}

#[tokio::test]
async fn density_corrupt_stored_value_falls_back_to_cozy() {
    use forge_storage::settings::Density;
    let backend = setup_backend().await;
    backend
        .set_string(forge_storage::reserved_keys::DENSITY, "ultra-wide")
        .await
        .expect("inject corrupt density");
    assert_eq!(backend.density().await.expect("get density"), Density::Cozy);
}

#[tokio::test]
async fn font_overrides_persist_per_role_and_unset_deletes() {
    let backend = setup_backend().await;
    backend
        .set_font_body(Some("Custom Sans".to_owned()))
        .await
        .expect("set body font");
    backend
        .set_font_mono(Some("Custom Mono".to_owned()))
        .await
        .expect("set mono font");
    assert_eq!(
        backend.font_body().await.expect("get body"),
        Some("Custom Sans".to_owned())
    );
    assert_eq!(
        backend.font_mono().await.expect("get mono"),
        Some("Custom Mono".to_owned())
    );

    backend.set_font_body(None).await.expect("unset body");
    assert_eq!(backend.font_body().await.expect("get body"), None);
    assert_eq!(
        backend.font_mono().await.expect("get mono"),
        Some("Custom Mono".to_owned()),
        "unsetting one role must not clear the other"
    );
}

#[tokio::test]
async fn unsetting_a_font_that_was_never_stored_succeeds() {
    let backend = setup_backend().await;
    backend
        .set_font_mono(None)
        .await
        .expect("unset on absent key must not error");
    assert_eq!(backend.font_mono().await.expect("get mono"), None);
}

#[tokio::test]
async fn voice_gate_settings_with_no_stored_keys_yields_the_type_defaults() {
    let backend = setup_backend().await;
    let settings = voice_gate_settings(&backend).await.expect("load defaults");
    assert_eq!(settings, VoiceGateSettings::default());
}

#[tokio::test]
async fn voice_gate_settings_round_trip_through_the_typed_setters() {
    let backend = setup_backend().await;
    set_voice_gate_enabled(&backend, true)
        .await
        .expect("enable");
    set_voice_gate_input_device_id(&backend, Some("alsa_input.usb-Yeti".to_owned()))
        .await
        .expect("set device");
    set_voice_gate_threshold(&backend, 0.42)
        .await
        .expect("set threshold");
    set_voice_gate_hold_ms(&backend, 250)
        .await
        .expect("set hold");

    let settings = voice_gate_settings(&backend).await.expect("load");
    assert_eq!(
        settings,
        VoiceGateSettings {
            enabled: true,
            input_device_id: Some("alsa_input.usb-Yeti".to_owned()),
            threshold: 0.42,
            hold_ms: 250,
        }
    );
}

#[tokio::test]
async fn voice_gate_threshold_falls_back_to_the_default_for_unparsable_or_non_finite_values() {
    let backend = setup_backend().await;
    for stored in ["abc", "", " 0.5", "NaN", "inf", "-inf", "1e40"] {
        backend
            .set_string(AUDIO_VOICE_GATE_THRESHOLD, stored)
            .await
            .expect("inject threshold");
        let settings = voice_gate_settings(&backend).await.expect("load");
        assert_eq!(
            settings.threshold, VOICE_GATE_DEFAULT_THRESHOLD,
            "stored threshold {stored:?} must not survive as a usable gain",
        );
    }
}

#[tokio::test]
async fn voice_gate_threshold_read_clamps_stored_values_into_the_unit_range() {
    let backend = setup_backend().await;
    for (stored, expected) in [
        ("-0.5", 0.0),
        ("0", 0.0),
        ("1", 1.0),
        ("1.5", 1.0),
        ("340282350000000000000000000000000000000", 1.0),
    ] {
        backend
            .set_string(AUDIO_VOICE_GATE_THRESHOLD, stored)
            .await
            .expect("inject threshold");
        let settings = voice_gate_settings(&backend).await.expect("load");
        assert_eq!(
            settings.threshold, expected,
            "stored threshold {stored:?} clamped wrong",
        );
    }
}

#[tokio::test]
async fn voice_gate_hold_ms_falls_back_to_the_default_for_unparsable_values() {
    let backend = setup_backend().await;
    for stored in ["abc", "", "-100", "3.5", "4294967296"] {
        backend
            .set_string(AUDIO_VOICE_GATE_HOLD_MS, stored)
            .await
            .expect("inject hold");
        let settings = voice_gate_settings(&backend).await.expect("load");
        assert_eq!(
            settings.hold_ms, VOICE_GATE_DEFAULT_HOLD_MS,
            "stored hold {stored:?} must fall back, not collapse to zero",
        );
    }
}

#[tokio::test]
async fn set_voice_gate_threshold_persists_an_already_clamped_value() {
    let backend = setup_backend().await;
    for (written, expected) in [(2.0f32, 1.0f32), (-1.0, 0.0), (0.42, 0.42)] {
        set_voice_gate_threshold(&backend, written)
            .await
            .expect("set threshold");
        let raw = backend
            .get_string(AUDIO_VOICE_GATE_THRESHOLD)
            .await
            .expect("read raw")
            .expect("threshold key present");
        let parsed: f32 = raw.parse().expect("stored threshold is numeric");
        assert_eq!(
            parsed, expected,
            "writing {written} must store a clamped value, got {raw:?}",
        );
    }
}

#[tokio::test]
async fn clearing_the_voice_gate_input_device_removes_the_key_rather_than_blanking_it() {
    let backend = setup_backend().await;
    set_voice_gate_input_device_id(&backend, Some("hw:CARD=Yeti".to_owned()))
        .await
        .expect("set device");
    assert_eq!(
        voice_gate_settings(&backend)
            .await
            .expect("load after set")
            .input_device_id,
        Some("hw:CARD=Yeti".to_owned())
    );

    set_voice_gate_input_device_id(&backend, None)
        .await
        .expect("clear device");
    assert_eq!(
        backend
            .get_string(AUDIO_VOICE_GATE_INPUT_DEVICE_ID)
            .await
            .expect("read raw after clear"),
        None,
        "clearing must delete the key, not store an empty string",
    );
}
