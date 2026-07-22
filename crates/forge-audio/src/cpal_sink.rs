use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::convert;
use crate::device::DeviceId;
use crate::error::AudioError;
use crate::events::{AudioEvent, AudioEventSink};
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::sink::AudioSink;

pub struct CpalSink {
    device_id: DeviceId,
    target_sample_rate: Option<u32>,
    target_channels: Option<u16>,
    event_sink: Arc<dyn AudioEventSink>,
}

impl CpalSink {
    pub fn new(
        device_id: DeviceId,
        target_sample_rate: Option<u32>,
        target_channels: Option<u16>,
        event_sink: Arc<dyn AudioEventSink>,
    ) -> Self {
        Self {
            device_id,
            target_sample_rate,
            target_channels,
            event_sink,
        }
    }

    fn spawn_playback(
        &self,
        buffer: PcmBuffer,
        stop: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> tokio::task::JoinHandle<()> {
        let device_id_str = self.device_id.0.clone();
        let event_sink = Arc::clone(&self.event_sink);
        let target_sr = self.target_sample_rate;
        let target_ch = self.target_channels;

        tokio::task::spawn_blocking(move || {
            run_playback(
                device_id_str,
                buffer,
                target_sr,
                target_ch,
                event_sink,
                stop,
                paused,
            );
        })
    }
}

#[async_trait]
impl AudioSink for CpalSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        self.spawn_playback(buffer, stop, paused)
            .await
            .map_err(|e| AudioError::JoinFailed(e.to_string()))
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        self.spawn_playback(buffer, Arc::clone(&stop), Arc::clone(&paused));
        Ok(PlaybackHandle::from_flags(stop, paused))
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let join = self.spawn_playback(buffer, Arc::clone(&stop), Arc::clone(&paused));
        Ok(ControlledPlayback::from_handle(
            PlaybackHandle::from_flags(stop, paused),
            join,
        ))
    }
}

struct StartedStream {
    stream: cpal::Stream,
    duration_ms: u64,
    device_name: String,
}

fn run_playback(
    device_id_str: String,
    buffer: PcmBuffer,
    target_sr: Option<u32>,
    target_ch: Option<u16>,
    event_sink: Arc<dyn AudioEventSink>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
) {
    let host = cpal::default_host();
    let candidates = candidate_device_ids(&device_id_str);

    let mut last_error = format!("device '{}' not found", device_id_str);
    for (idx, candidate) in candidates.iter().enumerate() {
        match try_start_stream(
            &host, candidate, &buffer, target_sr, target_ch, &stop, &paused,
        ) {
            Ok(started) => {
                if idx > 0 {
                    tracing::warn!(
                        requested = %device_id_str,
                        using = %candidate,
                        "output device open failed; fell back through canonical chain"
                    );
                }
                event_sink.emit(AudioEvent::PlaybackStarted {
                    clip_id: None,
                    device: started.device_name,
                    duration_secs: Some(started.duration_ms as f64 / 1000.0),
                    looped: false,
                });
                wait_for_completion(started.duration_ms, &stop, &paused);
                drop(started.stream);
                event_sink.emit(AudioEvent::PlaybackFinished { clip_id: None });
                return;
            }
            Err(e) => {
                last_error = e.to_string();
            }
        }
    }

    event_sink.emit(AudioEvent::PlaybackFailed {
        clip_id: None,
        error: last_error,
    });
}

fn candidate_device_ids(requested: &str) -> Vec<String> {
    let mut ids = vec![requested.to_string()];
    for name in crate::device::CANONICAL_OUTPUT_CHAIN {
        if *name != requested {
            ids.push((*name).to_string());
        }
    }
    ids
}

