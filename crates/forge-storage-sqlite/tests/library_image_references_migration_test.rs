mod common;
use forge_storage::{MediaFormat, MediaKind};
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect_pools};

const VERSION_BEFORE_LIBRARY_HOLDS: i64 = 76;
const LIBRARY_KIND: &str = "library";
const IMAGE_SLOT: &str = "image";
const HELD_WAV: &str = "sha256-0000000000000000000000000000000000000000000000000000000000000001";
const HELD_PNG: &str = "sha256-0000000000000000000000000000000000000000000000000000000000000002";

const BLOBS: [(&str, MediaFormat); 9] = [
    (HELD_WAV, MediaFormat::Wav),
    (
        "sha256-00000000000000000000000000000000000000000000000000000000000000a3",
        MediaFormat::Mp3,
    ),
    (
        "sha256-00000000000000000000000000000000000000000000000000000000000000a4",
        MediaFormat::Ogg,
    ),
    (
        "sha256-00000000000000000000000000000000000000000000000000000000000000a5",
        MediaFormat::Flac,
    ),
    (
        "sha256-00000000000000000000000000000000000000000000000000000000000000a6",
        MediaFormat::M4a,
    ),
    (HELD_PNG, MediaFormat::Png),
    (
        "sha256-00000000000000000000000000000000000000000000000000000000000000b7",
        MediaFormat::Gif,
    ),
    (
        "sha256-00000000000000000000000000000000000000000000000000000000000000b8",
        MediaFormat::Webp,
    ),
    (
        "sha256-00000000000000000000000000000000000000000000000000000000000000b9",
        MediaFormat::Svg,
    ),
];

#[tokio::test]
async fn upgrading_gives_every_existing_image_a_library_hold_and_no_audio_one() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let pools = connect_pools(&common::database_url(&dir))
        .await
        .expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_LIBRARY_HOLDS, pools.writer())
        .await
        .expect("the schema before library holds");
    for (id, format) in BLOBS {
        sqlx::query(
            "INSERT INTO media_blobs (id, format, byte_size, label, imported_at)
             VALUES (?, ?, 1, 'fixture', 0)",
        )
        .bind(id)
        .bind(format.as_str())
        .execute(pools.writer())
        .await
        .expect("a blob row");
    }
    for (referrer_id, blob) in [("clip-alpha", HELD_WAV), ("clip-beta", HELD_PNG)] {
        sqlx::query(
            "INSERT INTO media_references (referrer_kind, referrer_id, slot, blob_id)
             VALUES ('soundboard_clip', ?, 'source', ?)",
        )
        .bind(referrer_id)
        .bind(blob)
        .execute(pools.writer())
        .await
        .expect("a user reference");
    }

    apply_migrations(pools.writer()).await.expect("migrate");

    let mut held: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT referrer_id, slot, blob_id FROM media_references WHERE referrer_kind = ?",
    )
    .bind(LIBRARY_KIND)
    .fetch_all(pools.writer())
    .await
    .expect("library holds");
    held.sort();
    let mut expected: Vec<(String, String, String)> = BLOBS
        .iter()
        .filter(|(_, format)| format.kind() == MediaKind::Image)
        .map(|(id, _)| ((*id).to_owned(), IMAGE_SLOT.to_owned(), (*id).to_owned()))
        .collect();
    expected.sort();
    assert_eq!(held, expected);
}
