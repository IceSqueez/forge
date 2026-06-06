use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::convert;
use crate::device::DeviceId;
use crate::error::AudioError;
use crate::events::{AudioEvent, AudioEventSink};
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
}

#[async_trait]
impl AudioSink for CpalSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        let device_id_str = self.device_id.0.clone();
        let event_sink = Arc::clone(&self.event_sink);
        let target_sr = self.target_sample_rate;
        let target_ch = self.target_channels;

        tokio::task::spawn_blocking(move || {
            run_playback(device_id_str, buffer, target_sr, target_ch, event_sink);
        });

        Ok(())
    }
}

fn run_playback(
    device_id_str: String,
    buffer: PcmBuffer,
    target_sr: Option<u32>,
    target_ch: Option<u16>,
    event_sink: Arc<dyn AudioEventSink>,
) {
    let host = cpal::default_host();

    let device = match find_device(&host, &device_id_str) {
        Some(d) => d,
        None => {
            event_sink.emit(AudioEvent::PlaybackFailed {
                clip_id: None,
                error: format!("device '{}' not found", device_id_str),
            });
            return;
        }
    };

    let config = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            event_sink.emit(AudioEvent::PlaybackFailed {
                clip_id: None,
                error: e.to_string(),
            });
            return;
        }
    };

    let device_sr = config.sample_rate();
    let device_ch = config.channels();
    let sample_format = config.sample_format();

    let dst_sr = target_sr.unwrap_or(device_sr);
    let dst_ch = target_ch.unwrap_or(device_ch);

    let converted = match prepare_samples(&buffer, dst_sr, dst_ch) {
        Ok(s) => s,
        Err(e) => {
            event_sink.emit(AudioEvent::PlaybackFailed {
                clip_id: None,
                error: e.to_string(),
            });
            return;
        }
    };

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

    let rx_f32 = rx.clone();
    let rx_i16 = rx.clone();
    let rx_i32 = rx.clone();

    let stream = match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for s in data.iter_mut() {
                    *s = rx_f32.try_recv().map(|v| v as f32 / 32767.0).unwrap_or(0.0);
                }
            },
            stream_error_fn,
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            stream_config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                for s in data.iter_mut() {
                    *s = rx_i16.try_recv().unwrap_or(0);
                }
            },
            stream_error_fn,
            None,
        ),
        SampleFormat::I32 => device.build_output_stream(
            stream_config,
            move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                for s in data.iter_mut() {
                    *s = rx_i32.try_recv().map(|v| v as i32).unwrap_or(0);
                }
            },
            stream_error_fn,
            None,
        ),
        other => {
            event_sink.emit(AudioEvent::PlaybackFailed {
                clip_id: None,
                error: format!("unsupported sample format {:?}", other),
            });
            return;
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            event_sink.emit(AudioEvent::PlaybackFailed {
                clip_id: None,
                error: e.to_string(),
            });
            return;
        }
    };

    if let Err(e) = stream.play() {
        event_sink.emit(AudioEvent::PlaybackFailed {
            clip_id: None,
            error: e.to_string(),
        });
        return;
    }

    event_sink.emit(AudioEvent::PlaybackStarted {
        clip_id: None,
        device: device_name,
    });

    std::thread::sleep(Duration::from_millis(duration_ms + 50));
    drop(stream);

    event_sink.emit(AudioEvent::PlaybackFinished { clip_id: None });
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
