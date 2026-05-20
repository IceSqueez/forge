use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::{AudioEvent, AudioEventSink, PcmBuffer};
use forge_runtime::{SoundPlayer, SoundPlayerError};
use forge_storage::SoundboardClipsRepo;
use forge_types::{ClipId, OutputDevice};

use crate::error::SoundboardError;
use crate::sink_factory::AudioSinkFactory;

pub struct SoundboardPlayer {
    sink_factory: Arc<dyn AudioSinkFactory>,
    event_sink: Arc<dyn AudioEventSink>,
    clips_repo: Arc<dyn SoundboardClipsRepo>,
}

impl SoundboardPlayer {
    pub fn new(
        sink_factory: Arc<dyn AudioSinkFactory>,
        event_sink: Arc<dyn AudioEventSink>,
        clips_repo: Arc<dyn SoundboardClipsRepo>,
    ) -> Self {
        Self {
            sink_factory,
            event_sink,
            clips_repo,
        }
    }

    pub async fn play(
        &self,
        clip_id: ClipId,
        override_device: Option<OutputDevice>,
    ) -> Result<(), SoundboardError> {
        let clip = self
            .clips_repo
            .get(clip_id)
            .await
            .map_err(|e| SoundboardError::Storage(e.to_string()))?
            .ok_or_else(|| SoundboardError::ClipNotFound(clip_id.to_string()))?;

        let device = override_device.unwrap_or_else(|| clip.output_device.clone());
        let device_label = device_label(&device);

        let sink = match self.sink_factory.build(&device).await {
            Ok(s) => s,
            Err(e) => {
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    error: e.to_string(),
                });
                return Err(SoundboardError::Audio(e));
            }
        };

        let path = clip.file_path.clone();
        let buffer = match tokio::task::spawn_blocking(move || forge_audio::decode_file(&path))
            .await
            .map_err(|e| SoundboardError::JoinError(e.to_string()))
            .and_then(|r| r.map_err(SoundboardError::Audio))
        {
            Ok(b) => b,
            Err(e) => {
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    error: e.to_string(),
                });
                return Err(e);
            }
        };

        let volume = clip.volume;
        let scaled: Vec<i16> = buffer
            .samples
            .iter()
            .map(|&s| {
                let v = s as f32 * volume;
                v.clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
            .collect();
        let buffer = PcmBuffer::new(scaled, buffer.sample_rate, buffer.channels);

        self.event_sink.emit(AudioEvent::PlaybackStarted {
            clip_id: Some(clip_id),
            device: device_label,
        });

        match sink.play(buffer).await {
            Ok(()) => {
                self.event_sink.emit(AudioEvent::PlaybackFinished {
                    clip_id: Some(clip_id),
                });
                Ok(())
            }
            Err(e) => {
                let error = e.to_string();
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    error,
                });
                Err(SoundboardError::Audio(e))
            }
        }
    }
}

#[async_trait]
impl SoundPlayer for SoundboardPlayer {
    async fn play(
        &self,
        clip_id: ClipId,
        output_device_override: Option<OutputDevice>,
    ) -> Result<(), SoundPlayerError> {
        SoundboardPlayer::play(self, clip_id, output_device_override)
            .await
            .map_err(|e| SoundPlayerError::Play(e.to_string()))
    }
}

