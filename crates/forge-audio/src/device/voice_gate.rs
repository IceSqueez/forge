use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread::Thread;
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::device::catalog::DeviceId;
use crate::device::gate_machine::{GateEdge, GateMachine, GateSettings};
use crate::device::samples::peak;
use crate::error::AudioError;

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceGateConfig {
    pub device: Option<DeviceId>,
    pub threshold: f32,
    pub hold: Duration,
}

#[derive(Debug, Clone)]
pub struct VoiceGateWatch(watch::Receiver<VoiceGateState>);

impl VoiceGateWatch {
    pub fn latest(&mut self) -> VoiceGateState {
        self.0.borrow_and_update().clone()
    }

    pub async fn changed(&mut self) -> Option<VoiceGateState> {
        self.0.changed().await.ok()?;
        Some(self.latest())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceGateState {
    Inactive,
    Active,
    Unavailable(String),
}

enum GateControl {
    SetDevice(Option<DeviceId>),
    StreamFailed(String),
    Stop,
}

const REOPEN_INTERVAL: Duration = Duration::from_secs(5);

struct GateShared {
    level_bits: AtomicU32,
    threshold_bits: AtomicU32,
    hold_nanos: AtomicU64,
    gate_open: AtomicBool,
    hiccups: AtomicU64,
}

impl GateShared {
    fn settings(&self) -> GateSettings {
        GateSettings {
            threshold: f32::from_bits(self.threshold_bits.load(Ordering::Relaxed)),
            hold_nanos: self.hold_nanos.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone)]
struct MonitorMailbox {
    controls: crossbeam_channel::Sender<GateControl>,
    monitor: Thread,
}

impl MonitorMailbox {
    fn post(&self, control: GateControl) -> bool {
        let delivered = self.controls.send(control).is_ok();
        self.monitor.unpark();
        delivered
    }
}

pub struct VoiceGateMonitor {
    mailbox: MonitorMailbox,
    state_rx: watch::Receiver<VoiceGateState>,
    shared: Arc<GateShared>,
}

impl VoiceGateMonitor {
    pub fn start(config: VoiceGateConfig) -> Self {
        let shared = Arc::new(GateShared {
            level_bits: AtomicU32::new(0.0f32.to_bits()),
            threshold_bits: AtomicU32::new(config.threshold.to_bits()),
            hold_nanos: AtomicU64::new(u64::try_from(config.hold.as_nanos()).unwrap_or(u64::MAX)),
            gate_open: AtomicBool::new(false),
            hiccups: AtomicU64::new(0),
        });
        let (control_tx, control_rx) = crossbeam_channel::unbounded();
        let (state_tx, state_rx) = watch::channel(VoiceGateState::Inactive);

        let publisher = GatePublisher::new(state_tx, Arc::clone(&shared));
        let failure_tx = control_tx.clone();
        let monitor = std::thread::spawn(move || {
            MonitorLoop {
                controls: control_rx,
                mailbox: MonitorMailbox {
                    controls: failure_tx,
                    monitor: std::thread::current(),
                },
                publisher,
            }
            .run(config.device);
        });

        Self {
            mailbox: MonitorMailbox {
                controls: control_tx,
                monitor: monitor.thread().clone(),
            },
            state_rx,
            shared,
        }
    }

    pub fn subscribe(&self) -> VoiceGateWatch {
        VoiceGateWatch(self.state_rx.clone())
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
            u64::try_from(config.hold.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        if !self.mailbox.post(GateControl::SetDevice(config.device)) {
            tracing::debug!("voice gate monitor stopped; device change dropped");
        }
    }

    pub fn stop(&self) {
        self.mailbox.post(GateControl::Stop);
    }
}

impl Drop for VoiceGateMonitor {
    fn drop(&mut self) {
        self.mailbox.post(GateControl::Stop);
    }
}

struct GatePublisher {
    state_tx: watch::Sender<VoiceGateState>,
    shared: Arc<GateShared>,
    published_open: bool,
}

impl GatePublisher {
    fn new(state_tx: watch::Sender<VoiceGateState>, shared: Arc<GateShared>) -> Self {
        Self {
            state_tx,
            shared,
            published_open: false,
        }
    }

    fn restart(&mut self) {
        self.published_open = false;
        self.state_tx.send_replace(VoiceGateState::Inactive);
    }

    fn forward_gate(&mut self) {
        let open = self.shared.gate_open.load(Ordering::Acquire);
        if open == self.published_open {
            return;
        }
        self.published_open = open;
        self.state_tx.send_replace(if open {
            VoiceGateState::Active
        } else {
            VoiceGateState::Inactive
        });
    }

    fn unavailable(&mut self, reason: String) {
        self.shared.gate_open.store(false, Ordering::Release);
        self.published_open = false;
        self.shared
            .level_bits
            .store(0.0f32.to_bits(), Ordering::Relaxed);
        self.state_tx
            .send_replace(VoiceGateState::Unavailable(reason));
    }

    fn report_hiccups(&self) {
        let hiccups = self.shared.hiccups.swap(0, Ordering::Relaxed);
        if hiccups > 0 {
            tracing::debug!(hiccups, "cpal input stream hiccups");
        }
    }
}

struct MonitorLoop {
    controls: crossbeam_channel::Receiver<GateControl>,
    mailbox: MonitorMailbox,
    publisher: GatePublisher,
}

impl MonitorLoop {
    fn run(mut self, initial: Option<DeviceId>) {
        let mut device = initial;
        loop {
            match open_and_start(device.clone(), &self.publisher.shared, self.mailbox.clone()) {
                Ok(_stream) => {
                    self.publisher.restart();
                    let keep_running = self.follow_stream(&mut device);
                    self.publisher.report_hiccups();
                    if !keep_running {
                        return;
                    }
                }
                Err(err) => {
                    self.publisher.unavailable(err.to_string());
                    match self.next_control(Some(REOPEN_INTERVAL)) {
                        Some(GateControl::Stop) => return,
                        Some(GateControl::SetDevice(next)) => device = next,
                        Some(GateControl::StreamFailed(_)) | None => {}
                    }
                }
            }
        }
    }

    fn follow_stream(&mut self, device: &mut Option<DeviceId>) -> bool {
        loop {
            match self.next_control(None) {
                Some(GateControl::Stop) => return false,
                Some(GateControl::SetDevice(next)) => {
                    if next == *device {
                        continue;
                    }
                    *device = next;
                    return true;
                }
                Some(GateControl::StreamFailed(reason)) => {
                    tracing::error!("cpal input stream error: {}", reason);
                    self.publisher.unavailable(reason);
                    return true;
                }
                None => {}
            }
        }
    }

    fn next_control(&mut self, patience: Option<Duration>) -> Option<GateControl> {
        let deadline = patience.map(|patience| Instant::now() + patience);
        loop {
            self.publisher.forward_gate();
            match self.controls.try_recv() {
                Ok(control) => return Some(control),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return Some(GateControl::Stop);
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
            match deadline {
                None => std::thread::park(),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return None;
                    }
                    std::thread::park_timeout(left);
                }
            }
        }
    }
}

struct GateCallback {
    epoch: Instant,
    machine: GateMachine,
    shared: Arc<GateShared>,
    monitor: Thread,
}

impl GateCallback {
    fn on_block(&mut self, peak: f32) {
        self.shared
            .level_bits
            .store(peak.to_bits(), Ordering::Relaxed);
        let now_nanos = u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX);
        if let Some(edge) = self.machine.tick(peak, self.shared.settings(), now_nanos) {
            self.shared
                .gate_open
                .store(edge == GateEdge::Opened, Ordering::Release);
            self.monitor.unpark();
        }
    }
}

fn open_and_start(
    device_id: Option<DeviceId>,
    shared: &Arc<GateShared>,
    mailbox: MonitorMailbox,
) -> Result<cpal::Stream, AudioError> {
    use cpal::traits::{DeviceTrait, StreamTrait};

    let host = cpal::default_host();
    let device = find_input_device(&host, device_id.as_ref()).ok_or(AudioError::NoInputDevice)?;

    let config = device
        .default_input_config()
        .map_err(|e| AudioError::Host(e.to_string()))?;
    let stream_config: cpal::StreamConfig = config.into();
    let sample_format = config.sample_format();

    shared.gate_open.store(false, Ordering::Release);
    let ctx = CallbackContext {
        callback: GateCallback {
            epoch: Instant::now(),
            machine: GateMachine::default(),
            shared: Arc::clone(shared),
            monitor: mailbox.monitor.clone(),
        },
        mailbox,
    };

    let stream = build_input_stream(&device, stream_config, sample_format, ctx)?;

    stream.play().map_err(|e| AudioError::Host(e.to_string()))?;
    Ok(stream)
}

struct CallbackContext {
    callback: GateCallback,
    mailbox: MonitorMailbox,
}

fn build_input_stream(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    ctx: CallbackContext,
) -> Result<cpal::Stream, AudioError> {
    use cpal::SampleFormat;
    use cpal::{I24, U24};

    let stream = match sample_format {
        SampleFormat::I8 => typed_input_stream::<i8>(device, stream_config, ctx),
        SampleFormat::I16 => typed_input_stream::<i16>(device, stream_config, ctx),
        SampleFormat::I24 => typed_input_stream::<I24>(device, stream_config, ctx),
        SampleFormat::I32 => typed_input_stream::<i32>(device, stream_config, ctx),
        SampleFormat::I64 => typed_input_stream::<i64>(device, stream_config, ctx),
        SampleFormat::U8 => typed_input_stream::<u8>(device, stream_config, ctx),
        SampleFormat::U16 => typed_input_stream::<u16>(device, stream_config, ctx),
        SampleFormat::U24 => typed_input_stream::<U24>(device, stream_config, ctx),
        SampleFormat::U32 => typed_input_stream::<u32>(device, stream_config, ctx),
        SampleFormat::U64 => typed_input_stream::<u64>(device, stream_config, ctx),
        SampleFormat::F32 => typed_input_stream::<f32>(device, stream_config, ctx),
        SampleFormat::F64 => typed_input_stream::<f64>(device, stream_config, ctx),
        other => {
            return Err(AudioError::UnsupportedSampleFormat(format!("{other:?}")));
        }
    };

    stream.map_err(|e| AudioError::Host(e.to_string()))
}

fn typed_input_stream<T>(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    ctx: CallbackContext,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;

    let CallbackContext {
        mut callback,
        mailbox,
    } = ctx;
    let shared = Arc::clone(&callback.shared);
    device.build_input_stream(
        stream_config,
        move |data: &[T], _: &cpal::InputCallbackInfo| callback.on_block(peak(data)),
        move |err| input_stream_error_fn(err, &shared, &mailbox),
        None,
    )
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
            .or_else(|| {
                tracing::warn!(
                    device = id.as_str(),
                    "input device not found, falling back to default"
                );
                host.default_input_device()
            }),
    }
}

fn input_stream_error_fn(err: cpal::Error, shared: &GateShared, mailbox: &MonitorMailbox) {
    if !crate::device::cpal_sink::ends_playback(err.kind()) {
        shared.hiccups.fetch_add(1, Ordering::Relaxed);
        return;
    }
    mailbox.post(GateControl::StreamFailed(err.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLD: f32 = 0.5;
    const NEVER_RELEASES: u64 = u64::MAX;
    const RELEASES_AT_ONCE: u64 = 0;
    const LOUD: f32 = 1.0;
    const SILENT: f32 = 0.0;
    const STREAM_LOST: &str = "the microphone was unplugged";
    const REOPEN_REFUSED: &str = "no input device";
    const MIC: &str = "builtin-mic";
    const USB_MIC: &str = "usb-mic";
    const PATIENCE_SPENT: Option<Duration> = Some(Duration::ZERO);
    const HANG_GUARD: Duration = Duration::from_secs(60);
    const WAKE_ROUNDS: usize = 500;

    fn shared_holding_for(hold_nanos: u64) -> Arc<GateShared> {
        Arc::new(GateShared {
            level_bits: AtomicU32::new(0.0f32.to_bits()),
            threshold_bits: AtomicU32::new(THRESHOLD.to_bits()),
            hold_nanos: AtomicU64::new(hold_nanos),
            gate_open: AtomicBool::new(false),
            hiccups: AtomicU64::new(0),
        })
    }

    struct Gate {
        callback: GateCallback,
        publisher: GatePublisher,
        state_rx: watch::Receiver<VoiceGateState>,
    }

    impl Gate {
        fn holding_for(hold_nanos: u64) -> Self {
            let (state_tx, state_rx) = watch::channel(VoiceGateState::Inactive);
            let shared = shared_holding_for(hold_nanos);
            Self {
                callback: GateCallback {
                    epoch: Instant::now(),
                    machine: GateMachine::default(),
                    shared: Arc::clone(&shared),
                    monitor: std::thread::current(),
                },
                publisher: GatePublisher::new(state_tx, shared),
                state_rx,
            }
        }

        fn tick(&mut self, peak: f32) {
            self.callback.on_block(peak);
            self.publisher.forward_gate();
        }

        fn state_published(&mut self) -> Option<VoiceGateState> {
            self.state_rx
                .has_changed()
                .unwrap()
                .then(|| self.state_rx.borrow_and_update().clone())
        }
    }

    #[test]
    fn a_peak_at_the_threshold_opens_the_gate() {
        let mut gate = Gate::holding_for(NEVER_RELEASES);

        gate.tick(THRESHOLD);

        assert_eq!(gate.state_published(), Some(VoiceGateState::Active));
    }

    #[test]
    fn a_peak_just_under_the_threshold_leaves_the_gate_closed() {
        let mut gate = Gate::holding_for(RELEASES_AT_ONCE);

        gate.tick(f32::from_bits(THRESHOLD.to_bits() - 1));

        assert_eq!(gate.state_published(), None);
    }

    #[test]
    fn a_held_open_gate_announces_active_only_once() {
        let mut gate = Gate::holding_for(NEVER_RELEASES);
        gate.tick(THRESHOLD);
        gate.state_published();

        gate.tick(1.0);

        assert_eq!(gate.state_published(), None);
    }

    #[test]
    fn silence_inside_the_hold_keeps_the_gate_open() {
        let mut gate = Gate::holding_for(NEVER_RELEASES);
        gate.tick(THRESHOLD);
        gate.state_published();

        gate.tick(0.0);

        assert_eq!(gate.state_published(), None);
    }

    #[test]
    fn silence_past_the_hold_closes_the_gate_and_announces_it_once() {
        let mut gate = Gate::holding_for(RELEASES_AT_ONCE);
        gate.tick(THRESHOLD);
        gate.state_published();

        gate.tick(0.0);
        let closed = gate.state_published();
        gate.tick(0.0);

        assert_eq!(
            (closed, gate.state_published()),
            (Some(VoiceGateState::Inactive), None)
        );
    }

    #[test]
    fn every_tick_reports_its_peak_as_the_level() {
        let mut gate = Gate::holding_for(NEVER_RELEASES);

        gate.tick(0.25);

        assert_eq!(
            f32::from_bits(gate.callback.shared.level_bits.load(Ordering::Relaxed)),
            0.25
        );
    }

    struct Parked {
        monitor: MonitorLoop,
        shared: Arc<GateShared>,
        state_rx: watch::Receiver<VoiceGateState>,
    }

    impl Parked {
        fn new() -> Self {
            let (control_tx, controls) = crossbeam_channel::unbounded();
            let (state_tx, state_rx) = watch::channel(VoiceGateState::Inactive);
            let shared = shared_holding_for(NEVER_RELEASES);
            Self {
                monitor: MonitorLoop {
                    controls,
                    mailbox: MonitorMailbox {
                        controls: control_tx,
                        monitor: std::thread::current(),
                    },
                    publisher: GatePublisher::new(state_tx, Arc::clone(&shared)),
                },
                shared,
                state_rx,
            }
        }

        fn set_gate(&self, open: bool) {
            self.shared.gate_open.store(open, Ordering::Release);
        }

        fn publish_gate(&mut self, open: bool) {
            self.set_gate(open);
            self.monitor.publisher.forward_gate();
        }

        fn state(&self) -> VoiceGateState {
            self.state_rx.borrow().clone()
        }
    }

    #[test]
    fn a_failed_reopen_stays_unavailable_through_the_wait_for_the_next_attempt() {
        for (case, gate_was_open, open_was_published) in [
            ("the gate was closed", false, false),
            ("the gate was open and published", true, true),
            ("the gate opened in the last block", true, false),
        ] {
            let mut parked = Parked::new();
            parked.set_gate(gate_was_open);
            if open_was_published {
                parked.monitor.publisher.forward_gate();
            }
            let level = &parked.monitor.publisher.shared.level_bits;
            level.store(0.5f32.to_bits(), Ordering::Relaxed);

            parked
                .monitor
                .publisher
                .unavailable(REOPEN_REFUSED.to_owned());
            let control = parked.monitor.next_control(PATIENCE_SPENT);

            assert!(control.is_none(), "{case}: nothing was posted");
            let level = &parked.monitor.publisher.shared.level_bits;
            assert_eq!(
                f32::from_bits(level.load(Ordering::Relaxed)),
                0.0,
                "{case}: the meter froze on the last peak"
            );
            assert_eq!(
                parked.state(),
                VoiceGateState::Unavailable(REOPEN_REFUSED.to_owned()),
                "{case}"
            );
        }
    }

    #[test]
    fn a_reopened_stream_starts_inactive_and_publishes_the_next_opening() {
        let lost = VoiceGateState::Unavailable(STREAM_LOST.to_owned());
        for (case, previous, new_stream_already_open) in [
            ("after an open gate", VoiceGateState::Active, false),
            ("after a lost stream", lost.clone(), false),
            (
                "after an open gate, opening again at once",
                VoiceGateState::Active,
                true,
            ),
            ("after a lost stream, opening at once", lost, true),
        ] {
            let mut parked = Parked::new();
            parked.publish_gate(true);
            if previous != VoiceGateState::Active {
                parked.monitor.publisher.unavailable(STREAM_LOST.to_owned());
            }
            assert_eq!(parked.state(), previous, "{case}: wrong setup");
            parked.set_gate(new_stream_already_open);

            parked.monitor.publisher.restart();
            assert_eq!(parked.state(), VoiceGateState::Inactive, "{case}: reopened");

            parked.publish_gate(true);
            assert_eq!(
                parked.state(),
                VoiceGateState::Active,
                "{case}: next opening"
            );
        }
    }

    #[test]
    fn following_a_stream_ends_on_stop_on_a_new_device_and_on_a_lost_stream() {
        let mic = Some(DeviceId::new(MIC));
        let usb_mic = Some(DeviceId::new(USB_MIC));
        let lost = VoiceGateState::Unavailable(STREAM_LOST.to_owned());
        for (case, controls, reopens, device_after, state_after) in [
            (
                "stop ends the monitor",
                vec![GateControl::Stop],
                false,
                mic.clone(),
                VoiceGateState::Inactive,
            ),
            (
                "the device already open is no change",
                vec![GateControl::SetDevice(mic.clone()), GateControl::Stop],
                false,
                mic.clone(),
                VoiceGateState::Inactive,
            ),
            (
                "another device reopens",
                vec![GateControl::SetDevice(usb_mic.clone())],
                true,
                usb_mic,
                VoiceGateState::Inactive,
            ),
            (
                "the system default is a device of its own",
                vec![GateControl::SetDevice(None)],
                true,
                None,
                VoiceGateState::Inactive,
            ),
            (
                "a lost stream says why and reopens",
                vec![GateControl::StreamFailed(STREAM_LOST.to_owned())],
                true,
                mic.clone(),
                lost,
            ),
        ] {
            let mut parked = Parked::new();
            for control in controls {
                assert!(parked.monitor.mailbox.post(control), "{case}");
            }
            let mut device = mic.clone();

            let keeps_running = parked.monitor.follow_stream(&mut device);

            assert_eq!(keeps_running, reopens, "{case}");
            assert_eq!(device, device_after, "{case}");
            assert_eq!(parked.state(), state_after, "{case}");
        }
    }

    #[test]
    fn a_queued_control_is_taken_even_when_the_patience_is_already_spent() {
        let mut parked = Parked::new();
        assert!(parked.monitor.mailbox.post(GateControl::Stop));

        let control = parked.monitor.next_control(PATIENCE_SPENT);

        assert!(matches!(control, Some(GateControl::Stop)));
    }

    #[test]
    fn an_input_stream_error_ends_the_stream_only_when_the_device_can_no_longer_record() {
        for (kind, ends) in [
            (cpal::ErrorKind::Xrun, false),
            (cpal::ErrorKind::DeviceChanged, false),
            (cpal::ErrorKind::RealtimeDenied, false),
            (cpal::ErrorKind::DeviceNotAvailable, true),
            (cpal::ErrorKind::StreamInvalidated, true),
        ] {
            let parked = Parked::new();

            input_stream_error_fn(
                cpal::Error::new(kind),
                &parked.shared,
                &parked.monitor.mailbox,
            );

            assert_eq!(
                matches!(
                    parked.monitor.controls.try_recv(),
                    Ok(GateControl::StreamFailed(_))
                ),
                ends,
                "wrong verdict for {kind:?}"
            );
            assert_eq!(
                parked.shared.hiccups.load(Ordering::Relaxed),
                u64::from(!ends),
                "wrong hiccup count for {kind:?}"
            );
        }
    }

    struct LiveMonitor {
        mailbox: MonitorMailbox,
        callback: GateCallback,
        state_rx: watch::Receiver<VoiceGateState>,
        reopened: tokio::sync::mpsc::UnboundedReceiver<Option<DeviceId>>,
        thread: std::thread::JoinHandle<()>,
    }

    impl LiveMonitor {
        fn spawn() -> Self {
            let (control_tx, controls) = crossbeam_channel::unbounded();
            let (state_tx, state_rx) = watch::channel(VoiceGateState::Inactive);
            let (reopened_tx, reopened) = tokio::sync::mpsc::unbounded_channel();
            let shared = shared_holding_for(RELEASES_AT_ONCE);
            let publisher = GatePublisher::new(state_tx, Arc::clone(&shared));
            let own_mailbox = control_tx.clone();
            let thread = std::thread::spawn(move || {
                let mut monitor = MonitorLoop {
                    controls,
                    mailbox: MonitorMailbox {
                        controls: own_mailbox,
                        monitor: std::thread::current(),
                    },
                    publisher,
                };
                let mut device = None;
                while monitor.follow_stream(&mut device) {
                    reopened_tx.send(device.clone()).unwrap();
                }
            });
            let monitor = thread.thread().clone();
            Self {
                mailbox: MonitorMailbox {
                    controls: control_tx,
                    monitor: monitor.clone(),
                },
                callback: GateCallback {
                    epoch: Instant::now(),
                    machine: GateMachine::default(),
                    shared,
                    monitor,
                },
                state_rx,
                reopened,
                thread,
            }
        }

        async fn sees(&mut self, wanted: VoiceGateState) {
            let seen =
                tokio::time::timeout(HANG_GUARD, self.state_rx.wait_for(|state| *state == wanted))
                    .await;
            assert!(
                matches!(seen, Ok(Ok(_))),
                "the monitor slept through a wake and never published {wanted:?}"
            );
        }

        async fn reopens_on(&mut self, device: Option<DeviceId>) {
            let reopened = tokio::time::timeout(HANG_GUARD, self.reopened.recv()).await;
            assert_eq!(
                reopened.ok().flatten(),
                Some(device),
                "the monitor slept through a posted control"
            );
        }

        fn stop(self) {
            assert!(self.mailbox.post(GateControl::Stop));
            self.thread.join().unwrap();
        }
    }

    #[tokio::test]
    async fn a_gate_edge_landing_right_after_a_publish_wakes_the_monitor() {
        let mut live = LiveMonitor::spawn();

        for _ in 0..WAKE_ROUNDS {
            live.callback.on_block(LOUD);
            live.sees(VoiceGateState::Active).await;
            live.callback.on_block(SILENT);
            live.sees(VoiceGateState::Inactive).await;
        }

        live.stop();
    }

    #[tokio::test]
    async fn a_control_posted_right_after_a_publish_wakes_the_monitor() {
        let mut live = LiveMonitor::spawn();

        for round in 0..WAKE_ROUNDS {
            let device = Some(DeviceId::new(format!("mic-{round}")));
            live.callback.on_block(LOUD);
            live.sees(VoiceGateState::Active).await;
            assert!(live.mailbox.post(GateControl::SetDevice(device.clone())));
            live.reopens_on(device).await;
            live.callback.on_block(SILENT);
            live.sees(VoiceGateState::Inactive).await;
        }

        live.stop();
    }
}