fn try_start_stream(
    host: &cpal::Host,
    device_id_str: &str,
    buffer: &PcmBuffer,
    target_sr: Option<u32>,
    target_ch: Option<u16>,
    stop: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
) -> Result<StartedStream, AudioError> {
    let device = find_device(host, device_id_str)
        .ok_or_else(|| AudioError::Host(format!("device '{}' not found", device_id_str)))?;

    let config = device
        .default_output_config()
        .map_err(|e| AudioError::Host(e.to_string()))?;

    let device_sr = config.sample_rate();
    let device_ch = config.channels();
    let sample_format = config.sample_format();

    let dst_sr = target_sr.unwrap_or(device_sr);
    let dst_ch = target_ch.unwrap_or(device_ch);

    let converted = prepare_samples(buffer, dst_sr, dst_ch)?;

    let duration_ms = if dst_sr > 0 {
        (converted.len() as u64 / u64::from(dst_ch)) * 1000 / u64::from(dst_sr)
    } else {
        0
    };

    let (tx, rx) = crossbeam_channel::bounded::<i16>(converted.len().max(1));
    for s in converted {
        let _ = tx.send(s);
    }
    drop(tx);

    let stream_config = cpal::StreamConfig {
        channels: dst_ch,
        sample_rate: dst_sr,
        buffer_size: cpal::BufferSize::Default,
    };

    let device_name = device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_default();

    let stream = build_output_stream(
        &device,
        stream_config,
        sample_format,
        rx,
        Arc::clone(stop),
        Arc::clone(paused),
    )?;

    stream.play().map_err(|e| AudioError::Host(e.to_string()))?;

    Ok(StartedStream {
        stream,
        duration_ms,
        device_name,
    })
}

fn build_output_stream(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    sample_format: SampleFormat,
    rx: crossbeam_channel::Receiver<i16>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
) -> Result<cpal::Stream, AudioError> {
    let stream = match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                    data.fill(0.0);
                    return;
                }
                for s in data.iter_mut() {
                    *s = rx.try_recv().map(|v| v as f32 / 32767.0).unwrap_or(0.0);
                }
            },
            stream_error_fn,
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            stream_config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                    data.fill(0);
                    return;
                }
                for s in data.iter_mut() {
                    *s = rx.try_recv().unwrap_or(0);
                }
            },
            stream_error_fn,
            None,
        ),
        SampleFormat::I32 => device.build_output_stream(
            stream_config,
            move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                    data.fill(0);
                    return;
                }
                for s in data.iter_mut() {
                    *s = rx.try_recv().map(|v| v as i32).unwrap_or(0);
                }
            },
            stream_error_fn,
            None,
        ),
        other => {
            return Err(AudioError::Host(format!(
                "unsupported sample format {:?}",
                other
            )));
        }
    };

    stream.map_err(|e| AudioError::Host(e.to_string()))
}

fn wait_for_completion(duration_ms: u64, stop: &Arc<AtomicBool>, paused: &Arc<AtomicBool>) {
    let total_ms = duration_ms + 50;
    let mut elapsed_ms = 0u64;
    while elapsed_ms < total_ms || paused.load(Ordering::Relaxed) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let step = if elapsed_ms < total_ms {
            (total_ms - elapsed_ms).min(20)
        } else {
            20
        };
        std::thread::sleep(Duration::from_millis(step));
        if !paused.load(Ordering::Relaxed) {
            elapsed_ms += step;
        }
    }
}

fn find_device(host: &cpal::Host, id_str: &str) -> Option<cpal::Device> {
    host.output_devices()
        .ok()?
        .find(|d| d.id().ok().map(|id| id.id() == id_str).unwrap_or(false))
}

fn prepare_samples(buffer: &PcmBuffer, dst_sr: u32, dst_ch: u16) -> Result<Vec<i16>, AudioError> {
    let resampled =
        convert::resample(&buffer.samples, buffer.sample_rate, dst_sr, buffer.channels)?;
    Ok(convert::remix(&resampled, buffer.channels, dst_ch))
}

fn stream_error_fn(err: cpal::Error) {
    tracing::error!("cpal stream error: {}", err);
}
