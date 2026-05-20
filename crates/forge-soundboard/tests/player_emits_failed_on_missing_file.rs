//! Regression tests: SoundboardPlayer must emit PlaybackFailed (not crash silently)
//! when the clip file is missing on disk, and when the sink factory returns an error
//! (e.g. device disconnect before playback starts).

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_audio::{AudioError, AudioEvent, AudioEventSink, AudioSink};
use forge_soundboard::{AudioSinkFactory, SoundboardPlayer};
use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::{ClipId, OutputDevice};
use time::OffsetDateTime;

struct RecordingEventSink {
    events: Arc<Mutex<Vec<AudioEvent>>>,
}

impl RecordingEventSink {
    fn new() -> (Self, Arc<Mutex<Vec<AudioEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        (Self { events: Arc::clone(&events) }, events)
    }
}

impl AudioEventSink for RecordingEventSink {
    fn emit(&self, event: AudioEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct NullFactory;

#[async_trait]
impl AudioSinkFactory for NullFactory {
    async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        Ok(Arc::new(forge_audio::NullSink))
    }
}

struct DeviceNotFoundFactory;

#[async_trait]
impl AudioSinkFactory for DeviceNotFoundFactory {
    async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        Err(AudioError::DeviceNotFound("test-device".to_string()))
    }
}

struct MockClipsRepo {
    clip: StoredClip,
}

#[async_trait]
impl SoundboardClipsRepo for MockClipsRepo {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        Ok(vec![self.clip.clone()])
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        Ok(if self.clip.id == id { Some(self.clip.clone()) } else { None })
    }

    async fn save(&self, _clip: &StoredClip) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ClipId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

fn stored_clip_with_path(id: ClipId, path: PathBuf) -> StoredClip {
    StoredClip {
        id,
        name: "missing clip".to_string(),
        file_path: path,
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: OffsetDateTime::now_utc(),
    }
}

/// Regression: play() with a clip whose file does not exist on disk must
/// emit PlaybackFailed and return Err (not panic or silently succeed).
#[tokio::test]
async fn play_missing_file_emits_playback_failed_and_returns_err() {
    let clip_id = ClipId::new();
    let clip = stored_clip_with_path(
        clip_id,
        PathBuf::from("/nonexistent_forge_qa_test/missing.wav"),
    );

    let (event_sink, events) = RecordingEventSink::new();
    let player = SoundboardPlayer::new(
        Arc::new(NullFactory),
        Arc::new(event_sink),
        Arc::new(MockClipsRepo { clip }),
    );

    let result = player.play(clip_id, None).await;

    assert!(result.is_err(), "play with missing file must return Err");

    let recorded = events.lock().unwrap();
    assert_eq!(recorded.len(), 1, "exactly one event must be emitted");
    assert!(
        matches!(recorded[0], AudioEvent::PlaybackFailed { clip_id: Some(id), .. } if id == clip_id),
        "emitted event must be PlaybackFailed with correct clip_id, got: {:?}",
        recorded[0]
    );
}

/// Regression: play() when sink factory returns DeviceNotFound must emit
/// PlaybackFailed and return Err — covers audio device disconnect before playback.
#[tokio::test]
async fn play_device_not_found_emits_playback_failed_and_returns_err() {
    let clip_id = ClipId::new();
    let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    std::fs::write(
        tmp.path(),
        b"RIFF$\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x22V\0\0D\xac\0\0\x02\0\x10\0data\0\0\0\0",
    )
    .unwrap();

    let clip = stored_clip_with_path(clip_id, tmp.path().to_path_buf());

    let (event_sink, events) = RecordingEventSink::new();
    let player = SoundboardPlayer::new(
        Arc::new(DeviceNotFoundFactory),
        Arc::new(event_sink),
        Arc::new(MockClipsRepo { clip }),
    );

    let result = player.play(clip_id, None).await;

    assert!(result.is_err(), "play with device-not-found must return Err");

    let recorded = events.lock().unwrap();
    assert_eq!(recorded.len(), 1, "exactly one event must be emitted");
    assert!(
        matches!(recorded[0], AudioEvent::PlaybackFailed { clip_id: Some(id), .. } if id == clip_id),
        "emitted event must be PlaybackFailed with correct clip_id, got: {:?}",
        recorded[0]
    );
}
