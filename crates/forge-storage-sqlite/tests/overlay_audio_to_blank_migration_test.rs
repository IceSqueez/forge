#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use forge_storage::{OverlayConfig, OverlayDefinition, OverlayRepo};
use forge_storage_sqlite::{MIGRATIONS, SqliteOverlayRepo, connect};
use forge_types::Variant;
use sqlx::SqlitePool;

const BLANK_MIGRATION_SQL: &str = include_str!("../migrations/0045_overlay_audio_to_blank.sql");
const SCHEMA_VERSION_BEFORE_BLANK: i64 = 44;

const AUDIO_KIND: &str = "overlay.audio";
const BLANK_KIND: &str = "overlay.blank";
const ALERT_KIND: &str = "overlay.alert";

const TRANSPORT_KEYS: &[&str] = &[
    "clip_id",
    "clip_path",
    "report_path",
    "clip_media_type",
    "clip_duration_ms",
    "command",
];
const KEPT_KEY: &str = "accent";

async fn pool_before_blank() -> SqlitePool {
    let pool = connect("sqlite::memory:").await.expect("connect");
    for migration in MIGRATIONS
        .iter()
        .filter(|m| m.version <= SCHEMA_VERSION_BEFORE_BLANK)
    {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&pool)
            .await
            .unwrap_or_else(|error| panic!("migration {} failed: {error}", migration.version));
    }
    pool
}

async fn apply_blank_migration(pool: &SqlitePool) {
    sqlx::raw_sql(BLANK_MIGRATION_SQL)
        .execute(pool)
        .await
        .expect("apply the audio-to-blank migration");
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn transport_config() -> OverlayConfig {
    let mut config: OverlayConfig = TRANSPORT_KEYS
        .iter()
        .map(|key| ((*key).to_owned(), text("/audio/v1/clip/n3Zq")))
        .collect();
    config.insert("clip_duration_ms".to_owned(), Variant::Int(2_500));
    config.insert(KEPT_KEY.to_owned(), text("sky"));
    config
}

async fn seed(repo: &SqliteOverlayRepo, name: &str, kind_id: &str) -> OverlayDefinition {
    let mut created = repo.create(name, kind_id, 1).await.expect("seed overlay");
    created.config = transport_config();
    repo.save(&created).await.expect("seed overlay config");
    repo.set_retained_content(&created.id, &transport_config())
        .await
        .expect("seed retained content");
    repo.get(&created.id)
        .await
        .expect("read seeded overlay")
        .expect("seeded overlay exists")
}

async fn reread(repo: &SqliteOverlayRepo, seeded: &OverlayDefinition) -> OverlayDefinition {
    repo.get(&seeded.id)
        .await
        .expect("the migrated row still decodes")
        .expect("the migrated row still exists")
}

#[tokio::test]
async fn an_audio_overlay_becomes_a_blank_overlay_under_the_same_identity_and_credential() {
    let pool = pool_before_blank().await;
    let repo = SqliteOverlayRepo::new(pool.clone());
    let audio = seed(&repo, "Stage audio", AUDIO_KIND).await;

    apply_blank_migration(&pool).await;

    let migrated = reread(&repo, &audio).await;
    assert_eq!(
        (
            migrated.kind_id.as_str(),
            &migrated.id,
            &migrated.credential,
            migrated.config_schema_version
        ),
        (BLANK_KIND, &audio.id, &audio.credential, 1),
        "the OBS browser source URL carries id and credential, so either changing breaks it"
    );
}

#[tokio::test]
async fn a_migrated_audio_overlay_keeps_its_style_and_drops_every_transport_key() {
    let pool = pool_before_blank().await;
    let repo = SqliteOverlayRepo::new(pool.clone());
    let audio = seed(&repo, "Stage audio", AUDIO_KIND).await;

    apply_blank_migration(&pool).await;

    let migrated = reread(&repo, &audio).await;
    assert_eq!(
        migrated.config,
        OverlayConfig::from([(KEPT_KEY.to_owned(), text("sky"))]),
        "a stored clip address survived into the blank look's generated page"
    );
}

#[tokio::test]
async fn a_migrated_audio_overlay_has_nothing_retained_to_replay() {
    let pool = pool_before_blank().await;
    let repo = SqliteOverlayRepo::new(pool.clone());
    let audio = seed(&repo, "Stage audio", AUDIO_KIND).await;

    apply_blank_migration(&pool).await;

    assert_eq!(
        repo.get_retained_content(&audio.id)
            .await
            .expect("read retained"),
        None,
        "a reconnecting page would be handed a spent announcement"
    );
}

#[tokio::test]
async fn an_overlay_of_another_kind_is_left_byte_for_byte_untouched() {
    let pool = pool_before_blank().await;
    let repo = SqliteOverlayRepo::new(pool.clone());
    let alert = seed(&repo, "Sub alert", ALERT_KIND).await;
    let retained_before = repo.get_retained_content(&alert.id).await.unwrap();

    apply_blank_migration(&pool).await;

    assert_eq!(
        (
            reread(&repo, &alert).await,
            repo.get_retained_content(&alert.id).await.unwrap()
        ),
        (alert, retained_before),
        "the rewrite reached a row that was never an audio overlay"
    );
}

#[tokio::test]
async fn replaying_the_migration_changes_nothing_further() {
    let pool = pool_before_blank().await;
    let repo = SqliteOverlayRepo::new(pool.clone());
    let audio = seed(&repo, "Stage audio", AUDIO_KIND).await;
    apply_blank_migration(&pool).await;
    let once = reread(&repo, &audio).await;

    apply_blank_migration(&pool).await;

    assert_eq!(reread(&repo, &audio).await, once);
}