fn device_label(device: &OutputDevice) -> String {
    match device {
        OutputDevice::Default => "default".to_string(),
        OutputDevice::ByName { name } => name.clone(),
        OutputDevice::ById { id } => id.clone(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use time::OffsetDateTime;

    use async_trait::async_trait;
    use forge_audio::{AudioError, AudioEvent, AudioEventSink, AudioSink, PcmBuffer};
    use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
    use forge_types::{ClipId, OutputDevice};

    use super::*;
    use crate::sink_factory::AudioSinkFactory;

    struct CountingSink {
        count: Arc<Mutex<usize>>,
        last_buf: Arc<Mutex<Option<PcmBuffer>>>,
    }

    #[async_trait]
    impl AudioSink for CountingSink {
        async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
            *self.count.lock().unwrap() += 1;
            *self.last_buf.lock().unwrap() = Some(buffer);
            Ok(())
        }
    }

    type SharedCount = Arc<Mutex<usize>>;
    type SharedBuf = Arc<Mutex<Option<PcmBuffer>>>;

    struct CountingFactory {
        count: SharedCount,
        last_buf: SharedBuf,
    }

    impl CountingFactory {
        fn new() -> (Self, SharedCount, SharedBuf) {
            let count = Arc::new(Mutex::new(0usize));
            let last_buf = Arc::new(Mutex::new(None));
            (
                Self {
                    count: Arc::clone(&count),
                    last_buf: Arc::clone(&last_buf),
                },
                count,
                last_buf,
            )
        }
    }

    #[async_trait]
    impl AudioSinkFactory for CountingFactory {
        async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            Ok(Arc::new(CountingSink {
                count: Arc::clone(&self.count),
                last_buf: Arc::clone(&self.last_buf),
            }))
        }
    }

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

    fn make_wav_tempfile(
        sample_rate: u32,
        channels: u16,
        n_frames: usize,
    ) -> tempfile::NamedTempFile {
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
        }
    }

    #[tokio::test]
    async fn play_emits_started_and_finished_and_calls_sink() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 2205);
        let clip = make_stored_clip(clip_id, tmp.path().to_path_buf());

        let (factory, play_count, _last_buf) = CountingFactory::new();
        let (event_sink, events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: Some(clip) };

        let player = SoundboardPlayer::new(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        player.play(clip_id, None).await.unwrap();

        assert_eq!(*play_count.lock().unwrap(), 1, "sink must be called once");

        let recorded = events.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert!(matches!(
            recorded[0],
            AudioEvent::PlaybackStarted {
                clip_id: Some(_),
                ..
            }
        ));
        assert!(matches!(
            recorded[1],
            AudioEvent::PlaybackFinished { clip_id: Some(_) }
        ));
    }

    #[tokio::test]
    async fn play_applies_volume_to_samples() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let mut clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        clip.volume = 0.5;

        let (factory, _count, last_buf) = CountingFactory::new();
        let (event_sink, _events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: Some(clip) };

        let player = SoundboardPlayer::new(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        player.play(clip_id, None).await.unwrap();

        let buf = last_buf.lock().unwrap();
        let buf = buf.as_ref().unwrap();
        for &s in &buf.samples {
            assert!(
                s.abs() <= i16::MAX / 2 + 1,
                "volume 0.5 must halve sample magnitude"
            );
        }
    }

    #[tokio::test]
    async fn play_returns_clip_not_found_when_repo_returns_none() {
        let clip_id = ClipId::new();
        let (factory, _count, _buf) = CountingFactory::new();
        let (event_sink, events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: None };

        let player = SoundboardPlayer::new(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        let err = player.play(clip_id, None).await.unwrap_err();
        assert!(
            matches!(err, SoundboardError::ClipNotFound(_)),
            "expected ClipNotFound, got {:?}",
            err
        );
        assert!(
            events.lock().unwrap().is_empty(),
            "no events must be emitted on ClipNotFound"
        );
    }

    #[tokio::test]
    async fn play_overrides_device_when_specified() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let mut clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        clip.output_device = OutputDevice::Default;

        let received_device: Arc<Mutex<Option<OutputDevice>>> = Arc::new(Mutex::new(None));
        let received_device_clone = Arc::clone(&received_device);

        struct DeviceCapturingFactory {
            captured: Arc<Mutex<Option<OutputDevice>>>,
        }

        #[async_trait]
        impl AudioSinkFactory for DeviceCapturingFactory {
            async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
                *self.captured.lock().unwrap() = Some(device.clone());
                Ok(Arc::new(forge_audio::NullSink))
            }
        }

        let (event_sink, _events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: Some(clip) };
        let player = SoundboardPlayer::new(
            Arc::new(DeviceCapturingFactory {
                captured: received_device_clone,
            }),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        let override_dev = OutputDevice::ByName {
            name: "Speakers".to_string(),
        };
        player
            .play(clip_id, Some(override_dev.clone()))
            .await
            .unwrap();

        let captured = received_device.lock().unwrap();
        assert_eq!(*captured, Some(override_dev));
    }
}
