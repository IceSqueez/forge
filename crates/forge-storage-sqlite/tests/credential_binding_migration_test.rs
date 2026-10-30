use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use forge_storage::{CredentialId, CredentialsRepo};
use forge_storage_sqlite::{MIGRATIONS, SqliteCredentialsRepo, apply_migrations, connect};

const VERSION_BEFORE_BINDING: i64 = 61;
const KEY: [u8; 32] = [0x5c; 32];
const NONCE: [u8; 12] = [0x07; 12];

#[tokio::test]
async fn a_row_from_before_the_binding_keeps_its_bytes_and_stays_readable_after_the_migration() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_BINDING, &pool)
        .await
        .expect("the schema before the binding");
    let cipher = Aes256Gcm::new_from_slice(&KEY).expect("a 32-byte key");
    let sealed = cipher
        .encrypt(
            &Nonce::try_from(NONCE.as_slice()).expect("a 12-byte nonce"),
            "token".as_bytes(),
        )
        .expect("the legacy sealing");
    sqlx::query("INSERT INTO credentials (id, encrypted, nonce, last_refresh) VALUES (?, ?, ?, 0)")
        .bind("twitch:bot")
        .bind(&sealed)
        .bind(NONCE.as_slice())
        .execute(&pool)
        .await
        .expect("a stored credential");

    apply_migrations(&pool).await.expect("the migrations apply");

    let (stored, bound): (Vec<u8>, i64) =
        sqlx::query_as("SELECT encrypted, bound_to_id FROM credentials WHERE id = 'twitch:bot'")
            .fetch_one(&pool)
            .await
            .expect("the row survives");
    let read = SqliteCredentialsRepo::new_with_key(pool, KEY)
        .load(&CredentialId::new("twitch:bot"))
        .await
        .expect("the row decrypts");
    assert_eq!((stored, bound, read), (sealed, 0, Some("token".to_owned())));
}
