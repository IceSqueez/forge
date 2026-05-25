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

#[tokio::test]
async fn server_bind_address_default_is_loopback() {
    let backend = setup_backend().await;
    let addr = backend.server_bind_address().await.expect("default addr");
    assert_eq!(addr, "127.0.0.1");
}

#[tokio::test]
async fn server_bind_address_roundtrip() {
    let backend = setup_backend().await;
    for addr in ["127.0.0.1", "0.0.0.0", "::1", "::"] {
        backend
            .set_server_bind_address(addr)
            .await
            .expect("set valid addr");
        let got = backend.server_bind_address().await.expect("get addr");
        assert_eq!(got, addr);
    }
}

#[tokio::test]
async fn server_bind_address_rejects_invalid() {
    use forge_storage::StorageError;

    let backend = setup_backend().await;
    let err = backend
        .set_server_bind_address("192.168.1.1")
        .await
        .expect_err("should reject");
    assert!(
        matches!(err, StorageError::ValidationFailed { .. }),
        "expected ValidationFailed, got {err:?}"
    );
}

#[tokio::test]
async fn server_port_default_is_8081() {
    let backend = setup_backend().await;
    let port = backend.server_port().await.expect("default port");
    assert_eq!(port, 8081);
}

#[tokio::test]
async fn server_port_roundtrip() {
    let backend = setup_backend().await;
    for port in [8081u16, 9000, 443] {
        backend.set_server_port(port).await.expect("set port");
        let got = backend.server_port().await.expect("get port");
        assert_eq!(got, port);
    }
}

#[tokio::test]
async fn server_lan_bind_enabled_default_is_false() {
    let backend = setup_backend().await;
    let enabled = backend
        .server_lan_bind_enabled()
        .await
        .expect("default lan_bind");
    assert!(!enabled);
}

#[tokio::test]
async fn server_lan_bind_enabled_roundtrip() {
    let backend = setup_backend().await;
    backend
        .set_server_lan_bind_enabled(true)
        .await
        .expect("enable");
    assert!(
        backend
            .server_lan_bind_enabled()
            .await
            .expect("get enabled")
    );
    backend
        .set_server_lan_bind_enabled(false)
        .await
        .expect("disable");
    assert!(
        !backend
            .server_lan_bind_enabled()
            .await
            .expect("get disabled")
    );
}

#[tokio::test]
async fn server_auth_required_for_reads_default_is_false() {
    let backend = setup_backend().await;
    let required = backend
        .server_auth_required_for_reads()
        .await
        .expect("default auth_reads");
    assert!(!required);
}

#[tokio::test]
async fn server_auth_required_for_reads_roundtrip() {
    let backend = setup_backend().await;
    backend
        .set_server_auth_required_for_reads(true)
        .await
        .expect("enable auth_reads");
    assert!(
        backend
            .server_auth_required_for_reads()
            .await
            .expect("get enabled")
    );
    backend
        .set_server_auth_required_for_reads(false)
        .await
        .expect("disable auth_reads");
    assert!(
        !backend
            .server_auth_required_for_reads()
            .await
            .expect("get disabled")
    );
}

#[tokio::test]
async fn sheet_width_roundtrip() {
    let backend = setup_backend().await;
    backend
        .set_sheet_width("viewers_drawer", 420.0)
        .await
        .expect("set sheet_width");
    let got = backend
        .sheet_width("viewers_drawer")
        .await
        .expect("get sheet_width");
    assert_eq!(got, Some(420.0_f32));
}

#[tokio::test]
async fn sheet_width_absent_key_returns_none() {
    let backend = setup_backend().await;
    let got = backend
        .sheet_width("no_such_sheet")
        .await
        .expect("absent key");
    assert!(got.is_none());
}

#[tokio::test]
async fn sheet_width_corrupt_value_returns_none() {
    let backend = setup_backend().await;
    backend
        .set_string("sheet_width:corrupt_key", "not_a_float")
        .await
        .expect("inject corrupt value");
    let got = backend
        .sheet_width("corrupt_key")
        .await
        .expect("corrupt value fallback");
    assert!(got.is_none());
}

#[tokio::test]
async fn sheet_width_keys_do_not_collide() {
    let backend = setup_backend().await;
    backend
        .set_sheet_width("action_editor", 480.0)
        .await
        .expect("set action_editor");
    backend
        .set_sheet_width("trigger_editor", 360.0)
        .await
        .expect("set trigger_editor");

    let action = backend
        .sheet_width("action_editor")
        .await
        .expect("get action_editor");
    let trigger = backend
        .sheet_width("trigger_editor")
        .await
        .expect("get trigger_editor");

    assert_eq!(action, Some(480.0_f32));
    assert_eq!(trigger, Some(360.0_f32));
}
