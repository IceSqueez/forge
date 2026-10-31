use forge_events::test_support::NullPublisher;
use std::sync::Arc;

use forge_hotkey::testing::test_client;
use forge_hotkey::{HotkeyConfig, HotkeyReconciler, HotkeySyncedClipsRepo};
use forge_soundboard::ClipLibrary;
use forge_storage::{DataProvider, StoredClip};
use forge_types::{ClipId, OutputDevice};
use time::OffsetDateTime;

#[tokio::test]
async fn a_clip_imported_through_the_library_registers_its_combo() {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let provider: Arc<dyn DataProvider> = Arc::new(
        forge_storage_sqlite::SqliteBackend::open_for_test(
            "sqlite::memory:",
            [0x11; 32],
            media_root.path().to_owned(),
            None,
        )
        .await
        .expect("an in-memory backend"),
    );
    let (client, _backend) = test_client(HotkeyConfig::default(), Arc::new(NullPublisher));
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        provider.trigger_instance_repo(),
        provider.soundboard_clips_repo(),
    );
    let clips = HotkeySyncedClipsRepo::wrap(provider.soundboard_clips_repo(), Some(reconciler));
    let library = ClipLibrary::new(clips, provider.media_repo());
    let source = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    std::fs::write(source.path(), b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0").unwrap();
    let imported = StoredClip {
        id: ClipId::new(),
        name: "airhorn".to_owned(),
        file_path: source.path().to_owned(),
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: Some("F9".to_owned()),
        created_at: OffsetDateTime::now_utc(),
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    };

    library.save_clip(&imported).await.unwrap();

    let registered: Vec<String> = client
        .registered_combos()
        .into_iter()
        .map(|(_, combo)| combo.as_str().to_owned())
        .collect();
    assert_eq!(registered, ["F9"]);
}
