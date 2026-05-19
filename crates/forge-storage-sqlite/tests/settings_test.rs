#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::SettingsRepo;
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
    assert_eq!(map.len(), 2);
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
    let days = backend
        .event_log_retention_days()
        .await
        .expect("default retention");
    assert_eq!(days, 7);
}

#[tokio::test]
async fn event_log_retention_days_roundtrip() {
    let backend = setup_backend().await;
    for value in [1u32, 7, 30] {
        backend
            .set_event_log_retention_days(value)
            .await
            .expect("set");
        let got = backend.event_log_retention_days().await.expect("get");
        assert_eq!(got, value);
    }
}

#[tokio::test]
async fn event_log_retention_days_invalid_string_falls_back_to_seven() {
    use forge_storage::reserved_keys::EVENT_LOG_RETENTION_DAYS_KEY;

    let backend = setup_backend().await;
    backend
        .set_string(EVENT_LOG_RETENTION_DAYS_KEY, "not_a_number")
        .await
        .expect("set invalid");
    let days = backend.event_log_retention_days().await.expect("fallback");
    assert_eq!(days, 7);
}
