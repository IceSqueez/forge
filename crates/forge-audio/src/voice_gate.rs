use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::device::DeviceId;
use crate::error::AudioError;

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceGateConfig {
    pub device: Option<DeviceId>,
    pub threshold: f32,
    pub hold: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceGateState {
    Inactive,
    Active,
    Unavailable(String),
}

enum GateControl {
    SetDevice(Option<DeviceId>),
    Stop,
}

struct GateShared {
    level_bits: AtomicU32,
    threshold_bits: AtomicU32,
    hold_nanos: AtomicU64,
}

pub struct VoiceGateMonitor {
    control_tx: crossbeam_channel::Sender<GateControl>,
    state_rx: watch::Receiver<VoiceGateState>,
    shared: Arc<GateShared>,
}

impl VoiceGateMonitor {
    pub fn start(config: VoiceGateConfig) -> Self {
        let shared = Arc::new(GateShared {
            level_bits: AtomicU32::new(0.0f32.to_bits()),
            threshold_bits: AtomicU32::new(config.threshold.to_bits()),
            hold_nanos: AtomicU64::new(config.hold.as_nanos().min(u128::from(u64::MAX)) as u64),
        });
        let (control_tx, control_rx) = crossbeam_channel::unbounded();
        let (state_tx, state_rx) = watch::channel(VoiceGateState::Inactive);

        let thread_shared = Arc::clone(&shared);
        // cpal::Stream is !Send; it is built and dropped entirely inside this thread.
        std::thread::spawn(move || run(config, control_rx, state_tx, thread_shared));

        Self {
            control_tx,
            state_rx,
            shared,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<VoiceGateState> {
        self.state_rx.clone()
    }

    pub fn state(&self) -> VoiceGateState {
        self.state_rx.borrow().clone()
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level_bits.load(Ordering::Relaxed))
    }

    pub fn reconfigure(&self, config: VoiceGateConfig) {
        self.shared
            .threshold_bits
            .store(config.threshold.to_bits(), Ordering::Relaxed);
        self.shared.hold_nanos.store(
            config.hold.as_nanos().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        let _ = self.control_tx.send(GateControl::SetDevice(config.device));
    }

    pub fn stop(&self) {
        let _ = self.control_tx.send(GateControl::Stop);
    }
}

impl Drop for VoiceGateMonitor {
    fn drop(&mut self) {
        let _ = self.control_tx.send(GateControl::Stop);
    }
}

fn run(
    initial: VoiceGateConfig,
    control_rx: crossbeam_channel::Receiver<GateControl>,
    state_tx: watch::Sender<VoiceGateState>,
    shared: Arc<GateShared>,
) {
    let mut device = initial.device;
    loop {
        match open_and_start(device.clone(), &shared, state_tx.clone()) {
            Ok(_stream) => {
                let _ = state_tx.send(VoiceGateState::Inactive);
                loop {
                    match control_rx.recv() {
                        Ok(GateControl::Stop) | Err(_) => return,
                        Ok(GateControl::SetDevice(next)) => {
                            if next == device {
                                continue;
                            }
                            device = next;
                            break;
                        }
                    }
                }
            }
            Err(err) => {
                let _ = state_tx.send(VoiceGateState::Unavailable(err.to_string()));
                match control_rx.recv() {
                    Ok(GateControl::Stop) | Err(_) => return,
                    Ok(GateControl::SetDevice(next)) => device = next,
                }
            }
        }
    }
}

fn gate_tick(
    peak: f32,
    epoch: Instant,
    shared: &GateShared,
    is_active: &AtomicBool,
    last_active_nanos: &AtomicU64,
    state_tx: &watch::Sender<VoiceGateState>,
) {
    shared.level_bits.store(peak.to_bits(), Ordering::Relaxed);
    let threshold = f32::from_bits(shared.threshold_bits.load(Ordering::Relaxed));
    let now_nanos = epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;

    if peak >= threshold {
        last_active_nanos.store(now_nanos, Ordering::Relaxed);
        if !is_active.swap(true, Ordering::Relaxed) {
            let _ = state_tx.send(VoiceGateState::Active);
        }
        return;
    }

    let hold = shared.hold_nanos.load(Ordering::Relaxed);
    let last_active = last_active_nanos.load(Ordering::Relaxed);
    if is_active.load(Ordering::Relaxed) && now_nanos.saturating_sub(last_active) >= hold {
        is_active.store(false, Ordering::Relaxed);
        let _ = state_tx.send(VoiceGateState::Inactive);
    }
}

fn open_and_start(
    device_id: Option<DeviceId>,
    shared: &Arc<GateShared>,
    state_tx: watch::Sender<VoiceGateState>,
) -> Result<cpal::Stream, AudioError> {
    use cpal::traits::{DeviceTrait, StreamTrait};

    let host = cpal::default_host();
    let device = find_input_device(&host, device_id.as_ref())
        .ok_or_else(|| AudioError::Host("no matching input device found".to_string()))?;

    let config = device
        .default_input_config()
        .map_err(|e| AudioError::Host(e.to_string()))?;
    let stream_config: cpal::StreamConfig = config.into();
    let sample_format = config.sample_format();

    let ctx = CallbackContext {
        epoch: Instant::now(),
        shared: Arc::clone(shared),
        is_active: Arc::new(AtomicBool::new(false)),
        last_active_nanos: Arc::new(AtomicU64::new(0)),
        state_tx,
    };

    let stream = build_input_stream(&device, stream_config, sample_format, ctx)?;

    stream.play().map_err(|e| AudioError::Host(e.to_string()))?;
    Ok(stream)
}

struct CallbackContext {
    epoch: Instant,
    shared: Arc<GateShared>,
    is_active: Arc<AtomicBool>,
    last_active_nanos: Arc<AtomicU64>,
    state_tx: watch::Sender<VoiceGateState>,
}

fn build_input_stream(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    ctx: CallbackContext,
) -> Result<cpal::Stream, AudioError> {
    use cpal::SampleFormat;
    use cpal::traits::DeviceTrait;

    let CallbackContext {
        epoch,
        shared,
        is_active,
        last_active_nanos,
        state_tx,
    } = ctx;

    let stream = match sample_format {
        SampleFormat::F32 => {
            let error_tx = state_tx.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let peak = data.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
                    gate_tick(
                        peak,
                        epoch,
                        &shared,
                        &is_active,
                        &last_active_nanos,
                        &state_tx,
                    );
                },
                move |err| input_stream_error_fn(err, &error_tx),
                None,
            )
        }
        SampleFormat::I16 => {
            let error_tx = state_tx.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let peak = data
                        .iter()
                        .fold(0.0f32, |acc, s| acc.max((*s as f32 / 32767.0).abs()));
                    gate_tick(
                        peak,
                        epoch,
                        &shared,
                        &is_active,
                        &last_active_nanos,
                        &state_tx,
                    );
                },
                move |err| input_stream_error_fn(err, &error_tx),
                None,
            )
        }
        SampleFormat::I32 => {
            let error_tx = state_tx.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    let peak = data.iter().fold(0.0f32, |acc, s| {
                        acc.max((*s as f32 / i32::MAX as f32).abs())
                    });
                    gate_tick(
                        peak,
                        epoch,
                        &shared,
                        &is_active,
                        &last_active_nanos,
                        &state_tx,
                    );
                },
                move |err| input_stream_error_fn(err, &error_tx),
                None,
            )
        }
        other => {
            return Err(AudioError::Host(format!(
                "unsupported input sample format {:?}",
                other
            )));
        }
    };

    stream.map_err(|e| AudioError::Host(e.to_string()))
}

fn find_input_device(host: &cpal::Host, id: Option<&DeviceId>) -> Option<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};

    match id {
        None => host.default_input_device(),
        Some(id) => host
            .input_devices()
            .ok()?
            .find(|d| {
                d.id()
                    .ok()
                    .map(|found| found.id() == id.as_str())
                    .unwrap_or(false)
            })
            .or_else(|| host.default_input_device()),
    }
}

fn input_stream_error_fn(err: cpal::Error, state_tx: &watch::Sender<VoiceGateState>) {
    tracing::error!("cpal input stream error: {}", err);
    let _ = state_tx.send(VoiceGateState::Unavailable(err.to_string()));
}
