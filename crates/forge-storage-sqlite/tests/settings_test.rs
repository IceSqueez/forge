#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::reserved_keys::EVENT_LOG_RETENTION_DAYS_KEY;
use forge_storage::{
    Language, SettingsRepo, event_log_retention_days, set_event_log_retention_days,
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
    // Migration 0016 seeds app.language = 'en', so the map includes that row too.
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
async fn last_onboarding_step_roundtrips() {
    use forge_storage::reserved_keys::LAST_ONBOARDING_STEP;

    let repo = setup().await;
    repo.set_string(LAST_ONBOARDING_STEP, "connect_obs")
        .await
        .expect("set last_onboarding_step");
    let got = repo
        .get_string(LAST_ONBOARDING_STEP)
        .await
        .expect("get last_onboarding_step");
    assert_eq!(got, Some("connect_obs".to_owned()));
}

#[tokio::test]
async fn language_seeds_en_and_round_trips_through_typed_repo() {
    let repo = setup().await;
    // Migration 0016 seeds 'en' so a fresh install reads En through the typed accessor.
    assert_eq!(repo.language().await.expect("read seeded"), Language::En);
    repo.set_language(Language::Uk).await.expect("write Uk");
    assert_eq!(repo.language().await.expect("read after Uk"), Language::Uk);
    repo.set_language(Language::En).await.expect("write En");
    assert_eq!(repo.language().await.expect("read after En"), Language::En);
}

#[tokio::test]
async fn last_onboarding_step_overwrites_correctly() {
    use forge_storage::reserved_keys::LAST_ONBOARDING_STEP;

    let repo = setup().await;
    repo.set_string(LAST_ONBOARDING_STEP, "welcome")
        .await
        .expect("set welcome");
    repo.set_string(LAST_ONBOARDING_STEP, "starter_pack")
        .await
        .expect("set starter_pack");
    let got = repo
        .get_string(LAST_ONBOARDING_STEP)
        .await
        .expect("get after overwrite");
    assert_eq!(got, Some("starter_pack".to_owned()));
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
        .set_string(EVENT_LOG_RETENTION_DAYS_KEY, "not_a_number")
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
    // Unlike language(), a corrupt density must not error - the UI boots anyway.
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
