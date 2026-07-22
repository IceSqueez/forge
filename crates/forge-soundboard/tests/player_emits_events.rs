#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_audio::{AudioError, AudioEvent, AudioEventSink, AudioSink, PcmBuffer};
use forge_soundboard::{AudioSinkFactory, SoundboardPlayer, SoundboardSettingsHandle};
use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::{ClipId, OutputDevice};
use time::OffsetDateTime;

struct RecordingEventSink {
    events: Arc<Mutex<Vec<AudioEvent>>>,
}

impl RecordingEventSink {
    fn new() -> (Self, Arc<Mutex<Vec<AudioEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                events: Arc::clone(&events),
            },
            events,
        )
    }
}

impl AudioEventSink for RecordingEventSink {
    fn emit(&self, event: AudioEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct NullSink;

#[async_trait]
impl AudioSink for NullSink {
    async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
        Ok(())
    }
}

struct NullFactory;

#[async_trait]
impl AudioSinkFactory for NullFactory {
    async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        Ok(Arc::new(NullSink))
    }
}

struct MockClipsRepo {
    clip: Option<StoredClip>,
}

#[async_trait]
impl SoundboardClipsRepo for MockClipsRepo {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        Ok(self.clip.as_ref().filter(|c| c.id == id).cloned())
    }

    async fn save(&self, _clip: &StoredClip) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ClipId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

fn write_wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * u32::from(block_align);
    let data_len = (samples.len() * 2) as u32;
    let file_len = 36 + data_len;

    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    buf.write_all(b"RIFF").unwrap();
    buf.write_all(&file_len.to_le_bytes()).unwrap();
    buf.write_all(b"WAVE").unwrap();
    buf.write_all(b"fmt ").unwrap();
    buf.write_all(&16u32.to_le_bytes()).unwrap();
    buf.write_all(&1u16.to_le_bytes()).unwrap();
    buf.write_all(&channels.to_le_bytes()).unwrap();
    buf.write_all(&sample_rate.to_le_bytes()).unwrap();
    buf.write_all(&byte_rate.to_le_bytes()).unwrap();
    buf.write_all(&block_align.to_le_bytes()).unwrap();
    buf.write_all(&bits_per_sample.to_le_bytes()).unwrap();
    buf.write_all(b"data").unwrap();
    buf.write_all(&data_len.to_le_bytes()).unwrap();
    for &s in samples {
        buf.write_all(&s.to_le_bytes()).unwrap();
    }
    buf
}

fn make_wav_tempfile(sample_rate: u32, channels: u16, n_frames: usize) -> tempfile::NamedTempFile {
    let samples: Vec<i16> = (0..n_frames * channels as usize)
        .map(|i| ((i % 256) as i16) * 100)
        .collect();
    let wav_bytes = write_wav(sample_rate, channels, &samples);
    let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    std::fs::write(tmp.path(), &wav_bytes).unwrap();
    tmp
}

fn make_stored_clip(id: ClipId, path: PathBuf) -> StoredClip {
    StoredClip {
        id,
        name: "test clip".to_string(),
        file_path: path,
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: OffsetDateTime::now_utc(),
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    }
}

#[tokio::test]
async fn play_emits_playback_started_then_finished_in_order() {
    let clip_id = ClipId::new();
    let tmp = make_wav_tempfile(22_050, 1, 2_205);
    let clip = make_stored_clip(clip_id, tmp.path().to_path_buf());

    let (event_sink, events) = RecordingEventSink::new();
    let player = SoundboardPlayer::with_settings(
        Arc::new(NullFactory),
        Arc::new(event_sink),
        Arc::new(MockClipsRepo { clip: Some(clip) }),
        SoundboardSettingsHandle::default(),
    );

    player.play(clip_id, None).await.unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2_000);
    while events.lock().unwrap().len() < 2 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let recorded = events.lock().unwrap();
    assert_eq!(recorded.len(), 2, "exactly 2 events must be emitted");

    assert!(
        matches!(recorded[0], AudioEvent::PlaybackStarted { clip_id: Some(id), .. } if id == clip_id),
        "first event must be PlaybackStarted with correct clip_id"
    );
    assert!(
        matches!(recorded[1], AudioEvent::PlaybackFinished { clip_id: Some(id) } if id == clip_id),
        "second event must be PlaybackFinished with correct clip_id"
    );
}

#[tokio::test]
async fn play_with_device_override_routes_to_specified_device() {
    let clip_id = ClipId::new();
    let tmp = make_wav_tempfile(22_050, 1, 100);
    let clip = make_stored_clip(clip_id, tmp.path().to_path_buf());

    let captured_device: Arc<Mutex<Option<OutputDevice>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured_device);

    struct CapturingFactory {
        captured: Arc<Mutex<Option<OutputDevice>>>,
    }

    #[async_trait]
    impl AudioSinkFactory for CapturingFactory {
        async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            *self.captured.lock().unwrap() = Some(device.clone());
            Ok(Arc::new(NullSink))
        }
    }

    let (event_sink, _events) = RecordingEventSink::new();
    let player = SoundboardPlayer::with_settings(
        Arc::new(CapturingFactory {
            captured: captured_clone,
        }),
        Arc::new(event_sink),
        Arc::new(MockClipsRepo { clip: Some(clip) }),
        SoundboardSettingsHandle::default(),
    );

    let override_dev = OutputDevice::ByName {
        name: "Headphones".to_string(),
    };
    player
        .play(clip_id, Some(override_dev.clone()))
        .await
        .unwrap();

    let got = captured_device.lock().unwrap();
    assert_eq!(*got, Some(override_dev));
}

#[tokio::test]
async fn play_unknown_clip_emits_no_events_and_returns_err() {
    let clip_id = ClipId::new();
    let (event_sink, events) = RecordingEventSink::new();
    let player = SoundboardPlayer::with_settings(
        Arc::new(NullFactory),
        Arc::new(event_sink),
        Arc::new(MockClipsRepo { clip: None }),
        SoundboardSettingsHandle::default(),
    );

    let result = player.play(clip_id, None).await;
    assert!(result.is_err(), "play with unknown clip_id must return Err");
    assert!(
        events.lock().unwrap().is_empty(),
        "no events must be emitted when clip is not found"
    );
}
