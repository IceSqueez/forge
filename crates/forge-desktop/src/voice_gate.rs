use std::sync::Mutex;
use std::time::Duration;

use forge_audio::{DeviceId, VoiceGateConfig, VoiceGateMonitor, VoiceGateState};
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::VoiceGateSettings;
use tokio::sync::watch;

pub fn config_from_settings(settings: &VoiceGateSettings) -> VoiceGateConfig {
    VoiceGateConfig {
        device: settings.input_device_id.clone().map(DeviceId::new),
        threshold: settings.threshold,
        hold: Duration::from_millis(u64::from(settings.hold_ms)),
    }
}

struct Running {
    monitor: VoiceGateMonitor,
    pump: tokio::task::JoinHandle<()>,
}

pub struct VoiceGateOwner {
    rt_handle: tokio::runtime::Handle,
    speak: Option<SpeakQueueHandle>,
    running: Mutex<Option<Running>>,
}

impl VoiceGateOwner {
    pub fn new(rt_handle: tokio::runtime::Handle, speak: Option<SpeakQueueHandle>) -> Self {
        Self {
            rt_handle,
            speak,
            running: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.running.lock(), Ok(guard) if guard.is_some())
    }

    pub fn state(&self) -> Option<VoiceGateState> {
        let guard = self.running.lock().ok()?;
        guard.as_ref().map(|running| running.monitor.state())
    }

    pub fn level(&self) -> f32 {
        match self.running.lock() {
            Ok(guard) => guard
                .as_ref()
                .map(|running| running.monitor.level())
                .unwrap_or(0.0),
            Err(_) => 0.0,
        }
    }

    pub fn start(&self, config: VoiceGateConfig) {
        let Ok(mut guard) = self.running.lock() else {
            return;
        };
        if let Some(previous) = guard.take() {
            previous.pump.abort();
            previous.monitor.stop();
        }
        let monitor = VoiceGateMonitor::start(config);
        let pump = self
            .rt_handle
            .spawn(pump_states(monitor.subscribe(), self.speak.clone()));
        *guard = Some(Running { monitor, pump });
    }

    pub fn reconfigure(&self, config: VoiceGateConfig) {
        if let Ok(guard) = self.running.lock()
            && let Some(running) = guard.as_ref()
        {
            running.monitor.reconfigure(config);
        }
    }

    pub fn stop(&self) {
        let taken = match self.running.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        let Some(running) = taken else {
            return;
        };
        running.pump.abort();
        running.monitor.stop();
        drop(running);
        if let Some(speak) = self.speak.clone() {
            self.rt_handle.spawn(async move {
                notify_inactive(&speak).await;
            });
        }
    }
}

async fn pump_states(mut rx: watch::Receiver<VoiceGateState>, speak: Option<SpeakQueueHandle>) {
    let Some(speak) = speak else {
        return;
    };
    loop {
        let state = rx.borrow_and_update().clone();
        match state {
            VoiceGateState::Active => notify_active(&speak).await,
            VoiceGateState::Inactive => notify_inactive(&speak).await,
            VoiceGateState::Unavailable(message) => {
                tracing::warn!(
                    error = %message,
                    "voice gate input unavailable; releasing the speak queue"
                );
                notify_inactive(&speak).await;
            }
        }
        if rx.changed().await.is_err() {
            break;
        }
    }
}

async fn notify_active(speak: &SpeakQueueHandle) {
    if let Err(e) = speak.notify_voicegate_active().await {
        tracing::warn!(error = %e, "failed to hold the speak queue for the voice gate");
    }
}

async fn notify_inactive(speak: &SpeakQueueHandle) {
    if let Err(e) = speak.notify_voicegate_inactive().await {
        tracing::warn!(error = %e, "failed to release the speak queue from the voice gate");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_settings_maps_stored_values_onto_the_monitor_config() {
        for (settings, expected) in [
            (
                VoiceGateSettings {
                    enabled: true,
                    input_device_id: Some("alsa_input.usb-Yeti".to_owned()),
                    threshold: 0.42,
                    hold_ms: 250,
                },
                VoiceGateConfig {
                    device: Some(DeviceId::new("alsa_input.usb-Yeti")),
                    threshold: 0.42,
                    hold: Duration::from_millis(250),
                },
            ),
            (
                VoiceGateSettings {
                    enabled: false,
                    input_device_id: None,
                    threshold: 0.0,
                    hold_ms: 0,
                },
                VoiceGateConfig {
                    device: None,
                    threshold: 0.0,
                    hold: Duration::ZERO,
                },
            ),
            (
                VoiceGateSettings {
                    enabled: true,
                    input_device_id: None,
                    threshold: 1.0,
                    hold_ms: u32::MAX,
                },
                VoiceGateConfig {
                    device: None,
                    threshold: 1.0,
                    hold: Duration::from_millis(4_294_967_295),
                },
            ),
        ] {
            assert_eq!(
                config_from_settings(&settings),
                expected,
                "mapping drifted for {settings:?}",
            );
        }
    }
}
